import assert from 'node:assert/strict';
import { test } from 'node:test';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { Type } from '@earendil-works/pi-ai';
import { evaluateTrial } from '../../research/src/trials.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';
import research from '../../research/plugins/research.ts';

const probe = defineExtension({ name: 'test.recovery-probe', tools: [defineTool({
  name: 'recovery_probe', description: 'Test-only ledger observation', parameters: Type.Object({ fail: Type.Boolean() }), replay: 'safe',
  async execute(args, api, ctx) {
    await evaluateTrial(api, ctx, { hypothesis: 'Original recovery protocol', fail: args.fail }, async () => {
      if (args.fail) throw new Error('original_scientific_failure');
      return { nativeProtocolOnly: true };
    });
    return { content: [{ type: 'text', text: 'Recorded protocol trial' }] };
  },
})] });

test('original model failure has safe diagnostics and one replayable same-conversation recovery', { timeout: 20_000 }, async () => {
  const state = await setup(); state.options.extensions.push(probe);
  try {
    await state.restart();
    const privateMarker = '/private/test-only/provider-secret';
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('recovery_probe', { fail: true }), { stopReason: 'toolUse' }),
      fauxAssistantMessage('', { stopReason: 'error', errorMessage: `400 invalid request: unsupported response format ${privateMarker}` }),
    ]);
    const original = { requestId: 'recover-original', goal: 'Keep original ledger and budget', model: state.model, datasetId: 'controlled', maxTrials: 3 };
    const admission = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: original });
    assert.equal(admission.statusCode, 202, admission.body); const receipt = admission.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const studyBefore = (await state.service.research.view(receipt.conversationId)).research!;
    assert.equal(studyBefore.trials.length, 1); assert.equal(studyBefore.trials[0]!.status, 'FAILED');
    const diagnostics = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}/diagnostics`, headers: state.headers });
    assert.equal(diagnostics.statusCode, 200, diagnostics.body);
    assert.equal(diagnostics.json().submission.id, receipt.submissionId);
    assert.equal(diagnostics.json().submission.reason, 'model_error');
    assert.equal(diagnostics.json().generation.error, 'model_request_rejected');
    assert.ok(!diagnostics.body.includes(privateMarker));
    const url = `/api/v3/research/${receipt.conversationId}/resume`; const payload = { submissionId: receipt.submissionId };
    assert.equal((await state.service.app.inject({ method: 'POST', url, payload })).statusCode, 401);
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' }),
    ]);
    const [first, replay] = await Promise.all([0, 1].map(() => state.service.app.inject({ method: 'POST', url, headers: state.headers, payload })));
    assert.equal(first.statusCode, 202, first.body); assert.equal(replay.statusCode, 202, replay.body); assert.deepEqual(first.json(), replay.json());
    const recovered = first.json(); assert.equal(recovered.conversationId, receipt.conversationId); assert.notEqual(recovered.submissionId, receipt.submissionId); assert.equal(recovered.previousSubmissionId, receipt.submissionId);
    await (await state.service.research.harness.submission(recovered.submissionId, context))!.wait(context);
    assert.equal((await state.service.research.submission(receipt.submissionId)).status, 'unanswered');
    const detail = (await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json();
    assert.equal(detail.submission.id, recovered.submissionId); assert.equal(detail.originalSubmission.id, receipt.submissionId);
    assert.equal(detail.trials, 2); assert.equal(detail.input.policy.max_trials, 3); assert.deepEqual(detail.input, studyBefore.input);
    const stored = (await state.service.research.view(receipt.conversationId)).research!; assert.deepEqual(stored.trials[0], studyBefore.trials[0]);
    const failureTask = diagnostics.json().generation.taskId; const oldTask = await state.service.research.harness.getTask(failureTask, context); assert.equal(oldTask!.state.status, 'terminal');
    assert.equal((await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: { submissionId: recovered.submissionId } })).statusCode, 409);
    await state.restart();
    const again = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload }); assert.equal(again.statusCode, 202, again.body); assert.deepEqual(again.json(), recovered);
    const originalReplay = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: original }); assert.deepEqual(originalReplay.json(), receipt);
    assert.equal(state.faux.state.callCount, 3);
  } finally { await state.cleanup(); }
});

test('state projections do not construct full transcript watches', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    state.faux.setResponses([fauxAssistantMessage('', { stopReason: 'error', errorMessage: 'insufficient_quota billing quota exhausted test-only' })]);
    const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: 'no-transcript-read', goal: 'Unavailable model', model: state.model, datasetId: 'controlled', maxTrials: 2 } });
    const receipt = start.json(); await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    state.service.research.view = async () => { throw new Error('full_transcript_watch_must_not_be_read'); };
    const detail = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers }); assert.equal(detail.statusCode, 200, detail.body); assert.equal(detail.json().submission.reason, 'model_error');
    const diagnostics = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}/diagnostics`, headers: state.headers }); assert.equal(diagnostics.statusCode, 200, diagnostics.body); assert.equal(diagnostics.json().generation.error, 'model_quota_exhausted');
    const listed = await state.service.app.inject({ url: '/api/v3/research', headers: state.headers }); assert.equal(listed.statusCode, 200, listed.body);
  } finally { await state.cleanup(); }
});

