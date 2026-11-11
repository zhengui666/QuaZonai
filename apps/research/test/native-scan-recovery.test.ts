import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineExtension } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { FactorTask } from '../plugins/native-factor-task.ts';
import { PortfolioEvaluationTask } from '../plugins/native-portfolio-task.ts';
import { CatalogScopeTask } from '../plugins/native-catalog-scope.ts';
import { readNativeEvidence } from '../plugins/native-evidence.ts';
import { NativeRemoteUncertainty } from '../src/native-execution.ts';
import { reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';
import type { NativeExecutionRequest, NativeJson } from '../../../packages/contracts/src/nautilus.ts';

// These exercise the real Durable lifecycle and HTTP receipt protocol, not scientific results.
const context = BACKGROUND_CONTEXT;
const implementationDigest = `sha256:${'a'.repeat(64)}`;
const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: {
  schema_version: 1, selection: { schema_version: 1, event_start_ns: '0', event_end_ns: '20', decision_cutoff_ns: '20', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] },
  parameters: { schema_version: 1, fast_period: 2, slow_period: 3, label_horizon_observations: 1 },
} };
const policy = { name: 'Receipt protocol only', kind: 'factor-threshold' as const, instrumentId: 'TEST.SIM', threshold: 0, investedWeight: '1', targetTtlNs: '5',
  simulationSettings: { base_currency: 'USD', starting_capital: '1000' } };

async function waitingForOriginalReceipt(host: Awaited<ReturnType<typeof openResearch>>, id: TaskId) {
  const deadline = Date.now() + 5000;
  for (;;) {
    const current = await host.harness.getTask(id, context);
    assert.ok(current);
    if (current.state.status === 'sleeping' && (current.state.checkpoint as { phase: string }).phase === 'reconcile') return current;
    assert.notEqual(current.state.status, 'terminal', JSON.stringify(current.state));
    assert.ok(Date.now() < deadline, 'science task did not retain its original receipt checkpoint');
    await delay(5);
  }
}

