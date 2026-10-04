// Exclusive generated ownership; all analysis/compaction finishes before writes.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

export const marker = '// Generated from Rust OpenAPI. Do not edit.\n';
export const manifestPath = 'response-contract/manifest.json';
const ownedPath = value => value === 'responses.cjs' || value === 'responses.d.cts'
  || /^response-contract\/(?:modules\/(?:schema|shared)-[a-z0-9]+\.cjs|[a-z][a-z0-9-]*\.(?:cjs|mjs|d\.cts|d\.mts|json))$/.test(value);
const error = message => { throw new Error(`Generated response ownership: ${message}`); };

export function completeOutputs(payload, provenance = new Map(), imports = new Map()) {
  if (payload.has(manifestPath)) error('manifest must be independently constructed');
  const files = [...payload].sort(([a], [b]) => a.localeCompare(b)).map(([file, content]) => {
    if (!ownedPath(file)) error(`out-of-scope path ${file}`);
    if (!content.startsWith(marker) && !file.endsWith('.json')) error(`missing marker ${file}`);
    if (file.endsWith('.json') && JSON.parse(content).generated !== marker.trim()) error(`missing JSON marker ${file}`);
    return { path: file, bytes: Buffer.byteLength(content), importIds: imports.get(file) ?? [], provenance: provenance.get(file) ?? {} };
  });
  const manifest = { generated: marker.trim(), version: 1, ownedRoot: 'response-contract', manifestPath, files };
  return new Map([...payload, [manifestPath, JSON.stringify(manifest, null, 2) + '\n']]);
}
function regularOrAbsent(filename) {
  let stat; try { stat = fs.lstatSync(filename); } catch (cause) { if (cause.code === 'ENOENT') return false; throw cause; }
  if (stat.isSymbolicLink() || !stat.isFile()) error(`not a regular file ${filename}`);
  return true;
}
function marked(filename) {
  const content = fs.readFileSync(filename, 'utf8');
  if (filename.endsWith('.json')) {
    try { return JSON.parse(content).generated === marker.trim(); } catch { return false; }
  }
  return content.startsWith(marker);
}
function inventory(root) {
  const found = [];
  function walk(directory, prefix) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const relative = prefix + entry.name;
      if (entry.isSymbolicLink()) error(`symlink ${relative}`);
      if (entry.isDirectory()) {
        if (relative !== 'response-contract/modules') error(`unknown directory ${relative}`);
        walk(path.join(directory, entry.name), relative + '/');
      } else if (entry.isFile()) found.push(relative);
      else error(`special file ${relative}`);
    }
  }
  const subtree = path.join(root, 'response-contract');
  let stat;
  try { stat = fs.lstatSync(subtree); } catch (cause) { if (cause.code !== 'ENOENT') throw cause; }
  if (stat) {
    if (stat.isSymbolicLink() || !stat.isDirectory()) error('owned subtree is not a directory');
    walk(subtree, 'response-contract/');
  }
  for (const file of ['responses.cjs', 'responses.d.cts']) if (regularOrAbsent(path.join(root, file))) found.push(file);
  return found.sort();
}
function previousManifest(root) {
  const filename = path.join(root, manifestPath);
  if (!regularOrAbsent(filename)) return new Set();
  let manifest; try { manifest = JSON.parse(fs.readFileSync(filename, 'utf8')); } catch { error('invalid previous manifest JSON'); }
  if (manifest.generated !== marker.trim() || manifest.version !== 1 || manifest.ownedRoot !== 'response-contract'
    || manifest.manifestPath !== manifestPath || !Array.isArray(manifest.files)) error('invalid previous manifest ownership');
  const previous = new Set([manifestPath]);
  for (const entry of manifest.files) {
    if (!entry || !ownedPath(entry.path) || entry.path === manifestPath) error('invalid previous manifest entry');
    previous.add(entry.path);
  }
  return previous;
}
export function checkOutputs(root, expected, { requireTracked = false } = {}) {
  if (!fs.existsSync(root)) error('output directory is missing');
  if (fs.lstatSync(root).isSymbolicLink()) error('output directory is a symlink');
  const actual = inventory(root); const expectedPaths = [...expected.keys()].sort();
  const missing = expectedPaths.filter(file => !actual.includes(file));
  const extra = actual.filter(file => !expected.has(file));
  if (missing.length || extra.length) error(`file set differs; missing: ${missing.join(', ')}; extra: ${extra.join(', ')}`);
  for (const [file, content] of expected) if (fs.readFileSync(path.join(root, file), 'utf8') !== content) error(`stale bytes ${file}`);
  if (requireTracked) {
    const result = spawnSync('git', ['ls-files', '-z', '--', '.'], { cwd: root, encoding: 'utf8' });
    if (result.status !== 0 || result.error) error('cannot verify tracked generated paths');
    const tracked = new Set(result.stdout.split('\0'));
    const untracked = expectedPaths.filter(file => !tracked.has(file));
    if (untracked.length) error(`untracked owned paths: ${untracked.join(', ')}`);
  }
}
export function publishOutputs(root, expected) {
  if (fs.existsSync(root) && fs.lstatSync(root).isSymbolicLink()) error('output directory is a symlink');
  const exists = fs.existsSync(root);
  const actual = exists ? inventory(root) : [];
  const previous = exists ? previousManifest(root) : new Set();
  const stale = actual.filter(file => !expected.has(file));
  // Audit ALL destinations and stale paths before the first mkdir/write/unlink.
  // Unknown data is never deleted, including a file absent from an old manifest.
  for (const file of actual) {
    if (!expected.has(file) && !previous.has(file)) error(`unknown file ${file}`);
    if (!marked(path.join(root, file))) error(`unmarked file ${file}`);
    if (stale.includes(file) && (!file.startsWith('response-contract/') || !previous.has(file))) error(`unsafe stale path ${file}`);
  }
  for (const [file] of expected) if (!ownedPath(file)) error(`invalid expected path ${file}`);
  fs.mkdirSync(path.join(root, 'response-contract/modules'), { recursive: true });
  // Manifest last: interruption is detectable by a complete independent check.
  for (const [file, content] of expected) {
    if (file === manifestPath) continue;
    const target = path.join(root, file);
    if (fs.existsSync(target) && fs.readFileSync(target, 'utf8') === content) continue;
    fs.writeFileSync(target, content);
  }
  for (const file of stale) fs.unlinkSync(path.join(root, file));
  const manifest = path.join(root, manifestPath);
  if (!fs.existsSync(manifest) || fs.readFileSync(manifest, 'utf8') !== expected.get(manifestPath)) fs.writeFileSync(manifest, expected.get(manifestPath));
}
