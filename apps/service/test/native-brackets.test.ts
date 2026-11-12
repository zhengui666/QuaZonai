import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { downstream } from '../src/downstream.ts';
import { Application, json, Problem } from '../src/facts.ts';
import { validateNativeTargetBrackets } from '../../../packages/contracts/src/native-order-preview.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { setup, context } from './support.ts';

// HTTP/owner protocol tests only; no native worker or exchange is exercised.
const nativeBrackets = { 'BTC-USDC-SPOT.HYPERLIQUID': { enabled: true as const, stop_loss: '90.000000000000000001', take_profit: '110.000000000000000001' } };
test('bracket contract requires an explicit enable and decimal strings', () => {
  validateNativeTargetBrackets(nativeBrackets);
  for (const value of [null, {}, { BTC: { enabled: false, stop_loss: '90', take_profit: '110' } }, { BTC: { enabled: true, stop_loss: 90, take_profit: '110' } }]) {
    assert.throws(() => validateNativeTargetBrackets(value));
  }
});

test('retired delivery is rejected before transport while cancellation binds the retained target', async () => {
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
    await assert.rejects(downstream({ id: source.observation.view.id, receiptId: 'target-original', connection: source.observation.connection,
      target: { valid_until_ns: String(BigInt(Date.now()+60_000)*1_000_000n), targets: [] }, nativeBrackets }, 'deliver', new AbortController().signal),
      (error: unknown) => error instanceof Problem && error.statusCode === 410 && error.message === 'native_strategy_only');
    assert.equal(posts.length, 0, 'Retired delivery must not reach the transport');
    await state.service.research.harness.commit(async tx => {
      (await tx.doc(Application)).observations[source.observation.view.id] = json(source.observation);
    }, context);
    const result = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${source.observation.view.id}/cancel-protection`, headers: state.headers });
    assert.equal(result.statusCode, 200, result.body);
    assert.deepEqual(result.json(), { target_id: 'target-original', cancellation_requested: 2, settled: false });
    assert.deepEqual(posts[0], { clientId: source.connection.clientId, accountId: source.connection.accountId });
    const rejected = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${source.observation.view.id}/cancel-protection` });
    assert.equal(rejected.statusCode, 401);
    assert.equal(posts.length, 1);
  } finally { await state.cleanup(); await receiver.close(); }
});


test('new bracket observations are refused while retained protection receipts survive restart and cancellation', async () => {
  const state = await setup();
  const remote = Fastify();
  const posts: unknown[] = [];
  const cancels: { id: string; body: unknown }[] = [];
  remote.post('/targets', request => { posts.push(request.body); return { status: 'PENDING' }; });
  remote.post<{ Params: { id: string } }>('/targets/:id/cancel-protection', request => {
    cancels.push({ id: request.params.id, body: request.body });
    return { target_id: request.params.id, cancellation_requested: cancels.length === 1 ? 2 : 0, settled: cancels.length > 1 };
  });
  try {
    const url = await remote.listen({ host: '127.0.0.1', port: 0 });
    const source = accountSourceFixture();
    source.observation.connection.url = url;
    source.observation.input = { requestId: 'retained-bracket-request', portfolioId: 'portfolio-original', mode: 'paper', connectionId: source.connection.id, nativeBrackets };
    source.observation.view.receipt = { id: 'retained-target', status: 'ACKNOWLEDGED', expiresAt: new Date(Date.now()+60_000).toISOString(), externalId: 'original-native-target', acknowledgedAt: null,
      nativeProtection: { requested_brackets: 1, status: 'NATIVE_SUBMIT_ONLY', contingency: 'UPSTREAM_OTO_OUO_NOT_ATOMIC_OCO', fill_guaranteed: false } };
    // Explicit historical protocol state, not evidence of an actual exchange submission.
    await state.service.research.harness.commit(async tx => {
      (await tx.doc(Application)).observations[source.observation.view.id] = json(source.observation);
    }, context);
    const original = await state.service.work.getObservation(source.observation.view.id);
    for (let attempt = 0; attempt < 2; attempt++) {
      const start: import('fastify').LightMyRequestResponse = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
        payload: { ...source.observation.input, requestId: 'new-bracket-request' } });
      assert.equal(start.statusCode, 410, start.body);
      assert.equal(start.json().error, 'native_strategy_only');
      assert.deepEqual((await state.service.work.snapshot())!.observations, { [source.observation.view.id]: json(original) });
      await state.restart();
    }
    const restored = await state.service.work.getObservation(source.observation.view.id);
    assert.deepEqual(restored.input, original.input);
    assert.deepEqual(restored.view.receipt, original.view.receipt);
    assert.equal(restored.taskId, original.taskId);
    const path = `/api/v3/observations/${source.observation.view.id}/cancel-protection`;
    assert.equal((await state.service.app.inject({ method: 'POST', url: path })).statusCode, 401);
    assert.equal(cancels.length, 0);
    for (const settled of [false, true]) {
      const response = await state.service.app.inject({ method: 'POST', url: path, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body);
      assert.equal(response.json().settled, settled, 'Request acceptance is not cancellation settlement');
      assert.equal(response.json().target_id, original.view.receipt!.id);
    }
    assert.deepEqual(cancels, [1, 2].map(() => ({ id: original.view.receipt!.id, body: { clientId: source.connection.clientId, accountId: source.connection.accountId } })));
    assert.equal(posts.length, 0, 'Refusal, restart and cancellation cannot deliver a new target');
  } finally { await state.cleanup(); await remote.close(); }
});
