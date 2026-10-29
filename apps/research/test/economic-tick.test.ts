import test from 'node:test';
import assert from 'node:assert/strict';
import { validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { validateNativeTickEconomicRequest, validateNativeTickEconomicResult } from '../../../packages/contracts/src/economic-tick.ts';

export function economicRequest() {
  return {
    schema_version: 1,
    research: { schema_version: 1, input: { modality: 'TRADE_TICK', instrument_ids: ['BTC-USDT0.HYPERLIQUID'],
      event_start_ns: '0', event_end_ns: '691200000000000', decision_cutoff_ns: '691200000000000',
      representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1', maximum_source_rows: '1308222597', source_revision: 'original-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
      sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100000 },
      target: { kind: 'FIXED_EVENT_TIME', horizon_ns: '10000000000' }, parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 },
      train_end_ns: '259200000000000', validation_end_ns: '432000000000000' },
    scenario: { initial_usdt: '3000', usdt0_per_usdt: '1', spot_fraction: '0.5', gross_fraction: '1', maximum_asset_weight: '0.02', participation_fraction: '0.01',
      perp_taker_fee_bps: '4.5', spot_taker_fee_bps: '7', extra_cost_bps: '1.5', initial_margin_fraction: '0.1', maintenance_margin_fraction: '0.05',
      decision_interval_ns: '10000000000', base_latency_ns: '1000000', jitter_min_ns: '100000', jitter_max_ns: '1500000', random_seed: '0', maximum_mark_age_ns: '60000000000' },
    fx_instrument_ids: ['USDT0-USDC.HYPERLIQUID', '@150.HYPERLIQUID', '@230.HYPERLIQUID'], funding_source: 'funding-rates.jsonl',
  };
}

test('economic replay preserves exact original request and explicit cash/latency assumptions', () => {
  const request = economicRequest();
  request.scenario.random_seed = '9007199254740993';
  request.scenario.initial_usdt = '+0003000.000000000000000001';
  request.scenario.perp_taker_fee_bps = '+0004.5000'; request.scenario.spot_taker_fee_bps = '+0007.0000';
  const result = validateNativeTickEconomicRequest(request);
  assert.equal(result.scenario.initial_usdt, '3000.000000000000000001'); assert.equal(result.scenario.perp_taker_fee_bps, '4.5'); assert.equal(result.scenario.spot_taker_fee_bps, '7'); assert.deepEqual(result.research, request.research); assert.notEqual(result.research, request.research); assert.notEqual(result.scenario, request.scenario);
});

test('economic replay rejects invalid scenario bounds, mixed fields and nonexact wire values', () => {
  const updates = [
    { initial_usdt: '0' }, { usdt0_per_usdt: '-1' }, { spot_fraction: '0' }, { spot_fraction: '1' }, { gross_fraction: '1.000000000000000001' },
    { maximum_asset_weight: '0' }, { participation_fraction: '0' }, { participation_fraction: '1.1' },
    { initial_margin_fraction: '0.05' }, { maintenance_margin_fraction: '0.100000000000000001' },
    { perp_taker_fee_bps: '-1' }, { spot_taker_fee_bps: '-1' },
    { perp_taker_fee_bps: '9999.999999999999999999', extra_cost_bps: '0.000000000000000001' },
    { spot_taker_fee_bps: '9999.999999999999999999', extra_cost_bps: '0.000000000000000001' },
    { taker_fee_bps: '4.5' }, { perp_taker_fee_bps: undefined }, { spot_taker_fee_bps: undefined },
    { extra_cost_bps: '-1' }, { extra_cost_bps: '1e-3' }, { initial_usdt: '100000000000000000000' }, { initial_usdt: '0.0000000000000000001' },
    { decision_interval_ns: '0' }, { maximum_mark_age_ns: '0' }, { jitter_min_ns: '1500001' },
    { random_seed: '9223372036854775808' }, { random_seed: '01' }, { base_latency_ns: 1000 }, { unknown: '1' },
  ];
  for (const update of updates) assert.throws(() => validateNativeTickEconomicRequest({ ...economicRequest(), scenario: { ...economicRequest().scenario, ...update } }));
  for (const update of [{ unknown: true }, { funding_source: '../funding-rates.jsonl' }, { funding_source: '/funding-rates.jsonl' },
    { funding_source: 'funding-rates.parquet' }, { fx_instrument_ids: [] }, { fx_instrument_ids: ['A.SIM', 'A.SIM'] }]) {
    assert.throws(() => validateNativeTickEconomicRequest({ ...economicRequest(), ...update }));
  }
});


