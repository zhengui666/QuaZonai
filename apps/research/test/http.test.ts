import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createModels, fauxProvider, fauxAssistantMessage } from '@earendil-works/pi-ai';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { openResearch } from '../src/host.ts';
import { researchServer } from '../src/http.ts';

test('real HTTP preserves native identity and truthful admitted/conflict/invalid/not-found states', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-research-http-'));
  const faux = fauxProvider({ models: [{ id: 'http' }], tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage('Protocol test completed; no scientific claim.')]);
  const host = await openResearch({ database: join(directory, 'durable.sqlite'), models });
  const server = researchServer(host);
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  const address = server.address() as { port: number };
  const origin = `http://127.0.0.1:${address.port}`;
  const input = { goal: 'HTTP protocol check', model: { provider: faux.provider.id, modelId: 'http' }, data: {}, policy: {} };
  const post = (body: unknown) => fetch(`${origin}/research`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  try {
    assert.equal((await fetch(`${origin}/health/live`)).status, 200);
    const available = await (await fetch(`${origin}/models`)).json();
    assert.equal(available[0].modelId, 'http');
    const result = await post({ requestId: 'original', input });
    assert.equal(result.status, 202);
    const receipt = await result.json();
    await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
    const replay = await post({ requestId: 'original', input });
    assert.equal(replay.status, 202);
    assert.deepEqual(await replay.json(), receipt);
    assert.equal((await post({ requestId: 'original', input: { ...input, goal: 'different' } })).status, 409);
    assert.equal((await post({ requestId: 'invalid', input: { ...input, policy: { max_trials: -1 } } })).status, 400);
    assert.equal((await post({ input: null })).status, 400);
    const view = await (await fetch(`${origin}/research/${receipt.conversationId}`)).json();
    assert.deepEqual(view.research.input, input);
    assert.equal((await fetch(`${origin}/research/999999`)).status, 404);
    assert.equal((await fetch(`${origin}/research/no-id`)).status, 404);
    assert.equal((await fetch(`${origin}/research/${receipt.conversationId}/stop`, { method: 'POST' })).status, 202);
    const page = await (await fetch(`${origin}/research`)).json();
    assert.equal(page.items.length, 1);
    assert.ok(Array.isArray((await (await fetch(`${origin}/tasks`)).json()).tasks));
    assert.ok(await (await fetch(`${origin}/usage`)).json());
    assert.equal(faux.state.callCount, 1);
  } finally {
    await new Promise<void>((done, reject) => server.close((error) => error ? reject(error) : done()));
    await host.close();
    await rm(directory, { recursive: true, force: true });
  }
});
