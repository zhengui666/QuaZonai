import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { canonical } from '../../research/src/documents.ts';
import { trustedSourceChecks } from '../../research/src/review-source.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { reviewSourceChecks } from '../../research/src/review-workflow.ts';
import { assertCurrentReviewSource, prepareReviewSourceEvidence } from '../src/review-source.ts';
import type { DatasetConfig } from '../src/config.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
const digest = (value: string) => createHash('sha256').update(value).digest('hex');
// Source-authority/file boundary fixtures, not market PIT evidence or an actual native-validation result.
async function fixture(maximumRows: number | null = 1000) {
  const root = await mkdtemp(join(tmpdir(), 'qz-review-source-test-')); await mkdir(join(root, 'catalog'));
  const selection = { schema_version: 1, ...(maximumRows === null ? {} : { maximum_rows: maximumRows }), event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['ORIGINAL.SIM-1-MINUTE-LAST-EXTERNAL'] };
  const request = { forecast: { selection }, train_end_ns: '140', validation_end_ns: '180' };
  const metadata = { registered_ref: 'original', native_snapshot_ref: 'snapshot:1', partition: 'VALIDATION', origin: 'REAL', pit_status: 'VERIFIED', revision_policy: 'AS_KNOWN_THEN',
    provenance_reference: 'configured-source-owner:fixture', availability_provenance: 'fixture source-owner statement', quality: { datasets: [{ dataset_revision_id: 'native-dataset:1', instrument_ids: ['ORIGINAL.SIM'], selection: { ...selection, event_start_ns: '140', event_end_ns: '180' } }] } };
  const license = { schemaVersion: 1, datasetId: 'original', sourceId: 'source:original', sourceRevision: 'snapshot:1', datasetRevisionId: 'native-dataset:1',
    grantId: 'grant:fixture', issuer: 'fixture-source-owner', reference: 'fixture-only:license', status: 'ACTIVE', purposes: ['RESEARCH'], validFromNs: '0', validUntilNs: '9223372036854775807' };
  const files: JsonObject[] = [];
  for (const [path, body] of Object.entries({ 'catalog/bars.parquet': 'not market data', 'metadata.json': JSON.stringify(metadata), 'sealed-metadata.json': JSON.stringify({ ...metadata, partition: 'SEALED', quality: { datasets: [{ dataset_revision_id: 'native-dataset:1', instrument_ids: ['ORIGINAL.SIM'], selection: { ...selection, event_start_ns: '180' } }] } }), 'license.json': JSON.stringify(license) })) {
    await writeFile(join(root, path), body); files.push({ path, bytes: String(Buffer.byteLength(body)), sha256: digest(body) });
  }
  const manifest = { schemaVersion: 1, datasetId: 'original', sourceRevision: 'snapshot:1', availableAtNs: '250', revisionKind: 'APPEND', data: { catalog: 'catalog', request }, files };
  await writeFile(join(root, 'revision.json'), JSON.stringify({ ...manifest, contentDigest: digest(canonical(manifest)) }));
  const configured: DatasetConfig = { id: 'original', name: 'Original registered source', assetClass: 'SPOT', currency: 'USD', source: 'source:original', start: null, end: null,
    ready: true, data: { catalog: join(root, 'catalog'), request }, revisionManifest: join(root, 'revision.json'),
    reviewSourceEvidence: { metadata: { path: 'metadata.json', sha256: digest(JSON.stringify(metadata)) }, license: { path: 'license.json', sha256: digest(JSON.stringify(license)) } } };
  const replaceLicense = async (status: 'ACTIVE' | 'REVOKED', revisionKind = 'APPEND') => {
    const body = JSON.stringify({ ...license, status }); await writeFile(join(root, 'license.json'), body);
    const nextFiles = files.map(file => file.path === 'license.json' ? { ...file, bytes: String(Buffer.byteLength(body)), sha256: digest(body) } : file);
    const nextManifest = { ...manifest, revisionKind, files: nextFiles };
    await writeFile(join(root, 'revision.json'), JSON.stringify({ ...nextManifest, contentDigest: digest(canonical(nextManifest)) }));
    return { ...configured, reviewSourceEvidence: { ...configured.reviewSourceEvidence!, license: { path: 'license.json', sha256: digest(body) } } };
  };
  const replaceMetadata = async (changed: JsonObject) => {
    const body = JSON.stringify(changed); await writeFile(join(root, 'metadata.json'), body);
    const nextFiles = files.map(file => file.path === 'metadata.json' ? { ...file, bytes: String(Buffer.byteLength(body)), sha256: digest(body) } : file);
    const nextManifest = { ...manifest, files: nextFiles }; await writeFile(join(root, 'revision.json'), JSON.stringify({ ...nextManifest, contentDigest: digest(canonical(nextManifest)) }));
    return { ...configured, reviewSourceEvidence: { ...configured.reviewSourceEvidence!, metadata: { path: 'metadata.json', sha256: digest(body) } } };
  };
  return { root, configured, metadata, replaceLicense, replaceMetadata, sealedDigest: files.find(file => file.path === 'sealed-metadata.json')!.sha256 as string, async close() { await rm(root, { recursive: true, force: true }); } };
}
test('configured source provenance requires byte bindings and the settled native validation receipt', async () => {
  const f = await fixture(); try {
    const captured = await prepareReviewSourceEvidence(f.configured); assert.ok(captured);
    assert.equal(trustedSourceChecks(captured, null, f.configured.data, '300')[0]!.status, 'UNKNOWN');
    // The actual Rust metadata validator is exercised separately; this tests receipt consumption.
    const native = { schema_version: 1, method_version: 'qz.review-source.v1', validated: true, metadata: captured.metadata };
    assert.deepEqual(trustedSourceChecks(captured, native, f.configured.data, '300').map(check => [check.id, check.status]), [['provenance', 'PASS'], ['no_leakage', 'PASS']]);
    assert.equal(trustedSourceChecks({ ...captured, license: { ...captured.license, status: 'REVOKED' } }, native, f.configured.data, '300')[0]!.status, 'FAIL');
    assert.equal(trustedSourceChecks({ ...captured, license: { ...captured.license, validUntilNs: '299' } }, native, f.configured.data, '300')[0]!.status, 'FAIL');
    assert.equal(trustedSourceChecks(captured, native, { ...f.configured.data, catalog: '/another/snapshot' }, '300')[0]!.status, 'FAIL');
    assert.equal(trustedSourceChecks(captured, { ...native, metadata: { ...captured.metadata, native_snapshot_ref: 'another-revision' } }, f.configured.data, '300')[0]!.status, 'UNKNOWN');
  } finally { await f.close(); }
});
test('source provenance accepts an absent or zero legacy row quota with exact original source scope', async () => {
  for (const maximumRows of [null, 0]) {
    const f = await fixture(maximumRows);
    try { assert.ok(await prepareReviewSourceEvidence(f.configured)); }
    finally { await f.close(); }
  }
});
test('source evidence cannot silently rewrite an omitted scientific row limit to zero', async () => {
  const f = await fixture(null);
  try {
    const quality = f.metadata.quality.datasets[0]!;
    const configured = await f.replaceMetadata({ ...f.metadata, quality: { datasets: [{ ...quality, selection: { ...quality.selection, maximum_rows: 0 } }] } });
    await assert.rejects(prepareReviewSourceEvidence(configured), /review_source_validation_scope_mismatch/);
    assert.equal('maximum_rows' in (configured.data.request as { forecast: { selection: object } }).forecast.selection, false);
  } finally { await f.close(); }
});
test('unconfigured and caller-authored claims cannot manufacture source provenance', async () => {
  const f = await fixture(); try {
    const { reviewSourceEvidence: _descriptor, ...unconfigured } = f.configured; assert.equal(await prepareReviewSourceEvidence(unconfigured), null);
    const checks = reviewSourceChecks({ goal: 'boundary', model: { provider: 'fixture', modelId: 'fixture' }, policy: {}, data: { ...f.configured.data, provenance: { verified: true, pit_verified: true } } },
      { taskId: 1, status: 'COMPUTED', proposal: {}, result: {} }, [], 'trial:fixture');
    assert.equal(checks.find(check => check.id === 'provenance')!.status, 'UNKNOWN');
    await writeFile(join(f.root, 'metadata.json'), JSON.stringify({ ...f.metadata, origin: 'FIXTURE' }));
    await assert.rejects(prepareReviewSourceEvidence(f.configured), /source_manifest_file_(?:digest_)?mismatch/);
  } finally { await f.close(); }
});
test('source descriptors reject unmanifested files, changed snapshots and root escapes', async () => {
  const f = await fixture(); try {
    await writeFile(join(f.root, 'unregistered.json'), '{}'); const descriptor = f.configured.reviewSourceEvidence!;
    await assert.rejects(prepareReviewSourceEvidence({ ...f.configured, reviewSourceEvidence: { ...descriptor, metadata: { path: 'unregistered.json', sha256: digest('{}') } } }), /review_source_file_not_bound/);
    await assert.rejects(prepareReviewSourceEvidence({ ...f.configured, reviewSourceEvidence: { ...descriptor, metadata: { path: '../metadata.json', sha256: descriptor.metadata.sha256 } } }), /review_source_descriptor_invalid/);
    await assert.rejects(prepareReviewSourceEvidence({ ...f.configured, data: { ...f.configured.data, request: { changed: true } } }), /review_source_snapshot_not_registered/);
  } finally { await f.close(); }
});

