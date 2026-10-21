import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import { createModels, fauxAssistantMessage, fauxProvider } from '@earendil-works/pi-ai';
import { getSupportedThinkingLevels } from '@earendil-works/pi-ai/models';
import { openaiCodexProvider } from '@earendil-works/pi-ai/providers/openai-codex';
import type { ProviderAuth } from '@earendil-works/pi-ai';
import type { ModelLoginView } from '../../../packages/contracts/src/index.ts';
import { ModelCredentials } from '../../research/src/credentials.ts';
import { Application, json } from '../src/facts.ts';
import { setup, studied, context } from './support.ts';

const api = '/api/v3';
async function waitFor(read: () => Promise<ModelLoginView>, condition: (view: ModelLoginView) => boolean) {
  for (;;) { const view = await read(); if (condition(view)) return view; await new Promise(resolve => setImmediate(resolve)); }
}

test('official provider login prompts, original retries, cancellation, persistence, safe errors and shutdown', { timeout: 20_000 }, async () => {
  const state = await setup(); const credentials = await ModelCredentials.open(join(state.directory, 'private-auth'));
  const faux = fauxProvider({ provider: 'controlled-auth', models: [{ id: 'reasoning', reasoning: true }] });
  const deviceIds: string[] = []; let logins = 0; let ambient = false; let broken = false;
  const auth: ProviderAuth = {
    apiKey: {
      name: 'Controlled API Key',
      async login(interaction) { logins++; const key = await interaction.prompt({ type: 'secret', message: 'Controlled key' }); if (key === 'fail') throw new Error('synthetic_secret_must_not_reflect'); return { type: 'api_key', key }; },
      async resolve({ credential }) { if (broken) throw new Error('synthetic_secret_must_not_reflect'); return credential?.key || ambient ? { auth: { apiKey: credential?.key ?? 'ambient-test-only' } } : undefined; },
    },
    oauth: {
      name: 'Controlled OAuth',
      async login(interaction, options) {
        logins++; deviceIds.push(options!.getDeviceId!());
        assert.equal(await interaction.prompt({ type: 'text', message: 'Optional domain (blank for default)' }), '');
        interaction.notify({ type: 'auth_url', url: 'javascript:invalid' });
        interaction.notify({ type: 'auth_url', url: 'https://example.com/authorize', instructions: 'Controlled authorization' });
        const option = await interaction.prompt({ type: 'select', message: 'Controlled method', options: [{ id: 'manual', label: 'Manual' }] }); assert.equal(option, 'manual');
        const code = await interaction.prompt({ type: 'manual_code', message: 'Controlled code' });
        return { type: 'oauth', access: code, refresh: 'synthetic-refresh-only', expires: Date.now() + 60_000 };
      },
      async refresh(value) { return value; }, async toAuth(value) { return { apiKey: value.access }; },
    },
  };
  const models = createModels({ credentials }); models.setProvider({ ...faux.provider, auth });
  state.options.models = models; await state.restart();
  const request = (method: 'GET' | 'POST' | 'DELETE' | 'PATCH', path: string, payload?: object) => state.service.app.inject({ method, url: `${api}${path}`, headers: state.headers, ...(payload ? { payload } : {}) });
  const read = (id: string) => async () => (await request('GET', `/models/login/${id}`)).json<ModelLoginView>();
  const start = (id: string, type = 'api_key') => request('POST', '/models/login', { requestId: id, providerId: faux.provider.id, type });
  const respond = (id: string, promptId: number, value: string) => request('POST', `/models/login/${id}/respond`, { promptId, value });
  try {
    assert.equal((await state.service.app.inject(`${api}/models`)).statusCode, 401);
    const catalog = (await request('GET', '/models')).json(); assert.equal(catalog.providers[0].configured, false); assert.deepEqual(catalog.models, []);
    assert.equal((await start('key')).statusCode, 202);
    const key = await waitFor(read('key'), view => !!view.prompt); assert.equal(key.prompt!.type, 'secret');
    assert.equal((await start('key')).statusCode, 202); assert.equal(logins, 1);
    assert.equal((await start('duplicate')).statusCode, 409);
    assert.equal((await request('POST', '/models/login', { requestId: 'key', providerId: 'changed', type: 'api_key' })).statusCode, 409);
    assert.equal((await respond('key', key.prompt!.id, 'synthetic-key-only')).statusCode, 200);
    const completed = await waitFor(read('key'), view => view.status === 'done'); assert.equal(completed.prompt, null); assert.deepEqual(completed.events, []);
    assert.equal((await respond('key', key.prompt!.id, 'synthetic-key-only')).statusCode, 200);
    assert.equal(JSON.stringify(completed).includes('synthetic-key-only'), false);
    assert.equal((await request('GET', '/models')).json().providers[0].configured, true);
    await state.restart(); assert.equal((await request('GET', '/models')).json().providers[0].configured, true);
    await start('empty-secret'); const empty = await waitFor(read('empty-secret'), view => !!view.prompt);
    assert.equal((await respond('empty-secret', empty.prompt!.id, '')).statusCode, 400);
    assert.deepEqual(await credentials.read(faux.provider.id), { type: 'api_key', key: 'synthetic-key-only' });
    assert.equal((await read('empty-secret')()).status, 'running');
    await request('DELETE', '/models/login/empty-secret');
    assert.equal((await request('DELETE', `/models/providers/${faux.provider.id}`)).json().configured, false);
    ambient = true; assert.equal((await request('DELETE', `/models/providers/${faux.provider.id}`)).json().configured, true); ambient = false;
    broken = true; const unavailable = await request('GET', '/models'); assert.equal(unavailable.json().providers[0].error, 'provider_status_unavailable'); assert.equal(unavailable.body.includes('synthetic_secret'), false); broken = false;
    await start('failed'); const failed = await waitFor(read('failed'), view => !!view.prompt); await respond('failed', failed.prompt!.id, 'fail');
    const failure = await waitFor(read('failed'), view => view.status === 'failed'); assert.equal(failure.error, 'model_login_failed'); assert.equal(JSON.stringify(failure).includes('synthetic_secret'), false);
    for (const id of ['oauth-one', 'oauth-two']) {
      await start(id, 'oauth'); const optional = await waitFor(read(id), view => view.prompt?.type === 'text');
      assert.equal((await respond(id, optional.prompt!.id, '')).statusCode, 200);
      const select = await waitFor(read(id), view => view.prompt?.type === 'select');
      assert.equal(select.events.length, 1); assert.equal(select.prompt!.type, 'select');
      assert.equal((await respond(id, select.prompt!.id, 'invalid')).statusCode, 400);
      await respond(id, select.prompt!.id, 'manual');
      const code = await waitFor(read(id), view => view.prompt?.type === 'manual_code');
      assert.equal((await respond(id, 999, 'stale')).statusCode, 409);
      await respond(id, code.prompt!.id, 'synthetic-code-only'); await waitFor(read(id), view => view.status === 'done');
    }
    assert.equal(deviceIds[0], deviceIds[1]); assert.ok(deviceIds[0]);
    await start('cancelled'); await waitFor(read('cancelled'), view => !!view.prompt);
    assert.equal((await request('GET', '/models')).json().providers[0].login.id, 'cancelled');
    assert.equal((await request('DELETE', '/models/login/cancelled')).json().status, 'cancelled');
    await request('DELETE', '/models/login/late'); assert.equal((await start('late')).statusCode, 409);
    const facts = JSON.stringify(await state.service.research.harness.snapshot(Application, context));
    for (const value of ['synthetic-key-only', 'synthetic-code-only', 'synthetic-refresh-only']) assert.equal(facts.includes(value), false);
    await start('shutdown'); await waitFor(read('shutdown'), view => !!view.prompt);
    await state.restart(); assert.equal((await request('GET', '/models/login/shutdown')).statusCode, 404);
  } finally { await state.cleanup(); await credentials.close(); }
});

