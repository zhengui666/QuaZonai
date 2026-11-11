import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import type { TaskId } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { openResearch } from '../../research/src/host.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import strategyPortfolio from '../../research/plugins/strategy-portfolio.ts';
import { FactorTask } from '../../research/plugins/native-factor-task.ts';
import { CatalogScopeTask } from '../../research/plugins/native-catalog-scope.ts';
import { PortfolioEvaluationTask } from '../../research/plugins/native-portfolio-task.ts';
import { TickEconomicTask } from '../../research/plugins/tick-economic.ts';
import { nativeCommands } from '../../research/src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';
import type { ParameterScanPlan, ParameterScanView } from '../../../packages/contracts/src/parameter-scans.ts';
import { ParameterScanRecords, createParameterScanExtension, parameterScanWorkbench } from '../src/parameter-scans.ts';
import { json } from '../src/facts.ts';
import { setup, context, wat } from './support.ts';

// Controlled HTTP receipts exercise routing, isolation and Durable recovery only.
// They perform no market calculation and provide no performance evidence.
async function protocol(mode: 'complete' | 'running' = 'complete', tick = false) {
  const state = await setup(); const requests: NativeExecutionRequest[] = []; let settle = false; let deletes = 0; let completed = false; let changeImplementation = false;
  const result = (request: NativeExecutionRequest) => {
    const input = request.input as Record<string, unknown>;
    if (request.command === 'replay-tick-economic') return { schema_version: 1, request: request.input, model: 'CONTROLLED_PROTOCOL_ONLY', methods: {}, coverage: {}, assets: [],
      days: [{ day_start_ns: '259200000000000', equity_usdt: '3000.000000000000000001', net_return: null, unknown_reason: 'CONTROLLED_PROTOCOL_ONLY' }], metrics: [],
      partition_metrics: [{ partition: 'VALIDATION', days: '1', valid_return_days: '0', sharpe_365: null, annualized_return_365: null }],
      conversion_ns: null, initial_usdc_after_cost: null, funding_usdc: '0', fees_usdc_equivalent: null, fees_by_currency: {}, initial_conversion_cost_usdc: '0', nominal_usd_settled_usdc: '0',
      native_fills: '0', native_rejections: '0', native_perp_pnl_rounding_events: '0', perp_pnl_rounding_bound_usdc: '0', funding_missing_instruments: [],
      targets: [], target_asof_ns: null, target_cash_weight: null, skipped_cash: '0', skipped_minimum: '0', consumed_fuel: null, limitations: ['CONTROLLED_PROTOCOL_ONLY'] };
    if (request.command === 'catalog-factor-scope') return ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'];
    if (request.command === 'forecast') return { points: [{ instrument_id: 'TEST.SIM', event_ns: '10', available_ns: '10', forecast: 1 }, { instrument_id: 'TEST.SIM', event_ns: '15', available_ns: '15', forecast: 1 }] };
    if (request.command === 'blend-targets') return { target_points: (input.rows as unknown[][]).map(row => row[0]) };
    if (request.command === 'simulate') return { statistics: [{ native_key: 'Sharpe Ratio (252 days)', value: null, reason_code: 'CONTROLLED_PROTOCOL_ONLY' }],
      canonical_result: { portfolio_snapshots: [{ ts_event: '10', total_equity: ['3000.000000000000000001 USD'] }, { ts_event: '19', total_equity: ['3001.000000000000000002 USD'] }] } };
    return { request: request.input, native_versions: { protocol: 'CONTROLLED_PROTOCOL_ONLY' }, partitions: [{ partition: 'VALIDATION', metric: {
      counts: { included: '9007199254740995' }, pearson_ic: { value: null, status: 'CONTROLLED_PROTOCOL_ONLY' },
    } }] };
  };
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${(changeImplementation && requests.length ? 'b' : 'a').repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (request.method === 'POST') {
      let encoded = ''; for await (const chunk of request) encoded += chunk;
      const original = JSON.parse(encoded) as NativeExecutionRequest; requests.push(original);
      response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: mode === 'complete' ? 'COMPLETED' : 'RUNNING', ...(mode === 'complete' ? { result: result(original) } : {}) })); return;
    }
    const requestId = request.url!.split('/').at(-1)!;
    const original = requests.find(row => row.requestId === requestId);
    if (!original) { response.statusCode = 404; response.end('{"error":"not_found"}'); return; }
    if (request.method === 'DELETE') deletes++;
    response.end(JSON.stringify({ schemaVersion: 1, requestId, status: settle ? completed ? 'COMPLETED' : 'CANCELLED' : 'UNKNOWN', ...(settle ? completed ? { result: result(original) } : {} : { error: 'CONTROLLED_RECEIPT_NOT_YET_SETTLED' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const target = { id: 'controlled-scan-protocol', url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` };
  const request = { schema_version: 1, forecast: { schema_version: 1, selection: { schema_version: 1,
    bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30', maximum_rows: 0 },
    parameters: { schema_version: 1, fast_period: 2, slow_period: 4, label_horizon_observations: 1 } }, train_end_ns: '10', validation_end_ns: '20' };
  const nativeModel = (adapter_kind: string, upstream_class: string, parameters: Record<string, unknown>) => ({ schema_version: 1, adapter_kind, upstream_class, upstream_version: '0.68.0', parameters });
  const strategy = { name: 'controlled original', kind: 'factor-threshold', instrumentId: 'TEST.SIM', threshold: 0, investedWeight: '0.250000000000000001', targetTtlNs: '60000000000',
    simulationSettings: { schema_version: 1, base_currency: 'USD', starting_capital: '3000', account_kind: 'CASH', leverage: '1', exposure_tolerance: '0.0001', snapshot_interval_ms: 1000,
      fee_rates: [{ instrument_id: 'TEST.SIM', maker: '0', taker: '0' }],
      fee_model: nativeModel('NAUTILUS_MAKER_TAKER', 'nautilus_execution::models::fee::MakerTakerFeeModel', {}),
      fill_model: nativeModel('NAUTILUS_DEFAULT_FILL', 'nautilus_execution::models::fill::DefaultFillModel', { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '42' }),
      latency_model: nativeModel('NAUTILUS_STATIC_LATENCY', 'nautilus_execution::models::latency::StaticLatencyModel', { base_latency_ns: '1', insert_latency_ns: '0', update_latency_ns: '0', cancel_latency_ns: '0' }) } };
  const tickRequest = { schema_version: 1, input: { modality: 'TRADE_TICK', instrument_ids: ['TEST.SIM'], event_start_ns: '0', event_end_ns: '691200000000000', decision_cutoff_ns: '691200000000000',
    representation: 'RECEIVED_TIME_COHORT_FLOW_CACHE_V1', source_revision: 'controlled-tick-source', availability: 'RECEIVED_TIME_OBSERVED', maximum_receive_delay_ns: '86400000000000' },
    sampling: { kind: 'RECEIVED_TIME_COHORT_V1', maximum_cohort_rows: 100 }, target: { kind: 'FIXED_RECEIVED_TIME', horizon_ns: '1000000000' },
    parameters: { fast_period_cohorts: 2, slow_period_cohorts: 4 }, train_end_ns: '259200000000000', validation_end_ns: '432000000000000' };
  const tickPolicy = { scenario: { initial_usdt: '3000', usdt0_per_usdt: '1', spot_fraction: '0.5', gross_fraction: '1', maximum_asset_weight: '0.02', participation_fraction: '0.01',
    perp_taker_fee_bps: '4.5', spot_taker_fee_bps: '7', extra_cost_bps: '1', initial_margin_fraction: '0.1', maintenance_margin_fraction: '0.05', decision_interval_ns: '1000000000',
    base_latency_ns: '100000000', jitter_min_ns: '0', jitter_max_ns: '50000000', random_seed: '0', maximum_mark_age_ns: '60000000000', decision_clock: 'FIXED_INTERVAL_V1' },
    fx_instrument_ids: ['FX.SIM'], funding_source: 'funding-rates.jsonl', bookCores: 1 };
  const researchId = await state.service.research.harness.commit(async tx => {
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
    const study = await tx.doc(Study, conversation.id);
    study.input = { goal: 'Controlled scan binding', model: state.model, data: { catalog: '/shared/controlled-primary', request, nautilusTarget: target,
      sealed: { catalog: '/never-readable-independent', request } }, policy: { strategy: json(strategy) } };
    study.trials.push({ taskId: 71, proposal: { members: [{ model: wat, weight: '0.5' }, { model: wat.replace('f64.const 1', 'f64.const 2'), weight: '0.5' }] }, status: 'COMPUTED', result: { request } });
    if (tick) {
      study.input.data = { catalog: '/shared/controlled-primary', nativeInput: { modality: 'TRADE_TICK', request: tickRequest }, nautilusTarget: target,
        sealed: { catalog: '/never-readable-independent', nativeInput: { modality: 'TRADE_TICK', request: tickRequest } } };
      study.input.policy = { tickEconomic: tickPolicy };
      study.trials[0]!.proposal = { model: wat };
    }
    study.final = { taskId: 99, trialId: 71, status: 'COMPUTED', result: { independent_marker: 999 } };
    return conversation.id;
  }, context);
  const original = await state.service.research.harness.snapshot(Study, researchId, context);
  const plan: ParameterScanPlan = { name: 'Controlled scan transport', source: { kind: 'factor', factorId: `r${researchId}.t71.m1` }, mode: 'FACTOR',
    axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['2', '5'] } }, { key: 'slow_period', domain: { kind: 'values', values: ['3', '4'] } }],
    objective: { metricId: 'factor.pearson_ic', partition: 'VALIDATION', direction: 'maximize' }, execution: { concurrency: 1 } };
  if (tick) plan.source = { kind: 'research-trial', researchId, trialId: 71 };
  return { state, researchId, original, plan, requests, get deletes() { return deletes; }, settle() { settle = true; }, complete() { settle = true; completed = true; },
    changeImplementationAfterFirst() { changeImplementation = true; },
    async close() { await state.cleanup(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } };
}
async function waitUntil(check: () => boolean | Promise<boolean>) {
  const deadline = Date.now() + 10_000;
  while (!await check()) { assert.ok(Date.now() < deadline, 'the original Durable operation must make progress'); await new Promise(resolve => setTimeout(resolve, 15)); }
}

test('scan HTTP creates one deterministic batch, clips validation, binds the selected factor member and retains the completed source', { timeout: 20_000 }, async () => {
  const p = await protocol();
  try {
    const api = p.state.service.app; const headers = p.state.headers;
    const preview = await api.inject({ method: 'POST', url: '/api/v3/parameter-scans/preview', headers, payload: p.plan });
    assert.equal(preview.statusCode, 200, preview.body); assert.equal(preview.json().grid.total, '4'); assert.equal(preview.json().grid.eligible, '2'); assert.equal(preview.json().grid.skipped, '2');
    const empty = { ...p.plan, axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['5'] } }] };
    const emptyPreview = await api.inject({ method: 'POST', url: '/api/v3/parameter-scans/preview', headers, payload: empty });
    assert.equal(emptyPreview.statusCode, 200, emptyPreview.body); assert.equal(emptyPreview.json().grid.eligible, '0');
    const emptyStart = await api.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers, payload: { ...empty, requestId: 'empty-scan' } });
    assert.equal(emptyStart.statusCode, 422); assert.equal(emptyStart.json().error, 'parameter_scan_no_eligible_points');
    const input = { ...p.plan, requestId: 'original-scan' };
    const responses = await Promise.all([0, 1].map(() => api.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers, payload: input })));
    for (const response of responses) assert.equal(response.statusCode, 202, response.body);
    assert.equal(responses[0]!.json().id, responses[1]!.json().id);
    const admitted = responses[0]!.json() as ParameterScanView;
    await p.state.service.research.harness.waitForTask(admitted.taskId as TaskId, context);
    const detail = (await api.inject({ method: 'GET', url: `/api/v3/parameter-scans/${admitted.id}`, headers })).json() as ParameterScanView;
    assert.equal(detail.status, 'COMPLETED'); assert.equal(detail.counts.completed, '2'); assert.equal(detail.counts.skipped, '2'); assert.equal(detail.counts.comparable, '0'); assert.equal(detail.best, null);
    assert.deepEqual(detail.sourceView.memberIndexes, [1]);
    assert.equal(p.requests.length, 2, 'invalid parameter pairs do not submit work');
    for (const request of p.requests) {
      const body = request.input as { forecast: { selection: { event_end_ns: string; decision_cutoff_ns: string } } };
      assert.equal(body.forecast.selection.event_end_ns, '20'); assert.equal(body.forecast.selection.decision_cutoff_ns, '20');
      assert.ok(request.args.includes('/shared/controlled-primary')); assert.ok(!JSON.stringify(request).includes('/never-readable-independent'));
      const file = Object.values(request.files)[0]!;
      const instance = new WebAssembly.Instance(new WebAssembly.Module(Buffer.from(file, 'base64')));
      assert.equal((instance.exports.predict as CallableFunction)(0, 0, 0, 0, 0, 0, 0, 0), 2, 'factor member 1 is preserved exactly');
    }
    const rows = (await api.inject({ method: 'GET', url: `/api/v3/parameter-scans/${admitted.id}/trials?pageSize=1&page=2`, headers })).json();
    assert.equal(rows.total, 2); assert.equal(rows.items.length, 1); assert.equal(rows.items[0].sampleCounts.included, '9007199254740995');
    const skipped = (await api.inject({ method: 'GET', url: `/api/v3/parameter-scans/${admitted.id}/trials/2`, headers })).json();
    assert.equal(skipped.status, 'SKIPPED'); assert.equal(skipped.taskId, null);
    const invalidOrdinal = await api.inject({ method: 'GET', url: `/api/v3/parameter-scans/${admitted.id}/trials/9007199254740995`, headers });
    assert.equal(invalidOrdinal.statusCode, 422); assert.equal(invalidOrdinal.json().error, 'parameter_scan_ordinal_invalid');
    const surface = (await api.inject({ method: 'GET', url: `/api/v3/parameter-scans/${admitted.id}/surface?x=fast_period&y=slow_period`, headers })).json();
    assert.equal(surface.cells.length, 4); assert.equal(surface.cells.filter((row: { status: string }) => row.status === 'SKIPPED').length, 2);
    assert.deepEqual(await p.state.service.research.harness.snapshot(Study, p.researchId, context), p.original, 'scanning does not reopen final research or replace its ledger');
    const replay = await api.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers, payload: input });
    assert.equal(replay.json().id, admitted.id); assert.equal(p.requests.length, 2);
    const conflict = await api.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers, payload: { ...input, name: 'changed' } });
    assert.equal(conflict.statusCode, 409); assert.equal(conflict.json().error, 'request_id_conflict');
    const source = (await api.inject({ method: 'GET', url: `/api/v3/parameter-scans/options?sourceKind=factor&sourceId=${p.plan.source.kind === 'factor' ? p.plan.source.factorId : ''}&mode=FACTOR`, headers })).json();
    assert.equal(source.defaults.fast_period, '2'); assert.ok(source.modes.includes('STRATEGY'));
    const ensembleOptions = await api.inject({ method: 'GET', url: `/api/v3/parameter-scans/options?sourceKind=research-trial&researchId=${p.researchId}&trialId=71`, headers });
    assert.equal(ensembleOptions.statusCode, 200, ensembleOptions.body);
    assert.deepEqual(ensembleOptions.json().modes, ['STRATEGY']); assert.equal(ensembleOptions.json().defaults.investedWeight, '0.250000000000000001');
  } finally { await p.close(); }
});

test('strategy scan detail exposes the original shared-account curve and keeps exact allocation strings', { timeout: 20_000 }, async () => {
  const p = await protocol();
  try {
    const plan: ParameterScanPlan = { ...p.plan, mode: 'STRATEGY', axes: [{ key: 'investedWeight', domain: { kind: 'values', values: ['0.250000000000000001'] } }], objective: { metricId: 'account.sharpe252', partition: 'VALIDATION', direction: 'maximize' } };
    const created = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...plan, requestId: 'strategy-scan' } });
    assert.equal(created.statusCode, 202, created.body); const scan = created.json() as ParameterScanView;
    await p.state.service.research.harness.waitForTask(scan.taskId as TaskId, context);
    const detail = await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}/trials/0`, headers: p.state.headers });
    assert.equal(detail.statusCode, 200, detail.body); assert.equal(detail.json().status, 'COMPLETED', detail.body);
    assert.deepEqual(detail.json().curve.map((row: { value: string }) => row.value), ['3000.000000000000000001', '3001.000000000000000002']);
    assert.equal(detail.json().parameters.investedWeight, '0.250000000000000001'); assert.equal(detail.json().metric.value, null);
    assert.equal(detail.json().metric.reason, 'CONTROLLED_PROTOCOL_ONLY');
    const simulation = p.requests.find(row => row.command === 'simulate')!;
    assert.equal((simulation.input as { target_points: { targets: { weight: string }[] }[] }).target_points[0]!.targets[0]!.weight, '0.250000000000000001');
  } finally { await p.close(); }
});

test('stop retains uncertain original remote receipts, runs one official cleanup and publishes the eventual stop over SSE', { timeout: 25_000 }, async () => {
  const p = await protocol('running'); const abort = new AbortController();
  try {
    const plan: ParameterScanPlan = { ...p.plan, axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['2', '3'] } }] };
    const created = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...plan, requestId: 'stopped-scan' } });
    assert.equal(created.statusCode, 202, created.body); const scan = created.json() as ParameterScanView;
    await waitUntil(() => p.requests.length === 1);
    const address = await p.state.service.app.listen({ host: '127.0.0.1', port: 0 });
    const events = await fetch(`${address}/api/v3/parameter-scans/${scan.id}/events`, { headers: p.state.headers, signal: abort.signal });
    assert.equal(events.status, 200); const reader = events.body!.getReader();
    let seen = '';
    const stoppedEvent = (async () => { for (;;) { const chunk = await reader.read(); if (chunk.done) throw new Error('SSE ended before the remote receipt settled'); seen += new TextDecoder().decode(chunk.value); if (seen.includes('"status":"STOPPED"')) return; } })();
    const stops = await Promise.all([0, 1].map(() => p.state.service.app.inject({ method: 'POST', url: `/api/v3/parameter-scans/${scan.id}/stop`, headers: p.state.headers })));
    for (const response of stops) assert.equal(response.statusCode, 202, response.body);
    await waitUntil(() => p.deletes > 0);
    const unresolved = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
    assert.equal(unresolved.status, 'STOPPING'); assert.equal(p.requests.length, 1, 'stop never creates the next parameter point');
    assert.equal(unresolved.counts.unknown, '1', 'a local abort does not hide the unresolved original remote execution');
    const tasks = await p.state.service.research.harness.commit(tx => tx.scanTasks({ kind: 'qz.parameter-scan.stop' }, 10), context);
    assert.equal(tasks.items.length, 1, 'concurrent stop requests reuse the committed cleanup');
    const record = (await p.state.service.research.harness.snapshot(ParameterScanRecords, p.researchId, scan.id, context))!.value!;
    assert.equal(record.stopTaskId, tasks.items[0]!.id);
    p.settle(); await stoppedEvent;
    assert.ok(seen.includes('event: parameter-scan'));
    const settled = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
    assert.equal(settled.status, 'STOPPED'); assert.equal(p.requests.length, 1);
  } finally { abort.abort(); await p.close(); }
});

