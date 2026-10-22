import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { DatasetConfig } from '../src/config.ts';
import type { PmState } from '../src/pm-facts.ts';
import { readSourceRevision } from '../src/pm-source.ts';
import { asOfDatasets, captureDataset } from '../src/pm-datasets.ts';

// Real files and source-produced content hashes. No native market/PIT quality or
// live producer is implied by these deliberately small file-consumer fixtures.
function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') return `{${Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
    .map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`).join(',')}}`;
  return JSON.stringify(value);
}
const sha256 = (bytes: string | Buffer) => createHash('sha256').update(bytes).digest('hex');
function seal(manifest: JsonObject): JsonObject {
  return { ...manifest, contentDigest: sha256(canonical({ schemaVersion: manifest.schemaVersion, datasetId: manifest.datasetId, sourceRevision: manifest.sourceRevision,
    availableAtNs: manifest.availableAtNs, revisionKind: manifest.revisionKind, data: manifest.data, files: manifest.files })) };
}
const freshState = (): PmState => ({ mandates: {}, datasets: {}, commands: {} });
const request = (start = '100', end = '200') => ({ forecast: { selection: {
  event_start_ns: start, event_end_ns: end, decision_cutoff_ns: end, bar_types: ['SOURCE.SIM-1-MINUTE-LAST-EXTERNAL'],
} }, train_end_ns: '140', validation_end_ns: '180' });
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'qz-pm-source-'));
  const manifestPath = join(root, 'revision.json');
  const configured: DatasetConfig = { id: 'original-bars', name: 'Original source files', assetClass: 'SPOT', currency: 'USD',
    source: 'source:original', start: null, end: null, ready: true, data: { legacy: 'must-not-mutate' }, revisionManifest: manifestPath };
  const put = async (path: string, body: string) => { await mkdir(dirname(join(root, path)), { recursive: true }); await writeFile(join(root, path), body); };
  const publish = async (revision: string, directory: string, kind = 'APPEND', availableAtNs = '250', data: JsonObject = {}) => {
    const path = `${directory}/bars.parquet`; await put(path, `original bytes for ${revision}`);
    const bytes = await readFile(join(root, path));
    const manifest = seal({ schemaVersion: 1, datasetId: configured.id, sourceRevision: revision, revisionKind: kind,
      availableAtNs, data: { catalog: directory, request: request(), ...data },
      files: [{ path, bytes: String(bytes.length), sha256: sha256(bytes) }] });
    await writeFile(manifestPath, JSON.stringify(manifest)); return manifest;
  };
  const save = (manifest: JsonObject) => writeFile(manifestPath, JSON.stringify(manifest));
  return { root, configured, put, publish, save, cleanup: () => rm(root, { recursive: true, force: true }) };
}

test('PM source: legacy static datasets have no fabricated source revision or filesystem read', async () => {
  const state = await fixture();
  try {
    const { revisionManifest: _path, ...legacy } = state.configured;
    assert.equal(await readSourceRevision(legacy), null);
    const registered = captureDataset({ ...legacy, data: { request: request() } }, freshState(), '300');
    assert.ok(registered.revision.missingCapabilities.includes('catalog_content_immutability_not_verified'));
    assert.equal(registered.revision.qualityStatus, 'INSUFFICIENT');
  } finally { await state.cleanup(); }
});

test('PM source: actual file digests and all declared catalogs are bound without rewriting original configuration', async () => {
  const state = await fixture();
  try {
    const original = structuredClone(state.configured);
    const manifest = await state.publish('source:1', 'version-1');
    await state.put('version-1-partitions/index.json', '{"partition":"original"}');
    const partitionBytes = await readFile(join(state.root, 'version-1-partitions/index.json'));
    const data = manifest.data as JsonObject;
    data.catalog_partitions = 'version-1-partitions'; data.sealed = { catalog: 'version-1' };
    (manifest.files as JsonObject[]).push({ path: 'version-1-partitions/index.json', bytes: String(partitionBytes.length), sha256: sha256(partitionBytes) });
    const signed = seal(manifest); await state.save(signed);
    const read = await readSourceRevision(state.configured); assert.ok(read);
    assert.equal(read.contentDigest, signed.contentDigest);
    assert.equal(read.availableAtNs, '250');
    assert.equal(read.configured.data.catalog, join(state.root, 'version-1'));
    assert.equal(read.configured.data.catalog_partitions, join(state.root, 'version-1-partitions'));
    assert.equal((read.configured.data.sealed as JsonObject).catalog, join(state.root, 'version-1'));
    assert.deepEqual(state.configured, original);
    assert.equal((read.manifest.data as JsonObject).catalog, 'version-1', 'The original producer manifest remains its original relative-path fact.');
  } finally { await state.cleanup(); }
});

