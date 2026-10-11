import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { Study } from '../src/documents.ts';
import { evaluateTrial, trialFeedback } from '../src/trials.ts';
import research from '../plugins/research.ts';
import wasm from '../plugins/factor-wasm.ts';

const context = BACKGROUND_CONTEXT;

async function setup() {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-durable-test-'));
  const faux = fauxProvider({ models: [{ id: 'test-research' }], tokensPerSecond: 100_000 });
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
    assert.ok((await host.tasks()).tasks.length > 0);
    await host.stop(receipt.conversationId);
    assert.equal((await host.tasks()).tasks.length, 0);
    const submission = (await host.harness.submission(receipt.submissionId, context))!;
    assert.equal((await submission.status(context)).status, 'unanswered');
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
