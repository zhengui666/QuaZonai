import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { nativeCommands } from '../../research/src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';
import type { ParameterScanPlan, ParameterScanView } from '../../../packages/contracts/src/parameter-scans.ts';
import { setup, context } from './support.ts';

// HTTP/Pi protocol only: controlled responses are NOT performance evidence.
async function protocol(mode: 'complete' | 'running' | 'invalid' | 'rank' = 'complete') {
  const state = await setup(); const requests: NativeExecutionRequest[] = []; let deletes = 0; let settle = false;
  const version = '0.69.0';
  const result = (request: JsonObject): JsonObject => {
    if (mode === 'invalid') return { request, simulation: {}, observations: [] };
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(Number((request.strategy as JsonObject).fast_period));
    return { schema_version: 1, request, native_version: version,
      canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], components: {}, statistics: { general: {}, returns: { 'Sharpe Ratio (252 days)': mode === 'rank' ? bytes.toString('hex') : null }, pnls: {} } },
      data_rows_by_type: { QuoteTick: '20' }, notes: [] };
  };
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') {
      let body = ''; for await (const chunk of incoming) body += chunk;
      const request = JSON.parse(body) as NativeExecutionRequest; requests.push(request);
      const output = result(request.input as JsonObject);
      response.end(JSON.stringify({ schemaVersion: 1, requestId: request.requestId, status: mode === 'running' ? 'RUNNING' : 'COMPLETED', ...(mode === 'running' ? {} : { result: output }) })); return;
    }
    if (incoming.method === 'DELETE') { deletes++; settle = true; }
    response.end(JSON.stringify({ schemaVersion: 1, requestId: incoming.url!.split('/').at(-1), status: settle ? 'CANCELLED' : 'UNKNOWN' }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const target = { id: 'controlled-native-scan', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
  const request: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
    event_start_ns: '9007199254740993', event_end_ns: '9007199254741993', available_end_ns: '9007199254741993',
    venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: 'USD', leverage: '1', maker_fee: '0', taker_fee: '0.000000000000000001' },
    strategy: { kind: 'EMA_CROSS', trade_size: '0.500000000000000001', fast_period: 14, slow_period: 50 } };
  const researchId = await state.service.research.harness.commit(async tx => {
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } }); const study = await tx.doc(Study, conversation.id);
    study.input = { goal: 'Controlled native scan routing', model: state.model, data: { catalog: '/registered/catalog', nautilusTarget: target, native_strategy_backtest_request: request }, policy: {} };
    return conversation.id;
  }, context);
  const plan: ParameterScanPlan = { name: 'Native numeric grid', source: { kind: 'research', researchId }, mode: 'STRATEGY', axes: [{ key: 'strategy.fast_period', domain: { kind: 'values', values: ['2', '3', '4', '5'] } }], objective: { metricId: 'Sharpe Ratio (252 days)', partition: 'FULL_SELECTION', direction: 'minimize' }, execution: { concurrency: 2 } };
  return { state, requests, request, plan, researchId, get deletes() { return deletes; }, async close() { await state.cleanup(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } };
}
async function waitUntil(check: () => boolean | Promise<boolean>) {
  const deadline = Date.now() + 10000;
  while (!await check()) { assert.ok(Date.now() < deadline, 'original Pi task must progress'); await new Promise(resolve => setTimeout(resolve, 15)); }
}
test('native scan dispatches independent official strategies without rewriting configuration', { timeout: 20000 }, async () => {
  const p = await protocol('complete');
  try {
    const app = p.state.service.app; const headers = p.state.headers;
    const options = await app.inject({ method: 'GET', url: `/api/v3/parameter-scans/options?sourceKind=research&researchId=${p.researchId}`, headers });
    assert.equal(options.statusCode, 200, options.body); assert.ok(options.json().parameters.some((row: { key: string }) => row.key === 'strategy.slow_period'));
    assert.ok(options.json().parameters.some((row: { key: string }) => row.key === 'venue.taker_fee'));
    const preview = await app.inject({ method: 'POST', url: '/api/v3/parameter-scans/preview', headers, payload: p.plan });
    assert.equal(preview.json().objective.direction, 'minimize');
    const input = { ...p.plan, requestId: 'native-request', execution: { concurrency: 2 } };
    const replies = await Promise.all([0, 1].map(() => app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers, payload: input })));
    assert.equal(replies[0]!.statusCode, 202, replies[0]!.body); assert.equal(replies[0]!.json().id, replies[1]!.json().id);
    const view = replies[0]!.json() as ParameterScanView;
    await p.state.service.research.harness.waitForTask(view.taskId as TaskId, context);
    const detail = (await app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}`, headers })).json();
    assert.equal(detail.counts.completed, '4'); assert.equal(detail.counts.comparable, '0'); assert.equal(detail.best, null);
    assert.equal(detail.objective.direction, 'minimize'); assert.equal(p.requests.length, 4);
    const cases = p.requests.map(row => row.input as JsonObject);
    assert.deepEqual(cases.map(row => (row.strategy as JsonObject).fast_period).sort(), [2, 3, 4, 5]);
    for (const row of cases) { assert.equal(row.event_start_ns, p.request.event_start_ns); assert.deepEqual(row.venue, p.request.venue); assert.equal((row.strategy as JsonObject).trade_size, '0.500000000000000001'); }
    assert.ok(p.requests.every(row => row.command === 'native-strategy-backtest'));
    const trial = (await app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}/trials/3`, headers })).json();
    assert.equal(trial.details.effectiveRequest.strategy.fast_period, 5); assert.equal(trial.metric.reason, 'metric_unavailable');
  } finally { await p.close(); }
});
test('native scan rejects missing numeric leaves and expanding registered data scope without dispatch', async () => {
  const p = await protocol(); try {
    for (const key of ['invented.native_parameter', 'event_start_ns']) {
      const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans/preview', headers: p.state.headers, payload: { ...p.plan, execution: { concurrency: 1 }, axes: [{ key, domain: { kind: 'values', values: ['1', '2'] } }] } });
      assert.equal(response.statusCode, 422, response.body);
    }
    assert.equal(p.requests.length, 0);
  } finally { await p.close(); }
});
test('removed batchSize is rejected instead of silently changing execution semantics', async () => {
  const p = await protocol(); try {
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans/preview', headers: p.state.headers,
      payload: { ...p.plan, execution: { concurrency: 2, batchSize: 2 } } });
    assert.equal(response.statusCode, 422, response.body); assert.equal(p.requests.length, 0);
  } finally { await p.close(); }
});
test('bad native evidence fails every candidate rather than publishing partial science', async () => {
  const p = await protocol('invalid'); try {
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'bad-evidence', execution: { concurrency: 1 } } });
    assert.equal(response.statusCode, 202, response.body); const view = response.json();
    await p.state.service.research.harness.waitForTask(view.taskId as TaskId, context);
    const detail = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}`, headers: p.state.headers })).json();
    assert.equal(detail.counts.failed, '4'); assert.equal(detail.counts.completed, '0'); assert.equal(detail.best, null);
  } finally { await p.close(); }
});
test('stopping a native scan cancels the original Pi-owned receipt once and dispatches no later points', { timeout: 20000 }, async () => {
  const p = await protocol('running'); try {
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'stop-original', execution: { concurrency: 1 } } });
    assert.equal(response.statusCode, 202, response.body); const view = response.json();
    await waitUntil(() => p.requests.length === 1);
    await p.state.restart();
    const stop = await p.state.service.app.inject({ method: 'POST', url: `/api/v3/parameter-scans/${view.id}/stop`, headers: p.state.headers }); assert.equal(stop.statusCode, 202, stop.body);
    await waitUntil(async () => (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}`, headers: p.state.headers })).json().status === 'STOPPED');
    assert.equal(p.requests.length, 1); assert.ok(p.deletes >= 1);
  } finally { await p.close(); }
});