test('PM source: same-size file mutation and a changed unsigned manifest are refused', async () => {
  const state = await fixture();
  try {
    const manifest = await state.publish('source:1', 'version-1');
    const file = join(state.root, 'version-1/bars.parquet');
    const original = await readFile(file);
    await writeFile(file, Buffer.alloc(original.length, 120));
    await assert.rejects(readSourceRevision(state.configured), /source_manifest_file_digest_mismatch/);
    await writeFile(file, original);
    (manifest.data as JsonObject).request = request('101', '200');
    await state.save(manifest);
    await assert.rejects(readSourceRevision(state.configured), /source_manifest_digest_mismatch/);
  } finally { await state.cleanup(); }
});

test('PM source: future publication and historical admission before source availability are refused', async () => {
  const state = await fixture();
  try {
    const future = (BigInt(Date.now()) * 1_000_000n + 60_000_000_000n).toString();
    await state.publish('future', 'future', 'APPEND', future);
    await assert.rejects(readSourceRevision(state.configured), /source_revision_not_yet_available/);
    await state.publish('late-correction', 'late', 'CORRECTION', '250');
    const source = await readSourceRevision(state.configured); assert.ok(source);
    assert.throws(() => captureDataset(source.configured, freshState(), '249', source), /available|clock|cutoff/,
      'A correction received at 250 cannot be admitted into an as-of-249 decision.');
  } finally { await state.cleanup(); }
});

test('PM source: append and correction retain the immutable earlier data, lineage and as-of access', async () => {
  const files = await fixture();
  try {
    const state = freshState();
    await files.publish('revision:1', 'v1');
    const first = await readSourceRevision(files.configured); assert.ok(first);
    const registered = captureDataset(first.configured, state, '300', first);
    const original = structuredClone(registered);
    await files.publish('revision:2', 'v2', 'APPEND', '350', { request: request('200', '300') });
    const append = await readSourceRevision(files.configured); assert.ok(append);
    const second = captureDataset(append.configured, state, '400', append);
    assert.equal(second.revision.parentRevisionId, registered.revision.id);
    assert.equal(second.revision.revisionKind, 'APPEND');
    await files.publish('revision:3', 'v3', 'CORRECTION', '450', { request: request('100', '200') });
    const correction = await readSourceRevision(files.configured); assert.ok(correction);
    const third = captureDataset(correction.configured, state, '500', correction);
    assert.equal(third.revision.parentRevisionId, second.revision.id);
    assert.equal(third.revision.revisionKind, 'CORRECTION');
    assert.deepEqual(state.datasets[registered.revision.id], original);
    assert.equal(await readFile(join(files.root, 'v1/bars.parquet'), 'utf8'), 'original bytes for revision:1');
    assert.deepEqual(asOfDatasets(state, [registered.revision.id], '300'), [original]);
    assert.throws(() => asOfDatasets(state, [third.revision.id], '499'), /dataset_not_available_asof/);
    assert.equal(Object.keys(state.datasets).length, 3);
    (first.manifest.data as JsonObject).producerScratchField = 'mutated-after-admission';
    assert.deepEqual(state.datasets[registered.revision.id], original,
      'Admission snapshots the original manifest instead of retaining a mutable producer-object alias.');
  } finally { await files.cleanup(); }
});

test('PM source: an identical source identity cannot silently replace its admitted payload', async () => {
  const files = await fixture();
  try {
    const state = freshState();
    await files.publish('source:fixed', 'v1');
    const first = await readSourceRevision(files.configured); assert.ok(first);
    const original = captureDataset(first.configured, state, '300', first);
    await files.publish('source:fixed', 'v2', 'CORRECTION', '350', { request: request('100', '250') });
    const changed = await readSourceRevision(files.configured); assert.ok(changed);
    assert.throws(() => captureDataset(changed.configured, state, '400', changed), /immutable_source_revision_conflict/);
    assert.equal(Object.keys(state.datasets).length, 1);
    assert.equal(state.datasets[original.revision.id]!.data.catalog, join(files.root, 'v1'));
  } finally { await files.cleanup(); }
});

