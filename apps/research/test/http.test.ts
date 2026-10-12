import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { request as httpRequest } from 'node:http';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
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
    for (const number of ['9007199254740992', '9007199254740993', '1e999']) {
      const invalid = await fetch(`${origin}/research`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: `{"requestId":"lossy-${number}","input":${JSON.stringify(input).replace('"data":{}', `"data":{"nested":[${number}]}`)}}` });
      assert.equal(invalid.status, 400);
      assert.deepEqual(await invalid.json(), { error: 'invalid_json_number' });
    }
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

test('maintenance closes new admissions, drains an admitted request and keeps native tasks observable without cancellation', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-research-maintenance-'));
  const faux = fauxProvider({ models: [{ id: 'maintenance' }], tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  let toolEntered!: () => void; let releaseTool!: () => void;
  const toolEntering = new Promise<void>((done) => { toolEntered = done; });
  const toolReleased = new Promise<void>((done) => { releaseTool = done; });
  const extension = defineExtension({ name: 'maintenance-probe', tools: [defineTool({
    name: 'maintenance_probe', description: 'Protocol-only active native task.', parameters: Type.Object({}), replay: 'unsafe',
    async execute() { toolEntered(); await toolReleased; return {}; },
  })] });
  faux.setResponses([fauxAssistantMessage(fauxToolCall('maintenance_probe', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Maintenance protocol completed.')]);
  const host = await openResearch({ database: join(directory, 'durable.sqlite'), models, extensions: [extension] });
  let entered!: () => void; let release!: () => void;
  const entering = new Promise<void>((done) => { entered = done; });
  const released = new Promise<void>((done) => { release = done; });
  const originalStart = host.start.bind(host);
  host.start = async (...args) => { entered(); await released; return originalStart(...args); };
  const server = researchServer(host);
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  const origin = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const input = { goal: 'Maintenance protocol', model: { provider: faux.provider.id, modelId: 'maintenance' }, data: {}, policy: {} };
  const post = (key: string) => fetch(`${origin}/research`, { method: 'POST', body: JSON.stringify({ requestId: key, input }) });
  let delayed: ReturnType<typeof httpRequest> | undefined;
  try {
    const admitted = post('before-close');
    await entering;
    // An incomplete body that arrived before closure must not start afterward.
    const delayedResponse = new Promise<number>((done, reject) => {
      delayed = httpRequest(`${origin}/research`, { method: 'POST', headers: { 'Transfer-Encoding': 'chunked' } }, (response) => { response.resume(); response.on('end', () => done(response.statusCode!)); });
      delayed.on('error', reject);
      delayed.write('{"requestId":"slow-body","input":');
    });
    const closed = new Promise<void>((done) => server.on('request', (request) => { if (request.url === '/maintenance/quiesce') done(); }));
    let settled = false;
    const quiesced = fetch(`${origin}/maintenance/quiesce`, { method: 'POST' }).then((response) => { settled = true; return response; });
    await closed;
    const rejected = await post('after-close');
    assert.equal(rejected.status, 503);
    assert.deepEqual(await rejected.json(), { error: 'research_maintenance' });
    assert.equal(settled, false);
    delayed!.end(`${JSON.stringify(input)}}`);
    assert.equal(await delayedResponse, 503);
    assert.equal((await fetch(`${origin}/tasks`)).status, 200);
    release();
    const receipt = await (await admitted).json();
    assert.equal((await quiesced).status, 200);
    await toolEntering;
    assert.ok((await (await fetch(`${origin}/tasks`)).json()).tasks.length > 0);
    releaseTool();
    await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
    assert.equal((await post('still-closed')).status, 503);
    assert.equal((await (await fetch(`${origin}/research`)).json()).items.length, 1);
    assert.equal((await fetch(`${origin}/maintenance/resume`, { method: 'POST' })).status, 200);
    assert.deepEqual(await (await post('before-close')).json(), receipt);
    assert.equal(faux.state.callCount, 2);
  } finally {
    release(); releaseTool(); delayed?.destroy();
    await new Promise<void>((done, reject) => server.close((error) => error ? reject(error) : done()));
    await host.close(); await rm(directory, { recursive: true, force: true });
  }
});
