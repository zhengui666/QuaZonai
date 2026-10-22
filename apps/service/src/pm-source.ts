import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, readFile, readdir, realpath } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { DatasetConfig } from './config.ts';
import { counter, json, nowNs, object, Problem, text } from './facts.ts';
import { digest } from './pm-facts.ts';

export interface SourceRevision {
  configured: DatasetConfig;
  sourceRevision: string;
  availableAtNs: string;
  contentDigest: string;
  revisionKind: 'APPEND' | 'CORRECTION' | 'RETRACTION';
  manifest: JsonObject;
}
/** A source-produced manifest is a data input, never a command or an HTTP path. */
export async function readSourceRevision(configured: DatasetConfig): Promise<SourceRevision | null> {
  if (!configured.revisionManifest) return null;
  const path = configured.revisionManifest; const root = await realpath(dirname(path));
  const manifest = object(JSON.parse(await readFile(path, 'utf8')), 'source_manifest');
  if (Object.keys(manifest).some(key => !['schemaVersion', 'datasetId', 'sourceRevision', 'availableAtNs', 'data', 'files', 'contentDigest', 'revisionKind'].includes(key))) throw new Problem('source_manifest_field_unsupported');
  if (manifest.schemaVersion !== 1 || manifest.datasetId !== configured.id) throw new Problem('source_manifest_identity_mismatch');
  const sourceRevision = text(manifest.sourceRevision, 'source_revision');
  const availableAtNs = counter(manifest.availableAtNs, 'source_available_clock');
  if (BigInt(availableAtNs) > BigInt(nowNs())) throw new Problem('source_revision_not_yet_available');
  const data = object(manifest.data, 'source_data');
  if (!Array.isArray(manifest.files) || manifest.files.length === 0) throw new Problem('source_manifest_files_required');
  const contentDigest = text(manifest.contentDigest, 'source_content_digest');
  if (contentDigest !== digest({ schemaVersion: manifest.schemaVersion, datasetId: manifest.datasetId, sourceRevision, availableAtNs, revisionKind: manifest.revisionKind, data, files: manifest.files })) throw new Problem('source_manifest_digest_mismatch');
  if (!['APPEND', 'CORRECTION', 'RETRACTION'].includes(String(manifest.revisionKind))) throw new Problem('source_revision_kind_invalid');
  const files = new Set<string>();
  for (const value of manifest.files) {
    const entry = object(value, 'manifest_file'); const name = text(entry.path, 'manifest_path');
    if (isAbsolute(name) || name.split(/[\\/]/).includes('..') || files.has(name)) throw new Problem('source_manifest_path_invalid');
    const absolute = resolve(root, name); const stat = await lstat(absolute);
    if (await realpath(absolute) !== absolute || relative(root, absolute) !== name) throw new Problem('source_manifest_path_invalid');
    if (!stat.isFile() || stat.isSymbolicLink() || String(stat.size) !== counter(entry.bytes, 'manifest_file_bytes')) throw new Problem('source_manifest_file_mismatch');
    const hash = createHash('sha256'); for await (const chunk of createReadStream(absolute)) hash.update(chunk);
    if (hash.digest('hex') !== entry.sha256) throw new Problem('source_manifest_file_digest_mismatch');
    files.add(name);
  }
  const resolved = json(data);
  const catalogs: string[] = [];
  for (const item of [resolved, resolved.sealed]) if (item && typeof item === 'object' && !Array.isArray(item)) {
    for (const key of ['catalog', 'catalog_partitions']) if (typeof item[key] === 'string') {
      const absolute = resolve(root, item[key]); const name = relative(root, absolute);
      if (name.startsWith(`..${sep}`) || isAbsolute(name)) throw new Problem('source_catalog_outside_manifest_root');
      item[key] = absolute; catalogs.push(absolute);
    }
  }
  async function verifyCoverage(directory: string): Promise<void> {
    const stat = await lstat(directory);
    if (stat.isSymbolicLink() || await realpath(directory) !== directory) throw new Problem('source_catalog_symlink_unsupported');
    if (stat.isFile()) { if (!files.has(relative(root, directory))) throw new Problem('source_catalog_file_unmanifested'); return; }
    if (!stat.isDirectory()) throw new Problem('source_catalog_type_invalid');
    for (const item of await readdir(directory)) await verifyCoverage(resolve(directory, item));
  }
  for (const catalog of catalogs) await verifyCoverage(catalog);
  return { configured: { ...configured, data: resolved }, sourceRevision, availableAtNs, contentDigest,
    revisionKind: manifest.revisionKind as SourceRevision['revisionKind'], manifest };
}
