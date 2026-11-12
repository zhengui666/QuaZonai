import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { test } from 'node:test';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import { Study, Requests } from '../../research/src/documents.ts';
import { CampaignResearchDoc } from '../../research/src/pm-campaign.ts';
import { recoveryRequestId } from '../../research/src/recovery.ts';
import { fact, PmDocument, pmRecord, pmState, type PmRecord } from '../src/pm-facts.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

// The child is a real public research conversation. Only the existing parent
// ownership record is initialized directly; no removed research fitting runs.
async function stoppedPmResearch() {
  const state = await setup();
  try {
    let entered!: () => void; const generating = new Promise<void>(resolve => { entered = resolve; });
    state.faux.setResponses([async (_transcript, options) => {
      entered();
      await new Promise<void>(resolve => { if (options?.signal?.aborted) resolve(); else options?.signal?.addEventListener('abort', () => resolve(), { once: true }); });
      return fauxAssistantMessage('Original owner stop');
    }]);
    const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'existing-child', goal: 'Test original recovery ownership', datasetId: 'controlled', model: state.model } });
    assert.equal(start.statusCode, 202, start.body);
    const receipt = start.json(); const conversationId = receipt.conversationId as ConversationId;
    await generating;
    await state.service.research.stop(conversationId);
    const id = 'existing-parent'; const path = `/api/v3/mandates/${id}`;
    await state.service.research.harness.commit(async tx => {
      const parent = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      pmState(await tx.doc(PmDocument)).mandates[id] = {
        input: { datasetId: 'controlled' }, conversationId: parent.id,
        mandate: { ...fact(id), capitalSource: { kind: 'RESEARCH_SCENARIO', capital: { amount: '1000', currency: 'USD' } } },
        campaigns: { original: { id: 'original', state: 'CONCLUDED', runtime: { conversationId, taskId: null }, datasetRevisionIds: [] } },
        retirements: { original: { strategyVersionId: 'original', reconciliationRequired: false } },
        latestStrategyId: 'original', commands: {}, decisions: {}, artifacts: {}, evidence: {}, monitorTaskId: null,
        outcomes: {}, attributions: {}, cycles: {}, comparisons: {}, capacities: {}, ensembles: {}, gaps: {}, events: {}, strategies: {}, assessments: {},
        datasets: [], derivedTrials: {}, budgetMovements: {}, searchFamily: {}, currentDecisionId: null, latestCycleId: null,
      } as unknown as PmRecord;
      // Restore the original parent ownership link without invoking campaign tools.
      (await tx.doc(Study, conversationId)).input!.campaign = { mandateRevisionId: id } as never;
    }, context);
    const stopped = await state.service.research.state(conversationId);
    assert.equal(stopped.research!.final!.status, 'STOPPED'); assert.equal(stopped.research!.final!.reason, 'owner_stop');
    assert.equal(stopped.research!.final!.trialId, null); assert.equal(stopped.research!.final!.result, null);
    assert.equal(stopped.submission!.status, 'unanswered');
    assert.equal(stopped.research!.trials.length, 0);
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
  let finishModel!: () => void; const modelPending = new Promise<void>(resolve => { finishModel = resolve; });
  const host = f.state.service.research;
  const original = host.harness.conversation.bind(host.harness);
  try {
    f.state.faux.setResponses([async () => { await modelPending; return fauxAssistantMessage('', { stopReason: 'error', errorMessage: 'test-only ended provider' }); }]);
    const recoveryId = recoveryRequestId(f.conversationId, f.submissionId);
    const interception = t.mock.method(host.harness, 'conversation', async (...args: Parameters<typeof original>) => {
      const handle = await original(...args);
      if (!handle || handle.id !== f.conversationId) return handle;
      return new Proxy(handle, { get(target, property, receiver) {
        if (property === 'submit') return async (...submitArgs: Parameters<typeof target.submit>) => {
          if (submitArgs[1]?.requestId === recoveryId) { reached(); await released; }
          return target.submit(...submitArgs);
        };
        const value = Reflect.get(target, property, receiver); return typeof value === 'function' ? value.bind(target) : value;
      } });
    });
    const resuming = f.state.service.app.inject({ method: 'POST', url: `/api/v3/research/${f.conversationId}/resume`, headers: f.state.headers,
      payload: { submissionId: f.submissionId } }).then(response => response);
    await submitting;
    const checkpoint = (await host.harness.snapshot(Study, f.conversationId, context))!;
    assert.equal(checkpoint.final?.reason, 'owner_stop', 'the stop remains visible until the atomic submit transaction commits');
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
    finishModel(); await host.stop(f.conversationId);
  } finally { release(); finishModel(); t.mock.restoreAll(); await f.state.cleanup(); }
});
