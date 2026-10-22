import assert from 'node:assert/strict';
import { test } from 'node:test';
import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { PmRecord } from '../src/pm-facts.ts';
import { observePmOutcome } from '../src/pm-outcomes.ts';
import { captureAccount } from '../src/pm-account.ts';
import { accountKey } from '../src/capital-exit.ts';
import { json, object } from '../src/facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

// Source/receipt protocol only. These controlled records grant no strategy,
// scientific, native-engine or execution qualification. The test persists the
// actual outcome consumer through official SQLite commits and service restart.
const Probe = defineDoc<JsonObject>({ kind: 'test.pm-outcome-heartbeats', version: 1, scope: 'session', initial: () => ({ value: {} }) });
function fixture() {
  const f = accountSourceFixture(); const original = f.capture();
  const observed = f.application.observations[f.connection.id]!;
  const snapshot = object(observed.snapshot); const wrapper = object(snapshot.nativeObservation); const native = object(wrapper.snapshot);
  const envelope: JsonObject = { schemaVersion: 1, accountSourceId: 'native-source', nativeAccountId: 'VENUE-001',
    interval: { startNs: '100', endExclusiveNs: '200' }, availableAtNs: '201', complete: { cashFlows: true, executions: true }, cashFlows: [], executions: [] };
  snapshot.accountEvents = envelope;
  const command = { id: 'command:original', idempotencyKey: 'target:original', state: 'ACKNOWLEDGED' };
  const record = { mandate: { id: 'mandate:protocol' }, evidence: { [original.evidence.id]: original.evidence },
    artifacts: { [original.evidence.objectKey]: original.artifact }, outcomes: {}, attributions: {}, gaps: {}, retirements: {},
    decisions: { original: { id: 'decision:original', accountBasis: original.basis, targetCommandId: command.id, comparisonRef: 'comparison:original', validUntilNs: '10000' } },
    comparisons: { 'comparison:original': { scope: { id: 'scope:original' } } }, commands: { [command.id]: command } } as unknown as PmRecord;
  const advance = (at: number) => {
    const sequence = (9007199254740995n + BigInt(at)).toString();
    snapshot.sequence = sequence; snapshot.nativeClockNs = String(at); snapshot.observationId = `observation:${at}`;
    wrapper.sequence = sequence; wrapper.observed_at_ns = String(at + 2);
    native.ts_event = String(at); native.ts_init = String(at + 1); native.event_id = `native-heartbeat:${at}`;
    object(snapshot.openOrders).asof_ns = String(at); object(snapshot.openOrders).available_ns = String(at + 2);
    observed.snapshotReceivedAtNs = String(at + 3);
    const control = f.application.accountControls[accountKey(f.connection)]!;
    control.status.observationId = String(snapshot.observationId); control.receivedAtNs = String(at + 2);
    object(envelope.interval).endExclusiveNs = String(at + 1); envelope.availableAtNs = String(at + 2);
    return captureAccount(f.application, f.connection.id, String(at + 4));
  };
  return { record, envelope, advance, bindingId: f.connection.id };
}

test('advancing heartbeat clocks and empty complete envelopes do not grow account evidence or historical research attributions', () => {
  const f = fixture();
  for (let index = 0; index < 100; index++) f.record.decisions[`research:${index}`] = {
    ...f.record.decisions.original!, id: `research:${index}`, targetCommandId: null,
  };
  observePmOutcome(f.record, f.advance(200)); const original = structuredClone(f.record);
  for (let at = 201; at <= 2200; at++) observePmOutcome(f.record, f.advance(at));
  assert.deepEqual(f.record, original, 'Two thousand new transport sequences must not become financial observations or multiply by 100 unoffered research candidates.');
  assert.equal(Object.keys(f.record.outcomes).length, 1);
  assert.ok(Object.values(f.record.outcomes).every(outcome => outcome.targetCommandId === 'command:original'));
});

