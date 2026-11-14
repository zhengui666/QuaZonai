import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineDocFamily, type JsonObject } from '@earendil-works/pi-durable';
import { canonicalJsonDigest } from '../src/native-json.ts';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { NativeExecutionTask, NativeRemoteUncertainty, executeNativeValue, remoteNativeExecutionReceipt } from '../src/native-execution.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

const Calls = defineDocFamily<any, null>({ kind: 'qz.native-call', version: 1, scope: 'task', family: true, initial: () => ({ taskId: null }), checkpointWhen: () => true });
// Seed the existing official Pi task/receipt identity. New HTTP admission is retired.
// These are protocol/history tests, not measured scientific backtests.
test('retained UNKNOWN survives untrusted 429/501 and wrong-id GET replies; explicit historical refusal remains settled', { timeout: 20_000 }, async () => {
  for (const status of [429, 501]) for (const mode of ['generic', 'wrong-id', 'explicit'] as const) {
    const directory = await mkdtemp(join(tmpdir(), 'qz-remote-admission-'));
    const original: NativeExecutionRequest = { schemaVersion: 1, requestId: 'b'.repeat(64), invocationId: 'retained-original', implementationDigest: `sha256:${'a'.repeat(64)}`, command: 'native-strategy-backtest', args: [], input: { quantity: '0.000000000000000001' }, files: {} };
    const requests: string[] = [];
    const server = createServer((request, response) => {
      requests.push(`${request.method} ${request.url}`);
      assert.equal(request.method, 'GET'); assert.equal(request.url, `/v1/executions/${original.requestId}`);
      response.setHeader('content-type', 'application/json'); response.statusCode = status;
      response.end(JSON.stringify(mode === 'generic' ? { error: 'too_many_requests' } : {
        schemaVersion: 1, requestId: '0'.repeat(64), admission: 'REJECTED', error: 'nautilus_tick_capacity_exhausted',
      }));
    });
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    const target = { id: 'original-admission', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
    const models = createModels(); models.setProvider(fauxProvider().provider);
    const options = { database: join(directory, 'research.sqlite'), models, deferResume: true };
    let host = await openResearch(options);
    try {
      const root = await host.harness.root(BACKGROUND_CONTEXT);
      const id = await root.commit(async tx => {
        const taskId = await tx.createTask(NativeExecutionTask, { target, requestId: original.requestId, basis: 'retained-basis', command: original.command, request: { request: original as unknown as JsonObject }, receiptRequired: true }, { ownership: { kind: 'conversation' } });
        const reference: JsonObject = { taskId, target, requestId: original.requestId, status: mode === 'explicit' ? 'REJECTED' : 'UNKNOWN', error: mode === 'explicit' ? (status === 429 ? 'nautilus_tick_capacity_exhausted' : 'nautilus_tick_resource_limits_unavailable') : `retained_http_${status}` };
        if (mode === 'explicit') reference.admission = 'REJECTED';
        (await tx.doc(NativeRemoteUncertainty, root.id, String(taskId), null)).requests.push(reference);
        return taskId;
      }, BACKGROUND_CONTEXT);
      const task = (await host.harness.getTask(id, BACKGROUND_CONTEXT))!;
      const before = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), BACKGROUND_CONTEXT);
      if (mode === 'explicit') await host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, root.id, id), BACKGROUND_CONTEXT);
      else await assert.rejects(host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, root.id, id), BACKGROUND_CONTEXT), /native_remote_execution_unsettled/);
      await reconcileNativeRemoteTasks(host.harness, [task], BACKGROUND_CONTEXT);
      assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), BACKGROUND_CONTEXT), before);
      assert.deepEqual(requests, mode === 'explicit' ? [] : [`GET /v1/executions/${original.requestId}`]);
      await host.close(); host = await openResearch(options);
      assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), BACKGROUND_CONTEXT), before);
      assert.deepEqual((await host.harness.getTask(id, BACKGROUND_CONTEXT))!.input, task.input);
      if (mode === 'explicit') await host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, root.id, id), BACKGROUND_CONTEXT);
      else await assert.rejects(host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, root.id, id), BACKGROUND_CONTEXT), /native_remote_execution_unsettled/);
      assert.equal((await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items.length, 1);
      const beforeCalls = [...requests];
      await assert.rejects(host.withNativeHost(() => executeNativeValue(target, original.command, original.input, original.args, directory, BACKGROUND_CONTEXT)), /native_http_execution_retired/);
      assert.deepEqual(requests, beforeCalls, 'new execution cannot retry the old refused or unknown invocation');
      assert.equal((await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items.length, 1);
    } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
  }
});

