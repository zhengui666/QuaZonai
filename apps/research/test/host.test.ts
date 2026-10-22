import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { Study } from '../src/documents.ts';
import { evaluateTrial, trialFeedback } from '../src/trials.ts';
import research from '../plugins/research.ts';
import { Continuation } from '../src/continuation.ts';
import { researchSettings } from '../src/settings.ts';
import wasm from '../plugins/factor-wasm.ts';

const context = BACKGROUND_CONTEXT;

async function setup(reasoning = false) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-durable-test-'));
  const faux = fauxProvider({ models: [{ id: 'test-research', reasoning }], tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  return { directory, faux, models, model, database: join(directory, 'research.sqlite'), async cleanup() { await rm(directory, { recursive: true, force: true }); } };
}

const input = (model: { provider: string; modelId: string }) => ({ goal: 'Find predictive factors without target positions.', model, data: {}, policy: { max_trials: 3 } });

test('official SQLite persists original identity, inputs, native view and submission replay', async () => {
  const setup_ = await setup(); let calls = 0;
  const plugin = defineExtension({ name: 'probe', tools: [defineTool({
    name: 'probe', description: 'Read-only protocol probe', parameters: Type.Object({}), replay: 'safe',
    async execute() { calls++; return { content: [{ type: 'text', text: 'observed' }] }; },
  })] });
  setup_.faux.setResponses([fauxAssistantMessage(fauxToolCall('probe', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('done')]);
  let host = await openResearch({ ...setup_, extensions: [plugin] });
  try {
    const receipt = await host.start('same-request', input(setup_.model));
    const submission = (await host.harness.submission(receipt.submissionId, context))!;
    assert.equal((await submission.wait(context)).status, 'done');
    assert.equal(calls, 1);
    assert.deepEqual(await host.start('same-request', input(setup_.model)), receipt);
    await assert.rejects(host.start('same-request', { ...input(setup_.model), goal: 'changed' }), /request_id_conflict/);
    assert.equal(calls, 1);
    const before = await host.view(receipt.conversationId);
    assert.deepEqual(before.research!.input, input(setup_.model));
    assert.ok(before.docs['pi.usage']);
    await host.close();
    host = await openResearch({ ...setup_, extensions: [plugin] });
    assert.deepEqual(await host.start('same-request', input(setup_.model)), receipt);
    assert.deepEqual((await host.view(receipt.conversationId)).entries, before.entries);
    assert.equal(calls, 1);
    assert.equal((await host.list()).items.length, 1);
  } finally { await host.close(); await setup_.cleanup(); }
});

test('failed scientific trials stay in ledger and optional final selection ends native generation', async () => {
  const setup_ = await setup();
  const plugin = defineExtension({ name: 'test-science', tools: [defineTool({
    name: 'evaluate', description: 'Trial ledger protocol test, not native science',
    parameters: Type.Object({ fail: Type.Boolean() }), replay: 'safe',
    async execute(args, api, ctx) {
      const result = await evaluateTrial(api, ctx, { fail: args.fail }, async () => {
        if (args.fail) throw new Error('native_input_missing');
        return { protocolOnly: true };
      });
      return { content: [{ type: 'text', text: JSON.stringify(result) }] };
    },
  })] });
  setup_.faux.setResponses([
    fauxAssistantMessage(fauxToolCall('evaluate', { fail: true }), { stopReason: 'toolUse' }),
    fauxAssistantMessage(fauxToolCall('evaluate', { fail: false }), { stopReason: 'toolUse' }),
    fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'budget complete' }), { stopReason: 'toolUse' }),
  ]);
  const host = await openResearch({ ...setup_, extensions: [plugin, research] });
  try {
    const receipt = await host.start('record-failures', input(setup_.model));
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = await host.harness.snapshot(Study, receipt.conversationId, context);
    assert.deepEqual(facts!.trials.map((trial) => trial.status), ['FAILED', 'COMPUTED']);
    assert.equal(facts!.trials[0]!.error, 'native_input_missing');
    assert.equal(facts!.final!.reason, 'budget complete');
    assert.equal(setup_.faux.state.callCount, 3);
  } finally { await host.close(); await setup_.cleanup(); }
});

