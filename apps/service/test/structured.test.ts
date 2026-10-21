import assert from 'node:assert/strict';
import { test } from 'node:test';
import { researchCapital, researchSpecification, researchInstructions, researchCriterion } from '../../../packages/contracts/src/research.ts';
import { factorViews, factorId } from '../src/factors.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import research from '../../research/plugins/research.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import heldoutWasm from '../../research/plugins/heldout-wasm.ts';
import strategyPortfolio from '../../research/plugins/strategy-portfolio.ts';
import { maxDbCounter } from '../../research/src/strategy.ts';
import { Application } from '../src/facts.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup, wat } from './support.ts';

const specification = researchSpecification({ version: 1, name: 'Volume hypothesis', question: 'Does volume predict the registered return?', rationale: 'Falsifiable original economic mechanism',
  evaluation: { metric: 'pearson_ic', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'single-factor', maxMembers: 1, stop: 'budget-or-selection' },
  capital: { scenarios: ['1000.000000000000000001'], participation: '0.01', maximumAgeSeconds: 120 } });

test('new structured native studies reject missing bindings and unsupported runners before model admission', async () => {
  const state = await setup();
  try {
    state.options.extensions.push(factorWasm, strategyPortfolio);
    const extensions = [...state.options.extensions];
    const dataset = state.options.config.datasets[0]!;
    const modelRef = (adapter_kind: string, upstream_class: string, parameters: Record<string, string> = {}) => ({ schema_version: 1, adapter_kind, upstream_class, upstream_version: '0.63.0', parameters });
    Object.assign(dataset.strategy!, { simulationSettings: { schema_version: 1, base_currency: 'USD', starting_capital: '3000', account_kind: 'MARGIN', leverage: '1', exposure_tolerance: '0.000001', snapshot_interval_ms: 60000,
      fill_model: modelRef('NAUTILUS_DEFAULT_FILL', 'nautilus_execution::models::fill::DefaultFillModel', { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '7' }),
      fee_model: modelRef('NAUTILUS_MAKER_TAKER', 'nautilus_execution::models::fee::MakerTakerFeeModel'),
      latency_model: modelRef('NAUTILUS_STATIC_LATENCY', 'nautilus_execution::models::latency::StaticLatencyModel', { base_latency_ns: '1', insert_latency_ns: '0', update_latency_ns: '0', cancel_latency_ns: '0' }),
      fee_rates: [{ instrument_id: 'TEST.SIM', maker: '0', taker: '0' }] } });
    const original = structuredClone(dataset.data);
    const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
      selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30', maximum_rows: 100 },
      parameters: { schema_version: 1, fast_period: 2, slow_period: 3, label_horizon_observations: 1 } } };
    const ensemble = researchSpecification({ ...specification, evaluation: { metric: 'Sharpe Ratio (252 days)', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'ensemble', maxMembers: 8, stop: 'budget-or-selection' } });
    const scan = { schema_version: 1, event_start_ns: '0', train_end_ns: '10', validation_end_ns: '20', event_end_ns: '30', decision_cutoff_ns: '30', maximum_rows: 100, parameters: request.forecast.parameters };
    const failures: { data: typeof original; spec: typeof specification; error: string; missingStrategy?: boolean; missingPortfolio?: boolean; missingFactor?: boolean }[] = [
      { data: { catalog: original.catalog!, storageMode: 'price-volume' }, spec: ensemble, error: 'native_request_required' },
      { data: { catalog: original.catalog!, catalog_scan_request: scan }, spec: ensemble, error: 'catalog_factor_only_account_binding_required' },
      { data: { catalog: original.catalog!, request, catalog_scan_request: scan }, spec: specification, error: 'ambiguous_factor_request' },
      { data: { request }, spec: specification, error: 'catalog_required' },
      { data: { catalog: original.catalog!, request }, spec: ensemble, error: 'strategy_policy_required', missingStrategy: true },
      { data: { catalog: original.catalog!, request }, spec: ensemble, error: 'strategy_adapter_not_configured', missingPortfolio: true },
      { data: { catalog: original.catalog!, request }, spec: specification, error: 'selected_factor_evaluator_unavailable', missingFactor: true },
      { data: { catalog: original.catalog!, request }, spec: specification, error: 'selected_factor_evaluator_unavailable', missingFactor: true, missingPortfolio: true },
      { data: { catalog: original.catalog!, request, sealed: { catalog: original.catalog!, request } }, spec: specification, error: 'registered_final_evaluator_required' },
    ];
    for (const [index, failure] of failures.entries()) {
      dataset.data = failure.data;
      const strategy = dataset.strategy;
      if (failure.missingStrategy) delete dataset.strategy;
      state.options.extensions = extensions.filter(extension => !(failure.missingPortfolio && extension === strategyPortfolio || failure.missingFactor && extension === factorWasm));
      await state.restart();
      const requestId = `invalid-native-binding-${index}`;
      const body = { requestId, goal: failure.spec.question, specification: failure.spec, model: state.model, datasetId: dataset.id, maxTrials: 100000 };
      const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: body });
      assert.equal(response.statusCode, 400, response.body); assert.equal(response.json().error, failure.error);
      assert.equal(state.faux.state.callCount, 0);
      assert.equal((await state.service.research.harness.snapshot(Application, context))!.researchRequests[`request:${requestId}`], undefined);
      assert.equal((await state.service.research.harness.snapshot(Requests, context))?.requests[`request:${requestId}`], undefined);
      dataset.strategy = strategy;
    }
    const strategy = dataset.strategy!;
    dataset.data = { catalog: original.catalog!, request };
    const invalidPolicies = [
      { policy: { kind: 'factor-threshold' }, error: 'strategy_instrument_invalid' },
      { policy: { ...strategy, simulationSettings: { base_currency: 'USD' } }, error: 'strategy_starting_capital_required' },
      { policy: { ...strategy, instrumentIds: ['TEST.SIM', 'MISSING.SIM'] }, error: 'strategy_instrument_fee_missing' },
      { policy: { ...strategy, simulationSettings: { ...strategy.simulationSettings, typo: true } }, error: 'strategy_simulation_settings_invalid' },
      { policy: { ...strategy, targetTtlNs: (maxDbCounter + 1n).toString() }, error: 'target_ttl_invalid' },
      { policy: { ...strategy, targetTtlNs: (maxDbCounter - 20n).toString() }, error: 'target_ttl_invalid' },
    ];
    for (const [index, invalid] of invalidPolicies.entries()) {
      dataset.strategy = invalid.policy as typeof strategy; state.options.extensions = extensions;
      await state.restart();
      const requestId = `invalid-native-policy-${index}`;
      const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
        payload: { requestId, goal: ensemble.question, specification: ensemble, model: state.model, datasetId: dataset.id, maxTrials: 100000 } });
      assert.equal(response.statusCode, 400, response.body);
      assert.equal(response.json().error, invalid.error);
      assert.equal(state.faux.state.callCount, 0);
      assert.equal((await state.service.research.harness.snapshot(Application, context))!.researchRequests[`request:${requestId}`], undefined);
      assert.equal((await state.service.research.harness.snapshot(Requests, context))?.requests[`request:${requestId}`], undefined);
    }
    dataset.strategy = strategy;
    state.options.extensions = [...extensions, heldoutWasm];
    const changed = structuredClone(request); changed.forecast.parameters.fast_period = 1;
    const sealedFailures: { sealed: typeof original; error: string }[] = [
      { sealed: { request }, error: 'heldout_catalog_required' },
      { sealed: { catalog: original.catalog! }, error: 'native_request_required' },
      { sealed: { catalog: original.catalog!, request: { ...request, train_end_ns: '0' } }, error: 'native_partition_boundaries' },
      { sealed: { catalog: original.catalog!, request: changed }, error: 'heldout_criteria_mismatch' },
    ];
    for (const [index, failure] of sealedFailures.entries()) {
      dataset.data = { catalog: original.catalog!, request, sealed: failure.sealed };
      await state.restart();
      const requestId = `invalid-native-sealed-${index}`;
      const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
        payload: { requestId, goal: specification.question, specification, model: state.model, datasetId: dataset.id, maxTrials: 100000 } });
      assert.equal(response.statusCode, 400, response.body); assert.equal(response.json().error, failure.error);
      assert.equal(state.faux.state.callCount, 0);
      assert.equal((await state.service.research.harness.snapshot(Application, context))!.researchRequests[`request:${requestId}`], undefined);
      assert.equal((await state.service.research.harness.snapshot(Requests, context))?.requests[`request:${requestId}`], undefined);
    }
    state.options.extensions = extensions;
    dataset.data = { catalog: original.catalog!, request };
    await state.restart();
    state.faux.setResponses([fauxAssistantMessage('Valid native binding admitted; no computation claimed.')]);
    const accepted = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'valid-native-ensemble', goal: ensemble.question, specification: ensemble, model: state.model, datasetId: dataset.id, maxTrials: 1 } });
    assert.equal(accepted.statusCode, 202, accepted.body);
    await (await state.service.research.harness.submission(accepted.json().submissionId, context))!.wait(context);
    assert.equal(state.faux.state.callCount, 1);
    state.options.extensions.push(heldoutWasm);
    dataset.data = { catalog: original.catalog!, request, sealed: { catalog: original.catalog!, request } };
    await state.restart();
    state.faux.setResponses([fauxAssistantMessage('Registered finalizer available; no computation claimed.')]);
    const sealed = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'valid-native-sealed-factor', goal: specification.question, specification, model: state.model, datasetId: dataset.id, maxTrials: 1 } });
    assert.equal(sealed.statusCode, 202, sealed.body);
    await (await state.service.research.harness.submission(sealed.json().submissionId, context))!.wait(context);
    assert.equal(state.faux.state.callCount, 2);
  } finally { await state.cleanup(); }
});

