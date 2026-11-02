import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import type { ReviewReport } from '../../../packages/contracts/src/review.ts';
import { ReviewLedger } from '../../research/src/review-documents.ts';
import { context, setup, studied } from './support.ts';
import { json } from '../src/facts.ts';

// Controlled stored evidence tests HTTP/Durable provenance and recovery only;
// the empty cold-start report is explicitly INCONCLUSIVE, never native evidence.
test('review reports remain owner-only and preserve identity through projection and restart', { timeout: 30_000 }, async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state, 'review-original');
    const current = await state.service.research.state(receipt.conversationId);
    const trialId = Number(current.research!.trials[0]!.taskId);
    const report = scoreReview({ id: 'fixture:review:original',
      target: { kind: 'FACTOR', id: `research:${receipt.conversationId}:trial:${trialId}`, version: 'fixture:immutable-version',
        sourceEvidenceRefs: ['fixture:validation-evidence'], evaluationStartNs: '100' },
      policy: createReviewPolicy('FACTOR', 'fixture:review-policy', '90'), metrics: [], checks: [],
      createdAt: '2026-11-02T00:00:00.000Z', narrative: 'Controlled protocol fixture; no scientific qualification.' });
    assert.equal(report.decision, 'INCONCLUSIVE');
    await state.service.research.harness.commit(async tx => {
      (await tx.doc(ReviewLedger, receipt.conversationId)).records['fixture:immutable-version'] = {
        trialId, version: report.target.version, status: 'COMPLETED', report: json(report),
      };
    }, context);
    const factorId = `r${receipt.conversationId}.t${trialId}.m0`;
    const paths = [`/api/v3/research/${receipt.conversationId}/reviews`, `/api/v3/factors/${factorId}/review`];
    for (const url of paths) {
      const denied = await state.service.app.inject({ url });
      assert.equal(denied.statusCode, 401, denied.body);
    }
    const read = async () => {
      const research = await state.service.app.inject({ url: paths[0]!, headers: state.headers });
      assert.equal(research.statusCode, 200, research.body);
      assert.deepEqual(research.json<ReviewReport[]>(), [report]);
      const factor = await state.service.app.inject({ url: paths[1]!, headers: state.headers });
      assert.equal(factor.statusCode, 200, factor.body);
      assert.deepEqual(factor.json<ReviewReport>(), report);
      const detail = await state.service.app.inject({ url: `/api/v3/factors/${factorId}`, headers: state.headers });
      assert.equal(detail.statusCode, 200, detail.body);
      assert.equal(detail.json().review.id, report.id);
      assert.equal(detail.json().combinable, false);
      assert.equal(detail.json().reason, 'review_inconclusive');
      const list = await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers });
      assert.equal(list.statusCode, 200, list.body);
      assert.equal(list.json().find((item: { id: string }) => item.id === factorId).review.id, report.id);
    };
    const before = await state.service.research.harness.inspect(context);
    await read(); await read();
    const after = await state.service.research.harness.inspect(context);
    assert.deepEqual(after.tasks.map(task => task.record.id), before.tasks.map(task => task.record.id));
    await state.restart();
    await read();
    const restarted = await state.service.research.state(receipt.conversationId);
    assert.equal(restarted.research!.trials.length, current.research!.trials.length);
    assert.equal(restarted.research!.final!.trialId, current.research!.final!.trialId);
  } finally { await state.cleanup(); }
});

