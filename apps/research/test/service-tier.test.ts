import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import * as zlib from 'node:zlib';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import type { Provider } from '@earendil-works/pi-ai/models';
import { openaiProvider } from '@earendil-works/pi-ai/providers/openai';
import { openaiCodexProvider } from '@earendil-works/pi-ai/providers/openai-codex';
import { openResearch } from '../src/host.ts';
import research from '../plugins/research.ts';

const context = BACKGROUND_CONTEXT;
const input = (goal: string) => ({ goal, model: { provider: 'openai-codex', modelId: 'gpt-6.1-sol' }, thinkingLevel: 'max' as const, data: {}, policy: { max_trials: 3 } });
type RequestBody = { instructions: string; service_tier?: string; reasoning: { effort: string }; prompt_cache_key: string; composition_marker?: string };

function controlledProvider(provider: Provider = openaiCodexProvider()) {
  const requests: RequestBody[] = [];
  let fail = false;
  let summary = false;
  const token = `synthetic.${Buffer.from(JSON.stringify({ 'https://api.openai.com/auth': { chatgpt_account_id: 'synthetic-account' } })).toString('base64')}.synthetic`;
  const models = createModels();
  models.setProvider({ ...provider,
    auth: { apiKey: { name: 'Synthetic request auth', async resolve() { return { auth: { apiKey: token } }; } } },
    streamSimple(model, transcript, options) {
      return provider.streamSimple(model, transcript, { ...options, transport: 'sse', fetch: async (_url, init) => {
        const headers = new Headers(init?.headers);
        const body = headers.get('content-encoding') === 'zstd'
          ? zlib.zstdDecompressSync(init!.body as Uint8Array).toString()
          : String(init!.body);
        requests.push(JSON.parse(body));
        if (fail) return new Response(JSON.stringify({ error: { message: '401 unauthorized: synthetic rejection' } }), { status: 401 });
        const item = summary
          ? { type: 'message', id: `msg_${requests.length}`, role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: 'Original study and model choices retained.', annotations: [] }] }
          : { type: 'function_call', id: `fc_${requests.length}`, call_id: `call_${requests.length}`, name: 'research_finish', arguments: JSON.stringify({ reason: 'Request serialization check completed.' }), status: 'completed' };
        const events = [
          { type: 'response.output_item.done', output_index: 0, item },
          { type: 'response.completed', response: { id: `resp_${requests.length}`, status: 'completed', output: [item] } },
        ];
        return new Response(events.map(event => `data: ${JSON.stringify(event)}\n\n`).join(''), { headers: { 'content-type': 'text/event-stream' } });
      } });
    },
  });
  return { models, requests, fail(value: boolean) { fail = value; }, summarize() { summary = true; } };
}

