import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineDocFamily, defineExtension, defineTask, MemoryStorage } from '@earendil-works/pi-durable';
import type { JsonObject, Tx, TaskId, ConversationId } from '@earendil-works/pi-durable';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { executeNativeValue, withNativeRemoteTracking, NativeRemoteUncertainty, NativeExecutionTask } from '../src/native-execution.ts';
import type { NativeExecutionRequest, RemoteNativeTarget, ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';

import { canonicalJsonDigest } from '../src/native-json.ts';
import { readNativeEvidence } from '../plugins/native-evidence.ts';
const Calls = defineDocFamily<any, null>({ kind: 'qz.native-call', version: 1, scope: 'task', family: true, initial: () => ({ taskId: null }), checkpointWhen: () => true });
async function retain(tx: Tx, conversationId: ConversationId, owner: TaskId | undefined, target: RemoteNativeTarget, original: NativeExecutionRequest) {
  const basis = canonicalJsonDigest({ target, command: original.command, input: original.input, args: original.args, files: original.files });
  const id = await tx.createTask(NativeExecutionTask, { target, requestId: original.requestId, basis, command: original.command, request: { request: original as unknown as JsonObject }, ...(owner ? { ownerTaskId: owner } : {}), receiptRequired: true }, { ownership: owner ? { kind: 'task', taskId: owner } : { kind: 'conversation' } });
  if (owner) (await tx.doc(Calls, owner, basis, null)).taskId = id;
  (await tx.doc(NativeRemoteUncertainty, conversationId, String(owner ?? id), null)).requests.push({ taskId: owner ?? id, target, requestId: original.requestId, status: 'UNKNOWN' });
  return id;
}
const context = BACKGROUND_CONTEXT;
test('an interrupted native parent reuses its original sleeping execution and immutable invocation', { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-durable-'));
  const requests: NativeExecutionRequest[] = ['warmup', 'main', 'large'].map((mode, index): NativeExecutionRequest => ({ schemaVersion: 1, requestId: String(index + 1).padStart(64, '0'), invocationId: `retained-${mode}`, implementationDigest: `sha256:${'a'.repeat(64)}`, command: 'validate-capital-exit', args: [], input: mode === 'main' ? { capital: '10' } : { mode }, files: {} })); const gets: string[] = [];
  let completed = false;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    assert.equal(request.method, 'GET'); gets.push(request.url!);
    const original = requests.find(value => request.url === `/v1/executions/${value.requestId}`);
    assert.ok(original);
    const result = (original.input as JsonObject).mode === 'large' ? { blob: 'x'.repeat(2 * 1024 * 1024) } : { answer: 'original-result' };
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: completed || (original.input as JsonObject).mode === 'warmup' ? 'COMPLETED' : 'RUNNING', ...(completed || (original.input as JsonObject).mode === 'warmup' ? { result } : {}) }));
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
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'native-probe', tasks: [Parent] })], deferResume: true };
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    const parentId = await root.commit(async tx => { const id = await tx.createTask(Parent, {}, { ownership: { kind: 'conversation' } }); for (const original of requests.slice(0, 2)) await retain(tx, root.id, id, target, original); return id; }, context);
    host.harness.resume();
    for (let i = 0; i < 200; i++) {
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
      if (gets.length >= 2 && tasks.items.at(-1)?.state.status === 'sleeping') break;
      await delay(5);
    }
    const before = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
    assert.equal(before.items.length, 2); assert.equal(before.items.at(-1)!.state.status, 'sleeping');
    assert.ok(gets.length >= 2);
    const original = structuredClone(requests);
    await host.close();
    completed = true;
    host = await openResearch(options); host.harness.resume();
    const result = await host.harness.waitForTask(parentId, context);
    assert.equal(result.state.outcome.status, 'completed');
    if (result.state.outcome.status === 'completed') assert.deepEqual(result.state.outcome.result, { answer: 'original-result' });
    assert.deepEqual(requests, original, 'reopening never creates another POST or invocation identity');
    const refs = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(parentId), context);
    assert.equal(refs!.requests.length, 2, 'concurrent replay callers retain only the two original invocation references');
    const after = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
    assert.deepEqual(after.items.map(item => item.id), before.items.map(item => item.id));
    assert.deepEqual(after.items.map(item => item.input), before.items.map(item => item.input));
    assert.ok(gets.every(path => requests.some(original => path === `/v1/executions/${original.requestId}`)));
    const largeId = await (await host.harness.root(context)).commit(tx => retain(tx, root.id, undefined, target, requests[2]!), context);
    const largeTask = await host.harness.waitForTask(largeId, context); assert.equal(largeTask.state.outcome.status, 'completed');
    const large = (await readNativeEvidence(host.harness, root.id, largeId, (largeTask.state.outcome.status === 'completed' && largeTask.state.outcome.result as any).native, context)).value as JsonObject;
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
  const directory = await mkdtemp(join(tmpdir(), 'qz-scoped-process-')), binary = join(directory, 'runner.mjs'), marker = join(directory, 'calls.jsonl');
  await writeFile(binary, `#!${process.execPath}\nimport{appendFileSync}from'node:fs';let text='';for await(const part of process.stdin)text+=part;appendFileSync(${JSON.stringify(marker)},JSON.stringify({command:process.argv[2]})+'\\n');process.stdout.write(text);`, { mode: 0o755 });
  const target: ProcessNativeTarget = { kind:'RUST_PROCESS',id:'scope-protocol',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-only',precision:'high'},research:{singleStrategyRegistration:true,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false} };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const first = await openResearch({ database: '/tmp/qz-scope-first.sqlite', models, storage: new MemoryStorage() });
  const second = await openResearch({ database: '/tmp/qz-scope-second.sqlite', models, storage: new MemoryStorage() });
  try {
    const values = await Promise.all([first, second].map((host, index) => host.withNativeHost(async () => {
      await Promise.resolve();
      return executeNativeValue(target, 'native-strategy-backtest', { owner: index }, [], '/unused', context);
    })));
    assert.deepEqual(values, [{ owner: 0 }, { owner: 1 }]);
    for (const [index, host] of [first, second].entries()) {
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
      assert.equal(tasks.items.length, 1);
      const input = tasks.items[0]!.input as JsonObject;
      assert.equal(((((input.request as JsonObject).request as JsonObject).input as JsonObject).owner), index);
    }
    await assert.rejects(executeNativeValue(target, 'native-strategy-backtest', {}, [], '/unused', context), /native_harness_required/);
    assert.equal((await readFile(marker, 'utf8')).trim().split('\n').length, 2);
    const large = { payload: 'r'.repeat(2 * 1024 * 1024) };
    assert.deepEqual(await first.withNativeHost(() => executeNativeValue(target, 'native-strategy-backtest', large, [], '/unused', context)), large);
    const records = await first.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution', order: 'descending' }, 10), context);
    assert.ok(JSON.stringify(records.items[0]!.input).length < 1000, 'large native requests remain complete immutable references in the task');
  } finally {
    await first.close(); await second.close(); await rm(directory, { recursive: true, force: true });
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
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-native-paged-'));
  const original: NativeExecutionRequest = { schemaVersion: 1, requestId: 'a'.repeat(64), invocationId: 'retained-paged', implementationDigest: `sha256:${'a'.repeat(64)}`, command: 'native-strategy-backtest', args: [], input: {}, files: {} }; let gets = 0;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json'); assert.equal(request.method, 'GET'); assert.equal(request.url, `/v1/executions/${original.requestId}`); gets++;
    const input = original;
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
    const root = await host.harness.root(context);
    const id = await root.commit(tx => retain(tx, root.id, undefined, target, original), context);
    const task = await host.harness.waitForTask(id, context); assert.equal(task.state.outcome.status, 'completed');
    const result = (await readNativeEvidence(host.harness, root.id, id, (task.state.outcome.status === 'completed' && task.state.outcome.result as any).native, context)).value as JsonObject;
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
    assert.equal(gets, 2, 'original task and status-only read both GET the same retained receipt');
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});

