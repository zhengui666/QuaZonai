import assert from 'node:assert/strict';
import test from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { fittedReviewPrior } from '../plugins/native-fitted-portfolio.ts';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';

function fixture(end = '200') {
  const members = [1, 2].map(sourceTrialId => ({ sourceTrialId, factorVersionId: `factor:7:${sourceTrialId}`, model: 'original' }));
  const trials = members.map(member => ({ taskId: member.sourceTrialId, result: { request: { validation_end_ns: end } } }));
  const records: Record<string, JsonObject> = {};
  for (const member of members) {
    const policy = createReviewPolicy('FACTOR', `policy-${member.sourceTrialId}`, '1000');
    policy.calibration = { rootCount: 30, endNs: '50', evidenceRefs: ['historical-reference'] };
    const report = scoreReview({ id: `report-${member.sourceTrialId}`, target: { kind: 'FACTOR', id: `research:7:trial:${member.sourceTrialId}`,
      version: `version-${member.sourceTrialId}`, sourceEvidenceRefs: [`research:7:trial:${member.sourceTrialId}:validation`], evaluationStartNs: '100' },
      policy, metrics: policy.metricRules.map(rule => ({ key: rule.key, value: rule.direction === 'HIGHER' ? rule.high : rule.low,
        status: 'AVAILABLE', observationCount: 100, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: `research:7:trial:${member.sourceTrialId}:validation` })),
      checks: policy.requiredChecks.map(id => ({ id, status: 'PASS', reason: 'original check', evidenceRefs: ['native-original'] })), createdAt: '2026-11-02T00:00:00Z' });
    assert.equal(report.decision, 'PASS');
    records[report.target.version] = JSON.parse(JSON.stringify({ trialId: member.sourceTrialId, status: 'COMPLETED', version: report.target.version, policyId: report.policyId, report }));
  }
  return { members, trials, records };
}
test('fit prior comes from original PASS receipts and reports their complete source identity', () => {
  const source = fixture(); const result = fittedReviewPrior(source.members, source.records, source.trials, 7, '300');
  assert.equal(result.reason, null);
  assert.deepEqual((result.prior!.members as JsonObject[])[0], { factor_version_id: 'factor:7:1', source_report_id: 'report-1',
    source_evidence_refs: ['research:7:trial:1:validation'], source_end_ns: '200', decision: 'PASS', quality_coefficient: 1 });
});
test('current validation quality cannot be relabelled as TRAIN-only fitting evidence', () => {
  const source = fixture('300');
  assert.deepEqual(fittedReviewPrior(source.members, source.records, source.trials, 7, '300'), { prior: null, reason: 'review_evidence_not_before_training_cutoff' });
  source.trials[0]!.result.request.validation_end_ns = '301';
  assert.equal(fittedReviewPrior(source.members, source.records, source.trials, 7, '300').prior, null);
});
test('missing, wrong-version, and manipulated quality receipts cannot create priors', () => {
  const source = fixture();
  assert.equal(fittedReviewPrior(source.members, {}, source.trials, 7, '300').reason, 'historical_pass_review_missing');
  source.members[0]!.factorVersionId = 'factor:wrong:1';
  assert.equal(fittedReviewPrior(source.members, source.records, source.trials, 7, '300').reason, 'review_factor_version_mismatch');
  source.members[0]!.factorVersionId = 'factor:7:1';
  (source.records['version-1']!.report as JsonObject).qualityCoefficient = 0.5;
  assert.equal(fittedReviewPrior(source.members, source.records, source.trials, 7, '300').prior, null);
});
