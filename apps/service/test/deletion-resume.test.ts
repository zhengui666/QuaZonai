import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { test } from 'node:test';
import { awaitWithContext } from '@earendil-works/chord/context';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { Type } from '@earendil-works/pi-ai';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import type { PmMandateView } from '../../../packages/contracts/src/pm.ts';
import research from '../../research/plugins/research.ts';
import { Study, Requests } from '../../research/src/documents.ts';
import { CampaignResearchDoc } from '../../research/src/pm-campaign.ts';
import { registerHypothesis } from '../../research/src/pm-ledger.ts';
import { recoveryRequestId } from '../../research/src/recovery.ts';
import { evaluateTrial } from '../../research/src/trials.ts';
import { PmDocument, pmRecord, pmState } from '../src/pm-facts.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

// Real owner HTTP, official Durable/SQLite and PM retirement. The probe records
// only an explicit TEST_ONLY trial; no market/scientific success is asserted.
async function stoppedPmResearch() {
  const state = await setup();
  let entered!: () => void; const enteredTool = new Promise<void>(resolve => { entered = resolve; });
  const probe = defineExtension({ name: 'test.deleted-pm-resume', tools: [defineTool({
    name: 'deleted_pm_resume_probe', description: 'TEST_ONLY original trial and cancellation ownership', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) {
      const hypothesis = await registerHypothesis(api, ctx, { economicFamilyId: 'test:deleted-pm-resume', mechanism: 'Exercise parent deletion after owner stop.',
        prediction: 'Removed parent cannot authorize a resumed child.', falsification: 'A new model turn appears after parent deletion.', requiredDataCapabilities: [], incrementalQuestion: 'Is the original stop retained?' });
      await evaluateTrial(api, ctx, { hypothesisId: hypothesis.id, hypothesis: 'TEST_ONLY control observation.' }, async () => ({ protocolOnly: true }));
      entered(); await awaitWithContext(new Promise<void>(() => {}), ctx);
      return { content: [{ type: 'text', text: 'Unreachable after owner stop.' }] };
    },
  })] });
  try {
    state.options.extensions = [research, probe];
    state.options.config.datasets[0]!.data.request = { train_end_ns: '140', validation_end_ns: '180', forecast: {
      schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {},
    } };
    await state.restart();
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('strategy_intake', { name: 'Deleted-parent recovery protocol', archetype: 'PREDICTIVE_ALPHA',
        economicMechanism: 'Test original task ownership only.', falsificationConditions: ['A removed parent starts new model work.'],
        requiredCapabilities: [], intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['TEST_ONLY'], ambiguities: [], ownerBoundaries: [] }), { stopReason: 'toolUse' }),
      fauxAssistantMessage(fauxToolCall('deleted_pm_resume_probe', {}), { stopReason: 'toolUse' }),
    ]);
    const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { requestId: 'deleted-parent-original', goal: 'Control-only recovery of an original PM child', datasetId: 'controlled', model: state.model, maxTrials: 3,
        funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '1000', currency: 'USD' } } } });
    assert.equal(start.statusCode, 202, start.body); await enteredTool;
    const id = start.json<PmMandateView>().mandate.id;
    const path = `/api/v3/mandates/${encodeURIComponent(id)}`;
    const current = await state.service.app.inject({ url: path, headers: state.headers });
    assert.equal(current.statusCode, 200, current.body); const view = current.json<PmMandateView>();
    const campaignId = view.campaigns[0]!.id;
    const conversationId = view.campaigns[0]!.runtime.conversationId as ConversationId;
    assert.notEqual(conversationId, pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), id).conversationId);
    const retired = await state.service.app.inject({ method: 'POST', url: `${path}/retire`, headers: state.headers,
      payload: { requestId: 'original-retire-before-delete', expectedRevision: view.mandate.revision } });
    assert.equal(retired.statusCode, 202, retired.body);
    const pm = pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), id);
    await state.service.research.harness.waitForTask(pm.artifacts[`campaign-control:${campaignId}`]!.taskId as TaskId, context);
    const stopped = await state.service.research.state(conversationId);
    assert.equal(stopped.research!.final!.status, 'STOPPED'); assert.equal(stopped.research!.final!.reason, 'owner_stop');
    assert.equal(stopped.research!.final!.trialId, null); assert.equal(stopped.research!.final!.result, null);
    assert.equal(stopped.submission!.status, 'unanswered'); assert.equal(stopped.submission!.reason, 'aborted');
    assert.equal(stopped.research!.trials.length, 1); assert.ok(stopped.research!.trials.length < Number(stopped.research!.input!.policy.max_trials));
    return { state, id, path, conversationId, submissionId: stopped.submission!.id };
  } catch (error) { await state.cleanup(); throw error; }
}
async function deleteParent(f: Awaited<ReturnType<typeof stoppedPmResearch>>) {
  const deadline = Date.now() + 8000;
  for (;;) {
    const response = await f.state.service.app.inject({ method: 'DELETE', url: f.path, headers: f.state.headers });
    if (response.statusCode !== 409 || response.json().error !== 'pm_delete_tasks_active') return response;
    assert.ok(Date.now() < deadline, response.body); await delay(15);
  }
}

