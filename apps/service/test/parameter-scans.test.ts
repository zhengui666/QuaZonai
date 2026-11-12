import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { IndicatorConfiguration } from '../../research/plugins/indicator-strategy.ts';
import { nativeCommands } from '../../research/src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';
import type { ParameterScanPlan, ParameterScanView } from '../../../packages/contracts/src/parameter-scans.ts';
import { setup, context } from './support.ts';

// HTTP/Pi protocol only: controlled responses are NOT performance evidence.
async function protocol(mode: 'complete' | 'running' | 'invalid' | 'rank' = 'complete', configured = false) {
  const state = await setup(); const requests: NativeExecutionRequest[] = []; let deletes = 0; let settle = false;
  const version = '0.69.0';
  const result = (request: JsonObject): JsonObject => {
    if (mode === 'invalid') return { request, simulation: {}, observations: [] };
    const summary = { 'orders.open': '0', 'orders.inflight': '0' };
    return { request, observations: [], performance_scope: 'FULL_SELECTION_INCLUDING_PRE_DECISION_CASH', decision_policy: 'BATCH_LAST_SETTLED_ACCOUNT_ONLY', skipped_unsettled_decisions: '0', merged_same_time_bars: '0', simulation: {
      schema_version: 1, native_version: version, iterations: '20', events: '0', orders: '0', positions: '0', consumed_target_points: '1', settled_replay_ns: '200', summary,
      statistics: [{ group: 'RETURNS', native_key: 'Sharpe Ratio (252 days)', currency: null, value: mode === 'rank' ? Number((request.signal as JsonObject).period) : null, reason_code: mode === 'rank' ? null : 'CONTROLLED_PROTOCOL_NOT_PERFORMANCE_EVIDENCE' }], returns_kind: 'PORTFOLIO_DAILY', returns_status: 'INSUFFICIENT_DATA', returns_reason: 'PORTFOLIO_DAILY_RETURNS_UNAVAILABLE', returns: [],
      canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'stopped', iterations: '20', total_events: '0', total_orders: '0', total_positions: '0' }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], components: {}, statistics: {}, summary },
    } };
  };
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') {
      let body = ''; for await (const chunk of incoming) body += chunk;
      const request = JSON.parse(body) as NativeExecutionRequest; requests.push(request);
      const batch = request.command === 'simulate-indicator-strategy-batch';
      const output = batch ? { schema_version: 1, native_version: version, reuse: 'NAUTILUS_ENGINE_RESET', results: ((request.input as JsonObject).requests as JsonObject[]).map(result) } : result(request.input as JsonObject);
      response.end(JSON.stringify({ schemaVersion: 1, requestId: request.requestId, status: mode === 'running' ? 'RUNNING' : 'COMPLETED', ...(mode === 'running' ? {} : { result: output }) })); return;
    }
    if (incoming.method === 'DELETE') { deletes++; settle = true; }
    response.end(JSON.stringify({ schemaVersion: 1, requestId: incoming.url!.split('/').at(-1), status: settle ? 'CANCELLED' : 'UNKNOWN' }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const target = { id: 'controlled-native-scan', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
  const request: JsonObject = { schema_version: 1, selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '9007199254740993', event_end_ns: '9007199254741993', decision_cutoff_ns: '9007199254741993', maximum_rows: 0 },
    settings: { base_currency: 'USD', fee_rates: [{ instrument_id: 'TEST.SIM', maker: '0', taker: '0.000000000000000001' }], fee_model: { upstream_version: version }, fill_model: { upstream_version: version }, latency_model: { upstream_version: version, parameters: { insert_latency_ns: '9007199254740993' } } },
    signal: { kind: 'RSI', period: 14, entry_below: 30, exit_above: 70 }, long_weight: '0.500000000000000001', target_ttl_seconds: 60, decision_start_ns: '9007199254740993' };
  const researchId = await state.service.research.harness.commit(async tx => {
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } }); const study = await tx.doc(Study, conversation.id);
    study.input = { goal: 'Controlled native scan routing', model: state.model, data: { catalog: '/registered/catalog', nautilusTarget: target, ...(!configured ? { indicator_strategy_request: request } : {}) }, policy: {} };
    if (configured) (await tx.doc(IndicatorConfiguration, conversation.id)).request = request;
    return conversation.id;
  }, context);
  const plan: ParameterScanPlan = { name: 'Native numeric grid', source: { kind: 'research', researchId }, mode: 'STRATEGY', axes: [{ key: 'signal.period', domain: { kind: 'values', values: ['2', '3', '4', '5'] } }], objective: { metricId: 'Sharpe Ratio (252 days)', partition: 'FULL_SELECTION', direction: 'minimize' }, execution: { concurrency: 2 } };
  return { state, requests, request, plan, researchId, get deletes() { return deletes; }, async close() { await state.cleanup(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } };
}
async function waitUntil(check: () => boolean | Promise<boolean>) {
  const deadline = Date.now() + 10000;
  while (!await check()) { assert.ok(Date.now() < deadline, 'original Pi task must progress'); await new Promise(resolve => setTimeout(resolve, 15)); }
}
for (const batchSize of [1, 2]) test(`native scan service dispatches exact configurations with explicit batchSize ${batchSize}`, { timeout: 20000 }, async () => {
  const p = await protocol('complete', true);
  try {
    const app = p.state.service.app; const headers = p.state.headers;
    const options = await app.inject({ method: 'GET', url: `/api/v3/parameter-scans/options?sourceKind=research&researchId=${p.researchId}`, headers });
    assert.equal(options.statusCode, 200, options.body); assert.ok(options.json().parameters.some((row: { key: string }) => row.key === 'settings.latency_model.parameters.insert_latency_ns'));
    assert.ok(options.json().parameters.some((row: { key: string }) => row.key === 'settings.fee_rates.0.taker'));
    const preview = await app.inject({ method: 'POST', url: '/api/v3/parameter-scans/preview', headers, payload: p.plan });
    assert.equal(preview.json().objective.direction, 'minimize');
    const input = { ...p.plan, requestId: 'native-request', execution: { concurrency: 2, batchSize } };
    const replies = await Promise.all([0, 1].map(() => app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers, payload: input })));
    assert.equal(replies[0]!.statusCode, 202, replies[0]!.body); assert.equal(replies[0]!.json().id, replies[1]!.json().id);
    const view = replies[0]!.json() as ParameterScanView;
    await p.state.service.research.harness.waitForTask(view.taskId as TaskId, context);
    const detail = (await app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}`, headers })).json();
    assert.equal(detail.counts.completed, '4'); assert.equal(detail.counts.comparable, '0'); assert.equal(detail.best, null);
    assert.equal(detail.objective.direction, 'minimize'); assert.equal(p.requests.length, 4 / batchSize);
    const cases = p.requests.flatMap(row => batchSize > 1 ? (row.input as JsonObject).requests as JsonObject[] : [row.input as JsonObject]);
    assert.deepEqual(cases.map(row => (row.signal as JsonObject).period).sort(), [2, 3, 4, 5]);
    for (const row of cases) { assert.deepEqual(row.selection, p.request.selection); assert.deepEqual(row.settings, p.request.settings); assert.equal(row.long_weight, '0.500000000000000001'); }
    assert.ok(p.requests.every(row => row.command === (batchSize > 1 ? 'simulate-indicator-strategy-batch' : 'simulate-indicator-strategy')));
    const trial = (await app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}/trials/3`, headers })).json();
    assert.equal(trial.details.effectiveRequest.signal.period, 5); assert.equal(trial.metric.reason, 'CONTROLLED_PROTOCOL_NOT_PERFORMANCE_EVIDENCE');
  } finally { await p.close(); }
});
test('native scan rejects missing numeric leaves and expanding registered data scope without dispatch', async () => {
  const p = await protocol(); try {
    for (const [key, batchSize] of [['invented.native_parameter', 1], ['selection.event_start_ns', 2]] as const) {
      const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans/preview', headers: p.state.headers, payload: { ...p.plan, execution: { concurrency: 1, batchSize }, axes: [{ key, domain: { kind: 'values', values: ['1', '2'] } }] } });
      assert.equal(response.statusCode, 422, response.body);
    }
    assert.equal(p.requests.length, 0);
  } finally { await p.close(); }
});
test('bad native evidence fails every batch point rather than publishing partial science', async () => {
  const p = await protocol('invalid'); try {
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'bad-evidence', execution: { concurrency: 1, batchSize: 4 } } });
    assert.equal(response.statusCode, 202, response.body); const view = response.json();
    await p.state.service.research.harness.waitForTask(view.taskId as TaskId, context);
    const detail = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}`, headers: p.state.headers })).json();
    assert.equal(detail.counts.failed, '4'); assert.equal(detail.counts.completed, '0'); assert.equal(detail.best, null);
  } finally { await p.close(); }
});
test('stopping a native scan cancels the original Pi-owned batch receipt once and dispatches no later points', { timeout: 20000 }, async () => {
  const p = await protocol('running'); try {
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'stop-original', execution: { concurrency: 1, batchSize: 2 } } });
    assert.equal(response.statusCode, 202, response.body); const view = response.json();
    await waitUntil(() => p.requests.length === 1);
    await p.state.restart();
    const stop = await p.state.service.app.inject({ method: 'POST', url: `/api/v3/parameter-scans/${view.id}/stop`, headers: p.state.headers }); assert.equal(stop.statusCode, 202, stop.body);
    await waitUntil(async () => (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${view.id}`, headers: p.state.headers })).json().status === 'STOPPED');
    assert.equal(p.requests.length, 1); assert.ok(p.deletes >= 1);
  } finally { await p.close(); }
});

