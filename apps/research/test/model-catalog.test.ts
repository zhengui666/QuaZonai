import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { ModelCredentials } from '../src/credentials.ts';
import { createModels } from '@earendil-works/pi-ai';
import { openaiProvider } from '@earendil-works/pi-ai/providers/openai';
import { discoveredOpenAIProvider, readCatalog, thinkingLevels, defaultThinking, unavailableThinkingLevels, reasoningModes, resolveReasoning } from '../src/model-catalog.ts';
import { subscriptionSpeed } from '../src/models.ts';
const entry = (slug: string, levels = ['low', 'max', 'ultra']) => ({ slug, display_name: slug, supported_reasoning_levels: levels.map(effort => ({ effort })), default_reasoning_level: 'low', context_window: 272000, service_tiers: [{ id: 'priority' }] });

test('official provider refresh replaces catalog and discovers new IDs and Ultra from metadata', async () => {
  const original = openaiProvider();
  let body = { models: [entry('new-model')] }; let fail = false; let calls = 0;
  const provider = discoveredOpenAIProvider(original, async () => { calls++; return new Response(JSON.stringify(body), { status: fail ? 503 : 200 }); });
  const credential = { type: 'oauth' as const, access: 'test', refresh: 'test', expires: Date.now() + 60000 };
  const refresh = () => provider.refreshModels!({ allowNetwork: true, force: true, credential, signal: new AbortController().signal, publish: async publication => { publication.update?.(); return true; } });
  await refresh();
  assert.deepEqual(provider.getModels().map(model => model.id), ['new-model']);
  assert.deepEqual(thinkingLevels(provider.getModels()[0]!), ['low', 'max']);
  assert.deepEqual(reasoningModes(provider.getModels()[0]!), [{ level: 'ultra', effectiveLevel: 'max', mode: 'PI_MULTI_AGENT' }]);
  assert.deepEqual(unavailableThinkingLevels(provider.getModels()[0]!), []);
  assert.equal(defaultThinking(provider.getModels()[0]!, 'medium'), 'low');
  body = { models: [entry('replacement', ['low', 'max'])] }; await refresh();
  assert.deepEqual(provider.getModels().map(model => model.id), ['replacement']);
  assert.ok(!thinkingLevels(provider.getModels()[0]!).includes('ultra'));
  fail = true; await assert.rejects(refresh, /model_catalog_http_503/);
  assert.deepEqual(provider.getModels().map(model => model.id), ['replacement']);
  assert.equal(calls, 3);
});

test('literal provider Ultra is rejected; the collaboration mode must resolve before streaming', async () => {
  const original = openaiProvider();
  const model = readCatalog({ models: [entry('new-unpriced-model')] }, original)[0]!;
  const payloads: Record<string, any>[] = [];
  const models = createModels();
  models.setProvider({ ...original, getModels: () => [model], getAllModels: () => [model], auth: { apiKey: { name: 'Request serialization test', resolve: async () => ({ auth: { apiKey: 'sk-test' } }) } }, streamSimple(model, transcript, options) {
    return original.streamSimple(model as import('@earendil-works/pi-ai').Model<'openai-responses'>, transcript, { ...options, fetch: async (_url, init) => {
      payloads.push(JSON.parse(String(init?.body)));
      return new Response('data: {"type":"response.completed","response":{"id":"test","status":"completed","output":[]}}\n\n', { headers: { 'content-type': 'text/event-stream' } });
    } });
  } });
  let unpriced = 0;
  const wrapped = subscriptionSpeed(models, undefined, async () => { unpriced++; }).models;
  assert.throws(() => wrapped.completeSimple(model, { messages: [] }, { reasoning: 'ultra' as any }), /thinking_level_unsupported/);
  await wrapped.completeSimple(model, { messages: [] }, { reasoning: 'low' });
  assert.equal(payloads.length, 1);
  assert.equal(payloads[0]!.reasoning.effort, 'low');
  assert.equal(payloads[0]!.max_output_tokens, undefined);
  await wrapped.completeSimple(model, { messages: [] }, { reasoning: 'low', maxTokens: 128 });
  assert.equal(payloads[1]!.max_output_tokens, 128);
  assert.equal(unpriced, 2);
});

test('model response prompts are ignored and hidden/unsupported models omitted', () => {
  const provider = openaiProvider();
  assert.deepEqual(readCatalog({ models: [{ ...entry('hidden'), visibility: 'hide' }, { ...entry('unsupported'), supported_in_api: false }, { ...entry('allowed'), model_messages: { instructions: 'untrusted' } }] }, provider).map(model => model.id), ['allowed']);
});

