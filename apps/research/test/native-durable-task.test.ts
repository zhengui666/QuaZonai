import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineExtension, defineTask, MemoryStorage } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { executeNativeValue, withNativeRemoteTracking, NativeRemoteUncertainty } from '../src/native-execution.ts';
import type { NativeExecutionRequest, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';

const context = BACKGROUND_CONTEXT;
test('an interrupted native parent reuses its original sleeping execution and immutable invocation', { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-durable-'));
  const requests: NativeExecutionRequest[] = [];
  let completed = false;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.url === '/v1/capabilities') {
      response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: ['validate-capital-exit'] })); return;
    }
    if (request.method === 'POST') {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const value = JSON.parse(Buffer.concat(chunks).toString()) as NativeExecutionRequest;
      requests.push(value); response.statusCode = 202;
      response.end(JSON.stringify({ schemaVersion: 1, requestId: value.requestId, status: (value.input as JsonObject).mode === 'warmup' ? 'COMPLETED' : 'RUNNING', ...((value.input as JsonObject).mode === 'warmup' ? { result: { warmup: true } } : {}) })); return;
    }
    const original = requests.find(value => request.url === `/v1/executions/${value.requestId}`);
    assert.ok(original);
    const result = (original.input as JsonObject).mode === 'large' ? { blob: 'x'.repeat(2 * 1024 * 1024) } : { answer: 'original-result' };
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: completed ? 'COMPLETED' : 'RUNNING', ...(completed ? { result } : {}) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'durable-probe', url: `http://127.0.0.1:${address.port}` };
  const Parent = defineTask<Record<string, never>, { phase: 'compute' }, JsonObject>({
    name: 'qz.test.native-parent', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      if (await runtime.memo('preparation', ctx) !== true) {
        await withNativeRemoteTracking(task.id, runtime, ctx, () => Promise.all([0, 1].map(() => executeNativeValue(target, 'validate-capital-exit', { mode: 'warmup' }, [], directory, ctx))));
        await runtime.memo('preparation', true, ctx);
      }
      await withNativeRemoteTracking(task.id, runtime, ctx, async () => {
        const result = await executeNativeValue(target, 'validate-capital-exit', { capital: '10' }, [], directory, ctx);
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: result as JsonObject } }), ctx);
      });
    } },
    abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
  });
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'native-probe', tasks: [Parent] })] };
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    const parentId = await root.commit(tx => tx.createTask(Parent, {}, { ownership: { kind: 'conversation' } }), context);
    host.harness.resume();
    for (let i = 0; i < 200; i++) {
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
      if (requests.length === 2 && tasks.items.at(-1)?.state.status === 'sleeping') break;
      await delay(5);
    }
    const before = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
    assert.equal(before.items.length, 2); assert.equal(before.items.at(-1)!.state.status, 'sleeping');
    assert.equal(requests.length, 2);
    const original = structuredClone(requests);
    await host.close();
    completed = true;
    host = await openResearch(options);
    const result = await host.harness.waitForTask(parentId, context);
    assert.equal(result.state.outcome.status, 'completed');
    if (result.state.outcome.status === 'completed') assert.deepEqual(result.state.outcome.result, { answer: 'original-result' });
    assert.deepEqual(requests, original, 'reopening never creates another POST or invocation identity');
    const refs = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(parentId), context);
    assert.equal(refs!.requests.length, 2, 'concurrent callers retain only sent invocation references');
    const after = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
    assert.deepEqual(after.items.map(item => item.id), before.items.map(item => item.id));
    const large = await host.withNativeHost(() => executeNativeValue(target, 'validate-capital-exit', { mode: 'large' }, [], directory, context)) as JsonObject;
    assert.equal((large.blob as string).length, 2 * 1024 * 1024);
    const retained = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution', order: 'descending' }, 10), context);
    assert.equal(retained.items.length, 3);
    assert.ok(JSON.stringify(retained.items[0]!.state).length < 1000, 'a native receipt references complete evidence instead of duplicating the scientific payload');
  } finally {
    await host.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});

