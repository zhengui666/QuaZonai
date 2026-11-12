import { obj } from '@earendil-works/pi-schema';
import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTask, defineTool } from '@earendil-works/pi-durable';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { NativeRemoteUncertainty, executeNativeValue, remoteNativeExecutionReceipt, nativeCommands, withNativeRemoteTracking } from '../src/native-execution.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

test('untrusted 429 and 501 responses never prove native upstream strategy non-execution', { timeout: 15_000 }, async () => {
  for (const command of ['native-strategy-backtest']) for (const mode of ['explicit', 'generic', 'wrong-id'] as const) {
    const directory = await mkdtemp(join(tmpdir(), 'qz-remote-admission-'));
    let posts = 0; let reads = 0; let original: NativeExecutionRequest | undefined;
    const server = createServer(async (request, response) => {
      response.setHeader('content-type', 'application/json');
      if (request.url === '/v1/capabilities') {
        response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return;
      }
      if (request.method === 'POST') {
        posts++; let data = ''; for await (const chunk of request) data += chunk;
        original = JSON.parse(data) as NativeExecutionRequest; assert.equal(original.command, command);
        response.statusCode = mode === 'explicit' ? 501 : 429;
        response.end(JSON.stringify(mode === 'generic' ? { error: 'too_many_requests' } : {
          schemaVersion: 1, requestId: mode === 'wrong-id' ? '0'.repeat(64) : original.requestId,
          admission: 'REJECTED', error: mode === 'explicit' ? 'nautilus_tick_resource_limits_unavailable' : 'nautilus_tick_capacity_exhausted',
        })); return;
      }
      reads++; response.statusCode = 404; response.end(JSON.stringify({ error: 'nautilus_execution_not_found' }));
    });
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    const target = { id: 'controlled-pre-admission', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
    const Compute = defineTask<{}, { phase: 'compute' }, null>({
      name: 'test.remote-admission.compute', version: 1, initial: () => ({ phase: 'compute' }),
      phases: { async compute(task, runtime, context) {
        try {
          await withNativeRemoteTracking(task.id, runtime, context, () => executeNativeValue(target, command, {}, [], directory, context));
          assert.fail('controlled endpoint rejected admission');
        } catch (error) {
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: String(error) } } }), context);
        }
      } },
      async abort(_task, runtime, context) {
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
      },
    });
    const extension = defineExtension({ name: 'qz.research', tasks: [Compute], tools: [defineTool({
      name: 'compute', description: 'Controlled remote admission protocol', parameters: obj({}), replay: 'safe',
      async execute(_args, api, context) {
        const id = await api.commit(tx => tx.createTask(Compute, {}, { ownership: { kind: 'task', taskId: api.taskId } }), context);
        await api.waitForTask(id, context);
        return { output: [{ type: 'text', text: 'Controlled admission finished.' }], control: { terminate: true } };
      },
    })] });
    const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
    faux.setResponses([fauxAssistantMessage(fauxToolCall('compute', {}), { stopReason: 'toolUse' })]);
    const options = { database: join(directory, 'research.sqlite'), models, extensions: [extension] };
    let host = await openResearch(options);
    try {
      const receipt = await host.start(`original-${mode}`, { goal: 'Verify admission evidence only', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
      await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
      const task = (await host.harness.commit(tx => tx.scanTasks({ kind: 'test.remote-admission.compute' }, 10), BACKGROUND_CONTEXT)).items[0]!;
      assert.equal(task.state.outcome?.status, 'failed');
      const facts = await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT);
      assert.equal(facts?.requests.length, 1);
      const reference = facts!.requests[0]!;
      assert.equal(reference.requestId, original!.requestId);
      assert.equal(reference.status, 'UNKNOWN');
      await assert.rejects(host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, task.conversationId, task.id), BACKGROUND_CONTEXT), /native_remote_execution_unsettled/);
      await reconcileNativeRemoteTasks(host.harness, [task], BACKGROUND_CONTEXT);
      assert.equal((await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT))!.requests[0]!.status, 'UNKNOWN');
      await host.close(); host = await openResearch(options);
      assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT), facts);
      assert.equal(posts, 1, 'reopening never resubmits rejected or uncertain work');
    } finally {
      await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
      await rm(directory, { recursive: true, force: true });
    }
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
  const requests: NativeExecutionRequest[] = []; let completed = false; let reads = 0;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (request.method === 'POST') {
      let data = ''; for await (const chunk of request) data += chunk;
      const value = JSON.parse(data) as NativeExecutionRequest; requests.push(value);
      response.statusCode = 429; response.end(JSON.stringify({ error: 'too_many_requests' })); return;
    }
    reads++; const original = requests[0]!;
    assert.equal(request.url, `/v1/executions/${original.requestId}`);
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: completed ? 'COMPLETED' : 'UNKNOWN',
      ...(completed ? { result: { protocolOnly: true } } : { error: 'receipt_unavailable' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const target = { id: 'controlled-capacity', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const options = { database: join(directory, 'research.sqlite'), models }; let host = await openResearch(options);
  try {
    const root = await host.harness.root(BACKGROUND_CONTEXT);
    const id = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request: { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }], event_start_ns: '0', event_end_ns: '20', available_end_ns: '20', venue: { base_currency: null }, strategy: { kind: 'EMA_CROSS', trade_size: '0.01', fast_period: 2, slow_period: 3 } }, catalog: directory, target }, { ownership: { kind: 'conversation' } }), BACKGROUND_CONTEXT);
    const deadline = Date.now() + 5000;
    for (;;) {
      const task = await host.harness.getTask(id, BACKGROUND_CONTEXT);
      if (task?.state.status === 'sleeping' && (task.state.checkpoint as { phase: string }).phase === 'reconcile') break;
      assert.notEqual(task?.state.status, 'terminal'); assert.ok(Date.now() < deadline); await delay(5);
    }
    assert.equal(requests.length, 1); const original = structuredClone(requests[0]);
    await host.close(); completed = true; host = await openResearch(options);
    const settled = await host.harness.waitForTask(id, BACKGROUND_CONTEXT);
    // Receipt was adopted, but a protocol-only object is not valid backtest evidence.
    assert.equal(settled.state.outcome.status, 'failed');
    assert.match(settled.state.outcome.status === 'failed' ? settled.state.outcome.error.message : '', /native_strategy/);
    assert.ok(reads > 0); assert.deepEqual(requests, [original], 'only GET may observe uncertain original work');
    const facts = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), BACKGROUND_CONTEXT);
    assert.equal(facts!.requests[0]!.status, 'COMPLETED');
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});

