import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Study } from '../../research/src/documents.ts';
import { reviewTargetVersion } from '../../research/src/review-identity.ts';
import { ReviewLedger } from '../../research/src/review-documents.ts';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import { freezeReviewedFactors } from '../src/reviewed-factor-admission.ts';
import { admittedReviewedMembers } from '../../research/plugins/native-fitted-portfolio.ts';
import { context, setup, wat } from './support.ts';
import { json } from '../src/facts.ts';

// Controlled source facts verify real Durable authorization, freezing and proxy
// handling. They are not proof of scientific qualification or market performance.
test('historical factor IDs authorize a reachable quality prior across real Durable conversations', { timeout: 30_000 }, async () => {
  const state = await setup();
  const data = { catalog: '/controlled/catalog', request: { schema_version: 1, train_end_ns: '300', validation_end_ns: '400', forecast: {
    schema_version: 1, selection: { event_start_ns: '0', event_end_ns: '500', decision_cutoff_ns: '500', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {} } } };
  try {
    const sources = await state.service.research.harness.commit(async tx => {
      const sources = [];
      for (const trialId of [1, 2]) {
        const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
        const id = conversation.id;
        const study = await tx.doc(Study, id);
        study.input = { goal: 'Controlled historical source', model: { provider: 'fixture', modelId: 'fixture' }, data: {}, policy: {} };
        const trial = { taskId: trialId, status: 'COMPUTED', proposal: { model: wat, hypothesis: `Original ${trialId}` }, result: { request: { validation_end_ns: '200' } } };
        study.trials = [trial];
        const policy = createReviewPolicy('FACTOR', `policy-${id}`, '1000');
        policy.calibration = { rootCount: 30, endNs: '50', evidenceRefs: ['fixture-reference'] };
        const version = reviewTargetVersion(study.input, trial, study.trials, policy);
        const evidenceRef = `research:${id}:trial:${trialId}:validation`;
        const report = scoreReview({ id: `report-${id}`, target: { kind: 'FACTOR', id: `research:${id}:trial:${trialId}`,
          version, sourceEvidenceRefs: [evidenceRef], evaluationStartNs: '100' }, policy,
          metrics: policy.metricRules.map(rule => ({ key: rule.key, value: rule.high, status: 'AVAILABLE', observationCount: 100,
            method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef })),
          checks: policy.requiredChecks.map(id => ({ id, status: 'PASS', reason: 'controlled fixture', evidenceRefs: [evidenceRef] })), createdAt: '2026-11-02T00:00:00Z' });
        assert.equal(report.decision, 'PASS');
        (await tx.doc(ReviewLedger, id)).records[version] = { trialId, version, policyId: policy.id, status: 'COMPLETED', report: json(report) };
        sources.push({ id, factorId: `r${id}.t${trialId}.m0` });
      }
      return sources;
    }, context);
    const ids = sources.map(source => source.factorId);
    // Production admission must use current source/grant authority even when a
    // historical PASS exists. The protocol fixture has no such source evidence.
    await assert.rejects(state.service.research.harness.commit(tx => freezeReviewedFactors(tx, ids, data, state.options.config), context), /reviewed_factor_current_source_required/);
    const admitted = await state.service.research.harness.commit(async tx => {
      const frozen = await freezeReviewedFactors(tx, ids, data);
      const destination = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      (await tx.doc(Study, destination.id)).input = { goal: 'New research', model: { provider: 'fixture', modelId: 'fixture' }, data, policy: { reviewedFactorSources: json(frozen!) } };
      return { frozen, destinationId: destination.id };
    }, context);
    const selected = admittedReviewedMembers(admitted.frozen, ids, '300');
    assert.equal(selected.binding.reason, null);
    assert.equal(selected.members[0]!.sourceFactorId, ids[0]);
    assert.equal((selected.binding.prior!.members as { quality_coefficient: number }[])[0]!.quality_coefficient, 1);
    assert.throws(() => admittedReviewedMembers(admitted.frozen, [ids[0]!, 'not-admitted'], '300'), /not_admitted/);
    await assert.rejects(state.service.research.harness.commit(tx => freezeReviewedFactors(tx, ids,
      { ...data, request: { ...data.request, train_end_ns: '200' } }), context), /not_before_training_cutoff/);
    const tampered = structuredClone(admitted.frozen!); tampered.members[0]!.modelSource = wat.replace('f64.const 1', 'f64.const 2');
    // Explicitly change bytes even if the shared test source has a different body.
    tampered.members[0]!.modelSha256 = '0'.repeat(64);
    assert.throws(() => admittedReviewedMembers(tampered, ids, '300'), /frozen_binding_invalid/);
    await state.restart();
    const original = await state.service.research.harness.snapshot(Study, admitted.destinationId, context);
    assert.deepEqual(admittedReviewedMembers(original!.input!.policy.reviewedFactorSources, ids, '300'), selected);
    await state.service.research.harness.commit(async tx => { (await tx.doc(Study, sources[0]!.id)).deletedFactors = { [ids[0]!]: '2026-11-02T00:00:00Z' }; }, context);
    await assert.rejects(state.service.research.harness.commit(tx => freezeReviewedFactors(tx, ids, data), context), /not_found/);
  } finally { await state.cleanup(); }
});
