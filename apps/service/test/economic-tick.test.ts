import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { TaskId } from '@earendil-works/pi-durable';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { NativeRemoteUncertainty } from '../../research/src/native-execution.ts';
import { TickEconomicReplay, TickEconomicReplays } from '../src/economic-tick.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { validateNativeTickEconomicRequest, validateNativeTickEconomicResult, type NativeTickEconomicRequest } from '../../../packages/contracts/src/economic-tick.ts';
import { Application, json } from '../src/facts.ts';
import { configDefaults } from '../src/config.ts';
import { createService } from '../src/service.ts';

const context = BACKGROUND_CONTEXT;
// Controlled original study and worker processes check protocol binding, not performance.
test('owner API binds a retained unselected computed factor through the actual remote worker without creating trials', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-economic-api-'));
  const catalog = join(directory, 'primary-catalog'); await mkdir(catalog);
  const independent = join(directory, 'independent-catalog'); await mkdir(independent);
  const republishedPrimary = join(directory, 'republished-primary'); await mkdir(republishedPrimary);
  const republishedIndependent = join(directory, 'republished-independent'); await mkdir(republishedIndependent);
  const binary = join(directory, 'controlled-job'); const calls = join(directory, 'calls');
  await writeFile(binary, `#!/usr/bin/python3
import json, os, sys, time
request=json.load(sys.stdin)
assert sys.argv[1]=='replay-tick-economic'
assert request['scenario']['decision_clock']=='FIXED_INTERVAL_V1'
models=[sys.argv[i+1] for i,arg in enumerate(sys.argv) if arg=='--model']
assert all(os.path.isfile(model) for model in models)
assert len(models)==len(request.get('signal_weights',['1']))
with open(${JSON.stringify(calls)},'a') as log: log.write('run\\n')
if request['scenario']['random_seed']=='44': time.sleep(3)
print(json.dumps({'schema_version':1,'request':request,'model':'CONTROLLED_PROTOCOL_ONLY','methods':{'scope':'test protocol only','models':str(len(models)),'catalog':sys.argv[sys.argv.index('--catalog')+1],'funding_catalog':sys.argv[sys.argv.index('--funding-catalog')+1]},
'coverage':{'modality':'TRADE_TICK','source_revision':request['research']['input']['source_revision']},
'assets':[{'instrument_id':'BTC.SIM','native_kind':'CryptoPerpetual','cohorts':'2','decisions':'1','execution_attempts':'0'}],
'days':[{'day_start_ns':'0','equity_usdt':None,'net_return':None,'unknown_reason':'controlled_missing_mark','missing_balances':[{'ledger':'CASH','currency':'BTC','amount':'0.2'},{'ledger':'MARGIN','currency':'ETH','amount':'1'}]}],
'metrics':[{'partition':'HELDOUT','regime':'UNKNOWN','days':'1','valid_return_days':'0','sharpe_365':None,'conditional_annualized_return_365':None}],
'conversion_ns':None,'initial_usdc_after_cost':None,'funding_usdc':'0','fees_usdc_equivalent':None,
'fees_by_currency':{},'initial_conversion_cost_usdc':'0','nominal_usd_settled_usdc':'0',
'native_fills':'0','native_rejections':'0','native_perp_pnl_rounding_events':'0','perp_pnl_rounding_bound_usdc':'0',
'funding_missing_instruments':[],'targets':[{'instrument_id':'BTC.SIM','currency':'USDC','weight':'0.1234567890123456789012345678'}],'target_asof_ns':None,'target_cash_weight':None,'skipped_cash':'0','skipped_minimum':'0','consumed_fuel':None,'limitations':['CONTROLLED_PROTOCOL_NOT_MARKET_EVIDENCE']}))
`, { mode: 0o700 });
  const worker = await createNautilusWorker({ binary, tickBinary: binary, dataDir: join(directory, 'worker'), catalogRoots: [catalog, independent, republishedPrimary, republishedIndependent] });
  worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening');
  const target = { id: 'test-economic-worker', url: `http://127.0.0.1:${(worker.server.address() as AddressInfo).port}` };
  const full = { schema_version: 1 as const, input: { modality: 'TRADE_TICK' as const, instrument_ids: ['BTC.SIM'], event_start_ns: '0', event_end_ns: '691200000000000', decision_cutoff_ns: '691200000000000',
    representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1' as const, source_revision: 'controlled-original-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' as const },
    sampling: { kind: 'EVENT_TIME_COHORT_V1' as const, maximum_cohort_rows: 100 }, target: { kind: 'FIXED_EVENT_TIME' as const, horizon_ns: '10000000000' },
    parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 }, train_end_ns: '259200000000000', validation_end_ns: '432000000000000' };
  const research = { ...full, input: { ...full.input, event_end_ns: full.validation_end_ns } };
  const data = { catalog, nativeInput: { modality: 'TRADE_TICK', request: research }, sealed: { catalog: independent, nativeInput: { modality: 'TRADE_TICK', request: full } }, nautilusTarget: target };
  const dataset = { id: 'ticks', name: 'Controlled original ticks', assetClass: 'TEST', currency: 'USDT', source: 'test protocol only', start: null, end: null, ready: true,
    nativeModality: 'TRADE_TICK' as const, preparation: { archiveLoaded: true, nativePrepared: true, scientificQualification: 'UNVERIFIED' as const }, data };
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const config = configDefaults({ dataDir: join(directory, 'service'), research: { database: join(directory, 'research.sqlite') }, datasets: [dataset], nautilus: { backtest: target } });
  const options = { config, models }; let service = await createService(options);
  try {
    const session = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'local-test-owner' } });
    assert.equal(session.statusCode, 200, session.body);
    const headers = { cookie: session.cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ') };
    assert.equal((await service.app.inject({ method: 'GET', url: '/api/v3/tick-economic-replays' })).statusCode, 401);
    assert.deepEqual((await service.app.inject({ method: 'GET', url: '/api/v3/tick-economic-replays', headers })).json(), []);
    for (const id of ['missing', '__proto__']) assert.equal((await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${id}`, headers })).statusCode, 404);
    const observedPhases: string[] = [];
    const commits = service.research.harness.observeCommits({ advance({ changes }) {
      for (const change of changes) if (change.type === 'task' && change.value.kind === TickEconomicReplay.definition.name) {
        const checkpoint = 'checkpoint' in change.value.state ? change.value.state.checkpoint : null;
        if (checkpoint && typeof checkpoint === 'object' && !Array.isArray(checkpoint)) {
          const progress = checkpoint.progress;
          if (progress && typeof progress === 'object' && !Array.isArray(progress) && typeof progress.phase === 'string') observedPhases.push(progress.phase);
        }
      }
    }, end() {} });
    const unsubscribe = () => commits.detach();
    const researchId = await service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const input = { goal: 'Controlled retained factor protocol', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: json(data), policy: {} };
      const study = await tx.doc(Study, conversation.id); study.input = input;
      study.trials.push({ taskId: 77, proposal: { hypothesis: 'Controlled retained version', model: '(module (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)) (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) f64.const 1))' }, status: 'COMPUTED',
        result: { request: iterationRequest(json(data)), native_versions: { 'nautilus-model': 'controlled-protocol' }, coverage: { modality: 'TRADE_TICK', source_revision: full.input.source_revision, trade_count: '2' } } });
      (await tx.doc(Requests)).requests['request:original'] = { input, conversationId: conversation.id, metadata: { original: { datasetId: 'ticks' }, dataset: json(dataset) } };
      return conversation.id;
    }, context);
    // Republishing the registry must not redirect the retained factor's frozen inputs.
    service.config.datasets[0]!.data = { ...service.config.datasets[0]!.data, catalog: republishedPrimary,
      sealed: { ...json(service.config.datasets[0]!.data.sealed), catalog: republishedIndependent } };
    const unlimitedInput = research.input;
    const request = validateNativeTickEconomicRequest({ schema_version: 1, research: { ...research, input: unlimitedInput }, scenario: { initial_usdt: '3000', usdt0_per_usdt: '1', spot_fraction: '0.5', gross_fraction: '1', maximum_asset_weight: '0.02', participation_fraction: '0.01',
      perp_taker_fee_bps: '4.5', spot_taker_fee_bps: '7', extra_cost_bps: '1.5', initial_margin_fraction: '0.1', maintenance_margin_fraction: '0.05', decision_interval_ns: '10000000000', decision_clock: 'FIXED_INTERVAL_V1', base_latency_ns: '1000000', jitter_min_ns: '100000', jitter_max_ns: '1500000', random_seed: '0', maximum_mark_age_ns: '60000000000' },
      fx_instrument_ids: ['USDT0-USDC.HYPERLIQUID'], funding_source: 'funding-rates.jsonl', context_source: 'context-prices.manifest.json' });
    assert.equal(Object.hasOwn(request.scenario, 'taker_fee_bps_by_instrument'), false);
    const overrideScenario = { ...request.scenario, taker_fee_bps_by_instrument: { 'xyz:NVDA.HYPERLIQUID': '09.00', 'BTC-USDC-SPOT.HYPERLIQUID': '1.40' } };
    assert.deepEqual(validateNativeTickEconomicRequest({ ...request, scenario: overrideScenario }).scenario.taker_fee_bps_by_instrument,
      { 'xyz:NVDA.HYPERLIQUID': '9', 'BTC-USDC-SPOT.HYPERLIQUID': '1.4' });
    for (const fee of ['-1', '9998.5', 'NaN', 9]) assert.throws(() => validateNativeTickEconomicRequest({ ...request,
      scenario: { ...request.scenario, taker_fee_bps_by_instrument: { 'xyz:NVDA.HYPERLIQUID': fee } } }));
    assert.throws(() => validateNativeTickEconomicRequest({ ...request, scenario: { ...request.scenario, taker_fee_bps_by_instrument: null } }));
    const legacyPolicy = { maximum_entry_notional_usdc: '100', maximum_holding_ns: '60500000000' };
    assert.deepEqual(validateNativeTickEconomicRequest({ ...request, small_capital_policy: legacyPolicy }).small_capital_policy, legacyPolicy);
    const clockPolicy = { ...legacyPolicy, decision_clock: 'FIXED_INTERVAL_V1' as const };
    assert.deepEqual(validateNativeTickEconomicRequest({ ...request, small_capital_policy: clockPolicy }).small_capital_policy, clockPolicy);
    assert.throws(() => validateNativeTickEconomicRequest({ ...request, small_capital_policy: { ...legacyPolicy, decision_clock: 'unknown' } }), /decision_clock_invalid/);
    assert.throws(() => validateNativeTickEconomicRequest({ ...request, small_capital_policy: { ...legacyPolicy, decision_clock: ['FIXED_INTERVAL_V1'] } }), /decision_clock_invalid/);
    assert.equal(validateNativeTickEconomicRequest({ ...request, spot_valuation_source: 'spot-valuation.jsonl' }).spot_valuation_source, 'spot-valuation.jsonl');
    assert.throws(() => validateNativeTickEconomicRequest({ ...request, spot_valuation_source: '../spot-valuation.jsonl' }), /spot_valuation_source_invalid/);
    const payload = { requestId: 'economic-original', datasetId: 'ticks', factorId: `r${researchId}.t77.m0`, request };
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', payload })).statusCode, 401);
    const accepted = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload });
    assert.equal(accepted.statusCode, 202, accepted.body); const view = accepted.json();
    await service.research.harness.waitForTask(view.taskId as TaskId, context);
    const result = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${view.id}`, headers })).json();
    assert.equal(result.status, 'COMPUTED', JSON.stringify(result)); assert.equal(result.scientificQualification, 'UNQUALIFIED_EXPLORATORY');
    assert.deepEqual([...new Set(observedPhases)], ['QUEUED', 'PREPARING_MODELS', 'REPLAYING']); unsubscribe();
    const taskBeforeReads = await service.research.harness.getTask(view.taskId as TaskId, context);
    const recordBeforeReads = await service.research.harness.snapshot(TickEconomicReplays, context);
    assert.equal(result.createdAt, (taskBeforeReads!.input as { createdAt: string }).createdAt);
    assert.ok(Number.isFinite(Date.parse(result.createdAt))); assert.equal(result.progress, null);
    assert.equal(result.executionModel, 'CONTROLLED_PROTOCOL_ONLY'); assert.equal(Object.hasOwn(result, 'portfolioId'), false);
    const summaries = (await service.app.inject({ method: 'GET', url: '/api/v3/tick-economic-replays', headers })).json();
    assert.deepEqual(summaries, [Object.fromEntries(Object.entries(result).filter(([key]) => key !== 'result'))]);
    assert.equal(Object.hasOwn(summaries[0], 'result'), false);
    assert.deepEqual((await service.app.inject({ method: 'GET', url: '/api/v3/portfolios', headers })).json(), []);
    await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${view.id}`, headers });
    assert.deepEqual(await service.research.harness.getTask(view.taskId as TaskId, context), taskBeforeReads);
    assert.deepEqual(await service.research.harness.snapshot(TickEconomicReplays, context), recordBeforeReads);
    assert.deepEqual(result.result.days[0].missing_balances, [{ ledger: 'CASH', currency: 'BTC', amount: '0.2' }, { ledger: 'MARGIN', currency: 'ETH', amount: '1' }]);
    const invalid = structuredClone(result.result); invalid.days[0].missing_balances[0].amount = 'unknown';
    assert.throws(() => validateNativeTickEconomicResult(invalid), /tick_economic_result_days_invalid/);
    const legacy = structuredClone(result.result); delete legacy.days[0].missing_balances;
    assert.doesNotThrow(() => validateNativeTickEconomicResult(legacy));
    const nullable = structuredClone(result.result); nullable.days[0].missing_balances = null;
    assert.throws(() => validateNativeTickEconomicResult(nullable), /tick_economic_result_days_invalid/);
    const invalidLedger = structuredClone(result.result); invalidLedger.days[0].missing_balances[0].ledger = 'OTHER';
    assert.throws(() => validateNativeTickEconomicResult(invalidLedger), /tick_economic_result_days_invalid/);
    invalidLedger.days[0].missing_balances[0].ledger = ['CASH'];
    assert.throws(() => validateNativeTickEconomicResult(invalidLedger), /tick_economic_result_days_invalid/);
    const largeBalance = structuredClone(result.result); largeBalance.days[0].missing_balances[0].amount = '999999999999999999999999999.12345678';
    assert.doesNotThrow(() => validateNativeTickEconomicResult(largeBalance));
    for (const amount of ['0', '-12.34', '+1', '.5', '1.']) {
      largeBalance.days[0].missing_balances[0].amount = amount;
      assert.doesNotThrow(() => validateNativeTickEconomicResult(largeBalance));
    }
    for (const amount of ['', '1e3', '1..2', '1\n']) {
      invalid.days[0].missing_balances[0].amount = amount;
      assert.throws(() => validateNativeTickEconomicResult(invalid), /tick_economic_result_days_invalid/);
    }
    assert.equal(result.result.methods.catalog, catalog);
    assert.equal(result.result.methods.funding_catalog, republishedPrimary); assert.equal(result.fundingCatalog, republishedPrimary);
    assert.deepEqual(result.result.request, request); assert.equal(result.result.request.context_source, 'context-prices.manifest.json'); assert.equal(result.result.targets[0].weight, '0.1234567890123456789012345678'); assert.equal(result.result.days[0].equity_usdt, null); assert.equal(result.result.days[0].net_return, null);
    assert.equal((await service.research.harness.snapshot(Study, researchId, context))!.trials.length, 1);
    assert.equal((await service.research.harness.snapshot(Study, researchId, context))!.final, null);
    const replayed = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload });
    assert.equal(replayed.statusCode, 202, replayed.body); assert.equal(replayed.json().id, view.id);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: { ...payload, request: { ...request, scenario: { ...request.scenario, random_seed: '1' } } } })).statusCode, 409);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: { ...payload, requestId: 'different-model', request: { ...request, research: { ...request.research, parameters: { ...request.research.parameters, slow_period_cohorts: 3 } } } } })).statusCode, 409);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers, payload: { requestId: 'still-unsupported', name: 'No BAR reinterpretation', datasetId: 'ticks', capital: '3000', researchIds: [researchId] } })).statusCode, 422);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: { ...payload, requestId: 'arbitrary-funding', fundingCatalog: catalog } })).statusCode, 409);
    const fullRequest = validateNativeTickEconomicRequest({ ...request, research: full });
    const final = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers,
      payload: { ...payload, requestId: 'economic-full-original', fundingCatalog: republishedIndependent, request: fullRequest } });
    assert.equal(final.statusCode, 202, final.body);
    await service.research.harness.waitForTask(final.json().taskId as TaskId, context);
    const fullResult = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${final.json().id}`, headers })).json();
    assert.equal(fullResult.status, 'COMPUTED'); assert.equal(fullResult.result.methods.catalog, independent);
    assert.deepEqual(fullResult.result.request, fullRequest);
    assert.equal(fullResult.result.methods.funding_catalog, republishedIndependent);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers,
      payload: { ...payload, requestId: 'unregistered-window', request: { ...request, research: { ...request.research, input: { ...request.research.input, event_end_ns: '604800000000000' } } } } })).statusCode, 409);
    const references = (await service.research.harness.snapshot(NativeRemoteUncertainty, researchId, String(view.taskId), context))!.requests;
    assert.equal(references.length, 1); assert.equal(references[0]!.status, 'COMPLETED');
    const originalTask = await service.research.harness.getTask(view.taskId as TaskId, context);
    const originalGraph = await service.research.harness.commit(tx => tx.scanTasks({ conversationId: researchId }, 100), context);
    const ownedNative = originalGraph.items.filter(task => task.kind === 'qz.native.execution' && task.owner === originalTask!.id);
    assert.equal(ownedNative.length, 1); assert.equal(ownedNative[0]!.state.status, 'terminal');
    assert.equal((ownedNative[0]!.input as { requestId: string }).requestId, references[0]!.requestId);
    // Stop the service while the actual worker process is still running. The
    // original native child must recover its checkpoint and GET its receipt.
    const restartRequest = { ...request, small_capital_policy: clockPolicy, scenario: { ...request.scenario, random_seed: '44' } };
    const restarting = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers,
      payload: { ...payload, requestId: 'economic-real-restart', request: restartRequest } });
    assert.equal(restarting.statusCode, 202, restarting.body);
    const restartView = restarting.json(); const deadline = Date.now() + 5000;
    while ((await readFile(calls, 'utf8')).trim().split('\n').length < 3) {
      assert.ok(Date.now() < deadline, 'the real worker must accept the third invocation');
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    const restartReference = (await service.research.harness.snapshot(NativeRemoteUncertainty, researchId, String(restartView.taskId), context))!.requests[0]!;
    await service.close(); service = await createService(options);
    await service.research.harness.waitForTask(restartView.taskId as TaskId, context);
    const restored = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${restartView.id}`, headers })).json();
    assert.equal(restored.status, 'COMPUTED', JSON.stringify(restored)); assert.deepEqual(restored.result.request, restartRequest);
    const restoredReference = (await service.research.harness.snapshot(NativeRemoteUncertainty, researchId, String(restartView.taskId), context))!.requests[0]!;
    assert.equal(restoredReference.requestId, restartReference.requestId); assert.equal(restoredReference.status, 'COMPLETED');
    assert.equal((await readFile(calls, 'utf8')).trim(), 'run\nrun\nrun', 'service reopen must not launch the original physical invocation twice');
    assert.deepEqual((await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${view.id}`, headers })).json(), result);
    assert.deepEqual((await service.app.inject({ method: 'GET', url: '/api/v3/tick-economic-replays', headers })).json().find((row: { id: string }) => row.id === view.id), summaries[0]);
    const addMember = (differentTarget = false) => service.research.harness.commit(async tx => {
      const original = (await tx.doc(Study, researchId)).input!;
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const input = { ...original, data: { ...original.data, ...(differentTarget ? { nautilusTarget: { ...target, id: 'another-worker' } } : {}) } };
      const study = await tx.doc(Study, conversation.id); study.input = input;
      const trial = json((await tx.doc(Study, researchId)).trials[0]!);
      study.trials.push({ ...trial, taskId: 78 });
      const key = `request:member-${conversation.id}`;
      (await tx.doc(Requests)).requests[key] = { input, conversationId: conversation.id, metadata: { original: { datasetId: 'ticks' }, dataset: json(dataset) } };
      return conversation.id;
    }, context);
    const secondResearchId = await addMember();
    const members = [{ factorId: payload.factorId, weight: '0.25' }, { factorId: `r${secondResearchId}.t78.m0`, weight: '0.75' }];
    const combination = { requestId: 'economic-combination', datasetId: 'ticks', members, request };
    const combined = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: combination });
    assert.equal(combined.statusCode, 202, combined.body);
    await service.research.harness.waitForTask(combined.json().taskId as TaskId, context);
    const combinedView = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${combined.json().id}`, headers })).json();
    assert.equal(combinedView.status, 'COMPUTED', JSON.stringify(combinedView));
    assert.deepEqual(combinedView.members, members); assert.equal(combinedView.factorId, undefined);
    assert.equal(combinedView.result.methods.models, '2'); assert.deepEqual(combinedView.result.request.signal_weights, ['0.25', '0.75']);
    assert.deepEqual(combinedView.sources, [researchId, secondResearchId].map((id, index) => ({ factorId: members[index]!.factorId, researchId: id, trialId: index ? 78 : 77, memberIndex: 0, sourceRevision: full.input.source_revision })));
    const combinedSummary = (await service.app.inject({ method: 'GET', url: '/api/v3/tick-economic-replays', headers })).json().find((row: { id: string }) => row.id === combinedView.id);
    assert.deepEqual(combinedSummary.members, members); assert.equal(Object.hasOwn(combinedSummary, 'result'), false);
    assert.equal((await service.research.harness.snapshot(Study, researchId, context))!.trials.length, 1);
    assert.equal((await service.research.harness.snapshot(Study, secondResearchId, context))!.trials.length, 1);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: combination })).json().id, combinedView.id);
    for (const invalid of [
      { ...combination, factorId: payload.factorId },
      { ...combination, members: [members[0]] },
      { ...combination, members: [members[0], members[0]] },
      { ...combination, members: members.map(member => ({ ...member, weight: '0.25' })) },
      { ...combination, request: { ...request, signal_weights: ['0.5', '0.5'] } },
    ]) assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: invalid })).statusCode, 400);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers,
      payload: { ...combination, members: members.map(member => ({ ...member, weight: '0.5' })) } })).statusCode, 409);
    const otherTargetId = await addMember(true);
    const differentTarget = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers,
      payload: { ...combination, requestId: 'member-other-target', members: [members[0], { factorId: `r${otherTargetId}.t78.m0`, weight: '0.75' }] } });
    assert.equal(differentTarget.statusCode, 409, differentTarget.body);
    assert.equal(differentTarget.json().error, 'tick_economic_member_binding_mismatch');
    await service.close(); service = await createService(options);
    assert.deepEqual((await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${combinedView.id}`, headers })).json(), combinedView);
    assert.equal((await readFile(calls, 'utf8')).trim(), 'run\nrun\nrun\nrun', 'fixed combinations retain one original invocation');

    const receivedFull: NativeTickEconomicRequest['research'] = { ...full,
      input: { ...full.input, instrument_ids: ['BTC.SIM', 'ETH.SIM', 'USDT0-USDC.HYPERLIQUID'], availability: 'RECEIVED_TIME_OBSERVED',
        representation: 'RECEIVED_TIME_COHORT_FLOW_CACHE_V1', maximum_receive_delay_ns: '86400000000000' },
      sampling: { ...full.sampling, kind: 'RECEIVED_TIME_COHORT_V1' }, target: { kind: 'FIXED_RECEIVED_TIME', horizon_ns: '10000000000' } };
    const receivedResearch = { ...receivedFull, input: { ...receivedFull.input, event_end_ns: receivedFull.validation_end_ns } };
    const receivedData = { ...data, nativeInput: { modality: 'TRADE_TICK', request: receivedResearch },
      sealed: { ...data.sealed, nativeInput: { modality: 'TRADE_TICK', request: receivedFull } } };
    const receivedDataset = { ...dataset, id: 'received-ticks', data: receivedData };
    const subsetTarget = { ...target, id: 'registered-subset-worker' };
    const subsetIds = ['BTC.SIM', 'USDT0-USDC.HYPERLIQUID'];
    const subsetResearch = { ...receivedResearch, input: { ...receivedResearch.input, instrument_ids: subsetIds } };
    const subsetFull = { ...receivedFull, input: { ...receivedFull.input, instrument_ids: subsetIds } };
    const subsetDataset = { ...dataset, id: 'received-subset', data: { ...receivedData, catalog: republishedPrimary,
      nativeInput: { modality: 'TRADE_TICK', request: subsetResearch }, nautilusTarget: subsetTarget,
      sealed: { catalog: republishedIndependent, nativeInput: { modality: 'TRADE_TICK', request: subsetFull }, nautilusTarget: subsetTarget } } };
    service.config.datasets.push(receivedDataset, subsetDataset);
    const receivedId = await service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const input = { ...(await tx.doc(Study, researchId)).input!, data: json(receivedData) };
      const study = await tx.doc(Study, conversation.id); study.input = input;
      const trial = json((await tx.doc(Study, researchId)).trials[0]!);
      study.trials.push({ ...trial, taskId: 79, result: { ...json(trial.result), request: iterationRequest(json(receivedData)) } });
      (await tx.doc(Requests)).requests['request:received-original'] = { input, conversationId: conversation.id,
        metadata: { original: { datasetId: receivedDataset.id }, dataset: json(receivedDataset) } };
      return conversation.id;
    }, context);
    const originalStudy = await service.research.harness.snapshot(Study, receivedId, context);
    const subsetRequest = validateNativeTickEconomicRequest({ ...request, research: subsetResearch });
    const subsetPayload = { requestId: 'registered-subset', datasetId: subsetDataset.id, factorId: `r${receivedId}.t79.m0`, request: subsetRequest };
    const subset = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: subsetPayload });
    assert.equal(subset.statusCode, 202, subset.body);
    await service.research.harness.waitForTask(subset.json().taskId as TaskId, context);
    const subsetView = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${subset.json().id}`, headers })).json();
    assert.equal(subsetView.status, 'COMPUTED', JSON.stringify(subsetView));
    assert.equal(subsetView.datasetId, subsetDataset.id); assert.equal(subsetView.scientificQualification, 'UNQUALIFIED_EXPLORATORY');
    assert.deepEqual(subsetView.result.request, subsetRequest);
    assert.equal(subsetView.result.methods.catalog, republishedPrimary); assert.equal(subsetView.fundingCatalog, republishedPrimary);
    assert.equal(subsetView.result.methods.funding_catalog, republishedPrimary);
    assert.deepEqual(((await service.research.harness.getTask(subset.json().taskId as TaskId, context))!.input as { target: unknown }).target, subsetTarget);
    assert.deepEqual(subsetView.sources, [{ factorId: subsetPayload.factorId, researchId: receivedId, trialId: 79, memberIndex: 0, sourceRevision: receivedFull.input.source_revision }]);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload: subsetPayload })).json().id, subsetView.id);
    const subsetHeldout = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers,
      payload: { ...subsetPayload, requestId: 'registered-subset-full', request: { ...subsetRequest, research: subsetFull } } });
    assert.equal(subsetHeldout.statusCode, 202, subsetHeldout.body);
    await service.research.harness.waitForTask(subsetHeldout.json().taskId as TaskId, context);
    const subsetHeldoutView = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${subsetHeldout.json().id}`, headers })).json();
    assert.equal(subsetHeldoutView.status, 'COMPUTED'); assert.equal(subsetHeldoutView.result.methods.catalog, republishedIndependent);
    assert.equal(subsetHeldoutView.result.methods.funding_catalog, republishedIndependent);
    assert.deepEqual(await service.research.harness.snapshot(Study, receivedId, context), originalStudy);
    const eventInput = { ...subsetResearch.input }; delete eventInput.maximum_receive_delay_ns;
    const incompatible: [string, NativeTickEconomicRequest['research']][] = [
      ['asset', { ...subsetResearch, input: { ...subsetResearch.input, instrument_ids: ['BTC.SIM', 'UNSEEN.SIM'] } }],
      ['source', { ...subsetResearch, input: { ...subsetResearch.input, source_revision: 'different-source' } }],
      ['range', { ...subsetResearch, input: { ...subsetResearch.input, event_end_ns: '518400000000000' } }],
      ['partition', { ...subsetResearch, train_end_ns: '172800000000000' }],
      ['horizon', { ...subsetResearch, target: { kind: 'FIXED_RECEIVED_TIME', horizon_ns: '20000000000' } }],
      ['parameters', { ...subsetResearch, parameters: { ...subsetResearch.parameters, slow_period_cohorts: 3 } }],
      ['family', { ...subsetResearch, input: { ...subsetResearch.input, representation: 'RECEIVED_TIME_COHORT_CACHE_V1' } }],
      ['clock', { ...subsetResearch, input: { ...eventInput, availability: 'EVENT_TIME_PROXY_UNVERIFIED', representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1' },
        sampling: { ...subsetResearch.sampling, kind: 'EVENT_TIME_COHORT_V1' }, target: { kind: 'FIXED_EVENT_TIME', horizon_ns: '10000000000' } }],
    ];
    for (const [name, changed] of incompatible) {
      const configured = { ...subsetDataset, id: `subset-${name}`, data: { ...subsetDataset.data, nativeInput: { modality: 'TRADE_TICK', request: changed } } };
      service.config.datasets.push(configured);
      const refused = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers,
        payload: { ...subsetPayload, requestId: `incompatible-subset-${name}`, datasetId: configured.id, request: { ...subsetRequest, research: changed } } });
      assert.equal(refused.statusCode, 409, `${name}: ${refused.body}`); assert.equal(refused.json().error, 'tick_economic_original_request_mismatch');
    }
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 6, 'subset retries reuse their original tasks and incompatible registered datasets never execute');
  } finally { await service.close(); await worker.close(); await rm(directory, { recursive: true, force: true }); }
});
