import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { resolveStrategyPolicy, simulationSelection, thresholdPolicy, thresholdTargets } from '../src/strategy.ts';

const allocations = [{ instrumentId: 'A.SIM', weight: '0.4' }, { instrumentId: 'B-YES.SIM', weight: '0.6' }];
const original: JsonObject = { name: 'shared-many-markets', kind: 'cross-market-threshold', instrumentIds: ['A.SIM', 'B-YES.SIM', 'C.SIM'],
  threshold: 0, investedWeight: '0.75', targetTtlNs: '5', forecastMode: 'price-volume', evaluationPartition: 'HELDOUT',
  simulationSettings: { starting_capital: '3000', base_currency: 'pUSD', fee_rates: [
    { instrument_id: 'A.SIM', maker: '0.005000', taker: '0.005000' },
    { instrument_id: 'B-YES.SIM', maker: '0.000000', taker: '0.000000' },
    { instrument_id: 'C.SIM', maker: '0.001000', taker: '0.001000' },
  ] } };
const point = (instrument_id: string, at: string, forecast: number | null): JsonObject => ({ instrument_id, available_ns: at, event_ns: at, forecast });
const units = (value: unknown) => { const [a, b = ''] = String(value).split('.'); return BigInt(a + b.padEnd(18, '0')); };
const weights = (frame: JsonObject) => (frame.targets as JsonObject[]).map((target) => target.weight);

test('cross-market resolution binds every registered exact allocation and its original reserve without changing original inputs', () => {
  const before = structuredClone(original);
  const resolved = resolveStrategyPolicy({ allocations }, original);
  assert.equal(resolved.kind, 'cross-market-threshold');
  assert.deepEqual((resolved as unknown as JsonObject).allocations, allocations);
  assert.equal(resolved.simulationSettings.starting_capital, '3000');
  assert.deepEqual(resolved.simulationSettings.fee_rates, ((original.simulationSettings as JsonObject).fee_rates as JsonObject[]).slice(0, 2));
  assert.deepEqual(original, before);
  const request: JsonObject = { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30',
    bar_types: ['C.SIM-1-MINUTE-LAST-EXTERNAL', 'B-YES.SIM-1-MINUTE-LAST-EXTERNAL', 'A.SIM-1-MINUTE-LAST-EXTERNAL', 'B-YES.SIM-OTHER-1-MINUTE-LAST-EXTERNAL'] } } };
  const selected = simulationSelection(request, 'VALIDATION', resolved);
  assert.deepEqual(selected.bar_types, ['A.SIM-1-MINUTE-LAST-EXTERNAL', 'B-YES.SIM-1-MINUTE-LAST-EXTERNAL']);
  const [frame] = thresholdTargets({ points: [point('A.SIM', '10', 1), point('B-YES.SIM', '10', 1)] }, resolved, 'pUSD');
  assert.deepEqual((frame!.targets as JsonObject[]).map((target) => String(target.instrument_id) + '-1-MINUTE-LAST-EXTERNAL'), selected.bar_types);
  assert.equal(selected.event_start_ns, '10'); assert.equal(selected.event_end_ns, '20');
  assert.equal(((request.forecast as JsonObject).selection as JsonObject).event_end_ns, '30');
});

test('real asynchronous market observations share one exact risky budget and never add missing or expiry BARs', () => {
  const policy = resolveStrategyPolicy({ allocations }, original);
  const forecasts = { points: [point('B-YES.SIM', '18', -1), point('A.SIM', '10', 1), point('B-YES.SIM', '11', 1),
    point('C.SIM', '12', 999), point('B-YES.SIM', '15', 1), point('A.SIM', '16', null), point('A.SIM', '17', -1), point('A.SIM', '18', 1)] };
  const before = structuredClone(forecasts);
  const frames = thresholdTargets(forecasts, policy, 'pUSD');
  assert.deepEqual(frames.map((frame) => frame.asof_ns), ['10', '11', '15', '16', '17', '18']);
  assert.deepEqual(frames.map((frame) => frame.rebalance_instrument_ids), [['A.SIM'], ['B-YES.SIM'], ['B-YES.SIM'], ['A.SIM'], ['A.SIM'], ['A.SIM', 'B-YES.SIM']]);
  assert.deepEqual(frames.map(weights), [
    ['0.300000000000000000', '0.000000000000000000'],
    ['0.300000000000000000', '0.450000000000000000'],
    ['0.000000000000000000', '0.450000000000000000'],
    ['0.000000000000000000', '0.450000000000000000'],
    ['0.000000000000000000', '0.450000000000000000'],
    ['0.300000000000000000', '0.000000000000000000'],
  ]);
  assert.deepEqual(frames.map((frame) => frame.cash_weight), ['0.700000000000000000', '0.250000000000000000', '0.550000000000000000', '0.550000000000000000', '0.550000000000000000', '0.700000000000000000']);
  for (const frame of frames) {
    assert.equal(weights(frame).reduce<bigint>((sum, weight) => sum + units(weight), units(frame.cash_weight)), 10n ** 18n);
    assert.equal(BigInt(String(frame.valid_until_ns)), BigInt(String(frame.asof_ns)) + 5n);
  }
  assert.deepEqual(forecasts, before);
});

