import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import Fastify from 'fastify';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { CapitalExitPlanningContext, CapitalExitPreview } from '../../../packages/contracts/src/capital-exit.ts';
import { Application, json, nowNs, object } from '../src/facts.ts';
import { accountKey, capitalCommandPermission, capitalStatusForBinding, CapitalExitCommand, capitalStatus, control, readCapitalControl, recordCapitalControl, transportCapability } from '../src/capital-exit.ts';
import { context, setup } from './support.ts';

async function until(read: () => Promise<boolean>, message: string) {
  const deadline = Date.now() + 10_000;
  do { if (await read()) return; await sleep(10); } while (Date.now() < deadline);
  assert.fail(message);
}

// Actual owner HTTP and official Durable/SQLite admission and receipt paths.
// This source proves protocol ordering, never venue execution or balances.
async function fixture() {
  const state = await setup(), remote = Fastify();
  let permission: 'SUPPORTED' | 'READABLE' | 'FAILED' = 'SUPPORTED', loseResponse = false, receiptReady = false;
  let accountSequence = 0n;
  let nextStatusGate: { enter: () => void; released: Promise<void> } | null = null;
  const releaseStatusGates: (() => void)[] = [];
  const posts: JsonObject[] = [], gets: string[] = [];
  const envelope = { clientId: 'transport-client', accountId: 'transport-account', mode: 'paper' as const, currency: 'USDC' };
  const money = (amount: string) => ({ amount, currency: 'USDC' });
  const plan: CapitalExitPlanningContext = { schema_version: 2, id: 'original-context', account_source_id: 'native-source',
    observation_id: 'native-observation', account_control_revision: '7', currency: 'USDC', environment: 'PAPER',
    asof: new Date().toISOString(), valid_until: new Date(Date.now() + 120_000).toISOString(),
    funds: { total_cash: money('100'), free_cash: money('100'), locked_cash: money('0'), verified_withdrawable: null },
    instruments: [], capability: 'SUPPORTED', reason_codes: [], supported_policies: ['CASH_ONLY'] };
  const preview: CapitalExitPreview = { schema_version: 2, id: 'original-preview', plan_artifact_id: 'original-plan',
    account_source_id: 'native-source', original_observation_id: 'native-observation', expected_account_control_revision: '7',
    environment: 'PAPER', scope: { kind: 'AMOUNT', amount: '10', currency: 'USDC' }, policy: { kind: 'CASH_ONLY' },
    capability: 'SUPPORTED', reason_codes: [], valid_until: new Date(Date.now() + 120_000).toISOString(),
    funds: { requested: money('10'), remaining_managed_capital: money('965'), estimated_execution_cost: money('0') },
    proposed_cancellations: [], retained_protective_orders: [], reduction_legs: [] };
  for (const prefix of ['', '/other-source']) {
    remote.get(prefix + '/capabilities', (_request, reply) => permission === 'FAILED' ? reply.code(503).send({ error: 'source_unavailable' }) : {
      ...envelope, capitalExit: { version: 2, supported: permission === 'SUPPORTED', readable: true, policies: ['CASH_ONLY'],
        actions: permission === 'SUPPORTED' ? ['START', 'PAUSE', 'CANCEL'] : ['PAUSE', 'CANCEL'] } });
    remote.get(prefix + '/capital-exits', async () => {
      if (prefix === '' && nextStatusGate) {
        const gate = nextStatusGate; nextStatusGate = null; gate.enter(); await gate.released;
      }
      return { ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7', reserved: '25', exits: [] };
    });
  }
  remote.get('/account', () => ({ ...envelope, accountSourceId: 'native-source', observationId: 'native-observation', controlRevision: '7',
    sequence: String(++accountSequence), nativeClockNs: nowNs(), equity: '1000', cash: '100', positions: [] }));
  remote.get('/capital-exits/planning-context', () => ({ ...envelope, context: plan }));
  remote.post('/capital-exits/preview', request => ({ ...envelope, requestId: object(request.body).requestId, preview }));
  remote.post('/capital-exits', (request, reply) => {
    const body = json(request.body); posts.push(body);
    if (loseResponse) { reply.hijack(); reply.raw.destroy(); return; }
    return { ...body, status: 'REJECTED', result: null };
  });
  remote.get<{ Params: { id: string } }>('/capital-exits/commands/:id', (request, reply) => {
    gets.push(request.params.id); const original = posts.find(item => item.requestId === request.params.id);
    return receiptReady && original ? { ...original, status: 'REJECTED', result: null } : reply.code(503).send({ error: 'receipt_unknown' });
  });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  try {
    const connection = { id: 'permission-account', name: 'Existing cash account', url, clientId: envelope.clientId, accountId: envelope.accountId, mode: envelope.mode };
    state.options.config.jobBinary = join(state.directory, 'unavailable-science-worker');
    state.options.config.connections = [connection]; await state.restart();
    const admitted = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'existing-account', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: connection.id } });
    assert.equal(admitted.statusCode, 202, admitted.body); assert.equal(admitted.json().portfolioId, null);
    const path = '/api/v3/observations/' + admitted.json().id + '/capital-exits';
    const planning = await state.service.app.inject({ url: path + '/planning-context', headers: state.headers }); assert.equal(planning.statusCode, 200, planning.body);
    const planned = await state.service.app.inject({ method: 'POST', url: path + '/preview', headers: state.headers,
      payload: { requestId: 'original-preview-request', amount: '10', currency: 'USDC', contextId: plan.id, policy: { kind: 'CASH_ONLY' } } });
    assert.equal(planned.statusCode, 200, planned.body);
    const request = { requestId: 'original-capital-command', action: 'START', previewId: preview.id };
    const native = { schema_version: 2, preview_id: preview.id, expected_account_control_revision: '7',
      expected_source_observation_id: 'native-observation', acknowledged_plan_artifact_id: preview.plan_artifact_id };
    const read = async () => (await state.service.research.harness.snapshot(Application, context))!;
    return { state, connection, path, request, native, posts, gets, read,
      permission(value: typeof permission) { permission = value; },
      holdNextStatus() {
        let enter!: () => void, release!: () => void;
        const entered = new Promise<void>(resolve => { enter = resolve; });
        const released = new Promise<void>(resolve => { release = resolve; });
        nextStatusGate = { enter, released }; releaseStatusGates.push(release);
        return { entered, release };
      },
      loseResponse() { loseResponse = true; }, allowReceipt() { receiptReady = true; },
      send: () => state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: request }),
      refresh: () => state.service.app.inject({ url: path, headers: state.headers }),
      async cleanup() { for (const release of releaseStatusGates) release(); await state.cleanup(); await remote.close(); } };
  } catch (error) { await state.cleanup(); await remote.close(); throw error; }
}