test('reopening the service preserves the two original parameter-point invocations and identical receipt replays', { timeout: 20_000 }, async () => {
  const p = await protocol('running');
  try {
    const created = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: { ...p.plan, requestId: 'restart-scan' } });
    assert.equal(created.statusCode, 202, created.body); const scan = created.json() as ParameterScanView;
    await waitUntil(() => p.requests.length === 1); const original = structuredClone(p.requests[0]!); const originalId = original.requestId;
    await p.state.restart();
    assert.equal(new Set(p.requests.map(row => row.requestId)).size, 1, 'closing/reopening retains the first original invocation identity');
    p.complete(); await p.state.service.research.harness.waitForTask(scan.taskId as TaskId, context);
    const result = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
    assert.equal(result.status, 'COMPLETED'); assert.equal(result.counts.completed, '2');
    assert.equal(p.requests[0]!.requestId, originalId);
    assert.equal(new Set(p.requests.map(row => row.requestId)).size, 2, 'two parameter points have two original remote invocations');
    assert.equal(new Set(p.requests.map(row => row.invocationId)).size, 2);
    for (const replay of p.requests.filter(row => row.requestId === originalId)) assert.deepEqual(replay, original, 'a pre-response crash may replay HTTP, never change its original command or bytes');
    assert.deepEqual(await p.state.service.research.harness.snapshot(Study, p.researchId, context), p.original);
  } finally { await p.close(); }
});

