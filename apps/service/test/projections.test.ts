import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, TaskQuery } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { awaitWithContext } from '@earendil-works/chord/context';
import { Study } from '../../research/src/documents.ts';
import { built, context, setup, studied } from './support.ts';
import { Application } from '../src/facts.ts';
import { metrics, researchTaskViews, tickEconomicMetrics } from '../src/projections.ts';
import { researchCriterion, researchSpecification } from '../../../packages/contracts/src/research.ts';

test('dataset projection retains compact catalog horizons and partitions without advertising account strategy', async () => {
  const state = await setup();
  try {
    const dataset = state.options.config.datasets[0]!;
    const parameters = { schema_version: 1, fast_period: 2, slow_period: 3, label_horizon_observations: 7 };
    const selection = { schema_version: 1, event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30', maximum_rows: 100, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] };
    dataset.data.request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1, selection, parameters } };
    state.options.config.datasets.push({ ...dataset, id: 'catalog-scan', data: { catalog: dataset.data.catalog!, catalog_scan_request: {
      schema_version: 1, event_start_ns: '0', train_end_ns: '10', validation_end_ns: '20', event_end_ns: '30', decision_cutoff_ns: '30', maximum_rows: 100, parameters } } });
    const original = structuredClone(state.options.config.datasets);
    await state.restart();
    const response = await state.service.app.inject({ url: '/api/v3/datasets', headers: state.headers });
    assert.equal(response.statusCode, 200, response.body);
    const [legacy, compact] = response.json();
    assert.equal(compact.researchSettings.horizon, 7);
    assert.deepEqual(compact.researchSettings.partitions, { trainEndNs: '10', validationEndNs: '20', endNs: '30', historical: false });
    assert.deepEqual(legacy.researchSettings.partitions, compact.researchSettings.partitions);
    assert.equal(legacy.researchSettings.horizon, compact.researchSettings.horizon);
    assert.equal(legacy.researchSettings.strategy.kind, 'factor-threshold');
    assert.deepEqual(legacy.researchSettings.instruments, ['TEST.SIM']);
    assert.equal(compact.researchSettings.strategy, null);
    assert.deepEqual(compact.researchSettings.instruments, []);
    assert.equal('data' in compact, false);
    assert.deepEqual(state.options.config.datasets, original);
    assert.equal(state.faux.state.callCount, 0);
  } finally { await state.cleanup(); }
});

test('list and usage do not load historical task results; list retains its existing summary fields', { timeout: 20_000 }, async t => {
  const storage = await openNodeSqliteStorage(':memory:'); const state = await setup(true, undefined, storage);
  try {
    const { receipt } = await studied(state);
    const get = (path: string) => state.service.app.inject({ url: `/api/v3/${path}`, headers: state.headers });
    const detail = (await get(`research/${receipt.conversationId}`)).json();
    const { input: _input, trialRecords: _trials, final: _final, tasks: _tasks, submission: _submission, previousStops: _stops, reviews: _reviews, messages: _messages, messagesNextCursor: _messagesNextCursor, executionUncertain: _uncertain, executionUncertainty: _uncertainty, toolPolicy: _toolPolicy, assessments: _assessments, ...expected } = detail;
    const scan = storage.scanTasks.bind(storage); const scans: TaskQuery[] = [];
    t.mock.method(storage, 'scanTasks', (...args: Parameters<typeof scan>) => { scans.push(args[0]); return scan(...args); });
    const list = await get('research'); const usage = await get('usage');
    assert.equal(list.statusCode, 200); assert.deepEqual(list.json().items, [expected]);
    assert.equal(usage.statusCode, 200); assert.equal(usage.json().activeTasks, 0);
    assert.ok(scans.every(query => query.status && query.status !== 'terminal'), 'summary reads must not scan full historical task receipts');
  } finally { await state.cleanup(); }
});