test('PM source: changing APPEND into RETRACTION is a changed source fact, not an idempotent revision', async () => {
  const files = await fixture();
  try {
    const state = freshState();
    const manifest = await files.publish('source:fixed', 'v1');
    const first = await readSourceRevision(files.configured); assert.ok(first);
    captureDataset(first.configured, state, '300', first);
    await files.save({ ...manifest, revisionKind: 'RETRACTION' });
    await assert.rejects(async () => {
      const changed = await readSourceRevision(files.configured); assert.ok(changed);
      captureDataset(changed.configured, state, '400', changed);
    }, /digest|immutable_source_revision_conflict/,
    'A source revision cannot change its scientific invalidation meaning under the old receipt.');
  } finally { await files.cleanup(); }
});

test('PM source: unmanifested catalog and partition files cannot join the scientific input', async () => {
  const state = await fixture();
  try {
    await state.publish('original', 'v1');
    await state.put('v1/late-unregistered.parquet', 'not in the source receipt');
    await assert.rejects(readSourceRevision(state.configured), /source_catalog_file_unmanifested/);
    await rm(join(state.root, 'v1/late-unregistered.parquet'));
    const manifest = JSON.parse(await readFile(state.configured.revisionManifest!, 'utf8')) as JsonObject;
    await state.put('partition-index/extra.json', '{}');
    (manifest.data as JsonObject).catalog_partitions = 'partition-index'; await state.save(seal(manifest));
    await assert.rejects(readSourceRevision(state.configured), /source_catalog_file_unmanifested/);
  } finally { await state.cleanup(); }
});

test('PM source: neither leaf nor ancestor symlinks can substitute a catalog outside its manifest root', async () => {
  const state = await fixture(); const outside = await mkdtemp(join(tmpdir(), 'qz-pm-outside-'));
  try {
    await mkdir(join(outside, 'catalog')); await writeFile(join(outside, 'catalog/bars.parquet'), 'external-data');
    const bytes = await readFile(join(outside, 'catalog/bars.parquet'));
    await symlink(outside, join(state.root, 'alias'), 'dir');
    const manifest = seal({ schemaVersion: 1, datasetId: state.configured.id, sourceRevision: 'outside', revisionKind: 'APPEND', availableAtNs: '250',
      data: { catalog: 'alias/catalog', request: request() }, files: [{ path: 'alias/catalog/bars.parquet', bytes: String(bytes.length), sha256: sha256(bytes) }] });
    await state.save(manifest);
    await assert.rejects(readSourceRevision(state.configured), /symlink|outside_manifest_root|source_manifest_path_invalid/,
      'A non-symlink leaf under a symlink ancestor still refers to data outside the original source root.');
  } finally { await state.cleanup(); await rm(outside, { recursive: true, force: true }); }
});


test('PM source: a new revision cannot recycle any previously admitted catalog path with changed bytes', async () => {
  const files = await fixture();
  try {
    const state = freshState();
    for (const [revision, directory, admittedAt] of [['first', 'v1', '300'], ['second', 'v2', '400']] as const) {
      await files.publish(revision, directory);
      const source = await readSourceRevision(files.configured); assert.ok(source);
      captureDataset(source.configured, state, admittedAt, source);
    }
    const original = structuredClone(Object.values(state.datasets)[0]);
    await files.publish('third', 'v1', 'CORRECTION', '450');
    const recycled = await readSourceRevision(files.configured); assert.ok(recycled);
    assert.throws(() => captureDataset(recycled.configured, state, '500', recycled), /immutable|catalog.*reus|catalog.*changed/,
      'Checking only the immediately previous v2 directory misses corruption of the older admitted v1 input.');
    assert.deepEqual(Object.values(state.datasets)[0], original);
    assert.equal(Object.keys(state.datasets).length, 2);
  } finally { await files.cleanup(); }
});

test('PM source: a nested catalog alias cannot overwrite files covered by an earlier broader catalog', async () => {
  const files = await fixture();
  try {
    const state = freshState();
    await files.publish('original', 'immutable/segment');
    const manifest = JSON.parse(await readFile(files.configured.revisionManifest!, 'utf8')) as JsonObject;
    (manifest.data as JsonObject).catalog = 'immutable'; await files.save(seal(manifest));
    const first = await readSourceRevision(files.configured); assert.ok(first);
    captureDataset(first.configured, state, '300', first);
    await files.publish('correction', 'immutable/segment', 'CORRECTION', '350');
    const changed = await readSourceRevision(files.configured); assert.ok(changed);
    assert.throws(() => captureDataset(changed.configured, state, '400', changed), /immutable|catalog.*reus|catalog.*changed/,
      'Different path strings still share the exact source file beneath an admitted catalog root.');
    assert.equal(Object.keys(state.datasets).length, 1);
  } finally { await files.cleanup(); }
});
