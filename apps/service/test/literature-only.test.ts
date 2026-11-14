import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { setup, context } from './support.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';

const allowed = ['research_brief', 'research_finish', 'research_literature', 'research_literature_note', 'research_literature_search'];
test('literature-only HTTP admission strips native bindings and durable tools remain restricted on resume', { timeout: 20000 }, async () => {
  const state = await setup();
  const dataset = state.options.config.datasets[0]!;
  dataset.data.sealed = { catalog: '/nonexistent/SEALED_SENTINEL' };
  dataset.policy = { tickEconomic: { retired: true } };
  let calls = 0;
  state.faux.setResponses([
    transcript => {
      calls++;
      return fauxAssistantMessage(fauxToolCall('native_strategy_catalog', {}), { stopReason: 'toolUse' });
    },
    transcript => {
      const result = transcript.messages.find(message => message.role === 'toolResult');
      assert.ok(result?.role === 'toolResult' && result.isError);
      return fauxAssistantMessage(fauxToolCall('native_runner_build', { requestId: 'must-not-run' }), { stopReason: 'toolUse' });
    },
    transcript => {
      const result = transcript.messages.filter(message => message.role === 'toolResult').at(-1);
      assert.ok(result?.role === 'toolResult' && result.isError);
      return fauxAssistantMessage(fauxToolCall('research_brief', {}), { stopReason: 'toolUse' });
    },
    transcript => {
      assert.ok(!JSON.stringify(transcript).includes('SEALED_SENTINEL'));
      return fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Discussion only' }), { stopReason: 'toolUse' });
    },
  ]);
  try {
    for (const extra of [{ runnerBuildRequestId: 'forbidden', expectedRunnerRevision: 1 }, { input: { data: { sealed: {} } } }, { specification: {} }]) {
      const rejected = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
        payload: { requestId: 'reject-literature-binding', mode: 'LITERATURE_ONLY', goal: 'Discuss', model: state.model, datasetId: dataset.id, ...extra } });
      assert.equal(rejected.statusCode, 400, rejected.body);
      assert.match(rejected.body, /literature_only_binding_unsupported/);
    }
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'literature-only', mode: 'LITERATURE_ONLY', goal: 'Discuss literature', model: state.model, datasetId: dataset.id } });
    assert.equal(response.statusCode, 202, response.body);
    const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.equal(calls, 1);
    const study = await state.service.research.harness.snapshot(Study, receipt.conversationId, context);
    assert.deepEqual(study!.input!.data, { dataset: { id: dataset.id, name: dataset.name, assetClass: dataset.assetClass, currency: dataset.currency } });
    assert.deepEqual(study!.input!.policy, {});
    assert.equal(study!.trials.length, 0);
    const requests = await state.service.research.harness.snapshot(Requests, context);
    assert.equal(requests!.requests['request:literature-only']!.sourceEvidence, undefined);
    assert.ok(!JSON.stringify(requests!.requests['request:literature-only']).includes('SEALED_SENTINEL'));
    const agent = await (await state.service.research.harness.conversation(receipt.conversationId, context))!.agent(context);
    assert.deepEqual(agent.tools.map(tool => tool.name).sort(), allowed);
    assert.deepEqual(agent.callable.map(tool => tool.name).sort(), allowed);
    await state.restart();
    const reopened = await (await state.service.research.harness.conversation(receipt.conversationId, context))!.agent(context);
    assert.deepEqual(reopened.tools.map(tool => tool.name).sort(), allowed);
    state.faux.setResponses([transcript => {
      calls++;
      return fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Still only discussion' }), { stopReason: 'toolUse' });
    }]);
    const resume = await state.service.app.inject({ method: 'POST', url: `/api/v3/research/${receipt.conversationId}/resume`, headers: state.headers, payload: { submissionId: receipt.submissionId, instruction: 'Continue discussion' } });
    assert.equal(resume.statusCode, 202, resume.body);
    await (await state.service.research.harness.submission(resume.json().submissionId, context))!.wait(context);
    assert.equal(calls, 2);
    const resumed = await (await state.service.research.harness.conversation(receipt.conversationId, context))!.agent(context);
    assert.deepEqual(resumed.callable.map(tool => tool.name).sort(), allowed);
  } finally { await state.cleanup(); }
});

test('direct host input cannot attach sealed data or native policy to literature mode', () => {
  const input = { mode: 'LITERATURE_ONLY', goal: 'Discuss', model: { provider: 'test', modelId: 'test' }, data: { dataset: { id: 'registered' } }, policy: {} };
  assert.equal(researchInput(input).mode, 'LITERATURE_ONLY');
  for (const key of ['sealed', 'nativeInput', 'catalog', 'request']) assert.throws(() => researchInput({ ...input, data: { ...input.data, [key]: {} } }), /literature_only_data_unsupported/);
  assert.throws(() => researchInput({ ...input, policy: { tickEconomic: {} } }), /literature_only_policy_unsupported/);
});