test('detail reads live task state from the official graph and caches immutable terminal metadata', { timeout: 20_000 }, async t => {
  const storage = await openNodeSqliteStorage(':memory:'); const state = await setup(true, undefined, storage);
  try {
    const { receipt } = await studied(state); const host = state.service.research;
    const scan = storage.scanTasks.bind(storage); let scans = 0;
    t.mock.method(storage, 'scanTasks', (...args: Parameters<typeof scan>) => { if (args[0].conversationId === receipt.conversationId && args[0].status === 'terminal') scans++; return scan(...args); });
    const get = () => state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    const [first, concurrent] = await Promise.all([get(), get()]);
    assert.equal(first.statusCode, 200); assert.deepEqual(first.json(), concurrent.json()); assert.equal(scans, 0, 'studied already read detail and populated terminal metadata');
    let finishFailure!: () => void; const failure = new Promise<void>(resolve => { finishFailure = resolve; });
    const task = defineTask<{ fail: boolean; result: string | null }, { phase: 'compute' }, string | null>({ name: 'test.metadata-only', version: 1, initial: () => ({ phase: 'compute' }),
      phases: { async compute(task, runtime, ctx) { if (task.input.fail) await awaitWithContext(failure, ctx); await runtime.commit(() => ({ status: 'terminal', outcome: task.input.fail ? { status: 'failed', error: { message: 'test_failure' } } : { status: 'completed', result: task.input.result } }), ctx); } },
      async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); } });
    host.registry.install(defineExtension({ name: 'test.task-metadata', tasks: [task] }));
    const ids = await host.harness.commit(async tx => {
      const ids = [];
      for (let i = 0; i < 101; i++) ids.push(await tx.createTask(task, { fail: false, result: i === 0 ? 'large protocol result '.repeat(100_000) : null }, { conversationId: receipt.conversationId as ConversationId, ownership: { kind: 'conversation' } }));
      return ids;
    }, context);
    await Promise.all(ids.map(id => host.harness.waitForTask(id, context)));
    const updated = (await get()).json(); assert.equal(updated.tasks.length, 100);
    assert.deepEqual(updated.tasks.map((task: { id: number }) => task.id), ids.slice(-100).reverse());
    assert.ok(updated.tasks.every((task: { outcome: string }) => task.outcome === 'completed'));
    const pending = await host.harness.commit(tx => tx.createTask(task, { fail: true, result: null }, { conversationId: receipt.conversationId as ConversationId, ownership: { kind: 'conversation' } }), context);
    const running = (await get()).json().tasks[0]; assert.equal(running.id, pending); assert.notEqual(running.status, 'terminal');
    const usage = await state.service.app.inject({ url: '/api/v3/usage', headers: state.headers }); assert.ok(usage.json().activeTasks > 0);
    finishFailure(); await host.harness.waitForTask(pending, context);
    const final = (await get()).json(); const failed = final.tasks[0]; assert.equal(failed.id, pending); assert.equal(failed.status, 'terminal'); assert.equal(failed.outcome, 'failed'); assert.equal(failed.error, 'task_failed');
    assert.equal(scans, 0, 'committed changes must update metadata without reloading evidence');
    const cold = await researchTaskViews(host)(receipt.conversationId); assert.deepEqual(cold, final.tasks); assert.equal(scans, 10, 'cold history is read in ten small pages, not one huge allocation');
    assert.equal((await storage.task(ids[0]!, context))!.state.outcome!.status, 'completed');
  } finally { await state.cleanup(); }
});

test('read projections retain partition/component availability and native scalar precision', () => {
  const result = metrics({ partitions_component: { index: 2 }, partitions: [{ partition: 'HELDOUT', metric: {
    pearson_ic: { value: null, status: 'CONSTANT_SCORE' }, counts: { included: '9007199254740995' },
  } }] });
  assert.deepEqual(result.find(item => item.key === 'HELDOUT.pearson_ic'), {
    key: 'HELDOUT.pearson_ic', label: 'component 2: HELDOUT.pearson_ic', value: null,
    status: 'CONSTANT_SCORE', partition: 'HELDOUT', component: 2,
  });
  assert.equal(result.find(item => item.key === 'HELDOUT.counts.included')!.value, '9007199254740995');
});

test('tick financial criterion reads overall partition statistics from an economic-only final', () => {
  const spec = researchSpecification({ version: 1, name: 'Net returns', question: 'Evaluate net returns', rationale: 'Use the original account returns',
    evaluation: { metric: 'Sharpe Ratio (365 days)', direction: 'maximize', target: 5, scheme: 'registered-partitions' },
    search: { scope: 'single-factor', maxMembers: 1, stop: 'selection' } });
  const economic = { status: 'COMPUTED', taskId: 7, summary: {
    partition_metrics: [{ partition: 'HELDOUT', days: '30', valid_return_days: '29', sharpe_365: 6, annualized_return_365: 3.1 }],
    metrics: [{ partition: 'HELDOUT', regime: 'BEAR', sharpe_365: 100 }],
  } };
  const actual = metrics({ economic });
  assert.equal(researchCriterion(spec, 'COMPUTED', actual)!.value, 6);
  assert.equal(researchCriterion(spec, 'COMPUTED', actual)!.status, 'met');
  assert.equal(actual.find(row => row.key === 'HELDOUT.portfolio.annualized_return_365')!.value, 3.1);
  assert.deepEqual(tickEconomicMetrics({ ...economic, status: 'FAILED' }), []);
  assert.deepEqual(tickEconomicMetrics({ ...economic, summary: { metrics: economic.summary.metrics } }), []);
  const missing = metrics({ economic: { ...economic, summary: { partition_metrics: [{ ...economic.summary.partition_metrics[0]!, sharpe_365: null }] } } });
  assert.equal(researchCriterion(spec, 'COMPUTED', missing)!.status, 'insufficient');
  assert.equal(researchCriterion(spec, 'COMPUTED', missing)!.value, null);
});

