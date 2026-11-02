import assert from 'node:assert/strict';
import test from 'node:test';
import { nativeFactorReviewMetrics, projectReviewScienceMetrics } from '../src/review-science-adapter.ts';

const diagnostic = (value: number) => ({ method_version: 'qz.review-science.v1', scope: 'POOLED_TIME_SERIES', observations: 100,
  metrics: [{ code: 'rank_ic_lower', value, status: 'AVAILABLE', reason: null }], limitations: [] });

test('review projection keeps original validation diagnostics separate from heldout', () => {
  const source = { partitions: [{ partition: 'VALIDATION', diagnostics: diagnostic(-0.02) }, { partition: 'HELDOUT', diagnostics: diagnostic(0.9) }] };
  const [metric] = nativeFactorReviewMetrics(source, 'native:trial1');
  assert.equal(metric!.value, -0.02); assert.equal(metric!.key, 'factor.rank_ic_lower');
  assert.equal(metric!.method, 'qz.review-science.v1'); assert.equal(metric!.unit, 'CORRELATION');
  assert.equal(metric!.evidenceRef, 'native:trial1#validation');
  assert.equal(nativeFactorReviewMetrics(source, 'native:trial1', 'HELDOUT')[0]!.value, 0.9);
});

test('historical summaries and unknown producers do not acquire fabricated review evidence', () => {
  assert.deepEqual(nativeFactorReviewMetrics({ partitions: [{ partition: 'VALIDATION', metric: { pearson_ic: { value: 0.8 } } }] }, 'old'), []);
  assert.deepEqual(projectReviewScienceMetrics({ ...diagnostic(0.9), method_version: 'model-claim' }, 'bad'), []);
  assert.deepEqual(nativeFactorReviewMetrics({ partitions: [{ partition: 'HELDOUT', diagnostics: diagnostic(0.9) }] }, 'only-heldout'), []);
});

test('missing confidence remains null and contradictory available evidence is rejected', () => {
  const missing = { ...diagnostic(0.9), metrics: [{ code: 'rank_ic_lower', value: null, status: 'INSUFFICIENT_EVIDENCE', reason: 'TOO_FEW_BLOCKS' }] };
  assert.equal(projectReviewScienceMetrics(missing, 'ref')[0]!.value, null);
  assert.equal(projectReviewScienceMetrics(missing, 'ref')[0]!.status, 'INSUFFICIENT');
  assert.throws(() => projectReviewScienceMetrics({ ...diagnostic(0.9), metrics: [{ code: 'rank_ic_lower', value: null, status: 'AVAILABLE' }] }, 'ref'), /value_invalid/);
});