test('native abort joins the running tool; cancelling an observation does not cancel work', async () => {
  const setup_ = await setup();
  let enter!: () => void;
  const entered = new Promise<void>((resolve) => { enter = resolve; });
  const plugin = defineExtension({ name: 'blocking-tool', tools: [defineTool({
    name: 'block', description: 'Cancellation protocol probe', parameters: Type.Object({}),
    async execute(_args, _api, ctx) {
      enter();
      await awaitWithContext(new Promise<void>(() => {}), ctx);
      return {};
    },
  })] });
  setup_.faux.setResponses([fauxAssistantMessage(fauxToolCall('block', {}), { stopReason: 'toolUse' })]);
  const host = await openResearch({ ...setup_, extensions: [plugin] });
  try {
    const receipt = await host.start('cancel-running', input(setup_.model));
    await entered;
    const observer = await host.watch(receipt.conversationId);
    let deliveries = 0;
    observer.start(async () => { deliveries++; });
    await observer.stop();
    assert.equal((await observer.closed).reason, 'stopped');
    const deliveredBeforeStop = deliveries;
    assert.equal((await host.submission(receipt.submissionId)).status, 'placed');
    assert.ok((await host.tasks()).tasks.length > 0);
    await host.stop(receipt.conversationId);
    assert.equal((await host.tasks()).tasks.length, 0);
    const submission = (await host.harness.submission(receipt.submissionId, context))!;
    assert.equal((await submission.status(context)).status, 'unanswered');
    assert.equal(deliveries, deliveredBeforeStop);
  } finally { await host.close(); await setup_.cleanup(); }
});

test('explicit zero trial budget records rejection and terminates without running science', async () => {
  const setup_ = await setup(); let computations = 0;
  const plugin = defineExtension({ name: 'budget-science', tools: [defineTool({
    name: 'evaluate', description: 'Budget protocol probe', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) { return trialFeedback(api, ctx, () => evaluateTrial(api, ctx, { hypothesis: 'budget check' }, async () => { computations++; return {}; })); },
  })] });
  setup_.faux.setResponses([fauxAssistantMessage(fauxToolCall('evaluate', {}), { stopReason: 'toolUse' })]);
  const host = await openResearch({ ...setup_, extensions: [plugin] });
  try {
    const receipt = await host.start('zero-budget', { ...input(setup_.model), policy: { max_trials: 0 } });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.equal(computations, 0);
    assert.equal(facts.trials[0]!.status, 'REJECTED');
    assert.equal(facts.final!.reason, 'trial_budget_exhausted');
    assert.equal(facts.final!.status, 'STOPPED');
    assert.equal(setup_.faux.state.callCount, 1);
  } finally { await host.close(); await setup_.cleanup(); }
});

test('default factor input preparation failure remains a disclosed scientific trial', async () => {
  const setup_ = await setup();
  setup_.faux.setResponses([
    fauxAssistantMessage(fauxToolCall('factor_evaluate', { hypothesis: 'Original failed hypothesis', model: '(module)' }), { stopReason: 'toolUse' }),
    fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'No input catalog supplied' }), { stopReason: 'toolUse' }),
  ]);
  const host = await openResearch({ ...setup_, extensions: [wasm, research] });
  try {
    const receipt = await host.start('missing-native-input', input(setup_.model));
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.equal(facts.trials.length, 1);
    assert.equal(facts.trials[0]!.status, 'FAILED');
    assert.equal(facts.trials[0]!.error, 'native_request_required');
    assert.equal((facts.trials[0]!.proposal as { hypothesis: string }).hypothesis, 'Original failed hypothesis');
  } finally { await host.close(); await setup_.cleanup(); }
});

test('repeating an unchanged scientific ledger closes technically and preserves original policy on restart', async () => {
  const setup_ = await setup(true);
  setup_.faux.setResponses([
    ...Array.from({ length: 6 }, () => fauxAssistantMessage('This study is still unfinished.')),
    fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Original criteria cannot be evaluated with missing data' }), { stopReason: 'toolUse' }),
  ]);
  let host = await openResearch({ ...setup_, extensions: [research] });
  try {
    const original = input(setup_.model);
    const receipt = await host.start('uncapped-yields', original);
    await host.harness.waitForIdle(context);
    assert.equal((await (await host.harness.conversation(receipt.conversationId, context))!.agent(context)).thinkingLevel, 'medium');
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.deepEqual(facts.input, original);
    assert.equal(facts.trials.length, 0);
    assert.equal(facts.final!.reason, 'research_no_new_evidence');
    assert.equal(facts.final!.conclusion, 'NEEDS_EVIDENCE');
    const continuation = await host.harness.snapshot(Continuation, receipt.conversationId, context);
    assert.equal(continuation!.yields, 2);
    assert.equal(setup_.faux.state.callCount, 2);
    await host.close();
    host = await openResearch({ ...setup_, extensions: [research], thinkingLevel: 'high' });
    assert.deepEqual(await host.start('uncapped-yields', original), receipt);
    assert.equal(setup_.faux.state.callCount, 2);
    assert.deepEqual(await host.harness.snapshot(Continuation, receipt.conversationId, context), continuation);
    assert.equal((await (await host.harness.conversation(receipt.conversationId, context))!.agent(context)).thinkingLevel, 'medium');
  } finally { await host.close(); await setup_.cleanup(); }
});

