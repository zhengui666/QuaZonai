import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { object } from '../src/facts.ts';
import { addDecimal, compareDecimal, multiplyDecimal, subtractDecimal } from '../../../packages/contracts/src/pm-validation.ts';

const fixtureDirectory = process.env.QZ_PM_FORWARD_FIXTURE_DIR;
const binary = process.env.QZ_JOB_BINARY;
const options = { timeout: 120_000, skip: !fixtureDirectory || !binary ? 'Requires actual native forward fixture and matching job binary.' : false };
const rows = (value: unknown): JsonObject[] => { assert.ok(Array.isArray(value)); return value.map(item => object(item)); };
const sameDecimal = (actual: unknown, expected: string) => assert.equal(compareDecimal(String(actual), expected), 0, `${actual} != ${expected}`);
const close = (actual: unknown, expected: number) => {
  assert.equal(typeof actual, 'number'); assert.ok(Number.isFinite(actual));
  assert.ok(Math.abs(Number(actual) - expected) <= 1e-13 * Math.max(1, Math.abs(expected)), `${actual} != scalar oracle ${expected}`);
};
async function run(source: JsonObject, request: JsonObject): Promise<JsonObject> {
  const scratch = await mkdtemp(join(tmpdir(), 'qz-forward-oracle-'));
  try { return await executeNative(binary!, 'build-observed-spot', request,
    ['--catalog', String(source.catalog), '--objects', String(source.objectsDirectory)], scratch, BACKGROUND_CONTEXT); }
  finally { await rm(scratch, { recursive: true, force: true }); }
}
let actual: Promise<{ source: JsonObject; request: JsonObject; result: JsonObject }> | undefined;
function fixture() {
  return actual ??= (async () => {
    const source = object(JSON.parse(await readFile(join(fixtureDirectory!, 'source.json'), 'utf8')));
    assert.equal(source.evidenceKind, 'CONTROLLED_NATIVE_FORWARD_MODEL_TEST_NOT_LIVE_QUALIFICATION');
    const request = object(source.request); return { source, request, result: await run(source, request) };
  })();
}

