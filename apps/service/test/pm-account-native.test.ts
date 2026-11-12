import assert from 'node:assert/strict';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { compareObservedSpotAccount, valueObservedSpotAccount } from '../src/pm-account-native.ts';
import { object } from '../src/facts.ts';
import { observedSpotFixture } from './pm-observed-fixture.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';

const fixtureDirectory = process.env.QZ_PM_OBSERVED_FIXTURE_DIR;
const binary = process.env.QZ_JOB_BINARY;
const nativeOptions = { timeout: 120_000,
  skip: !fixtureDirectory || !binary ? 'Requires the original exported observed-spot catalog and matching job with hyperliquid-offline.' : false };


test('PM observed native: exact source inventory is valued using original as-of marks and retains its derived origin', nativeOptions, async () => {
  const f = await observedSpotFixture();
  try {
    const result = await valueObservedSpotAccount({ ...f.common, selection: object(f.source.valuationSelection), settings: f.settings }, BACKGROUND_CONTEXT);
    assert.equal(result.asOfNs, f.native.ts_event); assert.ok(BigInt(result.availableAtNs) > BigInt(result.asOfNs));
    assert.equal(result.original.basis, 'DERIVED_REPORT_CURRENCY_INVENTORY_VALUE');
    assert.equal(result.reportValue.currency, f.settings.base_currency); assert.equal(f.common.capture.basis.equity, null);
    assert.deepEqual(result.datasetBinding, { datasetRevisionId: f.common.datasetRevision.id,
      datasetContentRef: f.common.datasetRevision.contentRef, nativeDatasetRevisionId: f.common.nativeDatasetRevisionId });
    assert.deepEqual(object(object(result.request.initial_state).snapshot).balances, f.native.balances);
    for (const item of result.original.legs as JsonObject[]) {
      if (item.price_available_ns !== null) assert.ok(BigInt(String(item.price_available_ns)) <= BigInt(result.asOfNs));
    }
    const wrong = { ...object(f.source.valuationSelection), decision_cutoff_ns: (BigInt(result.asOfNs) + 1n).toString() };
    await assert.rejects(valueObservedSpotAccount({ ...f.common, selection: wrong, settings: f.settings }, BACKGROUND_CONTEXT), /valuation_price_cutoff_mismatch/);
  } finally { await f.cleanup(); }
});

test('PM observed native: HOLD preserves original inventory with zero active orders while real rebalance pays its original costs', nativeOptions, async () => {
  const f = await observedSpotFixture();
  try {
    const simulation = { ...structuredClone(f.simulation), settlements: [] };
    const originalSimulation = structuredClone(simulation);
    const result = await compareObservedSpotAccount({ ...f.common, operation: { kind: 'SIMULATION', simulation } }, BACKGROUND_CONTEXT);
    const { settlements: _legacy, ...wireSimulation } = originalSimulation;
    assert.deepEqual(result.request.simulation, wireSimulation);
    assert.equal(Object.hasOwn(object(result.request.simulation), 'settlements'), false);
    assert.deepEqual(simulation.settlements, []);
    assert.deepEqual(simulation, originalSimulation, 'Native request preparation retains the caller\'s original evidence.');
    assert.equal(result.hold.status, 'COMPLETED', result.hold.reason ?? ''); assert.equal(result.rebalance.status, 'COMPLETED', result.rebalance.reason ?? '');
    const hold = result.hold.result!, rebalanced = result.rebalance.result!;
    assert.equal(hold.orders, '0'); assert.equal(hold.consumed_target_points, '0'); assert.ok(BigInt(String(rebalanced.orders)) > 0n);
    const account = (value: JsonObject) => object(object((object(value.canonical_result).accounts as JsonObject[])[0]!.Cash).base);
    assert.deepEqual(account(hold).balances_starting, account(rebalanced).balances_starting);
    const holdReport = object(hold.spot_cash_report), rebalanceReport = object(rebalanced.spot_cash_report);
    assert.deepEqual(object(holdReport.flow_evidence).period_start_ns, object(rebalanceReport.flow_evidence).period_start_ns);
    assert.deepEqual(object(holdReport.flow_evidence).period_end_ns, object(rebalanceReport.flow_evidence).period_end_ns);
    assert.notEqual(JSON.stringify(account(rebalanced).commissions), JSON.stringify(account(hold).commissions));
    assert.deepEqual(object(result.request.initial_state).snapshot, f.native);
  } finally { await f.cleanup(); }
});