test('deleted PM mandate rejects direct child resume without a new submission, model call or cleared stop, including after restart', { timeout: 30_000 }, async () => {
  const f = await stoppedPmResearch();
  try {
    const removed = await deleteParent(f); assert.equal(removed.statusCode, 200, removed.body);
    const before = structuredClone((await f.state.service.research.harness.snapshot(Study, f.conversationId, context))!);
    const originalCampaign = structuredClone((await f.state.service.research.harness.snapshot(CampaignResearchDoc, f.conversationId, context))!);
    const requests = structuredClone((await f.state.service.research.harness.snapshot(Requests, context))!);
    const calls = f.state.faux.state.callCount;
    const requestId = recoveryRequestId(f.conversationId, f.submissionId);
    for (const restart of [false, true]) {
      if (restart) await f.state.restart();
      const response = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/research/${f.conversationId}/resume`, headers: f.state.headers,
        payload: { submissionId: f.submissionId } });
      assert.equal(response.statusCode, 409, response.body); assert.equal(response.json().error, 'mandate_deleted');
      assert.equal(await f.state.service.research.harness.commit(tx => tx.submissionByRequest(f.conversationId, requestId), context), undefined);
      assert.deepEqual((await f.state.service.research.state(f.conversationId)).submission!.id, f.submissionId);
      assert.deepEqual(await f.state.service.research.harness.snapshot(Study, f.conversationId, context), before);
      assert.deepEqual(await f.state.service.research.harness.snapshot(CampaignResearchDoc, f.conversationId, context), originalCampaign);
      assert.deepEqual(await f.state.service.research.harness.snapshot(Requests, context), requests);
      assert.equal(f.state.faux.state.callCount, calls);
      assert.equal(pmRecord(pmState(await f.state.service.research.harness.snapshot(PmDocument, context)), f.id).deletedAt, removed.json().deletedAt);
    }
  } finally { await f.state.cleanup(); }
});

// Pause only the public Conversation.submit handoff after the real recovery
// transaction; all admission and execution still use official Durable methods.
test('PM deletion serializes with child resume through its final submit handoff', { timeout: 30_000 }, async t => {
  const f = await stoppedPmResearch();
  let reached!: () => void; const submitting = new Promise<void>(resolve => { reached = resolve; });
  let release!: () => void; const released = new Promise<void>(resolve => { release = resolve; });
  const host = f.state.service.research;
  const original = host.harness.conversation.bind(host.harness);
  try {
    f.state.faux.setResponses([fauxAssistantMessage(fauxToolCall('deleted_pm_resume_probe', {}), { stopReason: 'toolUse' })]);
    const recoveryId = recoveryRequestId(f.conversationId, f.submissionId);
    const interception = t.mock.method(host.harness, 'conversation', async (...args: Parameters<typeof original>) => {
      const handle = await original(...args);
      if (!handle || handle.id !== f.conversationId) return handle;
      return new Proxy(handle, { get(target, property, receiver) {
        if (property === 'submit') return async (...submitArgs: Parameters<typeof target.submit>) => {
          if (submitArgs[0].requestId === recoveryId) { reached(); await released; }
          return target.submit(...submitArgs);
        };
        const value = Reflect.get(target, property, receiver); return typeof value === 'function' ? value.bind(target) : value;
      } });
    });
    const resuming = f.state.service.app.inject({ method: 'POST', url: `/api/v3/research/${f.conversationId}/resume`, headers: f.state.headers,
      payload: { submissionId: f.submissionId } }).then(response => response);
    await submitting;
    const checkpoint = (await host.harness.snapshot(Study, f.conversationId, context))!;
    assert.equal(checkpoint.final, null, 'the original recovery transaction has reached the submit handoff');
    const deletion = f.state.service.app.inject({ method: 'DELETE', url: f.path, headers: f.state.headers }).then(response => response);
    const early = await Promise.race([deletion.then(() => 'deleted'), delay(50).then(() => 'waiting')]);
    assert.equal(early, 'waiting', 'parent deletion must wait for the child owner operation including its submit');
    assert.equal(pmRecord(pmState(await host.harness.snapshot(PmDocument, context)), f.id).deletedAt, undefined);
    release();
    const resumed = await resuming; assert.equal(resumed.statusCode, 202, resumed.body);
    assert.notEqual(resumed.json().submissionId, f.submissionId);
    const blocked = await deletion; assert.equal(blocked.statusCode, 409, blocked.body); assert.equal(blocked.json().error, 'pm_delete_tasks_active');
    assert.equal(pmRecord(pmState(await host.harness.snapshot(PmDocument, context)), f.id).deletedAt, undefined);
    interception.mock.restore();
    // Stop the actually admitted resumed input before cleanup; deletion never
    // pretends that it cancelled this independently running child.
    await host.stop(f.conversationId);
  } finally { release(); t.mock.restoreAll(); await f.state.cleanup(); }
});