test('native forward: current marks revalue original observed quantities without creating a new snapshot or rolling TTL', options, async () => {
  const { request, result } = await fixture();
  assert.equal(result.kind, 'OBSERVED_SPOT_FORWARD_BUILD'); assert.equal(result.basis, 'FORWARD_MODEL_ESTIMATE_FROM_OBSERVED_SPOT_BALANCES');
  assert.deepEqual(result.request, request);
  const initial = object(request.initial_state), snapshot = object(initial.snapshot), cutoff = String(object(request.selection).decision_cutoff_ns);
  const valuation = object(result.valuation); assert.equal(valuation.asof_ns, cutoff);
  assert.ok(BigInt(String(snapshot.ts_event)) < BigInt(cutoff));
  assert.deepEqual(object(object(valuation.request).initial_state).snapshot, snapshot);
  assert.equal(snapshot.base_currency_equity, null, 'Raw aggregate equity cannot be overwritten by derived report NAV.');
  const balances = rows(snapshot.balances), legs = rows(valuation.legs); const prices = new Map<string, JsonObject>();
  let total = '0'; let sourceAvailable = [String(initial.available_ns), String(initial.open_orders_available_ns)].reduce((a, b) => BigInt(a) > BigInt(b) ? a : b);
  for (const leg of legs) {
    const originals = balances.filter(balance => object(balance.total).currency === leg.currency); assert.equal(originals.length, 1);
    sameDecimal(leg.native_total, String(object(originals[0]!.total).amount));
    const expected = leg.price === null ? String(leg.native_total) : multiplyDecimal(String(leg.native_total), String(leg.price));
    sameDecimal(leg.report_value, expected); total = addDecimal(total, expected);
    if (leg.price !== null) {
      assert.ok(BigInt(String(leg.price_event_ns)) <= BigInt(String(leg.price_available_ns)) && BigInt(String(leg.price_available_ns)) <= BigInt(cutoff));
      assert.ok(leg.source_row_key); prices.set(String(leg.instrument_id), leg);
      if (BigInt(String(leg.price_available_ns)) > BigInt(sourceAvailable)) sourceAvailable = String(leg.price_available_ns);
    }
  }
  sameDecimal(valuation.total, total); assert.equal(valuation.available_ns, sourceAvailable);
  // Exported original engine purchase at 100 retained 4.9965 BTC after fees;
  // current mark is 200. Reusing initial capital or the old price would fail.
  sameDecimal(legs.find(leg => leg.currency === 'BTC')!.price, '200'); sameDecimal(total, '1499.3');
  assert.equal(object(valuation.source_evidence).historical_availability, 'UNVERIFIED');
  const input = object(object(result.build).input); sameDecimal(input.capital_assumption, total);
  for (const asset of rows(input.assets)) close(Number(asset.current_weight), Number(prices.get(String(asset.instrument_id))!.report_value) / Number(total));
  const target = object(result.target); assert.equal(target.asof_ns, cutoff);
  assert.equal(target.valid_until_ns, (BigInt(cutoff) + BigInt(String(object(object(request.mandate).rebalance_schedule).target_ttl_seconds)) * 1_000_000_000n).toString());
  assert.equal(result.target_block_reason, null);
  let tradingCash = String(legs.find(leg => leg.currency === valuation.report_currency)!.report_value);
  let totalCost = '0', roundedInventory = '0';
  for (const trade of rows(result.trade_estimates)) {
    const price = prices.get(String(trade.instrument_id))!;
    sameDecimal(trade.current_quantity, String(price.native_total)); sameDecimal(trade.current_notional, String(price.report_value));
    sameDecimal(trade.price, String(price.price)); assert.equal(trade.source_row_key, price.source_row_key);
    sameDecimal(trade.net_trade_notional, subtractDecimal(String(trade.target_notional), String(trade.current_notional)));
    sameDecimal(trade.rounded_trade_notional, multiplyDecimal(String(trade.rounded_trade_quantity), String(trade.price)));
    sameDecimal(trade.post_rounding_quantity, addDecimal(String(trade.current_quantity), String(trade.rounded_trade_quantity)));
    sameDecimal(trade.post_rounding_notional, multiplyDecimal(String(trade.post_rounding_quantity), String(trade.price)));
    const asset = rows(input.assets).find(asset => asset.instrument_id === trade.instrument_id)!;
    const absoluteTrade = compareDecimal(String(trade.rounded_trade_notional), '0') < 0 ? multiplyDecimal(String(trade.rounded_trade_notional), '-1') : String(trade.rounded_trade_notional);
    sameDecimal(trade.modeled_linear_cost, multiplyDecimal(absoluteTrade, String(asset.transaction_cost_rate)));
    tradingCash = subtractDecimal(tradingCash, String(trade.rounded_trade_notional));
    totalCost = addDecimal(totalCost, String(trade.modeled_linear_cost));
    roundedInventory = addDecimal(roundedInventory, String(trade.post_rounding_notional));
  }
  tradingCash = subtractDecimal(tradingCash, totalCost);
  assert.ok(compareDecimal(tradingCash, '0') >= 0, 'Rounded trades plus original linear fees cannot borrow cash.');
  sameDecimal(addDecimal(tradingCash, roundedInventory), subtractDecimal(total, totalCost));
  assert.equal(Object.hasOwn(result, 'statistics'), false); assert.equal(Object.hasOwn(result, 'spot_cash_report'), false);
});