test('catalog preparation retains an unresolved original scope, resumes it and only then dispatches parameter points', { timeout: 20_000 }, async () => {
  const p = await protocol('running');
  try {
    await p.state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, p.researchId); const data = study.input!.data;
      const request = data.request as { forecast: { selection: Record<string, unknown>; parameters: Record<string, unknown> }; train_end_ns: string; validation_end_ns: string };
      data.catalog_scan_request = json({ ...request.forecast.selection, parameters: request.forecast.parameters, train_end_ns: request.train_end_ns, validation_end_ns: request.validation_end_ns });
      delete data.request;
    }, context);
    const input = { ...p.plan, axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['2'] } }], requestId: 'catalog-scan' };
    const created = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: input });
    assert.equal(created.statusCode, 202, created.body); const scan = created.json() as ParameterScanView;
    await waitUntil(() => p.requests.length === 1);
    assert.equal(p.requests[0]!.command, 'catalog-factor-scope');
    await waitUntil(async () => {
      const response = await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers });
      return response.json().status === 'RECONCILING';
    });
    const unresolved = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
    assert.equal(unresolved.counts.unknown, '1'); assert.equal(p.requests.length, 1);
    p.complete(); await p.state.service.research.harness.waitForTask(scan.taskId as TaskId, context);
    assert.deepEqual(p.requests.map(row => row.command), ['catalog-factor-scope', 'scan-factor-catalog']);
    const request = p.requests[1]!.input as { event_end_ns: string; decision_cutoff_ns: string };
    assert.equal(request.event_end_ns, '20'); assert.equal(request.decision_cutoff_ns, '20');
    const result = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
    assert.equal(result.status, 'COMPLETED'); assert.equal(result.counts.completed, '1'); assert.equal(result.counts.unknown, '0');
  } finally { await p.close(); }
});