for (const finalFirst of [true, false]) {
  test(`mixed final/read round is bounded (${finalFirst ? 'final first' : 'read first'}); later science cannot run`, async () => {
    const setup_ = await setup(); let computed = 0;
    const plugin = defineExtension({ name: 'mixed-science', tools: [defineTool({
      name: 'evaluate', description: 'Scientific ledger protocol only', parameters: Type.Object({}), replay: 'safe', executionMode: 'sequential',
      async execute(_args, api, ctx) { return trialFeedback(api, ctx, () => evaluateTrial(api, ctx, {}, async () => { computed++; return {}; })); },
    })] });
    const calls = [fauxToolCall('research_finish', { reason: 'Requested stop' }), fauxToolCall('research_brief', {})];
    setup_.faux.setResponses([fauxAssistantMessage(finalFirst ? calls : calls.reverse(), { stopReason: 'toolUse' }), fauxAssistantMessage(fauxToolCall('evaluate', {}), { stopReason: 'toolUse' })]);
    const host = await openResearch({ ...setup_, extensions: [plugin, research] });
    try {
      const receipt = await host.start(`mixed-${finalFirst}`, input(setup_.model));
      assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
      const view = await host.view(receipt.conversationId);
      assert.equal(view.research!.final!.reason, 'Requested stop');
      assert.equal(view.research!.trials.length, 0);
      assert.equal(computed, 0);
      assert.equal(setup_.faux.state.callCount, finalFirst ? 1 : 2);
      if (!finalFirst) assert.ok(JSON.stringify(view.entries).includes('research_already_final'));
    } finally { await host.close(); await setup_.cleanup(); }
  });
}

test('native backoff checkpoint survives close/reopen; quota and auth fail without retry or fallback', { timeout: 10_000 }, async () => {
  const setup_ = await setup();
  let host = await openResearch({ ...setup_, settings: { retry: { baseDelayMs: 500 } } });
  try {
    const settings = researchSettings();
    assert.deepEqual(settings.retry, { enabled: true, maxRetries: 6, baseDelayMs: 2000, maxAgentDelayMs: 60000 });
    assert.equal(settings.progress!.partialIntervalMs, 250);
    assert.equal(researchSettings({ stream: { timeoutMs: -1 } }).stream?.timeoutMs, undefined);
    const graph = await host.harness.watchTaskGraph(context);
    let ready!: (id: TaskId) => void;
    const backoff = new Promise<TaskId>((resolve) => { ready = resolve; });
    graph.start(async (value) => { const task = Object.values(value.tasks).find((task) => 'phase' in task.state && task.state.phase === 'retry'); if (task) ready(task.id); });
    setup_.faux.setResponses([(_transcript, options) => {
      assert.equal(options!.timeoutMs, undefined); assert.equal(options!.maxRetries, 0);
      return fauxAssistantMessage('', { stopReason: 'error', errorMessage: '503 service unavailable' });
    }]);
    const receipt = await host.start('transient-retry', input(setup_.model));
    const taskId = await backoff;
    const task = (await host.harness.getTask(taskId, context))!;
    assert.notEqual(task.state.status, 'terminal');
    const checkpoint = task.state.status !== 'terminal' ? task.state.checkpoint as { phase: string; attempt: number; until: number } : null;
    assert.equal(checkpoint!.phase, 'retry'); assert.equal(checkpoint!.attempt, 1); assert.ok(checkpoint!.until > Date.now());
    const observer = await host.watch(receipt.conversationId);
    observer.start(async () => {});
    await graph.stop(); await host.close();
    assert.equal((await observer.closed).reason, 'stopped');
    await assert.rejects(host.start('after-close', input(setup_.model)), /research_closing/);
    setup_.faux.setResponses([() => { assert.ok(Date.now() >= checkpoint!.until); return fauxAssistantMessage('Recovered original submission'); }]);
    host = await openResearch({ ...setup_, settings: { retry: { baseDelayMs: 500 } } });
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    assert.equal(setup_.faux.state.callCount, 2);
    const agent = await (await host.harness.conversation(receipt.conversationId, context))!.agent(context);
    assert.equal(agent.thinkingLevel, (await host.availableModels())[0]!.effectiveThinkingLevel);
    assert.ok((await host.view(receipt.conversationId)).docs['pi.agent']);
    for (const message of ['insufficient_quota: billing quota exhausted', '401 unauthorized: invalid api key']) {
      const before: number = setup_.faux.state.callCount;
      setup_.faux.setResponses([fauxAssistantMessage('', { stopReason: 'error', errorMessage: message })]);
      const rejected = await host.start(message, input(setup_.model));
      assert.equal((await (await host.harness.submission(rejected.submissionId, context))!.wait(context)).status, 'unanswered');
      assert.equal(setup_.faux.state.callCount, before + 1);
    }
  } finally { await host.close(); await setup_.cleanup(); }
});