test('custom structured data remains opaque and an admitted request replays after native adapters are installed', async () => {
  const state = await setup();
  try {
    const datasets = state.options.config.datasets;
    datasets[0]!.data = { catalog: 'custom protocol catalog', request: { schema_version: 2, forecast: { protocol: 'opaque' } }, catalog_scan_request: { schema_version: 2, protocol: 'opaque' } };
    await state.restart();
    state.faux.setResponses([fauxAssistantMessage('Custom research input retained; no native result claimed.')]);
    const body = { requestId: 'opaque-original', goal: specification.question, specification, model: state.model, datasetId: 'controlled', maxTrials: 1 };
    const post = () => state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: body });
    const response = await post(); assert.equal(response.statusCode, 202, response.body); const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const original = (await state.service.research.state(receipt.conversationId)).research;
    const nativeInput = researchInput({ goal: specification.question, specification, model: state.model, data: datasets[0]!.data, policy: { max_trials: 1 } });
    state.faux.setResponses([fauxAssistantMessage('Original custom admission retained.')]);
    const nativeReceipt = await state.service.research.start('opaque-native-original', nativeInput);
    await (await state.service.research.harness.submission(nativeReceipt.submissionId, context))!.wait(context);
    state.options.extensions.push(factorWasm, strategyPortfolio);
    state.options.config.datasets = [];
    await state.restart();
    const replay = await post(); assert.equal(replay.statusCode, 202, replay.body); assert.deepEqual(replay.json(), receipt);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research, original);
    const nativeReplay = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'opaque-native-original', input: nativeInput } });
    assert.equal(nativeReplay.statusCode, 202, nativeReplay.body); assert.deepEqual(nativeReplay.json(), nativeReceipt);
    assert.equal(state.faux.state.callCount, 2);
    state.options.config.datasets = datasets;
    await state.restart();
    state.faux.setResponses([fauxAssistantMessage('Custom runner remains available beside native adapters.')]);
    const mixed = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...body, requestId: 'opaque-mixed' } });
    assert.equal(mixed.statusCode, 202, mixed.body);
    await (await state.service.research.harness.submission(mixed.json().submissionId, context))!.wait(context);
    assert.equal(state.faux.state.callCount, 3);
  } finally { await state.cleanup(); }
});