test('a different base URL cannot inherit a recorded capability with identical client/account names', async () => {
  const f = await fixture();
  try {
    const app = await f.read(), original = control(app.accountControls![accountKey(f.connection, app.accountControls)]);
    const alias = { ...f.connection, id: 'unobserved-alias', url: f.connection.url + '/other-source' };
    const before = structuredClone(original);
    assert.equal(transportCapability(original, alias), null);
    assert.deepEqual(original, before, 'A miss cannot manufacture another URL capability receipt.');
    assert.notEqual(capitalStatusForBinding(original, alias).capability, 'SUPPORTED');
    assert.equal(capitalStatusForBinding(original, alias).reserved, null);
    assert.equal(transportCapability(original, f.connection)!.document.clientId, f.connection.clientId);
  } finally { await f.cleanup(); }
});

test('an actually read second URL retains its own first receipt even when its envelope is identical', async () => {
  const f = await fixture();
  try {
    const app = await f.read(), key = accountKey(f.connection, app.accountControls), original = control(app.accountControls![key]);
    const first = transportCapability(original, f.connection)!;
    const alias = { ...f.connection, id: 'new-source', url: f.connection.url + '/other-source' };
    await sleep(5); recordCapitalControl(app.accountControls!, alias, await readCapitalControl(alias, 'USDC'));
    const second = transportCapability(control(app.accountControls![accountKey(alias, app.accountControls)]), alias)!;
    assert.deepEqual(second.document, first.document);
    assert.ok(BigInt(second.receivedAtNs) > BigInt(first.receivedAtNs), 'The second URL has its own actual first read time.');
    recordCapitalControl(app.accountControls!, alias, await readCapitalControl(alias, 'USDC'));
    assert.deepEqual(transportCapability(control(app.accountControls![key]), alias), second);
    assert.deepEqual(transportCapability(control(app.accountControls![key]), f.connection), first);
  } finally { await f.cleanup(); }
});

test('a delayed old status response cannot restore its earlier SUPPORTED permission after a newer READABLE read commits', { timeout: 20_000 }, async () => {
  const f = await fixture(); const gate = f.holdNextStatus();
  let oldRead: ReturnType<typeof readCapitalControl> | undefined;
  try {
    oldRead = readCapitalControl(f.connection, 'USDC');
    await gate.entered; // The first real capability GET completed; its status GET is still in flight.
    await sleep(5); f.permission('READABLE');
    const newer = await readCapitalControl(f.connection, 'USDC');
    await f.state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); recordCapitalControl(app.accountControls!, f.connection, newer);
    }, context);
    gate.release(); const older = await oldRead;
    await f.state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); recordCapitalControl(app.accountControls!, f.connection, older);
    }, context);
    const app = await f.read(), state = control(app.accountControls![accountKey(f.connection, app.accountControls)]);
    assert.equal(object(transportCapability(state, f.connection)!.document.capitalExit).supported, false);
    assert.equal(capitalStatusForBinding(state, f.connection).capability, 'UNSUPPORTED');
    assert.throws(() => capitalCommandPermission(state, f.connection, 'START'), /capital_exit_initiation_unavailable/);
    const denied = await f.send(); assert.equal(denied.statusCode, 409, denied.body); assert.equal(f.posts.length, 0);
  } finally { gate.release(); if (oldRead) await oldRead.catch(() => {}); await f.cleanup(); }
});