test('parallel explicit service scopes keep two Harnesses isolated and reject an unowned request', async () => {
  let posts = 0;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: ['validate-capital-exit'] })); return; }
    posts++; const chunks = []; for await (const chunk of request) chunks.push(chunk);
    const value = JSON.parse(Buffer.concat(chunks).toString()) as NativeExecutionRequest;
    response.end(JSON.stringify({ schemaVersion: 1, requestId: value.requestId, status: 'COMPLETED', result: value.input }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'scope-probe', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const first = await openResearch({ database: '/tmp/qz-scope-first.sqlite', models, storage: new MemoryStorage() });
  const second = await openResearch({ database: '/tmp/qz-scope-second.sqlite', models, storage: new MemoryStorage() });
  try {
    const values = await Promise.all([first, second].map((host, index) => host.withNativeHost(async () => {
      await Promise.resolve();
      return executeNativeValue(target, 'validate-capital-exit', { owner: index }, [], '/unused', context);
    })));
    assert.deepEqual(values, [{ owner: 0 }, { owner: 1 }]);
    for (const [index, host] of [first, second].entries()) {
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
      assert.equal(tasks.items.length, 1);
      const input = tasks.items[0]!.input as JsonObject;
      assert.equal(((((input.request as JsonObject).request as JsonObject).input as JsonObject).owner), index);
    }
    await assert.rejects(executeNativeValue(target, 'validate-capital-exit', {}, [], '/unused', context), /native_harness_required/);
    assert.equal(posts, 2);
    const large = { payload: 'r'.repeat(2 * 1024 * 1024) };
    assert.deepEqual(await first.withNativeHost(() => executeNativeValue(target, 'validate-capital-exit', large, [], '/unused', context)), large);
    const records = await first.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution', order: 'descending' }, 10), context);
    assert.ok(JSON.stringify(records.items[0]!.input).length < 1000, 'large native requests remain complete immutable references in the task');
  } finally {
    await first.close(); await second.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('large canonical receipt stays paged through official SQLite task settlement and reopen', { timeout: 120000 }, async () => {
  if (!process.env.QZ_NATIVE_PAGED_CHILD) {
    const { promisify } = await import('node:util'); const { execFile } = await import('node:child_process');
    const env: NodeJS.ProcessEnv = { ...process.env, QZ_NATIVE_PAGED_CHILD: '1' }; delete env.NODE_TEST_CONTEXT;
    const child = await promisify(execFile)(process.execPath, ['--max-old-space-size=160', '--experimental-strip-types', '--test', '--test-name-pattern=large canonical receipt stays paged', import.meta.filename], {
      timeout: 115000, env, maxBuffer: 1024 * 1024,
    }); assert.match(child.stdout, /pass 1|# pass 1/); return;
  }
  const { isNativeTable, readNativeTable } = await import('../src/native-table.ts');
  const { NativeEvidencePart } = await import('../plugins/native-evidence.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-paged-'));
  let posts = 0;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: ['native-strategy-backtest'] })); return; }
    if (request.method === 'POST') posts++;
    const chunks = []; for await (const chunk of request) chunks.push(chunk);
    const input = request.method === 'GET' ? { requestId: request.url!.split('/').at(-1) } : JSON.parse(Buffer.concat(chunks).toString());
    response.write(`{"schemaVersion":1,"requestId":${JSON.stringify(input.requestId)},"status":"COMPLETED","result":{"canonical_result":{"orders":[`);
    const row = JSON.stringify({ quantity: '0.12345678901234567890', detail: 'original'.repeat(128) });
    for (let i = 0; i < 220000; i++) if (!response.write((i ? ',' : '') + row)) await new Promise<void>(resolve => response.once('drain', resolve));
    response.end(']}}}');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'paged-native-receipt', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    const result = await host.withNativeHost(() => executeNativeValue(target, 'native-strategy-backtest', {}, [], directory, context)) as JsonObject;
    const table = (result.canonical_result as JsonObject).orders;
    assert.ok(isNativeTable(table)); assert.equal(table.rowCount, 220000);
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
    assert.ok(JSON.stringify(tasks).length < 1024 * 1024);
    await host.close(); host = await openResearch(options);
    const last = await readNativeTable(table, 219999, 1, async key => (await host.harness.snapshot(NativeEvidencePart, table.conversationId, key, context))!.data);
    assert.equal((last[0] as JsonObject).quantity, '0.12345678901234567890');
    const { remoteNativeExecutionStatus } = await import('../src/native-execution.ts');
    const status = await remoteNativeExecutionStatus(target, 'a'.repeat(64));
    assert.equal(status.status, 'COMPLETED'); assert.equal(Object.hasOwn(status, 'result'), false);
    assert.equal(posts, 1);
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
