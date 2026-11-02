import { test } from 'node:test';
import assert from 'node:assert/strict';
import { allocationReturnPanel, portfolioReviewVersion } from '../src/review-allocation.ts';
import type { PortfolioFact } from '../src/facts.ts';

test('allocation consumes original native daily return values without curve arithmetic', () => {
  const native = { returns_kind: 'PORTFOLIO_DAILY', returns_status: 'OK', returns: Array.from({ length: 30 }, (_, i) => ({ timestamp_ns: String(100 + i), value: i % 2 ? 0.01 : -0.01, reason_code: null })) };
  const panel = allocationReturnPanel(native);
  assert.deepEqual(panel.times, native.returns.map(row => row.timestamp_ns)); assert.deepEqual(panel.returns, native.returns.map(row => row.value));
  assert.throws(() => allocationReturnPanel({ ...native, returns_kind: 'POSITION' }), /native_daily_returns_required/);
  assert.throws(() => allocationReturnPanel({ ...native, returns: [...native.returns.slice(0, 29), { ...native.returns[29]!, value: null }] }), /native_returns_incomplete/);
  assert.throws(() => allocationReturnPanel({ ...native, returns: [...native.returns.slice(0, 29), native.returns[0]!] }), /native_returns_incomplete/);
});
test('portfolio version binds original membership weights policy and input while excluding presentation', () => {
  const source = { input: { requestId: 'one' }, evidence: [{ researchId: 1, selected: { taskId: 10 } }], weights: ['1'], policy: { kind: 'factor-threshold' }, view: { name: 'Original' } } as unknown as PortfolioFact;
  const original = portfolioReviewVersion(source);
  assert.equal(portfolioReviewVersion({ ...source, view: { ...source.view, name: 'Renamed' } }), original);
  assert.notEqual(portfolioReviewVersion({ ...source, weights: ['0.5'] }), original);
  assert.notEqual(portfolioReviewVersion({ ...source, policy: { kind: 'changed' } }), original);
});
test('external allocation results are checked against exact caps and turnover before shared replay', async () => {
  const { validateReviewAllocationWeights } = await import('../src/review-allocation.ts');
  const result = { schema_version: 1, method: 'test-only', kind: 'PORTFOLIO', solver_version: 'test-only',
    members: [{ id: 'a', source_report_id: 'ra', weight: '0.5', prior: 0.5, quality_coefficient: 1 }, { id: 'b', source_report_id: 'rb', weight: '0.5', prior: 0.5, quality_coefficient: 1 }],
    covariance: [[1, 0], [0, 1]], lambda: 1, objective: 0.5, primal_residual: 0, dual_residual: 0, requires_new_candidate: true, requires_shared_account_study: true } as const;
  const policy = { id: 'p', frozenAtNs: '100', kind: 'PORTFOLIO', observationTimesNs: [], observationEvidenceRefs: [], memberCaps: [{ id: 'a', maximumWeight: '0.6' }, { id: 'b', maximumWeight: '0.6' }], clusterCaps: [], currentWeights: [{ id: 'a', weight: '0.5' }, { id: 'b', weight: '0.5' }], maximumTurnover: '0.1' } as const;
  validateReviewAllocationWeights(structuredClone(result) as any, structuredClone(policy) as any);
  const invalid = structuredClone(result) as any; invalid.members[0].weight = '0.600000000000000001'; invalid.members[1].weight = '0.399999999999999999';
  assert.throws(() => validateReviewAllocationWeights(invalid, structuredClone(policy) as any), /cap_violated/);
  invalid.members[0].weight = '0.56'; invalid.members[1].weight = '0.44';
  assert.throws(() => validateReviewAllocationWeights(invalid, structuredClone(policy) as any), /turnover_violated/);
});
