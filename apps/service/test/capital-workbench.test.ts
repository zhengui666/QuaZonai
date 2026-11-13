import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import Fastify from 'fastify';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { CapitalExitPlanningContext, CapitalExitPreview, CapitalExitPolicy, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import type { ConnectionConfig } from '../src/config.ts';
import { accountKey, control, CapitalExitCommand } from '../src/capital-exit.ts';
import { Application, json, nowNs, object } from '../src/facts.ts';
import { context, setup } from './support.ts';

const money = (amount: string) => ({ amount, currency: 'USDC' });
const actions = ['START', 'PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION', 'RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'];
async function until<T>(read: () => Promise<T>, ready: (value: T) => boolean) {
  const deadline = Date.now() + 12_000; let value: T;
  do {
    value = await read(); if (ready(value)) return value;
    await new Promise(resolve => setTimeout(resolve, 20));
  } while (Date.now() < deadline);
  assert.fail('Expected capital protocol state, received ' + JSON.stringify(value));
}

async function protocolReceiver() {
  // This local HTTP source tests actual service/SQLite protocol behavior, never exchange execution or financial evidence.
  const server = Fastify(); const envelope = { clientId: 'capital-client', accountId: 'capital-account', mode: 'paper' as const, currency: 'USDC' };
  let observationId = 'observation-original'; let controlRevision = '7'; let sequence = 0n; let blocked = false;
  let loseNextStart = false; let allowReceipt = true; let current: CapitalExitView | null = null;
  const previews = new Map<string, CapitalExitPreview>(); const receipts = new Map<string, JsonObject>();
  const previewPosts: JsonObject[] = []; const startPosts: JsonObject[] = []; const actionPosts: JsonObject[] = []; const receiptGets: string[] = []; const targetPosts: JsonObject[] = [];
  const plan: CapitalExitPlanningContext = {
    schema_version: 2, id: 'planning-original', account_source_id: 'physical-source', observation_id: observationId, account_control_revision: controlRevision,
    currency: 'USDC', environment: 'PAPER', asof: new Date().toISOString(), valid_until: new Date(Date.now() + 120_000).toISOString(),
    funds: { total_cash: money('100'), free_cash: money('100'), locked_cash: money('0'), verified_withdrawable: null },
    instruments: [{ instrument_id: 'BTC/USDC.HYPERLIQUID', symbol: 'BTC', quantity: '0.01', eligible_quantity: '0.01', quantity_increment: '0.00001', price_increment: '1',
      bid_price: '50000', ask_price: '50001', quote_asof: new Date().toISOString(), fee_rate: '0.0004', reference_evidence_id: 'original-quote' }],
    capability: 'SUPPORTED', reason_codes: [], supported_policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'],
  };
  server.get('/capabilities', () => ({ ...envelope, capitalExit: { version: 2, supported: true, readable: true,
    policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'], actions } }));
  server.get('/account', () => ({ ...envelope, accountSourceId: 'physical-source', observationId, controlRevision,
    sequence: String(++sequence), nativeClockNs: nowNs(), equity: '1000', cash: '100',
    positions: [{ instrumentId: 'BTC/USDC.HYPERLIQUID', quantity: '0.01', value: '500', pnl: null }] }));
  server.get('/capital-exits', () => ({ ...envelope, accountSourceId: 'physical-source', observationId, controlRevision,
    reserved: current?.funds.reserved_amount.amount ?? '0', blocked, reason: blocked ? 'current_quote_unavailable' : null, exits: current ? [current] : [] }));
  server.get('/capital-exits/planning-context', () => ({ ...envelope, context: plan }));
  server.get<{ Params: { exitId: string } }>('/capital-exits/:exitId', (request, reply) => current?.id === request.params.exitId ? { ...envelope, exit: current } : reply.code(404).send({ error: 'exit_not_found' }));
  server.post('/capital-exits/preview', request => {
    const body = object(request.body); previewPosts.push(json(body)); const input = object(body.request);
    const scope = object(input.scope); const policy = input.policy as unknown as CapitalExitPolicy;
    const preview: CapitalExitPreview = {
      schema_version: 2, id: 'preview-' + body.requestId, plan_artifact_id: 'plan-' + body.requestId,
      account_source_id: String(input.account_source_id), original_observation_id: String(input.expected_source_observation_id),
      expected_account_control_revision: String(input.expected_account_control_revision), environment: 'PAPER', scope: scope as unknown as CapitalExitPreview['scope'], policy,
      capability: 'SUPPORTED', reason_codes: [], valid_until: new Date(Date.now() + 60_000).toISOString(),
      funds: { requested: money(String(scope.amount)), remaining_managed_capital: money('900'), estimated_execution_cost: money('0') },
      proposed_cancellations: [], retained_protective_orders: [], reduction_legs: policy.kind === 'BOUNDED_LIMIT' ? policy.legs : [],
    };
    previews.set(preview.id, preview); return { ...envelope, requestId: body.requestId, preview };
  });
  server.post('/capital-exits', (request, reply) => {
    const body = object(request.body); startPosts.push(json(body)); const command = object(body.command); const preview = previews.get(String(command.preview_id));
    assert.ok(preview, 'The source only accepts the original acknowledged preview.');
    controlRevision = String(BigInt(controlRevision) + 1n);
    current = {
      schema_version: 2, id: 'native-exit-' + body.requestId, revision: '1', account_source_id: 'physical-source', environment: 'PAPER',
      state: 'WAITING_CASH', preparation_state: 'WAITING_CASH', account_control_revision: controlRevision, command_id: String(body.requestId), reason_codes: [],
      policy: preview.policy, scope: preview.scope, remaining_trading: 'FENCED_PENDING_TARGET',
      funds: { requested_amount: preview.funds.requested, reserved_amount: preview.funds.requested, staged_for_withdrawal_amount: money('0'), waived_amount: money('0'),
        released_cash_amount: money('0'), verified_withdrawable_amount: null, reconciled_withdrawal_amount: money('0'), withdrawability: 'UNVERIFIED' },
      action_availability: { PAUSE: { allowed: true, reason: null }, CANCEL: { allowed: true, reason: null } }, progress: [], events: [],
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    };
    const receipt = json({ ...body, status: 'ACCEPTED', result: current }); receipts.set(String(body.requestId), receipt);
    if (loseNextStart) { loseNextStart = false; reply.hijack(); reply.raw.destroy(); return; }
    return receipt;
  });
  server.post<{ Params: { exitId: string } }>('/capital-exits/:exitId/actions', request => {
    const body = object(request.body); actionPosts.push(json(body)); const command = object(body.command);
    assert.equal(request.params.exitId, current!.id); assert.equal(command.action, 'PAUSE');
    controlRevision = String(BigInt(controlRevision) + 1n); current!.revision = String(BigInt(current!.revision) + 1n);
    current!.account_control_revision = controlRevision; current!.command_id = String(body.requestId); current!.state = 'PAUSED';
    current!.action_availability.PAUSE = { allowed: false, reason: 'already_paused' };
    const receipt = json({ ...body, status: 'ACCEPTED', result: current }); receipts.set(String(body.requestId), receipt); return receipt;
  });
  server.get<{ Params: { requestId: string } }>('/capital-exits/commands/:requestId', (request, reply) => {
    receiptGets.push(request.params.requestId); return allowReceipt && receipts.has(request.params.requestId) ? receipts.get(request.params.requestId) : reply.code(404).send({ error: 'original_receipt_unavailable' });
  });
  server.post('/targets', request => { targetPosts.push(json(request.body)); throw new Error('An account-only binding does not deliver a target.'); });
  const nativePreviewPosts: { path: string; body: JsonObject }[] = [];
  let wrongPreviewIdentity = false; let falselySubmitted = false; let malformedPreview = false;
  for (const path of ['/sizing/fixed-risk', '/orders/preview']) server.post(path, request => {
    assert.equal(request.headers['x-qz-client-id'], envelope.clientId);
    const body = object(request.body); nativePreviewPosts.push({ path, body });
    assert.equal(body.clientId, envelope.clientId); assert.equal(body.accountId, envelope.accountId);
    return { ...envelope, accountId: wrongPreviewIdentity ? 'wrong-account' : envelope.accountId,
      preview_only: true, ...(path === '/sizing/fixed-risk'
        ? { instrument_id: object(body.sizing).instrument_id, quantity: '0.00000001', model: 'NAUTILUS_FIXED_RISK', input_source: 'REQUEST_SCENARIO', account_balance_verified: false }
        : { submitted: falselySubmitted, automatic_protection_enabled: false, orders: malformedPreview ? [null] : [{ instrument_id: body.instrument_id }] }) };
  });
  const url = await server.listen({ host: '127.0.0.1', port: 0 });
  const connection: ConnectionConfig = { id: 'capital-connection', name: 'Existing cash account', url, clientId: envelope.clientId, accountId: envelope.accountId, mode: envelope.mode };
  return { nativePreviewPosts, malformedResponse() { malformedPreview = true; }, wrongIdentity() { wrongPreviewIdentity = true; }, submittedResponse() { falselySubmitted = true; }, connection, plan, previewPosts, startPosts, actionPosts, receiptGets, targetPosts,
    retainOriginal(requestId = 'original-lost-start') {
      current = { schema_version: 2, id: 'original-exit', revision: '1', account_source_id: 'physical-source', environment: 'PAPER', state: 'WAITING_CASH', preparation_state: 'WAITING_CASH', account_control_revision: controlRevision, command_id: requestId, reason_codes: [],
        policy: { kind: 'CASH_ONLY' }, scope: { kind: 'AMOUNT', amount: '100', currency: 'USDC' }, remaining_trading: 'FENCED_PENDING_TARGET',
        funds: { requested_amount: money('100'), reserved_amount: money('100'), staged_for_withdrawal_amount: money('0'), waived_amount: money('0'), released_cash_amount: money('0'), verified_withdrawable_amount: null, reconciled_withdrawal_amount: money('0'), withdrawability: 'UNVERIFIED' },
        action_availability: { PAUSE: { allowed: true, reason: null }, CANCEL: { allowed: true, reason: null } }, progress: [], events: [], created_at: new Date().toISOString(), updated_at: new Date().toISOString() };
      receipts.set(requestId, json({ ...envelope, requestId, command: { preview_id: 'retained-original-preview' }, status: 'ACCEPTED', result: current }));
    },
    get current() { return current; }, advanceObservation() { observationId = 'observation-later'; }, blockQuotes() { blocked = true; },
    loseStart() { loseNextStart = true; allowReceipt = false; }, revealReceipt() { allowReceipt = true; }, close: () => server.close() };
}

test('account-only observation reads retained capital facts and can stop without a new plan', { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = await protocolReceiver();
  remote.retainOriginal();
  try {
    state.options.config.jobBinary = join(state.directory, 'missing-science-worker'); state.options.config.connections = [remote.connection]; await state.restart();
    const bindingRequest = { requestId: 'existing-account', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: remote.connection.id };
    const created = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: bindingRequest });
    assert.equal(created.statusCode, 202, created.body); const binding = created.json(); const path = '/api/v3/observations/' + binding.id + '/capital-exits';
    assert.equal(binding.portfolioId, null); assert.equal(binding.receipt, null); assert.equal(binding.purpose, 'ACCOUNT_ONLY'); assert.equal(binding.equity, '1000');
    const fact = await state.service.work.getObservation(binding.id); assert.equal(fact.target, null); assert.equal(fact.view.receipt, null);
    assert.deepEqual((await state.service.research.harness.snapshot(Application, context))!.portfolios, {});
    const initialControls = (await state.service.research.harness.snapshot(Application, context))!.accountControls!;
    const initialMonitorId = control(initialControls[accountKey(remote.connection, initialControls)]).monitorTaskId!;
    assert.ok(initialMonitorId, 'Existing obligation keeps an original read monitor.');
    const repeated = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: bindingRequest });
    assert.equal(repeated.statusCode, 202, repeated.body); assert.equal(repeated.json().id, binding.id);
    assert.equal((await state.service.app.inject({ url: path + '/planning-context', headers: state.headers })).statusCode, 400);
    assert.equal((await state.service.app.inject({ method: 'POST', url: path + '/preview', headers: state.headers, payload: { requestId: 'removed-preview' } })).statusCode, 404);
    const refused = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: { requestId: 'new-start', action: 'START' } });
    assert.equal(refused.statusCode, 410); assert.equal(remote.startPosts.length, 0); assert.equal(remote.previewPosts.length, 0); assert.deepEqual(remote.targetPosts, []);
    const detail = await state.service.app.inject({ url: path + '/' + remote.current!.id, headers: state.headers }); assert.equal(detail.statusCode, 200, detail.body);
    assert.equal(detail.json().funds.reserved_amount.amount, '100');
    remote.blockQuotes();
    const blocked = await state.service.app.inject({ url: path, headers: state.headers }); assert.equal(blocked.json().capability, 'BLOCKED');
    const paused = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers,
      payload: { requestId: 'pause-despite-market', action: 'PAUSE', exitId: remote.current!.id, expectedRevision: remote.current!.revision } });
    assert.equal(paused.statusCode, 202, paused.body);
    await until(async () => remote.actionPosts.length, value => value === 1);
    assert.equal(object(remote.actionPosts[0]!.command).action, 'PAUSE');
    remote.current!.funds.reserved_amount = money('99');
    const invalid = await state.service.app.inject({ url: path + '/' + remote.current!.id, headers: state.headers });
    assert.equal(invalid.statusCode, 422, invalid.body); assert.match(invalid.body, /capital_obligation_mismatch/);
  } finally { await state.cleanup(); await remote.close(); }
});

