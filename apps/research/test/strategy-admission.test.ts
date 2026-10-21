import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { thresholdPolicy, thresholdTargets, validateNativeSimulationSettings } from '../src/strategy.ts';

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
