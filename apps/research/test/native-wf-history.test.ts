import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineDocFamily, type JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { NativeExecutionTask, NativeRemoteUncertainty, nativeCommands, reconcileNativeExecution, cancelUnsettledNativeExecutions } from '../src/native-execution.ts';
import { NativeEvidencePart } from '../plugins/native-evidence.ts';
import { isNativeTable, readNativeTable } from '../src/native-table.ts';

// Historical protocol fixture in the official store, not a production DB mutation.
const Calls = defineDocFamily<any, null>({ kind: 'qz.native-call', version: 1, scope: 'task', family: true, initial: () => ({ taskId: null }), checkpointWhen: () => true });

test('retired WF reconciliation preserves large nested evidence and UNKNOWN across SQLite reopen without POST', { timeout: 120000 }, async () => {
  if (!process.env.QZ_WF_HISTORY_CHILD) {
    const { promisify } = await import('node:util'); const { execFile } = await import('node:child_process');
    const env: NodeJS.ProcessEnv = { ...process.env, QZ_WF_HISTORY_CHILD: '1' }; delete env.NODE_TEST_CONTEXT;
    const child = await promisify(execFile)(process.execPath, ['--max-old-space-size=160', '--experimental-strip-types', '--test', import.meta.filename], { env, timeout: 115000, maxBuffer: 1024 * 1024 });
    assert.match(child.stdout, /pass 1|# pass 1/); return;
  }
  const directory = await mkdtemp(join(tmpdir(), 'qz-wf-history-'));
  const methods: string[] = []; let completed = false;
  const requestId = 'a'.repeat(64);
  const server = createServer(async (req, res) => {
    methods.push(req.method!); assert.equal(req.url, `/v1/executions/${requestId}`);
    res.setHeader('content-type', 'application/json');
    if (!completed || req.method === 'DELETE') { res.end(JSON.stringify({ schemaVersion: 1, requestId, status: 'UNKNOWN', error: 'fixture_unknown' })); return; }
    res.write(`{"schemaVersion":1,"requestId":"${requestId}","status":"COMPLETED","result":{"folds":[{"test_result":{"canonical_result":{"orders":[`);
    const row = JSON.stringify({ quantity: '0.12345678901234567890', detail: 'original'.repeat(128) });
    for (let i = 0; i < 220000; i++) if (!res.write((i ? ',' : '') + row)) await new Promise<void>(resolve => res.once('drain', resolve));
    res.end(']}}}]}}');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'old-wf-worker', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const options = { database: join(directory, 'research.sqlite'), models, deferResume: true };
  let host = await openResearch(options);
  try {
    assert.equal(nativeCommands.has('native-walk-forward'), false, 'historical decoding never re-enables new execution');
    const root = await host.harness.root(context);
    const ids = await root.commit(async tx => {
      const owner = await tx.createTask(NativeStrategyBacktestTask, { request: {}, catalog: directory, target, walkForward: true }, { ownership: { kind: 'conversation' } });
      const child = await tx.createTask(NativeExecutionTask, { target, requestId, basis: 'old-wf', command: 'native-walk-forward', request: {}, ownerTaskId: owner, receiptRequired: true }, { ownership: { kind: 'task', taskId: owner } });
      (await tx.doc(Calls, owner, 'old-wf', null)).taskId = child;
      (await tx.doc(NativeRemoteUncertainty, root.id, String(owner), null)).requests.push({ taskId: owner, target, requestId, status: 'UNKNOWN' });
      return { owner, child };
    }, context);
    // This tests the production reconciliation boundary, not a fabricated scheduler checkpoint.
    const runtime = () => ({ conversationId: root.id, signal: new AbortController().signal,
      snapshotAsOf: host.harness.snapshotAsOf.bind(host.harness), waitForTask: host.harness.waitForTask.bind(host.harness), watchDoc: host.harness.watchDoc.bind(host.harness), getTask: host.harness.getTask.bind(host.harness), snapshot: host.harness.snapshot.bind(host.harness), commit: host.harness.commit.bind(host.harness) } as unknown as Parameters<typeof reconcileNativeExecution>[1]);
    assert.equal(await reconcileNativeExecution(ids.owner, runtime(), ids.child, context), false);
    await cancelUnsettledNativeExecutions(ids.owner, runtime(), context);
    assert.equal((await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(ids.owner), context))!.requests[0]!.status, 'UNKNOWN');
    assert.equal((await host.harness.snapshot(Calls, ids.owner, 'old-wf', context))!.adoptedReceipt, undefined);
    await host.close(); host = await openResearch(options); completed = true;
    assert.equal(await reconcileNativeExecution(ids.owner, runtime(), ids.child, context), true);
    const adopted = (await host.harness.snapshot(Calls, ids.owner, 'old-wf', context))!.adoptedReceipt;
    const table = adopted.native.value.folds[0].test_result.canonical_result.orders;
    assert.ok(isNativeTable(table)); assert.equal(table.rowCount, 220000);
    assert.equal((await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(ids.owner), context))!.requests[0]!.status, 'COMPLETED');
    await host.close(); host = await openResearch(options);
    const read = await readNativeTable(table, 219999, 1, async key => (await host.harness.snapshot(NativeEvidencePart, root.id, key, context))!.data);
    assert.equal((read[0] as JsonObject).quantity, '0.12345678901234567890');
    const count = methods.length;
    assert.equal(await reconcileNativeExecution(ids.owner, runtime(), ids.child, context), true);
    assert.equal(methods.length, count, 'adopted receipt survives reopen without downloading again');
    assert.deepEqual(methods, ['GET', 'GET'], 'Retired HTTP cancellation preserves UNKNOWN without sending DELETE.');
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});
