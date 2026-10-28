import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import { copyFile, link, lstat, mkdir, mkdtemp, open, readFile, readdir, realpath, rename, rm } from 'node:fs/promises';
import { dirname, join, relative, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { JsonObject } from '../../../packages/contracts/src/index.ts';
import type { DatasetConfig } from './config.ts';
import { json, nowNs, object, Problem } from './facts.ts';
import { digest } from './pm-facts.ts';
import { readSourceRevision } from './pm-source.ts';

/** Stable adapter-owned alias; owners select a source, never author manifests. */
export function datasetRevisionManifest(output: string, datasetId: string): string {
  return join(dirname(resolve(output)), '.qz-source-revisions', createHash('sha256').update(datasetId).digest('hex'), 'current.json');
}
type FileEntry = { path: string; bytes: string; sha256: string };
async function writeComplete(path: string, value: unknown) {
  const file = await open(path, 'wx', 0o600);
  try { await file.writeFile(JSON.stringify(value, null, 2) + '\n'); await file.sync(); } finally { await file.close(); }
}
async function fingerprint(path: string): Promise<string> {
  const hash = createHash('sha256'); for await (const chunk of createReadStream(path)) hash.update(chunk); return hash.digest('hex');
}
const identity = (stat: Awaited<ReturnType<typeof lstat>>) => [stat.dev, stat.ino, stat.size, stat.mtimeMs, stat.ctimeMs].join(':');

/** Snapshot complete native files into a NEW owned directory before publishing
 * the stable alias. Existing revisions and caller catalogs are never rewritten.
 * The availability clock is local completed publication, not historical PIT. */
export async function prepareDatasetRevision(dataset: DatasetConfig, producerEvidence: JsonObject, options: { shareDuplicateFiles?: boolean } = {}) {
  if (!dataset.revisionManifest) throw new Problem('dataset_revision_manifest_required');
  const manifestPath = resolve(dataset.revisionManifest); const root = dirname(manifestPath);
  await mkdir(join(root, 'revisions'), { recursive: true });
  if (await realpath(root) !== root) throw new Problem('dataset_revision_symlink_unsupported');
  const staging = await mkdtemp(join(root, '.candidate-'));
  let moved = false;
  async function current() {
    try { await lstat(manifestPath); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; return null; }
    // A missing file inside an already published revision is broken evidence,
    // never an absent head that permits restarting the source history.
    return await readSourceRevision(dataset);
  }
  async function publishCompared<T>(expected: string | null, publish: (head: Awaited<ReturnType<typeof current>>) => Promise<T>, replay?: (head: NonNullable<Awaited<ReturnType<typeof current>>>) => Promise<T | undefined>): Promise<T> {
    // SQLite's process lock is only a short filesystem publication mutex. No
    // task, manifest, business state or scheduler is stored in this database;
    // an interrupted process releases the lock without a stale PID/lease file.
    const mutex = new DatabaseSync(join(root, 'publication-lock.sqlite'));
    try {
      mutex.exec('PRAGMA busy_timeout=0; BEGIN IMMEDIATE');
      const head = await current();
      if ((head?.contentDigest ?? null) !== expected) {
        const original = head && replay ? await replay(head) : undefined;
        if (original !== undefined) return original;
        throw new Problem('dataset_revision_publication_conflict', 409);
      }
      return await publish(head);
    } finally { if (mutex.isTransaction) mutex.exec('ROLLBACK'); mutex.close(); }
  }
  try {
    const files: FileEntry[] = []; const originalStates = new Map<string, string>(); const data = json(dataset.data);
    const cloned = new Map<string, { path: string; sha256: string }>();
    async function snapshot(source: string, destination: string): Promise<void> {
      const stat = await lstat(source);
      if (stat.isSymbolicLink() || await realpath(source) !== source) throw new Problem('dataset_revision_symlink_unsupported');
      originalStates.set(source, identity(stat));
      if (stat.isDirectory()) {
        await mkdir(destination, { recursive: true });
        for (const name of (await readdir(source)).sort()) await snapshot(join(source, name), join(destination, name));
      } else if (stat.isFile()) {
        await mkdir(dirname(destination), { recursive: true });
        const inode = options.shareDuplicateFiles ? await lstat(source, { bigint: true }) : null;
        const key = inode ? `${inode.dev}:${inode.ino}` : null; const previous = key ? cloned.get(key) : undefined;
        // Cache range views share derived files; clone once, then link only inside this owned snapshot.
        if (previous) await link(previous.path, destination);
        else await copyFile(source, destination, constants.COPYFILE_EXCL | constants.COPYFILE_FICLONE);
        const copied = await lstat(destination);
        if (copied.size !== stat.size) throw new Problem('dataset_revision_copy_incomplete');
        const sha256 = previous?.sha256 ?? await fingerprint(destination);
        if (key && !previous) cloned.set(key, { path: destination, sha256 });
        files.push({ path: relative(staging, destination), bytes: String(copied.size), sha256 });
      } else throw new Problem('dataset_revision_regular_files_required');
    }
    let catalogs = 0;
    for (const [label, candidate] of [['primary', data], ['sealed', data.sealed]] as const) {
      if (!candidate) continue;
      const scope = object(candidate);
      for (const key of ['catalog', 'catalog_partitions']) if (typeof scope[key] === 'string') {
        const target = `${label}-${key}`; await snapshot(resolve(scope[key]), join(staging, target)); scope[key] = target; catalogs++;
      }
    }
    if (!catalogs || !files.length) throw new Problem('dataset_revision_catalog_required');
    await writeComplete(join(staging, 'producer.json'), producerEvidence);
    const producer = await lstat(join(staging, 'producer.json'));
    files.push({ path: 'producer.json', bytes: String(producer.size), sha256: await fingerprint(join(staging, 'producer.json')) });
    for (const [path, state] of originalStates) if (identity(await lstat(path)) !== state) throw new Problem('dataset_revision_source_changed');
    const prior = await current(); const expectedHead = prior?.contentDigest ?? null;
    const content = digest({ datasetId: dataset.id, data, files });
    if (prior) {
      const oldData = json(prior.manifest.data);
      for (const candidate of [oldData, oldData.sealed]) if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
        for (const key of ['catalog', 'catalog_partitions']) if (typeof candidate[key] === 'string') candidate[key] = candidate[key].replace(/^revisions\/[^/]+\//, '');
      }
      const oldFiles = (prior.manifest.files as JsonObject[]).map(entry => ({ ...entry, path: String(entry.path).replace(/^revisions\/[^/]+\//, '') }));
      if (content === digest({ datasetId: dataset.id, data: oldData, files: oldFiles })) return { manifestPath, sourceRevision: prior.sourceRevision, data: json(prior.configured.data),
        publish: () => publishCompared(expectedHead, async head => head!) };
    }
    // A genuine correction back to former bytes still gets a fresh directory;
    // it cannot relabel a historical immutable directory as a new revision.
    const sourceRevision = `sha256:${digest({ content, parentRevision: prior?.sourceRevision ?? null })}`;
    const id = sourceRevision.slice('sha256:'.length); const directory = join(root, 'revisions', id);
    for (const candidate of [data, data.sealed]) if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
      for (const key of ['catalog', 'catalog_partitions']) if (typeof candidate[key] === 'string') candidate[key] = `revisions/${id}/${candidate[key]}`;
    }
    try { await rename(staging, directory); moved = true; }
    catch (error) {
      if (!['EEXIST', 'ENOTEMPTY'].includes(String((error as NodeJS.ErrnoException).code))) throw error;
      // A retry may reuse only the exact complete immutable bytes.
      for (const entry of files) if (await fingerprint(join(directory, entry.path)) !== entry.sha256) throw new Problem('dataset_revision_existing_content_mismatch');
    }
    const entries = files.map(entry => ({ ...entry, path: `revisions/${id}/${entry.path}` }));
    const candidate = { schemaVersion: 1, datasetId: dataset.id, sourceRevision, revisionKind: prior ? 'CORRECTION' : 'APPEND', data, files: entries };
    const configuredData = json(data);
    for (const scope of [configuredData, configuredData.sealed]) if (scope && typeof scope === 'object' && !Array.isArray(scope)) {
      for (const key of ['catalog', 'catalog_partitions']) if (typeof scope[key] === 'string') scope[key] = resolve(root, scope[key]);
    }
    const history = join(directory, 'manifest.json');
    return { manifestPath, sourceRevision, data: configuredData, publish: () => publishCompared(expectedHead, async () => {
      // A failed unpublished attempt may be finalized again at its actual retry
      // clock. The process lock and original-head comparison ensure that a
      // previously published revision's receipt can never be rewritten here.
      const body = { ...candidate, availableAtNs: nowNs() };
      const manifest = { ...body, contentDigest: digest(body) };
      const pending = join(root, `.publish-${id}-${process.pid}-${Date.now()}.json`);
      const historyPending = join(directory, `.manifest-${process.pid}-${Date.now()}.json`);
      await writeComplete(pending, manifest);
      try {
        await readSourceRevision({ ...dataset, revisionManifest: pending });
        await writeComplete(historyPending, manifest);
        await rename(historyPending, history);
        // Reader sees old complete manifest or new complete manifest, never a partial file.
        await rename(pending, manifestPath);
      } finally { await rm(pending, { force: true }); await rm(historyPending, { force: true }); }
      return (await readSourceRevision(dataset))!;
    }, async head => {
      if (head.sourceRevision !== sourceRevision) return undefined;
      const { availableAtNs: _clock, contentDigest: _digest, ...published } = head.manifest;
      if (digest(published) !== digest(candidate) || digest(JSON.parse(await readFile(history, 'utf8'))) !== digest(head.manifest)) throw new Problem('dataset_revision_publication_receipt_mismatch', 409);
      return head; // Exact original completed receipt; never publish a new clock on replay.
    }) };
  } finally { if (!moved) await rm(staging, { recursive: true, force: true }); }
}

export async function publishDatasetRevision(dataset: DatasetConfig, producerEvidence: JsonObject) {
  return (await prepareDatasetRevision(dataset, producerEvidence)).publish();
}
