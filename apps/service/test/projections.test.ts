import assert from 'node:assert/strict';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { test } from 'node:test';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, TaskQuery } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { awaitWithContext } from '@earendil-works/chord/context';
import { Study } from '../../research/src/documents.ts';
import { built, context, fauxAssistantMessage, fauxToolCall, setup, studied } from './support.ts';
import { Application, object } from '../src/facts.ts';
import { BuildPortfolio } from '../src/science.ts';
import { metrics, researchTaskViews } from '../src/projections.ts';
import { evaluatePortfolio } from '../../research/plugins/native-portfolio.ts';
import { modelFeedback } from '../../research/src/feedback.ts';
import research from '../../research/plugins/research.ts';

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

test('research detail exposes original catalog final daily returns and assumptions without private account data', async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state);
    const get = () => state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    const legacy = (await get()).json();
    assert.equal('assumptions' in legacy.final, false);
    assert.equal('assumptions' in legacy.trialRecords[0], false);
    assert.equal('dailyReturns' in legacy.final, false);
    assert.equal('account' in legacy.final, false); assert.equal('account' in legacy.trialRecords[0], false);
    const assumptions = ['CONDITIONAL_RESEARCH_NOT_VERIFIED_HISTORICAL_EXECUTION', 'Original receive clock is modeled; pUSD uses a margin proxy.'];
    const dailyReturns = [{ timestamp_ns: '86400000000000000', value: 0.021, reason_code: null },
      { timestamp_ns: '86486400000000000', value: null, reason_code: 'ORIGINAL_NATIVE_RETURN_UNAVAILABLE' },
      { timestamp_ns: '86572800000000000', value: -0.01, reason_code: null }];
    const coverage = { catalog_series: '3000000', observed_markets: '2999999', bars: '9007199254740994', source_records: '9007199254740993', consumed_fuel: [null, '9223372036854775807'] };
    const execution = { native_version: 'TEST_ONLY_NATIVE', summary: { commissions: '0.000000000000000001 pUSD', final_balance: '3021.000000000000000001 pUSD' },
      iterations: '1', events: '9007199254740994', positions: '7', consumed_target_points: '19', returns_kind: 'PORTFOLIO_DAILY', returns_status: 'INCOMPLETE', returns_reason: 'TEST_ONLY_ORIGINAL_REASON' };
    const recordedFees = { currency: 'pUSD', fills: [
      { commission: '9007199254741000.000000000000000001', fill_quantity: '9007199254740993.000000000000000001', fill_price: '0.000000000000000001', allocations: [{ private: 'OWNER_ONLY_FEE_ALLOCATIONS' }] },
      { commission: '-0.000000000000000001', fill_quantity: '0.000000000000000001', fill_price: '0.000000000000000001', allocations: [] },
    ] };
    const feeTotals = { currency: 'pUSD', fills: '2', commission_total: '9007199254741000', gross_fill_notional: '0.009007199254740993000000000000000002' };
    const finalFeeTotals = { ...feeTotals, commission_total: '123456789' };
    const binary = join(state.directory, 'catalog-daily-protocol.mjs');
    await writeFile(binary, `#!${process.execPath}\n// TEST ONLY: retain original wire values; no scientific calculation.\nlet raw = ''; for await (const chunk of process.stdin) raw += chunk; const request = JSON.parse(raw); const recorded = ${JSON.stringify(recordedFees)}; if (request.evaluation_start_ns === '20') recorded.fills[0].commission = '123456789.000000000000000001';\nprocess.stdout.write(JSON.stringify({ source: request.source, revision: request.revision, assumptions: ${JSON.stringify(assumptions)}, ...${JSON.stringify(coverage)}, simulation: { ...${JSON.stringify(execution)}, recorded_fee_replay: request.score_threshold === 2 ? null : request.score_threshold === 1 ? { ...recorded, fills: [] } : request.score_threshold === 3 ? { ...recorded, fills: recorded.fills.slice(1) } : recorded, statistics: [], returns: ${JSON.stringify(dailyReturns)}, canonical_result: { private: 'OWNER_ONLY_ACCOUNT' }, fills: [987654.321] } }));\n`, { mode: 0o700 });
    const model = '(module (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)) (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) f64.const 1))';
    const members = [{ model, weight: '0.5' }, { model, weight: '0.5' }];
    const data = { catalog: state.options.config.datasets[0]!.data.catalog!, catalog_partitions: '/private/test-only-manifest', train_end_ns: '10', validation_end_ns: '20',
      catalog_ensemble_request: { schema_version: 1, event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30', evaluation_start_ns: '10', source: 'TEST_ONLY', revision: 'protocol',
        modeled_lag_ns: '0', score_threshold: 0, members: members.map(member => ({ weight: member.weight })), gross_exposure: '1', target_ttl_ns: '5', settings: { base_currency: 'pUSD', starting_capital: '3000' } } };
    const input = { data, members, policy: { name: 'TEST_ONLY', kind: 'catalog-ensemble' as const }, binary };
    const scope = { barTypes: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] };
    const validation = await evaluatePortfolio({ ...input, partition: 'VALIDATION' }, context, scope);
    const final = await evaluatePortfolio({ ...input, partition: 'HELDOUT' }, context, scope);
    const validationPortfolio = object(validation.summary.portfolio);
    assert.deepEqual(validationPortfolio.recorded_fee_totals, feeTotals);
    assert.deepEqual(object(final.summary.portfolio).recorded_fee_totals, finalFeeTotals);
    assert.deepEqual(object(modelFeedback(validation.summary).portfolio).recorded_fee_totals, feeTotals);
    for (const [threshold, expected] of [[1, { currency: 'pUSD', fills: '0', commission_total: '0', gross_fill_notional: '0' }], [2, null],
      [3, { currency: 'pUSD', fills: '1', commission_total: '-0.000000000000000001', gross_fill_notional: '0.000000000000000000000000000000000001' }]] as const) {
      const selected = { ...input, data: { ...data, catalog_ensemble_request: { ...data.catalog_ensemble_request, score_threshold: threshold } } };
      assert.deepEqual(object((await evaluatePortfolio({ ...selected, partition: 'VALIDATION' }, context, scope)).summary.portfolio).recorded_fee_totals, expected);
    }
    assert.equal('daily_returns' in validationPortfolio, false);
    assert.deepEqual(object(final.summary.portfolio).daily_returns, dailyReturns);
    assert.equal(JSON.stringify(modelFeedback({ ...validation.summary, portfolio: { ...validationPortfolio, daily_returns: dailyReturns } })).includes('86400000000000000'), false);
    assert.throws(() => modelFeedback(final.summary), /validation_feedback_required/);
    await state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, receipt.conversationId);
      const privateFields = { canonical_result: { private: 'OWNER_ONLY_ACCOUNT' }, fills: [987654.321], catalog_scope: { taskId: 42 },
        summary: { ...execution.summary, canonical_result: { private: 'OWNER_ONLY_ACCOUNT' }, fills: [987654.321], daily_returns: dailyReturns, owned_ref: { taskId: 42 } } };
      study.trials[0]!.result = { ...validation.summary, portfolio: { ...validationPortfolio, ...privateFields, daily_returns: dailyReturns } };
      study.final!.result = { ...final.summary, portfolio: { ...object(final.summary.portfolio), ...privateFields } };
    }, context);
    const response = await get(); assert.equal(response.statusCode, 200, response.body);
    for (const [partition, view] of [['VALIDATION', response.json().trialRecords[0]], ['HELDOUT', response.json().final]] as const) {
      assert.deepEqual(view.account, { source: 'TEST_ONLY', revision: 'protocol', partition, ...coverage, ...execution, orders: null, recorded_fee_totals: partition === 'HELDOUT' ? finalFeeTotals : feeTotals });
      for (const key of ['canonical_result', 'fills', 'allocations', 'provenance', 'catalog_scope', 'daily_returns']) assert.equal(key in view.account, false);
    }
    assert.deepEqual(response.json().trialRecords[0].assumptions, assumptions);
    assert.deepEqual(response.json().final.assumptions, assumptions);
    assert.deepEqual(response.json().final.dailyReturns, dailyReturns);
    assert.equal('dailyReturns' in response.json().trialRecords[0], false);
    assert.equal(response.body.includes('OWNER_ONLY_ACCOUNT'), false);
    assert.equal(response.body.includes('987654.321'), false);
    assert.equal(response.body.includes('OWNER_ONLY_FEE_ALLOCATIONS'), false);
    await state.service.research.harness.commit(async tx => {
      delete object((await tx.doc(Study, receipt.conversationId)).final!.result).registered_request;
    }, context);
    const historical = (await get()).json();
    assert.equal('executionRequest' in historical.final, false);
    assert.deepEqual(historical.final.registeredRequest, final.summary.request);
    state.service.research.registry.install(research);
    const trialId = response.json().trialRecords[0].id;
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('research_trial', { trialId }), { stopReason: 'toolUse' })]);
    const conversation = await state.service.research.harness.conversation(receipt.conversationId, context);
    const read = await conversation!.submit({ type: 'input', content: 'Read the original validation feedback only.', requestId: 'catalog-daily-read' }, context);
    await read.wait(context);
    const entries = JSON.stringify((await state.service.research.view(receipt.conversationId)).entries);
    assert.ok(entries.includes('research_trial'));
    assert.equal(entries.includes('86400000000000000'), false);
    assert.equal(entries.includes('ORIGINAL_NATIVE_RETURN_UNAVAILABLE'), false);
    assert.equal(entries.includes('OWNER_ONLY_ACCOUNT'), false);
    assert.equal(entries.includes('OWNER_ONLY_FEE_ALLOCATIONS'), false);
    assert.ok(entries.includes('recorded_fee_totals'));
    assert.ok(entries.includes('9007199254741000'));
    assert.equal(entries.includes('123456789'), false);
  } finally { await state.cleanup(); }
});