test('heldout source metadata binds its own original SEALED partition and cannot borrow Validation evidence', async () => {
  const f = await fixture(); try {
    const configured = { ...f.configured, reviewSourceEvidence: { ...f.configured.reviewSourceEvidence!, heldoutMetadata: { path: 'sealed-metadata.json', sha256: f.sealedDigest } } };
    const validation = await prepareReviewSourceEvidence(configured); const heldout = await prepareReviewSourceEvidence(configured, 'HELDOUT');
    assert.ok(validation); assert.ok(heldout); assert.equal(heldout.partition, 'HELDOUT'); assert.equal(heldout.metadata.partition, 'SEALED');
    assert.equal(heldout.validationStartNs, '180'); assert.equal(heldout.validationEndNs, '200');
    assert.notEqual(heldout.metadataDigest, validation.metadataDigest);
    const validationReceipt = { schema_version: 1, method_version: 'qz.review-source.v1', validated: true, metadata: validation.metadata };
    assert.equal(trustedSourceChecks(heldout, validationReceipt, configured.data, '300')[0]!.status, 'UNKNOWN');
    await assert.rejects(prepareReviewSourceEvidence({ ...configured, reviewSourceEvidence: { ...configured.reviewSourceEvidence, heldoutMetadata: configured.reviewSourceEvidence.metadata } }, 'HELDOUT'), /review_source_metadata_identity_mismatch/);
  } finally { await f.close(); }
});

