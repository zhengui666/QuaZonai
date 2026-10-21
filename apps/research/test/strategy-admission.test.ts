import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { maxDbCounter, thresholdPolicy, thresholdTargets, validateCatalogEnsembleRequest, validateNativeSimulationSettings } from '../src/strategy.ts';

const settings: JsonObject = { schema_version: 1, base_currency: 'pUSD', starting_capital: '3000', account_kind: 'MARGIN', leverage: '1',
  exposure_tolerance: '0.000001', snapshot_interval_ms: 1000,
  fee_model: { schema_version: 1, adapter_kind: 'NAUTILUS_MAKER_TAKER', upstream_class: 'nautilus_execution::models::fee::MakerTakerFeeModel', upstream_version: '0.63.0', parameters: {} },
  fill_model: { schema_version: 1, adapter_kind: 'NAUTILUS_DEFAULT_FILL', upstream_class: 'nautilus_execution::models::fill::DefaultFillModel', upstream_version: '0.63.0',
    parameters: { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '1' } },
  latency_model: { schema_version: 1, adapter_kind: 'NAUTILUS_STATIC_LATENCY', upstream_class: 'nautilus_execution::models::latency::StaticLatencyModel', upstream_version: '0.63.0',
    parameters: { base_latency_ns: '1', insert_latency_ns: '0', update_latency_ns: '0', cancel_latency_ns: '0' } },
  fee_rates: [{ instrument_id: 'A.SIM', maker: '0', taker: '0' }] };
const policy: JsonObject = { name: 'original', kind: 'factor-threshold', instrumentId: 'A.SIM', threshold: 0, investedWeight: '0.75', targetTtlNs: '5', simulationSettings: settings };

test('admission shares required threshold mapping validation with target construction', () => {
  const before = structuredClone(policy);
  assert.equal(thresholdPolicy(policy), policy);
  assert.equal(validateNativeSimulationSettings(settings), settings);
  for (const [field, error] of [['instrumentId', 'strategy_instrument_invalid'], ['threshold', 'strategy_threshold_invalid'],
    ['investedWeight', 'policy_weight_decimal_required'], ['targetTtlNs', 'target_ttl_counter_required'], ['simulationSettings', 'strategy_simulation_settings_required']]) {
    const incomplete = { ...policy }; delete incomplete[field!];
    assert.throws(() => thresholdPolicy(incomplete), new RegExp(error!));
    assert.throws(() => thresholdTargets({ points: [] }, incomplete as never, 'pUSD'), new RegExp(error!));
  }
  assert.throws(() => thresholdPolicy({ ...policy, threshold: Infinity }), /strategy_threshold_invalid/);
  assert.throws(() => thresholdPolicy({ ...policy, investedWeight: '1.1' }), /strategy_weight_invalid/);
  assert.throws(() => thresholdPolicy({ ...policy, targetTtlNs: '0' }), /target_ttl_invalid/);
  assert.throws(() => thresholdPolicy({ ...policy, targetTtlNs: (maxDbCounter + 1n).toString() }), /target_ttl_invalid/);
  assert.equal(thresholdPolicy({ ...policy, targetTtlNs: maxDbCounter.toString() }).targetTtlNs, maxDbCounter.toString());
  assert.deepEqual(policy, before);
});

test('native admission requires every simulation wire field and its primitive type without changing legacy mapping settings', () => {
  for (const field of Object.keys(settings)) {
    const incomplete = { ...settings }; delete incomplete[field];
    assert.throws(() => validateNativeSimulationSettings(incomplete), /strategy_/);
  }
  for (const invalid of [{ ...settings, account_kind: null }, { ...settings, account_kind: ['CASH'] }, { ...settings, schema_version: '1' }, { ...settings, starting_capital: 3000 },
    { ...settings, leverage: 'NaN' }, { ...settings, exposure_tolerance: Infinity }, { ...settings, snapshot_interval_ms: '1000' },
    { ...settings, fee_model: {} }, { ...settings, fill_model: [] }, { ...settings, latency_model: { schema_version: 1, adapter_kind: 'NAUTILUS_STATIC_LATENCY', upstream_class: 'x', upstream_version: '0.63.0' } },
    { ...settings, fee_rates: [{ instrument_id: 'A.SIM', maker: '0' }] }]) {
    assert.throws(() => validateNativeSimulationSettings(invalid), /strategy_/);
  }
  const legacy = { ...policy, simulationSettings: { base_currency: 'pUSD' } };
  assert.equal(thresholdPolicy(legacy), legacy);
  assert.throws(() => validateNativeSimulationSettings(legacy.simulationSettings), /strategy_starting_capital_required/);
});