test('a changed official implementation stops subsequent parameter points without submitting mixed-version work', { timeout: 20_000 }, async () => {
  const p = await protocol();
  try {
    p.changeImplementationAfterFirst();
    const input = { ...p.plan, axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['1', '2', '3'] } }], requestId: 'implementation-change' };
    const created = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: input });
    assert.equal(created.statusCode, 202, created.body); const scan = created.json() as ParameterScanView;
    await p.state.service.research.harness.waitForTask(scan.taskId as TaskId, context);
    const detail = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
    assert.equal(detail.status, 'FAILED'); assert.equal(detail.error, 'nautilus_scan_implementation_changed');
    assert.equal(p.requests.length, 1); assert.equal(detail.counts.completed, '1'); assert.equal(detail.counts.failed, '1'); assert.equal(detail.counts.stopped, '1');
  } finally { await p.close(); }
});

test('configured native research tools can be disabled and enabled while scans keep the same scientific Task definitions', { timeout: 20_000 }, async () => {
  const p = await protocol();
  try {
    p.state.options.extensions.push(factorWasm, strategyPortfolio);
    await p.state.restart();
    for (const task of [FactorTask, CatalogScopeTask, PortfolioEvaluationTask]) {
      assert.equal(p.state.service.research.registry.snapshot().task(task.definition.name), task);
      assert.equal(p.state.service.research.registry.snapshot().installed().filter(extension => extension.tasks?.some(candidate => candidate.definition.name === task.definition.name)).length, 1);
    }
    for (const name of [factorWasm.name, strategyPortfolio.name]) {
      const disabled = await p.state.service.app.inject({ method: 'PATCH', url: `/api/v3/extensions/${name}`, headers: p.state.headers, payload: { enabled: false } });
      assert.equal(disabled.statusCode, 200, disabled.body); assert.equal(disabled.json().enabled, false);
      assert.equal(p.state.service.research.registry.snapshot().extension(name), undefined);
    }
    assert.ok(!p.state.service.research.registry.snapshot().tools().some(row => ['factor_evaluate', 'strategy_portfolio_evaluate'].includes(row.tool.name)));
    for (const task of [FactorTask, CatalogScopeTask, PortfolioEvaluationTask]) assert.equal(p.state.service.research.registry.snapshot().task(task.definition.name), task);
    for (const mode of ['FACTOR', 'STRATEGY'] as const) {
      const created = await p.state.service.app.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers,
        payload: { ...p.plan, requestId: `native-tools-disabled-${mode}`, mode, axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['2'] } }],
          objective: { metricId: mode === 'FACTOR' ? 'factor.pearson_ic' : 'account.sharpe252', partition: 'VALIDATION', direction: 'maximize' } } });
      assert.equal(created.statusCode, 202, created.body); const scan = created.json() as ParameterScanView;
      await p.state.service.research.harness.waitForTask(scan.taskId as TaskId, context);
      const detail = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
      assert.equal(detail.status, 'COMPLETED'); assert.equal(detail.counts.completed, '1');
    }
    for (const name of [factorWasm.name, strategyPortfolio.name]) {
      const enabled = await p.state.service.app.inject({ method: 'PATCH', url: `/api/v3/extensions/${name}`, headers: p.state.headers, payload: { enabled: true } });
      assert.equal(enabled.statusCode, 200, enabled.body); assert.equal(enabled.json().enabled, true);
    }
    assert.ok(p.state.service.research.registry.snapshot().tools().some(row => row.tool.name === 'factor_evaluate'));
    assert.ok(p.state.service.research.registry.snapshot().tools().some(row => row.tool.name === 'strategy_portfolio_evaluate'));
  } finally { await p.close(); }
});