test('structured admission captures the mandate, rejects unsupported overrides and retains exact original identity through restart', async () => {
  const state = await setup();
  try {
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
    const input = { requestId: 'structured-original', goal: specification.question, specification, model: state.model, datasetId: 'controlled', maxTrials: 1 };
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input });
    assert.equal(response.statusCode, 202, response.body); const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const detail = (await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json();
    assert.equal(detail.title, specification.name); assert.deepEqual(detail.input.specification, specification); assert.equal(detail.criterion.status, 'insufficient');
    assert.match(researchInstructions(detail.input), /Original structured research mandate/); assert.match(researchInstructions(detail.input), /Volume hypothesis/);
    const factors = (await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json();
    assert.equal(factors.length, 1); assert.equal(factors[0].id, factorId(receipt.conversationId, detail.final.trialId, 0)); assert.equal(factors[0].evidence, 'independent');
    assert.equal(factors[0].combinable, false); assert.equal(factors[0].reason, 'research_criterion_insufficient');
    const assumptions = { scenarios: ['20.000000000000000001'], participation: '0.02', maximumAgeSeconds: 60 };
    assert.deepEqual(researchCapital(assumptions), assumptions); assert.throws(() => researchCapital({ ...assumptions, horizon: 9 }), /research_override_unsupported/);
    const assessed = await state.service.app.inject({ method: 'POST', url: `/api/v3/factors/${factors[0].id}/capacity`, headers: state.headers, payload: assumptions }); assert.equal(assessed.statusCode, 200, assessed.body);
    assert.equal(assessed.json().status, 'UNAVAILABLE'); assert.equal(assessed.json().assumptions.participation, '0.02'); assert.equal(assessed.json().scenarios[0].capital, assumptions.scenarios[0]);
    assert.equal((await state.service.app.inject({ method: 'POST', url: `/api/v3/factors/${factors[0].id}/capacity`, headers: state.headers, payload: { ...assumptions, maximumAgeSeconds: 0 } })).statusCode, 400);
    assert.deepEqual((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json().input, detail.input);
    const body = { requestId: 'atomic-original', name: 'Atomic shared-account configuration', factors: [{ factorId: factors[0].id, weight: '1' }], datasetId: 'controlled', capital: '1000' };
    for (const members of [{ factors: body.factors }, { researchIds: [receipt.conversationId] }]) {
      const rejected = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: { ...body, factors: undefined, ...members } });
      assert.equal(rejected.statusCode, 422, rejected.body); assert.equal(rejected.json().error, 'research_criterion_insufficient');
    }
    assert.equal(assessed.json().reason, 'research_criterion_insufficient');
    assert.equal((await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...input, specification: { ...specification, name: 'changed' } } })).statusCode, 409);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input })).json(), receipt);
    assert.equal((await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json()[0].id, factors[0].id);
    assert.equal(state.faux.state.callCount, 1);
    const bad = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...input, requestId: 'unsupported', specification: { ...specification, horizon: 2 } } });
    assert.equal(bad.statusCode, 400); assert.equal(bad.json().error, 'research_override_unsupported');
  } finally { await state.cleanup(); }
});

