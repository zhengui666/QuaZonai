import assert from 'node:assert/strict';
import { test } from 'node:test';
import { access, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { TaskId } from '@earendil-works/pi-durable';
import { datasetResearchSettings, researchSpecification } from '../../../packages/contracts/src/research.ts';
import research from '../../research/plugins/research.ts';
import strategyPortfolio from '../../research/plugins/strategy-portfolio.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import type { DatasetConfig } from '../src/config.ts';
import { loadConfig } from '../src/config.ts';
import { Application } from '../src/facts.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { object } from '../src/facts.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

const request = { schema_version: 1, event_start_ns: '0', evaluation_start_ns: '10', event_end_ns: '30', decision_cutoff_ns: '30',
  source: 'TimeSeventeen/Polymarket-v2', revision: 'TEST_ONLY_protocol', modeled_lag_ns: '0', score_threshold: 0,
  members: [{ weight: '0.5' }, { weight: '0.5' }], gross_exposure: '0.9', target_ttl_ns: '5',
  settings: { schema_version: 1, base_currency: 'pUSD', starting_capital: '3000', account_kind: 'MARGIN', leverage: '1',
    exposure_tolerance: '0.000001', snapshot_interval_ms: 1000, fee_rates: [],
    fee_model: { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_FEES', native_version: '0.63.0', parameters: {} },
    fill_model: { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_LIQUIDITY', native_version: '0.63.0',
      parameters: { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '1' } },
    latency_model: { schema_version: 1, adapter_kind: 'NAUTILUS_STATIC_LATENCY', upstream_class: 'nautilus_execution::models::latency::StaticLatencyModel', upstream_version: '0.63.0',
      parameters: { base_latency_ns: '1', insert_latency_ns: '0', update_latency_ns: '0', cancel_latency_ns: '0' } } } };
const specification = researchSpecification({ version: 1, name: 'Catalog ensemble protocol', question: 'Protocol only, no profitability claim', rationale: 'Original complete ensemble',
  evaluation: { metric: 'Sharpe Ratio (252 days)', direction: 'maximize', scheme: 'registered-partitions' },
  search: { scope: 'ensemble', maxMembers: 8, stop: 'budget-or-selection' } });

test('catalog ensemble projection reports original partitions and v2 input without a market shortlist', () => {
  const data = { catalog: '/private/catalog', catalog_partitions: '/private/partitions.json', train_end_ns: '10', validation_end_ns: '20', catalog_ensemble_request: request };
  const strategy = { name: 'all markets', kind: 'catalog-ensemble', evaluationPartition: 'HELDOUT', evidenceMode: 'HISTORICAL_REASSESSMENT' };
  const original = structuredClone(data);
  const view = datasetResearchSettings(data, strategy);
  assert.deepEqual(view.instruments, []);
  assert.match(view.inputFamily, /v2/);
  assert.match(view.inputFamily, /source_taker_quantity_imbalance/);
  assert.match(view.inputFamily, /source_fee_per_cash/);
  assert.equal(view.horizon, null);
  assert.deepEqual(view.partitions, { trainEndNs: '10', validationEndNs: '20', endNs: '30', historical: true });
  assert.equal(view.strategy?.kind, 'catalog-ensemble');
  assert.deepEqual(view.strategy?.simulationSettings, request.settings);
  assert.deepEqual(data, original);
});

test('catalog ensemble admission requires original manifest and matching account policy before model admission', async () => {
  const state = await setup();
  try {
    state.options.extensions.push(strategyPortfolio, factorWasm);
    const dataset: DatasetConfig = state.options.config.datasets[0]!;
    const catalog = dataset.data.catalog!;
    const data = { catalog, catalog_partitions: '/private/partitions.json', train_end_ns: '10', validation_end_ns: '20', catalog_ensemble_request: request };
    const strategy = { name: 'all markets', kind: 'catalog-ensemble' as const, evaluationPartition: 'HELDOUT' as const };
    for (const [index, [binding, policy, error, missingScope]] of [
      [{ ...data, catalog_partitions: '' }, strategy, 'catalog_partitions_required', false],
      [data, dataset.strategy, 'catalog_ensemble_strategy_required', false],
      [data, strategy, 'strategy_adapter_not_configured', true],
      [data, strategy, 'strategy_adapter_not_configured', 'all'],
      [data, strategy, 'heldout_data_required', false],
      [{ ...data, sealed: { ...data, catalog_partitions: '' } }, strategy, 'heldout_catalog_partitions_required', false],
      [{ ...data, sealed: { ...data, catalog_ensemble_request: { ...request, modeled_lag_ns: '1' } } }, strategy, 'heldout_criteria_mismatch', false],
      [{ ...data, catalog_ensemble_request: { ...request, source: 'unsupported' }, sealed: data }, strategy, 'catalog_ensemble_request_invalid', false],
      [{ ...data, catalog_ensemble_request: { ...request, settings: { ...request.settings, starting_capital: '0' } }, sealed: data }, strategy, 'strategy_simulation_settings_invalid', false],
      [{ ...data, sealed: { ...data, catalog_ensemble_request: { ...request, settings: { ...request.settings, fee_model: {} } } } }, strategy, 'strategy_simulation_model_invalid', false],
      [{ ...data, sealed: { ...data, catalog_ensemble_request: { ...request, event_end_ns: '20' } } }, strategy, 'catalog_ensemble_clock_invalid', false],
    ].entries()) {
      dataset.data = binding as typeof dataset.data;
      dataset.strategy = policy as typeof dataset.strategy;
      state.options.extensions = missingScope === 'all' ? [] : missingScope ? [strategyPortfolio] : [strategyPortfolio, factorWasm];
      await state.restart();
      const requestId = `catalog-invalid-${index}`;
      const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
        payload: { requestId, goal: specification.question, specification, model: state.model, datasetId: dataset.id, maxTrials: 100000 } });
      assert.equal(response.statusCode, 400, response.body);
      assert.equal(response.json().error, error);
      assert.equal(state.faux.state.callCount, 0);
      assert.equal((await state.service.research.harness.snapshot(Application, context))!.researchRequests[`request:${requestId}`], undefined);
      assert.equal((await state.service.research.harness.snapshot(Requests, context))?.requests[`request:${requestId}`], undefined);
    }
  } finally { await state.cleanup(); }
});

