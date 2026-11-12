import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { Requests, Study } from '../src/documents.ts';
import { ResearchConfiguration } from '../src/research-configuration.ts';

// The former QZ signal/configuration/batch implementation is intentionally gone.
// Keep admission, tool visibility, continuation and original Doc identity tests.
test('research rejects retired indicator bindings and only exposes upstream strategy execution', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-upstream-only-'));
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  const input = { goal: 'Discuss a native strategy', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} };
  try {
    for (const key of ['indicator_strategy_request', 'indicator_strategy_batch_request']) {
      await assert.rejects(host.start(key, { ...input, data: { catalog: directory, [key]: {} } }), /custom_indicator_strategy_removed/);
      const rejected = (await host.harness.snapshot(Requests, context))?.requests[`request:${key}`];
      assert.equal(rejected?.conversationId, null); assert.equal(rejected?.submissionId, null);
      await assert.rejects(host.start(key, { ...input, data: { catalog: directory, [key]: {} } }), /custom_indicator_strategy_removed/);
    }
    assert.equal(faux.state.callCount, 0);
    faux.setResponses([fauxAssistantMessage(fauxToolCall('native_indicator_strategy_evaluate', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('The retired tool cannot run; discuss the upstream strategy instead.')]);
    const ordinary = await host.start('ordinary', input);
    await (await host.harness.submission(ordinary.submissionId, context))!.wait(context);
    const agent = await (await host.harness.conversation(ordinary.conversationId, context))!.agent(context);
    assert.ok(agent.tools.some(tool => tool.name === 'research_brief'));
    assert.ok(agent.tools.some(tool => tool.name === 'native_strategy_backtest'));
    assert.ok(!agent.tools.some(tool => /native_indicator|factor_evaluate|ensemble|review_finish|strategy_portfolio/.test(tool.name)));
    const study = (await host.harness.snapshot(Study, ordinary.conversationId, context))!;
    assert.equal(study.trials.length, 0); assert.equal(study.final, null);
    faux.setResponses([fauxAssistantMessage('Continue the same discussion without inventing results.')]);
    const resumed = await host.resume(ordinary.conversationId, ordinary.submissionId, 'Explain original upstream configuration.');
    await (await host.harness.submission(resumed.submissionId, context))!.wait(context);
    await host.harness.commit(async tx => { (await tx.doc(ResearchConfiguration, ordinary.conversationId)).request = { original: 'retained-doc' }; }, context);
    const stored = await host.harness.snapshot(ResearchConfiguration, ordinary.conversationId, context);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(ResearchConfiguration, ordinary.conversationId, context), stored);
    assert.deepEqual(await host.start('ordinary', input), ordinary);
    faux.setResponses([fauxAssistantMessage('Exact upstream binding is available; no execution claimed.')]);
    const bound = await host.start('native', { ...input, data: { catalog: directory, native_strategy_backtest_request: {} } });
    await (await host.harness.submission(bound.submissionId, context))!.wait(context);
    const boundAgent = await (await host.harness.conversation(bound.conversationId, context))!.agent(context);
    assert.ok(boundAgent.extensions.every(extension => extension.name !== 'qz.indicator-strategy'));
    assert.ok(boundAgent.tools.some(tool => tool.name === 'native_strategy_backtest'));
    assert.ok(!boundAgent.tools.some(tool => tool.name.startsWith('native_indicator')));
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