test('economic daily partitions require UTC cuts without restricting ordinary factor ticks', () => {
  for (const key of ['event_start_ns', 'event_end_ns', 'train_end_ns', 'validation_end_ns'] as const) {
    const request = economicRequest();
    request.research.input.decision_cutoff_ns = '777600000000000';
    if (key === 'event_start_ns' || key === 'event_end_ns') request.research.input[key] = String(BigInt(request.research.input[key]) + 1n);
    else request.research[key] = String(BigInt(request.research[key]) + 1n);
    assert.equal('evaluation_regimes' in request.research, false);
    assert.doesNotThrow(() => validateNativeTickFactorRequest(request.research));
    assert.throws(() => validateNativeTickEconomicRequest(request), /tick_economic_utc_daily_boundaries_required/);
  }
});


test('economic context is explicit and preserves absent-field legacy requests', () => {
  const legacy = validateNativeTickEconomicRequest(economicRequest());
  assert.equal(Object.hasOwn(legacy, 'context_source'), false);
  assert.equal(JSON.stringify(legacy).includes('context_source'), false);
  const requested = { ...economicRequest(), context_source: 'context-prices.manifest.json' };
  assert.equal(validateNativeTickEconomicRequest(requested).context_source, requested.context_source);
  for (const context_source of [null, '', '../context-prices.manifest.json', '/context-prices.manifest.json', 'other.json']) {
    assert.throws(() => validateNativeTickEconomicRequest({ ...economicRequest(), context_source }), /context_source_invalid/);
  }
});

test('small capital policy is optional and preserves exact validated values', () => {
  assert.equal(Object.hasOwn(validateNativeTickEconomicRequest(economicRequest()), 'small_capital_policy'), false);
  const request = { ...economicRequest(), small_capital_policy: { maximum_entry_notional_usdc: '+030.00', maximum_holding_ns: '60000000000' } };
  assert.deepEqual(validateNativeTickEconomicRequest(request).small_capital_policy, { maximum_entry_notional_usdc: '30', maximum_holding_ns: '60000000000' });
  for (const policy of [null, {}, { ...request.small_capital_policy, maximum_entry_notional_usdc: '0' },
    { ...request.small_capital_policy, maximum_holding_ns: '0' }, { ...request.small_capital_policy, extra: 1 }]) {
    assert.throws(() => validateNativeTickEconomicRequest({ ...request, small_capital_policy: policy }));
  }
});

test('small capital diagnostics are optional on legacy results and strictly validated when present', () => {
  const result = { schema_version: 1, request: economicRequest(), model: 'test', methods: {}, coverage: {}, assets: [], days: [], metrics: [],
    conversion_ns: null, initial_usdc_after_cost: null, funding_usdc: '0', fees_usdc_equivalent: null, fees_by_currency: {},
    initial_conversion_cost_usdc: '0', nominal_usd_settled_usdc: '0', native_fills: '0', native_rejections: '0',
    native_perp_pnl_rounding_events: '0', perp_pnl_rounding_bound_usdc: '0', funding_missing_instruments: [], targets: [],
    target_asof_ns: null, target_cash_weight: null, skipped_cash: '0', skipped_minimum: '0', consumed_fuel: null, limitations: [] };
  assert.doesNotThrow(() => validateNativeTickEconomicResult(result));
  const diagnostics = { entry_attempts: '1', unfilled_entries: '0', positions_opened: '1', positions_closed: '0', exit_attempts: '1',
    maximum_exit_delay_ns: null, remaining_positions: [{ instrument_id: 'A.SIM', quantity: '0.1', entry_decision_ns: '1', first_fill_ns: '2', exit_decision_ns: '3' }] };
  assert.doesNotThrow(() => validateNativeTickEconomicResult({ ...result, small_capital_diagnostics: diagnostics }));
  assert.throws(() => validateNativeTickEconomicResult({ ...result, small_capital_diagnostics: { ...diagnostics, exit_attempts: '-1' } }));
});