test('ranking reflects the requested direction and original statistic keys, not a custom score', async () => {
  // Controlled wire values test ordering only, not a real strategy performance comparison.
  const p = await protocol('rank'); try {
    for (const direction of ['minimize', 'maximize'] as const) {
      const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: direction, objective: { ...p.plan.objective, direction } } });
      assert.equal(response.statusCode, 202, response.body); const id = response.json().id;
      await p.state.service.research.harness.waitForTask(response.json().taskId as TaskId, context);
      const detail = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${id}`, headers: p.state.headers })).json();
      assert.equal(detail.best.metric.value, direction === 'minimize' ? 2 : 5);
      assert.equal(detail.best.metric.key, 'Sharpe Ratio (252 days)');
      const trials = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${id}/trials?sort=metric`, headers: p.state.headers })).json();
      assert.equal(trials.items[0].metric.value, direction === 'minimize' ? 2 : 5);
    }
  } finally { await p.close(); }
});

test('HTTP preview and execution reject collapsed f64 points but preserve decimal-string distinctions', async () => {
  const p = await protocol(); try {
    const domains = [{ kind: 'values', values: ['30.000000000000000001', '30.000000000000000002'] }, { kind: 'range', start: '30', end: '30.00000000000000001', step: '0.000000000000000001' }];
    for (const domain of domains) for (const endpoint of ['/preview', '']) {
      const response = await p.state.service.app.inject({ method: 'POST', url: `/api/v3/parameter-scans${endpoint}`, headers: p.state.headers, payload: { ...p.plan, requestId: 'unrepresentable', axes: [{ key: 'strategy.fast_period', domain }] } });
      assert.equal(response.statusCode, 422, response.body); assert.equal(response.json().error, 'parameter_scan_native_number_indistinguishable');
    }
    assert.equal(p.requests.length, 0);
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'exact-decimals', axes: [{ key: 'strategy.trade_size', domain: { kind: 'values', values: ['0.500000000000000001', '0.500000000000000002'] } }] } });
    assert.equal(response.statusCode, 202, response.body);
    await p.state.service.research.harness.waitForTask(response.json().taskId as TaskId, context);
    assert.deepEqual(p.requests.map(row => ((row.input as JsonObject).strategy as JsonObject).trade_size).sort(), ['0.500000000000000001', '0.500000000000000002']);
  } finally { await p.close(); }
});
