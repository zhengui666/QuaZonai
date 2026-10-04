// Ownership/control-flow checks only. No mock result counts as OCI acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { link, mkdir, mkdtemp, readFile, rename, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { NativeDataExecution, ownedContainer } from './native-data-execution.mjs';

const instance = '00000000-0000-0000-0000-000000000001';
const run = '00000000-0000-0000-0000-000000000002';
const image = `sha256:${'a'.repeat(64)}`;
const id = 'b'.repeat(64);
function fixture() {
  const journal = { instance_id: instance, jobs: [{ run_id: run, external_id: `${run}/1`, attempt_no: 1,
    container_id: id, spec_json: JSON.stringify({ run_id: run, external_job_id: `${run}/1`, attempt_no: 1, image_ref: image }) }] };
  const inspect = { Id: id, Image: image, Config: { Labels: { 'io.quazonai.runtime': instance,
    'io.quazonai.run': run, 'io.quazonai.external-id': `${run}/1`, 'io.quazonai.attempt': '1', 'io.quazonai.role': 'JOB' } } };
  return { journal, inspect };
}
test('exact persisted identity and pinned image establish test ownership', () => {
  const { journal, inspect } = fixture();
  assert.equal(ownedContainer(inspect, journal, image), journal.jobs[0]);
  journal.jobs[0].container_id = null; // A Docker create can commit before its ACK.
  assert.equal(ownedContainer(inspect, journal, image), journal.jobs[0]);
});
for (const mismatch of ['runtime', 'run', 'external-id', 'attempt', 'role', 'image', 'id', 'spec', 'unknown']) {
  test(`uncertain ${mismatch} ownership fails closed`, () => {
    const { journal, inspect } = fixture();
    if (mismatch === 'image') inspect.Image = `sha256:${'c'.repeat(64)}`;
    else if (mismatch === 'id') inspect.Id = 'c'.repeat(64);
    else if (mismatch === 'spec') journal.jobs[0].spec_json = '{}';
    else if (mismatch === 'unknown') journal.jobs = [];
    else inspect.Config.Labels[`io.quazonai.${mismatch}`] = 'unexpected';
    assert.throws(() => ownedContainer(inspect, journal, image));
  });
}
test('cleanup preflights every owned label before removing any container', async () => {
  const { journal, inspect } = fixture(); const calls = [];
  const subject = new NativeDataExecution({});
  subject.started = true; subject.image = image; subject.journal = async () => journal;
  subject.docker = async (_name, args, cleanup) => {
    assert.equal(cleanup, true); calls.push(args[0]);
    if (args[0] === 'ps') return `${id}\n${'c'.repeat(64)}\n`;
    if (args[0] === 'inspect') return JSON.stringify([{ ...inspect, Id: args[1] }]);
    throw new Error('Unverified resources must not be removed');
  };
  await assert.rejects(subject.cleanupContainers());
  assert.deepEqual(calls, ['ps', 'inspect', 'inspect']); assert.equal(subject.evidence.cleanup_complete, false);
});
test('unreadable journal retains state rather than guessing ownership', async () => {
  const subject = new NativeDataExecution({}); subject.started = true;
  subject.journal = async () => { throw new Error('read-only journal unavailable'); };
  subject.docker = async () => { assert.fail('No guessed cleanup'); };
  await assert.rejects(subject.cleanupContainers(), /journal unavailable/);
  assert.equal(subject.evidence.cleanup_complete, false);
});
test('an explicit execution request cannot pass when its image is absent', async () => {
  const before = process.env.QUAZONAI_NATIVE_JOB_IMAGE;
  delete process.env.QUAZONAI_NATIVE_JOB_IMAGE;
  try { await assert.rejects(new NativeDataExecution({}).prepare(), /immutable Job image/); }
  finally { if (before !== undefined) process.env.QUAZONAI_NATIVE_JOB_IMAGE = before; }
});
test('completed owned cleanup verifies empty labels before permitting state deletion', async () => {
  const { journal, inspect } = fixture(); const calls = []; let listed = false;
  const subject = new NativeDataExecution({}); subject.started = true; subject.image = image;
  subject.journal = async () => journal;
  subject.docker = async (_name, args, cleanup) => {
    assert.equal(cleanup, true); calls.push(args);
    if (args[0] === 'ps') { if (listed) return ''; listed = true; return `${id}\n`; }
    if (args[0] === 'inspect') return JSON.stringify([inspect]);
    assert.deepEqual(args, ['rm', '--force', id]); return id;
  };
  await subject.cleanupContainers();
  assert.equal(subject.evidence.cleanup_complete, true); assert.equal(calls.length, 4);
});