test('a persisted scientific Task created under the original research extension resumes under the shared Service registration', { timeout: 20_000 }, async () => {
  const p = await protocol();
  try {
    await p.state.service.close();
    const originalHost = await openResearch({ ...p.state.options.config.research, models: p.state.options.models, extensions: [factorWasm, strategyPortfolio], deferResume: true });
    let taskId: TaskId;
    try {
      assert.equal(originalHost.registry.snapshot().task(FactorTask.definition.name), FactorTask);
      taskId = await originalHost.harness.commit(async tx => {
        const original = (await tx.doc(Study, p.researchId)).input!;
        return tx.createTask(FactorTask, { model: wat, data: json(original.data) }, { conversationId: p.researchId, ownership: { kind: 'conversation' } });
      }, context);
    } finally { await originalHost.close(); }
    p.state.options.extensions.push(factorWasm, strategyPortfolio);
    await p.state.restart();
    const settled = await p.state.service.research.harness.waitForTask(taskId!, context);
    assert.equal(settled.id, taskId!); assert.equal(settled.kind, FactorTask.definition.name); assert.equal(settled.version, FactorTask.definition.version);
    assert.equal(settled.state.outcome.status, 'completed'); assert.equal(p.requests.length, 1);
    assert.deepEqual(p.state.service.research.registry.snapshot().extension(factorWasm.name)?.tasks, []);
    assert.ok(p.state.service.research.registry.snapshot().extension('qz.parameter-scans')?.tasks?.includes(FactorTask));
    assert.deepEqual(await p.state.service.research.harness.snapshot(Study, p.researchId, context), p.original);
  } finally { await p.close(); }
});


