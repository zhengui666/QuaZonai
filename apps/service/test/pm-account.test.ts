import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { captureAccount } from '../src/pm-account.ts';
import { retirementAccountFlat, toNativeObservedSpotStart } from '../src/pm-account-native.ts';
import { accountKey } from '../src/capital-exit.ts';
import { acceptSnapshot } from '../src/downstream.ts';
import { json, nowNs } from '../src/facts.ts';
import { accountSourceFixture as fixture } from './pm-account-fixture.ts';
import type { PortfolioDecision } from '../../../packages/contracts/src/pm.ts';
import { validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';
import { projectAccountEvents } from '../src/pm-account-events.ts';


test('PM observed account: exact original balances and receipt clock survive projection without inferring cash or equity', () => {
  const f = fixture(); const capture = f.capture(); const initial = toNativeObservedSpotStart(capture, { selectionStartNs: '103' });
  assert.deepEqual(initial.snapshot, f.native); assert.equal(initial.available_ns, '102');
  assert.equal(initial.open_orders_available_ns, '102'); assert.equal(initial.open_orders_count, 0);
  assert.equal(initial.open_orders_asof_ns, '99'); assert.equal(initial.snapshot.ts_init, '100');
  assert.equal(capture.basis.sequence, '9007199254740995'); assert.equal(capture.basis.equity, null);
  assert.equal(initial.snapshot_evidence_ref, capture.evidence.id); assert.equal(initial.open_orders_evidence_ref, capture.evidence.id);
  const original = structuredClone(initial);
  f.native.account_id = 'changed-producer-object'; f.snapshot.positions = [{ instrumentId: 'OTHER', quantity: '999' }];
  assert.deepEqual(initial, original, 'The captured native source cannot change through a producer object alias.');
});

test('PM observed account: current and legacy snapshots cannot be backdated into an earlier validation interval', () => {
  const f = fixture(); assert.throws(() => toNativeObservedSpotStart(f.capture(), { selectionStartNs: '101' }), /snapshot_not_available_before_replay/);
  delete f.application.observations[f.connection.id]!.snapshotReceivedAtNs;
  const legacy = f.capture(); assert.equal(legacy.basis.availableAtNs, '110');
  const again = captureAccount(f.application, f.connection.id, '120');
  assert.equal(again.evidence.sourceRevision, legacy.evidence.sourceRevision); assert.deepEqual(again.evidence, legacy.evidence);
  assert.throws(() => toNativeObservedSpotStart(legacy, { selectionStartNs: '109' }), /snapshot_not_available_before_replay/);
  const missing = json(legacy); delete (missing.artifact as JsonObject).snapshotReceivedAtNs;
  assert.throws(() => toNativeObservedSpotStart(missing as unknown as typeof legacy, { selectionStartNs: '111' }), /snapshot_received_clock_counter_required/);
});

test('PM observed account: incomplete source identity, pending-order unknowns and non-cash states fail closed', () => {
  const cases: [string, (snapshot: JsonObject) => void, RegExp][] = [
    ['legacy normalized only', snapshot => { delete snapshot.nativeObservation; }, /native_account_observation_required/],
    ['no open orders source', snapshot => { delete snapshot.openOrders; }, /open_orders_report_required/],
    ['missing explicit count', snapshot => { delete (snapshot.openOrders as JsonObject).count; }, /open_orders_shape_invalid/],
    ['outstanding order', snapshot => { (snapshot.openOrders as JsonObject).count = 1; }, /no_open_orders_proof_required/],
    ['other account orders', snapshot => { (snapshot.openOrders as JsonObject).account_id = 'OTHER'; }, /open_orders_basis_mismatch/],
    ['other valuation orders', snapshot => { (snapshot.openOrders as JsonObject).asof_ns = '100'; }, /open_orders_basis_mismatch/],
    ['unavailable orders', snapshot => { (snapshot.openOrders as JsonObject).available_ns = '103'; }, /open_orders_clock_invalid/],
    ['source lost events', snapshot => { (snapshot.nativeObservation as JsonObject).dropped_events = '1'; }, /native_observation_has_gap/],
    ['other sequence', snapshot => { (snapshot.nativeObservation as JsonObject).sequence = '9007199254740996'; }, /native_snapshot_identity_mismatch/],
    ['other environment', snapshot => { ((snapshot.nativeObservation as JsonObject).binding as JsonObject).environment = 'LIVE'; }, /native_environment_mismatch/],
    ['stale valuation', snapshot => { ((snapshot.nativeObservation as JsonObject).snapshot as JsonObject).is_stale = true; }, /native_state_not_complete/],
    ['margin account', snapshot => { ((snapshot.nativeObservation as JsonObject).snapshot as JsonObject).account_type = 'MARGIN'; }, /native_multi_currency_cash_required/],
    ['implicit single currency', snapshot => { ((snapshot.nativeObservation as JsonObject).snapshot as JsonObject).base_currency = 'USDC'; }, /native_multi_currency_cash_required/],
    ['same clock other native account', snapshot => { ((snapshot.nativeObservation as JsonObject).snapshot as JsonObject).account_id = 'OTHER'; }, /native_account_identity_mismatch/],
    ['producer heartbeat later than receipt', snapshot => { (snapshot.nativeObservation as JsonObject).observed_at_ns = '103'; }, /snapshot_not_available_before_replay/],
  ];
  for (const [name, change, expected] of cases) {
    const f = fixture(); change(f.application.observations[f.connection.id]!.snapshot as JsonObject);
    assert.throws(() => toNativeObservedSpotStart(f.capture(), { selectionStartNs: '110' }), expected, name);
  }
});

test('PM observed account: microscopic locked funds, currency collisions and capital reservations are not rounded away', () => {
  for (const mode of ['locked', 'free', 'currency', 'duplicate', 'reserved'] as const) {
    const f = fixture(); const raw = ((f.application.observations[f.connection.id]!.snapshot as JsonObject).nativeObservation as JsonObject).snapshot as JsonObject;
    const balances = raw.balances as JsonObject[]; const first = balances[0]!;
    if (mode === 'locked') (first.locked as JsonObject).amount = '0.000000000000000001';
    if (mode === 'free') (first.free as JsonObject).amount = '900';
    if (mode === 'currency') (first.free as JsonObject).currency = 'USD';
    if (mode === 'duplicate') balances.push(json(first));
    if (mode === 'reserved') f.application.accountControls[accountKey(f.connection)]!.status.reserved = '0.000000000000000001';
    assert.throws(() => toNativeObservedSpotStart(f.capture(), { selectionStartNs: '110' }), /locked_or_borrowed|currency_conflict|capital_reserved/, mode);
  }
});

test('PM observed account: later local reservations and releases cannot inherit an earlier snapshot availability', () => {
  const f = fixture(); const original = f.capture(); assert.equal(original.basis.availableAtNs, '102');
  const control = f.application.accountControls[accountKey(f.connection)] as unknown as JsonObject;
  control.commands = { pending: { state: 'PENDING', reserved: '1' } };
  const reserved = captureAccount(f.application, f.connection.id, '120');
  assert.equal(reserved.basis.availableAtNs, '120'); assert.equal(reserved.basis.reserved?.amount, '1.000000000000000000');
  control.commands = {};
  const released = captureAccount(f.application, f.connection.id, '130');
  assert.equal(released.basis.availableAtNs, '130'); assert.equal(released.artifact.snapshotReceivedAtNs, '102');
  assert.notEqual(released.evidence.id, original.evidence.id);
  assert.throws(() => toNativeObservedSpotStart(released, { selectionStartNs: '129' }), /snapshot_not_available_before_replay/);
  assert.equal(toNativeObservedSpotStart(released, { selectionStartNs: '130' }).available_ns, '130');
  assert.deepEqual(captureAccount(f.application, f.connection.id, '140').evidence, released.evidence);
});

test('PM observed account: first real transport receipt and complete native extensions survive identical snapshot replay', () => {
  const f = fixture(); const observed = { ...f.observation, snapshot: null, snapshotReceivedAtNs: undefined };
  acceptSnapshot(observed, f.snapshot); const received = observed.snapshotReceivedAtNs; assert.ok(received);
  acceptSnapshot(observed, json(f.snapshot)); assert.equal(observed.snapshotReceivedAtNs, received);
  assert.deepEqual(observed.snapshot, f.snapshot);
  f.application.observations[f.connection.id] = json(observed);
  const capture = captureAccount(f.application, f.connection.id, nowNs());
  assert.equal(capture.basis.availableAtNs, received); assert.equal(capture.evidence.availableAtNs, received);
  assert.equal(toNativeObservedSpotStart(capture, { selectionStartNs: received }).available_ns, received);
});

test('PM observed account: a later transport sequence cannot silently switch the original native account behind an alias', () => {
  const f = fixture(); const observed = { ...f.observation, snapshot: null, snapshotReceivedAtNs: undefined };
  acceptSnapshot(observed, f.snapshot);
  for (const field of ['project_id', 'environment', 'native_account_id'] as const) {
    const next = json(f.snapshot); next.sequence = '9007199254740996';
    const native = next.nativeObservation as JsonObject; native.sequence = next.sequence;
    (native.binding as JsonObject)[field] = field === 'environment' ? 'LIVE' : 'OTHER';
    if (field === 'native_account_id') (native.snapshot as JsonObject).account_id = 'OTHER';
    assert.throws(() => acceptSnapshot(observed, next), /account_identity_mismatch|native_account_identity_mismatch/, field);
    assert.deepEqual(observed.snapshot, f.snapshot, 'The original alias-to-native mapping remains authoritative.');
  }
});

test('PM retirement: an empty normalized positions list cannot erase native BTC balances or missing open-order evidence', () => {
  const f = fixture(); assert.equal(retirementAccountFlat(f.capture()).flat, false);
  const raw = ((f.application.observations[f.connection.id]!.snapshot as JsonObject).nativeObservation as JsonObject).snapshot as JsonObject;
  const btc = (raw.balances as JsonObject[])[1]!; (btc.total as JsonObject).amount = '0'; (btc.free as JsonObject).amount = '0';
  assert.equal(retirementAccountFlat(f.capture()).flat, true);
  delete (f.application.observations[f.connection.id]!.snapshot as JsonObject).openOrders;
  assert.equal(retirementAccountFlat(f.capture()).flat, null);
});

test('PM observed account: separately evidenced derived NAV can fund an exact plan without rewriting missing source equity', () => {
  const capture = fixture().capture();
  const money = (amount: string) => ({ amount, currency: 'USDC' });
  const basis = { ...capture.basis, valuation: { origin: 'DERIVED' as const, reportValue: money('1000.000000000000000001'),
    valuationEvidenceRef: 'evidence:original-inventory-marks', methodRef: 'method:native-observed-spot-value', asOfNs: '99', availableAtNs: '111' } };
  const decision: PortfolioDecision = { schemaVersion: 1, id: 'decision:derived', revision: '0', supersedesId: null, createdAtNs: '112',
    producerVersion: 'protocol-test', cycleId: 'cycle:derived', mandateRevisionId: 'mandate:original', strategyVersionId: 'strategy:original',
    ensembleRevisionId: null, policyRevisionId: 'policy:original', basisRevision: 'basis:original', incumbentDecisionId: null,
    accountBasis: basis, researchStartingStateRef: null, action: 'HOLD', status: 'CANDIDATE', strategyAllocations: [], assetTargets: [],
    capital: { equity: money('1000.000000000000000001'), reserved: money('0'), ownerLiquidityReserve: money('0'),
      deployableBudget: money('1000.000000000000000001'), committedCapital: money('0'), uncommittedCash: money('1000.000000000000000001'),
      projectedTradingCash: money('0'), capacityEstimateIds: [], sizingEvidenceRef: 'sizing:original' }, comparisonRef: 'comparison:original',
    solverEvidenceRef: null, jointReplayEvidenceRef: null, reasonCodes: [], explanationFacts: {}, validUntilNs: null,
    nextInvalidationDependencies: [], targetCommandId: null };
  const validated = validatePmEntity('PortfolioDecision', decision);
  assert.equal(validated.accountBasis!.equity, null); assert.deepEqual(capture.basis.equity, null);
  assert.deepEqual(validated.accountBasis!.valuation, basis.valuation);
  assert.throws(() => validatePmEntity('PortfolioDecision', { ...decision, accountBasis: capture.basis }), /pm_account_money_unknown/);
  for (const change of [
    { reportValue: { ...basis.valuation.reportValue, currency: 'USD' } },
    { reportValue: money('1000') },
    { reportValue: money('-1') },
    { asOfNs: '100' },
    { availableAtNs: '101' },
    { valuationEvidenceRef: '' },
    { methodRef: '' },
  ]) assert.throws(() => validatePmEntity('PortfolioDecision', { ...decision,
    accountBasis: { ...basis, valuation: { ...basis.valuation, ...change } } }));
});

function eventsFixture() {
  const f = fixture();
  const envelope: JsonObject = { schemaVersion: 1, accountSourceId: 'native-source', nativeAccountId: 'VENUE-001',
    interval: { startNs: '80', endExclusiveNs: '100' }, availableAtNs: '101', complete: { cashFlows: true, executions: true },
    cashFlows: [{ id: 'cashflow:1', eventAtNs: '90', availableAtNs: '91', currency: 'USDC', amount: '0.000000000000000001', kind: 'DEPOSIT' }],
    executions: [{ id: 'fill:original', eventAtNs: '92', availableAtNs: '94', instrumentId: 'BTCUSDC.VENUE', orderId: 'order:original',
      clientTargetId: null, side: 'BUY', quantity: '0.125000000000000001', price: '123.456789000000000001', currency: 'USDC', fee: null }] };
  (f.application.observations[f.connection.id]!.snapshot as JsonObject).accountEvents = envelope;
  return { ...f, envelope };
}

test('PM account events: unavailable ledgers are distinct from source-confirmed empty intervals and partial evidence', () => {
  assert.equal(projectAccountEvents(fixture().capture()), null);
  const f = eventsFixture(); f.envelope.cashFlows = []; f.envelope.executions = [];
  const empty = projectAccountEvents(f.capture())!;
  assert.deepEqual(empty.complete, { cashFlows: true, executions: true }); assert.deepEqual(empty.cashFlows, []);
  (f.envelope.complete as JsonObject).cashFlows = false;
  assert.equal(projectAccountEvents(f.capture())!.complete.cashFlows, false);
  delete (f.envelope.complete as JsonObject).executions;
  assert.throws(() => projectAccountEvents(f.capture()), /complete_shape_invalid/);
});

test('PM account events: original decimals, unknown fees, source publication and actual receiver time remain separate', () => {
  const f = eventsFixture(); const capture = f.capture(); const result = projectAccountEvents(capture)!;
  assert.equal(result.cashFlows[0]!.amount, '0.000000000000000001');
  assert.equal(result.executions[0]!.quantity, '0.125000000000000001'); assert.equal(result.executions[0]!.fee, null);
  assert.equal(result.sourceAvailableAtNs, '101'); assert.equal(result.receivedAtNs, '102'); assert.equal(result.evidence.availableAtNs, '102');
  assert.equal(result.executions[0]!.availableAtNs, '94'); assert.equal(result.evidence.visibility, 'OWNER_ONLY');
  assert.equal(result.artifact.accountEvidenceRef, capture.evidence.id);
  const original = structuredClone(result); (f.envelope.cashFlows as JsonObject[])[0]!.amount = '999';
  assert.deepEqual(result, original, 'A producer alias cannot rewrite captured event evidence.');
});

test('PM account events: retries do not double-count original IDs and changed payloads cannot reuse an event identity', () => {
  const f = eventsFixture(); const flow = (f.envelope.cashFlows as JsonObject[])[0]!;
  (f.envelope.cashFlows as JsonObject[]).push(json(flow));
  const original = projectAccountEvents(f.capture())!; assert.equal(original.cashFlows.length, 1); assert.equal(original.identities.length, 2);
  assert.equal(original.newEventKeys.length, 2);
  const replay = projectAccountEvents(f.capture(), original.identities)!; assert.equal(replay.newEventKeys.length, 0);
  (f.envelope.cashFlows as JsonObject[])[1]!.amount = '1';
  assert.throws(() => projectAccountEvents(f.capture()), /event_identity_conflict/);
  f.envelope.cashFlows = [{ ...flow, amount: '2' }];
  assert.throws(() => projectAccountEvents(f.capture(), original.identities), /event_identity_conflict/);
});

test('PM account events: wrong account, post-receipt facts and unproven time coverage cannot enter observed attribution', () => {
  const cases: ((envelope: JsonObject) => void)[] = [
    envelope => { envelope.accountSourceId = 'other'; }, envelope => { envelope.nativeAccountId = 'OTHER'; },
    envelope => { envelope.availableAtNs = '103'; }, envelope => { (envelope.interval as JsonObject).endExclusiveNs = '101'; },
    envelope => { (envelope.cashFlows as JsonObject[])[0]!.eventAtNs = '100'; },
    envelope => { (envelope.cashFlows as JsonObject[])[0]!.availableAtNs = '89'; },
    envelope => { (envelope.cashFlows as JsonObject[])[0]!.availableAtNs = '102'; },
    envelope => { (envelope.cashFlows as JsonObject[])[0]!.amount = '-1'; },
    envelope => { (envelope.executions as JsonObject[])[0]!.quantity = '0'; },
  ];
  for (const change of cases) { const f = eventsFixture(); change(f.envelope); assert.throws(() => projectAccountEvents(f.capture())); }
});
