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
    const simulation = { ...f.simulation, settlements: [] };
    const result = await compareObservedSpotAccount({ ...f.common, operation: { kind: 'SIMULATION', simulation } }, BACKGROUND_CONTEXT);
    assert.equal(Object.hasOwn(object(result.request.simulation), 'settlements'), false, 'Empty native settlements serialize as omission.');
    assert.deepEqual(simulation.settlements, [], 'Wire normalization cannot mutate the caller\'s original evidence.');
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
    const study = { ...object(f.source.study), settlements: [] };
    const result = await compareObservedSpotAccount({ ...f.common, operation: { kind: 'STUDY', study, objectsDirectory } }, BACKGROUND_CONTEXT);
    assert.equal(Object.hasOwn(object(result.request.study), 'settlements'), false);
    assert.deepEqual(study.settlements, []);
    assert.equal(result.hold.status, 'COMPLETED', result.hold.reason ?? ''); assert.equal(result.rebalance.status, 'COMPLETED', result.rebalance.reason ?? '');
    const frames = result.rebalance.result!.frames as JsonObject[]; assert.ok(frames.length >= 2);
    const first = object(frames[0]!.input); const firstAsset = (first.assets as JsonObject[])[0]!;
    assert.ok(compareDecimal(String(firstAsset.current_weight), '0') > 0); assert.notEqual(first.current_cash_weight, '1');
    const failed = await compareObservedSpotAccount({ ...f.common, operation: { kind: 'STUDY', study: object(f.source.infeasibleStudy), objectsDirectory } }, BACKGROUND_CONTEXT);
    assert.equal(failed.hold.status, 'COMPLETED'); assert.equal(failed.rebalance.status, 'FAILED');
    assert.ok(failed.rebalance.reason); assert.ok(Array.isArray(failed.rebalance.result?.frames) && failed.rebalance.result.frames.length > 0,
      'Original infeasible solver evidence cannot be discarded or changed into a successful replay.');
  } finally { await f.cleanup(); }
});
