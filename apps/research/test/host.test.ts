import { bool, obj } from '@earendil-works/pi-schema';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { Study, Requests } from '../src/documents.ts';
import research from '../plugins/research.ts';
import { researchSettings } from '../src/settings.ts';

const context = BACKGROUND_CONTEXT;

async function setup(reasoning = false) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-durable-test-'));
  const faux = fauxProvider({ models: [{ id: 'test-research', reasoning }], tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  return { directory, faux, models, model, database: join(directory, 'research.sqlite'), async cleanup() { await rm(directory, { recursive: true, force: true }); } };
}

const input = (model: { provider: string; modelId: string }) => ({ goal: 'Find predictive factors without target positions.', model, data: {}, policy: { } });

test('native backoff checkpoint survives close/reopen; quota and auth fail without retry or fallback', { timeout: 10_000 }, async () => {
  const setup_ = await setup();
  let host = await openResearch({ ...setup_, settings: { retry: { baseDelayMs: 500 } } });
  try {
    const settings = researchSettings();
    assert.deepEqual(settings.retry, { enabled: true, maxRetries: 6, baseDelayMs: 2000, maxAgentDelayMs: 60000 });
    assert.equal(settings.progress!.partialIntervalMs, 250);
    assert.throws(() => researchSettings({ stream: { timeoutMs: -1 } }), /invalid_settings_stream_timeoutMs/);
    const graph = await host.harness.watchTaskGraph(context);
    let ready!: (id: TaskId) => void;
    const backoff = new Promise<TaskId>((resolve) => { ready = resolve; });
    graph.start(async (call) => { if (call.kind !== 'value') return; const value = call.value; const task = Object.values(value.tasks).find((task) => 'phase' in task.state && task.state.phase === 'retry'); if (task) ready(task.id); });
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
    assert.equal((await observer.closed).kind, 'stopped');
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
