import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { downstream } from '../src/downstream.ts';
import { Application, json } from '../src/facts.ts';
import { validateNativeTargetBrackets } from '../../../packages/contracts/src/native-order-preview.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { setup, context, settledObservation } from './support.ts';

// HTTP/owner protocol tests only; exchange behavior is tested in Rust Sandbox.
const nativeBrackets = { 'BTC-USDC-SPOT.HYPERLIQUID': { enabled: true as const, stop_loss: '90.000000000000000001', take_profit: '110.000000000000000001' } };
test('bracket contract requires an explicit enable and decimal strings', () => {
  validateNativeTargetBrackets(nativeBrackets);
  for (const value of [null, {}, { BTC: { enabled: false, stop_loss: '90', take_profit: '110' } }, { BTC: { enabled: true, stop_loss: 90, take_profit: '110' } }]) {
    assert.throws(() => validateNativeTargetBrackets(value));
  }
});

test('service delivery preserves explicit bracket bytes and cancellation binds the saved target', async () => {
  const receiver = Fastify(); const posts: unknown[] = [];
  receiver.post('/targets', request => { posts.push(request.body); return { status: 'PENDING' }; });
  receiver.post<{ Params: { id: string } }>('/targets/:id/cancel-protection', request => {
    posts.push(request.body); return { target_id: request.params.id, cancellation_requested: 2, settled: false };
  });
  const address = await receiver.listen({ host: '127.0.0.1', port: 0 });
  const state = await setup();
  try {
    const source = accountSourceFixture();
    source.observation.connection.url = address;
    source.observation.input = { requestId: 'protected-original', portfolioId: 'portfolio-original', mode: 'paper', connectionId: source.connection.id, nativeBrackets };
    source.observation.view.receipt = { id: 'target-original', status: 'ACKNOWLEDGED', expiresAt: new Date(Date.now()+60_000).toISOString(), externalId: null, acknowledgedAt: null };
    await downstream({ id: source.observation.view.id, receiptId: 'target-original', connection: source.observation.connection,
      target: { valid_until_ns: String(BigInt(Date.now()+60_000)*1_000_000n), targets: [] }, nativeBrackets }, 'deliver', new AbortController().signal);
    assert.deepEqual((posts[0] as { nativeBrackets: unknown }).nativeBrackets, nativeBrackets);
    await state.service.research.harness.commit(async tx => {
      (await tx.doc(Application)).observations[source.observation.view.id] = json(source.observation);
    }, context);
    const result = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${source.observation.view.id}/cancel-protection`, headers: state.headers });
    assert.equal(result.statusCode, 200, result.body);
    assert.deepEqual(result.json(), { target_id: 'target-original', cancellation_requested: 2, settled: false });
    assert.deepEqual(posts[1], { clientId: source.connection.clientId, accountId: source.connection.accountId });
    const rejected = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${source.observation.view.id}/cancel-protection` });
    assert.equal(rejected.statusCode, 401);
    assert.equal(posts.length, 2);
  } finally { await state.cleanup(); await receiver.close(); }
});