test('original maturity and real event IDs remain observable after bounded polling and SQLite reopen', { timeout: 30_000 }, async () => {
  const f = fixture(); const state = await setup();
  try {
    await state.service.research.harness.commit(async tx => { const doc = await tx.doc(Probe); doc.value = json(f.record);
      observePmOutcome(doc.value as unknown as PmRecord, f.advance(200)); }, context);
    const first = (await state.service.research.harness.snapshot(Probe, context))!;
    await state.service.research.harness.commit(async tx => { const record = object((await tx.doc(Probe)).value) as unknown as PmRecord;
      for (let at = 201; at <= 700; at++) observePmOutcome(record, f.advance(at)); }, context);
    assert.deepEqual(await state.service.research.harness.snapshot(Probe, context), first);
    await state.restart();
    await state.service.research.harness.commit(async tx => { const record = object((await tx.doc(Probe)).value) as unknown as PmRecord;
      for (let at = 701; at <= 1200; at++) observePmOutcome(record, f.advance(at)); }, context);
    assert.deepEqual(await state.service.research.harness.snapshot(Probe, context), first, 'Original sampling identity survives restart.');
    await state.service.research.harness.commit(async tx => { const record = object((await tx.doc(Probe)).value) as unknown as PmRecord;
      observePmOutcome(record, f.advance(10000));
      for (let at = 10001; at < 10200; at++) observePmOutcome(record, f.advance(at)); }, context);
    let record = object((await state.service.research.harness.snapshot(Probe, context))!.value) as unknown as PmRecord;
    assert.equal(Object.keys(record.outcomes).length, 2, 'The original target maturity is sampled once, without inventing a new deadline.');
    const priorOutcomes = structuredClone(record.outcomes);
    f.envelope.cashFlows = [{ id: 'source-deposit:1', eventAtNs: '10200', availableAtNs: '10201', amount: '5', currency: 'USDC', kind: 'DEPOSIT' }];
    await state.service.research.harness.commit(async tx => { const record = object((await tx.doc(Probe)).value) as unknown as PmRecord;
      observePmOutcome(record, f.advance(10202)); }, context);
    record = object((await state.service.research.harness.snapshot(Probe, context))!.value) as unknown as PmRecord;
    assert.equal(Object.keys(record.outcomes).length, 3);
    const ledger = object(record.artifacts[`account-event-ledger:${f.bindingId}`]); assert.equal((ledger.identities as JsonObject[]).length, 1);
    const eventEvidence = (ledger.evidenceRefs as string[])[0]!; assert.ok(record.evidence[eventEvidence]);
    for (const [id, outcome] of Object.entries(priorOutcomes)) assert.deepEqual(record.outcomes[id], outcome, 'Original historical evidence is immutable.');
    const afterEvent = structuredClone(record);
    await state.restart();
    await state.service.research.harness.commit(async tx => { const record = object((await tx.doc(Probe)).value) as unknown as PmRecord;
      for (let at = 10203; at < 10300; at++) observePmOutcome(record, f.advance(at));
    }, context);
    record = object((await state.service.research.harness.snapshot(Probe, context))!.value) as unknown as PmRecord;
    assert.deepEqual(record, afterEvent, 'Re-publication of the same original event ID does not append another event or outcome.');
  } finally { await state.cleanup(); }
});

test('an older uncertain command retains its actual settlement and matched execution without attributing later unrelated events to it', () => {
  const f = fixture(); f.record.commands['command:original']!.state = 'UNCERTAIN';
  observePmOutcome(f.record, f.advance(200));
  f.record.commands['command:new'] = { ...f.record.commands['command:original']!, id: 'command:new', idempotencyKey: 'target:new', state: 'ACKNOWLEDGED' };
  f.record.decisions.new = { ...f.record.decisions.original!, id: 'decision:new', targetCommandId: 'command:new' };
  observePmOutcome(f.record, f.advance(201));
  f.record.commands['command:original']!.state = 'ACKNOWLEDGED';
  observePmOutcome(f.record, f.advance(202));
  assert.equal(Object.values(f.record.outcomes).filter(outcome => outcome.decisionId === 'decision:original').at(-1)!.receiptState, 'ACKNOWLEDGED');
  const oldCount = Object.values(f.record.outcomes).filter(outcome => outcome.decisionId === 'decision:original').length;
  f.envelope.executions = [{ id: 'execution:new', eventAtNs: '203', availableAtNs: '204', instrumentId: 'BTC-SPOT.HYPERLIQUID',
    orderId: 'source-order:new', clientTargetId: 'target:new', side: 'BUY', quantity: '0.01', price: '10', currency: 'USDC', fee: null }];
  observePmOutcome(f.record, f.advance(205));
  assert.equal(Object.values(f.record.outcomes).filter(outcome => outcome.decisionId === 'decision:original').length, oldCount);
  (f.envelope.executions as JsonObject[]).push({ id: 'execution:late-original', eventAtNs: '206', availableAtNs: '207', instrumentId: 'BTC-SPOT.HYPERLIQUID',
    orderId: 'source-order:original', clientTargetId: 'target:original', side: 'BUY', quantity: '0.01', price: '10', currency: 'USDC', fee: null });
  observePmOutcome(f.record, f.advance(208));
  assert.equal(Object.values(f.record.outcomes).filter(outcome => outcome.decisionId === 'decision:original').length, oldCount + 1);
  assert.ok(Object.values(f.record.artifacts).some(artifact => Array.isArray(artifact.matchedExecutionIds)
    && artifact.matchedExecutionIds.includes('execution:late-original')));
});
