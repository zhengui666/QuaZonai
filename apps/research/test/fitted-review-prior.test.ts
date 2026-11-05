import assert from 'node:assert/strict';
import test from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { fittedReviewPrior } from '../plugins/native-fitted-portfolio.ts';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';

function fixture(end = '200', count = 2) {
  const members = Array.from({ length: count }, (_, index) => index + 1).map(sourceTrialId => ({ sourceTrialId, factorVersionId: `factor:7:${sourceTrialId}`, model: 'original' }));
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

test('sourceFactorIds traverses the real Durable tool and frozen task to the native fit boundary', { timeout: 20_000 }, async () => {
  const { mkdtemp, writeFile, readFile, rm } = await import('node:fs/promises');
  const { join } = await import('node:path'); const { tmpdir } = await import('node:os');
  const { createHash } = await import('node:crypto');
  const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } = await import('@earendil-works/pi-ai');
  const { BACKGROUND_CONTEXT: context } = await import('@earendil-works/chord/context');
  const { openResearch } = await import('../src/host.ts');
  const { default: fitted } = await import('../plugins/fitted-ensemble.ts');
  const { default: research } = await import('../plugins/research.ts');
  const { startLegacyResearch } = await import('./legacy-study.ts');
  const { compileWasm } = await import('../plugins/native-wasm.ts');
  const { readNativeEvidence } = await import('../plugins/native-evidence.ts');
  const { object } = await import('../src/trials.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-reviewed-flow-')); const log = join(directory, 'calls.jsonl'); const binary = join(directory, 'job');
  await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('./fitted-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
  const old = process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const source = fixture('5');
  const model = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 99))';
  const members = source.members.map(member => {
    const report = source.records[`version-${member.sourceTrialId}`]!.report as unknown as import('../../../packages/contracts/src/review.ts').ReviewReport;
    report.policy.calibration!.endNs = '1'; report.target.evaluationStartNs = '3';
    return { factorId: `r7.t${member.sourceTrialId}.m0`, factorVersionId: member.factorVersionId, sourceResearchId: 7,
      sourceTrialId: member.sourceTrialId, modelSource: model, modelSha256: createHash('sha256').update(compileWasm(model)).digest('hex'), sourceEndNs: '5', report };
  });
  const data = { catalog: directory, request: { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
    selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30' },
    parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1 } } } };
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [research, fitted] });
  try {
    faux.setResponses([fauxAssistantMessage(fauxToolCall('strategy_ensemble_fit', { hypothesis: 'Protocol-only admitted source IDs', sourceFactorIds: members.map(member => member.factorId) }), { stopReason: 'toolUse' }), fauxAssistantMessage('Protocol trace complete.')]);
    const receipt = await startLegacyResearch(host, 'historical-library-fit', { goal: 'Protocol-only cross-study fit, not scientific evidence', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data,
      policy: { max_trials: 1, reviewedFactorSources: JSON.parse(JSON.stringify({ schemaVersion: 1, fitEndNs: '10', members })) } });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const state = await host.state(receipt.conversationId); const result = object(state.research!.trials[0]!.result, 'fit_result');
    assert.equal(object(result.reviewPrior, 'prior').status, 'APPLIED');
    const nativeCalls = (await readFile(log, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    const fitCall = nativeCalls.find(call => call.command === 'fit-ensemble');
    assert.deepEqual(fitCall.input.review_prior.members.map((member: { factor_version_id: string }) => member.factor_version_id), ['factor:7:1', 'factor:7:2']);
    assert.ok(fitCall.input.review_prior.members.every((member: { source_end_ns: string }) => member.source_end_ns === '5'));
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.ensemble.fit-replay' }, 10), context);
    assert.equal(tasks.items.length, 1); const task = tasks.items[0]!;
    assert.equal(task.state.outcome!.status, 'completed');
    if (task.state.outcome!.status !== 'completed') return;
    const evidence = await readNativeEvidence(host.harness, receipt.conversationId, task.id, object((task.state.outcome!.result as JsonObject).native, 'native'), context);
    assert.deepEqual(object(evidence.fitRequest, 'request').review_prior, fitCall.input.review_prior);
  } finally { await host.close(); if (old === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = old; await rm(directory, { recursive: true, force: true }); }
});

test('all 65 authorized reviewed factor sources remain eligible for fitting', async () => {
  const { admittedReviewedMembers } = await import('../plugins/native-fitted-portfolio.ts');
  const { compileWasm } = await import('../plugins/native-wasm.ts');
  const { createHash } = await import('node:crypto');
  const source = fixture('5', 65); const model = '(module)';
  const modelSha256 = createHash('sha256').update(compileWasm(model)).digest('hex');
  const members = source.members.map(member => {
    const report = structuredClone(source.records[`version-${member.sourceTrialId}`]!.report) as unknown as import('../../../packages/contracts/src/review.ts').ReviewReport;
    report.policy.calibration!.endNs = '1'; report.target.evaluationStartNs = '3';
    return { factorId: `r7.t${member.sourceTrialId}.m0`, factorVersionId: member.factorVersionId, sourceResearchId: 7,
      sourceTrialId: member.sourceTrialId, modelSource: model, modelSha256, sourceEndNs: '5', report };
  });
  const actual = admittedReviewedMembers({ schemaVersion: 1, fitEndNs: '10', members }, members.map(member => member.factorId), '10');
  assert.equal(actual.members.length, 65);
  assert.deepEqual(actual.members.map(member => member.factorVersionId), members.map(member => member.factorVersionId));
});