test('owner research detail exposes the recorded economic outcome independently of predictive feedback', async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state, 'economic-projection');
    const get = () => state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    const original = (await get()).json().trialRecords[0];
    assert.equal('economic' in original, false);
    const calls = state.faux.state.callCount;
    const stored = (await state.service.research.harness.snapshot(Study, receipt.conversationId as ConversationId, context))!.trials[0]!.result;
    for (const economic of [
      { taskId: 700, status: 'COMPUTED', summary: { fees_usdc_equivalent: '0.123456789012345678', metrics: [{ partition: 'VALIDATION', sharpe_365: null }], days: [{ net_return: null, unknown_reason: 'MISSING_MARK' }] }, error: null },
      { taskId: 701, status: 'STARTED', summary: null, error: null },
      { taskId: 702, status: 'FAILED', summary: null, error: 'tick_economic_replay_failed' },
    ]) {
      await state.service.research.harness.commit(async tx => {
        (await tx.doc(Study, receipt.conversationId as ConversationId)).trials[0]!.economic = economic;
      }, context);
      const response = await get(); assert.equal(response.statusCode, 200, response.body);
      assert.deepEqual(response.json().trialRecords[0], { ...original, economic });
      assert.deepEqual((await get()).json().trialRecords[0].economic, economic);
    }
    assert.deepEqual((await state.service.research.harness.snapshot(Study, receipt.conversationId as ConversationId, context))!.trials[0]!.result, stored);
    assert.equal(state.faux.state.callCount, calls);
  } finally { await state.cleanup(); }
});

test('public research failures retain codes but do not expose native stderr', { timeout: 20_000 }, async () => {
  const state = await setup(); const diagnostic = 'Native failure at /private/test-only/catalog\n' + 'internal diagnostic '.repeat(30);
  try {
    const { receipt } = await studied(state, 'private-errors');
    await state.service.research.harness.commit(async tx => { const study = await tx.doc(Study, receipt.conversationId as ConversationId); study.final!.status = 'FAILED'; study.final!.error = diagnostic; study.trials[0]!.error = diagnostic; }, context);
    const response = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers }); const view = response.json();
    assert.equal(view.final.error, 'scientific_execution_failed'); assert.equal(view.trialRecords[0].error, 'scientific_execution_failed'); assert.ok(!response.body.includes('/private/test-only'));
    const stored = await state.service.research.harness.snapshot(Study, receipt.conversationId as ConversationId, context); assert.equal(stored!.final!.error, diagnostic); assert.equal(stored!.trials[0]!.error, diagnostic);
  } finally { await state.cleanup(); }
});

test('historical native codes and WAT failures keep specific safe public causes', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state, 'historic-native-causes');
    for (const [diagnostic, code, category] of [
      ['QZ_NATIVE_JOB_MEMORY_LIMIT\nprivate /private/test-only/model.wasm token=synthetic-secret', 'QZ_NATIVE_JOB_MEMORY_LIMIT', 'resource'],
      ['QZ_NATIVE_JOB_FAILED\nFORECAST_INSUFFICIENT_WARMUP: /private/test-only/catalog', 'FORECAST_INSUFFICIENT_WARMUP', 'data'],
      ['parseWat failed:\nfactor.wat:1:2: error: unexpected token\n(module private-source-marker)', 'wasm_compile_failed', 'model'],
      ['custom_evaluator_failed', 'custom_evaluator_failed', 'unknown'],
      ['strategy_market_not_allowed', 'strategy_market_not_allowed', 'request'],
      ['strategy_instrument_not_allowed', 'strategy_instrument_not_allowed', 'request'],
    ]) {
      await state.service.research.harness.commit(async tx => {
        const study = await tx.doc(Study, receipt.conversationId as ConversationId);
        study.final!.error = diagnostic!; study.final!.status = 'FAILED'; study.trials[0]!.error = diagnostic!;
      }, context);
      const response = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
      const view = response.json();
      assert.equal(view.trialRecords[0].error, code); assert.equal(view.final.error, code);
      assert.equal(view.trialRecords[0].errorDetail.category, category);
      assert.ok(view.final.errorDetail.message); assert.ok(view.final.errorDetail.action);
      if (category === 'unknown') assert.match(view.final.errorDetail.message, /无法确定具体原因/);
      if (code?.endsWith('_not_allowed')) assert.match(view.final.errorDetail.message, /原研究.*范围/);
      assert.doesNotMatch(response.body, /private-source-marker|\/private\/test-only|synthetic-secret/);
      assert.equal((await state.service.research.harness.snapshot(Study, receipt.conversationId as ConversationId, context))!.final!.error, diagnostic);
    }
  } finally { await state.cleanup(); }
});



