import { obj } from '@earendil-works/pi-schema';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, MemoryStorage } from '@earendil-works/pi-durable';
import type { NativeExecutionRequest, NativeJson, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { canonical } from '../src/documents.ts';
import { executionId } from '../../nautilus/src/server.ts';
import { defaultNativeTarget, executeNativeValue, nativeCommands, nativeTargetForData, NativeRemoteUncertainty } from '../src/native-execution.ts';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';

const digest = `sha256:${'a'.repeat(64)}`;
async function body(request: IncomingMessage): Promise<NativeExecutionRequest> {
  const chunks: Buffer[] = []; for await (const chunk of request) chunks.push(Buffer.from(chunk));
  return JSON.parse(Buffer.concat(chunks).toString('utf8'));
}
function reply(response: ServerResponse, value: unknown) { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify(value)); }
async function worker(handler: (request: IncomingMessage, response: ServerResponse) => Promise<void> | void) {
  const server = createServer((request, response) => {
    if (request.url === '/v1/capabilities') { reply(response, { schemaVersion: 1, implementationDigest: digest, commands: [...nativeCommands] }); return; }
    Promise.resolve(handler(request, response)).catch(error => { response.statusCode = 500; response.end(String(error)); });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'controlled-protocol-source', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const host = await openResearch({ database: '/tmp/qz-transport-memory.sqlite', models, storage: new MemoryStorage() });
  return { target, host, execute: (command: string, input: NativeJson, args: string[], directory: string, context: Parameters<typeof executeNativeValue>[5]) => host.withNativeHost(() => executeNativeValue(target, command, input, args, directory, context)), close: async () => { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } };
}

// Transport-only peer; original payload precision and receipt identity are preserved.
test('remote science keeps original payloads, shared catalog paths and distinct invocation receipts', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-remote-native-')); const requests: NativeExecutionRequest[] = [];
  const source = await worker(async (request, response) => {
    assert.equal(request.method, 'POST'); assert.equal(request.url, '/v1/execute');
    const admitted = await body(request); requests.push(admitted);
    const { requestId, ...original } = admitted;
    assert.equal(requestId, createHash('sha256').update(canonical(original as unknown as NativeJson)).digest('hex'));
    assert.equal(requestId, executionId(original), 'client and worker preserve the original wire identity');
    reply(response, { schemaVersion: 1, requestId, status: 'COMPLETED', result: { native: '受控协议', input: admitted.input } });
  });
  try {
    const args = ['--catalog', '/shared/catalog'];
    const input = { amount: '9007199254740993123', rows: Array.from({ length: 2048 }, (_, index) => ({
      clock: '1790000000000000001', index, label: '原生🐋',
    })) };
    for (let i = 0; i < 2; i++) assert.deepEqual(await source.execute('simulate', input, args, directory, BACKGROUND_CONTEXT), { native: '受控协议', input });
    assert.notEqual(requests[0]!.requestId, requests[1]!.requestId, 'changed bytes at an owner-managed catalog path cannot be replaced by an old invocation');
    assert.deepEqual(requests[0]!.args, args);
    assert.deepEqual(requests[0]!.files, {});
  } finally { await source.close(); await rm(directory, { recursive: true, force: true }); }
});

test('lost POST response reconciles the original receipt without submitting another operation', async () => {
  let original: NativeExecutionRequest | undefined; let posts = 0; let reads = 0;
  const source = await worker(async (request, response) => {
    if (request.method === 'POST') { posts++; original = await body(request); request.socket.destroy(); return; }
    reads++; assert.equal(request.url, `/v1/executions/${original!.requestId}`);
    reply(response, { schemaVersion: 1, requestId: original!.requestId, status: 'COMPLETED', result: ['BTC.TEST'] });
  });
  try {
    assert.deepEqual(await source.execute('catalog-factor-scope', {}, ['--catalog', '/shared/data'], '/unused', BACKGROUND_CONTEXT), ['BTC.TEST']);
    assert.equal(posts, 1); assert.equal(reads, 1);
  } finally { await source.close(); }
});

test('cancelling an observation leaves native work owned until explicit task cancellation confirms its receipt', async () => {
  for (const cancellationStatus of ['CANCELLED', 'UNKNOWN']) {
    const controller = new AbortController(); let original: NativeExecutionRequest | undefined; let cancelled = false;
    const source = await worker(async (request, response) => {
      if (request.method === 'POST') { original = await body(request); controller.abort(new Error('owner_stop')); return; }
      assert.equal(request.method, 'DELETE'); assert.equal(request.url, `/v1/executions/${original!.requestId}`); cancelled = true;
      reply(response, { schemaVersion: 1, requestId: original!.requestId, status: cancellationStatus, ...(cancellationStatus === 'UNKNOWN' ? { error: 'WORKER_INTERRUPTED' } : {}) });
    });
    try {
      await assert.rejects(source.execute('simulate', {}, [], '/unused', withAbortSignal(controller.signal, BACKGROUND_CONTEXT)), /owner_stop/);
      assert.equal(cancelled, false, 'cancelling a wait does not cancel the task');
      const task = (await source.host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items[0]!;
      await source.host.harness.abortTask(task.id, BACKGROUND_CONTEXT);
      await source.host.harness.waitForTask(task.id, BACKGROUND_CONTEXT);
      assert.equal(cancelled, true);
      const receipt = await source.host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT);
      assert.equal(receipt!.requests[0]!.status, cancellationStatus);
    } finally { await source.close(); }
  }
});

test('admitted instance survives environment changes and production refuses missing or local task bindings', async () => {
  const before = { remote: process.env.QZ_NAUTILUS_BACKTEST, local: process.env.QZ_JOB_BINARY, required: process.env.QZ_REQUIRE_REMOTE_NAUTILUS };
  try {
    delete process.env.QZ_NAUTILUS_BACKTEST; delete process.env.QZ_JOB_BINARY; delete process.env.QZ_REQUIRE_REMOTE_NAUTILUS;
    assert.throws(defaultNativeTarget, /nautilus_backtest_not_configured/);
    const admitted = { id: 'original-backtest', url: 'https://original.example' };
    process.env.QZ_NAUTILUS_BACKTEST = JSON.stringify({ id: 'new-backtest', url: 'https://new.example' });
    process.env.QZ_REQUIRE_REMOTE_NAUTILUS = '1';
    assert.deepEqual(nativeTargetForData({ nautilusTarget: admitted }), admitted);
    assert.throws(() => nativeTargetForData({}), /local_nautilus_target_disabled/);
    await assert.rejects(executeNativeValue('/old/binary', 'simulate', {}, [], '/unused', BACKGROUND_CONTEXT), /local_nautilus_target_disabled/);
  } finally {
    for (const [name, value] of [['QZ_NAUTILUS_BACKTEST', before.remote], ['QZ_JOB_BINARY', before.local], ['QZ_REQUIRE_REMOTE_NAUTILUS', before.required]]) {
      if (value === undefined) delete process.env[name!]; else process.env[name!] = value;
    }
  }
});

test('bounded cancellation retains its unknown original remote receipt after Durable abort and reopen', { timeout: 15_000 }, async t => {
  const actualTimeout = AbortSignal.timeout.bind(AbortSignal); const deadlines: number[] = [];
  // Exercise the real timeout branch with a short clock, not a 15-second wait.
  t.mock.method(AbortSignal, 'timeout', (milliseconds: number) => { deadlines.push(milliseconds); return actualTimeout(milliseconds === 15_000 ? 100 : milliseconds); });
  const directory = await mkdtemp(join(tmpdir(), 'qz-remote-stop-receipt-'));
  let accepted!: () => void; const entered = new Promise<void>(resolve => { accepted = resolve; });
  let original: NativeExecutionRequest | undefined; let cancels = 0; let posts = 0; const cancelPaths: string[] = [];
  const source = await worker(async (request, _response) => {
    if (request.method === 'POST') { posts++; original = await body(request); accepted(); return; }
    assert.equal(request.method, 'DELETE'); assert.equal(request.url, `/v1/executions/${original!.requestId}`);
    cancels++; cancelPaths.push(request.url!); // Deliberately no response: remote termination remains unknown.
  });
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('compute', {}), { stopReason: 'toolUse' })]);
  const data = { catalog: directory, nautilusTarget: source.target,
    request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: {} } } };
  const compute = defineExtension({ name: 'qz.research', tools: [defineTool({
    name: 'compute', description: 'Controlled remote cancellation protocol', parameters: obj({}), replay: 'safe',
    async execute(_args, api, context) { const id = await api.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request: {}, catalog: directory, target: source.target }, { ownership: { kind: 'task', taskId: api.taskId } }), context); await api.waitForTask(id, context); return {}; },
  })] });
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [compute] };
  let host = await openResearch(options);
  try {
    const start = await host.start('original-remote-stop', { goal: 'Retain unknown original receipt', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: {} });
    await entered; await host.stop(start.conversationId);
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native-strategy.backtest' }, 10), BACKGROUND_CONTEXT);
    assert.equal(tasks.items.length, 1); const task = tasks.items[0]!;
    assert.equal(task.state.outcome?.status, 'aborted'); assert.equal(posts, 1); assert.ok(cancels >= 1); assert.ok(cancelPaths.every(path => path === `/v1/executions/${original!.requestId}`));
    assert.ok(deadlines.includes(15_000));
    const facts = await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT);
    assert.deepEqual(facts, { requests: [{ taskId: task.id, requestId: original!.requestId, target: source.target, status: 'UNKNOWN', error: 'nautilus_cancellation_unknown' }] });
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT), facts);
    assert.equal(posts, 1, 'reading or reopening uncertainty never submits another remote execution');
  } finally { await host.close(); await source.close(); await rm(directory, { recursive: true, force: true }); }
});