test('only original-id explicit admission refusal settles a Durable remote reference without a terminal receipt', { timeout: 15_000 }, async () => {
  for (const command of ['replay-tick-economic']) for (const mode of ['explicit', 'generic', 'wrong-id'] as const) {
    const directory = await mkdtemp(join(tmpdir(), 'qz-remote-admission-'));
    let posts = 0; let reads = 0; let original: NativeExecutionRequest | undefined;
    const server = createServer(async (request, response) => {
      response.setHeader('content-type', 'application/json');
      if (request.url === '/v1/capabilities') {
        response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return;
      }
      if (request.method === 'POST') {
        posts++; let data = ''; for await (const chunk of request) data += chunk;
        original = JSON.parse(data) as NativeExecutionRequest; assert.equal(original.command, command);
        response.statusCode = mode === 'explicit' ? 501 : 429;
        response.end(JSON.stringify(mode === 'generic' ? { error: 'too_many_requests' } : {
          schemaVersion: 1, requestId: mode === 'wrong-id' ? '0'.repeat(64) : original.requestId,
          admission: 'REJECTED', error: mode === 'explicit' ? 'nautilus_tick_resource_limits_unavailable' : 'nautilus_tick_capacity_exhausted',
        })); return;
      }
      reads++; response.statusCode = 404; response.end(JSON.stringify({ error: 'nautilus_execution_not_found' }));
    });
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    const target = { id: 'controlled-pre-admission', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
    const Compute = defineTask<{}, { phase: 'compute' }, null>({
      name: 'test.remote-admission.compute', version: 1, initial: () => ({ phase: 'compute' }),
      phases: { async compute(task, runtime, context) {
        try {
          await withNativeRemoteTracking(task.id, runtime, context, () => executeNativeValue(target, command, {}, [], directory, context));
          assert.fail('controlled endpoint rejected admission');
        } catch (error) {
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: String(error) } } }), context);
        }
      } },
      async abort(_task, runtime, context) {
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
      },
    });
    const extension = defineExtension({ name: 'qz.research', tasks: [Compute], tools: [defineTool({
      name: 'compute', description: 'Controlled remote admission protocol', parameters: obj({}), replay: 'safe',
      async execute(_args, api, context) {
        const id = await api.commit(tx => tx.createTask(Compute, {}, { ownership: { kind: 'task', taskId: api.taskId } }), context);
        await api.waitForTask(id, context);
        return { output: [{ type: 'text', text: 'Controlled admission finished.' }], control: { terminate: true } };
      },
    })] });
    const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
    faux.setResponses([fauxAssistantMessage(fauxToolCall('compute', {}), { stopReason: 'toolUse' })]);
    const options = { database: join(directory, 'research.sqlite'), models, extensions: [extension] };
    let host = await openResearch(options);
    try {
      const receipt = await host.start(`original-${mode}`, { goal: 'Verify admission evidence only', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
      await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
      const task = (await host.harness.commit(tx => tx.scanTasks({ kind: 'test.remote-admission.compute' }, 10), BACKGROUND_CONTEXT)).items[0]!;
      assert.equal(task.state.outcome?.status, 'failed');
      const facts = await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT);
      assert.equal(facts?.requests.length, 1);
      const reference = facts!.requests[0]!;
      assert.equal(reference.requestId, original!.requestId);
      if (mode === 'explicit') {
        assert.equal(reference.status, 'REJECTED'); assert.equal(reference.admission, 'REJECTED');
        assert.equal(reference.error, 'nautilus_tick_resource_limits_unavailable');
        await host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, task.conversationId, task.id), BACKGROUND_CONTEXT);
        await reconcileNativeRemoteTasks(host.harness, [task], BACKGROUND_CONTEXT);
        assert.equal(reads, 0, 'explicit non-execution does not invent or query a terminal execution receipt');
      } else {
        assert.equal(reference.status, 'UNKNOWN');
        await assert.rejects(host.harness.commit(tx => assertNativeRemoteTaskSettled(tx, task.conversationId, task.id), BACKGROUND_CONTEXT), /native_remote_execution_unsettled/);
      }
      await host.close(); host = await openResearch(options);
      assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), BACKGROUND_CONTEXT), facts);
      assert.equal(posts, 1, 'reopening never resubmits rejected or uncertain work');
    } finally {
      await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
      await rm(directory, { recursive: true, force: true });
    }
  }
});

