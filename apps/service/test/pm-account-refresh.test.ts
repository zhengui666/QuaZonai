import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import type { JsonObject } from '@earendil-works/pi-durable';
import { accountKey, readCapitalControl, recordCapitalControl, recordCapitalControlFailure } from '../src/capital-exit.ts';
import { captureAccount } from '../src/pm-account.ts';
import { toNativeObservedSpotStart } from '../src/pm-account-native.ts';
import { Application, json, nsTime, object } from '../src/facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { built, context, setup } from './support.ts';

// Actual HTTP source reads and original storage consumers. These are transport
// protocol examples, never a brokerage account or investment qualification.
async function receiver() {
  const source = accountSourceFixture(); const server = Fastify(); const gets: string[] = []; const posts: string[] = []; let revision = '7'; let unavailable = false; let blocked = false; const hidden = new Set<string>();
  let snapshot = structuredClone(source.snapshot);
  for (const prefix of ['', '/alias']) {
    const envelope = { clientId: prefix ? 'client-B' : source.connection.clientId, accountId: prefix ? 'account-B' : source.connection.accountId, mode: 'paper', currency: 'USDC' };
    server.get(`${prefix}/capabilities`, (_request, reply) => { gets.push(`${prefix}/capabilities`); return unavailable ? reply.code(503).send({ error: 'original_capabilities_unavailable' }) : { ...envelope,
      capitalExit: { version: 1, supported: !prefix && !hidden.has(prefix), readable: !hidden.has(prefix) }, automaticTargets: { supported: !prefix, revisionBound: !prefix, snapshotBound: !prefix } }; });
    server.get(`${prefix}/capital-exits`, () => { gets.push(`${prefix}/capital-exits`); return { ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: revision, reserved: '0', blocked, exits: [] }; });
    server.get(`${prefix}/account`, () => { gets.push(`${prefix}/account`); return { ...snapshot, clientId: envelope.clientId, accountId: envelope.accountId }; });
    server.get(`${prefix}/targets/prior`, () => { gets.push(`${prefix}/targets/prior`); return { ...envelope, id: 'prior', status: 'EXPIRED', externalId: 'original-prior', expiresAt: nsTime('1000') }; });
    server.post(`${prefix}/targets`, () => { posts.push(`${prefix}/targets`); throw new Error('A read-only refresh must never offer a target.'); });
    server.post(`${prefix}/capital-exits`, () => { posts.push(`${prefix}/capital-exits`); throw new Error('A read-only refresh must never initiate a capital command.'); });
    server.post(`${prefix}/capital-exits/preview`, () => { posts.push(`${prefix}/capital-exits/preview`); throw new Error('An unreadable binding must never request a capital preview.'); });
  }
  const url = await server.listen({ host: '127.0.0.1', port: 0 });
  const a = { ...source.connection, url }, b = { ...source.connection, id: 'alias-B', url: `${url}/alias`, clientId: 'client-B', accountId: 'account-B' };
  return { source, a, b, gets, posts,
    advance() { revision = '8'; snapshot = structuredClone(snapshot); snapshot.controlRevision = '8'; snapshot.sequence = '9007199254740996'; object(snapshot.nativeObservation).sequence = snapshot.sequence; },
    failCapabilities(value: boolean) { unavailable = value; },
    hideSource(value: boolean, prefix = '') { if (value) hidden.add(prefix); else hidden.delete(prefix); },
    blockAccount() { blocked = true; },
    close: () => server.close() };
}