test('official OpenAI subscription catalog supports max reasoning', () => {
  const model = openaiCodexProvider().getModels().find(model => model.id === 'gpt-6.1-sol')!;
  assert.ok(model);
  assert.ok(getSupportedThinkingLevels(model).includes('max'));
});

test('settings retain supported reasoning and service tier before admission/replay', { timeout: 20_000 }, async () => {
  const state = await setup(); const model = state.faux.getModel(); model.reasoning = true;
  const patch = (payload: object) => state.service.app.inject({ method: 'PATCH', url: `${api}/settings`, headers: state.headers, payload });
  try {
    await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).settings = json({ name: 'Historical', defaultModel: state.model, maxTrials: 77, taskTimeoutSeconds: 0.001, nativeTimeoutSeconds: 0.001 }); }, context);
    assert.deepEqual(await state.service.work.getSettings(), { defaultModel: state.model, thinkingLevel: null, serviceTier: null });
    for (const field of ['name', 'maxTrials', 'taskTimeoutSeconds', 'nativeTimeoutSeconds']) assert.equal((await patch({ [field]: 1 })).json().error, 'settings_field_not_mutable');
    assert.equal((await patch({ defaultModel: state.model, thinkingLevel: 'unsupported' })).json().error, 'thinking_level_unsupported');
    assert.equal((await patch({ serviceTier: 'fast' })).json().error, 'invalid_service_tier');
    assert.equal((await patch({ serviceTier: 'priority' })).json().error, 'service_tier_unsupported');
    const unsupported = { requestId: 'unsupported-tier', goal: 'Controlled model protocol', model: state.model, datasetId: 'controlled', maxTrials: 1, serviceTier: 'priority' };
    assert.equal((await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: unsupported })).json().error, 'service_tier_unsupported');
    assert.equal((await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: { ...unsupported, serviceTier: 'fast' } })).json().error, 'invalid_service_tier');
    assert.equal((await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: { ...unsupported, serviceTier: null } })).json().error, 'invalid_service_tier');
    assert.equal((await patch({ defaultModel: { provider: 'missing', modelId: 'missing' } })).statusCode, 409);
    model.api = 'openai-codex-responses';
    assert.deepEqual((await state.service.app.inject({ url: `${api}/models`, headers: state.headers })).json().models[0].serviceTiers, ['default', 'priority']);
    assert.equal((await patch({ defaultModel: state.model, thinkingLevel: 'high', serviceTier: 'priority' })).statusCode, 200);
    const { input, receipt } = await studied(state, 'reasoning-frozen');
    const before = await state.service.research.view(receipt.conversationId); assert.equal(before.research!.input!.thinkingLevel, 'high');
    assert.equal(before.research!.input!.serviceTier, 'priority');
    assert.equal((await (await state.service.research.harness.conversation(receipt.conversationId, context))!.agent(context)).thinkingLevel, 'high');
    state.faux.setResponses([fauxAssistantMessage('done')]);
    const explicit = await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: { ...input, requestId: 'explicit-tier', serviceTier: 'default' } });
    assert.equal(explicit.statusCode, 202, explicit.body);
    await (await state.service.research.harness.submission(explicit.json().submissionId, context))!.wait(context);
    assert.equal((await state.service.research.view(explicit.json().conversationId)).research!.input!.serviceTier, 'default');
    await patch({ thinkingLevel: 'low', serviceTier: 'default' }); await state.restart();
    const replay = await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: input }); assert.deepEqual(replay.json(), receipt);
    assert.equal((await state.service.research.view(receipt.conversationId)).research!.input!.thinkingLevel, 'high');
    assert.equal((await state.service.research.view(receipt.conversationId)).research!.input!.serviceTier, 'priority');
    assert.equal((await state.service.research.harness.snapshot(Application, context))!.settings!.nativeTimeoutSeconds, 0.001);
    const { view } = await import('./support.ts').then(module => module.built(state)); assert.equal(view.status, 'COMPUTED', view.error ?? '');
    assert.equal((await patch({ defaultModel: null, thinkingLevel: null })).json().error, 'model_required');
    assert.equal((await patch({ defaultModel: null, thinkingLevel: null, serviceTier: null })).statusCode, 200);
    assert.deepEqual(await state.service.work.getSettings(), { defaultModel: null, thinkingLevel: null, serviceTier: null });
  } finally { await state.cleanup(); }
});