test('receipt observation is a single read; durable tasks own waiting and a cancelled read never sends a POST', async () => {
  const requestId = 'b'.repeat(64); let mode = 'RUNNING'; let reads = 0; let writes = 0;
  const server = createServer((request, response) => {
    if (request.method !== 'GET') writes++;
    assert.equal(request.url, `/v1/executions/${requestId}`); reads++;
    response.setHeader('content-type', 'application/json');
    if (mode === 'missing') { response.statusCode = 404; response.end('{"error":"not_found"}'); return; }
    if (mode === 'abort') return;
    response.end(JSON.stringify({ schemaVersion: 1, requestId, status: mode,
      ...(mode === 'COMPLETED' ? { result: { original: true } } : ['UNKNOWN', 'FAILED'].includes(mode) ? { error: 'native_outcome_unknown' } : {}) }));
  });
  server.listen(0, '127.0.0.1'); await new Promise<void>(resolve => server.once('listening', resolve));
  const target = { id: 'original-receipt-only', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
  try {
    for (const status of ['RUNNING', 'UNKNOWN', 'FAILED', 'CANCELLED', 'COMPLETED']) {
      mode = status; reads = 0;
      assert.equal((await remoteNativeExecutionReceipt(target, requestId)).status, status); assert.equal(reads, 1);
    }
    mode = 'missing'; await assert.rejects(remoteNativeExecutionReceipt(target, requestId), /nautilus_http_404/);
    mode = 'abort'; const controller = new AbortController();
    const waiting = remoteNativeExecutionReceipt(target, requestId, controller.signal); const timer = setTimeout(() => controller.abort(), 100);
    try { await assert.rejects(waiting, error => controller.signal.aborted && error instanceof Error && error.name === 'AbortError'); } finally { clearTimeout(timer); }
    assert.equal(writes, 0);
  } finally { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); }
});

test('native upstream strategy HTTP capacity response keeps its original invocation across restart without an unproven retry', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-capacity-'));
  const requests: string[] = []; let completed = false; let reads = 0;
  const request: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }], event_start_ns: '0', event_end_ns: '20', available_end_ns: '20', venue: { base_currency: null }, strategy: { kind: 'EMA_CROSS', trade_size: '0.01', fast_period: 2, slow_period: 3 } };
  const original: NativeExecutionRequest = { schemaVersion: 1, requestId: 'b'.repeat(64), invocationId: 'retained-capacity', implementationDigest: `sha256:${'a'.repeat(64)}`, command: 'native-strategy-backtest', args: ['--catalog', directory], input: request, files: {} };
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    requests.push(`${request.method} ${request.url}`); assert.equal(request.method, 'GET');
    reads++; assert.equal(request.url, `/v1/executions/${original.requestId}`);
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: completed ? 'COMPLETED' : 'UNKNOWN',
      ...(completed ? { result: { protocolOnly: true } } : { error: 'receipt_unavailable' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const target = { id: 'controlled-capacity', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const options = { database: join(directory, 'research.sqlite'), models, deferResume: true }; let host = await openResearch(options);
  try {
    const root = await host.harness.root(BACKGROUND_CONTEXT);
    const id = await root.commit(async tx => {
      const owner = await tx.createTask(NativeStrategyBacktestTask, { request, catalog: directory, target }, { ownership: { kind: 'conversation' } });
      const basis = canonicalJsonDigest({ target, command: original.command, input: request, args: original.args, files: {} });
      const child = await tx.createTask(NativeExecutionTask, { target, requestId: original.requestId, basis, command: original.command, request: { request: original as unknown as JsonObject }, ownerTaskId: owner, receiptRequired: true }, { ownership: { kind: 'task', taskId: owner } });
      (await tx.doc(Calls, owner, basis, null)).taskId = child;
      (await tx.doc(NativeRemoteUncertainty, root.id, String(owner), null)).requests.push({ taskId: owner, target, requestId: original.requestId, status: 'UNKNOWN', error: 'retained_http_429' });
      return owner;
    }, BACKGROUND_CONTEXT);
    host.harness.resume();
    const deadline = Date.now() + 5000;
    for (;;) {
      const task = await host.harness.getTask(id, BACKGROUND_CONTEXT);
      if (task?.state.status === 'sleeping' && (task.state.checkpoint as { phase: string }).phase === 'reconcile') break;
      assert.notEqual(task?.state.status, 'terminal'); assert.ok(Date.now() < deadline); await delay(5);
    }
    assert.ok(requests.length > 0);
    await assert.rejects(host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, root.id, id), BACKGROUND_CONTEXT), /native_remote_execution_unsettled/);
    await host.close(); completed = true; host = await openResearch(options); host.harness.resume();
    const settled = await host.harness.waitForTask(id, BACKGROUND_CONTEXT);
    // Receipt was adopted, but a protocol-only object is not valid backtest evidence.
    assert.equal(settled.state.outcome.status, 'failed');
    assert.match(settled.state.outcome.status === 'failed' ? settled.state.outcome.error.message : '', /native_strategy/);
    assert.ok(reads > 0); assert.ok(requests.every(value => value === `GET /v1/executions/${original.requestId}`), 'only original GET may observe uncertain original work');
    const children = (await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items;
    assert.equal(children.length, 1); assert.deepEqual((children[0]!.input as any).request.request, original);
    const facts = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), BACKGROUND_CONTEXT);
    assert.equal(facts!.requests[0]!.status, 'COMPLETED');
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});
