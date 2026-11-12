import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import type { ConversationId, JsonObject } from '@earendil-works/pi-durable';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { bindReviewSourceEvidence, ReviewIndependentSourceEvidence, ReviewSourceEvidence, reviewSourceDataDigest } from '../src/review-source.ts';
import type { CapturedReviewSource } from '../src/review-source.ts';

const context = BACKGROUND_CONTEXT;
const sealed: JsonObject = { catalog: '/TEST_ONLY/source/sealed', request: { scope: 'TEST_ONLY_HELDOUT' } };
const data: JsonObject = { catalog: '/TEST_ONLY/source/validation', request: { scope: 'TEST_ONLY_VALIDATION' }, sealed };
function capture(partition: 'VALIDATION' | 'HELDOUT', input: JsonObject): CapturedReviewSource {
  return { schemaVersion: 1, partition, datasetId: 'TEST_ONLY_DATASET', sourceId: 'TEST_ONLY_SOURCE', sourceRevision: `TEST_ONLY_${partition}`,
    snapshotDigest: `TEST_ONLY_SNAPSHOT_${partition}`, inputDataDigest: reviewSourceDataDigest(input), metadataDigest: `TEST_ONLY_METADATA_${partition}`, licenseDigest: `TEST_ONLY_GRANT_${partition}`,
    metadata: { partition: partition === 'HELDOUT' ? 'SEALED' : 'VALIDATION', TEST_ONLY_AUTHORITY_BINDING_NOT_MARKET_PROOF: true },
    license: { schemaVersion: 1, datasetId: 'TEST_ONLY_DATASET', sourceId: 'TEST_ONLY_SOURCE', sourceRevision: `TEST_ONLY_${partition}`, datasetRevisionId: `TEST_ONLY_NATIVE_${partition}`,
      grantId: `TEST_ONLY_GRANT_${partition}`, issuer: 'TEST_ONLY_ISSUER', reference: 'TEST_ONLY_REFERENCE', status: 'ACTIVE', purposes: ['RESEARCH'], validFromNs: '0', validUntilNs: '9223372036854775807' },
    receivedAt: '2026-11-02T00:00:00Z', validationStartNs: partition === 'VALIDATION' ? '10' : '20', validationEndNs: partition === 'VALIDATION' ? '20' : '30' };
}
const bundle = (): CapturedReviewSource => ({ ...capture('VALIDATION', data), independent: capture('HELDOUT', sealed) });
async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-bundle-')); const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  return { get host() { return host; }, faux,
    async conversation() { return host.harness.commit(tx => tx.createConversation({ ownership: { kind: 'ownerless' } }), context); },
    async bind(id: ConversationId, source: CapturedReviewSource) { await host.harness.commit(tx => bindReviewSourceEvidence(tx, id, data, source), context); },
    async reopen() { await host.close(); host = await openResearch(options); },
    async close() { await host.close(); await rm(directory, { recursive: true, force: true }); } };
}
// These exercise official Durable admission/rollback/identity. Source assertions are
// explicitly test-only; actual source files and native metadata validity are tested elsewhere.
test('a source bundle atomically freezes separate Validation and HELDOUT documents before model execution', async () => {
  const f = await fixture(); try {
    const conversation = await f.conversation(); const source = bundle(); await f.bind(conversation.id, source);
    const primary = await f.host.harness.snapshot(ReviewSourceEvidence, conversation.id, context);
    const independent = await f.host.harness.snapshot(ReviewIndependentSourceEvidence, conversation.id, context);
    assert.deepEqual(primary?.captured, source); assert.deepEqual(independent?.captured, source.independent);
    assert.equal(primary?.native, null); assert.equal(independent?.native, null); assert.equal(f.faux.state.callCount, 0);
    await f.reopen();
    assert.deepEqual(await f.host.harness.snapshot(ReviewSourceEvidence, conversation.id, context), primary);
    assert.deepEqual(await f.host.harness.snapshot(ReviewIndependentSourceEvidence, conversation.id, context), independent);
    await f.bind(conversation.id, source); assert.equal(f.faux.state.callCount, 0, 'replay does not run an evaluator or researcher');
  } finally { await f.close(); }
});

