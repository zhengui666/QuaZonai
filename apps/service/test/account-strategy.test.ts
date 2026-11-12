import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { Application, json } from '../src/facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

test('native strategy commands retain exact identity across HTTP uncertainty and Durable restart without replay', async () => {
  const state = await setup(); const source = accountSourceFixture(); const remote = Fastify();
  const calls: string[] = []; const receipts = new Map<string, object>();
  const envelope = { clientId: source.connection.clientId, accountId: source.connection.accountId, currency: 'USDC', mode: 'paper' };
  const native = { strategy_id: 'EMACross-001', state: 'RUNNING', native_version: 'original-upstream', instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID',
    config: { kind: 'EMA_CROSS', trade_size: '1.000000000000000001', fast_period: 10, slow_period: 50 }, restart_warmup: 'OFFICIAL_STRATEGY_REINITIALIZES_INDICATORS', default_start: 'PAUSED' };
  remote.get('/strategy', () => ({ ...envelope, strategy: native }));
  remote.post('/strategy/commands', (request, reply) => {
    const body = request.body as { requestId: string; action: string; clientId: string; accountId: string };
    assert.equal(body.clientId, envelope.clientId); assert.equal(body.accountId, envelope.accountId); calls.push(body.action);
    const receipt = { ...envelope, requestId: body.requestId, action: body.action, status: 'COMPLETED', result: { ...native, state: body.action === 'STOP' ? 'STOPPED' : 'RUNNING' }, error: null };
    if (body.requestId === 'start-rejected') Object.assign(receipt, { status: 'REJECTED', result: { admission: 'REJECTED', started: false, reason: 'account_native_cache_write_failed' }, error: 'account_native_cache_write_failed' });
    if (body.requestId === 'unrecorded-stop') return { ...receipt, status: 'UNCERTAIN', persisted: false, persistenceError: 'account_receipts_write_failed', error: 'strategy_outcome_not_durably_recorded' };
    receipts.set(body.requestId, receipt);
    if (body.requestId === 'start-once') { reply.raw.destroy(); return; } // Original accepted command, response lost.
    return receipt;
  });
  remote.get<{ Params: { id: string } }>('/strategy/commands/:id', (request, reply) => receipts.get(request.params.id) ?? reply.code(404).send({ error: 'missing' }));
  try {
    source.connection.url = await remote.listen({ host: '127.0.0.1', port: 0 });
    state.options.config.connections = [source.connection]; await state.restart();
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.observations[source.connection.id] = json({ ...source.observation, connection: source.connection });
      Object.assign(app.accountControls ??= {}, source.application.accountControls);
    }, context);
    const url = `/api/v3/observations/${source.connection.id}/strategy`;
    assert.equal((await state.service.app.inject({ method: 'GET', url })).statusCode, 401);
    const current = await state.service.app.inject({ method: 'GET', url, headers: state.headers });
    assert.equal(current.statusCode, 200); assert.deepEqual(current.json().strategy.config, native.config);
    const send = (requestId: string, action: string) => state.service.app.inject({ method: 'POST', url: url + '/commands', headers: state.headers, payload: { requestId, action } });
    const first = await send('start-once', 'START'); assert.equal(first.statusCode, 202, first.body);
    let app = (await state.service.research.harness.snapshot(Application, context))!;
    const command = Object.values(app.strategyCommands!)[0]!;
    await state.service.research.harness.waitForTask(command.taskId as never, context);
    const received = await state.service.app.inject({ method: 'GET', url: url + '/commands/start-once', headers: state.headers });
    assert.equal(received.statusCode, 200, received.body); assert.equal(received.json().status, 'COMPLETED');
    assert.equal(received.body.includes('connection'), false); assert.deepEqual(calls, ['START']);
    await state.restart();
    assert.equal((await send('start-once', 'START')).statusCode, 202); assert.deepEqual(calls, ['START']);
    assert.equal((await send('start-once', 'STOP')).statusCode, 409);
    assert.equal((await send('stop-once', 'STOP')).statusCode, 202);
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const stop = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'stop-once')!;
    await state.service.research.harness.waitForTask(stop.taskId as never, context);
    assert.deepEqual(calls, ['START', 'STOP']);
    assert.equal((await send('start-rejected', 'START')).statusCode, 202);
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const rejection = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'start-rejected')!;
    await state.service.research.harness.waitForTask(rejection.taskId as never, context);
    const rejected = await state.service.app.inject({ method: 'GET', url: url + '/commands/start-rejected', headers: state.headers });
    assert.equal(rejected.json().status, 'REJECTED'); assert.equal(rejected.json().receipt.result.started, false);
    const count = calls.length;
    await send('start-rejected', 'START'); assert.equal(calls.length, count, 'Confirmed rejection is not silently retried.');
    await send('unrecorded-stop', 'STOP');
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const unrecorded = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'unrecorded-stop')!;
    await state.service.research.harness.waitForTask(unrecorded.taskId as never, context);
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const saved = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'unrecorded-stop')!;
    assert.equal(saved.status, 'UNCERTAIN'); assert.equal((saved.receipt as { persisted: boolean }).persisted, false);
    const beforeRestart = calls.length;
    await state.restart();
    await send('unrecorded-stop', 'STOP');
    await state.service.app.inject({ method: 'GET', url: url + '/commands/unrecorded-stop', headers: state.headers });
    assert.equal(calls.length, beforeRestart, 'A non-durable remote STOP must only be read after Pi recovery, never resent.');
    const after = (await state.service.research.harness.snapshot(Application, context))!;
    assert.deepEqual(after.observations[source.connection.id]!.snapshot, source.observation.snapshot);
    assert.equal((await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: { requestId: 'old-target', portfolioId: 'old', mode: 'paper' } })).statusCode, 410);
  } finally { await state.cleanup(); await remote.close(); }
});