test('resume refuses exhausted budgets, stopped studies and wrong original submissions', { timeout: 20_000 }, async () => {
  const state = await setup(); state.options.extensions.push(probe);
  try {
    await state.restart();
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('recovery_probe', { fail: true }), { stopReason: 'toolUse' }),
      fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 unsupported response' }),
    ]);
    const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: 'exhausted-model-failure', goal: 'Original one attempt', model: state.model, datasetId: 'controlled', maxTrials: 1 } });
    const receipt = start.json(); await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const url = `/api/v3/research/${receipt.conversationId}/resume`;
    const wrong = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: { submissionId: receipt.submissionId + 1 } }); assert.equal(wrong.statusCode, 409);
    const exhausted = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: { submissionId: receipt.submissionId } }); assert.equal(exhausted.statusCode, 409); assert.equal(exhausted.json().error, 'trial_budget_exhausted');
    await state.service.research.stop(receipt.conversationId);
    const stopped = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: { submissionId: receipt.submissionId } }); assert.equal(stopped.statusCode, 409); assert.equal(stopped.json().error, 'research_already_final');
    const invalid = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: { submissionId: receipt.submissionId, maxTrials: 10 } }); assert.equal(invalid.statusCode, 400);
    assert.equal(state.faux.state.callCount, 2); assert.equal((await state.service.research.state(receipt.conversationId)).research!.trials.length, 1);
  } finally { await state.cleanup(); }
});

test('owner reads retain the complete prior unselected stop after same-study continuation', { timeout: 20_000 }, async () => {
  const state = await setup(); state.options.extensions.push(probe, research);
  try {
    await state.restart();
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('recovery_probe', { fail: false }), { stopReason: 'toolUse' }),
      fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Finite invocation, original objective still unmet' }), { stopReason: 'toolUse' }),
    ]);
    const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: 'continue-original-stop', goal: 'Original multi-generation research', model: state.model, datasetId: 'controlled', maxTrials: 3 } });
    const receipt = start.json(); assert.equal(start.statusCode, 202, start.body);
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const before = (await state.service.research.state(receipt.conversationId)).research!;
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
    const resumed = await state.service.app.inject({ method: 'POST', url: `/api/v3/research/${receipt.conversationId}/resume`, headers: state.headers, payload: { submissionId: receipt.submissionId } });
    assert.equal(resumed.statusCode, 202, resumed.body);
    const next = resumed.json(); await (await state.service.research.harness.submission(next.submissionId, context))!.wait(context);
    const detail = (await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json();
    assert.deepEqual(detail.previousStops, [{ previousSubmissionId: receipt.submissionId, final: before.final }]);
    assert.equal(detail.originalSubmission.id, receipt.submissionId); assert.equal(detail.submission.id, next.submissionId);
    assert.equal(detail.trials, 2); assert.deepEqual(detail.input, before.input); assert.equal(detail.datasetId, 'controlled');
    const admissions = (await state.service.work.snapshot())!.researchRequests;
    const collision = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: `qz.research.resume:${receipt.conversationId}:${receipt.submissionId}`, input: detail.input } });
    assert.equal(collision.statusCode, 409, collision.body); assert.deepEqual((await state.service.work.snapshot())!.researchRequests, admissions);
    const after = (await state.service.research.state(receipt.conversationId)).research!; assert.deepEqual(after.trials[0], before.trials[0]);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json().previousStops, detail.previousStops);
    assert.equal(state.faux.state.callCount, 3);
  } finally { await state.cleanup(); }
});

