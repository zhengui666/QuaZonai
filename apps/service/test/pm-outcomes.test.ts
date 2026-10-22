import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { PmRecord } from '../src/pm-facts.ts';
import { observePmOutcome } from '../src/pm-outcomes.ts';
import { captureAccount } from '../src/pm-account.ts';
import { accountKey } from '../src/capital-exit.ts';
import { object } from '../src/facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';

// Protocol source facts test the actual consumer, not account performance.
function fixture() {
  const f = accountSourceFixture(); const observed = f.application.observations[f.connection.id]!;
  const snapshot = object(observed.snapshot); snapshot.equity = '100';
  const nativeObservation = object(snapshot.nativeObservation), native = object(nativeObservation.snapshot);
  const balance = (amount: string, currency: string) => ({ total: { amount, currency }, free: { amount, currency }, locked: { amount: '0', currency } });
  native.balances = [balance('100', 'USDC'), balance('0', 'BTC')]; native.total_equity = [{ amount: '100', currency: 'USDC' }, { amount: '0', currency: 'BTC' }];
  const original = f.capture();
  snapshot.equity = '160'; snapshot.nativeClockNs = '199'; snapshot.sequence = '9007199254740996'; snapshot.observationId = 'observation:closing';
  nativeObservation.sequence = snapshot.sequence; nativeObservation.observed_at_ns = '201';
  native.ts_event = '199'; native.ts_init = '200'; native.balances = [balance('160', 'USDC'), balance('0', 'BTC')];
  native.total_equity = [{ amount: '160', currency: 'USDC' }, { amount: '0', currency: 'BTC' }];
  snapshot.openOrders = { schema_version: 1, account_id: 'VENUE-001', asof_ns: '199', available_ns: '201', count: 0 };
  observed.snapshotReceivedAtNs = '202';
  const control = f.application.accountControls[accountKey(f.connection)]!; control.status.observationId = 'observation:closing'; control.receivedAtNs = '201';
  const envelope: JsonObject = { schemaVersion: 1, accountSourceId: 'native-source', nativeAccountId: 'VENUE-001',
    interval: { startNs: '100', endExclusiveNs: '200' }, availableAtNs: '201', complete: { cashFlows: true, executions: true },
    cashFlows: [{ id: 'deposit:original', eventAtNs: '150', availableAtNs: '151', amount: '50', currency: 'USDC', kind: 'DEPOSIT' }], executions: [] };
  snapshot.accountEvents = envelope;
  // Only fields read by observePmOutcome; no fake PM creation or qualification.
  const record = { mandate: { id: 'mandate:protocol' }, evidence: { [original.evidence.id]: original.evidence },
    artifacts: { [original.evidence.objectKey]: original.artifact }, outcomes: {}, attributions: {}, gaps: {},
    decisions: { original: { id: 'decision:original', accountBasis: original.basis, targetCommandId: 'command:original', comparisonRef: 'comparison:original' } },
    comparisons: { 'comparison:original': { scope: { id: 'scope:original' } } },
    commands: { 'command:original': { id: 'command:original', idempotencyKey: 'target:original', state: 'ACKNOWLEDGED' } } } as unknown as PmRecord;
  const capture = () => captureAccount(f.application, f.connection.id, '210');
  return { ...f, original, snapshot, envelope, record, capture };
}
const adjusted = (record: PmRecord) => Object.values(record.outcomes).at(-1)!.realizedMetrics.find(metric => metric.definition.key === 'account.external_flow_adjusted_equity_change')!;

test('PM observed outcome: original 100 to 160 equity with complete deposit 50 yields account-only adjusted change 10', () => {
  const f = fixture(); observePmOutcome(f.record, f.capture());
  const outcome = Object.values(f.record.outcomes)[0]!; const metric = adjusted(f.record);
  assert.equal(metric.status, 'AVAILABLE', JSON.stringify({ metric, gaps: f.record.gaps })); assert.equal(metric.value, '10'); assert.equal(metric.currency, 'USDC');
  assert.equal(metric.role, 'OBSERVED'); assert.equal(metric.definition.direction, 'DIAGNOSTIC');
  assert.equal(outcome.receiptState, 'ACKNOWLEDGED'); assert.equal(outcome.reconciliationStatus, 'PARTIAL');
  assert.ok(outcome.reasonCodes.includes('account_currency_delta_not_strategy_pnl_or_time_weighted_return'));
  const normalizations = Object.values(f.record.artifacts).filter(item => item.method === 'qz.account-flow-adjusted-currency-delta/1');
  assert.equal(normalizations.length, 1); assert.equal(normalizations[0]!.externalNetCashFlow, '50'); assert.equal(normalizations[0]!.returnMethod, null);
  const executions = Object.values(f.record.artifacts).find(item => item.identification === 'REPORTED_TARGET_ID_ONLY_NOT_PNL_CAUSALITY')!;
  assert.deepEqual(executions.matchedExecutionIds, []); assert.deepEqual(executions.unmatchedExecutionIds, [], 'ACK cannot generate a fill.');
});

