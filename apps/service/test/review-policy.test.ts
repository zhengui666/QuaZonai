import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import type { ReviewPolicy } from '../../../packages/contracts/src/review.ts';
import { Study, Requests } from '../../research/src/documents.ts';
import { ReviewLedger } from '../../research/src/review-documents.ts';
import { ReviewPolicies, resolveRegisteredReviewPolicy } from '../src/review-policy.ts';
import { json } from '../src/facts.ts';
import { context, setup, fauxAssistantMessage } from './support.ts';

// Controlled retained reports test authority and provenance through real HTTP and
// Durable storage. They are deliberately not native or market evidence.
test('HTTP calibration resolves retained reports and never accepts caller-authored calibration references', { timeout: 30_000 }, async () => {
  const state = await setup();
  try {
    const forged = createReviewPolicy('FACTOR', 'caller-forged-calibration', '1000');
    forged.calibration = { rootCount: 30, endNs: '90', evidenceRefs: ['caller:invented-reference'] };
    const input = { goal: 'Calibration authority protocol only.', model: state.model, data: state.options.config.datasets[0]!.data,
      policy: { max_trials: 1, reviewPolicy: forged } };
    const post = (path: string, payload: object, authenticated = true) => state.service.app.inject({ method: 'POST', url: `/api/v3${path}`, ...(authenticated ? { headers: state.headers } : {}), payload });
    const blocked = await post('/research', { requestId: 'forged-profile', input });
    assert.equal(blocked.statusCode, 422, blocked.body); assert.equal(blocked.json().error, 'review_policy_not_registered');
    assert.equal(state.faux.state.callCount, 0);
    assert.equal((await state.service.research.harness.snapshot(Requests, context))?.requests['request:forged-profile'], undefined);
    assert.deepEqual((await state.service.research.harness.snapshot(ReviewPolicies, context))?.records ?? {}, {});
    const denied = await post('/review-policies', { requestId: 'no-owner', kind: 'FACTOR', researchIds: Array.from({ length: 30 }, (_, index) => index), evaluationStartNs: '101' }, false);
    assert.equal(denied.statusCode, 401, denied.body);
    const extra = await post('/review-policies', { requestId: 'extra-claims', kind: 'FACTOR', researchIds: Array.from({ length: 30 }, (_, index) => index), evaluationStartNs: '101', calibration: forged.calibration });
    assert.equal(extra.statusCode, 400, extra.body); assert.equal(extra.json().error, 'review_policy_unknown_input');
    const absent = await post('/review-policies', { requestId: 'absent-roots', kind: 'FACTOR', researchIds: Array.from({ length: 30 }, (_, index) => index + 100000), evaluationStartNs: '101' });
    assert.equal(absent.statusCode, 404, absent.body);
    const roots = await state.service.research.harness.commit(async tx => {
      const ids = [];
      for (let index = 0; index < 30; index++) {
        const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } }); const trialId = index + 10000;
        const study = await tx.doc(Study, conversation.id);
        study.input = { goal: 'Controlled retained reference only', model: state.model, data: {}, policy: {} };
        study.trials = [{ taskId: trialId, status: 'COMPUTED', proposal: { hypothesis: 'Controlled reference' }, result: { request: { validation_end_ns: '80' } } }];
        const policy = createReviewPolicy('FACTOR', `fixture-reference-policy:${index}`, '1000');
        const evidenceRef = `research:${conversation.id}:trial:${trialId}:validation`;
        const report = scoreReview({ id: `fixture-reference-report:${index}`, target: { kind: 'FACTOR', id: `research:${conversation.id}:trial:${trialId}`,
          version: `fixture-reference-version:${index}`, sourceEvidenceRefs: [evidenceRef], evaluationStartNs: '10' }, policy,
          metrics: policy.metricRules.map(rule => ({ key: rule.key, value: rule.low + (rule.high - rule.low) * (index + 1) / 31,
            status: 'AVAILABLE', observationCount: 100, scope: rule.scope, method: rule.methods[0]!, unit: rule.unit, frequency: rule.frequency, evidenceRef })),
          checks: [], createdAt: '2026-11-02T00:00:00Z' });
        assert.notEqual(report.decision, 'PASS', 'reference collection retains unsuccessful evidence');
        (await tx.doc(ReviewLedger, conversation.id)).records[report.target.version] = { trialId, version: report.target.version, policyId: policy.id, status: 'COMPLETED', report: json(report) };
        ids.push(conversation.id);
      }
      return ids;
    }, context);
    const body = { requestId: 'from-retained-roots', kind: 'FACTOR', researchIds: roots, evaluationStartNs: '101' };
    const created = await post('/review-policies', body); assert.equal(created.statusCode, 201, created.body);
    const registered = created.json<ReviewPolicy>();
    assert.equal(registered.calibration!.rootCount, 30); assert.equal(registered.calibration!.endNs, '80');
    assert.equal(registered.calibration!.evidenceRefs.length, 30);
    assert.ok(registered.calibration!.evidenceRefs.every(ref => ref.endsWith(':validation')));
    assert.deepEqual(await resolveRegisteredReviewPolicy(state.service.research.harness, registered), registered);
    const replay = await post('/review-policies', body); assert.equal(replay.statusCode, 200, replay.body); assert.deepEqual(replay.json(), registered);
    const conflict = await post('/review-policies', { ...body, evaluationStartNs: '102' }); assert.equal(conflict.statusCode, 409, conflict.body);
    const altered = structuredClone(registered); altered.metricRules[0]!.high += 0.001;
    const mismatch = await post('/research', { requestId: 'registered-id-changed-body', input: { ...input, policy: { ...input.policy, reviewPolicy: altered } } });
    assert.equal(mismatch.statusCode, 422, mismatch.body); assert.equal(mismatch.json().error, 'review_policy_not_registered');
    state.faux.setResponses([fauxAssistantMessage('Controlled registered-profile admission, no scientific result.')]);
    const goodInput = { ...input, policy: { ...input.policy, reviewPolicy: registered } };
    const accepted = await post('/research', { requestId: 'registered-profile-original', input: goodInput });
    assert.equal(accepted.statusCode, 202, accepted.body); const receipt = accepted.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input!.policy.reviewPolicy, registered);
    const latestRoot = roots.at(-1)!;
    await state.service.research.harness.commit(async tx => {
      const ledger = await tx.doc(ReviewLedger, latestRoot); const record = Object.values(ledger.records)[0]!;
      const report = record.report as unknown as import('../../../packages/contracts/src/review.ts').ReviewReport;
      report.metrics[0]!.evidenceRef = 'source:heldout-forbidden'; record.report = json(report);
    }, context);
    const sealed = await post('/review-policies', { ...body, requestId: 'cannot-relabel-heldout' });
    assert.equal(sealed.statusCode, 422, sealed.body); assert.equal(sealed.json().error, 'review_reference_independent_evidence_forbidden');
    await state.restart();
    assert.deepEqual(await resolveRegisteredReviewPolicy(state.service.research.harness, registered), registered);
    const repeated = await post('/research', { requestId: 'registered-profile-original', input: goodInput });
    assert.deepEqual(repeated.json(), receipt); assert.equal(state.faux.state.callCount, 1);
  } finally { await state.cleanup(); }
});