test('a lost capital POST resumes its original Durable request and never sends a replacement command', { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = await protocolReceiver();
  remote.retainOriginal();
  try {
    state.options.config.jobBinary = join(state.directory, 'missing-science-worker'); state.options.config.connections = [remote.connection]; await state.restart();
    const created = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'account-for-lost-response', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: remote.connection.id } });
    assert.equal(created.statusCode, 202, created.body); const path = '/api/v3/observations/' + created.json().id + '/capital-exits';
    const request = { requestId: 'original-lost-start', action: 'START', previewId: 'retained-original-preview' };
    remote.loseStart();
    // Persist the original uncertain command in the real existing Doc; do not issue a retired START.
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); const key = accountKey(remote.connection, app.accountControls); const retained = control(app.accountControls![key]);
      const taskId = await tx.createTask(CapitalExitCommand, { key, requestId: request.requestId, connection: remote.connection, reconcile: true, binary: state.options.config.jobBinary, dataDir: state.options.config.dataDir }, { conversationId: retained.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
      retained.commands[request.requestId] = { original: request, body: { preview_id: request.previewId }, path: 'capital-exits', reserved: '100', state: 'UNCERTAIN', result: null, error: 'original_reply_unknown', taskId };
      app.accountControls![key] = json(retained);
    }, context);
    // Reopen the existing SQLite state so recovery, not new command admission, owns the retained task.
    await state.restart();
    const original = await until(async () => {
      const app = (await state.service.research.harness.snapshot(Application, context))!;
      return control(app.accountControls![accountKey(remote.connection, app.accountControls)]).commands[request.requestId];
    }, value => value?.state === 'UNCERTAIN' && remote.receiptGets.length > 0);
    assert.equal(original.reserved, '100'); assert.equal(remote.startPosts.length, 0);
    const taskId = original.taskId; await state.restart(); remote.revealReceipt();
    const settled = await until(async () => {
      const app = (await state.service.research.harness.snapshot(Application, context))!;
      return control(app.accountControls![accountKey(remote.connection, app.accountControls)]).commands[request.requestId];
    }, value => value?.state === 'ACCEPTED');
    assert.equal(settled.taskId, taskId); assert.equal((await state.service.research.harness.getTask(taskId as TaskId, context))!.state.status, 'terminal');
    const retry = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: request });
    assert.equal(retry.statusCode, 202, retry.body); assert.equal(retry.json().state, 'ACCEPTED');
    assert.equal(remote.startPosts.length, 0); assert.ok(remote.receiptGets.length > 0); assert.ok(remote.receiptGets.every(id => id === request.requestId));
    assert.deepEqual(remote.targetPosts, []);
  } finally { await state.cleanup(); await remote.close(); }
});


