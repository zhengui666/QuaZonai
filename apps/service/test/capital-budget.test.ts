import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { CapitalExitStatus, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import { decimalUnits } from '../src/facts.ts';
import { accountKey } from '../src/capital-exit.ts';
import { capitalExitTargetControl } from '../src/pm-account.ts';
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