test('native forward: independent scalar forecast/covariance/cost oracle reproduces HOLD and proposal over the exact model horizon', options, async () => {
  const { result } = await fixture(); const build = object(result.build), input = object(build.input), forecast = object(input.forecasts);
  const assets = rows(input.assets), history = object(input.return_history), members = rows(forecast.members), comparison = object(result.modeled_comparison);
  assert.equal(comparison.method, 'MATCHED_LINEAR_RETURN_VARIANCE_V1'); assert.equal(comparison.forecast_unit, 'RETURN_PER_HORIZON');
  assert.equal(comparison.basis, 'FORWARD_MODEL_ESTIMATE'); assert.ok(Array.isArray(comparison.limitations) && comparison.limitations.length > 0);
  assert.equal(input.objective, 'MAX_UTILITY'); assert.equal(input.risk, 'VARIANCE');
  assert.equal(object(object(input.covariance_estimator).parameters).ddof, 1);
  assert.equal(history.horizon_kind, forecast.horizon_kind); assert.equal(history.horizon_value, forecast.horizon_value);
  const instrumentIds = assets.map(asset => String(asset.instrument_id)); assert.deepEqual(history.instrument_ids, instrumentIds);
  const returns = history.asset_returns as number[][]; assert.equal(returns.length, assets.length); const n = returns[0]!.length; assert.ok(n > 1);
  const means = returns.map(series => { assert.equal(series.length, n); return series.reduce((a, b) => a + b, 0) / n; });
  const covariance = returns.map((left, i) => returns.map((right, j) => left.reduce((sum, value, t) => sum + (value - means[i]!) * (right[t]! - means[j]!), 0) / (n - 1)));
  const forecasts = instrumentIds.map((id, i) => members.reduce((sum, member) => {
    assert.equal(member.forecast_unit, 'RETURN_PER_HORIZON'); assert.deepEqual(member.instrument_ids, instrumentIds);
    assert.equal(member.horizon_value, history.horizon_value); assert.ok(BigInt(String(member.available_ns)) <= BigInt(String(forecast.decision_asof_ns)));
    return sum + Number(member.ensemble_weight) * Number((member.forecasts as number[])[i]);
  }, 0));
  for (const clock of history.available_ns as string[]) assert.ok(BigInt(clock) <= BigInt(String(forecast.decision_asof_ns)));
  const current = assets.map(asset => Number(asset.current_weight));
  const lambda = Number(object(object(input.optimizer).parameters).risk_aversion);
  const check = (weights: number[], outcome: JsonObject) => {
    let gross = 0, cost = 0, variance = 0;
    weights.forEach((weight, i) => { gross += weight * forecasts[i]!; cost += Math.abs(weight - current[i]!) * Number(assets[i]!.transaction_cost_rate);
      weights.forEach((other, j) => { variance += weight * covariance[i]![j]! * other; }); });
    close(outcome.gross_expected_return, gross); close(outcome.linear_turnover_cost, cost); close(outcome.net_expected_return, gross - cost);
    close(outcome.variance, variance); close(outcome.objective_value, lambda * variance + cost - gross);
  };
  check(current, object(comparison.hold)); assert.equal(object(comparison.hold).linear_turnover_cost, 0);
  const targets = rows(object(build.allocation).targets); assert.deepEqual(targets.map(row => row.instrument_id), instrumentIds);
  check(targets.map(row => Number(row.weight)), object(comparison.proposed));
  assert.notEqual(object(comparison.proposed).objective_value, object(comparison.proposed).net_expected_return);
  const rounded = object(result.rounded_modeled_comparison);
  assert.equal(rounded.method, 'MATCHED_ROUNDED_LINEAR_RETURN_VARIANCE_V1');
  assert.equal(rounded.forecast_unit, 'RETURN_PER_HORIZON');
  check(current, object(rounded.hold));
  const trades = rows(result.trade_estimates);
  const roundedWeights = instrumentIds.map(id => Number(trades.find(trade => trade.instrument_id === id)!.post_rounding_notional) / Number(input.capital_assumption));
  check(roundedWeights, object(rounded.proposed));
  const exactCost = trades.reduce((sum, trade) => addDecimal(sum, String(trade.modeled_linear_cost)), '0');
  close(object(rounded.proposed).linear_turnover_cost, Number(exactCost) / Number(input.capital_assumption));
});

test('native forward: late original input rejects, and full deployment with positive trading cost keeps proposal but blocks target', options, async () => {
  const { source, request } = await fixture(); const late = structuredClone(request);
  object(late.initial_state).available_ns = (BigInt(String(object(request.selection).decision_cutoff_ns)) + 1n).toString();
  await assert.rejects(run(source, late), /invalid contract field: observed_spot_start/);
  const full = structuredClone(request); const constraints = object(object(full.mandate).constraints);
  Object.assign(constraints, { min_cash_weight: '0', max_cash_weight: '0', min_net_exposure: '1', max_net_exposure: '1' });
  const result = await run(source, full);
  assert.deepEqual(result.request, full); assert.equal(result.target, null); assert.equal(result.target_block_reason, 'FORWARD_LINEAR_COST_FUNDING_SHORTFALL');
  assert.equal(result.rounded_modeled_comparison, null, 'No executable proposal is created from the blocked continuous model.');
  assert.ok(object(result.modeled_comparison).proposed, 'Blocked execution retains its real model comparison.');
  assert.ok(rows(object(object(result.build).allocation).targets).length > 0, 'Original continuous solver proposal remains inspectable.');
  assert.ok(object(result.build).input);
});
