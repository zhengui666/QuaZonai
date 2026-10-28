import test from 'node:test';
import assert from 'node:assert/strict';
import { validateNativeTickEconomicRequest } from '../../../packages/contracts/src/economic-tick.ts';

export function economicRequest() {
  return {
    schema_version: 1,
    research: { schema_version: 1, input: { modality: 'TRADE_TICK', instrument_ids: ['BTC-USDT0.HYPERLIQUID'],
      event_start_ns: '0', event_end_ns: '80000000000', decision_cutoff_ns: '80000000000',
      representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1', maximum_source_rows: '1308222597', source_revision: 'original-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
      sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100000 },
      target: { kind: 'FIXED_EVENT_TIME', horizon_ns: '10000000000' }, parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 },
      train_end_ns: '30000000000', validation_end_ns: '50000000000' },
    scenario: { initial_usdt: '3000', usdt0_per_usdt: '1', spot_fraction: '0.5', gross_fraction: '1', maximum_asset_weight: '0.02', participation_fraction: '0.01',
      taker_fee_bps: '3.5', extra_cost_bps: '1.5', initial_margin_fraction: '0.1', maintenance_margin_fraction: '0.05',
      decision_interval_ns: '10000000000', base_latency_ns: '1000000', jitter_min_ns: '100000', jitter_max_ns: '1500000', random_seed: '0', maximum_mark_age_ns: '60000000000' },
    fx_instrument_ids: ['USDT0-USDC.HYPERLIQUID', '@150.HYPERLIQUID', '@230.HYPERLIQUID'], funding_source: 'funding-rates.jsonl',
  };
}

test('economic replay preserves exact original request and explicit cash/latency assumptions', () => {
  const request = economicRequest();
  request.scenario.random_seed = '9007199254740993';
  request.scenario.initial_usdt = '+0003000.000000000000000001';
  const result = validateNativeTickEconomicRequest(request);
  assert.equal(result.scenario.initial_usdt, '3000.000000000000000001'); assert.deepEqual(result.research, request.research); assert.notEqual(result.research, request.research); assert.notEqual(result.scenario, request.scenario);
});

test('economic replay rejects invalid scenario bounds, mixed fields and nonexact wire values', () => {
  const updates = [
    { initial_usdt: '0' }, { usdt0_per_usdt: '-1' }, { spot_fraction: '0' }, { spot_fraction: '1' }, { gross_fraction: '1.000000000000000001' },
    { maximum_asset_weight: '0' }, { participation_fraction: '0' }, { participation_fraction: '1.1' },
    { initial_margin_fraction: '0.05' }, { maintenance_margin_fraction: '0.100000000000000001' },
    { taker_fee_bps: '-1' }, { taker_fee_bps: '9999.999999999999999999', extra_cost_bps: '0.000000000000000001' },
    { extra_cost_bps: '1e-3' }, { initial_usdt: '100000000000000000000' }, { initial_usdt: '0.0000000000000000001' },
    { decision_interval_ns: '0' }, { maximum_mark_age_ns: '0' }, { jitter_min_ns: '1500001' },
    { random_seed: '9223372036854775808' }, { random_seed: '01' }, { base_latency_ns: 1000 }, { unknown: '1' },
  ];
  for (const update of updates) assert.throws(() => validateNativeTickEconomicRequest({ ...economicRequest(), scenario: { ...economicRequest().scenario, ...update } }));
  for (const update of [{ unknown: true }, { funding_source: '../funding-rates.jsonl' }, { funding_source: '/funding-rates.jsonl' },
    { funding_source: 'funding-rates.parquet' }, { fx_instrument_ids: [] }, { fx_instrument_ids: ['A.SIM', 'A.SIM'] }]) {
    assert.throws(() => validateNativeTickEconomicRequest({ ...economicRequest(), ...update }));
  }
});