test('native settings differences split batches without cropping configuration or inventing comparability', async () => {
  const p = await protocol(); try {
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'native-settings', axes: [{ key: 'settings.latency_model.parameters.insert_latency_ns', domain: { kind: 'values', values: ['9007199254740993', '9007199254740994'] } }, { key: 'signal.period', domain: { kind: 'values', values: ['2', '3'] } }], execution: { concurrency: 2, batchSize: 4 } } });
    assert.equal(response.statusCode, 202, response.body);
    await p.state.service.research.harness.waitForTask(response.json().taskId as TaskId, context);
    assert.equal(p.requests.length, 2);
    const surface = await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${response.json().id}/surface?x=settings.latency_model.parameters.insert_latency_ns&y=signal.period&metricId=Unconfigured%20native%20statistic`, headers: p.state.headers });
    assert.equal(surface.statusCode, 200, surface.body);
    assert.equal(surface.json().metricId, 'Unconfigured native statistic');
    assert.ok(surface.json().cells.every((cell: { metric: { value: unknown } }) => cell.metric.value === null));
    for (const request of p.requests) {
      const rows = (request.input as JsonObject).requests as JsonObject[];
      assert.equal(rows.length, 2); assert.deepEqual(rows[0]!.settings, rows[1]!.settings);
    }
    const seeds = p.requests.map(row => (((((row.input as JsonObject).requests as JsonObject[])[0]!.settings as JsonObject).latency_model as JsonObject).parameters as JsonObject).insert_latency_ns).sort();
    assert.deepEqual(seeds, ['9007199254740993', '9007199254740994']);
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
      const response = await p.state.service.app.inject({ method: 'POST', url: `/api/v3/parameter-scans${endpoint}`, headers: p.state.headers, payload: { ...p.plan, requestId: 'unrepresentable', axes: [{ key: 'signal.entry_below', domain }] } });
      assert.equal(response.statusCode, 422, response.body); assert.equal(response.json().error, 'parameter_scan_native_number_indistinguishable');
    }
    assert.equal(p.requests.length, 0);
    const response = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'exact-decimals', axes: [{ key: 'long_weight', domain: { kind: 'values', values: ['0.500000000000000001', '0.500000000000000002'] } }] } });
    assert.equal(response.statusCode, 202, response.body);
    await p.state.service.research.harness.waitForTask(response.json().taskId as TaskId, context);
    assert.deepEqual(p.requests.map(row => (row.input as JsonObject).long_weight).sort(), ['0.500000000000000001', '0.500000000000000002']);
  } finally { await p.close(); }
});