test('PM observed outcome: missing, partial, wrong-currency or incomplete-interval flows keep adjusted change unknown', () => {
  for (const mode of ['missing', 'partial', 'currency', 'coverage', 'equity'] as const) {
    const f = fixture();
    if (mode === 'missing') delete f.snapshot.accountEvents;
    if (mode === 'partial') object(f.envelope.complete).cashFlows = false;
    if (mode === 'currency') (f.envelope.cashFlows as JsonObject[])[0]!.currency = 'EUR';
    if (mode === 'coverage') object(f.envelope.interval).startNs = '101';
    if (mode === 'equity') f.snapshot.equity = null;
    observePmOutcome(f.record, f.capture()); const metric = adjusted(f.record);
    assert.equal(metric.status, 'UNAVAILABLE', mode); assert.equal(metric.value, null, mode); assert.ok(metric.reasonCodes.length);
  }
});

test('PM observed outcome: repeated polling keeps original outcome and ledger, while a changed original event ID records an integrity gap', () => {
  const f = fixture(); const originalCapture = f.capture(); observePmOutcome(f.record, originalCapture);
  const original = structuredClone(f.record); observePmOutcome(f.record, f.capture());
  assert.deepEqual(f.record, original);
  const ledger = object(f.record.artifacts[`account-event-ledger:${f.connection.id}`]); assert.equal((ledger.identities as JsonObject[]).length, 1);
  (f.envelope.cashFlows as JsonObject[])[0]!.amount = '51'; observePmOutcome(f.record, f.capture());
  assert.ok(Object.values(f.record.gaps).some(gap => gap.reasonCode.includes('event_identity_conflict')));
  assert.equal(adjusted(f.record).status, 'UNAVAILABLE'); assert.deepEqual(f.record.outcomes[Object.keys(original.outcomes)[0]!], Object.values(original.outcomes)[0]);
  assert.deepEqual(object(f.record.artifacts[`account-event-ledger:${f.connection.id}`]).identities, ledger.identities);
});

test('PM observed outcome: a different account source behind the same binding cannot supply the original account delta', () => {
  const f = fixture(); f.snapshot.accountSourceId = 'different-account-source'; f.envelope.accountSourceId = 'different-account-source';
  f.application.accountControls[accountKey(f.connection)]!.status.accountSourceId = 'different-account-source';
  observePmOutcome(f.record, f.capture());
  assert.equal(adjusted(f.record).status, 'UNAVAILABLE'); assert.equal(adjusted(f.record).value, null);
});

test('PM observed outcome: stale or unpriced original inventory cannot yield available reported or adjusted equity', () => {
  for (const opening of [false, true]) for (const flag of ['is_stale', 'stale_instruments', 'stale_currencies', 'unpriced_instruments']) {
    const f = fixture();
    const snapshot = opening ? object(f.record.artifacts[f.original.evidence.objectKey]!.snapshot) : f.snapshot;
    const native = object(object(snapshot.nativeObservation).snapshot); native[flag] = flag === 'is_stale' ? true : ['ORIGINAL-UNPRICED'];
    observePmOutcome(f.record, f.capture());
    const result = Object.values(f.record.outcomes)[0]!;
    assert.equal(adjusted(f.record).status, 'UNAVAILABLE', `${opening ? 'opening' : 'closing'} ${flag}`);
    if (!opening) assert.equal(result.realizedMetrics.find(metric => metric.definition.key === 'account.observed_equity')!.status, 'UNAVAILABLE');
    assert.ok(Object.values(f.record.evidence).some(item => item.schemaId === 'qz.account-events/1'), 'Valid original event evidence remains retained.');
  }
});