test('catalog manifest paths resolve against the service file for primary and sealed bindings', async () => {
  const state = await setup();
  try {
    const file = join(state.directory, 'relative-config.json');
    await writeFile(file, JSON.stringify({ datasets: [{ id: 'relative', data: { catalog: 'catalog', catalog_partitions: 'parts.json',
      sealed: { catalog: 'sealed', catalog_partitions: 'sealed-parts.json' } } }], extensions: [] }));
    const data = (await loadConfig(file)).datasets[0]!.data;
    assert.equal(data.catalog, join(state.directory, 'catalog'));
    assert.equal(data.catalog_partitions, join(state.directory, 'parts.json'));
    assert.deepEqual(data.sealed, { catalog: join(state.directory, 'sealed'), catalog_partitions: join(state.directory, 'sealed-parts.json') });
  } finally { await state.cleanup(); }
});

test('selected catalog ensemble replays one original shared account without copying the catalog or inventing a target', async () => {
  const state = await setup(); const previous = process.env.QZ_JOB_BINARY;
  try {
    const primary = join(state.directory, 'all-primary'); const sealed = join(state.directory, 'all-sealed');
    await mkdir(primary); await mkdir(sealed);
    for (const [catalog, end] of [[primary, '20'], [sealed, '30']]) await writeFile(join(catalog!, 'binding.json'), JSON.stringify({ end, account_replay_protocol: true }));
    const manifest = join(state.directory, 'registered-partitions.json'); await writeFile(manifest, '[]');
    const log = join(state.directory, 'catalog-account-calls.jsonl'); const binary = state.options.config.jobBinary;
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('../../research/test/portfolio-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
    process.env.QZ_JOB_BINARY = binary;
    const dataset: DatasetConfig = state.options.config.datasets[0]!;
    const binding = { catalog: sealed, catalog_partitions: manifest, train_end_ns: '10', validation_end_ns: '20', catalog_ensemble_request: request };
    dataset.data = { ...binding, catalog: primary, catalog_ensemble_request: { ...request, event_end_ns: '20', decision_cutoff_ns: '20' }, sealed: binding };
    dataset.currency = 'pUSD';
    dataset.strategy = { name: 'dynamic all-catalog account', kind: 'catalog-ensemble', evaluationPartition: 'HELDOUT' };
    state.options.extensions = [research, strategyPortfolio, factorWasm];
    await state.restart();
    const model = '(module (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)) (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) f64.const 1))';
    const members = [{ model, weight: '0.25' }, { model, weight: '0.25' }, { model, weight: '0.5' }];
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('strategy_portfolio_evaluate', { hypothesis: 'TEST_ONLY shared-account service protocol, no science', members }), { stopReason: 'toolUse' }),
      async () => {
        const page = await state.service.research.list(); const original = await state.service.research.view(page.items[0]!.id);
        return fauxAssistantMessage(fauxToolCall('strategy_portfolio_finalize', { trialId: original.research!.trials[0]!.taskId, reason: 'Original protocol selection' }), { stopReason: 'toolUse' });
      },
    ]);
    const receipt = await state.service.work.startResearch({ requestId: 'catalog-account-study', goal: specification.question, specification, model: state.model, datasetId: dataset.id, maxTrials: 1 });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const original = (await state.service.research.view(receipt.conversationId)).research!;
    assert.equal(original.final?.status, 'COMPUTED', JSON.stringify(original.final));
    assert.deepEqual(object(original.final!.result).partitions, []);
    await state.service.research.harness.commit(async tx => {
      delete object((await tx.doc(Study, receipt.conversationId)).final!.result).catalog_scope;
    }, context);
    const input = { requestId: 'catalog-account-replay', name: 'Original full ensemble', researchIds: [receipt.conversationId], weights: [{ researchId: receipt.conversationId, weight: '1.0' }], datasetId: dataset.id, capital: '3000' };
    const admitted = await state.service.work.startPortfolio(input);
    await state.service.research.harness.waitForTask(admitted.taskId as TaskId, context);
    const fact = await state.service.work.getPortfolio(admitted.id);
    assert.equal(fact.view.status, 'COMPUTED', fact.view.error ?? '');
    assert.equal(fact.native!.catalog, sealed);
    assert.equal(fact.target, null); assert.deepEqual(fact.view.targets, []);
    assert.equal(fact.view.curve[0]!.value, '3000');
    assert.equal(fact.view.capacity!.reason, 'catalog_ensemble_capacity_unavailable');
    assert.deepEqual(object(object(fact.evidence[0]!.selected).proposal).members, members);
    assert.deepEqual(original.input!.data, dataset.data);
    await assert.rejects(access(join(state.directory, 'catalogs')), { code: 'ENOENT' });
    const calls = (await readFile(log, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    const accounts = calls.filter(call => call.command === 'replay-catalog-ensemble');
    assert.equal(accounts.length, 3);
    const owner = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    assert.deepEqual(owner.json().final.executionRequest, accounts[1]!.input);
    assert.deepEqual(owner.json().final.registeredRequest.members, request.members);
    assert.equal('executionRequest' in owner.json().trialRecords[0], false);
    assert.equal('registeredRequest' in owner.json().trialRecords[0], false);
    assert.ok(accounts.every(call => call.scope.length === 2));
    assert.equal(owner.body.includes('scope.json'), false);
    assert.deepEqual(accounts.map(call => call.input.members), Array.from({ length: 3 }, () => members.map(member => ({ weight: member.weight }))));
    assert.ok(accounts.every(call => call.input.settings.starting_capital === '3000'));
    assert.equal(accounts.at(-1).input.evaluation_start_ns, '20');
    assert.equal(accounts.at(-1).args[accounts.at(-1).args.indexOf('--partitions') + 1], manifest);
    assert.equal((await state.service.work.startPortfolio(input)).id, fact.view.id);
    await assert.rejects(state.service.work.startPortfolio({ ...input, requestId: 'catalog-combine-rejected', researchIds: [receipt.conversationId, 999], weights: [{ researchId: receipt.conversationId, weight: '0.5' }, { researchId: 999, weight: '0.5' }] }), /catalog_ensemble_single_selection_required/);
    const factorId = `r${receipt.conversationId}.t${original.final!.trialId}.m0`;
    await assert.rejects(state.service.work.startPortfolio({ ...input, requestId: 'catalog-component-rejected', researchIds: undefined, weights: undefined, factors: [{ factorId, weight: '1' }] }), /catalog_ensemble_single_selection_required/);
    for (const url of [`/api/v3/portfolios/${fact.view.id}/capacity`, `/api/v3/factors/${factorId}/capacity`]) {
      const response = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: { scenarios: ['3000'], participation: '0.01', maximumAgeSeconds: 60 } });
      assert.equal(response.statusCode, 200, response.body);
      assert.equal(response.json().reason, 'catalog_ensemble_capacity_unavailable');
      assert.equal(response.json().basis, 'NOT_ASSESSED');
    }
    await assert.rejects(state.service.work.startObservation({ requestId: 'no-invented-target', portfolioId: fact.view.id, connectionId: 'missing' }), /portfolio_target_missing/);
    await state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, receipt.conversationId);
      object(object(study.final!.result).portfolio).statistics = [];
      object(study.final!.result).partitions = [{ partition: 'HELDOUT', metric: { counts: { included: '20' } } }];
    }, context);
    await assert.rejects(state.service.work.startPortfolio({ ...input, requestId: 'catalog-component-ic-is-not-account' }), /heldout_account_statistics_missing/);
    assert.equal((await readFile(log, 'utf8')).trim().split('\n').length, calls.length);
    assert.equal(state.faux.state.callCount, 2);
  } finally { if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await state.cleanup(); }
});