test('status-only receipts preserve original top-level result presence and complete JSON validation', async () => {
  const { remoteNativeExecutionStatus } = await import('../src/native-execution.ts');
  const requestId = 'b'.repeat(64);
  let body = '';
  const server = createServer((_request, response) => { response.setHeader('content-type', 'application/json'); response.end(body); });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'status-presence', url: `http://127.0.0.1:${address.port}` };
  const prefix = `"schemaVersion":1,"requestId":"${requestId}"`;
  try {
    for (const content of [`{"result":null,${prefix},"status":"COMPLETED"}`, `{${prefix},"status":"COMPLETED","result":{"nested":[1,2]}}`]) {
      body = content; const result = await remoteNativeExecutionStatus(target, requestId);
      assert.equal(result.status, 'COMPLETED'); assert.equal(Object.hasOwn(result, 'result'), false);
    }
    for (const content of [`{${prefix},"status":"COMPLETED"}`, `{${prefix},"status":"COMPLETED","nested":{"result":{}}}`, `{${prefix},"status":"COMPLETED","result":{"rows":[1,2`]) {
      body = content; await assert.rejects(remoteNativeExecutionStatus(target, requestId));
    }
    body = `{${prefix},"status":"CANCELLED"}`;
    assert.equal((await remoteNativeExecutionStatus(target, requestId)).status, 'CANCELLED');
  } finally { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); }
});
