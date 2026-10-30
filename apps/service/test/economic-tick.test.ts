import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
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
import { validateNativeTickEconomicRequest } from '../../../packages/contracts/src/economic-tick.ts';
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
import json, os, sys
request=json.load(sys.stdin)
assert sys.argv[1]=='replay-tick-economic'
assert os.path.isfile(sys.argv[sys.argv.index('--model')+1])
with open(${JSON.stringify(calls)},'a') as log: log.write('run\\n')
print(json.dumps({'schema_version':1,'request':request,'model':'CONTROLLED_PROTOCOL_ONLY','methods':{'scope':'test protocol only','catalog':sys.argv[sys.argv.index('--catalog')+1],'funding_catalog':sys.argv[sys.argv.index('--funding-catalog')+1]},
'coverage':{'modality':'TRADE_TICK','source_revision':request['research']['input']['source_revision']},
'assets':[{'instrument_id':'BTC.SIM','native_kind':'CryptoPerpetual','cohorts':'2','decisions':'1','execution_attempts':'0'}],
'days':[{'day_start_ns':'0','equity_usdt':None,'net_return':None,'unknown_reason':'controlled_missing_mark'}],
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
    representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1' as const, maximum_source_rows: '2', source_revision: 'controlled-original-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' as const },
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
    const researchId = await service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const input = { goal: 'Controlled retained factor protocol', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: json(data), policy: { maxTrials: 100 } };
      const study = await tx.doc(Study, conversation.id); study.input = input;
      study.trials.push({ taskId: 77, proposal: { hypothesis: 'Controlled retained version', model: '(module (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)) (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) f64.const 1))' }, status: 'COMPUTED',
        result: { request: iterationRequest(json(data)), native_versions: { 'nautilus-model': 'controlled-protocol' }, coverage: { modality: 'TRADE_TICK', source_revision: full.input.source_revision, trade_count: '2' } } });
      (await tx.doc(Requests)).requests['request:original'] = { input, conversationId: conversation.id };
      (await tx.doc(Application)).researchRequests['request:original'] = { original: { datasetId: 'ticks' }, input, dataset: json(dataset) };
      return conversation.id;
    }, context);
    // Republishing the registry must not redirect the retained factor's frozen inputs.
    service.config.datasets[0]!.data = { ...service.config.datasets[0]!.data, catalog: republishedPrimary,
      sealed: { ...json(service.config.datasets[0]!.data.sealed), catalog: republishedIndependent } };
    const request = validateNativeTickEconomicRequest({ schema_version: 1, research, scenario: { initial_usdt: '3000', usdt0_per_usdt: '1', spot_fraction: '0.5', gross_fraction: '1', maximum_asset_weight: '0.02', participation_fraction: '0.01',
      perp_taker_fee_bps: '4.5', spot_taker_fee_bps: '7', extra_cost_bps: '1.5', initial_margin_fraction: '0.1', maintenance_margin_fraction: '0.05', decision_interval_ns: '10000000000', base_latency_ns: '1000000', jitter_min_ns: '100000', jitter_max_ns: '1500000', random_seed: '0', maximum_mark_age_ns: '60000000000' },
      fx_instrument_ids: ['USDT0-USDC.HYPERLIQUID'], funding_source: 'funding-rates.jsonl', context_source: 'context-prices.manifest.json' });
    assert.equal(validateNativeTickEconomicRequest({ ...request, spot_valuation_source: 'spot-valuation.jsonl' }).spot_valuation_source, 'spot-valuation.jsonl');
    assert.throws(() => validateNativeTickEconomicRequest({ ...request, spot_valuation_source: '../spot-valuation.jsonl' }), /spot_valuation_source_invalid/);
    const payload = { requestId: 'economic-original', datasetId: 'ticks', factorId: `r${researchId}.t77.m0`, request };
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', payload })).statusCode, 401);
    const accepted = await service.app.inject({ method: 'POST', url: '/api/v3/tick-economic-replays', headers, payload });
    assert.equal(accepted.statusCode, 202, accepted.body); const view = accepted.json();
    await service.research.harness.waitForTask(view.taskId as TaskId, context);
    const result = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${view.id}`, headers })).json();
    assert.equal(result.status, 'COMPUTED', JSON.stringify(result)); assert.equal(result.scientificQualification, 'UNQUALIFIED_EXPLORATORY');
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
    const originalReference = references[0]!;
    for (const completed of [true, false]) {
      const id = completed ? 'recovered-original' : 'uncertain-original';
      const taskId = await service.research.harness.commit(async tx => {
        const taskId = await tx.createTask(TickEconomicReplay, { id, dataDir: service.config.dataDir, catalog, fundingCatalog: republishedPrimary, target, request,
          model: '(module)' }, { conversationId: researchId, ownership: { kind: 'conversation' } });
        (await tx.doc(TickEconomicReplays)).runs[id] = json({ original: { ...payload, requestId: id }, view: { ...view, fundingCatalog: republishedPrimary, id, taskId, status: 'QUEUED' } });
        (await tx.doc(NativeRemoteUncertainty, researchId, String(taskId), null)).requests.push({ ...originalReference, taskId,
          requestId: completed ? originalReference.requestId! : '0'.repeat(64), status: 'PENDING' });
        return taskId;
      }, context);
      await service.research.harness.waitForTask(taskId, context);
      const recovered = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${id}`, headers })).json();
      assert.equal(recovered.status, completed ? 'COMPUTED' : 'FAILED');
      assert.equal(recovered.fundingCatalog, republishedPrimary);
      if (completed) { assert.deepEqual(recovered.result, result.result); assert.equal(recovered.result.methods.funding_catalog, republishedPrimary); }
      else {
        const unknown = (await service.research.harness.snapshot(NativeRemoteUncertainty, researchId, String(taskId), context))!.requests[0]!;
        assert.equal(unknown.requestId, '0'.repeat(64)); assert.equal(unknown.status, 'UNKNOWN');
      }
    }
    let recoveryGets = 0; let recoveryPosts = 0; let terminal: 'COMPLETED' | 'FAILED' | 'CANCELLED' = 'COMPLETED'; let holdRecovery = false; let onRecoveryRead: (() => void) | undefined;
    const recovering = createServer((message, response) => {
      if (message.method !== 'GET') recoveryPosts++;
      assert.equal(message.url, `/v1/executions/${originalReference.requestId}`);
      recoveryGets++; onRecoveryRead?.(); response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify({ schemaVersion: 1, requestId: originalReference.requestId,
        ...(holdRecovery || recoveryGets < 3 ? { status: 'RUNNING' } : terminal === 'COMPLETED' ? { status: terminal, result: result.result } : terminal === 'FAILED' ? { status: terminal, error: 'controlled_failure' } : { status: terminal }) }));
    });
    recovering.listen(0, '127.0.0.1'); await once(recovering, 'listening');
    try {
      const recoveryTarget = { id: 'controlled-original-recovery', url: `http://127.0.0.1:${(recovering.address() as AddressInfo).port}` };
      for (const status of ['COMPLETED', 'FAILED', 'CANCELLED'] as const) {
        terminal = status; recoveryGets = 0; const id = `running-original-${status}`;
        const taskId = await service.research.harness.commit(async tx => {
          const taskId = await tx.createTask(TickEconomicReplay, { id, dataDir: service.config.dataDir,
            catalog, target: recoveryTarget, request, model: '(module)' }, { conversationId: researchId, ownership: { kind: 'conversation' } });
          (await tx.doc(TickEconomicReplays)).runs[id] = json({ original: payload, view: { ...view, fundingCatalog: republishedPrimary, id, taskId, status: 'QUEUED' } });
          (await tx.doc(NativeRemoteUncertainty, researchId, String(taskId), null)).requests.push({ ...originalReference, taskId, target: recoveryTarget, status: 'PENDING' });
          return taskId;
        }, context);
        await service.research.harness.waitForTask(taskId, context);
        const recovered = (await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${id}`, headers })).json();
        assert.equal(recovered.status, status === 'COMPLETED' ? 'COMPUTED' : 'FAILED');
        if (status === 'COMPLETED') assert.deepEqual(recovered.result, result.result);
        const reference = (await service.research.harness.snapshot(NativeRemoteUncertainty, researchId, String(taskId), context))!.requests[0]!;
        assert.equal(reference.status, status); assert.equal(recoveryGets, 3); assert.equal(recoveryPosts, 0);
      }
      holdRecovery = true; recoveryGets = 0;
      const firstRead = new Promise<void>(resolve => { onRecoveryRead = resolve; });
      const stoppedTaskId = await service.research.harness.commit(async tx => {
        const id = 'locally-stopped-original';
        const taskId = await tx.createTask(TickEconomicReplay, { id, dataDir: service.config.dataDir,
          catalog, target: recoveryTarget, request, model: '(module)' }, { conversationId: researchId, ownership: { kind: 'conversation' } });
        (await tx.doc(TickEconomicReplays)).runs[id] = json({ original: payload, view: { ...view, fundingCatalog: republishedPrimary, id, taskId, status: 'QUEUED' } });
        (await tx.doc(NativeRemoteUncertainty, researchId, String(taskId), null)).requests.push({ ...originalReference, taskId, target: recoveryTarget, status: 'PENDING' });
        return taskId;
      }, context);
      await firstRead;
      await service.research.harness.abortTask(stoppedTaskId, context);
      await service.research.harness.waitForTask(stoppedTaskId, context);
      const stopped = (await service.app.inject({ method: 'GET', url: '/api/v3/tick-economic-replays/locally-stopped-original', headers })).json();
      assert.equal(stopped.status, 'STOPPED'); assert.equal(stopped.error, 'tick_economic_local_stop_remote_receipt_unsettled');
      const retained = (await service.research.harness.snapshot(NativeRemoteUncertainty, researchId, String(stoppedTaskId), context))!.requests[0]!;
      assert.equal(retained.requestId, originalReference.requestId); assert.equal(retained.status, 'PENDING'); assert.equal(recoveryPosts, 0);
    } finally { recovering.closeAllConnections(); await new Promise<void>(resolve => recovering.close(() => resolve())); }
    assert.equal((await readFile(calls, 'utf8')).trim(), 'run\nrun', 'recovery only reads the retained original receipt; it never POSTs another native run');
    await service.close(); service = await createService(options);
    assert.deepEqual((await service.app.inject({ method: 'GET', url: `/api/v3/tick-economic-replays/${view.id}`, headers })).json(), result);
    assert.equal((await readFile(calls, 'utf8')).trim(), 'run\nrun');
  } finally { await service.close(); await worker.close(); await rm(directory, { recursive: true, force: true }); }
});