test('list and usage do not load historical task results; list retains its existing summary fields', { timeout: 20_000 }, async t => {
  const storage = await openNodeSqliteStorage(':memory:'); const state = await setup(true, undefined, storage);
  try {
    const { receipt } = await studied(state);
    const get = (path: string) => state.service.app.inject({ url: `/api/v3/${path}`, headers: state.headers });
    const detail = (await get(`research/${receipt.conversationId}`)).json();
    const { input: _input, trialRecords: _trials, final: _final, tasks: _tasks, submission: _submission, previousStops: _stops, ...expected } = detail;
    const scan = storage.scanTasks.bind(storage); const scans: TaskQuery[] = [];
    t.mock.method(storage, 'scanTasks', (...args: Parameters<typeof scan>) => { scans.push(args[0]); return scan(...args); });
    const list = await get('research'); const usage = await get('usage');
    assert.equal(list.statusCode, 200); assert.deepEqual(list.json().items, [expected]);
    assert.equal(usage.statusCode, 200); assert.equal(usage.json().activeTasks, 0);
    assert.ok(scans.every(query => query.status && query.status !== 'terminal'), 'summary reads must not scan full historical task receipts');
  } finally { await state.cleanup(); }
});

test('detail hydrates task metadata once, shares concurrent reads and follows committed newest-100 updates', { timeout: 20_000 }, async t => {
  const storage = await openNodeSqliteStorage(':memory:'); const state = await setup(true, undefined, storage);
  try {
    const { receipt } = await studied(state); const host = state.service.research;
    const scan = storage.scanTasks.bind(storage); let scans = 0;
    t.mock.method(storage, 'scanTasks', (...args: Parameters<typeof scan>) => { if (args[0].conversationId === receipt.conversationId && !args[0].status) scans++; return scan(...args); });
    const get = () => state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    const [first, concurrent] = await Promise.all([get(), get()]);
    assert.equal(first.statusCode, 200); assert.deepEqual(first.json(), concurrent.json()); assert.equal(scans, 1);
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
    assert.equal(scans, 1, 'committed changes must update metadata without reloading evidence');
    const cold = await researchTaskViews(host)(receipt.conversationId); assert.deepEqual(cold, final.tasks); assert.equal(scans, 11, 'cold history is read in ten small pages, not one huge allocation');
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

test('public research failures retain codes but do not expose native stderr', { timeout: 20_000 }, async () => {
  const state = await setup(); const diagnostic = 'Native failure at /private/test-only/catalog\n' + 'internal diagnostic '.repeat(30);
  try {
    const { receipt } = await studied(state, 'private-errors');
    await state.service.research.harness.commit(async tx => { const study = await tx.doc(Study, receipt.conversationId as ConversationId); study.final!.status = 'FAILED'; study.final!.error = diagnostic; study.trials[0]!.error = diagnostic; }, context);
    const response = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers }); const view = response.json();
    assert.equal(view.final.error, 'scientific_execution_failed'); assert.equal(view.trialRecords[0].error, 'scientific_execution_failed'); assert.ok(!response.body.includes('/private/test-only'));
    const stored = await state.service.research.harness.snapshot(Study, receipt.conversationId as ConversationId, context); assert.equal(stored!.final!.error, diagnostic); assert.equal(stored!.trials[0]!.error, diagnostic);
    for (const exitCode of [137, 0, -1, '137']) {
      await state.service.research.harness.commit(async tx => {
        const study = await tx.doc(Study, receipt.conversationId as ConversationId);
        study.final!.errorDetail = study.trials[0]!.errorDetail = { operation: 'replay-catalog-ensemble', stage: 'execute', exitCode,
          code: 'synthetic-secret', message: '/private/test-only' };
      }, context);
      const projected = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
      for (const detail of [projected.json().final.errorDetail, projected.json().trialRecords[0].errorDetail]) {
        assert.equal(detail.exitCode, typeof exitCode === 'number' && exitCode >= 0 ? exitCode : undefined);
        assert.equal(detail.operation, 'replay-catalog-ensemble'); assert.equal(detail.stage, 'execute');
        if (exitCode === 137) { assert.match(detail.message, /可能/); assert.match(detail.message, /显式/); }
        assert.equal('signal' in detail, false);
      }
      assert.doesNotMatch(projected.body, /\/private\/test-only|synthetic-secret/);
    }
  } finally { await state.cleanup(); }
});

test('historical native codes and WAT failures keep specific safe public causes', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state, 'historic-native-causes');
    for (const [diagnostic, code, category] of [
      ['QZ_NATIVE_JOB_MEMORY_LIMIT\nprivate /private/test-only/model.wasm token=synthetic-secret', 'QZ_NATIVE_JOB_MEMORY_LIMIT', 'resource'],
      ['QZ_NATIVE_JOB_FAILED\nFORECAST_INSUFFICIENT_WARMUP: /private/test-only/catalog', 'FORECAST_INSUFFICIENT_WARMUP', 'data'],
      ['QZ_NATIVE_JOB_FAILED\nCATALOG_ENSEMBLE_SOURCE_SCOPE_INCOMPLETE: /private/test-only/archive', 'CATALOG_ENSEMBLE_SOURCE_SCOPE_INCOMPLETE', 'data'],
      ['QZ_NATIVE_JOB_FAILED\nCATALOG_ENSEMBLE_SCOPE_CHANGED: /private/test-only/scope', 'CATALOG_ENSEMBLE_SCOPE_CHANGED', 'data'],
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

test('portfolio reads and original admission replay share public failure projection', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const { input, view } = await built(state);
    for (const [diagnostic, code] of [
      ['Native backtest failed at /private/test-only/catalog', 'backtest_failed'],
      ['custom_evaluator_failed', 'custom_evaluator_failed'],
      ['QZ_NATIVE_JOB_MEMORY_LIMIT\n/private/test-only/catalog token=synthetic-secret', 'QZ_NATIVE_JOB_MEMORY_LIMIT'],
      ['parseWat failed:\nselected-factor.wat:1:2: error: private-source-marker', 'wasm_compile_failed'],
    ]) {
      await state.service.research.harness.commit(async tx => {
        const facts = await tx.doc(Application); const fact = facts.portfolios[view.id]!;
        const stored = fact.view as Record<string, unknown>; stored.error = diagnostic;
        stored.errorDetail = { code: 'synthetic-secret', message: 'private-source-marker', action: 'synthetic-secret' };
      }, context);
      const get = await state.service.app.inject({ url: `/api/v3/portfolios/${view.id}`, headers: state.headers });
      assert.equal(get.json().error, code); assert.equal(get.json().errorDetail.code, code);
      assert.doesNotMatch(get.body, /\/private\/test-only|private-source-marker|synthetic-secret/);
      const replay = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: input });
      assert.equal(replay.statusCode, 202); assert.equal(replay.json().id, view.id); assert.equal(replay.json().taskId, view.taskId);
      assert.equal(replay.json().error, code); assert.equal((await state.service.work.getPortfolio(view.id)).view.error, diagnostic);
      assert.doesNotMatch(replay.body, /\/private\/test-only|private-source-marker|synthetic-secret/);
    }
  } finally { await state.cleanup(); }
});