test('official Codex serializes max and per-study Fast choices without mixing concurrent research', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-service-tier-'));
  const controlled = controlledProvider();
  const other = fauxProvider({ provider: 'non-codex', models: [{ id: 'other' }] }); controlled.models.setProvider(other.provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models: controlled.models, extensions: [research] });
  try {
    const available = await host.availableModels();
    const codex = available.find(model => model.provider === 'openai-codex' && model.modelId === 'gpt-6.1-sol')!;
    assert.ok(codex.thinkingLevels.includes('max'));
    assert.deepEqual(codex.serviceTiers, ['default', 'priority']);
    assert.deepEqual(available.find(model => model.provider === 'non-codex')!.serviceTiers, []);
    const choices = [{ ...input('priority-study'), serviceTier: 'priority' }, { ...input('standard-study'), serviceTier: 'default' }, input('unspecified-study')] as const;
    const receipts = await Promise.all(choices.map(value => host.start(value.goal, value)));
    for (const receipt of receipts) assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    assert.equal(controlled.requests.length, choices.length);
    for (const choice of choices) {
      const request = controlled.requests.find(body => body.instructions.includes(choice.goal))!;
      assert.ok(request);
      assert.equal(request.reasoning.effort, 'max');
      assert.equal(request.service_tier, 'serviceTier' in choice ? choice.serviceTier : undefined);
    }
    assert.equal(new Set(controlled.requests.map(body => body.prompt_cache_key)).size, choices.length);
    const priorityRequest = controlled.requests.find(body => body.service_tier === 'priority')!;
    let transformed = 0;
    const result = await host.models.streamSimple(host.models.getModel('openai-codex', 'gpt-6.1-sol')!, { messages: [{ role: 'user', content: 'Check public payload hook composition.', timestamp: Date.now() }] }, {
      sessionId: priorityRequest.prompt_cache_key, reasoning: 'max',
      onPayload(payload) { transformed++; return { ...payload as object, composition_marker: 'preserved', service_tier: 'default' }; },
    }).result();
    assert.equal(result.stopReason, 'toolUse');
    assert.equal(transformed, 1);
    assert.equal(controlled.requests.at(-1)!.composition_marker, 'preserved');
    assert.equal(controlled.requests.at(-1)!.service_tier, 'priority');
    await assert.rejects(host.start('ultra', { ...input('ultra'), thinkingLevel: 'ultra' } as never), /invalid_thinking_level/);
    await assert.rejects(host.start('unsupported-max', { ...input('unsupported max'), model: { provider: 'openai-codex', modelId: 'gpt-5.3-codex-spark' } }), /thinking_level_unsupported/);
    await assert.rejects(host.start('unsupported-fast', { ...input('unsupported Fast'), model: { provider: other.provider.id, modelId: 'other' }, thinkingLevel: 'off', serviceTier: 'priority' }), /service_tier_unsupported/);
    assert.equal(controlled.requests.length, choices.length + 1);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('reopening a failed original Codex study keeps Fast and max on the recovered submission', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-service-tier-resume-'));
  const controlled = controlledProvider(); const database = join(directory, 'research.sqlite');
  const options = { database, models: controlled.models, extensions: [research] };
  let host = await openResearch(options);
  try {
    controlled.fail(true);
    const original = { ...input('recover-priority'), serviceTier: 'priority' as const };
    const receipt = await host.start('recover-priority', original);
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'unanswered');
    assert.equal(controlled.requests.length, 1);
    await host.close(); controlled.fail(false);
    host = await openResearch({ ...options, thinkingLevel: 'low' });
    assert.deepEqual(await host.start('recover-priority', original), receipt);
    const resumed = await host.resume(receipt.conversationId, receipt.submissionId);
    assert.equal((await (await host.harness.submission(resumed.submissionId, context))!.wait(context)).status, 'done');
    assert.equal(controlled.requests.length, 2);
    assert.deepEqual(controlled.requests.map(body => body.service_tier), ['priority', 'priority']);
    assert.deepEqual(controlled.requests.map(body => body.reasoning.effort), ['max', 'max']);
    assert.equal(controlled.requests[0]!.prompt_cache_key, controlled.requests[1]!.prompt_cache_key);
    assert.deepEqual((await host.view(receipt.conversationId)).research!.input, original);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('official OpenAI Responses requests Fast for subscription auth, including compaction immediately after reopening', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-openai-fast-'));
  const controlled = controlledProvider(openaiProvider());
  const options = { database: join(directory, 'research.sqlite'), models: controlled.models, extensions: [research], settings: { compaction: { keepRecentTokens: 0 } } };
  let host = await openResearch(options);
  try {
    assert.deepEqual((await host.availableModels('openai')).find(model => model.modelId === 'gpt-6.1-sol')!.serviceTiers, ['default', 'priority']);
    const receipt = await host.start('openai-fast', { ...input('OpenAI subscription Fast'), model: { provider: 'openai', modelId: 'gpt-6.1-sol' }, serviceTier: 'priority' });
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    assert.equal(controlled.requests.length, 1);
    assert.equal(controlled.requests[0]!.service_tier, 'priority');
    assert.equal(controlled.requests[0]!.reasoning.effort, 'max');
    await host.close(); controlled.summarize();
    host = await openResearch(options);
    const conversation = (await host.harness.conversation(receipt.conversationId, context))!;
    const compacted = await host.harness.waitForTask(await conversation.compact(undefined, context), context);
    assert.equal(compacted.state.outcome.status, 'completed');
    assert.equal(controlled.requests.length, 2);
    assert.equal(controlled.requests[1]!.service_tier, 'priority');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