test('supported search scope is enforced in the original counted trial ledger, not only model prose', async () => {
  const state = await setup();
  try {
    const ensemble = researchSpecification({ ...specification, evaluation: { metric: 'Sharpe Ratio (252 days)', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'ensemble', maxMembers: 2, stop: 'budget-or-selection' } });
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
    const receipt = await state.service.work.startResearch({ requestId: 'scope', goal: ensemble.question, specification: ensemble, model: state.model, datasetId: 'controlled', maxTrials: 1 });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.equal(study.trials.length, 1); assert.equal(study.trials[0]!.status, 'FAILED'); assert.equal(study.trials[0]!.error, 'research_search_scope_exceeded');
  } finally { await state.cleanup(); }
});

test('structured budget-or-selection rejects an unselected model response boundary and continues to an actual selection', async () => {
  const state = await setup();
  try {
    state.options.extensions.push(research); await state.restart();
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Response boundary is not a scientific stop' }), { stopReason: 'toolUse' }),
      transcript => { assert.match(JSON.stringify(transcript), /research_stop_condition_not_met/); return fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' }); },
    ]);
    const receipt = await state.service.work.startResearch({ requestId: 'structured-stop', goal: specification.question, specification, model: state.model, datasetId: 'controlled', maxTrials: 3 });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.equal(study.trials.length, 1); assert.equal(study.final!.status, 'COMPUTED'); assert.equal(study.final!.trialId, study.trials[0]!.taskId); assert.equal(state.faux.state.callCount, 2);
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Original zero trial budget' }), { stopReason: 'toolUse' })]);
    const zero = await state.service.work.startResearch({ requestId: 'structured-zero-stop', goal: specification.question, specification, model: state.model, datasetId: 'controlled', maxTrials: 0 });
    await (await state.service.research.harness.submission(zero.submissionId, context))!.wait(context);
    assert.equal((await state.service.research.harness.snapshot(Study, zero.conversationId, context))!.final!.reason, 'Original zero trial budget');
  } finally { await state.cleanup(); }
});

