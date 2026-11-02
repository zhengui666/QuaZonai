import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createReviewPolicy, scoreReview, validateReviewPolicy, buildReviewAllocationRequest } from '../../../packages/contracts/src/review-scoring.ts';
import type { ReviewMetric, ReviewScoringInput } from '../../../packages/contracts/src/review.ts';
function input(): ReviewScoringInput {
  const policy = createReviewPolicy('FACTOR', 'policy-1', '100');
  policy.calibration = { rootCount: 30, endNs: '90', evidenceRefs: ['reference-cohort'] };
  const metrics: ReviewMetric[] = policy.metricRules.map(rule => ({ key: rule.key, value: rule.high, status: 'AVAILABLE',
    observationCount: 100, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: 'native-1' }));
  return { id: 'report-1', target: { kind: 'FACTOR', id: 'f1', version: 'sha256:v1', sourceEvidenceRefs: ['native-1'], evaluationStartNs: '101' }, policy, metrics,
    checks: policy.requiredChecks.map(id => ({ id, status: 'PASS', reason: 'verified', evidenceRefs: ['source-1'] })), createdAt: '2026-11-02T00:00:00Z' };
}
test('evidence score is deterministic and report fields cannot override hard failure', () => {
  const request = input(); const report = scoreReview(request);
  assert.equal(report.totalScore, 100); assert.equal(report.decision, 'PASS'); assert.equal(report.qualityCoefficient, 1);
  request.checks.find(c => c.id === 'no_leakage')!.status = 'FAIL';
  const failure = scoreReview(request); assert.equal(failure.decision, 'REJECT'); assert.equal(failure.totalScore, 100); assert.equal(failure.qualityCoefficient, 0);
});
test('missing metrics or reference cohort do not redistribute weight or auto-release', () => {
  const request = input(); request.metrics.pop(); const result = scoreReview(request);
  assert.equal(result.totalScore, null); assert.equal(result.decision, 'INCONCLUSIVE');
  const cold = input(); cold.policy.calibration = null;
  assert.equal(scoreReview(cold).decision, 'INCONCLUSIVE');
});
test('observation band and required dimension floor are independent of total', () => {
  const request = input(); const rule = request.policy.metricRules.find(r => r.dimension === 'ROBUSTNESS')!;
  request.metrics.find(m => m.key === rule.key)!.value = rule.low + (rule.high - rule.low) * 0.59;
  const report = scoreReview(request); assert.ok(report.totalScore! >= 80); assert.equal(report.decision, 'INCONCLUSIVE');
});
test('policy cannot relax minimum thresholds or remove mandatory checks', () => {
  const request = input(); request.policy.passScore = 79; assert.throws(() => validateReviewPolicy(request.policy));
  const request2 = input(); request2.policy.requiredChecks.pop(); assert.throws(() => validateReviewPolicy(request2.policy));
});
test('duplicate and source-free evidence are not accepted', () => {
  const request = input(); request.metrics.push(request.metrics[0]!); assert.throws(() => scoreReview(request));
  const sourceFree = input(); sourceFree.checks[0]!.evidenceRefs = []; assert.equal(scoreReview(sourceFree).decision, 'INCONCLUSIVE');
});
test('historical market time is distinct from policy wall-clock admission', () => {
  const request = input(); request.policy.frozenAtNs = '99999'; assert.equal(scoreReview(request).decision, 'PASS');
  request.policy.calibration!.endNs = '101'; assert.equal(scoreReview(request).decision, 'INCONCLUSIVE');
});
test('weight request recomputes report authority and binds aligned observations', () => {
  const one = scoreReview(input()); const secondInput = input(); secondInput.id = 'report-2'; secondInput.target.id = 'f2'; const two = scoreReview(secondInput);
  const policy = { id: 'allocation-1', frozenAtNs: '1000', kind: 'FACTOR' as const, observationTimesNs: Array.from({ length: 30 }, (_, i) => String(200 + i)), observationEvidenceRefs: ['native-1', 'native-2'], memberCaps: [{ id: 'f1', maximumWeight: '0.8' }, { id: 'f2', maximumWeight: '0.8' }], clusterCaps: [], currentWeights: [{ id: 'f1', weight: '0' }, { id: 'f2', weight: '0' }], maximumTurnover: '1' };
  const members = [{ id: 'f1', report: one, observations: Array(30).fill(0.1) }, { id: 'f2', report: two, observations: Array(30).fill(0.2) }];
  assert.equal(buildReviewAllocationRequest(members, policy).members[0]!.source_report_id, one.id);
  one.checks[0]!.status = 'FAIL'; assert.throws(() => buildReviewAllocationRequest(members, policy));
});
test('calibration derives its anchors from distinct earlier roots rather than current winners', async () => {
  const { deriveReviewPolicyFromReference } = await import('../../../packages/contracts/src/review-scoring.ts');
  const request = input(); request.policy.calibration = null;
  const records = Array.from({ length: 30 }, (_, i) => ({ rootId: `root-${i}`, endNs: '80', evidenceRefs: [`reference-${i}`],
    metrics: request.metrics.map(metric => ({ ...metric, value: i / 100, evidenceRef: `reference-${i}` })) }));
  const result = deriveReviewPolicyFromReference(request.policy, records, '101');
  assert.equal(result.calibration!.rootCount, 30); assert.equal(result.metricRules.find(rule => rule.key === 'factor.rank_ic_lower')!.low, 0.02); assert.equal(result.metricRules.find(rule => rule.key === 'factor.rank_ic_lower')!.high, 0.26);
  assert.equal(result.metricRules[0]!.low, 0.9); assert.equal(result.metricRules[0]!.high, 1);
  records[29]!.rootId = records[0]!.rootId; assert.throws(() => deriveReviewPolicyFromReference(request.policy, records, '101'), /roots_required/);
});
test('pooled time series results cannot satisfy a cross-sectional rule', () => {
  const request = input(); const rule = request.policy.metricRules.find(r => r.key === 'factor.rank_ic_lower')!;
  rule.scope = 'CROSS_SECTIONAL'; const report = scoreReview(request);
  assert.equal(report.decision, 'INCONCLUSIVE'); assert.ok(report.missingEvidence.some(r => r.includes('method_or_scope_mismatch')));
});
for (const kind of ['FACTOR', 'PORTFOLIO', 'ALLOCATION'] as const) {
  test(`${kind}: the best member of a losing reference cohort cannot pass absolute admission requirements`, async () => {
    const { deriveReviewPolicyFromReference } = await import('../../../packages/contracts/src/review-scoring.ts');
    const request = input(); request.target.kind = kind;
    request.policy = createReviewPolicy(kind, `policy-${kind}`, '100');
    request.checks = request.policy.requiredChecks.map(id => ({ id, status: 'PASS', reason: 'fixture verified', evidenceRefs: ['native-1'] }));
    const records = Array.from({ length: 30 }, (_, index) => ({ rootId: `root-${index}`, endNs: '80', evidenceRefs: [`reference-${index}`], metrics: request.policy.metricRules.map(rule => ({ key: rule.key,
      value: rule.key === 'selection.dsr' ? 0.5 + index / 1000 : rule.minimumTarget ? -0.3 + index / 1000 : rule.low + (rule.high - rule.low) * (index + 1) / 31,
      status: 'AVAILABLE' as const, observationCount: 100, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: `reference-${index}` })) }));
    const minimums = request.policy.metricRules.map(rule => rule.minimumTarget);
    request.policy = deriveReviewPolicyFromReference(request.policy, records, '101');
    assert.deepEqual(request.policy.metricRules.map(rule => rule.minimumTarget), minimums, 'reference fitting must not alter absolute minimums');
    request.metrics = request.policy.metricRules.map(rule => ({ key: rule.key, value: rule.high, status: 'AVAILABLE', observationCount: 100, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: 'native-1' }));
    const result = scoreReview(request);
    assert.equal(result.totalScore, 100, 'relative ranking is deliberately excellent'); assert.equal(result.decision, 'REJECT'); assert.equal(result.qualityCoefficient, 0);
    for (const key of [kind === 'FACTOR' ? 'factor.rank_ic_lower' : 'account.net_return_lower', 'execution.stressed_net_return', 'incremental.net_return_delta_lower']) assert.ok(result.hardFailures.includes(`${key}:below_absolute_admission_minimum`), key);
  });
}
test('absolute requirements cannot be removed, made optional, relabelled or relaxed', () => {
  for (const change of [
    (rule: any) => { delete rule.minimumTarget; },
    (rule: any) => { rule.minimumTarget.value = -0.01; },
    (rule: any) => { rule.minimumTarget.inclusive = true; },
    (rule: any) => { rule.required = false; },
    (rule: any) => { rule.key = 'factor.other_score'; },
  ]) {
    const request = input(); change(request.policy.metricRules.find(rule => rule.key === 'factor.rank_ic_lower'));
    assert.throws(() => validateReviewPolicy(request.policy), /minimum_target/);
  }
  const request = input(); request.policy.metricRules.find(rule => rule.key === 'selection.dsr')!.minimumTarget!.value = 0.94;
  assert.throws(() => validateReviewPolicy(request.policy), /minimum_target/);
});
test('zero signed evidence is rejected, DSR 0.95 is allowed, and missing floor evidence stays inconclusive', () => {
  for (const key of ['factor.rank_ic_lower', 'execution.stressed_net_return', 'incremental.net_return_delta_lower']) {
    const request = input(); request.metrics.find(metric => metric.key === key)!.value = 0;
    assert.equal(scoreReview(request).decision, 'REJECT', key);
  }
  const allowed = input(); allowed.metrics.find(metric => metric.key === 'selection.dsr')!.value = 0.95;
  assert.equal(scoreReview(allowed).decision, 'PASS');
  const missing = input(); const metric = missing.metrics.find(metric => metric.key === 'factor.rank_ic_lower')!;
  metric.value = null; metric.status = 'INSUFFICIENT';
  assert.equal(scoreReview(missing).decision, 'INCONCLUSIVE');
});
test('independent outcome enforces its frozen sample and metric contract before testing absolute profitability', async () => {
  const { reviewIndependentOutcomeCheck } = await import('../../../packages/contracts/src/review-scoring.ts');
  const policy = createReviewPolicy('ALLOCATION', 'independent-policy', '100');
  for (const rule of policy.metricRules) if (['account.net_return_lower', 'data.coverage'].includes(rule.key)) rule.minimumObservations = 1000;
  const metrics = ['account.net_return_lower', 'data.coverage'].map(key => {
    const rule = policy.metricRules.find(rule => rule.key === key)!;
    return { key, value: key === 'data.coverage' ? 1 : 0.1, status: 'AVAILABLE' as const, observationCount: 200, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: 'sealed:native' };
  });
  assert.equal(reviewIndependentOutcomeCheck(policy, metrics, 'sealed:native').status, 'UNKNOWN', 'an otherwise available interval may have fewer observations than the frozen policy requires');
  metrics.forEach(metric => { metric.observationCount = 1000; });
  assert.equal(reviewIndependentOutcomeCheck(policy, metrics, 'sealed:native').status, 'PASS');
  metrics[0]!.scope = 'POOLED_TIME_SERIES'; assert.equal(reviewIndependentOutcomeCheck(policy, metrics, 'sealed:native').status, 'UNKNOWN');
  metrics[0]!.scope = 'SHARED_ACCOUNT'; metrics[0]!.value = -0.01;
  assert.equal(reviewIndependentOutcomeCheck(policy, metrics, 'sealed:native').status, 'FAIL');
});
test('a final portfolio outcome remains a veto even when its pre-test policy did not require that result', () => {
  const request = input(); request.target.kind = 'PORTFOLIO'; request.policy = createReviewPolicy('PORTFOLIO', 'portfolio-policy', '100');
  request.policy.calibration = { rootCount: 30, endNs: '90', evidenceRefs: ['reference-cohort'] };
  request.metrics = request.policy.metricRules.map(rule => ({ key: rule.key, value: rule.high, status: 'AVAILABLE', observationCount: 100, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: 'native-1' }));
  assert.equal(scoreReview(request).decision, 'PASS', 'pre-test score can pass without reading the independent outcome');
  request.checks.push({ id: 'independent_outcome', status: 'FAIL', reason: 'native_independent_return_below_frozen_minimum', evidenceRefs: ['native-1'] });
  assert.equal(scoreReview(request).decision, 'REJECT');
  request.checks.at(-1)!.status = 'UNKNOWN'; assert.equal(scoreReview(request).decision, 'INCONCLUSIVE');
});
test('thirty same-profile references with identical bounded metrics register using fixed versioned scales', async () => {
  const { deriveReviewPolicyFromReference } = await import('../../../packages/contracts/src/review-scoring.ts');
  const request = input(); request.policy.calibration = null;
  const dsr = request.policy.metricRules.find(rule => rule.key === 'selection.dsr')!;
  request.policy.metricRules.push({ ...dsr, key: 'selection.pbo', direction: 'LOWER', low: 0, high: 1, minimumTarget: undefined });
  const constants: Record<string, number> = { 'data.coverage': 0.985, 'robustness.positive_fraction': 1, 'mechanism.evidence_fraction': 1, 'selection.dsr': 0.99, 'selection.pbo': 0 };
  const records = Array.from({ length: 30 }, (_, index) => ({ rootId: `root-${index}`, endNs: '80', evidenceRefs: [`reference-${index}`], metrics: request.policy.metricRules.map(rule => ({ key: rule.key,
    value: constants[rule.key] ?? 0.01 + index / 1000, status: 'AVAILABLE' as const, observationCount: 100, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: `reference-${index}` })) }));
  const policy = deriveReviewPolicyFromReference(request.policy, records, '101');
  for (const [key, low, high] of [['data.coverage', 0.9, 1], ['robustness.positive_fraction', 0.5, 1], ['mechanism.evidence_fraction', 0, 1], ['selection.dsr', 0.5, 0.99], ['selection.pbo', 0, 1]] as const) {
    const rule = policy.metricRules.find(rule => rule.key === key)!;
    assert.equal(rule.low, low, key); assert.equal(rule.high, high, key); assert.equal(rule.normalization, 'FIXED_BOUNDS_V1', key);
  }
  assert.equal(policy.calibration!.rootCount, 30);
  assert.equal(policy.metricRules.find(rule => rule.key === 'factor.rank_ic_lower')!.normalization, 'REFERENCE_P10_P90_V1');
  assert.deepEqual(policy.metricRules.find(rule => rule.key === 'selection.dsr')!.minimumTarget, { value: 0.95, inclusive: true });
  records[0]!.metrics.find(metric => metric.key === 'data.coverage')!.status = 'UNAVAILABLE' as any;
  records[0]!.metrics.find(metric => metric.key === 'data.coverage')!.value = null as any;
  assert.throws(() => deriveReviewPolicyFromReference(request.policy, records, '101'), /review_reference_metric_insufficient/, 'fixed scales still require 30 qualified original values');
});
test('constant predictive or economic reference evidence does not get fabricated percentile anchors', async () => {
  const { deriveReviewPolicyFromReference } = await import('../../../packages/contracts/src/review-scoring.ts');
  for (const kind of ['FACTOR', 'PORTFOLIO', 'ALLOCATION'] as const) {
    const policy = createReviewPolicy(kind, `degenerate-${kind}`, '100');
    const records = Array.from({ length: 30 }, (_, index) => ({ rootId: `root-${index}`, endNs: '80', evidenceRefs: [`reference-${index}`], metrics: policy.metricRules.map(rule => ({ key: rule.key, value: rule.key === 'factor.rank_ic_lower' || rule.key === 'account.net_return_lower' ? 0.02 : rule.minimumTarget ? 0.96 + index / 1000 : 0.99,
      status: 'AVAILABLE' as const, observationCount: 100, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: `reference-${index}` })) }));
    assert.throws(() => deriveReviewPolicyFromReference(policy, records, '101'), /review_reference_metric_degenerate/, kind);
  }
});