test('joined physical account preserves each transport capability envelope and first receipt after readonly alias polls', async () => {
  const remote = await receiver();
  try {
    const app: { observations: Record<string, JsonObject>; accountControls: Record<string, JsonObject> } = { observations: {}, accountControls: {} };
    app.observations[remote.a.id] = json({ ...remote.source.observation, connection: remote.a });
    recordCapitalControl(app.accountControls, remote.a, await readCapitalControl(remote.a, 'USDC'));
    const before = captureAccount(app, remote.a.id); const original = object(before.artifact.capitalCapabilities);
    recordCapitalControl(app.accountControls, remote.b, await readCapitalControl(remote.b, 'USDC'));
    assert.equal(accountKey(remote.a, app.accountControls), accountKey(remote.b, app.accountControls));
    const after = captureAccount(app, remote.a.id);
    assert.deepEqual(after.artifact.capitalCapabilities, original, 'B cannot replace A\'s actual permission envelope or its clock.');
    assert.deepEqual(after.basis, before.basis, 'A different transport permission does not fabricate a new physical account event.');
    app.observations[remote.b.id] = json({ ...remote.source.observation, connection: remote.b,
      snapshot: { ...remote.source.snapshot, clientId: remote.b.clientId, accountId: remote.b.accountId } });
    const readonly = captureAccount(app, remote.b.id);
    assert.equal(object(readonly.artifact.capitalControl).capability, 'UNSUPPORTED', 'Readable state does not authorize a capital command.');
    assert.equal(toNativeObservedSpotStart(readonly, { selectionStartNs: readonly.basis.availableAtNs }).snapshot.account_id, 'VENUE-001');
    recordCapitalControlFailure(app.accountControls, remote.b, 'downstream_control_503');
    assert.equal(captureAccount(app, remote.b.id).basis.reserved, null);
    assert.deepEqual(captureAccount(app, remote.a.id).basis, before.basis, 'A failed readonly alias does not erase the healthy original binding.');
    recordCapitalControl(app.accountControls, remote.a, await readCapitalControl(remote.a, 'USDC'));
    assert.deepEqual(captureAccount(app, remote.a.id).artifact.capitalCapabilities, original, 'A identical read does not renew the first capability receipt.');
  } finally { await remote.close(); }
});

test('an unreadable joined alias cannot erase another binding physical reservation or gain a capital permission', async () => {
  const remote = await receiver();
  try {
    const app: { observations: Record<string, JsonObject>; accountControls: Record<string, JsonObject> } = { observations: {}, accountControls: {} };
    for (const connection of [remote.a, remote.b]) {
      app.observations[connection.id] = json({ ...remote.source.observation, connection,
        snapshot: { ...remote.source.snapshot, clientId: connection.clientId, accountId: connection.accountId } });
      recordCapitalControl(app.accountControls, connection, await readCapitalControl(connection, 'USDC'));
    }
    const before = captureAccount(app, remote.a.id);
    remote.hideSource(true, '/alias'); recordCapitalControl(app.accountControls, remote.b, await readCapitalControl(remote.b, 'USDC'));
    assert.deepEqual(captureAccount(app, remote.a.id).basis, before.basis, 'No physical update was reported by unreadable B.');
    const after = captureAccount(app, remote.b.id);
    assert.equal(after.basis.reserved, null);
    assert.notEqual(object(after.artifact.capitalControl).capability, 'SUPPORTED');
    assert.throws(() => toNativeObservedSpotStart(after, { selectionStartNs: after.basis.availableAtNs }), /capital_control_not_matched|capital_reserved_or_unknown/);
    remote.hideSource(false, '/alias'); remote.blockAccount();
    recordCapitalControl(app.accountControls, remote.b, await readCapitalControl(remote.b, 'USDC'));
    for (const connection of [remote.a, remote.b]) assert.equal(object(captureAccount(app, connection.id).artifact.capitalControl).capability, 'BLOCKED', 'An actual physical source fence applies across all aliases.');
  } finally { await remote.close(); }
});

