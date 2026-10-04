import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { completeOutputs, publishOutputs, checkOutputs, marker } from './validator-output.mjs';
const moduleA = 'response-contract/modules/schema-0.cjs';
const moduleB = 'response-contract/modules/schema-1.cjs';
const files = (...modules) => completeOutputs(new Map([['responses.cjs', marker + 'exports.x = 1;\n'], ['responses.d.cts', marker + 'export declare const x: number;\n'], ...modules.map(file => [file, marker + 'exports.test = true;\n'])]));
function fixture(run) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'qz-owned-'));
  try { return run(root); } finally { fs.rmSync(root, { recursive: true, force: true }); }
}
function bytes(root) {
  const result = new Map();
  const walk = (dir, prefix = '') => { for (const file of fs.readdirSync(dir, { withFileTypes: true })) { const relative = prefix + file.name; if (file.isDirectory()) walk(path.join(dir, file.name), relative + '/'); else result.set(relative, file.isSymbolicLink() ? 'symlink:' + fs.readlinkSync(path.join(dir, file.name)) : fs.readFileSync(path.join(dir, file.name), 'utf8')); } };
  walk(root); return result;
}
test('complete independent check catches missing, stale and extra files without modifying them', () => fixture(root => {
  const expected = files(moduleA); publishOutputs(root, expected); checkOutputs(root, expected);
  const first = bytes(root); publishOutputs(root, expected); assert.deepEqual(bytes(root), first);
  for (const fault of ['missing', 'stale', 'extra']) {
    publishOutputs(root, expected);
    const filename = path.join(root, fault === 'extra' ? 'response-contract/untracked.cjs' : moduleA);
    if (fault === 'missing') fs.unlinkSync(filename); else fs.writeFileSync(filename, marker + fault);
    const before = bytes(root); assert.throws(() => checkOutputs(root, expected), /ownership/); assert.deepEqual(bytes(root), before);
    if (fault === 'extra') fs.unlinkSync(filename);
  }
}));
test('new expected files are checked independently of the old manifest', () => fixture(root => {
  publishOutputs(root, files(moduleA)); assert.throws(() => checkOutputs(root, files(moduleA, moduleB)), /missing/);
}));
test('safe stale cleanup only removes previously owned marked paths; api.d.ts survives', () => fixture(root => {
  publishOutputs(root, files(moduleA)); fs.writeFileSync(path.join(root, 'api.d.ts'), 'separately owned API');
  publishOutputs(root, files(moduleB)); assert.equal(fs.existsSync(path.join(root, moduleA)), false);
  assert.equal(fs.readFileSync(path.join(root, 'api.d.ts'), 'utf8'), 'separately owned API'); checkOutputs(root, files(moduleB));
}));
test('unknown, unmarked and corrupted-manifest paths fail before every write', () => fixture(root => {
  const original = files(moduleA); const changed = files(moduleB); publishOutputs(root, original);
  const unknown = path.join(root, 'response-contract/unknown.cjs'); fs.writeFileSync(unknown, marker + 'unknown');
  const first = bytes(root); assert.throws(() => publishOutputs(root, changed), /unknown file/); assert.deepEqual(bytes(root), first); fs.unlinkSync(unknown);
  fs.writeFileSync(path.join(root, moduleA), 'user content'); const second = bytes(root);
  assert.throws(() => publishOutputs(root, changed), /unmarked/); assert.deepEqual(bytes(root), second);
  fs.writeFileSync(path.join(root, moduleA), original.get(moduleA));
  fs.writeFileSync(path.join(root, 'response-contract/manifest.json'), '{}'); const third = bytes(root);
  assert.throws(() => publishOutputs(root, changed), /manifest/); assert.deepEqual(bytes(root), third);
}));
test('symlinked owned file, directory, dangling directory and root are rejected', () => fixture(root => {
  const actual = path.join(root, 'actual'); fs.mkdirSync(actual); const expected = files(moduleA); publishOutputs(actual, expected);
  for (const relative of [moduleA, 'response-contract/modules', 'response-contract']) {
    const filename = path.join(actual, relative); const backup = path.join(root, 'backup'); fs.renameSync(filename, backup); fs.symlinkSync(backup, filename);
    assert.throws(() => checkOutputs(actual, expected), /symlink|not a directory/); assert.throws(() => publishOutputs(actual, expected), /symlink|not a directory/);
    fs.unlinkSync(filename); fs.renameSync(backup, filename);
  }
  const link = path.join(root, 'linked'); fs.symlinkSync(actual, link);
  assert.throws(() => checkOutputs(link, expected), /symlink/); assert.throws(() => publishOutputs(link, expected), /symlink/);
  const empty = path.join(root, 'empty'); fs.mkdirSync(empty); fs.symlinkSync(path.join(root, 'nonexistent'), path.join(empty, 'response-contract'));
  assert.throws(() => publishOutputs(empty, expected), /not a directory/); assert.equal(fs.existsSync(path.join(root, 'nonexistent')), false);
}));
test('traversal and separately owned API cannot enter the manifest', () => {
  for (const file of ['../escape.cjs', 'api.d.ts', 'response-contract/../api.d.ts', '/absolute.cjs']) assert.throws(() => completeOutputs(new Map([[file, marker]])), /out-of-scope/);
});

test('repository completeness rejects an untracked expected module', () => fixture(root => {
  const expected = files(moduleA); publishOutputs(root, expected);
  const git = (...args) => { const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' }); assert.equal(result.status, 0, result.stderr); };
  git('init', '-q'); git('add', '.'); checkOutputs(root, expected, { requireTracked: true });
  git('rm', '--cached', '--quiet', '--', moduleA);
  assert.throws(() => checkOutputs(root, expected, { requireTracked: true }), /untracked owned paths/);
}));
test('invalid expected set rejects before publishing any bytes', () => fixture(root => {
  publishOutputs(root, files(moduleA)); const before = bytes(root);
  const invalid = new Map(files(moduleB)); invalid.set('../escape.cjs', marker);
  assert.throws(() => publishOutputs(root, invalid), /invalid expected path/);
  assert.deepEqual(bytes(root), before);
}));

test('stale generated output is removed by marker and ownership, without content comparison', () => fixture(root => {
  publishOutputs(root, files(moduleA));
  fs.appendFileSync(path.join(root, moduleA), '// obsolete generated content\n');
  publishOutputs(root, files(moduleB));
  assert.equal(fs.existsSync(path.join(root, moduleA)), false);
  checkOutputs(root, files(moduleB));
}));

test('legacy generated filenames and manifest metadata migrate without digest checks', () => fixture(root => {
  const legacy = 'response-contract/modules/schema-aaaaaaaaaaaaaaaaaaaa.cjs';
  publishOutputs(root, files(legacy));
  const filename = path.join(root, 'response-contract/manifest.json');
  const manifest = JSON.parse(fs.readFileSync(filename, 'utf8'));
  for (const entry of manifest.files) entry.sha256 = 'unused legacy metadata';
  fs.writeFileSync(filename, JSON.stringify(manifest));
  publishOutputs(root, files(moduleA));
  assert.equal(fs.existsSync(path.join(root, legacy)), false);
  const current = JSON.parse(fs.readFileSync(filename, 'utf8'));
  for (const entry of current.files) assert.equal(Object.hasOwn(entry, 'sha256'), false);
  checkOutputs(root, files(moduleA));
}));
