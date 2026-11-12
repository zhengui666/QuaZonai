import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { CatalogScopeTask } from '../plugins/native-catalog-scope.ts';
import { readNativeEvidence } from '../plugins/native-evidence.ts';
import { NativeRemoteUncertainty, executeNativeValue, withNativeRemoteTracking, cancelUnsettledNativeExecutions } from '../src/native-execution.ts';
import { reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';
import type { NativeExecutionRequest, NativeJson } from '../../../packages/contracts/src/nautilus.ts';

// These exercise the real Durable lifecycle and HTTP receipt protocol, not scientific results.
const context = BACKGROUND_CONTEXT;
const implementationDigest = `sha256:${'a'.repeat(64)}`;
const version = JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8')).components.schemas.NativeModelRefV1.oneOf[0].properties.upstream_version.enum[0] as string;
const request: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
  event_start_ns: '0', event_end_ns: '20', available_end_ns: '20', venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' },
  strategy: { kind: 'EMA_CROSS', trade_size: '0.01', fast_period: 2, slow_period: 3 } };
function wireResult(original: JsonObject, large = false): JsonObject {
  return { schema_version: 1, request: original, native_version: version, data_rows_by_type: { QuoteTick: '20' }, notes: [],
    instrument_definition: { mode: 'HISTORICAL_AS_OF_START', ts_init_ns: '0', ts_event_ns: '0', scope: 'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START' }, equity_performance: { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: 'UNAVAILABLE', reason: 'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE', currency: null, returns: [], statistics: [252, 365].flatMap(annualization_days => ['CAGR', 'Sharpe Ratio'].map(name => ({ native_key: `${name} (${annualization_days} days)`, annualization_days, value: null }))), first_snapshot_ns: null, last_snapshot_ns: null, snapshot_count: '0', observed_utc_days: '0', unpriced_snapshot_count: '0', return_days: '0', notes: ['Protocol fixture, not measured performance'] }, canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], components: {}, statistics: {},
      ...(large ? { payload: 'x'.repeat(2 * 1024 * 1024) } : {}) } };
}

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

