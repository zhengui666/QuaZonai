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
  assert.equal(result.calibration!.rootCount, 30); assert.equal(result.metricRules[0]!.low, 0.02); assert.equal(result.metricRules[0]!.high, 0.26);
  records[29]!.rootId = records[0]!.rootId; assert.throws(() => deriveReviewPolicyFromReference(request.policy, records, '101'), /roots_required/);
});
test('pooled time series results cannot satisfy a cross-sectional rule', () => {
  const request = input(); const rule = request.policy.metricRules.find(r => r.key === 'factor.rank_ic_lower')!;
  rule.scope = 'CROSS_SECTIONAL'; const report = scoreReview(request);
  assert.equal(report.decision, 'INCONCLUSIVE'); assert.ok(report.missingEvidence.some(r => r.includes('method_or_scope_mismatch')));
});
