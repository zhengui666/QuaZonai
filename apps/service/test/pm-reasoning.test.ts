import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { PmMandateStart, PmMandateView } from '../../../packages/contracts/src/pm.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { Application, json } from '../src/facts.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';
import { context, fauxAssistantMessage, setup } from './support.ts';

const api = '/api/v3';
type State = Awaited<ReturnType<typeof setup>>;
async function fixture() {
  const state = await setup();
  state.options.config.datasets[0]!.data.request = { train_end_ns: '140', validation_end_ns: '180', forecast: {
    schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200',
      bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 }, parameters: {},
  } };
  await state.restart();
  return state;
}
const input = (state: State, requestId: string): PmMandateStart => ({ requestId, goal: 'Preserve the chosen research execution settings.',
  datasetId: 'controlled', funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '1000', currency: 'USD' } }, model: state.model, maxTrials: 1 });
const start = (state: State, value: object) => state.service.app.inject({ method: 'POST', url: `${api}/mandates`, headers: state.headers, payload: value });
async function nativeStudy(state: State, view: PmMandateView) {
  const deadline = Date.now() + 10_000;
  for (;;) {
    const admission = (await state.service.research.harness.snapshot(Requests, context))?.requests[`request:pm:${view.campaigns[0]!.id}`];
    if (admission) return { id: admission.conversationId, study: (await state.service.research.harness.snapshot(Study, admission.conversationId, context))! };
    assert.ok(Date.now() < deadline, 'The admitted mandate must reach its original native research.');
    await new Promise(resolve => setTimeout(resolve, 10));
  }
}

test('PM reasoning settings reach the native agent and survive changed defaults, replay and restart', { timeout: 30_000 }, async () => {
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
      const view = response.json<PmMandateView>(); const native = await nativeStudy(state, view);
      assert.equal(native.study.input!.thinkingLevel, thinkingLevel); assert.equal(native.study.input!.serviceTier, serviceTier);
      const conversation = (await state.service.research.harness.conversation(native.id, context))!;
      assert.equal((await conversation.agent(context)).thinkingLevel, thinkingLevel);
      admitted.push({ value, view, id: native.id, thinkingLevel, serviceTier });
    }
    assert.equal((await patch({ thinkingLevel: 'medium', serviceTier: 'default' })).statusCode, 200);
    await state.restart();
    for (const saved of admitted) {
      const replay = await start(state, saved.value); assert.equal(replay.statusCode, 202, replay.body);
      assert.equal(replay.json<PmMandateView>().mandate.id, saved.view.mandate.id);
      const restored = (await state.service.research.harness.snapshot(Study, saved.id, context))!;
      assert.equal(restored.input!.thinkingLevel, saved.thinkingLevel); assert.equal(restored.input!.serviceTier, saved.serviceTier);
      const record = pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates[saved.view.mandate.id]!;
      assert.equal(record.input.thinkingLevel, saved.thinkingLevel); assert.equal(record.input.serviceTier, saved.serviceTier);
      const conflict = await start(state, { ...saved.value, thinkingLevel: 'medium' });
      assert.equal(conflict.statusCode, 409); assert.equal(conflict.json().error, 'request_id_conflict');
    }
  } finally { await state.cleanup(); }
});

test('PM rejects invalid or unsupported model choices before creating a mandate', { timeout: 20_000 }, async () => {
  const state = await fixture();
  try {
    const original = input(state, 'pm-invalid-reasoning');
    for (const [patch, error] of [
      [{ thinkingLevel: 'ultra' }, 'invalid_thinking_level'], [{ thinkingLevel: null }, 'invalid_thinking_level'],
      [{ thinkingLevel: 'high' }, 'thinking_level_unsupported'], [{ serviceTier: 'fast' }, 'invalid_service_tier'],
      [{ serviceTier: null }, 'invalid_service_tier'], [{ serviceTier: 'priority' }, 'service_tier_unsupported'],
      [{ model: { provider: original.model.provider, modelId: 'missing' } }, 'model_unavailable'],
    ] as const) {
      const response = await start(state, { ...original, ...patch });
      assert.equal(response.statusCode, error === 'model_unavailable' ? 409 : 400, response.body);
      assert.equal(response.json().error, error);
    }
    assert.deepEqual(pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates, {});
  } finally { await state.cleanup(); }
});

test('PM freezes the selected model default without inheriting another model preferences', { timeout: 20_000 }, async () => {
  const state = await fixture();
  state.faux.setResponses([fauxAssistantMessage('Protocol only: no investment result.')]);
  try {
    await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).settings = json({
      defaultModel: { provider: 'other-provider', modelId: 'other-model' }, thinkingLevel: 'max', serviceTier: 'priority',
    }); }, context);
    const selected = (await state.service.research.availableModels(state.model.provider)).find(model => model.modelId === state.model.modelId)!;
    const response = await start(state, input(state, 'pm-model-default')); assert.equal(response.statusCode, 202, response.body);
    const native = await nativeStudy(state, response.json<PmMandateView>());
    assert.equal(native.study.input!.thinkingLevel, selected.effectiveThinkingLevel);
    assert.equal(native.study.input!.serviceTier, undefined);
  } finally { await state.cleanup(); }
});
