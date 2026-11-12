import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { Application, json, object, observation } from '../src/facts.ts';
import { captureAccount } from '../src/pm-account.ts';
import { toNativeObservedSpotStart } from '../src/pm-account-native.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

test('account HTTP preserves original exact inventory and observation clocks through SQLite restart without issuing targets', async () => {
  const state = await setup(); const source = accountSourceFixture(); const remote = Fastify(); let posts = 0;
  remote.get('/account', () => source.snapshot);
  remote.get('/capabilities', () => ({ clientId: source.connection.clientId, accountId: source.connection.accountId,
    mode: 'paper', currency: 'USDC', capitalExit: { version: 2, supported: false, readable: true },
    automaticTargets: { version: 1, supported: false, revisionBound: false, snapshotBound: false } }));
  remote.get('/capital-exits', () => ({ clientId: source.connection.clientId, accountId: source.connection.accountId,
    mode: 'paper', currency: 'USDC', accountSourceId: source.snapshot.accountSourceId, observationId: source.snapshot.observationId,
    controlRevision: source.snapshot.controlRevision, reserved: '0', exits: [] }));
  remote.post('/targets', (_request, reply) => { posts++; return reply.code(403).send({ error: 'not_authorized' }); });
  try {
    source.connection.url = await remote.listen({ host: '127.0.0.1', port: 0 });
    state.options.config.connections = [source.connection]; await state.restart();
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application);
      app.observations[source.connection.id] = json({ ...source.observation, connection: source.connection });
      Object.assign(app.accountControls ??= {}, source.application.accountControls);
    }, context);
    const refresh = `/api/v3/observations/${encodeURIComponent(source.connection.id)}/refresh`;
    assert.equal((await state.service.app.inject({ method: 'POST', url: refresh })).statusCode, 401);
    const response = await state.service.app.inject({ method: 'POST', url: refresh, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body);
    const before = (await state.service.research.harness.snapshot(Application, context))!;
    const captured = captureAccount(before, source.connection.id);
    assert.equal(captured.basis.equity, null);
    assert.equal(captured.basis.sequence, '9007199254740995');
    assert.equal(captured.basis.observedAtNs, '99');
    assert.deepEqual(object(object(captured.artifact.snapshot).nativeObservation).snapshot, source.native);
    const native = toNativeObservedSpotStart(captured, { selectionStartNs: captured.basis.availableAtNs });
    assert.deepEqual(native.snapshot, source.native);
    assert.equal(object((source.native.balances as object[])[0]).total && object(object((source.native.balances as object[])[0]).total).amount, '900.000000000000000001');
    await state.restart();
    const restored = (await state.service.research.harness.snapshot(Application, context))!;
    assert.deepEqual(captureAccount(restored, source.connection.id), captured);
    assert.deepEqual(observation(restored.observations[source.connection.id]).snapshot, observation(before.observations[source.connection.id]).snapshot);
    assert.equal(posts, 0);
  } finally { await state.cleanup(); await remote.close(); }
});