test('a primary source without an independent proof creates no borrowed independent evidence', async () => {
  const f = await fixture(); try {
    const conversation = await f.conversation(); const primary = capture('VALIDATION', data); await f.bind(conversation.id, primary);
    assert.deepEqual((await f.host.harness.snapshot(ReviewSourceEvidence, conversation.id, context))?.captured, primary);
    assert.equal((await f.host.harness.snapshot(ReviewIndependentSourceEvidence, conversation.id, context))?.captured ?? null, null);
  } finally { await f.close(); }
});

test('wrong independent data or partition rejects the whole transaction and cannot poison snapshot identities', async () => {
  const f = await fixture(); try {
    for (const wrong of [
      { ...capture('HELDOUT', sealed), inputDataDigest: reviewSourceDataDigest({ catalog: '/OTHER_UNREGISTERED_SOURCE' }) },
      { ...capture('HELDOUT', sealed), partition: 'VALIDATION' as const },
      { ...capture('HELDOUT', sealed), independent: capture('HELDOUT', sealed) },
    ]) {
      const conversation = await f.conversation(); const original = bundle(); original.sourceRevision = `TEST_ONLY_FAILED_${conversation.id}`; original.license.sourceRevision = original.sourceRevision; original.independent = wrong;
      await assert.rejects(f.bind(conversation.id, original), /review_independent_source_input_mismatch/);
      assert.equal((await f.host.harness.snapshot(ReviewSourceEvidence, conversation.id, context))?.captured ?? null, null);
      assert.equal((await f.host.harness.snapshot(ReviewIndependentSourceEvidence, conversation.id, context))?.captured ?? null, null);
      // A different digest under the failed attempt's revision remains admissible:
      // the failed transaction did not retain even its earlier primary writes.
      const corrected = bundle(); corrected.snapshotDigest = `TEST_ONLY_CORRECTED_${conversation.id}`;
      corrected.sourceRevision = original.sourceRevision; corrected.license.sourceRevision = original.sourceRevision;
      await f.bind(conversation.id, corrected);
      assert.ok((await f.host.harness.snapshot(ReviewIndependentSourceEvidence, conversation.id, context))?.captured);
    }
  } finally { await f.close(); }
});

test('session snapshot identities protect both primary and independent revisions across conversations', async () => {
  const f = await fixture(); try {
    const first = await f.conversation(); const source = bundle(); await f.bind(first.id, source);
    const primaryConflict = await f.conversation();
    await assert.rejects(f.bind(primaryConflict.id, { ...source, snapshotDigest: 'DIFFERENT_PRIMARY_BYTES' }), /review_source_revision_conflict/);
    assert.equal((await f.host.harness.snapshot(ReviewSourceEvidence, primaryConflict.id, context))?.captured ?? null, null);
    const independentConflict = await f.conversation();
    await assert.rejects(f.bind(independentConflict.id, { ...source, independent: { ...source.independent!, snapshotDigest: 'DIFFERENT_INDEPENDENT_BYTES' } }), /review_independent_source_revision_conflict/);
    assert.equal((await f.host.harness.snapshot(ReviewSourceEvidence, independentConflict.id, context))?.captured ?? null, null);
    assert.equal((await f.host.harness.snapshot(ReviewIndependentSourceEvidence, independentConflict.id, context))?.captured ?? null, null);
    assert.equal((await f.host.harness.snapshot(ReviewSourceEvidence, first.id, context))?.captured?.snapshotDigest, source.snapshotDigest);
    assert.equal((await f.host.harness.snapshot(ReviewIndependentSourceEvidence, first.id, context))?.captured?.snapshotDigest, source.independent!.snapshotDigest);
  } finally { await f.close(); }
});