for (const mode of ['native', 'large', 'scope'] as const) test(`scan ${mode} adopts an original UNKNOWN receipt after restart without repeating earlier calls`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-scan-receipt-'));
  const posts: NativeExecutionRequest[] = [];
  let completed = false;
  const uncertainCommand = mode === 'scope' ? 'catalog-factor-scope' : 'native-strategy-backtest';
  const value = (original: NativeExecutionRequest): NativeJson => original.command === 'catalog-factor-scope' ? ['TEST.SIM-1-MINUTE-LAST-EXTERNAL']
    : wireResult(original.input as JsonObject, mode === 'large');
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['native-strategy-backtest', 'catalog-factor-scope'] })); return; }
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
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'scan-receipt-protocol', tasks: [CatalogScopeTask] })] };
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    const data = { catalog: directory, request, nautilusTarget: target };
    const scanContext = { scanId: 'original-scan', ordinal: '0', implementationDigest };
    const id = await root.commit(tx => mode === 'scope'
      ? tx.createTask(CatalogScopeTask, { catalog: directory, binary: target, scanContext }, { ownership: { kind: 'conversation' } })
      : tx.createTask(NativeStrategyBacktestTask, { request, catalog: directory, target, scanContext }, { ownership: { kind: 'conversation' } }), context);
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
      if (mode === 'native') assert.deepEqual(output.request, request);
      else if (mode === 'scope') {
        const evidence = await readNativeEvidence(host.harness, root.id, id, output.native as JsonObject, context);
        assert.deepEqual(evidence.barTypes, ['TEST.SIM-1-MINUTE-LAST-EXTERNAL']);
        assert.equal(output.count, '1'); assert.equal(output.taskId, id); assert.equal(output.conversationId, root.id);
      } else {
        assert.equal(((output.canonical_result as JsonObject).payload as string).length, 2 * 1024 * 1024);
        assert.deepEqual(output.request, request);
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
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['native-strategy-backtest'] })); return; }
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
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [] });
  try {
    const root = await host.harness.root(context);
    const id = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request, catalog: directory, target,
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

test('ordinary upstream strategy UNKNOWN retains its original receipt and scan implementation changes reject before POST', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-scan-version-'));
  let posts = 0;
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['native-strategy-backtest'] })); return; }
    posts++; let body = ''; for await (const chunk of incoming) body += chunk;
    const original = JSON.parse(body) as NativeExecutionRequest;
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'UNKNOWN', error: 'receipt_unavailable' }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'scan-version-protocol', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [] });
  try {
    const root = await host.harness.root(context);
    const input = { request, catalog: directory, target };
    const ordinary = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, input, { ownership: { kind: 'conversation' } }), context);
    await waitingForOriginalReceipt(host, ordinary);
    assert.equal(posts, 1);
    const changed = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, { ...input, scanContext: { scanId: 'changed-build', ordinal: '0', implementationDigest: 'unmatched-existing-build-identity' } }, { ownership: { kind: 'conversation' } }), context);
    const result = await host.harness.waitForTask(changed, context);
    assert.equal(result.state.outcome.status, 'failed');
    if (result.state.outcome.status === 'failed') assert.match(result.state.outcome.error.message, /nautilus_scan_implementation_changed/);
    assert.equal(posts, 1, 'a scan does not start science on a changed implementation');
    await host.harness.abortTask(ordinary, context);
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
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['native-strategy-backtest'] })); return; }
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
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [] });
  try {
    const root = await host.harness.root(context);
    const id = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request, catalog: directory, target,
      ...(scan ? { scanContext: { scanId: 'active-stopped-scan', ordinal: '0' } } : {}) }, { ownership: { kind: 'conversation' } }), context);
    const deadline = Date.now() + 5000;
    for (;;) {
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
      if (reads > 0 && tasks.items[0]?.state.status === 'sleeping') {
        assert.equal((tasks.items[0].input as JsonObject).receiptRequired, true); break;
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
    assert.equal(facts!.requests[0]!.status, 'UNKNOWN');
    {
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

for (const retrying of [false, true]) test(`economic replay capacity cancellation preserves ${retrying ? 'an in-flight retry' : 'explicit non-admission'}`, { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-scan-capacity-stop-'));
  let posts = 0; let deletes = 0;
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest, commands: ['native-strategy-backtest'] })); return; }
    if (incoming.method === 'POST') {
      posts++; let body = ''; for await (const chunk of incoming) body += chunk;
      const original = JSON.parse(body) as NativeExecutionRequest;
      if (posts === 1) {
        response.statusCode = 429;
        response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, admission: 'REJECTED', error: 'nautilus_tick_capacity_exhausted' }));
      }
      return;
    }
    if (incoming.method === 'DELETE') deletes++;
    response.statusCode = 404; response.end('{"error":"not_found"}');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'scan-capacity-stop-protocol', url: `http://127.0.0.1:${address.port}` };
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const ReplayProbe = defineTask<{}, { phase: 'compute' }, null>({
    name: 'test.replay-capacity-probe', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      await withNativeRemoteTracking(task.id, runtime, ctx, () => executeNativeValue(target, 'native-strategy-backtest', {}, [], directory, ctx));
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
    } },
    async abort(task, runtime, ctx) {
      await cancelUnsettledNativeExecutions(task.id, runtime, ctx);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx);
    },
  });
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [defineExtension({ name: 'scan-capacity-stop-protocol', tasks: [ReplayProbe] })] });
  try {
    const root = await host.harness.root(context);
    const id = await root.commit(tx => tx.createTask(ReplayProbe, {}, { ownership: { kind: 'conversation' } }), context);
    const deadline = Date.now() + 8000;
    for (;;) {
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
      const native = tasks.items[0];
      if (retrying ? posts === 2 : native?.state.status === 'sleeping') break;
      assert.ok(Date.now() < deadline, 'original invocation did not reach capacity wait');
      await delay(5);
    }
    await host.harness.abortTask(id, context);
    assert.equal((await host.harness.waitForTask(id, context)).state.outcome.status, 'aborted');
    const facts = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context);
    assert.equal(facts!.requests[0]!.status, retrying ? 'UNKNOWN' : 'REJECTED');
    assert.equal(posts, retrying ? 2 : 1);
    assert.equal(deletes === 0, !retrying);
    await reconcileNativeRemoteTasks(host.harness, [{ id, conversationId: root.id }], context);
    assert.equal((await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context))!.requests[0]!.status, retrying ? 'UNKNOWN' : 'REJECTED');
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