test('captured dataset identity survives registry changes and identical inputs in different studies', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const first = await studied(state, 'original-display-facts');
    state.options.config.datasets[0]!.name = 'Changed current registry';
    state.options.config.datasets[0]!.source = 'Changed current source';
    await state.restart();
    const second = await studied(state, 'second-display-facts');
    const get = async (id: number) => (await state.service.app.inject({ url: `/api/v3/research/${id}`, headers: state.headers })).json();
    assert.equal((await get(first.receipt.conversationId)).dataset.name, 'Protocol dataset');
    assert.equal((await get(second.receipt.conversationId)).dataset.name, 'Changed current registry');
    await state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, first.receipt.conversationId as ConversationId);
      study.final!.reason = 'Original selection rationale'; study.final!.error = 'native_result_request_mismatch'; study.final!.status = 'FAILED';
    }, context);
    const detail = await get(first.receipt.conversationId);
    assert.equal(detail.final.reason, 'Original selection rationale');
    assert.equal(detail.final.error, 'native_result_request_mismatch');
    assert.equal(detail.final.errorDetail.category, 'runtime');
    assert.equal(detail.createdAt, null);
    assert.equal(detail.datasetId, 'controlled');
    assert.ok(detail.tasks.some((task: { status: string; outcome?: string }) => task.status === 'terminal' && task.outcome === 'completed'));
    const extensions = (await state.service.app.inject({ url: '/api/v3/extensions', headers: state.headers })).json();
    assert.equal(extensions.find((item: { name: string }) => item.name === 'qz.science').required, true);
  } finally { await state.cleanup(); }
});

for (const endpoint of ['usage', 'research']) test(`${endpoint} summary does not decode historical native task results`, { timeout: 20_000 }, async () => {
  const state = await setup(); const harness = state.service.research.harness;
  const commit = harness.commit.bind(harness); let historicalReads = 0;
  try {
    await studied(state, `summary-without-history-${endpoint}`);
    const expected = await state.service.app.inject({ url: `/api/v3/${endpoint}`, headers: state.headers });
    assert.equal(expected.statusCode, 200, expected.body);
    harness.commit = (callback, ctx) => commit((tx) => callback(new Proxy(tx, {
      get(target, property) {
        if (property === 'scanTasks') return () => { historicalReads++; throw new Error('unexpected_historical_task_read'); };
        const value = Reflect.get(target, property, target);
        return typeof value === 'function' ? value.bind(target) : value;
      },
    })), ctx);
    const response = await state.service.app.inject({ url: `/api/v3/${endpoint}`, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body);
    assert.deepEqual(response.json(), expected.json(), 'original summary fields and nullable values remain unchanged');
    assert.equal(historicalReads, 0);
  } finally { harness.commit = commit; await state.cleanup(); }
});

test('canonical upstream reports do not flatten executions or hexadecimal statistics into decimal metrics', () => {
  assert.deepEqual(metrics({ request: {}, canonical_result: { schema: 'nautilus-backtest-result/v1',
    statistics: { returns: { 'Sharpe Ratio': '0000000000000000', unavailable: null } },
    fills: [{ price: '123.456789012345678901', quantity: '0.000000000000000001' }], orders: [{ count: 1 }] }, data_rows_by_type: { QuoteTick: '9007199254740995' } }), []);
});

test('native result and prior submissions retain separate original reports and unavailable values', async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state);
    const original = { schema_version: 1, native_version: '0.69.0', request: { instrument_id: 'TEST.SIM', event_start_ns: '1790000000000000001' },
      canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' }, statistics: { returns: { 'Sharpe Ratio': '3ff0000000000000', missing: null } }, fills: [{ price: '123.456789012345678901' }] },
      data_rows_by_type: { QuoteTick: '9007199254740995' }, notes: ['Protocol projection only, not measured performance.'] };
    const previous = { ...original, canonical_result: { ...original.canonical_result, statistics: { returns: { 'Sharpe Ratio': '0000000000000000', missing: null } } } };
    const history = [{ previousSubmissionId: receipt.submissionId, final: { status: 'COMPUTED', result: previous } }];
    await state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, receipt.conversationId); study.previousNativeResults = history;
      study.final = { status: 'COMPUTED', taskId: 123, trialId: null, comparedTrials: [], scientificClaim: 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION', result: original };
    }, context);
    const response = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body); const detail = response.json();
    assert.deepEqual(detail.final.metrics, []);
    assert.deepEqual(detail.final.nativeStrategyResult.canonicalResult, original.canonical_result);
    assert.deepEqual(detail.final.nativeStrategyResult.dataRowsByType, original.data_rows_by_type);
    assert.deepEqual(detail.previousNativeResults, history);
    assert.equal(detail.final.nativeStrategyResult.canonicalResult.statistics.returns.missing, null);
  } finally { await state.cleanup(); }
});

