import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ConversationId, SubmissionId } from '@earendil-works/pi-durable';
import { Requests, Study } from '../../research/src/documents.ts';
import { Application, json } from '../src/facts.ts';
import { context, fauxAssistantMessage, setup } from './support.ts';

const api = '/api/v3';
type State = Awaited<ReturnType<typeof setup>>;
const fixture = setup;
const input = (state: State, requestId: string) => ({ requestId, goal: 'Preserve the chosen research execution settings.',
  datasetId: 'controlled', model: state.model });
const start = (state: State, value: object) => state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: value });
async function nativeStudy(state: State, receipt: { conversationId: ConversationId; submissionId: SubmissionId }) {
  await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
  return { id: receipt.conversationId, study: (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))! };
}

test('Native research reasoning settings reach the native agent and survive changed defaults, replay and restart', { timeout: 30_000 }, async () => {
  const state = await fixture();
  const model = state.faux.getModel(); model.reasoning = true; model.api = 'openai-codex-responses';
  state.faux.setResponses([fauxAssistantMessage('Protocol only: no investment result.'), fauxAssistantMessage('Protocol only: no investment result.')]);
  const patch = (payload: object) => state.service.app.inject({ method: 'PATCH', url: `${api}/settings`, headers: state.headers, payload });
  try {
    assert.equal((await patch({ defaultModel: state.model, thinkingLevel: 'high', serviceTier: 'priority' })).statusCode, 200);
    const inherited = input(state, 'pm-inherited-reasoning');
    const explicit = { ...input(state, 'pm-explicit-reasoning'), thinkingLevel: 'low', serviceTier: 'default' };
    const admitted = [];
    for (const [value, thinkingLevel, serviceTier] of [[inherited, 'high', 'priority'], [explicit, 'low', 'default']] as const) {
      const response = await start(state, value); assert.equal(response.statusCode, 202, response.body);
      const view = response.json(); const native = await nativeStudy(state, view);
      assert.equal(native.study.input!.thinkingLevel, thinkingLevel); assert.equal(native.study.input!.serviceTier, serviceTier);
      const conversation = (await state.service.research.harness.conversation(native.id, context))!;
      assert.equal((await conversation.agent(context)).thinkingLevel, thinkingLevel);
      admitted.push({ value, view, id: native.id, thinkingLevel, serviceTier });
    }
    assert.equal((await patch({ thinkingLevel: 'medium', serviceTier: 'default' })).statusCode, 200);
    await state.restart();
    for (const saved of admitted) {
      const replay = await start(state, saved.value); assert.equal(replay.statusCode, 202, replay.body);
      assert.equal(replay.json().conversationId, saved.view.conversationId);
      const restored = (await state.service.research.harness.snapshot(Study, saved.id, context))!;
      assert.equal(restored.input!.thinkingLevel, saved.thinkingLevel); assert.equal(restored.input!.serviceTier, saved.serviceTier);
      const agent = await (await state.service.research.harness.conversation(saved.id, context))!.agent(context);
      assert.equal(agent.thinkingLevel, saved.thinkingLevel);
      const conflict = await start(state, { ...saved.value, thinkingLevel: 'medium' });
      assert.equal(conflict.statusCode, 409); assert.equal(conflict.json().error, 'request_id_conflict');
    }
  } finally { await state.cleanup(); }
});

test('Native research rejects invalid or unsupported model choices before creating a research request', { timeout: 20_000 }, async () => {
  const state = await fixture();
  try {
    const original = input(state, 'pm-invalid-reasoning');
    for (const [patch, error] of [
      [{ thinkingLevel: 'ultra' }, 'thinking_level_unsupported'], [{ thinkingLevel: 42 }, 'thinking_level_unsupported'],
      [{ thinkingLevel: 'high' }, 'thinking_level_unsupported'], [{ serviceTier: 'fast' }, 'invalid_service_tier'],
      [{ serviceTier: null }, 'invalid_service_tier'], [{ serviceTier: 'priority' }, 'service_tier_unsupported'],
      [{ model: { provider: original.model.provider, modelId: 'missing' } }, 'model_unavailable'],
    ] as const) {
      const response = await start(state, { ...original, ...patch });
      assert.equal(response.statusCode, error === 'model_unavailable' ? 409 : 400, response.body);
      assert.equal(response.json().error, error);
    }
    assert.deepEqual((await state.service.research.harness.snapshot(Requests, context))?.requests ?? {}, {});
  } finally { await state.cleanup(); }
});

test('Native research freezes the selected model default without inheriting another model preferences', { timeout: 20_000 }, async () => {
  const state = await fixture();
  state.faux.setResponses([fauxAssistantMessage('Protocol only: no investment result.')]);
  try {
    await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).settings = json({
      defaultModel: { provider: 'other-provider', modelId: 'other-model' }, thinkingLevel: 'max', serviceTier: 'priority',
    }); }, context);
    const selected = (await state.service.research.availableModels(state.model.provider)).find(model => model.modelId === state.model.modelId)!;
    const response = await start(state, input(state, 'pm-model-default')); assert.equal(response.statusCode, 202, response.body);
    const native = await nativeStudy(state, response.json());
    assert.equal(native.study.input!.thinkingLevel, undefined, 'No unrelated model preference is captured');
    assert.equal((await (await state.service.research.harness.conversation(native.id, context))!.agent(context)).thinkingLevel, selected.effectiveThinkingLevel);
    assert.equal(native.study.input!.serviceTier, undefined);
  } finally { await state.cleanup(); }
});
