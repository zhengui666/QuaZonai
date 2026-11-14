import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { compile } from '@earendil-works/pi-schema';
import { openResearch } from '../src/host.ts';
import { Study } from '../src/documents.ts';
import { ResearchConfiguration } from '../src/research-configuration.ts';
import { nativeStrategyRequestSchema } from '../plugins/native-strategy-backtest.ts';
import type { ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';

const request = { schema_version: 1, instrument_id: 'BTC-USDC.HYPERLIQUID',
  selectors: [{ data_type: 'OrderBookDelta', identifier: 'BTC-USDC.HYPERLIQUID' }],
  event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000101',
  available_start_ns: '1790000000000000002', available_end_ns: '1790000000000000111',
  venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L2_MBP', starting_balances: ['5000 USDC'], base_currency: null,
    leverage: '1', maker_fee: '0.00015', taker_fee: '0.00045' },
  strategy: { kind: 'ORDER_BOOK_IMBALANCE', base: { strategy_id: 'OWNER-001', order_id_tag: 'native' }, max_trade_size: '0.001',
    trigger_min_size: '0.002', trigger_imbalance_ratio: '0.2', min_seconds_between_triggers: 1, time_in_force: 'IOC', dry_run: false } };

test('configuration consumes generated native schema without coercing nanoseconds or balances', () => {
  const validator = compile(nativeStrategyRequestSchema);
  assert.equal(validator.check(request), true);
  assert.equal(validator.check({ ...request, event_start_ns: Number(request.event_start_ns) }), false);
  assert.equal(validator.check({ ...request, catalog: '/different' }), false);
  assert.equal(validator.check({ ...request, venue: { ...request.venue, starting_balances: [5000] } }), false);
  assert.equal(validator.check({ ...request, strategy: { kind: 'CUSTOM_SIGNAL' } }), false);
  assert.equal(validator.check({ ...request, instrument_definition: { mode: 'STATIC_SCENARIO', ts_init_ns: '1795000000000000001', ts_event_ns: '1795000000000000000' } }), true);
  assert.equal(validator.check({ ...request, instrument_definition: { mode: 'STATIC_SCENARIO' } }), false);
  assert.equal(validator.check({ ...request, instrument_definition: { mode: 'STATIC_SCENARIO', ts_init_ns: 1795000000000000001, ts_event_ns: '1' } }), false);
});

// Real official Pi tool loop and real process transport; peer reports are protocol data, not profitability evidence.
const originalRequest = request;
for (const scenario of [false, true]) test(`catalog-only natural research configures and reopens an exact binding (static=${scenario})`, { timeout: 20000 }, async () => {
  const request = scenario ? { ...originalRequest, instrument_definition: { mode: 'STATIC_SCENARIO', ts_init_ns: '1795000000000000001', ts_event_ns: '1795000000000000000' } } : originalRequest;
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-configure-'));

  const cursor = { offset: '2', total_items: '4', instrument_ids: [request.instrument_id], data_types: ['OrderBookDelta'] };
  const inspection = { schema_version: 1, timestamp_basis: 'ts_init', interval_bounds: 'inclusive',
    coverage_basis: 'native_file_intervals_not_event_completeness',
    instruments: [{ instrument: { id: request.instrument_id, quote_currency: 'USDC', price_precision: 2, size_precision: 3 },
      ts_event_ns: scenario ? '1795000000000000000' : '1780000000000000001', ts_init_ns: scenario ? '1795000000000000001' : '1780000000000000002' }],
    selectors: [{ ...request.selectors[0], file_intervals: [{ start_ns: request.available_start_ns, end_ns: request.available_end_ns }] }] };
  const binary=join(directory,'controlled-native.mjs'),marker=join(directory,'calls.jsonl');
  await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';let text='';for await(const part of process.stdin)text+=part;const operation={command:process.argv[2],args:process.argv.slice(3),input:JSON.parse(text)};appendFileSync(${JSON.stringify(marker)},JSON.stringify(operation)+'\\n');const inspection=${JSON.stringify(inspection)},cursor=${JSON.stringify(cursor)},scenario=${JSON.stringify(scenario)};
    const result = operation.command === 'catalog-inspect' ? { ...inspection, next_cursor: operation.input.cursor ? null : cursor } : { schema_version: 1, native_version: '0.69.0', request: operation.input,
      instrument_definition: scenario ? { mode: 'STATIC_SCENARIO', ts_init_ns: '1795000000000000001', ts_event_ns: '1795000000000000000', scope: 'HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED' } : { mode: 'HISTORICAL_AS_OF_START', ts_init_ns: '0', ts_event_ns: '0', scope: 'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START' }, equity_performance: { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: 'UNAVAILABLE', reason: 'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE', currency: null, returns: [], statistics: [252, 365].flatMap(annualization_days => ['CAGR', 'Sharpe Ratio'].map(name => ({ native_key: name+' ('+annualization_days+' days)', annualization_days, value: null }))), first_snapshot_ns: null, last_snapshot_ns: null, snapshot_count: '0', observed_utc_days: '0', unpriced_snapshot_count: '0', return_days: '0', notes: ['Protocol fixture, not measured performance'] }, canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' },
        accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], statistics: {}, components: {} },
      data_rows_by_type: { OrderBookDelta: '2' }, notes: [] };