for (const retained of [false, true]) test(`typed official results remain visible without fabricated metrics (canonical=${retained})`, async () => {
 const state = await setup();
 try {
  const {receipt}=await studied(state);
  const original={schema_version:1,native_version:'0.69.0',request:{schema_version:1,name:'owner::factory',config:{}},
   native_results:[{run_id:'original-run',total_orders:7,elapsed_time:3.5,stats_pnls:{USDC:{PnL:'0.000000000000000019'}}}],
   canonical_result:retained?{schema:'nautilus-backtest-result/v1',orders:[],statistics:{}}:null,
   canonical_unavailable_reason:retained?null:'OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG',
   equity_performance:null,equity_performance_unavailable_reason:'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE'};
  await state.service.research.harness.commit(async tx=>{(await tx.doc(Study,receipt.conversationId)).final={status:'COMPUTED',taskId:123,trialId:null,comparedTrials:[],scientificClaim:'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION',result:original};},context);
  const response=await state.service.app.inject({url:`/api/v3/research/${receipt.conversationId}`,headers:state.headers});
  assert.equal(response.statusCode,200,response.body);
  const detail=response.json();assert.deepEqual(detail.final.metrics,[]);
  assert.deepEqual(detail.final.nativeStrategyResult.nativeResults,original.native_results);
  assert.deepEqual(detail.final.nativeStrategyResult.canonicalResult,original.canonical_result);
  assert.equal(detail.final.nativeStrategyResult.canonicalUnavailableReason,original.canonical_unavailable_reason);
  assert.equal(detail.final.nativeStrategyResult.equityPerformance,null);
  assert.equal(detail.final.nativeStrategyResult.equityUnavailableReason,original.equity_performance_unavailable_reason);
 }finally{await state.cleanup();}
});

test('typed run metadata has bounded pages and cannot mix a replaced result',async()=>{
 const state=await setup();
 try{
  const {receipt}=await studied(state);const rows=Array.from({length:45},(_,i)=>({run_id:`original-${i}`,value:'0.000000000000000019'}));
  await state.service.research.harness.commit(async tx=>{(await tx.doc(Study,receipt.conversationId)).final={status:'COMPUTED',taskId:123,trialId:null,comparedTrials:[],scientificClaim:'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION',result:{schema_version:1,native_results:rows,canonical_result:null,canonical_unavailable_reason:'DISPOSED'}};},context);
  const base=`/api/v3/research/${receipt.conversationId}`;
  const detail=(await state.service.app.inject({url:base,headers:state.headers})).json();
  assert.deepEqual(detail.final.nativeStrategyResult.nativeResults,rows.slice(0,20));assert.equal(detail.final.nativeStrategyResult.nativeResultsNextOffset,20);
  const loaded=[...detail.final.nativeStrategyResult.nativeResults];let offset=20;
  for(;;){const response=await state.service.app.inject({url:`${base}/native-runs?resultTaskId=123&offset=${offset}`,headers:state.headers});assert.equal(response.statusCode,200,response.body);const page=response.json();assert.ok(page.items.length<=20);assert.equal(page.total,45);loaded.push(...page.items);if(page.nextOffset===null)break;offset=page.nextOffset;}
  assert.deepEqual(loaded,rows);await state.restart();
  const replay=await state.service.app.inject({url:`${base}/native-runs?resultTaskId=123&offset=20`,headers:state.headers});assert.deepEqual(replay.json().items,rows.slice(20,40));
  const wrong=await state.service.app.inject({url:`${base}/native-runs?resultTaskId=124&offset=20`,headers:state.headers});assert.equal(wrong.statusCode,409);
 }finally{await state.cleanup();}
});
