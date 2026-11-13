import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as sleep } from 'node:timers/promises';
import Fastify from 'fastify';
import { Application, decimalUnits, json, nowNs, observation } from '../src/facts.ts';
import { accountKey, capitalStatus, control } from '../src/capital-exit.ts';
import { built, context, setup } from './support.ts';
import { AdjustAccount, unsettledAdjustment } from '../src/automatic.ts';
import type { TaskId } from '@earendil-works/pi-durable';

const id = (value: string) => `01900000-0000-7000-8000-${value.padStart(12, '0')}`;
async function until<T>(read: () => Promise<T>, ready: (value: T) => boolean): Promise<T> {
  const end = Date.now() + 14_000; let value: T;
  do { value = await read(); if (ready(value)) return value; await sleep(40); } while (Date.now() < end);
  assert.fail(`condition not reached: ${JSON.stringify(value)}`);
}

// Controlled HTTP transport with the actual native capital-contract validator.
// Receiver/native owner execution is covered separately; this establishes service budget and receipt truth, not fills.
const capitalCapability = (supported = true, readable = true) => ({ version: 2, supported, readable,
  policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'],
  actions: ['START', 'PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION', 'RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'] });
test('account-only capital bridge retains exact reservations, cancellation and uncertain command identities across restart', { timeout: 120_000 }, async () => {
  assert.ok(process.env.QZ_JOB_BINARY);
  const state = await setup(); const remote = Fastify();
  let sequence = 0n; let equity = '600'; let reservation: string | null = '0'; let controlRevision = '7'; let support = true; let execution = true; let readable = true; let observationId = id('2'); let positionsEvidence: 'known' | 'missing' | 'unvalued' = 'known';
  let losePreview = true; const previewRequests: Record<string, any>[] = []; const nativePreviews = new Map<string, Record<string, any>>();
  let capitalPosts = 0; let targetPosts = 0; let allowReceipt = false; let publishExit = false; let stale = false;
  let preview!: Record<string, any>; let exit!: Record<string, any>; let commandEnvelope!: Record<string, any>;
  const money = (amount: string) => ({ amount, currency: 'USD' });
  const status = () => ({ clientId: 'protocol-client', accountId: 'protocol-account', mode: 'paper', currency: 'USD', accountSourceId: id('1'), observationId, controlRevision,
    reserved: reservation, exits: publishExit ? [exit] : [], capabilities: { capitalExit: capitalCapability(support, readable), automaticTargets: { supported: execution, revisionBound: true, snapshotBound: true } } });
  remote.get('/capital-exits', status);
  remote.get('/capabilities', () => ({ clientId: 'protocol-client', accountId: 'protocol-account', mode: 'paper', currency: 'USD', capitalExit: capitalCapability(support, readable), automaticTargets: { supported: execution, revisionBound: true, snapshotBound: true } }));
  remote.get('/capital-exits/planning-context', () => ({ clientId: 'protocol-client', accountId: 'protocol-account', mode: 'paper', currency: 'USD',
    context: { schema_version: 2, id: 'planning-' + observationId + '-' + controlRevision, account_source_id: id('1'), observation_id: observationId,
      account_control_revision: controlRevision, currency: 'USD', environment: 'PAPER', asof: new Date().toISOString(), valid_until: new Date(Date.now() + 300_000).toISOString(),
      funds: { total_cash: money(equity), free_cash: money(equity), locked_cash: money('0'), verified_withdrawable: null },
      instruments: [], capability: 'SUPPORTED', reason_codes: [], supported_policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'] } }));
  remote.get('/account', async () => { const at = String(BigInt(nowNs()) - (stale ? 200_000_000_000n : 0n)); return { clientId: 'protocol-client', accountId: 'protocol-account', mode: 'paper', currency: 'USD', accountSourceId: id('1'), observationId, controlRevision,
    sequence: String(++sequence), nativeClockNs: at, equity, cash: equity, ...(String(positionsEvidence) === 'missing' ? {} : { positions: positionsEvidence === 'known' ? [] : [{ instrumentId: 'TEST.SIM', quantity: '1', value: null, pnl: null }] }), execution: { currency: 'USD', validUntilNs: String(BigInt(at) + 120_000_000_000n), liquidity: [{ instrumentId: 'TEST.SIM', notional: '100000', lotNotional: '1', minimumNotional: '1', takerFee: '0.01', currency: 'USD', eventNs: at, availableNs: at }] } }; });
  remote.post('/targets', (_req, reply) => { targetPosts++; return reply.code(410).send({ error: 'retired_target_must_not_be_called' }); });
  remote.post('/capital-exits/preview', async (req, reply) => {
    const body = req.body as any; previewRequests.push(body);
    if (nativePreviews.has(body.requestId)) { assert.deepEqual(body, nativePreviews.get(body.requestId)!.envelope); return { ...body, preview: nativePreviews.get(body.requestId)!.preview }; }
    preview = { schema_version: 2, id: id(String(3 + nativePreviews.size * 10)), account_source_id: id('1'),
      original_observation_id: body.request.expected_source_observation_id, environment: 'PAPER', expected_account_control_revision: controlRevision,
      scope: body.request.scope, policy: body.request.policy, plan_artifact_id: id('5'), valid_until: new Date(Date.now() + 300_000).toISOString(),
      funds: { requested: money(body.request.scope.amount), native_total_cash: money(equity), native_free_cash: money(equity), native_locked_cash: money('0'),
        remaining_managed_capital: money(equity), estimated_execution_cost: money('0'), existing_unrealized_pnl: null },
      proposed_cancellations: [], retained_protective_orders: [], reduction_legs: [], capability: 'SUPPORTED', reason_codes: [] };
    nativePreviews.set(body.requestId, { envelope: body, preview });
    if (losePreview) { losePreview = false; reply.hijack(); reply.raw.destroy(); return; }
    return { ...body, preview };
  });
  remote.post('/capital-exits', async (req, reply) => {
    capitalPosts++; commandEnvelope = req.body as any;
    exit = { schema_version: 2, id: id('6'), account_source_id: id('1'), environment: 'PAPER', account_control_revision: '8',
      revision: '1', state: 'WAITING_CASH', preparation_state: 'WAITING_CASH', reason_codes: [], scope: preview.scope, policy: preview.policy, command_id: id('7'),
      remaining_trading: 'FENCED_PENDING_TARGET', funds: { requested_amount: money(preview.scope.amount), reserved_amount: money(preview.scope.amount),
        staged_for_withdrawal_amount: money('0'), waived_amount: money('0'), released_cash_amount: null, verified_withdrawable_amount: null,
        reconciled_withdrawal_amount: money('0'), evidence_asof: null, evidence_valid_until: null, withdrawability: 'SIMULATED' },
      action_availability: { PAUSE: { allowed: true, reason: null }, CANCEL: { allowed: true, reason: null } },
      progress: [], events: [], created_at: new Date().toISOString(), updated_at: new Date().toISOString() };
    reply.hijack(); reply.raw.destroy(); // Applied remotely, deliberately unknown locally: never repeat this POST.
  });
  remote.get('/capital-exits/commands/:id', async (_req, reply) => allowReceipt ? { ...commandEnvelope, status: 'ACCEPTED', result: exit } : reply.code(404).send({ error: 'not_observed' }));
  remote.post('/capital-exits/:id/actions', async req => {
    const body = req.body as any; const action = body.command.action;
    exit = { ...exit, command_id: id(action === 'PAUSE' ? '8' : '9'), revision: String(BigInt(exit.revision) + 1n),
      account_control_revision: controlRevision = String(BigInt(controlRevision) + 1n),
      state: action === 'PAUSE' ? 'PAUSED' : 'CANCELLED_RESERVED', remaining_trading: 'FENCED_PENDING_TARGET',
      action_availability: { PAUSE: { allowed: false, reason: 'already_stopped' }, CANCEL: { allowed: action !== 'CANCEL', reason: action === 'CANCEL' ? 'already_cancelled' : null } } };
    return { ...body, status: 'ACCEPTED', result: exit };
  });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  try {
    state.options.config.jobBinary = process.env.QZ_JOB_BINARY!;
    const connection = { id: 'protocol', name: 'Controlled protocol, not installed capability', mode: 'paper' as const, url, clientId: 'protocol-client', accountId: 'protocol-account' };
    const aliasConnection = { ...connection, id: 'native-owner-alias', name: 'Same native source via another URL', url: `${url}?alias=1` };
    state.options.config.connections = [connection, aliasConnection]; await state.restart();
    const started = await state.service.work.startObservation({ requestId: 'account-original', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: connection.id });
    const alias = await state.service.work.startObservation({ requestId: 'account-alias', purpose: 'ACCOUNT_ONLY', mode: 'paper', connectionId: aliasConnection.id });
    const aliasId = alias.id;
    const controls = (await state.service.research.harness.snapshot(Application, context))!.accountControls!;
    assert.equal(accountKey(aliasConnection, controls), accountKey(connection, controls));
    assert.equal(started.receipt, null); assert.equal(alias.receipt, null); assert.equal(targetPosts, 0);
    const path = `/api/v3/observations/${started.id}/capital-exits`;
    const firstContext = await state.service.app.inject({ url: path + '/planning-context', headers: state.headers }); assert.equal(firstContext.statusCode, 200, firstContext.body);
    const lostPreview = { requestId: 'preview-lost-original', amount: '10', currency: 'USD', contextId: firstContext.json().id, policy: { kind: 'CASH_ONLY' } };
    const lost = await state.service.app.inject({ method: 'POST', url: `${path}/preview`, headers: state.headers, payload: lostPreview }); assert.ok(lost.statusCode >= 500, lost.body);
    observationId = id('12');
    const recoveredPreview = await state.service.app.inject({ method: 'POST', url: `${path}/preview`, headers: state.headers, payload: lostPreview }); assert.equal(recoveredPreview.statusCode, 200, recoveredPreview.body);
    assert.deepEqual(previewRequests[0], previewRequests[1]); assert.equal(recoveredPreview.json().original_observation_id, id('2'));
    assert.equal((await state.service.app.inject({ method: 'POST', url: `${path}/preview`, headers: state.headers, payload: { ...lostPreview, amount: '11' } })).statusCode, 409);
    const currentContext = await state.service.app.inject({ url: path + '/planning-context', headers: state.headers }); assert.equal(currentContext.statusCode, 200, currentContext.body);
    const request = { requestId: 'preview-exact', amount: '100.000000000000000001', currency: 'USD', contextId: currentContext.json().id,
      policy: { kind: 'NATURAL_RELEASE', deadline: new Date(Date.now() + 300_000).toISOString() } };
    const previewResult = await state.service.app.inject({ method: 'POST', url: `${path}/preview`, headers: state.headers, payload: request }); assert.equal(previewResult.statusCode, 200, previewResult.body);
    assert.equal(previewResult.json().funds.requested.amount, request.amount);
    const command = { requestId: 'start-exact', action: 'START', previewId: previewResult.json().id };
    const submitted = await state.service.app.inject({ method: 'POST', url: `${path}/commands`, headers: state.headers, payload: command }); assert.equal(submitted.statusCode, 202, submitted.body);
    const controlState = async () => { const records = (await state.service.research.harness.snapshot(Application, context))!.accountControls!; return control(records[accountKey(state.options.config.connections[0]!, records)]); };
    await until(controlState, value => value.commands['start-exact']?.state === 'UNCERTAIN' && value.commands['start-exact']?.error === 'capital_exit_command_requires_reconciliation');
    assert.equal(capitalStatus(await controlState()).reserved, request.amount); assert.equal(capitalPosts, 1);
    const aliasReserve = await state.service.app.inject({ url: `/api/v3/observations/${aliasId}/capital-exits`, headers: state.headers }); assert.equal(aliasReserve.json().reserved, request.amount);
    await state.restart(); await sleep(100);
    const replay = await state.service.app.inject({ method: 'POST', url: `${path}/commands`, headers: state.headers, payload: command }); assert.equal(replay.statusCode, 202, replay.body);
    assert.equal(capitalPosts, 1); assert.equal((await state.service.app.inject({ method: 'POST', url: `${path}/commands`, headers: state.headers, payload: { ...command, previewId: id('99') } })).statusCode, 409);
    allowReceipt = true;
    await until(controlState, value => value.commands['start-exact']?.state === 'ACCEPTED'); assert.equal(capitalPosts, 1); assert.equal(capitalStatus(await controlState()).reserved, request.amount);
    publishExit = true; reservation = request.amount; controlRevision = '8';
    const nativeStatus = await state.service.app.inject({ url: path, headers: state.headers }); assert.equal(nativeStatus.statusCode, 200, nativeStatus.body); assert.equal(nativeStatus.json().reserved, request.amount); assert.equal(nativeStatus.json().exits[0].funds.withdrawability, 'SIMULATED');
    for (const action of ['PAUSE', 'CANCEL']) {
      const actionId = `action-${action}`; const response = await state.service.app.inject({ method: 'POST', url: `${path}/commands`, headers: state.headers, payload: { requestId: actionId, action, exitId: exit.id, expectedRevision: exit.revision } }); assert.equal(response.statusCode, 202, response.body);
      await until(controlState, value => value.commands[actionId]?.state === 'ACCEPTED');
      const status = (await state.service.app.inject({ url: path, headers: state.headers })).json(); assert.equal(status.reserved, request.amount); assert.equal(status.exits[0].state, action === 'PAUSE' ? 'PAUSED' : 'CANCELLED_RESERVED');
    }
    reservation = null;
    const unknown = (await state.service.app.inject({ url: path, headers: state.headers })).json();
    assert.equal(unknown.reserved, null); assert.equal(targetPosts, 0);
    support = false; reservation = request.amount; equity = '3000';
    const readOnly = (await state.service.app.inject({ url: path, headers: state.headers })).json();
    assert.equal(readOnly.capability, 'UNSUPPORTED'); assert.equal(readOnly.exits.length, 1); assert.equal(readOnly.reserved, request.amount);
    readable = false; const historical = (await state.service.app.inject({ url: path, headers: state.headers })).json();
    assert.equal(historical.capability, 'BLOCKED'); assert.equal(historical.reason, 'downstream_capital_exit_unsupported');
    assert.equal(historical.reserved, null); assert.equal(historical.exits.length, 1); assert.equal(historical.exits[0].id, exit.id);
    assert.equal(targetPosts, 0, 'No capital exit state may regenerate the retired automatic target.');

  } finally { await state.cleanup(); await remote.close(); }
});


// Original target authority has no equivalent in native START/STOP. These
// retained facts exercise only recovery/retirement, never new signal generation.
async function retainedProtocol(mode: 'paper' | 'live', allocated: boolean) {
  const state = await setup(), remote = Fastify(); const queries: string[] = []; let posts = 0, available = false, mismatch = false, reserved = '100.000000000000000001', revision = '7', sequence = 9007199254740993n;
  const identity = { clientId: 'retained-client', accountId: 'retained-account', mode, currency: 'USD' };
  const target = { schema_version: 1, asof_ns: nowNs(), valid_until_ns: String(BigInt(nowNs()) + 300_000_000_000n), cash_weight: '1', targets: [] };
  const prepared = { schemaVersion: 1, controlRevision: '7', snapshotSequence: '9007199254740993', reserved,
    equity: '600', budget: '449.999999999999999999', capital: '449.999999999999999999', cashBuffer: '50', factors: [] };
  const receipt = { ...identity, id: 'original-before-upgrade', expiresAt: new Date(Date.now() + 300_000).toISOString(), externalId: 'original-remote-id', acknowledgedAt: null, status: 'ACKNOWLEDGED', allocation: prepared };
  remote.get('/capabilities', () => ({ ...identity, capitalExit: capitalCapability() }));
  remote.get('/capital-exits', () => ({ ...identity, accountSourceId: id('1'), observationId: id('2'), controlRevision: revision, reserved, exits: [] }));
  remote.get('/account', () => ({ ...identity, sequence: String(sequence), nativeClockNs: String(sequence), accountSourceId: id('1'), observationId: id('2'), controlRevision: revision, equity: '600', cash: '600', positions: [] }));
  remote.post('/targets', (_req, reply) => { posts++; return reply.code(410).send({ error: 'retired_target_must_not_be_called' }); });
  remote.get<{ Params: { id: string } }>('/targets/:id', (request, reply) => {
    queries.push(request.params.id);
    return available ? { ...receipt, ...(mismatch ? { allocation: { ...prepared, reserved: '0' }, accountId: allocated ? identity.accountId : 'other-account' } : {}) }
      : reply.code(404).send({ error: 'original_receipt_unknown' });
  });
  const connection = { ...identity, id: 'retained', name: 'Retained transport protocol, not a fill claim', url: await remote.listen({ host: '127.0.0.1', port: 0 }) };
  const source = await built(state); state.options.config.connections = [connection]; await state.restart();
  const original = await state.service.work.startObservation({ requestId: 'account-only', purpose: 'ACCOUNT_ONLY', mode, connectionId: connection.id });
  await state.service.research.harness.waitForTask((await state.service.work.getObservation(original.id)).taskId as TaskId, context);
  await state.service.research.harness.commit(async tx => {
    const app = await tx.doc(Application), fact = observation(app.observations[original.id]);
    fact.input.portfolioId = source.view.id; fact.target = target;
    fact.view.receipt = { id: allocated ? 'never-offered-base' : receipt.id, status: allocated ? 'NOT_OFFERED' : 'UNCERTAIN', expiresAt: receipt.expiresAt, externalId: null, acknowledgedAt: null };
    if (allocated) {
      fact.view.automaticPolicy = { enabled: false, cashBuffer: '50', maximumSnapshotAgeSeconds: 60 };
      fact.allocationContext = prepared; fact.allocationTarget = target;
      fact.view.allocation = { status: 'UNCERTAIN', reason: 'original_response_lost', budget: prepared.budget, equity: '600', capital: prepared.capital, reserved,
        receipt: { id: receipt.id, status: 'UNCERTAIN', expiresAt: receipt.expiresAt, externalId: null, acknowledgedAt: null }, members: [] } as any;
    }
    app.observations[original.id] = json(fact);
  }, context);
  return { state, connection, original, source, prepared, target, receipt, queries, get posts() { return posts; },
    allow(value = true) { available = value; }, mismatch(value: boolean) { mismatch = value; }, reserve(value: string) { reserved = value; revision = String(BigInt(revision) + 1n); sequence++; },
    async read() { return state.service.work.getObservation(original.id); }, async cleanup() { await state.cleanup(); await remote.close(); } };
}

for (const mode of ['paper', 'live'] as const) for (const allocated of [false, true]) test(`retained ${allocated ? 'allocation' : 'manual'} receipt in ${mode} survives uncertainty, identity mismatch and restart without another POST`, { timeout: 30_000 }, async () => {
  const p = await retainedProtocol(mode, allocated);
  try {
    const readReceipt = async () => { const fact = await p.read(); return allocated ? fact.view.allocation!.receipt! : fact.view.receipt!; };
    await p.state.service.work.refreshObservation(p.original.id);
    assert.equal((await readReceipt()).status, 'UNCERTAIN'); assert.equal(p.posts, 0); assert.ok(p.queries.every(id => id === p.receipt.id));
    await p.state.restart(); p.reserve('200.000000000000000001'); p.allow(); p.mismatch(true);
    await p.state.service.work.refreshObservation(p.original.id);
    assert.equal((await readReceipt()).status, 'UNCERTAIN'); assert.equal((await readReceipt()).id, p.receipt.id);
    if (allocated) assert.deepEqual((await p.read()).allocationContext, p.prepared, 'New reservation facts cannot rewrite the original sent context.');
    p.mismatch(false); await p.state.service.work.refreshObservation(p.original.id);
    assert.equal((await readReceipt()).status, 'ACKNOWLEDGED'); assert.equal((await readReceipt()).externalId, p.receipt.externalId);
    assert.equal(p.posts, 0); assert.ok(p.queries.every(id => id === p.receipt.id));
    if (allocated) { assert.equal((await p.read()).view.allocation!.budget, '449.999999999999999999'); assert.deepEqual((await p.read()).allocationContext, p.prepared); }
    const disabled = await p.state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${p.original.id}/automatic`, headers: p.state.headers, payload: { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 } });
    assert.equal(disabled.statusCode, 410); assert.equal(p.posts, 0);
    await p.state.restart(); assert.equal((await readReceipt()).id, p.receipt.id); assert.equal((await readReceipt()).status, 'ACKNOWLEDGED');
  } finally { await p.cleanup(); }
});

for (const phase of ['snapshot', 'deliver', 'abort'] as const) test(`retained unsent allocation ${phase} never sends or re-sizes and preserves its exact money/context`, async () => {
  const p = await retainedProtocol('paper', true);
  try {
    await p.state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application), fact = observation(app.observations[p.original.id]);
      fact.view.allocation!.status = 'SIZED'; fact.view.allocation!.receipt!.status = 'PENDING'; fact.view.automaticPolicy!.enabled = true;
      app.observations[p.original.id] = json(fact);
    }, context);
    await p.state.restart();
    const task: any = { id: 0, conversationId: p.source.view.researchIds[0], input: { id: p.original.id, binary: '/must-not-execute', dataDir: p.state.directory } };
    const runtime: any = { signal: new AbortController().signal, commit: (operation: any) => p.state.service.research.harness.commit(operation, context) };
    if (phase === 'abort') await AdjustAccount.definition.abort!(task, runtime, context);
    else await AdjustAccount.definition.phases[phase](task, runtime, context);
    const fact = await p.read(); assert.equal(fact.view.allocation!.receipt!.status, 'NOT_OFFERED'); assert.equal(fact.view.allocation!.status, 'STOPPED');
    assert.equal(unsettledAdjustment(fact), false); assert.deepEqual(fact.allocationContext, p.prepared); assert.deepEqual(fact.allocationTarget, p.target);
    assert.equal(fact.view.allocation!.budget, '449.999999999999999999'); assert.equal(p.posts, 0); assert.deepEqual(p.queries, []);
    await p.state.restart(); assert.equal((await p.read()).view.allocation!.receipt!.status, 'NOT_OFFERED'); assert.equal(p.posts, 0);
  } finally { await p.cleanup(); }
});

test('aborting an uncertain retained allocation schedules original receipt recovery, not remote cancellation or a new target', { timeout: 30_000 }, async () => {
  const p = await retainedProtocol('paper', true);
  try {
    const task: any = { id: 0, conversationId: p.source.view.researchIds[0], input: { id: p.original.id, binary: '/must-not-execute', dataDir: p.state.directory } };
    const runtime: any = { signal: new AbortController().signal, commit: (operation: any) => p.state.service.research.harness.commit(operation, context) };
    await AdjustAccount.definition.abort!(task, runtime, context);
    const recovering = await until(() => p.read(), fact => fact.view.allocation?.reason === 'automatic_receipt_requires_reconciliation' || p.queries.length > 0);
    assert.equal(recovering.view.allocation!.receipt!.status, 'UNCERTAIN'); assert.equal(unsettledAdjustment(recovering), true);
    assert.equal(recovering.view.allocation!.receipt!.id, p.receipt.id); assert.equal(p.posts, 0);
    await p.state.restart(); p.allow();
    const settled = await until(() => p.read(), fact => fact.view.allocation?.receipt?.status === 'ACKNOWLEDGED');
    assert.equal(settled.view.allocation!.receipt!.id, p.receipt.id); assert.deepEqual(settled.allocationContext, p.prepared);
    assert.equal(p.posts, 0); assert.ok(p.queries.every(id => id === p.receipt.id));
  } finally { await p.cleanup(); }
});