for (const changed of ['READABLE', 'FAILED'] as const) test('a current ' + changed + ' source rejects a new START despite its retained valid preview', { timeout: 20_000 }, async () => {
  const f = await fixture();
  try {
    f.permission(changed); const result = await f.send(); assert.equal(result.statusCode, 409, result.body);
    const app = await f.read(), state = control(app.accountControls![accountKey(f.connection, app.accountControls)]);
    assert.deepEqual(state.commands, {}); assert.equal(f.posts.length, 0);
    assert.notEqual(capitalStatusForBinding(state, f.connection).capability, 'SUPPORTED');
    assert.equal(state.status?.controlRevision, '7', 'Losing permission does not invent a new physical account revision.');
  } finally { await f.cleanup(); }
});

test('a persisted pending task rejects revoked permission before POST and releases only its unsent local reservation', { timeout: 20_000 }, async () => {
  const f = await fixture();
  try {
    const app = await f.read(), initial = control(app.accountControls![accountKey(f.connection, app.accountControls)]);
    const permissionBasis = capitalCommandPermission(initial, f.connection, 'START');
    f.permission('READABLE'); const revoked = await readCapitalControl(f.connection, 'USDC');
    const taskId = await f.state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application), key = accountKey(f.connection, facts.accountControls);
      recordCapitalControl(facts.accountControls!, f.connection, revoked);
      const state = control(facts.accountControls![key]);
      const conversation = (state.conversationId ?? (await tx.createConversation({ ownership: { kind: 'ownerless' } })).id) as ConversationId;
      const id = await tx.createTask(CapitalExitCommand, { key, requestId: f.request.requestId, connection: f.connection, reconcile: false,
        binary: f.state.options.config.jobBinary, dataDir: f.state.directory },
      { conversationId: conversation, ownership: { kind: 'conversation' }, background: true });
      state.commands[f.request.requestId] = { original: f.request, body: f.native, path: 'capital-exits', reserved: '10', state: 'PENDING', result: null, error: null, taskId: id, permissionBasis };
      state.conversationId = conversation; facts.accountControls![key] = json(state);
      assert.equal(capitalStatus(state).reserved, '35.000000000000000000'); return id;
    }, context);
    await f.state.service.research.harness.waitForTask(taskId as TaskId, context);
    const after = await f.read(), state = control(after.accountControls![accountKey(f.connection, after.accountControls)]), command = state.commands[f.request.requestId]!;
    assert.equal(f.posts.length, 0); assert.equal(command.state, 'REJECTED'); assert.equal(command.reserved, '0');
    assert.equal(command.result, null, 'A local refusal is not a remote receipt.'); assert.ok(command.error); assert.deepEqual(command.original, f.request);
    assert.equal(capitalStatus(state).reserved, '25.000000000000000000', 'The pre-existing native account reservation remains protected.');
  } finally { await f.cleanup(); }
});

test('a response-lost POST reconciles its original receipt after permission is revoked without a replacement POST', { timeout: 20_000 }, async () => {
  const f = await fixture();
  try {
    f.loseResponse(); const response = await f.send(); assert.equal(response.statusCode, 202, response.body);
    await until(async () => f.posts.length === 1, 'The original POST must have reached the source.');
    const original = structuredClone(f.posts[0]!); f.permission('READABLE'); const refreshed = await f.refresh(); assert.equal(refreshed.statusCode, 200, refreshed.body);
    f.allowReceipt();
    const app = await f.read(), command = control(app.accountControls![accountKey(f.connection, app.accountControls)]).commands[f.request.requestId]!;
    await f.state.service.research.harness.waitForTask(command.taskId as TaskId, context);
    const after = await f.read(), settled = control(after.accountControls![accountKey(f.connection, after.accountControls)]).commands[f.request.requestId]!;
    assert.equal(settled.state, 'REJECTED'); assert.deepEqual(settled.result, { ...original, status: 'REJECTED', result: null });
    assert.deepEqual(f.posts, [original]); assert.ok(f.gets.length > 0 && f.gets.every(id => id === f.request.requestId));
  } finally { await f.cleanup(); }
});