test('new use rechecks current source revocation and retraction instead of trusting a saved active grant', async () => {
  const f = await fixture(); try {
    const captured = await prepareReviewSourceEvidence(f.configured); assert.ok(captured);
    assert.equal((await assertCurrentReviewSource(f.configured, captured)).license.status, 'ACTIVE');
    const revoked = await f.replaceLicense('REVOKED');
    await assert.rejects(assertCurrentReviewSource(revoked, captured), /review_current_source_authorization_inactive/);
    const retracted = await f.replaceLicense('ACTIVE', 'RETRACTION');
    await assert.rejects(assertCurrentReviewSource(retracted, captured), /review_source_retracted/);
  } finally { await f.close(); }
});

test('an empty native HELDOUT partition is distinct from leaked heldout observations', async () => {
  const f = await fixture(); try {
    const raw = f.configured.data.request as JsonObject;
    const data = { ...f.configured.data, request: { ...raw, schema_version: 1, forecast: { ...(raw.forecast as JsonObject), schema_version: 1, parameters: {} } } };
    const counts = { observations: '0', included: '0', excluded: '0', indicator_warmup: '0', incomplete_labels: '0', cross_boundary_labels: '0', prediction_outside_partition: '0' };
    const empty = { partition: 'HELDOUT', metric: { counts, pearson_ic: { value: null, status: 'INSUFFICIENT_SAMPLES' } } };
    const trial = { taskId: 1, status: 'COMPUTED', proposal: {}, result: { request: iterationRequest(data), partitions: [{ partition: 'TRAIN' }, { partition: 'VALIDATION' }, empty] } };
    const input = { goal: 'boundary only', model: { provider: 'fixture', modelId: 'fixture' }, policy: {}, data };
    assert.equal(reviewSourceChecks(input, trial, [trial], 'trial:fixture').find(check => check.id === 'no_leakage')!.status, 'PASS');
    const leaked = { ...trial, result: { ...trial.result, partitions: [{ ...empty, metric: { ...empty.metric, counts: { ...counts, observations: '1' } } }] } };
    assert.equal(reviewSourceChecks(input, leaked, [leaked], 'trial:fixture').find(check => check.id === 'no_leakage')!.status, 'FAIL');
  } finally { await f.close(); }
});

test('equal timestamps cannot bind another BAR or instrument set even with valid source hashes', async () => {
  const f = await fixture(); try {
    const quality = f.metadata.quality.datasets[0]!;
    for (const changed of [
      { ...quality, selection: { ...quality.selection, bar_types: ['OTHER.SIM-1-MINUTE-LAST-EXTERNAL'] }, instrument_ids: ['OTHER.SIM'] },
      { ...quality, instrument_ids: ['OTHER.SIM'] },
      { ...quality, instrument_ids: ['ORIGINAL.SIM', 'UNREQUESTED.SIM'] },
    ]) {
      const configured = await f.replaceMetadata({ ...f.metadata, quality: { datasets: [changed] } });
      await assert.rejects(prepareReviewSourceEvidence(configured), /review_source_(?:validation_scope|instrument_coverage)_mismatch/);
    }
  } finally { await f.close(); }
});
