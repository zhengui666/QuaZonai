import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { observedSpotFixture } from './pm-observed-fixture.ts';
import { paperReceiver } from './pm-paper-receiver.ts';
import { nowNs, nsTime, object } from '../src/facts.ts';

// Actual HTTP receiver contract only. This deliberately does not create a PM
// decision, claim scientific qualification, or simulate a fill.
test('registered paper receiver fences original inventory atomically and retains its lost-response receipt', {
  skip: !process.env.QZ_PM_OBSERVED_FIXTURE_DIR, timeout: 15_000,
}, async t => {
  const f = await observedSpotFixture(join(process.env.QZ_PM_OBSERVED_FIXTURE_DIR!, 'science', 'cash-heavy-hour'));
  const directory = await mkdtemp(join(tmpdir(), 'qz-paper-receiver-'));
  const current = object(f.source.current_initial_state), realNow = Date.now.bind(Date);
  const offset = realNow() - Number(BigInt(String(current.open_orders_available_ns)) / 1_000_000n) - 1000;
  t.mock.method(Date, 'now', () => realNow() - offset);
  const receiver = await paperReceiver({ source: f.source, directory, connection: f.connection,
    originalSnapshot: object(object(f.application.observations[f.connection.id]).snapshot) });
  try {
    assert.equal((await fetch(`${f.connection.url}/capabilities`)).status, 503);
    const snapshot = receiver.publishCurrent(current);
    const caps = await (await fetch(`${f.connection.url}/capabilities`)).json();
    assert.equal(caps.automaticTargets.snapshotBound, true);
    const target = { schema_version: 1, asof_ns: nowNs(), valid_until_ns: (BigInt(nowNs()) + 10_000_000_000n).toString(), targets: [], cash_weight: '1' };
    const body = { id: 'controlled-transport-not-science', clientId: f.connection.clientId, accountId: f.connection.accountId,
      expiresAt: nsTime(target.valid_until_ns), target, allocation: { accountSourceId: snapshot.accountSourceId, observationId: snapshot.observationId,
        snapshotSequence: snapshot.sequence, nativeClockNs: snapshot.nativeClockNs, controlRevision: snapshot.controlRevision,
        reserved: '0.000000000000000000', validUntilNs: target.valid_until_ns } };
    const post = (payload: unknown) => fetch(`${f.connection.url}/targets`, { method: 'POST', headers: { 'content-type': 'application/json', 'x-qz-client-id': f.connection.clientId }, body: JSON.stringify(payload) });
    assert.equal((await post({ ...body, allocation: { ...body.allocation, snapshotSequence: '1' } })).status, 409,
      'Same control revision cannot authorize targets against stale actual inventory.');
    assert.equal(receiver.receipts.length, 0);
    assert.equal((await post(body)).status, 503, 'Response is lost only after original request persistence.');
    assert.equal(receiver.receipts.length, 1);
    assert.equal((await post({ ...body, allocation: { ...body.allocation, reserved: '1' } })).status, 409);
    assert.equal((await post(body)).status, 200, 'Exact external idempotency returns original receipt.');
    receiver.revealReceipts();
    const receipt = await (await fetch(`${f.connection.url}/targets/${body.id}`)).json();
    assert.equal(receipt.status, 'ACKNOWLEDGED'); assert.deepEqual(receipt.allocation, body.allocation);
    assert.deepEqual(JSON.parse(await readFile(join(directory, 'paper-receiver-receipts.json'), 'utf8')), [receipt]);
    assert.deepEqual(JSON.parse(await readFile(join(directory, 'original-paper-execution-model.json'), 'utf8')), receiver.registration);
    const heartbeat = receiver.heartbeat('3');
    assert.deepEqual(object(heartbeat.nativeObservation).snapshot, object(snapshot.nativeObservation).snapshot);
    assert.equal(object(heartbeat.execution).validUntilNs, object(snapshot.execution).validUntilNs);
    assert.equal((await post({ ...body, id: 'new-intent-against-old-inventory' })).status, 409);
    assert.equal((await post(body)).status, 200, 'A new source sequence cannot change an already accepted original receipt.');
  } finally { await receiver.close(); await rm(directory, { recursive: true, force: true }); await f.cleanup(); }
});
