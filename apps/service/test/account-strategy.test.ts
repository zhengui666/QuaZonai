import type { NativeAccountStrategyStatus } from '../../../packages/contracts/src/account-strategy.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { Application, json } from '../src/facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

test('native strategy commands retain exact identity across HTTP uncertainty and Durable restart without replay', async () => {
  const state = await setup(); const source = accountSourceFixture(); const remote = Fastify();
  let revision = '4', fenceReads = 0, unregistered = false;
  let statusOverride: Record<string, unknown> | null = null;
  const calls: string[] = []; const receipts = new Map<string, object>();
  const envelope = { clientId: source.connection.clientId, accountId: source.connection.accountId, currency: 'USDC', mode: 'paper' };
  const native = { strategy_id: 'EMACross-001', state: 'RUNNING', native_version: 'original-upstream', instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID',
    config: { kind: 'EMA_CROSS', trade_size: '1.000000000000000001', fast_period: 10, slow_period: 50 }, restart_warmup: 'OFFICIAL_STRATEGY_REINITIALIZES_INDICATORS', default_start: 'PAUSED' };
  const unregisteredStatus: NativeAccountStrategyStatus = { strategy_id: null, state: 'UNREGISTERED', recovery_only: true, default_start: 'UNREGISTERED', native_version: native.native_version, instrument_id: native.instrument_id, config: { kind: 'REGISTERED', name: 'user::strategy', config: null } };
  const configured = { instrument_id: 'TEST.SIM', strategy: { kind: 'REGISTERED', name: 'user::strategy', config: [null, '0.00001'] } };
  remote.get('/strategy/config', () => ({ ...envelope, accountSourceId: 'original-worker', strategyRevision: revision, configuration: configured, connected: false, runnerValidated: false }));
  remote.post('/strategy/config', request => { const body = request.body as Record<string, unknown>; assert.deepEqual(body.configuration, configured); return { ...envelope, requestId: body.requestId, configuration: configured, status: 'COMPLETED', connected: false, runnerValidated: false }; });
  remote.get('/strategy/fence', () => { fenceReads++; return { ...envelope, accountSourceId: 'original-worker', strategyRevision: revision }; });
  remote.get('/strategy', () => ({ ...envelope, accountSourceId: 'original-worker', strategyRevision: revision, strategy: statusOverride ?? (unregistered ? unregisteredStatus : native) }));
  remote.post('/strategy/commands', (request, reply) => {
    const body = request.body as { requestId: string; action: string; clientId: string; accountId: string; expectedAccountSourceId?: string; expectedStrategyRevision?: string };
    assert.equal(body.clientId, envelope.clientId); assert.equal(body.accountId, envelope.accountId); calls.push(body.action);
    if (body.action === 'START') { assert.equal(body.expectedAccountSourceId, 'original-worker'); assert.equal(body.expectedStrategyRevision, '4'); }
    const receipt = { ...envelope, requestId: body.requestId, action: body.action, status: 'COMPLETED', result: { ...native, state: body.action === 'STOP' ? 'STOPPED' : 'RUNNING' }, error: null };
    if (body.action === 'START' && body.expectedStrategyRevision !== revision) Object.assign(receipt, { status: 'REJECTED', result: { admission: 'REJECTED', started: false, reason: 'strategy_start_fence_changed' }, error: 'strategy_start_fence_changed' });
    if (body.requestId === 'start-rejected') Object.assign(receipt, { status: 'REJECTED', result: { admission: 'REJECTED', started: false, reason: 'account_native_cache_write_failed' }, error: 'account_native_cache_write_failed' });
    if (body.requestId === 'unrecorded-stop') return { ...receipt, status: 'UNCERTAIN', persisted: false, persistenceError: 'account_receipts_write_failed', error: 'strategy_outcome_not_durably_recorded' };
    if (body.action === 'RECOVER') Object.assign(receipt, { result: { recovery_only: true, native_version: 'original-upstream', native_session_id: 'original-session', ready: true } });
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
    const config = await state.service.app.inject({method:'GET',url:url+'/config',headers:state.headers});
    assert.equal(config.statusCode,200);assert.deepEqual(config.json().configuration,configured);
    const save = await state.service.app.inject({method:'POST',url:url+'/config',headers:state.headers,payload:{requestId:'save-only',expectedAccountSourceId:'original-worker',expectedStrategyRevision:'4',configuration:configured}});
    assert.equal(save.statusCode,200);assert.equal(save.json().runnerValidated,false);assert.deepEqual(calls,[],'Configuration save does not send START or RECOVER.');
    const send = (requestId: string, action: string) => state.service.app.inject({ method: 'POST', url: url + '/commands', headers: state.headers, payload: { requestId, action, ...(action === 'START' || action === 'RECOVER' ? { expectedAccountSourceId: 'original-worker', expectedStrategyRevision: '4' } : {}) } });
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
    revision = '5'; // STOP or another command changed the worker after the client saw revision 4.
    await send('stale-start', 'START');
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const stale = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'stale-start')!;
    await state.service.research.harness.waitForTask(stale.taskId as never, context);
    const staleReceipt = await state.service.app.inject({method:'GET',url:url+'/commands/stale-start',headers:state.headers});
    assert.equal(staleReceipt.json().status, 'REJECTED'); assert.equal(staleReceipt.json().error, 'strategy_start_fence_changed');
    assert.equal(fenceReads, 0, 'The service must not refresh the client fence and silently authorize an old START.');
    await send('unrecorded-stop', 'STOP');
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const unrecorded = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'unrecorded-stop')!;
    await state.service.research.harness.waitForTask(unrecorded.taskId as never, context);
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const saved = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'unrecorded-stop')!;
    assert.equal(saved.status, 'UNCERTAIN'); assert.equal((saved.receipt as { persisted: boolean }).persisted, false);
    const maintenance = await state.service.app.inject({ method: 'GET', url: '/api/v3/maintenance', headers: state.headers });
    assert.equal(maintenance.statusCode, 200);
    assert.ok(maintenance.json().blockingExternal.some((row: { kind: string }) => row.kind === 'STRATEGY_COMMAND'), 'Terminal Pi task cannot hide the actual non-durable STOP receipt.');
    const beforeRestart = calls.length;
    await state.restart();
    await send('unrecorded-stop', 'STOP');
    await state.service.app.inject({ method: 'GET', url: url + '/commands/unrecorded-stop', headers: state.headers });
    assert.equal(calls.length, beforeRestart, 'A non-durable remote STOP must only be read after Pi recovery, never resent.');
    const after = (await state.service.research.harness.snapshot(Application, context))!;
    assert.deepEqual(after.observations[source.connection.id]!.snapshot, source.observation.snapshot);
    const recoveredMaintenance = await state.service.app.inject({ method: 'GET', url: '/api/v3/maintenance', headers: state.headers });
    assert.ok(recoveredMaintenance.json().blockingExternal.some((row: { kind: string }) => row.kind === 'STRATEGY_COMMAND'));
    unregistered = true;
    const recoveryStatus = await state.service.app.inject({ method: 'GET', url, headers: state.headers });
    assert.equal(recoveryStatus.statusCode, 200); assert.equal(recoveryStatus.json().strategy.strategy_id, null);
    assert.equal(recoveryStatus.json().strategy.state, 'UNREGISTERED'); assert.deepEqual(recoveryStatus.json().strategy.config, unregisteredStatus.config);
    assert.equal(recoveryStatus.json().strategy.default_start, 'UNREGISTERED');
    assert.equal(Object.hasOwn(recoveryStatus.json().strategy, 'restart_warmup'), false);
    for (const invalid of [
      {strategy_id:null,state:'RUNNING',recovery_only:true},
      {strategy_id:null,state:'UNREGISTERED',recovery_only:false},
      {strategy_id:'x',state:'RUNNING',recovery_only:true},
    ]) { statusOverride = { ...native, ...invalid }; const bad = await state.service.app.inject({method:'GET',url,headers:state.headers}); assert.notEqual(bad.statusCode,200); }
    statusOverride = null;
    const beforeRecover = calls.length;
    const recoveryFence = await state.service.app.inject({ method: 'GET', url: url + '/fence', headers: state.headers });
    assert.equal(recoveryFence.statusCode, 200); assert.equal(recoveryFence.json().accountSourceId, 'original-worker');
    assert.equal(calls.length, beforeRecover, 'A fence read sends no account command.');
    await send('recover-once', 'RECOVER');
    app = (await state.service.research.harness.snapshot(Application, context))!;
    const recovery = Object.values(app.strategyCommands!).find(row => (row.original as { requestId: string }).requestId === 'recover-once')!;
    await state.service.research.harness.waitForTask(recovery.taskId as never, context);
    const recovered = await state.service.app.inject({ method: 'GET', url: url + '/commands/recover-once', headers: state.headers });
    assert.equal(recovered.json().status, 'COMPLETED'); assert.equal(recovered.json().receipt.result.recovery_only, true);
    assert.equal(recovered.json().receipt.result.strategy_id, undefined);
    await state.restart(); await send('recover-once', 'RECOVER');
    assert.equal(calls.filter(action => action === 'RECOVER').length, 1, 'Durable replay never reconnects.');
    assert.equal((await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: { requestId: 'old-target', portfolioId: 'old', mode: 'paper' } })).statusCode, 410);
  } finally { await state.cleanup(); await remote.close(); }
});