test('catalog refresh caches briefly but changes credentials and logout invalidate it', async () => {
  let calls = 0;
  const provider = discoveredOpenAIProvider(openaiProvider(), async () => { calls++; return Response.json({ models: [entry(`model-${calls}`)] }); });
  const refresh = (access?: string, force = false) => provider.refreshModels!({ allowNetwork: true, force, credential: access ? { type: 'oauth', access, refresh: 'test', expires: Date.now() + 60000 } : undefined, signal: new AbortController().signal, publish: async publication => { publication.update?.(); return true; } });
  await refresh('first'); await refresh('first');
  assert.equal(calls, 1);
  await refresh('second'); assert.equal(calls, 2);
  await refresh('second', true); assert.equal(calls, 3);
  await refresh(); assert.deepEqual(provider.getModels(), []);
  await refresh('second'); assert.equal(calls, 4);
});

test('discovery rejects unusable contracts instead of inventing runtime reasoning support', () => {
  const provider = openaiProvider();
  assert.throws(() => readCatalog({ models: [{ ...entry('bad'), context_window: 0 }] }, provider), /model_catalog_invalid/);
  assert.throws(() => readCatalog({ models: [{ ...entry('ultra-only', ['ultra']), default_reasoning_level: 'ultra' }] }, provider), /model_catalog_runtime_unsupported/);
  assert.deepEqual(readCatalog({ data: [{ id: 'unknown-without-capabilities' }] }, provider), []);
});


test('official model store restores new IDs and capabilities after restart with discovery offline', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-model-store-'));
  let credentials = await ModelCredentials.open(directory);
  try {
    await credentials.modify('openai', async () => ({ type: 'api_key', key: 'test' }));
    let models = createModels({ credentials, modelsStore: credentials.modelsStore });
    models.setProvider(discoveredOpenAIProvider(openaiProvider(), async () => Response.json({ models: [entry('future-model')] })));
    assert.equal((await models.refresh({ allowNetwork: true })).errors.size, 0);
    assert.equal(models.getModel('openai', 'future-model')?.id, 'future-model');
    await credentials.close(); credentials = await ModelCredentials.open(directory);
    models = createModels({ credentials, modelsStore: credentials.modelsStore });
    models.setProvider(discoveredOpenAIProvider(openaiProvider(), async () => { throw new Error('offline'); }));
    const result = await models.refresh({ allowNetwork: true });
    assert.ok(result.errors.has('openai'));
    const restored = models.getModel('openai', 'future-model')!;
    assert.ok(restored);
    assert.deepEqual(thinkingLevels(restored), ['low', 'max']);
    assert.equal(reasoningModes(restored)[0]!.level, 'ultra');
    assert.equal(defaultThinking(restored, 'medium'), 'low');
    const saved = await credentials.modelsStore.read('openai');
    assert.equal(saved?.models.length, 1);
    assert.equal(JSON.stringify(saved).includes('test'), false);
  } finally { await credentials.close(); await rm(directory, { recursive: true, force: true }); }
});


test('Ultra resolves the catalog collaboration effort separately from the user selection', () => {
  const provider = openaiProvider();
  const model = (levels: string[], preferred?: string) => readCatalog({ models: [{ ...entry('future', levels), multi_agent_reasoning_effort: preferred }] }, provider)[0]!;
  assert.deepEqual(resolveReasoning(model(['low', 'xhigh', 'max', 'ultra'], 'xhigh'), 'ultra'), { requestedLevel: 'ultra', effectiveLevel: 'xhigh', mode: 'PI_MULTI_AGENT' });
  assert.equal(resolveReasoning(model(['low', 'max', 'ultra'], 'high'), 'ultra').effectiveLevel, 'max');
  assert.equal(resolveReasoning(model(['low', 'high', 'ultra'], 'ultra'), 'ultra').effectiveLevel, 'high');
  assert.equal(defaultThinking(model(['low', 'max']), 'ultra'), 'ultra');
  assert.throws(() => resolveReasoning(model(['low', 'max']), defaultThinking(model(['low', 'max']), 'ultra')), /thinking_level_unsupported/);
  assert.equal(defaultThinking(model(['low', 'max', 'ultra']), 'ultra'), 'ultra');
});