test('native admission validates the original supported adapter parameters before any execution', () => {
  for (const field of ['fill_model', 'latency_model']) {
    const model = settings[field] as JsonObject;
    const parameters = model.parameters as JsonObject;
    assert.throws(() => validateNativeSimulationSettings({ ...settings, [field]: { ...model, parameters: {} } }), /strategy_simulation_/);
    for (const key of Object.keys(parameters)) {
      const incomplete = { ...parameters }; delete incomplete[key];
      assert.throws(() => validateNativeSimulationSettings({ ...settings, [field]: { ...model, parameters: incomplete } }), /strategy_simulation_/);
      assert.throws(() => validateNativeSimulationSettings({ ...settings, [field]: { ...model, parameters: { ...parameters, [key]: Number(parameters[key]) } } }), /strategy_simulation_/);
    }
  }
  for (const field of ['fee_model', 'fill_model', 'latency_model']) {
    const model = settings[field] as JsonObject;
    assert.throws(() => validateNativeSimulationSettings({ ...settings, [field]: { ...model, adapter_kind: 'UNKNOWN' } }), /strategy_simulation_model_invalid/);
    assert.throws(() => validateNativeSimulationSettings({ ...settings, [field]: { ...model, parameters: { ...(model.parameters as JsonObject), unsupported: '0' } } }), /strategy_simulation_model_parameters_invalid/);
  }
  const fill = settings.fill_model as JsonObject;
  for (const random_seed of ['9223372036854775808', '01', '-1']) assert.throws(() => validateNativeSimulationSettings({ ...settings,
    fill_model: { ...fill, parameters: { ...(fill.parameters as JsonObject), random_seed } } }), /strategy_simulation_/);
  const recorded = { ...settings,
    fee_model: { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_FEES', native_version: '0.63.0', parameters: {} },
    fill_model: { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_LIQUIDITY', native_version: '0.63.0', parameters: fill.parameters! } };
  assert.equal(validateNativeSimulationSettings(recorded), recorded);
  assert.throws(() => validateNativeSimulationSettings({ ...recorded, fee_model: { ...recorded.fee_model, native_version: undefined } }), /strategy_simulation_model_invalid/);
  assert.throws(() => validateNativeSimulationSettings({ ...settings,
    fee_model: { schema_version: 1, adapter_kind: 'FROZEN_SPOT_FEE_SCENARIO', native_version: '0.63.0', parameters: {} } }), /strategy_simulation_model_invalid/);
});

test('native admission binds exact linked model identities and all supported static combinations', () => {
  for (const field of ['fee_model', 'fill_model', 'latency_model']) {
    const model = settings[field] as JsonObject;
    for (const change of [{ upstream_version: '0.64.0' }, { upstream_class: 'another::Model' }]) {
      assert.throws(() => validateNativeSimulationSettings({ ...settings, [field]: { ...model, ...change } }), /strategy_simulation_model_invalid/);
    }
  }
  const fill = settings.fill_model as JsonObject;
  const recordedFill = { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_LIQUIDITY', native_version: '0.63.0', parameters: fill.parameters! };
  const fees: JsonObject[] = [settings.fee_model as JsonObject,
    { schema_version: 1, adapter_kind: 'NAUTILUS_POLYMARKET', upstream_class: 'nautilus_polymarket::models::PolymarketFeeModel', upstream_version: '0.63.0', parameters: {} },
    { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_FEES', native_version: '0.63.0', parameters: {} }];
  for (const fee of fees) {
    const original = { ...settings, fee_model: fee };
    assert.equal(validateNativeSimulationSettings(original), original);
    if (fee.adapter_kind !== 'POLYMARKET_V2_RECORDED_FEES') {
      assert.throws(() => validateNativeSimulationSettings({ ...original, fill_model: recordedFill }), /strategy_simulation_model_invalid/);
    }
  }
  const recorded = { ...settings, fee_model: fees[2]!, fill_model: recordedFill };
  assert.equal(validateNativeSimulationSettings(recorded), recorded);
  for (const field of ['fee_model', 'fill_model']) {
    assert.throws(() => validateNativeSimulationSettings({ ...recorded, [field]: { ...(recorded[field as 'fee_model' | 'fill_model']), native_version: '0.64.0' } }), /strategy_simulation_model_invalid/);
  }
  for (const change of [{ account_kind: 'CASH' }, { leverage: '2' }, { base_currency: 'USD' }]) {
    assert.throws(() => validateNativeSimulationSettings({ ...recorded, ...change }), /strategy_simulation_model_invalid/);
  }
});

test('native admission rejects static account and model bounds using exact decimals and original currencies', () => {
  for (const change of [{ starting_capital: '0' }, { starting_capital: '-1' }, { leverage: '0' }, { leverage: '100.000000000000000001' },
    { exposure_tolerance: '0' }, { exposure_tolerance: '0.001000000000000001' }, { account_kind: 'CASH', leverage: '1.000000000000000001' }]) {
    assert.throws(() => validateNativeSimulationSettings({ ...settings, ...change }), /strategy_simulation_settings_invalid/);
  }
  for (const currency of ['USD', 'EUR', 'CNY', 'USDC', 'USDC.e', 'pUSD', 'USDT']) {
    const original = { ...settings, base_currency: currency, multi_currency_spot_cash: null };
    assert.equal(validateNativeSimulationSettings(original), original);
  }
  for (const currency of ['BTC', 'PUSD', 'usd', 'FAKE']) assert.throws(() => validateNativeSimulationSettings({ ...settings, base_currency: currency }), /strategy_currency_unsupported/);
  for (const multi_currency_spot_cash of [{}, false]) assert.throws(() => validateNativeSimulationSettings({ ...settings, multi_currency_spot_cash }), /multi_currency_spot_cash_not_integrated/);
  const fill = settings.fill_model as JsonObject;
  for (const key of ['prob_fill_on_limit', 'prob_slippage']) {
    for (const invalid of ['-0.000000000000000001', '1.000000000000000001']) {
      assert.throws(() => validateNativeSimulationSettings({ ...settings, fill_model: { ...fill, parameters: { ...(fill.parameters as JsonObject), [key]: invalid } } }), /strategy_simulation_model_parameters_invalid/);
    }
  }
  const latency = settings.latency_model as JsonObject;
  assert.throws(() => validateNativeSimulationSettings({ ...settings, latency_model: { ...latency,
    parameters: { base_latency_ns: '0', insert_latency_ns: '0', update_latency_ns: '0', cancel_latency_ns: '0' } } }), /strategy_simulation_model_parameters_invalid/);
  assert.throws(() => validateNativeSimulationSettings({ ...settings, fee_rates: [...settings.fee_rates as JsonObject[], ...settings.fee_rates as JsonObject[]] }), /strategy_instrument_fee_duplicate/);
  for (const instrument_id of ['A\n.SIM', 'A'.repeat(201)]) assert.throws(() => validateNativeSimulationSettings({ ...settings, fee_rates: [{ instrument_id, maker: '0', taker: '0' }] }), /strategy_instrument_invalid/);
  const exact = { ...settings, starting_capital: '+.000000000000000001', leverage: '+0001.00000000000000000000', exposure_tolerance: '.0010000000000000000000' };
  assert.equal(validateNativeSimulationSettings(exact), exact);
  assert.throws(() => validateNativeSimulationSettings({ ...settings, starting_capital: '0'.repeat(64) + '1' }), /strategy_simulation_decimal_required/);
});

test('native admission rejects unknown native fields and requires original fees for every admitted instrument', () => {
  assert.throws(() => validateNativeSimulationSettings({ ...settings, exposure_tollerance: '0.000001' }), /strategy_simulation_settings_invalid/);
  for (const field of ['fee_model', 'fill_model', 'latency_model']) {
    assert.throws(() => validateNativeSimulationSettings({ ...settings, [field]: { ...(settings[field] as JsonObject), unexpected: 'x' } }), /strategy_simulation_model_invalid/);
  }
  assert.throws(() => validateNativeSimulationSettings({ ...settings, fee_rates: [{ ...(settings.fee_rates as JsonObject[])[0], takeer: '0' }] }), /strategy_instrument_fee_invalid/);
  assert.equal(validateNativeSimulationSettings(settings, ['A.SIM']), settings);
  assert.throws(() => validateNativeSimulationSettings(settings, ['B.SIM']), /strategy_instrument_fee_missing/);
  assert.throws(() => validateNativeSimulationSettings(settings, ['A.SIM', 'B.SIM']), /strategy_instrument_fee_missing/);
  const complete = { ...settings, fee_rates: [...settings.fee_rates as JsonObject[], { instrument_id: 'B.SIM', maker: '0', taker: '0' }] };
  const before = structuredClone(complete);
  assert.equal(validateNativeSimulationSettings(complete, ['A.SIM', 'B.SIM']), complete);
  assert.deepEqual(complete, before);
});

const catalogRequest: JsonObject = { schema_version: 1, event_start_ns: '0', evaluation_start_ns: '10', event_end_ns: '30', decision_cutoff_ns: '30',
  source: 'TimeSeventeen/Polymarket-v2', revision: 'original-source-revision', modeled_lag_ns: '0', score_threshold: 0,
  members: [{ weight: '0.5' }, { weight: '0.5' }], gross_exposure: '0.9', target_ttl_ns: '5', settings: { ...settings, fee_rates: [],
    fee_model: { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_FEES', native_version: '0.63.0', parameters: {} },
    fill_model: { schema_version: 1, adapter_kind: 'POLYMARKET_V2_RECORDED_LIQUIDITY', native_version: '0.63.0', parameters: (settings.fill_model as JsonObject).parameters! } } };
const catalogData: JsonObject = { train_end_ns: '10', validation_end_ns: '20', catalog_ensemble_request: catalogRequest };

test('catalog admission reuses native settings while scanning an uncapped source universe with dynamic member weights', () => {
  const before = structuredClone(catalogData);
  assert.deepEqual(validateCatalogEnsembleRequest(catalogData), catalogRequest);
  assert.deepEqual(catalogData, before);
  assert.throws(() => validateNativeSimulationSettings(catalogRequest.settings), /strategy_instrument_fee_missing/);
  const template: JsonObject = { ...catalogRequest, revision: 'another-original-revision', members: [{ weight: '1' }, { weight: '2', total_fuel: '10000' }, { weight: '3', total_fuel: null }], score_max_age_ns: null };
  const normalized = validateCatalogEnsembleRequest({ ...catalogData, catalog_ensemble_request: template });
  assert.deepEqual(normalized.members, [{ weight: '1' }, { weight: '2', total_fuel: '10000' }, { weight: '3' }]);
  assert.equal(Object.hasOwn(normalized, 'score_max_age_ns'), false);
  assert.equal(template.score_max_age_ns, null);
});

test('catalog admission rejects malformed native source, models, exposure and member bindings before paid execution', () => {
  for (const field of Object.keys(catalogRequest)) {
    const incomplete = { ...catalogRequest }; delete incomplete[field];
    assert.throws(() => validateCatalogEnsembleRequest({ ...catalogData, catalog_ensemble_request: incomplete }), /required|invalid|boundaries/);
  }
  const invalidRequests: JsonObject[] = [{ schema_version: 2 }, { source: 'TEST_ONLY' }, { revision: ' ' }, { source_extra: true },
    { gross_exposure: '0' }, { gross_exposure: '1.000000000000000001' }, { target_ttl_ns: '0' }, { score_threshold: Infinity },
    { score_max_age_ns: '0' }, { members: [{ weight: '1' }] }, { members: [{ weight: '0' }, { weight: '1' }] },
    { members: [{ weight: '1', total_fuel: '0' }, { weight: '1' }] }, { members: [{ weight: '1', unexpected: '1' }, { weight: '1' }] }];
  for (const change of invalidRequests) {
    assert.throws(() => validateCatalogEnsembleRequest({ ...catalogData, catalog_ensemble_request: { ...catalogRequest, ...change } }), /catalog_ensemble_/);
  }
  const originalSettings = catalogRequest.settings as JsonObject;
  const invalidSettings: JsonObject[] = [{ fill_model: settings.fill_model }, { fee_model: settings.fee_model, fill_model: settings.fill_model },
    { account_kind: 'CASH' }, { base_currency: 'USD' }, { leverage: '2' }, { starting_capital: '0' },
    { multi_currency_spot_cash: {} }, { unsupported: true }];
  for (const change of invalidSettings) {
    assert.throws(() => validateCatalogEnsembleRequest({ ...catalogData, catalog_ensemble_request: { ...catalogRequest, settings: { ...originalSettings, ...change } } }), /catalog_ensemble_|strategy_|multi_currency_/);
  }
  for (const field of Object.keys(originalSettings)) {
    const incomplete = { ...originalSettings }; delete incomplete[field];
    assert.throws(() => validateCatalogEnsembleRequest({ ...catalogData, catalog_ensemble_request: { ...catalogRequest, settings: incomplete } }), /strategy_/);
  }
});

test('catalog admission validates raw and clipped partition clocks, scalar limits and the native settlement TTL', () => {
  const invalid: JsonObject[] = [{ evaluation_start_ns: '0' }, { evaluation_start_ns: '30' }, { decision_cutoff_ns: '29' },
    { modeled_lag_ns: '11' }, { target_ttl_ns: '2' }, { target_ttl_ns: maxDbCounter.toString() },
    { modeled_lag_ns: (maxDbCounter + 1n).toString() }, { score_max_age_ns: (maxDbCounter + 1n).toString() }, { evaluation_start_ns: '01' },
    { members: [{ weight: '1', total_fuel: (maxDbCounter + 1n).toString() }, { weight: '1' }] }];
  for (const change of invalid) {
    assert.throws(() => validateCatalogEnsembleRequest({ ...catalogData, catalog_ensemble_request: { ...catalogRequest, ...change } }), /catalog_ensemble_|strategy_simulation_/);
  }
  const delayed = { ...catalogRequest, modeled_lag_ns: '10', score_max_age_ns: '1', target_ttl_ns: '3' };
  assert.deepEqual(validateCatalogEnsembleRequest({ ...catalogData, catalog_ensemble_request: delayed }), delayed);
});