for (const mode of ['factor', 'strategy', 'scope'] as const) test(`scan ${mode} adopts an original UNKNOWN receipt after restart without repeating earlier calls`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-scan-receipt-'));
  const posts: NativeExecutionRequest[] = [];
  let completed = false;
  const uncertainCommand = mode === 'factor' ? 'evaluate-factor' : mode === 'scope' ? 'catalog-factor-scope' : 'simulate';
  const value = (original: NativeExecutionRequest): NativeJson => {
    if (original.command === 'catalog-factor-scope') return ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'];
    if (original.command === 'evaluate-factor') return { request: original.input, partitions: [], acceptedOriginal: true };
    if (original.command === 'forecast') return { points: [{ instrument_id: 'TEST.SIM', event_ns: '10', available_ns: '11', forecast: 1 }] };
    if (original.command === 'blend-targets') return { target_points: ((original.input as JsonObject).rows as JsonObject[][]).map(row => row[0]!) };
    return { native_version: 'receipt-protocol-only', statistics: [], canonical_result: { original: true, payload: 'x'.repeat(2 * 1024 * 1024) } };
  };
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['evaluate-factor', 'forecast', 'blend-targets', 'simulate', 'catalog-factor-scope'] })); return; }
    let original: NativeExecutionRequest | undefined;
    if (incoming.method === 'POST') {
      let body = ''; for await (const chunk of incoming) body += chunk;
      original = JSON.parse(body) as NativeExecutionRequest; posts.push(original);
    } else original = posts.find(item => incoming.url === `/v1/executions/${item.requestId}`);
    assert.ok(original);
    const unavailable = original.command === uncertainCommand && !completed;
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: unavailable ? 'UNKNOWN' : 'COMPLETED',
      ...(unavailable ? { error: 'receipt_unavailable' } : { result: value(original) }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'scan-receipt-protocol', url: `http://127.0.0.1:${address.port}` };
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'scan-receipt-protocol', tasks: [FactorTask, PortfolioEvaluationTask, CatalogScopeTask] })] };
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    const data = { catalog: directory, request, nautilusTarget: target };
    const scanContext = { scanId: 'original-scan', ordinal: '0', implementationDigest };
    const id = await root.commit(tx => mode === 'factor'
      ? tx.createTask(FactorTask, { model: '(module)', data, binary: target, scanContext }, { ownership: { kind: 'conversation' } })
      : mode === 'scope' ? tx.createTask(CatalogScopeTask, { catalog: directory, binary: target, scanContext }, { ownership: { kind: 'conversation' } })
      : tx.createTask(PortfolioEvaluationTask, { members: [{ model: '(module)', weight: '1' }], policy, data, partition: 'VALIDATION', binary: target, scanContext }, { ownership: { kind: 'conversation' } }), context);
    await waitingForOriginalReceipt(host, id);
    const before = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 100), context);
    const uncertain = before.items.find(item => (item.input as JsonObject).command === uncertainCommand)!;
    assert.equal(uncertain.state.status, 'terminal'); assert.equal(uncertain.state.outcome!.status, 'failed');
    const originalPosts = structuredClone(posts);
    await host.close();
    completed = true;
    host = await openResearch({ ...options, deferResume: true });
    await reconcileNativeRemoteTasks(host.harness, [{ id, conversationId: root.id }], context);
    const observed = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context);
    assert.ok(observed!.requests.every(item => item.status === 'COMPLETED'), 'state-only observation can precede result adoption');
    host.harness.resume();
    const result = await host.harness.waitForTask(id, context);
    assert.equal(result.state.outcome.status, 'completed');
    if (result.state.outcome.status === 'completed') {
      const output = result.state.outcome.result as JsonObject;
      if (mode === 'factor') assert.equal(output.acceptedOriginal, true);
      else if (mode === 'scope') {
        const evidence = await readNativeEvidence(host.harness, root.id, id, output.native as JsonObject, context);
        assert.deepEqual(evidence.barTypes, ['TEST.SIM-1-MINUTE-LAST-EXTERNAL']);
        assert.equal(output.count, '1'); assert.equal(output.taskId, id); assert.equal(output.conversationId, root.id);
      } else {
        const evidence = await readNativeEvidence(host.harness, root.id, id, output.native as JsonObject, context);
        assert.equal(((evidence.simulation as JsonObject).canonical_result as JsonObject).original, true);
        assert.equal((((evidence.simulation as JsonObject).canonical_result as JsonObject).payload as string).length, 2 * 1024 * 1024);
        assert.ok(JSON.stringify(output).length < 10_000, 'strategy task keeps original native evidence by reference');
      }
    }
    assert.deepEqual(posts, originalPosts, 'only GET observes the original invocation; completed preparation is reused');
    const after = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 100), context);
    assert.deepEqual(after.items.map(item => item.id), before.items.map(item => item.id));
    assert.deepEqual(after.items.find(item => item.id === uncertain.id)!.state, uncertain.state, 'the official terminal native task is never rewritten');
    assert.equal(faux.state.callCount, 0);
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});

for (const cancellation of ['CANCELLED', 'missing'] as const) test(`stopping a scan waiting for an UNKNOWN receipt records ${cancellation} without another POST`, { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-scan-stop-'));
  let original: NativeExecutionRequest | undefined; let posts = 0; let lateCompletion = false; const cancelled: string[] = [];
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['evaluate-factor'] })); return; }
    if (incoming.method === 'POST') {
      posts++; let body = ''; for await (const chunk of incoming) body += chunk; original = JSON.parse(body) as NativeExecutionRequest;
    }
    assert.ok(original);
    if (incoming.method === 'DELETE') {
      cancelled.push(incoming.url!);
      if (cancellation === 'missing') { response.statusCode = 404; response.end('{"error":"not_found"}'); return; }
      response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'CANCELLED' })); return;
    }
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: lateCompletion ? 'COMPLETED' : 'UNKNOWN',
      ...(lateCompletion ? { result: { request: original.input } } : { error: 'receipt_unavailable' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'scan-cancellation-protocol', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'scan-cancellation-protocol', tasks: [FactorTask] })] });
  try {
    const root = await host.harness.root(context);
    const id = await root.commit(tx => tx.createTask(FactorTask, { model: '(module)', data: { catalog: directory, request }, binary: target,
      scanContext: { scanId: 'stopped-scan', ordinal: '0' } }, { ownership: { kind: 'conversation' } }), context);
    await waitingForOriginalReceipt(host, id);
    await host.harness.abortTask(id, context);
    const stopped = await host.harness.waitForTask(id, context);
    assert.equal(stopped.state.outcome.status, 'aborted');
    assert.deepEqual(cancelled, [`/v1/executions/${original!.requestId}`]); assert.equal(posts, 1);
    const facts = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context);
    assert.equal(facts!.requests[0]!.status, cancellation === 'CANCELLED' ? 'CANCELLED' : 'UNKNOWN');
    if (cancellation === 'missing') {
      lateCompletion = true;
      await reconcileNativeRemoteTasks(host.harness, [{ id, conversationId: root.id }], context);
      assert.equal((await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context))!.requests[0]!.status, 'COMPLETED');
      assert.equal((await host.harness.getTask(id, context))!.state.outcome!.status, 'aborted', 'a late receipt settles facts without reopening stopped science');
      assert.equal(posts, 1);
    }
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});

