import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, defineTask } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { FactorTask, runFactor } from '../plugins/native-factor-task.ts';

const context = BACKGROUND_CONTEXT;

test('native scientific child failure remains an explicit failed task and tool result', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-child-test-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('compute', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('The native request was unavailable.')]);
  const plugin = defineExtension({ name: 'native-compute-test', tasks: [FactorTask], tools: [defineTool({
    name: 'compute', description: 'Invalid-request native task test', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) { await runFactor(api, ctx, '(module)', { nautilusTarget: { id: 'unused-validation-source', url: 'http://127.0.0.1:1' } }); return {}; },
  })] });
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [plugin] });
  try {
    const receipt = await host.start('child-failure', { goal: 'Observe failure honestly', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const children = await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    assert.equal(children.items.length, 1);
    const child = children.items[0]!;
    assert.ok(child.owner !== undefined);
    assert.equal(child.state.status, 'terminal');
    if (child.state.status === 'terminal') {
      assert.equal(child.state.outcome.status, 'failed');
      if (child.state.outcome.status === 'failed') assert.equal(child.state.outcome.error.message, 'catalog_required');
    }
    assert.ok(JSON.stringify((await host.view(receipt.conversationId)).entries).includes('catalog_required'));
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('official ownership cascades abort from parent tool to child task', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-child-abort-test-'));
  let enter!: () => void;
  const entered = new Promise<void>((resolve) => { enter = resolve; });
  const Wait = defineTask<Record<string, never>, { phase: 'wait' }, null>({
    name: 'qz.test.wait', version: 1, initial: () => ({ phase: 'wait' }),
    phases: { async wait(_task, runtime, ctx) {
      enter(); await runtime.sleep(runtime.now() + 120_000, ctx);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
    } },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  const plugin = defineExtension({ name: 'owned-child', tasks: [Wait], tools: [defineTool({
    name: 'child', description: 'Ownership protocol test', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) {
      const id = await api.createTask(Wait, {}, { ownership: { kind: 'task', taskId: api.taskId } }, ctx);
      await api.waitForTask(id, ctx); return {};
    },
  })] });
  const faux = fauxProvider({ tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('child', {}), { stopReason: 'toolUse' })]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [plugin] });
  try {
    const receipt = await host.start('child-abort', { goal: 'Observe child cancellation', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
    await entered;
    assert.ok((await host.tasks()).tasks.some((task) => task.record.kind === 'qz.test.wait' && task.record.owner !== undefined));
    await host.stop(receipt.conversationId);
    assert.equal((await host.tasks()).tasks.length, 0);
    const children = await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.test.wait' }, 100), context);
    const child = children.items[0]!;
    assert.equal(child.state.status, 'terminal');
    if (child.state.status === 'terminal') assert.equal(child.state.outcome.status, 'aborted');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