test('a newly identified alias migrates its empty provisional control record into the actual physical account', async () => {
  const remote = await receiver();
  try {
    const records: Record<string, JsonObject> = {};
    remote.hideSource(true); recordCapitalControl(records, remote.a, await readCapitalControl(remote.a, 'USDC'));
    recordCapitalControl(records, remote.b, await readCapitalControl(remote.b, 'USDC'));
    const physical = accountKey(remote.b, records); const bBefore = structuredClone(records[physical]!.capabilitiesByBinding);
    remote.hideSource(false); recordCapitalControl(records, remote.a, await readCapitalControl(remote.a, 'USDC'));
    assert.equal(accountKey(remote.a, records), physical, 'An obsolete unknown-source alias cannot shadow the actual joined account.');
    assert.equal(Object.keys(records).length, 1);
    const capabilities = object(records[physical]!.capabilitiesByBinding);
    for (const [key, value] of Object.entries(object(bBefore))) assert.deepEqual(capabilities[key], value, 'Joining A preserves B original envelope and first receipt.');
    assert.equal(Object.keys(capabilities).length, 2);

    const protectedRecords: Record<string, JsonObject> = {};
    remote.hideSource(true); recordCapitalControl(protectedRecords, remote.a, await readCapitalControl(remote.a, 'USDC'));
    recordCapitalControl(protectedRecords, remote.b, await readCapitalControl(remote.b, 'USDC'));
    object(protectedRecords[accountKey(remote.a, protectedRecords)]!.commands).original = {
      original: {}, body: {}, path: 'capital-exits', reserved: '1', state: 'UNCERTAIN', result: null, error: null, taskId: 1 };
    const retained = structuredClone(protectedRecords);
    remote.hideSource(false); const fresh = await readCapitalControl(remote.a, 'USDC');
    assert.throws(() => recordCapitalControl(protectedRecords, remote.a, fresh), /account_alias_requires_reconciliation/);
    assert.deepEqual(protectedRecords, retained, 'A real unresolved command cannot silently move to another account record.');
  } finally { await remote.close(); }
});

test('capital workbench keeps unreadable alias failures local and cannot authorize its capital preview', { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = await receiver();
  try {
    state.options.config.connections = [remote.a, remote.b]; await state.restart();
    const controls = await Promise.all([remote.a, remote.b].map(connection => readCapitalControl(connection, 'USDC')));
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.accountControls ??= {};
      for (const [index, connection] of [remote.a, remote.b].entries()) {
        app.observations[connection.id] = json({ ...remote.source.observation, connection,
          snapshot: { ...remote.source.snapshot, clientId: connection.clientId, accountId: connection.accountId } });
        recordCapitalControl(app.accountControls, connection, controls[index]!);
      }
    }, context);
    remote.hideSource(true, '/alias');
    const url = `/api/v3/observations/${encodeURIComponent(remote.b.id)}/capital-exits`;
    const response = await state.service.app.inject({ url, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body); assert.equal(response.json().reserved, null);
    assert.notEqual(response.json().capability, 'SUPPORTED');
    const preview = await state.service.app.inject({ method: 'POST', url: `${url}/preview`, headers: state.headers,
      payload: { requestId: 'unreadable-preview', amount: '1', currency: 'USDC' } });
    assert.equal(preview.statusCode, 409, preview.body); assert.deepEqual(remote.posts, []);
    const app = (await state.service.research.harness.snapshot(Application, context))!;
    assert.equal(captureAccount(app, remote.a.id).basis.reserved?.amount, '0.000000000000000000');
    assert.equal(captureAccount(app, remote.b.id).basis.reserved, null);
  } finally { await state.cleanup(); await remote.close(); }
});

