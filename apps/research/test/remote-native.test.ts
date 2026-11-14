import { obj } from '@earendil-works/pi-schema';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineDocFamily, defineExtension, defineTool, MemoryStorage, type JsonObject } from '@earendil-works/pi-durable';
import type { NativeExecutionRequest, NativeJson, RemoteNativeTarget, ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { canonical } from '../src/documents.ts';
import { executionId } from '../../nautilus/src/server.ts';
import { defaultNativeTarget, executeNativeValue, nativeCommands, nativeTargetForData, NativeRemoteUncertainty, NativeExecutionTask, remoteNativeExecutionReceipt } from '../src/native-execution.ts';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';

import { readNativeEvidence } from '../plugins/native-evidence.ts';
import { canonicalJsonDigest } from '../src/native-json.ts';
import { reconcileNativeRemoteTasks, assertNativeRemoteTaskSettled } from '../src/native-remote-receipts.ts';
const Calls = defineDocFamily<any, null>({ kind: 'qz.native-call', version: 1, scope: 'task', family: true, initial: () => ({ taskId: null }), checkpointWhen: () => true });
const digest = `sha256:${'a'.repeat(64)}`;
function reply(response: ServerResponse, value: unknown) { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify(value)); }
async function worker(handler: (request: IncomingMessage, response: ServerResponse) => Promise<void> | void) {
  const writes: string[] = [];
  const server = createServer((request, response) => {
    if (request.method !== 'GET') writes.push(`${request.method} ${request.url}`);
    if (request.url === '/v1/capabilities') { reply(response, { schemaVersion: 1, implementationDigest: digest, commands: [...nativeCommands] }); return; }
    Promise.resolve(handler(request, response)).catch(error => { response.statusCode = 500; response.end(String(error)); });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'controlled-protocol-source', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const host = await openResearch({ database: '/tmp/qz-transport-memory.sqlite', models, storage: new MemoryStorage() });
  return { target, host, execute: (command: string, input: NativeJson, args: string[], directory: string, context: Parameters<typeof executeNativeValue>[5]) => host.withNativeHost(() => executeNativeValue(target, command, input, args, directory, context)), close: async () => { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); assert.deepEqual(writes, [], 'retained HTTP history never sends POST or DELETE'); } };
}

// Transport-only peer; original payload precision and receipt identity are preserved.
test('process science keeps original payloads, shared catalog paths and distinct invocation receipts', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-process-'));
  const marker = join(directory, 'calls.jsonl'), binary = join(directory, 'runner.mjs');
  await writeFile(binary, `#!${process.execPath}\nimport {appendFileSync} from 'node:fs'; let text=''; for await (const chunk of process.stdin) text+=chunk; const input=JSON.parse(text); appendFileSync(${JSON.stringify(marker)},JSON.stringify({command:process.argv[2],args:process.argv.slice(3),input})+'\\n'); process.stdout.write(JSON.stringify({native:'受控协议',input}));`, { mode: 0o755 });
  const target: ProcessNativeTarget = { kind: 'RUST_PROCESS', id: 'protocol-only', binary, capabilities: { schemaVersion: 1, protocol: 'quazonai-native-process-v1', identity: { nautilusVersion: 'protocol-only', precision: 'high' }, research: { singleStrategyRegistration: true, configuredRunFactory: true }, account: { compiled: false, registration: false, command: 'account-runtime', requiresExplicitStart: true }, running: false } };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    const args = ['--catalog', '/shared/catalog'];
    const input = { amount: '9007199254740993123', rows: Array.from({ length: 2048 }, (_, index) => ({ clock: '1790000000000000001', index, label: '原生🐋' })) };
    for (let i = 0; i < 2; i++) assert.deepEqual(await host.withNativeHost(() => executeNativeValue(target, 'native-strategy-backtest', input, args, directory, BACKGROUND_CONTEXT)), { native: '受控协议', input });
    const calls = (await readFile(marker, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(calls, [0, 1].map(() => ({ command: 'native-strategy-backtest', args, input })));
    const tasks = (await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items;
    assert.equal(tasks.length, 2);
    const requests = tasks.map(task => ((task.input as any).request.request) as NativeExecutionRequest);
    assert.notEqual(requests[0]!.requestId, requests[1]!.requestId);
    for (const { requestId, ...original } of requests) {
      assert.equal(requestId, createHash('sha256').update(canonical(original as unknown as NativeJson)).digest('hex'));
      assert.equal(requestId, executionId(original)); assert.deepEqual(original.args, args); assert.deepEqual(original.files, {}); assert.deepEqual(original.input, input);
    }
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('historical lost response reconciles its original receipt without submitting another operation', async () => {
  const original: NativeExecutionRequest = { schemaVersion: 1, requestId: 'b'.repeat(64), invocationId: 'retained-lost-response', implementationDigest: digest, command: 'catalog-factor-scope', args: ['--catalog', '/shared/data'], input: {}, files: {} }; let reads = 0;
  const source = await worker((request, response) => { assert.equal(request.method, 'GET'); reads++; assert.equal(request.url, `/v1/executions/${original.requestId}`); reply(response, { schemaVersion: 1, requestId: original.requestId, status: 'COMPLETED', result: ['BTC.TEST'] }); });
  try {
    const root = await source.host.harness.root(BACKGROUND_CONTEXT);
    const id = await root.commit(tx => tx.createTask(NativeExecutionTask, { target: source.target, requestId: original.requestId, basis: 'retained-lost', command: original.command, request: { request: original as unknown as JsonObject }, receiptRequired: true }, { ownership: { kind: 'conversation' } }), BACKGROUND_CONTEXT);
    const settled = await source.host.harness.waitForTask(id, BACKGROUND_CONTEXT);
    assert.equal(settled.state.outcome.status, 'completed'); assert.deepEqual((await readNativeEvidence(source.host.harness, settled.conversationId, id, (settled.state.outcome.status === 'completed' && settled.state.outcome.result as any).native, BACKGROUND_CONTEXT)).value, ['BTC.TEST']);
    assert.equal(reads, 1); assert.deepEqual((settled.input as any).request.request, original);
    assert.equal((await source.host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items.length, 1);
  } finally { await source.close(); }
});

test('aborting an observation and its historical task never sends DELETE or proves physical cancellation', async () => {
  for (const cancellationStatus of ['CANCELLED', 'UNKNOWN']) {
    let entered!: () => void; const observing = new Promise<void>(resolve => { entered = resolve; });
    const original: NativeExecutionRequest = { schemaVersion: 1, requestId: 'c'.repeat(64), invocationId: 'retained-observation', implementationDigest: digest, command: 'native-strategy-backtest', args: [], input: { amount: '0.000000000000000001' }, files: {} };
    let final = false; const paths: string[] = [];
    const source = await worker((request, response) => { assert.equal(request.method, 'GET'); paths.push(request.url!); assert.equal(request.url, `/v1/executions/${original.requestId}`); entered(); if (!final) return; reply(response, { schemaVersion: 1, requestId: original.requestId, status: cancellationStatus }); });
    try {
      const root = await source.host.harness.root(BACKGROUND_CONTEXT);
      const id = await root.commit(async tx => {
        const id = await tx.createTask(NativeExecutionTask, { target: source.target, requestId: original.requestId, basis: 'retained-observation', command: original.command, request: { request: original as unknown as JsonObject }, receiptRequired: true }, { ownership: { kind: 'conversation' } });
        (await tx.doc(NativeRemoteUncertainty, root.id, String(id), null)).requests.push({ taskId: id, target: source.target, requestId: original.requestId, status: 'UNKNOWN' }); return id;
      }, BACKGROUND_CONTEXT);
      await observing;
      const controller = new AbortController();
      const observation = remoteNativeExecutionReceipt(source.target, original.requestId, controller.signal);
      controller.abort(new Error('owner_stop')); await assert.rejects(observation, /owner_stop/);
      assert.notEqual((await source.host.harness.getTask(id, BACKGROUND_CONTEXT))!.state.status, 'terminal');
      await source.host.harness.abortTask(id, BACKGROUND_CONTEXT); await source.host.harness.waitForTask(id, BACKGROUND_CONTEXT);
      assert.equal((await source.host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), BACKGROUND_CONTEXT))!.requests[0]!.status, 'UNKNOWN');
      final = true; await reconcileNativeRemoteTasks(source.host.harness, [{ id, conversationId: root.id }], BACKGROUND_CONTEXT);
      assert.equal((await source.host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), BACKGROUND_CONTEXT))!.requests[0]!.status, cancellationStatus);
      assert.equal((await source.host.harness.getTask(id, BACKGROUND_CONTEXT))!.state.outcome!.status, 'aborted');
      assert.ok(paths.length > 0);
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
    await assert.rejects(executeNativeValue('/old/binary', 'native-strategy-backtest', {}, [], '/unused', BACKGROUND_CONTEXT), /local_nautilus_target_disabled/);
  } finally {
    for (const [name, value] of [['QZ_NAUTILUS_BACKTEST', before.remote], ['QZ_JOB_BINARY', before.local], ['QZ_REQUIRE_REMOTE_NAUTILUS', before.required]]) {
      if (value === undefined) delete process.env[name!]; else process.env[name!] = value;
    }
  }
});

test('bounded cancellation retains its unknown original remote receipt after Durable abort and reopen', { timeout: 15_000 }, async t => {
  const actualTimeout = AbortSignal.timeout.bind(AbortSignal); const deadlines: number[] = [];
  // Exercise the original-receipt reconciliation deadline without a five-second wait.
  t.mock.method(AbortSignal, 'timeout', (milliseconds: number) => { deadlines.push(milliseconds); return actualTimeout(milliseconds === 5000 ? 100 : milliseconds); });
  const directory = await mkdtemp(join(tmpdir(), 'qz-remote-stop-receipt-'));
  let accepted!: () => void; const entered = new Promise<void>(resolve => { accepted = resolve; });
  const original: NativeExecutionRequest = { schemaVersion: 1, requestId: 'd'.repeat(64), invocationId: 'retained-bounded', implementationDigest: digest, command: 'native-strategy-backtest', args: ['--catalog', directory], input: {}, files: {} }; const paths: string[] = [];
  const source = await worker(async (request, _response) => {
    assert.equal(request.method, 'GET'); assert.equal(request.url, `/v1/executions/${original.requestId}`); paths.push(request.url!); accepted(); // Unavailable receipt cannot establish termination.
  });
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('compute', {}), { stopReason: 'toolUse' })]);
  const data = { catalog: directory, nautilusTarget: source.target,
    request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: {} } } };
  const compute = defineExtension({ name: 'qz.research', tools: [defineTool({
    name: 'compute', description: 'Controlled remote cancellation protocol', parameters: obj({}), replay: 'safe',
    async execute(_args, api, context) {
      const id = await api.commit(async tx => {
        const id = await tx.createTask(NativeStrategyBacktestTask, { request: {}, catalog: directory, target: source.target }, { ownership: { kind: 'task', taskId: api.taskId } });
        const basis = canonicalJsonDigest({ target: source.target, command: original.command, input: {}, args: original.args, files: {} });
        const child = await tx.createTask(NativeExecutionTask, { target: source.target, requestId: original.requestId, basis, command: original.command, request: { request: original as unknown as JsonObject }, ownerTaskId: id, receiptRequired: true }, { ownership: { kind: 'task', taskId: id } });
        (await tx.doc(Calls, id, basis, null)).taskId = child;
        (await tx.doc(NativeRemoteUncertainty, api.conversationId, String(id), null)).requests.push({ taskId: id, target: source.target, requestId: original.requestId, status: 'UNKNOWN', error: 'retained_unknown' }); return id;
      }, context); await api.waitForTask(id, context); return {};
    },
  })] });
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [compute] };
  let host = await openResearch(options);
  try {
    const start = await host.start('original-remote-stop', { goal: 'Retain unknown original receipt', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: {} });
    await entered; await host.stop(start.conversationId);
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native-strategy.backtest' }, 10), BACKGROUND_CONTEXT);
    assert.equal(tasks.items.length, 1); const task = tasks.items[0]!;
    assert.equal(task.state.outcome?.status, 'aborted'); assert.ok(paths.length > 0);
    await reconcileNativeRemoteTasks(host.harness, [task], BACKGROUND_CONTEXT);
    assert.ok(deadlines.includes(5000));
    await assert.rejects(host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, task.conversationId, task.id), BACKGROUND_CONTEXT), /native_remote_execution_unsettled/);
    const facts = await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT);
    assert.equal(facts!.requests[0]!.status, 'UNKNOWN'); assert.equal(facts!.requests[0]!.requestId, original.requestId); assert.deepEqual(facts!.requests[0]!.target, source.target);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT), facts);
    assert.equal((await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items.length, 1);
    assert.ok(paths.every(path => path === `/v1/executions/${original.requestId}`));
  } finally { await host.close(); await source.close(); await rm(directory, { recursive: true, force: true }); }
});
