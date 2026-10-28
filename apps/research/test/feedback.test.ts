import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { modelBriefInput, modelFeedback } from '../src/feedback.ts';

const count = '9007199254740993';
const metric = { counts: { included: count }, pearson_ic: { value: null, status: 'INSUFFICIENT_SAMPLES' } };

test('portfolio projection retains exact nullable account facts without changing the original or returning owner-only arrays', () => {
  const original: JsonObject = { schema_version: 1, request: { original: 'iteration request' }, evidenceTaskId: 7,
    partitions_component: { index: 0, meaning: 'COMPONENT_FACTOR_DIAGNOSTICS_NOT_ACCOUNT_METRICS' },
    partitions: [{ partition: 'VALIDATION', start_ns: count, metric, assets: [{ instrument_id: 'TEST.SIM', metric }] },
      { partition: 'HELDOUT', metric: { secret: 'FINAL_OWNER_ONLY' } }],
    components: [{ index: 0, weight: '0.2500000000000000001', summary: { schema_version: 1, consumed_fuel: null,
      request: { repeated: true }, partitions: [{ partition: 'VALIDATION', metric, assets: [{ raw: 'OWNER_ARRAY' }] }] } }],
    portfolio: { partition: 'VALIDATION', evidenceMode: 'HISTORICAL_REASSESSMENT', native_version: '0.63.0',
      statistics: [{ native_key: 'Calendar Geometric Daily Return', value: null, reason_code: 'NATIVE_STATISTIC_UNAVAILABLE' },
        { native_key: 'Calendar Observed Return Days', value: 61, reason_code: null }],
      summary: { balance: '3000.000001', snapshots: ['OWNER_ARRAY'] }, orders: count, returns_status: 'UNAVAILABLE',
      returns_reason: 'unpriced', returns: ['OWNER_ARRAY'], provenance: { account: 'ONE_NATIVE_SHARED_ACCOUNT' } } };
  const before = structuredClone(original); const projected = modelFeedback(original);
  assert.equal(Object.keys(projected)[0], 'portfolio');
  assert.deepEqual((projected.portfolio as JsonObject).statistics, (original.portfolio as JsonObject).statistics);
  assert.equal((projected.portfolio as JsonObject).orders, count);
  assert.equal(((projected.portfolio as JsonObject).summary as JsonObject).balance, '3000.000001');
  assert.equal(((projected.components as JsonObject[])[0]!.summary as JsonObject).consumed_fuel, null);
  assert.equal((projected.components as JsonObject[])[0]!.weight, '0.2500000000000000001');
  assert.deepEqual((projected.partitions as JsonObject[])[0]!.metric, metric);
  assert.equal((projected.partitions as JsonObject[])[0]!.start_ns, count);
  assert.equal(JSON.stringify(projected).includes('FINAL_OWNER_ONLY'), false);
  assert.equal(JSON.stringify(projected).includes('OWNER_ARRAY'), false);
  assert.deepEqual(original, before);
});

test('portfolio feedback cannot expose a final partition', () => {
  assert.throws(() => modelFeedback({ portfolio: { partition: 'HELDOUT', statistics: [{ value: 12345 }] } }), /validation_feedback_required/);
});

test('existing non-portfolio feedback remains unchanged', () => {
  const original = { request: {}, partitions: [{ partition: 'VALIDATION', metric }] };
  assert.equal(modelFeedback(original), original);
});

test('catalog ensemble brief exposes the same v2 observed and unavailable mask contract', () => {
  const brief = modelBriefInput({ data: {}, policy: { strategy: { kind: 'catalog-ensemble' } } }).data.feature_model as JsonObject;
  assert.match(String(brief.setter), /mask=0 means an observed finite value/);
  assert.match(String(brief.setter), /mask=1 means NotYetAvailable, 2 Expired, 3 SourceMissing/);
  assert.match(String(brief.prediction), /nonfinite output is not a missing-prediction marker/);
});