test('pre-kernel filesystem failure is retained privately rather than laundered to operation_failed', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const missing = join(state.directory, 'missing-private-scratch');
    const { receipt } = await studied(state, 'missing-environment'); const id = 'missing-scratch';
    const taskId = await state.service.research.harness.commit(async tx => {
      const taskId = await tx.createTask(BuildPortfolio, { id, dataDir: missing, catalog: state.directory, binary: state.options.config.jobBinary,
        evidence: {}, policy: state.options.config.datasets[0]!.strategy!, capital: '1000', currency: 'USD' },
        { conversationId: receipt.conversationId as ConversationId, ownership: { kind: 'conversation' } });
      (await tx.doc(Application)).portfolios[id] = { input: { requestId: id }, evidence: [], policy: {}, native: null, target: null,
        view: { id, name: id, researchIds: [receipt.conversationId], datasetId: 'controlled', status: 'QUEUED', capital: '1000', currency: 'USD',
          createdAt: '2026-10-19T00:00:00.000Z', taskId, metrics: [], curve: [], targets: [], error: null } };
      return taskId;
    }, context);
    const task = await state.service.research.harness.waitForTask(taskId, context);
    const { view } = await state.service.work.getPortfolio(id);
    assert.equal(view.status, 'FAILED'); assert.match(view.error!, /ENOENT/); assert.ok(view.error!.includes(missing));
    assert.equal(task.state.outcome.status, 'failed');
    if (task.state.outcome.status === 'failed') assert.equal(task.state.outcome.error.message, view.error);
    const response = await state.service.app.inject({ url: `/api/v3/portfolios/${view.id}`, headers: state.headers });
    assert.equal(response.json().error, 'ENOENT'); assert.equal(response.json().errorDetail.category, 'runtime');
    assert.doesNotMatch(response.body, /missing-private-scratch/);
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