test('PM observed native: original solver frames use actual current inventory and an infeasible study retains both diagnostics and HOLD', nativeOptions, async () => {
  const f = await observedSpotFixture();
  try {
    const objectsDirectory = String(f.source.objectsDirectory);
    assert.ok(f.source.objectsDirectory && f.source.study && f.source.infeasibleStudy, 'Export the actual native study, objects and proven infeasible variant.');
    const study = { ...structuredClone(object(f.source.study)), settlements: [] };
    const originalStudy = structuredClone(study);
    const result = await compareObservedSpotAccount({ ...f.common, operation: { kind: 'STUDY', study, objectsDirectory } }, BACKGROUND_CONTEXT);
    const { settlements: _legacy, ...wireStudy } = originalStudy;
    assert.deepEqual(result.request.study, wireStudy);
    assert.equal(Object.hasOwn(object(result.request.study), 'settlements'), false);
    assert.deepEqual(study.settlements, []);
    assert.deepEqual(study, originalStudy);
    assert.equal(result.hold.status, 'COMPLETED', result.hold.reason ?? ''); assert.equal(result.rebalance.status, 'COMPLETED', result.rebalance.reason ?? '');
    const frames = result.rebalance.result!.frames as JsonObject[]; assert.ok(frames.length >= 2);
    const first = object(frames[0]!.input); const firstAsset = (first.assets as JsonObject[])[0]!;
    assert.ok(compareDecimal(String(firstAsset.current_weight), '0') > 0); assert.notEqual(first.current_cash_weight, '1');
    const failed = await compareObservedSpotAccount({ ...f.common, operation: { kind: 'STUDY', study: object(f.source.infeasibleStudy), objectsDirectory } }, BACKGROUND_CONTEXT);
    assert.equal(failed.hold.status, 'COMPLETED'); assert.equal(failed.rebalance.status, 'FAILED');
    for (const frame of failed.rebalance.result!.frames as JsonObject[]) if (object(frame.allocation).solver_status === 'INFEASIBLE') assertExactFundingInfeasibility(frame);
    assert.ok(failed.rebalance.reason); assert.ok(Array.isArray(failed.rebalance.result?.frames) && failed.rebalance.result.frames.length > 0,
      'Original infeasible solver evidence cannot be discarded or changed into a successful replay.');
  } finally { await f.cleanup(); }
});

function assertExactFundingInfeasibility(frame: JsonObject) {
  // Independent exact arithmetic for this single-asset native fixture. This
  // checks the original QP's funding row, without using its status as proof or
  // treating output exposure tolerance as permission to finance the shortfall.
  const exact = (value: unknown) => {
    assert.equal(typeof value, 'string'); assert.match(value as string, /^\d+(?:\.\d+)?$/);
    const [whole, fraction = ''] = (value as string).split('.');
    return { n: BigInt(`${whole}${fraction}`), d: 10n ** BigInt(fraction.length) };
  };
  const less = (a: ReturnType<typeof exact>, b: ReturnType<typeof exact>) => a.n * b.d < b.n * a.d;
  const input = object(frame.input), constraints = object(input.constraints), allocation = object(frame.allocation);
  assert.equal(allocation.solver_status, 'INFEASIBLE'); assert.equal(allocation.reason_code, 'NATIVE_PRIMAL_INFEASIBLE');
  assert.equal(allocation.targets, null); assert.equal(allocation.cash_weight, null); assert.equal(allocation.objective_value, null);
  assert.equal(constraints.cash_funding, 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1');
  assert.equal(constraints.long_only, true); assert.equal(constraints.min_asset_weight, '0');
  assert.deepEqual(constraints.asset_overrides, []);
  const assets = input.assets as JsonObject[]; assert.equal(assets.length, 1);
  const current = exact(assets[0]!.current_weight), cost = exact(assets[0]!.transaction_cost_rate);
  const maximum = exact(constraints.max_asset_weight), floor = exact(constraints.min_cash_weight);
  assert.ok(cost.n > 0n && cost.n < cost.d); assert.ok(less(maximum, current));
  assert.ok(floor.n >= 0n && floor.n <= floor.d);
  // With 0 <= w <= cap < current and cost < 1, w + cost*(current-w)
  // is minimized at w=0. Even this deployed-plus-fee minimum exceeds
  // 1-minCash, so no point can satisfy sum(w)+cash=1 and cash-fee>=minCash.
  const minimumDeployedPlusFee = { n: cost.n * current.n, d: cost.d * current.d };
  const fundingLimit = { n: floor.d - floor.n, d: floor.d };
  assert.ok(less(fundingLimit, minimumDeployedPlusFee), 'The original registered QP must be independently proven funding-infeasible.');
}
