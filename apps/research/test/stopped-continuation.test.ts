import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxAssistantMessage, fauxProvider, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { Requests, Study } from '../src/documents.ts';
import { evaluateTrial } from '../src/trials.ts';
import { recoveryRequestId } from '../src/recovery.ts';
import research from '../plugins/research.ts';

const context = BACKGROUND_CONTEXT;
const probe = defineExtension({ name: 'test.stopped-continuation', tools: [defineTool({
  name: 'continuation_trial', description: 'Test-only ledger protocol, not market evidence', parameters: Type.Object({}), replay: 'safe',
  async execute(_args, api, ctx) {
    await evaluateTrial(api, ctx, { hypothesis: 'Original protocol candidate', members: [{ model: '(module)', weight: '1' }], instrumentId: 'TEST.SIM' }, async () => ({ protocolOnly: true }));
    return { content: [{ type: 'text', text: 'Original protocol candidate registered' }] };
  },
})] });
const responses = () => [
  fauxAssistantMessage(fauxToolCall('continuation_trial', {}), { stopReason: 'toolUse' }),
  fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Invocation boundary is not scientific infeasibility' }), { stopReason: 'toolUse' }),
];
async function stoppedStudy(maximum = 3) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-stopped-continuation-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'original.sqlite'), models, extensions: [probe, research] };
  let host = await openResearch(options);
  faux.setResponses(responses());
  const input = { goal: 'Keep the original goal, data, model and budget', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: { max_trials: maximum } };
  const receipt = await host.start('original-owner-key', input);
  await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
  const before = (await host.state(receipt.conversationId)).research!;
  assert.equal(before.final!.status, 'STOPPED'); assert.equal((await host.submission(receipt.submissionId)).status, 'done');
  return { directory, faux, receipt, input, before, get host() { return host; },
    async reopen() { await host.close(); host = await openResearch(options); },
    async close() { await host.close(); await rm(directory, { recursive: true, force: true }); } };
}
function interruptNextRecovery(state: Awaited<ReturnType<typeof stoppedStudy>>) {
  const original = state.host.harness.conversation.bind(state.host.harness);
  state.host.harness.conversation = async (id, ctx) => {
    const handle = await original(id, ctx); if (!handle) return handle;
    const submit = handle.submit.bind(handle);
    handle.submit = async (draft, inner) => {
      if (draft.requestId?.startsWith('qz.research.resume:')) throw new Error('test_interruption_before_native_input');
      return submit(draft, inner);
    };
    return handle;
  };
}

test('unselected STOPPED continues with new native identity and registered result, preserving all prior facts', { timeout: 15_000 }, async () => {
  const state = await stoppedStudy();
  try {
    state.faux.setResponses(responses());
    const [next, replay] = await Promise.all([0, 1].map(() => state.host.resume(state.receipt.conversationId, state.receipt.submissionId)));
    assert.deepEqual(replay, next); assert.equal(next.previousSubmissionId, state.receipt.submissionId); assert.ok(next.submissionId > state.receipt.submissionId);
    await (await state.host.harness.submission(next.submissionId, context))!.wait(context);
    const facts = (await state.host.state(next.conversationId)).research!;
    assert.deepEqual(facts.input, state.before.input); assert.deepEqual(facts.trials.slice(0, 1), state.before.trials);
    assert.equal(facts.trials.length, 2); assert.equal(facts.trials[1]!.status, 'COMPUTED');
    assert.deepEqual(Reflect.get(facts, 'previousStops'), [{ previousSubmissionId: state.receipt.submissionId, final: state.before.final }]);
    assert.notEqual(facts.final!.taskId, state.before.final!.taskId);
    const oldTask = await state.host.harness.getTask(state.before.final!.taskId as never, context); assert.equal(oldTask!.state.status, 'terminal');
    assert.equal((await state.host.submission(state.receipt.submissionId)).status, 'done');
    assert.deepEqual(await state.host.start('original-owner-key', state.input), state.receipt);
    // Session map order cannot make a continuation look like the original admission.
    await state.host.harness.commit(async tx => { const doc = await tx.doc(Requests); doc.requests = Object.fromEntries(Object.entries(doc.requests).reverse()); }, context);
    assert.equal((await state.host.submissionFor(next.conversationId))!.id, state.receipt.submissionId);
    await assert.rejects(state.host.start(recoveryRequestId(next.conversationId, next.previousSubmissionId), state.input), /request_id_conflict/);
    await state.reopen();
    assert.deepEqual(await state.host.resume(next.conversationId, next.previousSubmissionId), next);
    assert.deepEqual((await state.host.state(next.conversationId)).research, facts);
    assert.equal(state.faux.state.callCount, 4);
  } finally { await state.close(); }
});