test('legacy model-error continuation receipt replays without clearing a later stop', { timeout: 20_000 }, async () => {
  const state = await setup(); state.options.extensions.push(research);
  try {
    await state.restart();
    state.faux.setResponses([fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 unsupported test-only' })]);
    const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: 'legacy-recovery', goal: 'Original legacy recovery', model: state.model, datasetId: 'controlled', maxTrials: 3 } });
    const receipt = start.json(); await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Recorded later stop' }), { stopReason: 'toolUse' })]);
    const next = await state.service.research.resume(receipt.conversationId, receipt.submissionId);
    await (await state.service.research.harness.submission(next.submissionId, context))!.wait(context);
    // PR210 native receipts predate additive continuation-admission metadata.
    const { Requests } = await import('../../research/src/documents.ts');
    await state.service.research.harness.commit(async tx => { delete (await tx.doc(Requests)).requests[`request:qz.research.resume:${receipt.conversationId}:${receipt.submissionId}`]; }, context);
    const before = (await state.service.research.state(receipt.conversationId)).research;
    await state.restart();
    assert.deepEqual(await state.service.research.resume(receipt.conversationId, receipt.submissionId), next);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research, before);
    const recoveryKey = `qz.research.resume:${receipt.conversationId}:${receipt.submissionId}`;
    const admissions = (await state.service.work.snapshot())!.researchRequests;
    const direct = await state.service.research.start(recoveryKey, before!.input!).then(() => 'admitted_duplicate', error => error.message);
    const collision = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: recoveryKey, input: before!.input } });
    assert.equal(direct, 'request_id_conflict'); assert.equal(collision.statusCode, 409, collision.body);
    assert.deepEqual((await state.service.work.snapshot())!.researchRequests, admissions);
    assert.equal((await state.service.research.list()).items.length, 1);
    assert.equal(state.faux.state.callCount, 2);
  } finally { await state.cleanup(); }
});

test('upgrade repairs an original application admission using a legacy recovery-shaped owner key', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const requestId = 'qz.research.resume:9000:9001';
    const original = { requestId, goal: 'Previously admitted owner request', model: state.model, datasetId: 'controlled', maxTrials: 1 };
    const input = { goal: original.goal, model: state.model, data: state.options.config.datasets[0]!.data, policy: { max_trials: 1 } };
    const { Application } = await import('../src/facts.ts');
    // Original accepted Application→Requests gap from before namespace reservation.
    await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).researchRequests[`request:${requestId}`] = { original, input }; }, context);
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
    await state.restart();
    const replay = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: original });
    assert.equal(replay.statusCode, 202, replay.body); const receipt = replay.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const current = await state.service.research.state(receipt.conversationId);
    assert.deepEqual(current.research!.input, input); assert.equal(current.research!.trials.length, 1);
    assert.equal(current.originalSubmission!.requestId, requestId); assert.equal((await state.service.research.list()).items.length, 1);
    const fresh = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...original, requestId: 'qz.research.resume:9002:9003' } });
    assert.equal(fresh.statusCode, 409); assert.equal(state.faux.state.callCount, 1);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: original })).json(), receipt);
    assert.equal(state.faux.state.callCount, 1);
  } finally { await state.cleanup(); }
});