test('explicit tick capacity refusal sleeps and resumes the original invocation until one execution is admitted', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-remote-capacity-'));
  const requests: NativeExecutionRequest[] = []; let executed = 0; let reads = 0;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.url === '/v1/capabilities') {
      response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return;
    }
    if (request.method === 'POST') {
      let data = ''; for await (const chunk of request) data += chunk;
      const value = JSON.parse(data) as NativeExecutionRequest; requests.push(value);
      if (requests.length === 1) {
        response.statusCode = 429;
        response.end(JSON.stringify({ schemaVersion: 1, requestId: value.requestId, admission: 'REJECTED', error: 'nautilus_tick_capacity_exhausted' })); return;
      }
      executed++;
      response.end(JSON.stringify({ schemaVersion: 1, requestId: value.requestId, status: 'COMPLETED', result: { protocolOnly: true } })); return;
    }
    reads++; response.statusCode = 404; response.end('{"error":"not_found"}');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const target = { id: 'controlled-capacity', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    const observation = host.withNativeHost(() => executeNativeValue(target, 'replay-tick-economic', { marker: 'original-input' }, [], directory, BACKGROUND_CONTEXT)).catch(() => undefined);
    let pending;
    for (let i = 0; i < 200; i++) {
      pending = (await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT)).items[0];
      if (pending?.state.status === 'sleeping') break;
      await delay(5);
    }
    assert.ok(pending); assert.equal(pending.state.status, 'sleeping');
    if (pending.state.status !== 'sleeping') return;
    assert.deepEqual(pending.state.checkpoint, { phase: 'submit' });
    assert.ok(pending.state.until - Date.now() > 4000);
    const rejected = await host.harness.snapshot(NativeRemoteUncertainty, pending.conversationId, String(pending.id), BACKGROUND_CONTEXT);
    assert.equal(rejected!.requests[0]!.status, 'REJECTED');
    assert.equal(rejected!.requests[0]!.error, 'nautilus_tick_capacity_exhausted');
    assert.equal(requests.length, 1); assert.equal(executed, 0); assert.equal(reads, 0);
    await host.close(); await observation;
    host = await openResearch(options);
    const settled = await host.harness.waitForTask(pending.id, BACKGROUND_CONTEXT);
    assert.equal(settled.state.outcome.status, 'completed');
    assert.equal(requests.length, 2); assert.equal(executed, 1); assert.equal(reads, 0);
    assert.deepEqual(requests[1], requests[0]);
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), BACKGROUND_CONTEXT);
    assert.deepEqual(tasks.items.map(task => task.id), [pending.id]);
    const completed = await host.harness.snapshot(NativeRemoteUncertainty, pending.conversationId, String(pending.id), BACKGROUND_CONTEXT);
    assert.equal(completed!.requests[0]!.status, 'COMPLETED');
    assert.equal(completed!.requests[0]!.admissionRejection, 'nautilus_tick_capacity_exhausted');
    assert.equal(completed!.requests[0]!.admission, undefined);
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