test('HTTP observation admission runs the official Durable delivery and preserves bracket receipt through restart and cancel', { timeout: 20_000 }, async () => {
  const state = await setup();
  const remote = Fastify();
  const posts: Record<string, unknown>[] = [];
  const cancels: { id: string; body: unknown }[] = [];
  const identity = { clientId: 'bracket-client', accountId: 'bracket-account' };
  const nativeProtection = { requested_brackets: 1, status: 'NATIVE_SUBMIT_ONLY', contingency: 'UPSTREAM_OTO_OUO_NOT_ATOMIC_OCO', fill_guaranteed: false };
  const receipt = () => ({ ...posts[0], status: 'ACKNOWLEDGED', externalId: 'original-native-target', acknowledgedAt: '2026-09-30T00:00:00.123456789Z', nativeProtection });
  // Local protocol receiver only. No native worker or exchange connection is opened.
  remote.post('/targets', request => { posts.push(request.body as Record<string, unknown>); return receipt(); });
  remote.get('/targets/:id', () => receipt());
  remote.get('/capabilities', () => ({ ...identity, mode: 'paper', currency: 'USD', capitalExit: { version: 2, supported: false, readable: true } }));
  remote.get('/capital-exits', () => ({ ...identity, mode: 'paper', currency: 'USD', accountSourceId: '01900000-0000-7000-8000-000000000001', observationId: '01900000-0000-7000-8000-000000000002', controlRevision: '1', reserved: '0', exits: [] }));
  remote.get('/account', () => ({ ...identity, sequence: '1', nativeClockNs: '1800000000123456789', currency: 'USD', equity: '1000', cash: null, positions: [] }));
  remote.post<{ Params: { id: string } }>('/targets/:id/cancel-protection', request => {
    cancels.push({ id: request.params.id, body: request.body });
    return { target_id: request.params.id, cancellation_requested: cancels.length === 1 ? 2 : 0, settled: cancels.length > 1 };
  });
  try {
    const url = await remote.listen({ host: '127.0.0.1', port: 0 });
    state.options.config.connections = [{ id: 'bracket-paper', name: 'Local protocol receiver', mode: 'paper', url, ...identity }];
    await state.restart();
    // Explicit transport-only input, not a simulated return or scientific qualification.
    const view = await state.service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const id = 'protocol-native-bracket-target';
      const view = { id, name: 'Protocol target only', researchIds: [conversation.id], datasetId: 'controlled', status: 'COMPUTED' as const,
        capital: '1000', currency: 'USD', createdAt: new Date().toISOString(), taskId: null, metrics: [], curve: [], targets: [], error: null };
      (await tx.doc(Application)).portfolios[id] = json({ input: { requestId: id, name: view.name, datasetId: 'controlled', capital: '1000' },
        view, evidence: [], policy: {}, native: null, target: { schema_version: 1, asof_ns: (BigInt(Date.now()) * 1_000_000n).toString(),
          valid_until_ns: (BigInt(Date.now() + 60_000) * 1_000_000n).toString(), cash_weight: '0.75', targets: [{ instrument_id: 'TEST.SIM', weight: '0.25' }] } });
      return view;
    }, context);
    assert.equal(view.status, 'COMPUTED', view.error ?? '');
    const brackets = { 'TEST.SIM': nativeBrackets['BTC-USDC-SPOT.HYPERLIQUID'] };
    const command = { requestId: 'bracket-http-original', portfolioId: view.id, mode: 'paper', connectionId: 'bracket-paper', nativeBrackets: brackets };
    const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: command });
    assert.equal(start.statusCode, 202, start.body);
    const id = start.json().id;
    const admitted = await state.service.work.getObservation(id);
    assert.ok(admitted.taskId, 'HTTP admission must create the actual official Durable task');
    const observed = await settledObservation(state.service, id);
    assert.equal(observed.status, 'OBSERVING', observed.error ?? '');
    assert.equal(posts.length, 1);
    assert.deepEqual(posts[0]!.nativeBrackets, brackets);
    assert.equal(posts[0]!.id, observed.receipt!.id);
    assert.equal(posts[0]!.expiresAt, observed.receipt!.expiresAt);
    assert.equal(posts[0]!.clientId, identity.clientId);
    assert.equal(posts[0]!.accountId, identity.accountId);
    assert.deepEqual(posts[0]!.target, (await state.service.work.getPortfolio(view.id)).target);
    assert.equal(observed.receipt!.status, 'ACKNOWLEDGED');
    assert.equal(observed.receipt!.externalId, 'original-native-target');
    assert.deepEqual(observed.receipt!.nativeProtection, nativeProtection);
    const repeat = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: command });
    assert.equal(repeat.statusCode, 202, repeat.body);
    assert.equal(repeat.json().id, id);
    assert.equal(posts.length, 1, 'Admission replay must not redeliver the target');
    await state.restart();
    const restored = await state.service.work.getObservation(id);
    assert.equal(restored.taskId, admitted.taskId);
    assert.deepEqual(restored.view.receipt, observed.receipt);
    assert.deepEqual(restored.input, command);
    const path = `/api/v3/observations/${id}/cancel-protection`;
    assert.equal((await state.service.app.inject({ method: 'POST', url: path })).statusCode, 401);
    assert.equal(cancels.length, 0);
    for (const settled of [false, true]) {
      const response = await state.service.app.inject({ method: 'POST', url: path, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body);
      assert.equal(response.json().settled, settled, 'Request acceptance must not be misreported as cancellation settlement');
      assert.equal(response.json().target_id, observed.receipt!.id);
    }
    assert.deepEqual(cancels, [1, 2].map(() => ({ id: observed.receipt!.id, body: identity })));
    assert.equal(posts.length, 1, 'Cancellation/restart must not submit another target');
  } finally { await state.cleanup(); await remote.close(); }
});