process.stdout.write(JSON.stringify(result));`,{mode:0o755});
  const target:ProcessNativeTarget={kind:'RUST_PROCESS',id:'registered-backtest',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-only',precision:'high'},research:{singleStrategyRegistration:true,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
  const calls=async()=> (await readFile(marker,'utf8')).trim().split('\n').map(line=>JSON.parse(line));
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  const call = (name: string, args = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
  faux.setResponses([call('native_strategy_brief'), call('native_strategy_catalog', { instrument_ids: [request.instrument_id], data_types: ['OrderBookDelta'], page_size: 2 }), call('native_strategy_catalog', { cursor }), call('native_strategy_configure', { request }),
    call('native_strategy_backtest'), fauxAssistantMessage('Historical computation only; no independent qualification or live order.')]);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  const data = { catalog: directory, nautilusTarget: target };
  const input = { goal: `Research BTC on this catalog using 5000 USDC, 1x leverage, maker .00015 taker .00045. No live trading. ${scenario ? 'Explicitly assume the selected later catalog definition as a static scenario, without claiming historically verified rules.' : ''}`, model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: {} };
  try {
    const admission = await host.start('catalog-natural', input);
    await (await host.harness.submission(admission.submissionId, context))!.wait(context);
    const study = (await host.harness.snapshot(Study, admission.conversationId, context))!;
    assert.equal(study.final?.status, 'COMPUTED');
    assert.equal((study.final!.result as any).instrument_definition.mode, scenario ? 'STATIC_SCENARIO' : 'HISTORICAL_AS_OF_START');
    assert.deepEqual(study.input!.data, { ...data, native_strategy_backtest_request: request });
    const configuration = (await host.harness.snapshot(ResearchConfiguration, admission.conversationId, context))!;
    assert.deepEqual(configuration.request, request); assert.ok(configuration.configuredByTaskId);
    const posts=await calls();
    assert.equal(posts.length, 3);
    assert.equal(posts[0]!.command, 'catalog-inspect'); assert.deepEqual(posts[0]!.input, { instrument_ids: [request.instrument_id], data_types: ['OrderBookDelta'], page_size: 2 });
    assert.equal(posts[1]!.command, 'catalog-inspect'); assert.deepEqual(posts[1]!.input, { cursor });
    assert.equal(posts[2]!.command, 'native-strategy-backtest'); assert.deepEqual(posts[2]!.input, request);
    assert.ok(posts.every(post => JSON.stringify(post.args) === JSON.stringify(['--catalog', directory])));
    assert.equal(faux.state.callCount, 6);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('catalog-natural', input), admission, 'original admission identity remains replayable after configuration');
    assert.deepEqual((await host.harness.snapshot(Study, admission.conversationId, context))!.input!.data, study.input!.data);
    assert.deepEqual(await calls(),posts,'reopen does not rerun original processes');
    const nextRequest = { ...request, strategy: { ...request.strategy, trigger_imbalance_ratio: '0.3' } };
    faux.setResponses([call('native_strategy_configure', { request: nextRequest }), fauxAssistantMessage('New configuration only; prior results are historical.')]);
    const next = await host.resume(admission.conversationId, admission.submissionId, 'Change ratio to .3 without running it');
    await (await host.harness.submission(next.submissionId, context))!.wait(context);
    const changed = (await host.harness.snapshot(Study, admission.conversationId, context))!;
    assert.equal(changed.final, null);
    assert.deepEqual(changed.input!.data.native_strategy_backtest_request, nextRequest);
    const nextConfig = (await host.harness.snapshot(ResearchConfiguration, admission.conversationId, context))!;
    assert.notEqual(nextConfig.configuredByTaskId, configuration.configuredByTaskId);
    const originalRun = await host.harness.getTask(configuration.evaluationTaskId!, context);
    assert.equal(originalRun!.state.status, 'terminal');
    assert.deepEqual((originalRun!.input as { request: unknown }).request, request);
    assert.deepEqual(await calls(),posts,'changing parameters does not rerun or overwrite historical execution');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

for (const boundary of ['sealed', 'outside-catalog', 'numeric-time'] as const) test(`configure rejects ${boundary} without altering registered data`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-configure-reject-'));
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  const original = { catalog: directory, ...(boundary === 'sealed' ? { sealed: {} } : {}) };
  const proposed = boundary === 'outside-catalog' ? { ...request, catalog: '/not-registered' }
    : boundary === 'numeric-time' ? { ...request, event_start_ns: Number(request.event_start_ns) } : request;
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_configure', { request: proposed }), { stopReason: 'toolUse' }), fauxAssistantMessage('Configuration was rejected, no execution.')]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    const admission = await host.start(boundary, { goal: 'Configure only', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: original, policy: {} });
    await (await host.harness.submission(admission.submissionId, context))!.wait(context);
    const study = (await host.harness.snapshot(Study, admission.conversationId, context))!;
    assert.deepEqual(study.input!.data, original);
    assert.equal((await host.harness.snapshot(ResearchConfiguration, admission.conversationId, context))?.configuredByTaskId, undefined);
    assert.equal(study.final, null);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
