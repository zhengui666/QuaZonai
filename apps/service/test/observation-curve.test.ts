import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Application, json, observation, nsTime } from '../src/facts.ts';
import { acceptSnapshot } from '../src/downstream.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { setup, context } from './support.ts';

// Source/UI protocol only: these snapshots do not claim native economics,
// broker provenance, strategy attribution, or a full scientific time series.
function source() {
  const original = accountSourceFixture(); const fact = original.observation;
  fact.snapshot = null; fact.view.curve = []; let sequence = 0n;
  const publish = (clock: bigint, equity: string | null) => {
    const incoming = json(original.snapshot); incoming.sequence = String(++sequence);
    incoming.nativeClockNs = String(clock); incoming.equity = equity;
    acceptSnapshot(fact, incoming); return incoming;
  };
  return { fact, publish };
}

test('observation display preserves unknown interval boundaries and same-clock value changes without heartbeat growth', () => {
  const { fact, publish } = source();
  publish(10n, '100.000000000000000001');
  for (let i = 0; i < 3000; i++) publish(10n, '100.000000000000000001');
  assert.deepEqual(fact.view.curve, [{ at: nsTime('10'), value: '100.000000000000000001' }]);
  publish(10n, '99.000000000000000001');
  assert.equal(fact.view.curve.length, 2, 'An actual same-clock changed value is not a duplicate heartbeat.');
  for (let clock = 11n; clock <= 3000n; clock++) publish(clock, null);
  assert.deepEqual(fact.view.curve.slice(-2), [{ at: nsTime('11'), value: null }, { at: nsTime('3000'), value: null }]);
  assert.equal(fact.view.curve.length, 4); assert.equal(fact.view.equity, null);
  const latest = publish(3001n, '101.000000000000000001');
  assert.deepEqual(fact.snapshot, latest); assert.equal(fact.view.curve.at(-1)!.value, '101.000000000000000001');
  assert.equal(fact.view.curve[2]!.value, null, 'The missing interval remains a chart break, never interpolated to zero.');
  const before = json(fact);
  assert.throws(() => acceptSnapshot(fact, { ...latest, equity: '102' }), /account_snapshot_conflict/);
  assert.deepEqual(json(fact), before, 'A conflicting receipt cannot edit either source facts or display points.');
});

test('bounded recent observation display and exact latest source survive official SQLite reopen', { timeout: 15_000 }, async () => {
  const state = await setup(); const { fact, publish } = source();
  try {
    for (let clock = 1n; clock <= 6000n; clock++) publish(clock, `${10000n + clock}.000000000000000001`);
    assert.equal(fact.view.curve.length, 1024); assert.equal(fact.view.curve[0]!.at, nsTime('4977'));
    assert.equal(fact.view.curve.at(-1)!.at, nsTime('6000'));
    const originalTarget = { id: 'unmodified-original-target', valid_until_ns: '9000' }; fact.target = originalTarget;
    await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).observations[fact.connection.id] = json(fact); }, context);
    await state.restart();
    const restored = observation((await state.service.research.harness.snapshot(Application, context))!.observations[fact.connection.id]);
    assert.deepEqual(restored.view.curve, fact.view.curve); assert.deepEqual(restored.snapshot, fact.snapshot); assert.deepEqual(restored.target, originalTarget);
    const next = { ...json(restored.snapshot!), sequence: '6001', nativeClockNs: '6001', equity: '16001.000000000000000001' };
    await state.service.research.harness.commit(async tx => { acceptSnapshot(observation((await tx.doc(Application)).observations[fact.connection.id]), next); }, context);
    const after = observation((await state.service.research.harness.snapshot(Application, context))!.observations[fact.connection.id]);
    assert.equal(after.view.curve.length, 1024); assert.equal(after.view.curve[0]!.at, nsTime('4978'));
    assert.deepEqual(after.snapshot, next); assert.deepEqual(after.target, originalTarget);
    assert.equal(after.view.equity, '16001.000000000000000001');
  } finally { await state.cleanup(); }
});