test('structured native input accepts only the registered inherited policy and supported direction', async () => {
  const state = await setup();
  try {
    const registered = state.options.config.datasets[0]!;
    assert.ok(registered.strategy && registered.strategy.kind !== 'catalog-ensemble');
    registered.strategy = { ...registered.strategy!, instrumentIds: ['TEST.SIM'], evaluationPartition: 'HELDOUT' };
    await state.restart();
    const policy = { ...registered.policy, max_trials: 0, strategy: registered.strategy };
    const input = { goal: specification.question, specification, model: state.model, data: registered.data, policy };
    const post = (requestId: string, value: unknown) => state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId, input: value } });
    assert.throws(() => researchSpecification({ ...specification, evaluation: { ...specification.evaluation, direction: 'minimize' } }), /research_evaluation_unsupported/);
    for (const [name, strategy] of Object.entries({ instruments: { ...registered.strategy, instrumentIds: ['UNREGISTERED'] }, simulation: { ...registered.strategy, simulationSettings: { base_currency: 'OTHER' } }, partition: { ...registered.strategy, evaluationPartition: 'TRAIN' }, strategy: { replacement: true } })) {
      const bad = await post(`native-override-${name}`, { ...input, policy: { ...policy, strategy } });
      assert.equal(bad.statusCode, 400, bad.body); assert.equal(bad.json().error, 'research_policy_unregistered');
    }
    const extra = await post('native-extra-policy', { ...input, policy: { ...policy, partitionOverride: 'TRAIN' } }); assert.equal(extra.statusCode, 400); assert.equal(extra.json().error, 'research_policy_unregistered');
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
    const accepted = await post('native-registered-policy', { ...input, policy: { ...policy, max_trials: 1 } }); assert.equal(accepted.statusCode, 202, accepted.body);
    await (await state.service.research.harness.submission(accepted.json().submissionId, context))!.wait(context);
    const recorded = (await state.service.research.state(accepted.json().conversationId)).research!.input!;
    assert.deepEqual(recorded.policy.strategy, registered.strategy);
    await state.restart(); assert.equal((await post('native-registered-policy', { ...input, policy: { ...policy, max_trials: 1 } })).json().conversationId, accepted.json().conversationId);
    const legacy = await post('native-legacy-policy', { ...input, specification: undefined, policy: { max_trials: 0, legacyOverride: true } }); assert.equal(legacy.statusCode, 202, legacy.body);
    await (await state.service.research.harness.submission(legacy.json().submissionId, context))!.wait(context);
  } finally { await state.cleanup(); }
});

test('identical scientific inputs retain their own original factor admission provenance after restart', async () => {
  const state = await setup();
  try {
    const expected = new Map<number, { id: string; name: string; source: string; currency: string }>();
    for (const [index, currency] of ['USD', 'CNY'].entries()) {
      const registered = state.options.config.datasets[0]!;
      registered.id = `provenance-${index}`; registered.name = `Original dataset ${index}`; registered.source = `TEST_ONLY source ${index}`; registered.currency = currency;
      await state.restart();
      state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
      const receipt = await state.service.work.startResearch({ requestId: `identical-input-${index}`, goal: specification.question, specification, model: state.model, datasetId: registered.id, maxTrials: 1 });
      await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
      expected.set(receipt.conversationId, { id: registered.id, name: registered.name, source: registered.source, currency });
    }
    await state.restart();
    const factors = (await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json(); assert.equal(factors.length, 2);
    for (const factor of factors) {
      const original = expected.get(factor.researchId)!; assert.equal(factor.datasetId, original.id); assert.equal(factor.currency, original.currency);
      const detail = (await state.service.app.inject({ url: `/api/v3/factors/${factor.id}`, headers: state.headers })).json(); assert.equal(detail.datasetId, original.id); assert.equal(detail.currency, original.currency);
      const research = (await state.service.app.inject({ url: `/api/v3/research/${factor.researchId}`, headers: state.headers })).json(); assert.equal(research.dataset.name, original.name); assert.equal(research.dataset.source, original.source);
    }
  } finally { await state.cleanup(); }
});

test('capital scenarios match positive native NUMERIC(38,18) and raw length bounds without changing original strings', () => {
  const accepted = ['99999999999999999999.999999999999999999', '0.000000000000000001', '0'.repeat(63) + '1', '1.' + '0'.repeat(62), '+.1', '1.', '000000000000000000001.2300000000000000000000'];
  for (const scenario of accepted) assert.deepEqual(researchCapital({ scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 }).scenarios, [scenario]);
  for (const scenario of ['100000000000000000000', '0'.repeat(64) + '1', '1.' + '0'.repeat(63), '0.0000000000000000001', '1.0000000000000000001', '1\n', '1e2', '-1', '0']) {
    assert.throws(() => researchCapital({ scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 }), /research_capital_scenarios_invalid/, scenario);
  }
});

test('HTTP rejects out-of-native-range capacity scenarios before research, factor lookup or native execution', async () => {
  const state = await setup();
  try {
    for (const scenario of ['100000000000000000000', '0'.repeat(64) + '1', '0.0000000000000000001']) {
      const capital = { scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 };
      for (const [url, payload] of [
        ['/api/v3/factors/r999.t1.m0/capacity', capital],
        ['/api/v3/research', { requestId: `invalid-${scenario}`, goal: specification.question, specification: { ...specification, capital }, model: state.model, datasetId: 'controlled', maxTrials: 1 }],
        ['/api/v3/portfolios', { requestId: `invalid-${scenario}`, name: 'Invalid native range', researchIds: [999], datasetId: 'controlled', capital: '1000', capacityPolicy: capital }],
      ] as const) {
        const response = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload });
        assert.equal(response.statusCode, 400, response.body); assert.equal(response.json().error, 'research_capital_scenarios_invalid');
      }
    }
    assert.equal(state.faux.state.callCount, 0);
  } finally { await state.cleanup(); }
});

