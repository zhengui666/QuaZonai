import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { CapitalExitStatus, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { ObservationFact } from '../src/facts.ts';
import { decimalUnits } from '../src/facts.ts';
import { accountKey } from '../src/capital-exit.ts';
import { capitalExitTargetControl, managedTradingBudget, unreservedStrategyCapital } from '../src/pm-account.ts';
import { digest } from '../src/pm-facts.ts';
import type { PmRecord } from '../src/pm-facts.ts';
import { pmTargetAllocation, pmTargetPayload } from '../src/pm-target.ts';
import { toNativeObservedSpotStart } from '../src/pm-account-native.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';

// These tests check control and target transport semantics. They do not claim
// native fills or prove a venue balance from a fabricated financial report.
const signal: JsonObject = { schema_version: 1, asof_ns: '1', valid_until_ns: '999999999999999999',
  targets: [{ instrument_id: 'BTC/USDC', currency: 'USDC', weight: '0.5' }], cash_weight: '0.5' };
const capabilities: JsonObject = { clientId: 'owner', accountId: 'account', mode: 'paper', currency: 'USDC',
  capitalExit: { version: 2, supported: true, readable: true, policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'], actions: [] },
  automaticTargets: { supported: true, revisionBound: true, snapshotBound: true } };
function status(kind: 'NATURAL_RELEASE' | 'BOUNDED_LIMIT' | 'CASH_ONLY', reserve = '700'): CapitalExitStatus {
  return { capability: 'SUPPORTED', reason: null, accountSourceId: 'source', observationId: 'observation', controlRevision: '2', reserved: reserve,
    exits: [{ id: 'exit', state: 'WAITING_CASH', remaining_trading: 'FENCED_PENDING_TARGET', policy: { kind } } as CapitalExitView] };
}

test('external staging does not deduct managed capital twice and release restores only the managed reservation', () => {
  assert.equal(managedTradingBudget('1000', '300', '50'), '650.000000000000000000');
  // The user moves 200 out of spot: observed spot equity and the spot reserve
  // both fall by 200. The 200 staged outside this account is not deducted again.
  assert.equal(managedTradingBudget('800', '100', '50'), '650.000000000000000000');
  // Ending the remaining obligation does not bring staged funds back to spot.
  assert.equal(managedTradingBudget('800', '0', '50'), '750.000000000000000000');
  assert.equal(managedTradingBudget('0.000000000000000009', '0.000000000000000003', '0.000000000000000001'), '0.000000000000000005');
  assert.equal(managedTradingBudget('5', '6', '0'), '0.000000000000000000');
  assert.equal(unreservedStrategyCapital('1000', '50', '600'), '600.000000000000000000');
  assert.equal(unreservedStrategyCapital('500', '50', '600'), '450.000000000000000000');
});

test('every active preparation mode retains the actual strategy signal and its unreserved denominator', () => {
  const original = structuredClone(signal);
  const control = capitalExitTargetControl(status('NATURAL_RELEASE'), capabilities, signal, '1000')!;
  assert.deepEqual(control, { mode: 'NATURAL_RELEASE', transition: true, controlRevision: '2', exitIds: ['exit'], signalTarget: original, signalCapital: '1000' });
  assert.deepEqual(signal, original, 'Capital control cannot rewrite native weights to manufacture a fresh signal.');
  const unsupported = structuredClone(capabilities); delete (unsupported.automaticTargets as JsonObject).snapshotBound;
  assert.throws(() => capitalExitTargetControl(status('NATURAL_RELEASE'), unsupported, signal, '1000'), /budget_fence_required/);
  for (const kind of ['NATURAL_RELEASE', 'BOUNDED_LIMIT', 'CASH_ONLY'] as const) for (const state of ['WAITING_CASH', 'PAUSED', 'CANCELLED_RESERVED'] as const) {
    const current = status(kind); current.exits[0]!.state = state;
    const active = capitalExitTargetControl(current, capabilities, signal, '1000')!;
    assert.equal(active.transition, true); assert.deepEqual(active.signalTarget, original); assert.equal(active.signalCapital, '1000');
    assert.equal(decimalUnits(String(active.signalCapital)) * decimalUnits('0.5') / (10n ** 18n), decimalUnits('500'),
      'The ordinary strategy notional remains 500 while the independent buy budget can be only 300.');
  }
});

test('PM target transport caps actual reserved funds without rewriting its original current native target', () => {
  const connection = { id: 'binding', name: 'Account', url: 'http://127.0.0.1/', clientId: 'owner', accountId: 'account', mode: 'paper' as const };
  const account = { accountBindingId: 'binding', accountSourceId: 'source', observationId: 'observation', sequence: '1', observedAtNs: '123',
    controlRevision: '2', currency: 'USDC', equity: { amount: '1000', currency: 'USDC' }, reserved: { amount: '700', currency: 'USDC' },
    positionsEvidenceRef: 'positions', outstandingCommandsRef: 'commands' };
  const decision = { id: 'decision', basisRevision: 'basis', policyRevisionId: 'policy', validUntilNs: signal.valid_until_ns,
    accountBasis: account, explanationFacts: { nativeTargetEvidenceRef: 'target-evidence' },
    assetTargets: [{ instrumentId: 'BTC/USDC', targetNotional: { amount: '500', currency: 'USDC' }, navWeight: '0.5' }],
    capital: { equity: { amount: '1000', currency: 'USDC' }, deployableBudget: { amount: '1000', currency: 'USDC' },
      committedCapital: { amount: '500', currency: 'USDC' }, ownerLiquidityReserve: { amount: '0', currency: 'USDC' } } } as unknown as Pm.PortfolioDecision;
  const record = { evidence: { 'target-evidence': { sourceKind: 'NATIVE_RESULT', contentDigest: digest(signal), sourceRevision: 'basis', objectKey: 'original-target', availableAtNs: '1' } },
    artifacts: { 'original-target': signal } } as unknown as PmRecord;
  const source = { connection, snapshot: { sequence: '1', nativeClockNs: '123', accountSourceId: 'source', observationId: 'observation', controlRevision: '2', currency: 'USDC' } } as unknown as ObservationFact;
  const controls = { [accountKey(connection)]: { commands: {}, status: status('NATURAL_RELEASE'), capabilities } as unknown as JsonObject };
  const payload = pmTargetPayload(record, decision);
  const allocation = pmTargetAllocation(record, decision, source, controls);
  assert.equal(allocation.reserved, '700'); assert.equal(allocation.budget, '300.000000000000000000'); assert.equal(allocation.capital, '300.000000000000000000');
  assert.deepEqual((allocation.capitalExit as JsonObject).signalTarget, payload);
  assert.equal((allocation.capitalExit as JsonObject).signalCapital, '1000', 'PM NAV weights must retain the original NAV denominator, not committed capital.');
  for (const kind of ['BOUNDED_LIMIT', 'CASH_ONLY'] as const) {
    (controls[accountKey(connection)]!.status as unknown as CapitalExitStatus).exits[0]!.policy = { kind } as CapitalExitView['policy'];
    const active = pmTargetAllocation(record, decision, source, controls);
    assert.deepEqual((active.capitalExit as JsonObject).signalTarget, payload);
    assert.equal((active.capitalExit as JsonObject).signalCapital, '1000');
    assert.equal(active.capital, '300.000000000000000000');
    assert.equal(decimalUnits(String((active.capitalExit as JsonObject).signalCapital)) * decimalUnits('0.5') / (10n ** 18n), decimalUnits('500'));
  }
  assert.deepEqual(pmTargetPayload(record, decision), payload);
  (controls[accountKey(connection)]!.commands as JsonObject).pending = { state: 'UNCERTAIN', reserved: '10' };
  assert.throws(() => pmTargetAllocation(record, decision, source, controls), /requires_reconciliation/);
});

test('a managed reservation does not rewrite original native balances or erase open-order limitations in scientific input', () => {
  const source = accountSourceFixture();
  source.application.accountControls[accountKey(source.connection)]!.status.reserved = '100';
  const capture = source.capture();
  const original = structuredClone(source.native);
  assert.throws(() => toNativeObservedSpotStart(capture, { selectionStartNs: '110' }), /capital_reserved_or_unknown/);
  const input = toNativeObservedSpotStart(capture, { selectionStartNs: '110', reservationMode: 'SEPARATE_TARGET_BUDGET' });
  assert.deepEqual(input.snapshot, original);
  assert.equal(capture.basis.reserved?.amount, '100.000000000000000000');
  (source.snapshot.openOrders as JsonObject).count = 1;
  source.application.observations[source.connection.id]!.snapshot = source.snapshot;
  assert.throws(() => toNativeObservedSpotStart(source.capture(), { selectionStartNs: '110', reservationMode: 'SEPARATE_TARGET_BUDGET' }), /no_open_orders_proof_required/);
});