test('owner HTTP refresh reads a retained account-only source without inventing a portfolio or receipt', { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = await receiver();
  try {
    state.options.config.connections = [remote.a]; await state.restart();
    const original = { ...structuredClone(remote.source.observation), connection: remote.a };
    const control = await readCapitalControl(remote.a, 'USDC');
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.observations[remote.a.id] = json(original);
      app.accountControls ??= {}; recordCapitalControl(app.accountControls, remote.a, control);
    }, context);
    remote.advance(); const priorReads = remote.gets.length;
    const url = `/api/v3/observations/${encodeURIComponent(remote.a.id)}/refresh`;
    assert.equal((await state.service.app.inject({ method: 'POST', url })).statusCode, 401);
    assert.equal(remote.gets.length, priorReads, 'Unauthorized refresh performs no source reads.');
    const response = await state.service.app.inject({ method: 'POST', url, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body); assert.equal(response.json().receipt, null);
    assert.deepEqual(remote.gets.slice(priorReads), ['/account', '/capabilities', '/capital-exits']);
    const app = (await state.service.research.harness.snapshot(Application, context))!;
    const observed = object(app.observations[remote.a.id]);
    assert.equal(object(observed.snapshot).controlRevision, '8');
    assert.equal(object(app.accountControls![accountKey(remote.a, app.accountControls)]!.status).controlRevision, '8');
    assert.ok(BigInt(String(observed.snapshotReceivedAtNs)) > 102n, 'A new actual receipt is not backdated to the archived receipt.');
    assert.deepEqual(app.portfolios, {}); assert.equal(observed.taskId, original.taskId);
    assert.deepEqual(remote.posts, [], 'Refresh neither sends a target nor initiates a capital command.');
  } finally { await state.cleanup(); await remote.close(); }
});

test('a failed current read is first available at its actual receipt, not at the retained historical snapshot', () => {
  const source = accountSourceFixture();
  recordCapitalControlFailure(source.application.accountControls, source.connection, 'downstream_control_503');
  const captured = captureAccount(source.application, source.connection.id);
  const failedAt = String(object(captured.artifact.capitalControlFailure).receivedAtNs);
  assert.ok(BigInt(failedAt) > 102n);
  assert.equal(captured.basis.availableAtNs, failedAt);
  assert.equal(captured.basis.reserved, null);
  assert.equal(captured.basis.observedAtNs, '99', 'A failed read cannot rewrite the original account economic time.');
});

test('official observation refresh follows account revision with actual capability/control GETs and preserves failure evidence', { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = await receiver();
  try {
    state.options.config.connections = [remote.a]; await state.restart(); const portfolio = await built(state);
    const original = { ...structuredClone(remote.source.observation), connection: remote.a,
      input: { ...remote.source.observation.input, portfolioId: portfolio.view.id },
      view: { ...remote.source.observation.view, portfolioId: portfolio.view.id, receipt: { id: 'prior', status: 'EXPIRED' as const, externalId: 'original-prior', expiresAt: nsTime('1000'), acknowledgedAt: null } } };
    const control = await readCapitalControl(remote.a, 'USDC');
    await state.service.research.harness.commit(async tx => { const app = await tx.doc(Application); app.observations[remote.a.id] = json(original);
      app.accountControls ??= {}; recordCapitalControl(app.accountControls, remote.a, control); }, context);
    remote.advance(); const priorReads = remote.gets.length;
    await state.service.work.refreshObservation(remote.a.id);
    const app = (await state.service.research.harness.snapshot(Application, context))!;
    assert.equal(object(object(app.observations[remote.a.id]).snapshot).controlRevision, '8');
    assert.equal(object(app.accountControls![accountKey(remote.a, app.accountControls)]!.status).controlRevision, '8', 'Cached physical control must advance with the actual account source.');
    assert.ok(remote.gets.slice(priorReads).includes('/capabilities')); assert.ok(remote.gets.slice(priorReads).includes('/capital-exits'));
    remote.failCapabilities(true);
    await assert.rejects(state.service.work.refreshObservation(remote.a.id), /downstream_control_503/);
    assert.ok((await state.service.work.getObservation(remote.a.id)).view.error?.includes('downstream_control_503'));
    assert.ok(remote.gets.includes('/targets/prior'), 'Capability failure does not remove original receipt reconciliation.');
    const failed = (await state.service.research.harness.snapshot(Application, context))!;
    assert.equal(captureAccount(failed, remote.a.id).basis.reserved, null, 'A failed current source is unknown, not a healthy stale reservation.');
    remote.failCapabilities(false); await state.service.work.refreshObservation(remote.a.id);
    const recovered = (await state.service.research.harness.snapshot(Application, context))!;
    assert.equal(captureAccount(recovered, remote.a.id).basis.reserved?.amount, '0.000000000000000000');
  } finally { await state.cleanup(); await remote.close(); }
});