test('interrupted explicit execution cannot launch a new Runtime or pass', async () => {
  const subject = new NativeDataExecution({ stopping: () => true, launch: () => assert.fail('No process after interruption') });
  await assert.rejects(subject.start(), /interrupted/); assert.equal(subject.started, false);
});

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const corruptBytes = Buffer.from('explicit test-owned corrupt native parquet');
async function corruptionFixture(t) {
  const root = await mkdtemp(resolve(tmpdir(), 'quazonai-native-partition-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const catalog = resolve(root, 'native-source/prepared/catalog');
  // Deliberately different from the native layout. Only the native descriptor
  // should select the target, even with other Parquet files and BAR-like names.
  const relativePath = 'arbitrary-layout/verified-series/owned.parquet';
  const bar = resolve(catalog, relativePath);
  const definition = resolve(catalog, 'data/instruments/original.parquet');
  const sentinel = resolve(catalog, 'data/bar/untouched.parquet');
  const outside = resolve(`${catalog}-outside`, 'sentinel.parquet');
  const originals = new Map([[bar, Buffer.from('verified native BAR bytes')],
    [definition, Buffer.from('original native instrument bytes')],
    [sentinel, Buffer.from('unselected sentinel bytes')],
    [outside, Buffer.from('outside catalog bytes')]]);
  for (const [path, bytes] of originals) {
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, bytes);
  }
  const partition = { relative_path: relativePath, data_kind: 'BAR', bar_type: 'verified-native-type',
    size_bytes: originals.get(bar).length, sha256: hash(originals.get(bar)) };
  const subject = new NativeDataExecution({ root });
  subject.prepared = { catalog_root: catalog, native_partitions: [partition],
    selection: { selection: { bar_types: [partition.bar_type] } } };
  return { subject, partition, root, catalog, bar, definition, sentinel, outside, originals };
}
async function assertOriginals(fixture, except) {
  for (const [path, bytes] of fixture.originals) {
    if (path !== except) assert.deepEqual(await readFile(path), bytes, `${path} must remain unchanged`);
  }
}

test('native descriptor selects exactly its BAR regardless of directory spelling and records actual changed bytes', async t => {
  const fixture = await corruptionFixture(t);
  await fixture.subject.corrupt();
  assert.deepEqual(await readFile(fixture.bar), corruptBytes);
  await assertOriginals(fixture, fixture.bar);
  assert.deepEqual(fixture.subject.evidence.corruption, { ...fixture.partition,
    before_sha256: fixture.partition.sha256, after_sha256: hash(corruptBytes),
    before_size_bytes: fixture.partition.size_bytes, after_size_bytes: corruptBytes.length });
  await assert.rejects(fixture.subject.corrupt(), /byte count changed/);
  await assertOriginals(fixture, fixture.bar);
});

for (const [name, change, error] of [
  ['missing manifest', f => { delete f.subject.prepared.native_partitions; }, /partition manifest/],
  ['zero partitions', f => { f.subject.prepared.native_partitions = []; }, /Exactly one/],
  ['two partitions', f => { f.subject.prepared.native_partitions.push({ ...f.partition }); }, /Exactly one/],
  ['wrong data kind', f => { f.partition.data_kind = 'INSTRUMENT'; }, /Only a discovered native BAR/],
  ['wrong verified series', f => { f.partition.bar_type = 'other-series'; }, /deep-equal/],
  ['changed hash', f => { f.partition.sha256 = '0'.repeat(64); }, /SHA256 changed/],
  ['malformed hash', f => { f.partition.sha256 = 'not-a-hash'; }, /SHA256 required/],
  ['changed size', f => { f.partition.size_bytes += 1; }, /byte count changed/],
  ['invalid size', f => { f.partition.size_bytes = '26'; }, /byte count required/],
  ['empty size', f => { f.partition.size_bytes = 0; }, /byte count required/],
  ['absolute path', f => { f.partition.relative_path = f.outside; }, /canonical and relative/],
  ['parent traversal', f => { f.partition.relative_path = '../catalog-outside/sentinel.parquet'; }, /canonical and relative/],
  ['internal traversal', f => { f.partition.relative_path = 'arbitrary-layout/../data/instruments/original.parquet'; }, /canonical and relative/],
  ['dot component', f => { f.partition.relative_path = `./${f.partition.relative_path}`; }, /canonical and relative/],
  ['empty component', f => { f.partition.relative_path = f.partition.relative_path.replace('/', '//'); }, /canonical and relative/],
  ['backslash path', f => { f.partition.relative_path = '..\\sentinel.parquet'; }, /canonical and relative/],
  ['empty path', f => { f.partition.relative_path = ''; }, /canonical and relative/],
  ['outside catalog root', f => { f.subject.prepared.catalog_root = `${f.catalog}-outside`; }, /invocation-owned catalog/],
  ['directory target', f => { f.partition.relative_path = 'arbitrary-layout'; }, /regular file/],
]) {
  test(`corruption rejects ${name} without changing definitions or any sentinel`, async t => {
    const fixture = await corruptionFixture(t);
    change(fixture);
    await assert.rejects(fixture.subject.corrupt(), error);
    await assertOriginals(fixture);
    assert.equal(fixture.subject.evidence.corruption, undefined);
  });
}

for (const target of ['bar', 'definition', 'outside']) {
  test(`corruption rejects a file symlink to ${target} before mutation`, async t => {
    const fixture = await corruptionFixture(t);
    await symlink(fixture[target], resolve(fixture.catalog, 'alias.parquet'));
    fixture.partition.relative_path = 'alias.parquet';
    await assert.rejects(fixture.subject.corrupt(), /must not traverse symlinks/);
    await assertOriginals(fixture);
  });
}
test('corruption rejects a parent directory symlink before mutation', async t => {
  const fixture = await corruptionFixture(t);
  await symlink(dirname(fixture.bar), resolve(fixture.catalog, 'alias'), 'dir');
  fixture.partition.relative_path = 'alias/owned.parquet';
  await assert.rejects(fixture.subject.corrupt(), /must not traverse symlinks/);
  await assertOriginals(fixture);
});
test('corruption rejects a symlinked catalog root before mutation', async t => {
  const fixture = await corruptionFixture(t);
  await rename(fixture.catalog, `${fixture.catalog}-moved`);
  await symlink(`${fixture.catalog}-moved`, fixture.catalog, 'dir');
  await assert.rejects(fixture.subject.corrupt(), /directory, not a symlink/);
  await assertOriginals(fixture);
});
test('corruption rejects a symlink above the catalog root before mutation', async t => {
  const fixture = await corruptionFixture(t);
  const source = resolve(fixture.root, 'native-source');
  await rename(source, `${source}-moved`);
  await symlink(`${source}-moved`, source, 'dir');
  await assert.rejects(fixture.subject.corrupt(), /root must be canonical without symlinks/);
  await assertOriginals(fixture);
});
test('corruption rejects shared hard links rather than changing another path', async t => {
  const fixture = await corruptionFixture(t);
  const alias = resolve(fixture.root, 'shared.parquet');
  await link(fixture.bar, alias);
  await assert.rejects(fixture.subject.corrupt(), /without shared hard links/);
  await assertOriginals(fixture);
  assert.deepEqual(await readFile(alias), fixture.originals.get(fixture.bar));
});
