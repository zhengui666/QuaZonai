import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import Fastify from 'fastify';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { CapitalExitPlanningContext, CapitalExitPreview, CapitalExitPolicy, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import type { ConnectionConfig } from '../src/config.ts';
import { accountKey, control } from '../src/capital-exit.ts';
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
  const url = await server.listen({ host: '127.0.0.1', port: 0 });
  const connection: ConnectionConfig = { id: 'capital-connection', name: 'Existing cash account', url, clientId: envelope.clientId, accountId: envelope.accountId, mode: envelope.mode };
  return { connection, plan, previewPosts, startPosts, actionPosts, receiptGets, targetPosts,
    get current() { return current; }, advanceObservation() { observationId = 'observation-later'; }, blockQuotes() { blocked = true; },
    loseStart() { loseNextStart = true; allowReceipt = false; }, revealReceipt() { allowReceipt = true; }, close: () => server.close() };
}

test('account-only capital preparation uses local contracts, retained planning observations, details and independent stop capability', { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = await protocolReceiver();
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
    await until(async () => (await state.service.research.harness.getTask(initialMonitorId as TaskId, context))!.state.status, value => value === 'terminal');
    const repeated = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: bindingRequest });
    assert.equal(repeated.statusCode, 202, repeated.body); assert.equal(repeated.json().id, binding.id);
    const planned = await state.service.app.inject({ url: path + '/planning-context', headers: state.headers }); assert.equal(planned.statusCode, 200, planned.body);
    assert.equal(planned.json().instruments[0].instrument_id, 'BTC/USDC.HYPERLIQUID');
    remote.advanceObservation();
    const previewRequest = { requestId: 'natural-preview', amount: '100', currency: 'USDC', contextId: remote.plan.id,
      policy: { kind: 'NATURAL_RELEASE', deadline: new Date(Date.now() + 60_000).toISOString() } };
    const preview = await state.service.app.inject({ method: 'POST', url: path + '/preview', headers: state.headers, payload: previewRequest });
    assert.equal(preview.statusCode, 200, preview.body); assert.equal(preview.json().original_observation_id, 'observation-original');
    assert.equal(object(remote.previewPosts[0]!.request).expected_source_observation_id, 'observation-original');
    const started = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers,
      payload: { requestId: 'natural-start', action: 'START', previewId: preview.json().id } });
    assert.equal(started.statusCode, 202, started.body);
    await until(async () => {
      const app = (await state.service.research.harness.snapshot(Application, context))!;
      return control(app.accountControls![accountKey(remote.connection, app.accountControls)]).commands['natural-start']?.state;
    }, value => value === 'ACCEPTED');
    assert.equal(remote.startPosts.length, 1); assert.deepEqual(remote.targetPosts, []);
    const startedControls = (await state.service.research.harness.snapshot(Application, context))!.accountControls!;
    const activeMonitorId = control(startedControls[accountKey(remote.connection, startedControls)]).monitorTaskId!;
    assert.notEqual(activeMonitorId, initialMonitorId, 'START must retain its replacement reader after the account-only admission reader has completed.');
    assert.notEqual((await state.service.research.harness.getTask(activeMonitorId as TaskId, context))!.state.status, 'terminal');
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
  try {
    state.options.config.jobBinary = join(state.directory, 'missing-science-worker'); state.options.config.connections = [remote.connection]; await state.restart();
    const created = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers,
      payload: { requestId: 'account-for-lost-response', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: remote.connection.id } });
    assert.equal(created.statusCode, 202, created.body); const path = '/api/v3/observations/' + created.json().id + '/capital-exits';
    await state.service.app.inject({ url: path + '/planning-context', headers: state.headers });
    const preview = await state.service.app.inject({ method: 'POST', url: path + '/preview', headers: state.headers,
      payload: { requestId: 'cash-preview', amount: '100', currency: 'USDC', contextId: remote.plan.id, policy: { kind: 'CASH_ONLY' } } });
    assert.equal(preview.statusCode, 200, preview.body); remote.loseStart();
    const request = { requestId: 'original-lost-start', action: 'START', previewId: preview.json().id };
    const started = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: request });
    assert.equal(started.statusCode, 202, started.body);
    const original = await until(async () => {
      const app = (await state.service.research.harness.snapshot(Application, context))!;
      return control(app.accountControls![accountKey(remote.connection, app.accountControls)]).commands[request.requestId];
    }, value => value?.state === 'UNCERTAIN' && remote.startPosts.length === 1);
    const taskId = original.taskId; await state.restart(); remote.revealReceipt();
    const settled = await until(async () => {
      const app = (await state.service.research.harness.snapshot(Application, context))!;
      return control(app.accountControls![accountKey(remote.connection, app.accountControls)]).commands[request.requestId];
    }, value => value?.state === 'ACCEPTED');
    assert.equal(settled.taskId, taskId); assert.equal((await state.service.research.harness.getTask(taskId as TaskId, context))!.state.status, 'terminal');
    const retry = await state.service.app.inject({ method: 'POST', url: path + '/commands', headers: state.headers, payload: request });
    assert.equal(retry.statusCode, 202, retry.body); assert.equal(retry.json().state, 'ACCEPTED');
    assert.equal(remote.startPosts.length, 1); assert.ok(remote.receiptGets.length > 0); assert.ok(remote.receiptGets.every(id => id === request.requestId));
    assert.deepEqual(remote.targetPosts, []);
  } finally { await state.cleanup(); await remote.close(); }
});