test('native preview service binds observation identity and never admits trading', { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = await protocolReceiver();
  try {
    state.options.config.connections = [remote.connection]; await state.restart();
    const created = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'native-preview-account', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: remote.connection.id } });
    assert.equal(created.statusCode, 202, created.body);
    const path = '/api/v3/observations/' + created.json().id;
    const sizing = { sizing: { instrument_id: 'BTC/USDC.HYPERLIQUID', entry: '100.000000000000000001', stop_loss: '95', equity: '1000', currency: 'USDC', risk: '0.01', commission_rate: '0', exchange_rate: '1', unit_batch_size: '0' } };
    const order = { instrument_id: 'BTC/USDC.HYPERLIQUID', order: { kind: 'STOP_LOSS', side: 'SELL', quantity: '0.00000001', trigger_price: '95' } };
    const call = (suffix: string, payload: JsonObject, headers = state.headers) => state.service.app.inject({ method: 'POST', url: path + suffix, headers, payload });
    const size = await call('/sizing/fixed-risk', sizing); assert.equal(size.statusCode, 200, size.body);
    assert.equal(size.json().account_balance_verified, false); assert.equal(size.json().quantity, '0.00000001');
    assert.deepEqual(remote.nativePreviewPosts[0]!.body, { ...sizing, clientId: remote.connection.clientId, accountId: remote.connection.accountId });
    const preview = await call('/orders/preview', order); assert.equal(preview.statusCode, 200, preview.body); assert.equal(preview.json().submitted, false);
    const before = remote.nativePreviewPosts.length;
    assert.equal((await call('/orders/preview', order, {} as typeof state.headers)).statusCode, 401);
    assert.notEqual((await call('/orders/preview', { ...order, accountId: 'different' })).statusCode, 200);
    assert.equal(remote.nativePreviewPosts.length, before);
    for (const extra of [{ operation: 'TARGET' }, { operation: 'CONTROL' }, { operation: 'SHUTDOWN' }, { operation: 'FIXED_RISK_SIZE' }, { requestId: 'forged' }, { extra: 'field' }] as JsonObject[]) {
      const rejected = await call('/sizing/fixed-risk', { sizing: { ...sizing.sizing, ...extra } });
      assert.equal(rejected.statusCode, 400, rejected.body);
    }
    assert.equal(remote.nativePreviewPosts.length, before);

    remote.malformedResponse(); assert.equal((await call('/orders/preview', order)).statusCode, 502);
    remote.submittedResponse(); assert.equal((await call('/orders/preview', order)).statusCode, 502);
    remote.wrongIdentity(); assert.equal((await call('/sizing/fixed-risk', sizing)).statusCode, 409);
    assert.deepEqual(remote.targetPosts, []); assert.deepEqual(remote.startPosts, []); assert.deepEqual(remote.actionPosts, []);
  } finally { await state.cleanup(); await remote.close(); }
});