test('new admission executes an isolated reviewer and persists its gate before any independent computation', { timeout: 30_000 }, async () => {
  const { Type } = await import('@earendil-works/pi-ai');
  const { defineExtension, defineTool } = await import('@earendil-works/pi-durable');
  const { evaluateTrial } = await import('../../research/src/trials.ts');
  const { finalizeFactor } = await import('../../research/plugins/final-evaluation.ts');
  const { ReviewSession } = await import('../../research/src/review-documents.ts');
  const { fauxAssistantMessage, fauxToolCall, wat } = await import('./support.ts');
  const state = await setup();
  try {
    state.options.extensions = [defineExtension({ name: 'test.new-review-pipeline', tools: [defineTool({
      name: 'review_candidate', description: 'Controlled candidate for review orchestration only, no native or market evidence.',
      parameters: Type.Object({}), replay: 'safe',
      async execute(_args, api, ctx) {
        const input = (await api.snapshot((await import('../../research/src/documents.ts')).Study, api.conversationId, ctx))!.input!;
        await evaluateTrial(api, ctx, { model: wat, hypothesis: 'Controlled protocol review candidate' }, async () => json({
          request: (await import('../../research/plugins/native-wasm.ts')).iterationRequest(input.data), partitions: [
            { partition: 'VALIDATION', metric: { counts: { included: '2' }, pearson_ic: { status: 'CONSTANT_SCORE', value: null } } },
            { partition: 'HELDOUT', privateSentinel: 'NEVER_REVIEW_HELDOUT_PAYLOAD' },
          ],
        }));
        await finalizeFactor({ trialId: api.taskId, reason: 'Controlled attempt at independent evaluation.' }, api, ctx);
        return { content: [{ type: 'text', text: 'Original review gate settled.' }], control: { terminate: true } };
      },
    })] })];
    state.options.config.datasets[0]!.data.request = { train_end_ns: '10', validation_end_ns: '20', forecast: {
      schema_version: 1, selection: { bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '40' }, parameters: {} } };
    state.options.config.datasets[0]!.data.sealed = structuredClone(state.options.config.datasets[0]!.data);
    await state.restart();
    const call = (name: string, args: import('@earendil-works/pi-durable').JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
    state.faux.setResponses([call('review_candidate', {}),
      ...['context', 'provenance', 'factor', 'selection', 'stress', 'portfolio'].map(kind => call(`review_${kind}`, {})),
      async () => {
        const page = await state.service.research.list();
        let source: string | undefined;
        for (const row of page.items) {
          const original = await state.service.research.state(row.id);
          if (original.research?.input && original.research.trials[0]) source = `research:${row.id}:trial:${original.research.trials[0].taskId}:validation`;
        }
        assert.ok(source);
        return call('review_finish', { narrative: 'The controlled candidate lacks historical source and statistical evidence.', findings: [],
          mechanism: ['mechanism', 'availability', 'direction', 'implementation', 'complexity'].map(criterion => ({ criterion, supported: false, evidenceRefs: [source!] })) });
      },
    ]);
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'actual-review-admission', goal: 'Controlled review pipeline, not market evidence.', model: state.model, datasetId: 'controlled', maxTrials: 1 } });
    assert.equal(response.statusCode, 202, response.body);
    const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const current = await state.service.research.state(receipt.conversationId);
    assert.ok(current.research!.input!.policy.reviewPolicies);
    assert.equal(current.research!.final?.status, 'STOPPED', JSON.stringify(await state.service.research.view(receipt.conversationId)));
    assert.equal(current.research!.final!.reason, 'independent_review_not_passed');
    assert.equal(current.research!.final!.result, null);
    const stored = Object.values(current.reviews!.records);
    assert.equal(stored.length, 1);
    const report = stored[0]!.report as unknown as ReviewReport;
    assert.equal(report.decision, 'INCONCLUSIVE'); assert.equal(report.qualityCoefficient, 0);
    // Consume the report actually produced by the new-admission workflow, through
    // the same cross-study admission transaction used by the service. Its version
    // must verify; its incomplete evidence must still prevent a quality prior.
    const { freezeReviewedFactors } = await import('../src/reviewed-factor-admission.ts');
    const importedId = `r${receipt.conversationId}.t${current.research!.trials[0]!.taskId}.m0`;
    await assert.rejects(state.service.research.harness.commit(tx => freezeReviewedFactors(tx,
      [importedId, 'r999999.t1.m0'], { request: { train_end_ns: '300', validation_end_ns: '400', forecast: {
        selection: { event_start_ns: '0', event_end_ns: '500', decision_cutoff_ns: '500' }, parameters: {} } } }), context), /reviewed_factor_pass_report_required/);
    assert.equal(report.target.id, `research:${receipt.conversationId}:trial:${current.research!.trials[0]!.taskId}`);
    const tasks = await state.service.research.harness.commit(tx => tx.scanTasks({ kind: 'qz.review.independent' }, 20), context);
    assert.equal(tasks.items.length, 1);
    const review = tasks.items[0]!;
    assert.equal(review.state.status, 'terminal');
    assert.equal(review.state.outcome!.status, 'completed');
    if (review.state.outcome!.status !== 'completed') return;
    const reviewResult = review.state.outcome!.result;
    assert.ok(reviewResult && typeof reviewResult === 'object' && !Array.isArray(reviewResult));
    assert.equal(typeof reviewResult.reviewerConversationId, 'number');
    const reviewerId = reviewResult.reviewerConversationId as import('@earendil-works/pi-durable').ConversationId;
    assert.notEqual(reviewerId, receipt.conversationId);
    const session = await state.service.research.harness.snapshot(ReviewSession, reviewerId, context);
    assert.equal(session!.finished, true); assert.equal(session!.calls, 7);
    assert.equal(JSON.stringify(session!.evidence).includes('NEVER_REVIEW_HELDOUT_PAYLOAD'), false);
    assert.equal((await state.service.research.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 20), context)).items.length, 0);
    const apiReport = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}/reviews`, headers: state.headers });
    assert.equal(apiReport.statusCode, 200); assert.deepEqual(apiReport.json(), [report]);
    const modelCalls = state.faux.state.callCount; assert.equal(modelCalls, 8);
    await state.restart();
    assert.equal((await state.service.research.state(receipt.conversationId)).research!.final!.reviewReportId, report.id);
    assert.equal(state.faux.state.callCount, modelCalls);
  } finally { await state.cleanup(); }
});