test('structured factor qualification requires an actual requested heldout metric with or without a threshold; legacy remains readable', () => {
  for (const target of [undefined, 0.1]) for (const value of [null, 0.2]) {
    const spec = { ...specification, evaluation: { ...specification.evaluation, ...(target === undefined ? {} : { target }) } };
    const result = { partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '2' }, pearson_ic: { value, status: value === null ? 'CONSTANT_SCORE' : 'DEFINED' } } }] };
    const trial = { taskId: 17, status: 'COMPUTED', proposal: { hypothesis: 'TEST_ONLY qualification protocol, not science', model: wat }, result };
    const study = { input: { goal: spec.question, specification: spec, model: { provider: 'test', modelId: 'test' }, data: {}, policy: { max_trials: 1, strategy: { simulationSettings: { base_currency: 'USD' } } } }, trials: [trial], final: { status: 'COMPUTED', trialId: 17, result } };
    const input = researchInput(study.input); const factor = factorViews(3, { ...study, input })[0]!; assert.equal(factor.combinable, value !== null); assert.equal(factor.reason, value === null ? 'research_criterion_insufficient' : null);
    const legacy = { ...input }; delete legacy.specification;
    assert.equal(factorViews(3, { ...study, input: legacy })[0]!.combinable, true);
  }
});

test('deterministic criterion never substitutes component diagnostics or missing metrics for a pass', () => {
  const spec = { ...specification, evaluation: { ...specification.evaluation, target: 0.1 } };
  const metric = { key: 'HELDOUT.pearson_ic', label: 'Pearson IC', partition: 'HELDOUT', value: 0.2 };
  assert.equal(researchCriterion(spec, 'COMPUTED', [metric])!.status, 'met');
  assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, value: 0.01 }])!.status, 'not_met');
  assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, component: 0 }])!.status, 'insufficient');
  assert.equal(researchCriterion(spec, 'FAILED', [metric])!.status, 'failed');
  const study = { input: { goal: specification.question, specification, model: { provider: 'test', modelId: 'test' }, data: {}, policy: { max_trials: 1, strategy: { simulationSettings: { base_currency: 'USD' } } } },
    trials: [{ taskId: 17, status: 'COMPUTED', proposal: { hypothesis: 'Controlled projection, not scientific evidence', members: [{ model: wat, weight: '0.25' }, { model: wat, weight: '0.75' }] }, result: {} }], final: { status: 'COMPUTED', trialId: 17, result: {} } };
  const factors = factorViews(3, { ...study, input: researchInput(study.input) }); assert.deepEqual(factors.map(row => row.id), ['r3.t17.m0', 'r3.t17.m1']);
  assert.ok(factors.every(row => row.evidence === 'component_only' && !row.combinable && row.metrics.length === 0));
});