test('saved continuation intent repairs only the missing native input after reopen', { timeout: 15_000 }, async () => {
  const state = await stoppedStudy();
  try {
    interruptNextRecovery(state);
    await assert.rejects(state.host.resume(state.receipt.conversationId, state.receipt.submissionId), /test_interruption_before_native_input/);
    const interrupted = (await state.host.state(state.receipt.conversationId)).research!;
    assert.equal(interrupted.final, null); assert.deepEqual(interrupted.trials, state.before.trials);
    assert.deepEqual(Reflect.get(interrupted, 'previousStops'), [{ previousSubmissionId: state.receipt.submissionId, final: state.before.final }]);
    const key = recoveryRequestId(state.receipt.conversationId, state.receipt.submissionId);
    const intent = (await state.host.harness.snapshot(Requests, context))!.requests[`request:${key}`]!;
    assert.deepEqual(intent.input, state.input);
    assert.deepEqual(Reflect.get(intent, 'recovery'), { previousSubmissionId: state.receipt.submissionId, mode: 'stopped' });
    state.faux.setResponses(responses()); await state.reopen();
    const next = await state.host.resume(state.receipt.conversationId, state.receipt.submissionId);
    await (await state.host.harness.submission(next.submissionId, context))!.wait(context);
    const facts = (await state.host.state(next.conversationId)).research!;
    assert.equal(facts.trials.length, 2); assert.deepEqual(Reflect.get(facts, 'previousStops'), Reflect.get(interrupted, 'previousStops'));
    const handle = (await state.host.harness.conversation(next.conversationId, context))!;
    const entries = JSON.stringify((await handle.context(context)).messages);
    assert.match(entries, /owner requests continuation/); assert.match(entries, /successor generations/);
    assert.equal(state.faux.state.callCount, 4);
  } finally { await state.close(); }
});

test('a later owner stop wins over interrupted continuation replay and startup repair', { timeout: 15_000 }, async () => {
  const state = await stoppedStudy();
  try {
    interruptNextRecovery(state);
    await assert.rejects(state.host.resume(state.receipt.conversationId, state.receipt.submissionId), /test_interruption_before_native_input/);
    await state.host.stop(state.receipt.conversationId);
    const stopped = (await state.host.state(state.receipt.conversationId)).research!;
    assert.equal(stopped.final!.reason, 'owner_stop');
    await assert.rejects(state.host.resume(state.receipt.conversationId, state.receipt.submissionId), /research_already_final/);
    await state.reopen();
    assert.deepEqual((await state.host.state(state.receipt.conversationId)).research, stopped);
    assert.equal((await state.host.state(state.receipt.conversationId)).submission!.id, state.receipt.submissionId);
    assert.equal(state.faux.state.callCount, 2);
  } finally { await state.close(); }
});

for (const kind of ['selected', 'assessed', 'failed-final', 'exhausted', 'wrong-predecessor'] as const) {
  test(`stopped continuation refuses ${kind} without changing facts or model calls`, { timeout: 15_000 }, async () => {
    const state = await stoppedStudy(kind === 'exhausted' ? 1 : 3);
    try {
      if (['selected', 'assessed', 'failed-final'].includes(kind)) await state.host.harness.commit(async tx => {
        const final = (await tx.doc(Study, state.receipt.conversationId)).final!;
        if (kind === 'selected') final.trialId = state.before.trials[0]!.taskId!;
        else if (kind === 'assessed') { final.status = 'COMPUTED'; final.result = { ownerFinalOnly: true }; }
        else final.status = 'FAILED';
      }, context);
      const before = (await state.host.state(state.receipt.conversationId)).research;
      await assert.rejects(state.host.resume(state.receipt.conversationId, state.receipt.submissionId + (kind === 'wrong-predecessor' ? 1 : 0)),
        kind === 'exhausted' ? /trial_budget_exhausted/ : kind === 'wrong-predecessor' ? /research_not_recoverable_conflict/ : /research_already_final/);
      assert.deepEqual((await state.host.state(state.receipt.conversationId)).research, before); assert.equal(state.faux.state.callCount, 2);
    } finally { await state.close(); }
  });
}

test('native continuation instructions distinguish response boundaries from actual stop criteria', { timeout: 15_000 }, async () => {
  const state = await stoppedStudy();
  try {
    state.faux.setResponses([
      (transcript) => { assert.match(JSON.stringify(transcript), /successor generations/); return fauxAssistantMessage('The original study remains unfinished.'); },
      (transcript) => { assert.match(JSON.stringify(transcript), /response boundary/); return fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Protocol test complete' }), { stopReason: 'toolUse' }); },
    ]);
    const next = await state.host.resume(state.receipt.conversationId, state.receipt.submissionId);
    await (await state.host.harness.submission(next.submissionId, context))!.wait(context);
    assert.equal((await state.host.state(next.conversationId)).research!.final!.reason, 'Protocol test complete');
  } finally { await state.close(); }
});