test('ordinary factor UNKNOWN behavior stays terminal and scan implementation changes reject before POST', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-scan-version-'));
  let posts = 0;
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['evaluate-factor'] })); return; }
    posts++; let body = ''; for await (const chunk of incoming) body += chunk;
    const original = JSON.parse(body) as NativeExecutionRequest;
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'UNKNOWN', error: 'receipt_unavailable' }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'scan-version-protocol', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'scan-version-protocol', tasks: [FactorTask] })] });
  try {
    const root = await host.harness.root(context);
    const input = { model: '(module)', data: { catalog: directory, request }, binary: target };
    const ordinary = await root.commit(tx => tx.createTask(FactorTask, input, { ownership: { kind: 'conversation' } }), context);
    assert.equal((await host.harness.waitForTask(ordinary, context)).state.outcome.status, 'failed');
    assert.equal(posts, 1);
    const changed = await root.commit(tx => tx.createTask(FactorTask, { ...input, scanContext: { scanId: 'changed-build', ordinal: '0', implementationDigest: 'unmatched-existing-build-identity' } }, { ownership: { kind: 'conversation' } }), context);
    const result = await host.harness.waitForTask(changed, context);
    assert.equal(result.state.outcome.status, 'failed');
    if (result.state.outcome.status === 'failed') assert.match(result.state.outcome.error.message, /nautilus_scan_implementation_changed/);
    assert.equal(posts, 1, 'a scan does not start science on a changed implementation');
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});

for (const scan of [true, false]) test(`${scan ? 'scan' : 'ordinary'} active native cancellation preserves its receipt requirement on DELETE 404`, { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-scan-active-stop-'));
  let original: NativeExecutionRequest | undefined; let posts = 0; let reads = 0; const cancelled: string[] = [];
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['evaluate-factor'] })); return; }
    if (incoming.method === 'POST') {
      posts++; let body = ''; for await (const chunk of incoming) body += chunk;
      original = JSON.parse(body) as NativeExecutionRequest;
      assert.equal(Object.hasOwn(original, 'receiptRequired'), false, 'cancellation policy is private Task input, not native Wire');
    }
    assert.ok(original);
    if (incoming.method === 'DELETE') {
      cancelled.push(incoming.url!); response.statusCode = 404; response.end('{"error":"not_found"}'); return;
    }
    if (incoming.method === 'GET') reads++;
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'RUNNING' }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'scan-active-cancellation-protocol', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'scan-active-cancellation-protocol', tasks: [FactorTask] })] });
  try {
    const root = await host.harness.root(context);
    const id = await root.commit(tx => tx.createTask(FactorTask, { model: '(module)', data: { catalog: directory, request }, binary: target,
      ...(scan ? { scanContext: { scanId: 'active-stopped-scan', ordinal: '0' } } : {}) }, { ownership: { kind: 'conversation' } }), context);
    const deadline = Date.now() + 5000;
    for (;;) {
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
      if (reads > 0 && tasks.items[0]?.state.status === 'sleeping') {
        assert.equal((tasks.items[0].input as JsonObject).receiptRequired, scan ? true : undefined); break;
      }
      assert.ok(Date.now() < deadline, 'original invocation did not reach RUNNING receipt polling');
      await delay(5);
    }
    await host.harness.abortTask(id, context);
    assert.equal((await host.harness.waitForTask(id, context)).state.outcome.status, 'aborted');
    assert.ok(cancelled.length > 0);
    assert.ok(cancelled.every(path => path === `/v1/executions/${original!.requestId}`));
    assert.equal(posts, 1);
    const facts = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context);
    assert.equal(facts!.requests[0]!.status, scan ? 'UNKNOWN' : 'CANCELLED');
    if (scan) {
      assert.equal(facts!.requests[0]!.error, 'nautilus_cancellation_unknown');
      await reconcileNativeRemoteTasks(host.harness, [{ id, conversationId: root.id }], context);
      assert.equal((await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context))!.requests[0]!.status, 'UNKNOWN', 'a still-running GET is not a cancellation confirmation');
      assert.equal(posts, 1);
    }
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