test('tick scans route original factor and economic evaluators with cohort periods and fee-net 365 metrics', { timeout: 20_000 }, async () => {
  const p = await protocol('complete', true);
  try {
    const host = p.state.service.research;
    const workbench = parameterScanWorkbench(host, p.state.options.config, createParameterScanExtension(() => host.harness).stopTask);
    const options = await workbench.options(p.plan.source, 'STRATEGY');
    assert.equal(options.source!.modality, 'TRADE_TICK');
    assert.equal(options.source!.currency, 'USDT');
    assert.deepEqual(options.parameters.map(row => row.key), ['fast_period', 'slow_period', 'investedWeight', 'maximum_asset_weight', 'participation_fraction']);
    assert.deepEqual(options.objectives.map(row => row.metricId), ['account.maxDrawdown', 'account.sharpe365', 'account.cagr365']);
    const strategy = { ...p.plan, mode: 'STRATEGY' as const, axes: [{ key: 'fast_period' as const, domain: { kind: 'values' as const, values: ['1', '2'] } },
      { key: 'investedWeight' as const, domain: { kind: 'values' as const, values: ['0.25'] } }], objective: { metricId: 'account.sharpe365' as const, partition: 'VALIDATION' as const, direction: 'maximize' as const } };
    await assert.rejects(workbench.preview({ ...strategy, objective: { ...strategy.objective, metricId: 'account.sharpe252' } }), /parameter_scan_modality_unsupported/);
    await assert.rejects(workbench.preview({ ...strategy, axes: [{ key: 'threshold', domain: { kind: 'values', values: ['0'] } }] }), /parameter_scan_modality_unsupported/);
    await assert.rejects(workbench.preview({ ...strategy, axes: [{ key: 'maximum_asset_weight', domain: { kind: 'range', start: '0.1', end: '0.2', step: '0.0000000000000000001' } }] }), /parameter_scan_value_invalid/);
    const factor = await workbench.start({ ...p.plan, requestId: 'tick-factor-scan' });
    await p.state.service.research.harness.waitForTask(factor.taskId as TaskId, context);
    const scan = await workbench.start({ ...strategy, requestId: 'tick-economic-scan' });
    await p.state.service.research.harness.waitForTask(scan.taskId as TaskId, context);
    const view = await workbench.get(scan.id);
    assert.equal(view.status, 'COMPLETED'); assert.equal(view.counts.completed, '2'); assert.equal(view.best, null);
    const economic = p.requests.filter(row => row.command === 'replay-tick-economic');
    assert.equal(economic.length, 2);
    for (const request of economic) {
      const body = request.input as any;
      assert.equal(body.research.input.event_end_ns, '432000000000000');
      assert.equal(body.research.input.decision_cutoff_ns, '432000000000000');
      assert.equal(body.scenario.gross_fraction, '0.25'); assert.equal(body.scenario.decision_interval_ns, '1000000000');
      assert.equal(body.scenario.perp_taker_fee_bps, '4.5');
      assert.ok(!JSON.stringify(request).includes('/never-readable-independent'));
    }
    assert.deepEqual(p.requests.filter(row => row.command === 'evaluate-tick-factor').map(row => (row.input as any).parameters.fast_period_cohorts), [2, 2]);
    const row = await workbench.trial(scan.id, '0');
    assert.deepEqual(row.sampleCounts, { days: '1', valid_return_days: '0' });
    assert.deepEqual(row.curve, [{ at: '1970-01-04T00:00:00.000000000Z', value: '3000.000000000000000001' }]);
    assert.deepEqual(await p.state.service.research.harness.snapshot(Study, p.researchId, context), p.original);
  } finally { await p.close(); }
});