test('cross-market decimal products round only allocation dust to cash, never multiply the total capital', () => {
  const exact = [{ instrumentId: 'A.SIM', weight: '0.3333333333333333333' }, { instrumentId: 'B-YES.SIM', weight: '0.6666666666666666667' }];
  const policy = resolveStrategyPolicy({ allocations: exact }, { ...original, investedWeight: '0.999999999999999999' });
  const frames = thresholdTargets({ points: [point('A.SIM', '10', 1), point('B-YES.SIM', '10', 1)] }, policy, 'pUSD');
  assert.deepEqual(weights(frames[0]!), ['0.333333333333333332', '0.666666666666666666']);
  assert.equal(frames[0]!.cash_weight, '0.000000000000000002');
  assert.equal(policy.simulationSettings.starting_capital, '3000');
});

test('cross-market input refuses unsupported choices, duplicate clocks and noncausal or nonfinite scores', () => {
  for (const bad of [[], [allocations[0]], [allocations[0], allocations[0]], [{ ...allocations[0], weight: '0.2' }, allocations[1]], [{ ...allocations[0], instrumentId: 'OUTSIDE.SIM' }, allocations[1]]]) {
    assert.throws(() => resolveStrategyPolicy({ allocations: bad }, original), /strategy_market_|portfolio_weights_invalid/);
  }
  assert.throws(() => resolveStrategyPolicy({ allocations, instrumentId: 'A.SIM' }, original), /strategy_market_/);
  assert.throws(() => resolveStrategyPolicy({}, original), /strategy_market_/);
  assert.throws(() => thresholdPolicy({ ...original, instrumentIds: ['A.SIM'] }), /strategy_instrument_universe_invalid/);
  const policy = resolveStrategyPolicy({ allocations }, original);
  assert.throws(() => thresholdTargets({ points: [point('A.SIM', '10', 1), point('A.SIM', '10', -1)] }, policy, 'pUSD'), /strategy_market_duplicate_clock/);
  assert.throws(() => thresholdTargets({ points: [{ ...point('A.SIM', '10', 1), event_ns: '11' }] }, policy, 'pUSD'), /strategy_clock_invalid/);
  for (const forecast of [NaN, Infinity, -Infinity]) assert.throws(() => thresholdTargets({ points: [point('A.SIM', '10', forecast)] }, policy, 'pUSD'), /strategy_forecast_nonfinite/);
});

test('legacy single-market target wire remains unchanged and cannot silently accept market allocations', () => {
  const legacy: JsonObject = { name: 'legacy', kind: 'factor-threshold', instrumentId: 'A.SIM', threshold: 0, investedWeight: '0.75', targetTtlNs: '5', simulationSettings: { base_currency: 'pUSD' } };
  const policy = thresholdPolicy(legacy);
  assert.deepEqual(thresholdTargets({ points: [point('A.SIM', '10', 1)] }, policy, 'pUSD'), [{ schema_version: 1, asof_ns: '10', valid_until_ns: '15',
    targets: [{ instrument_id: 'A.SIM', currency: 'pUSD', weight: '0.75' }], cash_weight: '0.250000000000000000' }]);
  assert.throws(() => resolveStrategyPolicy({ allocations }, legacy), /strategy_market_/);
});