test('tick economic scan reconciles the original invocation after restart and cancels unresolved work', { timeout: 25_000 }, async () => {
  const p = await protocol('running', true);
  try {
    const api = p.state.service.app;
    const input = { ...p.plan, mode: 'STRATEGY', requestId: 'tick-resume-stop', axes: [{ key: 'investedWeight', domain: { kind: 'values', values: ['0.25', '0.5'] } }],
      objective: { metricId: 'account.sharpe365', partition: 'VALIDATION', direction: 'maximize' } };
    const created = await api.inject({ method: 'POST', url: '/api/v3/parameter-scans', headers: p.state.headers, payload: input });
    assert.equal(created.statusCode, 202, created.body);
    const scan = created.json() as ParameterScanView;
    await waitUntil(() => p.requests.length === 1);
    const original = structuredClone(p.requests[0]);
    await p.state.restart();
    assert.equal(new Set(p.requests.map(row => row.requestId)).size, 1);
    const stopped = await p.state.service.app.inject({ method: 'POST', url: `/api/v3/parameter-scans/${scan.id}/stop`, headers: p.state.headers });
    assert.equal(stopped.statusCode, 202, stopped.body);
    await waitUntil(() => p.deletes > 0);
    const unsettled = (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json();
    assert.equal(unsettled.status, 'STOPPING'); assert.equal(unsettled.counts.unknown, '1');
    p.settle();
    await waitUntil(async () => (await p.state.service.app.inject({ method: 'GET', url: `/api/v3/parameter-scans/${scan.id}`, headers: p.state.headers })).json().status === 'STOPPED');
    assert.equal(p.requests.length, 1);
    assert.deepEqual(p.requests[0], original);
    assert.deepEqual(await p.state.service.research.harness.snapshot(Study, p.researchId, context), p.original);
  } finally { await p.close(); }
});
