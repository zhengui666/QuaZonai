import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as sleep } from 'node:timers/promises';
import Fastify from 'fastify';
import { Application, decimalUnits, json, nowNs, observation, portfolio } from '../src/facts.ts';
import { accountKey, capitalStatus, control, readCapitalControl, recordCapitalControl } from '../src/capital-exit.ts';
import { built, context, setup } from './support.ts';
import { AdjustAccount, unsettledAdjustment } from '../src/automatic.ts';
import { ObserveAccount } from '../src/science.ts';
import { workbench } from '../src/workbench.ts';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const id = (value: string) => `01900000-0000-7000-8000-${value.padStart(12, '0')}`;
async function until<T>(read: () => Promise<T>, ready: (value: T) => boolean): Promise<T> {
  const end = Date.now() + 14_000; let value: T;
  do { value = await read(); if (ready(value)) return value; await sleep(40); } while (Date.now() < end);
  assert.fail(`condition not reached: ${JSON.stringify(value)}`);
}

// Controlled transport and explicitly TEST_ONLY source facts; actual native V1 parsing and sizing.
// Original native owner transition/fencing/withdrawability semantics remain in native_capital_exit.rs.
test('native V1 bridge and durable automatic allocation retain account-wide reserves, exact commands and uncertain identities across restart', { timeout: 120_000 }, async () => {
  assert.ok(process.env.QZ_JOB_BINARY);
  const state = await setup(); const remote = Fastify();
  let sequence = 0n; let equity = '600'; let reservation: string | null = '0'; let controlRevision = '7'; let support = true; let execution = true; let readable = true; let observationId = id('2'); let rejectNext = false; let positionsEvidence: 'known' | 'missing' | 'unvalued' = 'known';
  let losePreview = true; const previewRequests: Record<string, any>[] = []; const nativePreviews = new Map<string, Record<string, any>>(); const targetQueries: string[] = [];
  let capitalPosts = 0; let targetPosts = 0; let allowReceipt = false; let publishExit = false; let allowTargetReceipt = false; let stale = false;
  let preview!: Record<string, any>; let exit!: Record<string, any>; let commandEnvelope!: Record<string, any>; const receipts = new Map<string, any>();
  const money = (amount: string) => ({ amount, currency: 'USD' });
  const status = () => ({ clientId: 'protocol-client', accountId: 'protocol-account', mode: 'paper', currency: 'USD', accountSourceId: id('1'), observationId, controlRevision,
    reserved: reservation, exits: publishExit ? [exit] : [], capabilities: { capitalExit: { version: 1, supported: support }, automaticTargets: { supported: execution, revisionBound: true } } });
  remote.get('/capital-exits', status);
  remote.get('/capabilities', () => ({ clientId: 'protocol-client', accountId: 'protocol-account', mode: 'paper', currency: 'USD', capitalExit: { version: 1, supported: support, readable }, automaticTargets: { supported: execution, revisionBound: true } }));
  remote.get('/account', async () => { const at = String(BigInt(nowNs()) - (stale ? 200_000_000_000n : 0n)); return { clientId: 'protocol-client', accountId: 'protocol-account', mode: 'paper', currency: 'USD', accountSourceId: id('1'), observationId, controlRevision,
    sequence: String(++sequence), nativeClockNs: at, equity, cash: equity, ...(positionsEvidence === 'missing' ? {} : { positions: positionsEvidence === 'known' ? [] : [{ instrumentId: 'TEST.SIM', quantity: '1', value: null, pnl: null }] }), execution: { currency: 'USD', validUntilNs: String(BigInt(at) + 120_000_000_000n), liquidity: [{ instrumentId: 'TEST.SIM', notional: '100000', lotNotional: '1', minimumNotional: '1', takerFee: '0.01', currency: 'USD', eventNs: at, availableNs: at }] } }; });
  remote.post('/targets', async (req, reply) => { targetPosts++; const body = req.body as any; const receipt = { ...body, status: rejectNext ? 'REJECTED' : 'ACKNOWLEDGED', externalId: `controlled-${targetPosts}`, acknowledgedAt: new Date().toISOString() }; receipts.set(body.id, receipt); rejectNext = false; if (targetPosts === 1) { reply.hijack(); reply.raw.destroy(); return; } return receipt; });
  remote.get<{ Params: { id: string } }>('/targets/:id', async (req, reply) => { targetQueries.push(req.params.id); return allowTargetReceipt && receipts.has(req.params.id) ? receipts.get(req.params.id) : reply.code(404).send({ error: 'not_observed' }); });
  remote.post('/capital-exits/preview', async (req, reply) => {
    const body = req.body as any; previewRequests.push(body);
    if (nativePreviews.has(body.requestId)) { assert.deepEqual(body, nativePreviews.get(body.requestId)!.envelope); return { ...body, preview: nativePreviews.get(body.requestId)!.preview }; }
    preview = { schema_version: 1, id: id(String(3 + nativePreviews.size * 10)), project_id: id('4'), account_source_id: id('1'), original_observation_id: body.request.expected_source_observation_id, environment: 'PAPER', managed_account_key: 'protocol-account', owner_binding_ref: 'TEST_ONLY', expected_account_control_revision: controlRevision,
      scope: body.request.scope, policy: body.request.policy, plan_artifact_id: id('5'), evidence_refs: [], valid_until: new Date(Date.now() + 300_000).toISOString(),
      funds: { requested: money(body.request.scope.amount), native_total_cash: money(equity), native_free_cash: money(equity), native_locked_cash: money('0'), verified_idle_cash: null, estimated_release: null, native_equity: money(equity), managed_capital_before: money(equity), remaining_managed_capital: null, estimated_execution_cost: null, existing_unrealized_pnl: null },
      proposed_cancellations: [], retained_protective_orders: [], reduction_legs: [], remaining_risk_evidence_id: null, capability: 'SUPPORTED', reason_codes: [] };
    nativePreviews.set(body.requestId, { envelope: body, preview });
    if (losePreview) { losePreview = false; reply.hijack(); reply.raw.destroy(); return; }
    return { ...body, preview };
  });
  remote.post('/capital-exits', async (req, reply) => {
    capitalPosts++; commandEnvelope = req.body as any;
    exit = { schema_version: 1, id: id('6'), project_id: id('4'), account_source_id: id('1'), environment: 'PAPER', managed_account_key: 'protocol-account', owner_binding_ref: 'TEST_ONLY', account_control_epoch: '1', account_control_revision: '8', revision: '1', state: 'REQUESTED', last_phase: 'REQUESTED', reason_codes: [],
      preview_id: preview.id, plan_artifact_id: preview.plan_artifact_id, scope: preview.scope, policy: preview.policy, command_id: id('7'), owner_command: { schema_version: 1, command_id: id('7'), account_control_epoch: '1', instruction: { action: 'START' } }, external_claim_id: null,
      remaining_trading: 'FENCED_PENDING_TARGET', funds: { requested_amount: money(preview.scope.amount), reserved_amount: money(preview.scope.amount), released_cash_amount: null, verified_withdrawable_amount: null, reconciled_withdrawal_amount: money('0'), unreleased_amount: null, evidence_asof: null, evidence_valid_until: null, withdrawability: 'SIMULATED' }, evidence_refs: [], created_at: new Date().toISOString(), updated_at: new Date().toISOString() };
    reply.hijack(); reply.raw.destroy(); // Applied remotely, deliberately unknown locally: never repeat this POST.
  });
  remote.get('/capital-exits/commands/:id', async (_req, reply) => allowReceipt ? { ...commandEnvelope, status: 'ACCEPTED', result: exit } : reply.code(404).send({ error: 'not_observed' }));
  remote.post('/capital-exits/:id/actions', async req => {
    const body = req.body as any; const action = body.command.action;
    exit = { ...exit, command_id: id(action === 'PAUSE' ? '8' : '9'), revision: String(BigInt(exit.revision) + 1n), account_control_revision: controlRevision = String(BigInt(controlRevision) + 1n),
      state: action === 'PAUSE' ? 'PAUSED' : 'CANCELLED_RESERVED', last_phase: exit.state,
      owner_command: { schema_version: 1, command_id: id(action === 'PAUSE' ? '8' : '9'), account_control_epoch: '1', instruction: { action } } };
    return { ...body, status: 'ACCEPTED', result: exit };
  });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  try {
    const built_ = await built(state); state.options.config.jobBinary = process.env.QZ_JOB_BINARY!;
    state.options.config.connections.push({ id: 'protocol', name: 'Controlled protocol, not installed capability', mode: 'paper', url, clientId: 'protocol-client', accountId: 'protocol-account' });
    await state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[built_.view.id]);
      fact.view.factors = [{ factorId: `r${fact.view.researchIds[0]}.t11.m0`, weight: '1' }];
      fact.view.capacity = { status: 'ESTIMATED', reason: null, scope: 'TEST_ONLY', basis: 'explicit transport test input, not measured science', observationStartNs: null, observationEndNs: null, currency: 'USD', minimumCapital: '1', maximumCapital: '1000', bindingInstrument: 'TEST.SIM', decisionClockNs: nowNs(), validUntilNs: String(BigInt(nowNs()) + 300_000_000_000n),
        assumptions: { participation: '0.01', maximumAgeSeconds: 120, executionWindow: 'TEST_ONLY', exit: 'TEST_ONLY', fees: 'TEST_ONLY', depth: 'TEST_ONLY' }, scenarios: [] };
      fact.target!.valid_until_ns = String(BigInt(nowNs()) + 300_000_000_000n); facts.portfolios[built_.view.id] = json(fact);
    }, context);
    await state.restart();
    const started = await state.service.work.startObservation({ requestId: 'automatic-original', portfolioId: built_.view.id, mode: 'paper', connectionId: 'protocol', automatic: { enabled: true, cashBuffer: '50', maximumSnapshotAgeSeconds: 120 } });
    const read = async () => (await state.service.work.getObservation(started.id)).view;
    const uncertain = await until(read, value => value.allocation?.receipt?.status === 'UNCERTAIN' && targetPosts === 1);
    await state.service.work.configureAutomatic(started.id, { enabled: false, cashBuffer: '50', maximumSnapshotAgeSeconds: 120 });
    await state.restart();
    const blockedBinding = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: { requestId: 'blocked-unknown-target', portfolioId: built_.view.id, mode: 'paper', connectionId: 'protocol', automatic: { enabled: true, cashBuffer: '50', maximumSnapshotAgeSeconds: 120 } } }); assert.equal(blockedBinding.statusCode, 409, blockedBinding.body);
    const primary = (await state.service.work.getObservation(started.id)).taskId;
    const refreshBefore = targetQueries.length; await state.service.work.refreshObservation(started.id); assert.ok(targetQueries.length > refreshBefore); assert.equal(targetQueries.at(-1), uncertain.allocation!.receipt!.id); assert.equal(targetPosts, 1);
    assert.equal((await state.service.work.getObservation(started.id)).taskId, primary, 'a read-only refresh must retain the active background reconciliation task');
    allowTargetReceipt = true;
    const first = await until(read, value => value.allocation?.status === 'DELIVERED'); assert.equal(first.allocation!.receipt!.id, uncertain.allocation!.receipt!.id); assert.equal(first.automaticPolicy!.enabled, false);
    await state.service.research.harness.waitForTask((await state.service.work.getObservation(started.id)).taskId as import('@earendil-works/pi-durable').TaskId, context);
    await state.service.work.configureAutomatic(started.id, { enabled: true, cashBuffer: '50', maximumSnapshotAgeSeconds: 120 });
    assert.equal(first.receipt!.status, 'NOT_OFFERED'); assert.equal(first.allocation!.budget, '550'); assert.ok(decimalUnits(first.allocation!.members[0]!.capital) > 0n && decimalUnits(first.allocation!.members[0]!.capital) <= decimalUnits('550')); assert.equal(targetPosts, 1);
    const firstReceipt = first.allocation!.receipt!.id; const offered = receipts.get(firstReceipt); assert.equal(offered.allocation.controlRevision, '7'); assert.deepEqual(offered.allocation.factors, built_.view.factors ?? [{ factorId: `r${built_.view.researchIds[0]}.t11.m0`, weight: '1' }]);
    const aliasConnection = { ...state.options.config.connections[0]!, id: 'native-owner-alias', name: 'Same native source via another URL', url: `${url}?alias=1` };
    state.options.config.connections.push(aliasConnection); await state.restart();
    const alias = await state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: { requestId: 'original-alias-binding', portfolioId: built_.view.id, connectionId: aliasConnection.id, mode: 'paper', automatic: { enabled: true, cashBuffer: '50', maximumSnapshotAgeSeconds: 120 } } });
    assert.equal(alias.statusCode, 202, alias.body); const aliasId = alias.json().id;
    const duplicate = await until(() => state.service.work.getObservation(aliasId), fact => fact.view.allocation?.reason === 'account_automatic_binding_already_enabled');
    assert.equal(duplicate.view.automaticPolicy!.enabled, false); assert.equal(targetPosts, 1);
    const controls = (await state.service.research.harness.snapshot(Application, context))!.accountControls!;
    assert.equal(accountKey(aliasConnection, controls), accountKey(state.options.config.connections[0]!, controls));
    const path = `/api/v3/observations/${started.id}/capital-exits`;
    const lostPreview = { requestId: 'preview-lost-original', amount: '10', currency: 'USD' };
    const lost = await state.service.app.inject({ method: 'POST', url: `${path}/preview`, headers: state.headers, payload: lostPreview }); assert.ok(lost.statusCode >= 500, lost.body);
    observationId = id('12');
    const recoveredPreview = await state.service.app.inject({ method: 'POST', url: `${path}/preview`, headers: state.headers, payload: lostPreview }); assert.equal(recoveredPreview.statusCode, 200, recoveredPreview.body);
    assert.deepEqual(previewRequests[0], previewRequests[1]); assert.equal(recoveredPreview.json().original_observation_id, id('2'));
    assert.equal((await state.service.app.inject({ method: 'POST', url: `${path}/preview`, headers: state.headers, payload: { ...lostPreview, amount: '11' } })).statusCode, 409);
    const request = { requestId: 'preview-exact', amount: '100.000000000000000001', currency: 'USD' };
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
    await until(read, value => value.allocation?.reason === 'capital_exit_fenced_pending_target');
    const postsBefore = targetPosts; reservation = null;
    await until(read, value => value.allocation?.reason === 'account_reservation_unknown'); assert.equal(targetPosts, postsBefore);
    support = false; reservation = request.amount; exit.remaining_trading = 'BUDGET_AWARE'; exit.revision = String(BigInt(exit.revision) + 1n); exit.account_control_revision = controlRevision = String(BigInt(controlRevision) + 1n); equity = '3000';
    const readOnly = (await state.service.app.inject({ url: path, headers: state.headers })).json(); assert.equal(readOnly.capability, 'UNSUPPORTED'); assert.equal(readOnly.exits.length, 1); assert.equal(readOnly.reserved, request.amount);
    const capped = await until(read, value => value.allocation?.status === 'DELIVERED' && value.allocation.equity === '3000');
    assert.equal(capped.allocation!.budget, '2849.999999999999999999'); assert.ok(decimalUnits(capped.allocation!.capital!) <= decimalUnits('1000')); assert.notEqual(capped.allocation!.receipt!.id, firstReceipt); assert.deepEqual(capped.allocation!.members.map(row => row.factorId), first.allocation!.members.map(row => row.factorId));
    equity = '250'; rejectNext = true;
    const rejected = await until(read, value => value.allocation?.status === 'REJECTED');
    const decreased = await until(read, value => value.allocation?.status === 'DELIVERED' && value.allocation.equity === '250'); assert.equal(decreased.allocation!.budget, '99.999999999999999999'); assert.notEqual(decreased.allocation!.receipt!.id, capped.allocation!.receipt!.id); assert.notEqual(decreased.allocation!.receipt!.id, rejected.allocation!.receipt!.id);
    const repeatedPosts = targetPosts; await until(read, value => value.allocation?.status === 'UNCHANGED' && BigInt(value.allocation.sequence!) > BigInt(decreased.allocation!.sequence!)); assert.equal(targetPosts, repeatedPosts);
    for (const [evidence, reason] of [['missing', 'account_positions_unknown'], ['unvalued', 'account_position_valuation_unknown']] as const) {
      positionsEvidence = evidence; await until(read, value => value.allocation?.reason === reason); assert.equal(targetPosts, repeatedPosts);
    }
    positionsEvidence = 'known'; const accepted = await until(read, value => value.allocation?.status === 'UNCHANGED');
    execution = false; const projectedAgain = await until(read, value => value.allocation?.status === 'PROJECTED'); assert.equal(projectedAgain.allocation!.receipt!.id, accepted.allocation!.receipt!.id, 'a projection retains the actual previous receipt, not a fictional applied capital context');
    const beforeRestore = targetPosts; execution = true; const restored = await until(read, value => value.allocation?.status === 'UNCHANGED'); assert.equal(targetPosts, beforeRestore); assert.equal(restored.allocation!.receipt!.id, accepted.allocation!.receipt!.id);
    stale = true; await until(read, value => value.allocation?.reason === 'account_snapshot_stale_or_future'); assert.equal(targetPosts, repeatedPosts); stale = false;
    // Expired historical targets produce only projections; no new executable target is invented.
    await state.service.research.harness.commit(async tx => { const facts = await tx.doc(Application); const fact = observation(facts.observations[started.id]); fact.target.valid_until_ns = '1'; facts.observations[started.id] = json(fact); }, context);
    const beforeExpiry = targetPosts; await until(read, value => value.allocation?.status === 'PROJECTED'); assert.equal(targetPosts, beforeExpiry);
    readable = false; const historical = (await state.service.app.inject({ url: path, headers: state.headers })).json();
    assert.equal(historical.capability, 'UNSUPPORTED'); assert.equal(historical.reserved, null); assert.equal(historical.exits.length, 1);
    await until(read, value => value.allocation?.reason === 'account_reservation_unknown'); assert.equal(targetPosts, beforeExpiry);
    await state.service.work.configureAutomatic(started.id, { enabled: false, cashBuffer: '50', maximumSnapshotAgeSeconds: 120 });
  } finally { await state.cleanup(); await remote.close(); }
});

for (const mode of ['paper', 'live'] as const) test(`new manual targets require fresh authoritative reservations and respect prepared allocations in ${mode}`, { timeout: 30_000 }, async () => {
  const state = await setup(); const remote = Fastify(); let reserved: string | null = '100'; let readable = true; let posts = 0; let revision = 7; let sequence = 0; const queries: string[] = [];
  const identity = { clientId: 'TEST_ONLY-fresh-manual', accountId: 'TEST_ONLY-fresh-account', mode, currency: 'USD' };
  remote.get('/capabilities', () => ({ ...identity, capitalExit: { version: 1, supported: readable, readable } }));
  remote.get('/capital-exits', () => ({ ...identity, accountSourceId: id('1'), observationId: id('2'), controlRevision: String(revision), reserved, exits: [] }));
  remote.post('/targets', req => { posts++; return { ...(req.body as object), status: 'ACKNOWLEDGED' }; });
  remote.get('/account', () => ({ ...identity, sequence: String(++sequence), nativeClockNs: nowNs(), equity: '1000', cash: '1000', positions: [] }));
  remote.get<{ Params: { id: string } }>('/targets/:id', (request, reply) => { queries.push(request.params.id); return reply.code(404).send({ error: 'TEST_ONLY-never-offered' }); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  try {
    const source = await built(state); const connection = { ...identity, id: 'fresh-manual', name: 'TEST_ONLY', url }; const alias = { ...connection, id: 'fresh-alias', url: `${url}?alias=1` };
    state.options.config.connections.push(connection, alias); await state.restart();
    const start = (requestId: string, connectionId = connection.id) => state.service.app.inject({ method: 'POST', url: '/api/v3/observations', headers: state.headers, payload: { requestId, portfolioId: source.view.id, mode, connectionId } });
    const blocked = await start('first-contact-reserved'); assert.equal(blocked.statusCode, 409, blocked.body); assert.equal(blocked.json().error, 'account_capital_exit_requires_budget_aware_target'); assert.equal(posts, 0);
    reserved = '0'; revision++; const zero = await readCapitalControl(connection, 'USD'); await state.service.research.harness.commit(async tx => { const facts = await tx.doc(Application); recordCapitalControl(facts.accountControls!, connection, zero); }, context);
    reserved = '100'; revision++; assert.equal((await start('cached-zero-is-not-authoritative')).statusCode, 409); assert.equal(posts, 0);
    const beforeAlias = (await state.service.research.harness.snapshot(Application, context))!.accountControls!; assert.notEqual(accountKey(alias, beforeAlias), accountKey(connection, beforeAlias));
    assert.equal((await start('new-alias-reserved', alias.id)).statusCode, 409); assert.equal(posts, 0);
    const joined = (await state.service.research.harness.snapshot(Application, context))!.accountControls!; assert.equal(accountKey(alias, joined), accountKey(connection, joined));
    readable = false; const unknown = await start('unreadable-reservations'); assert.equal(unknown.statusCode, 409, unknown.body); assert.equal(unknown.json().error, 'account_reservation_unknown'); assert.equal(posts, 0);
    readable = true; reserved = '0'; revision++;
    await state.service.research.harness.commit(async tx => { const facts = await tx.doc(Application); facts.observations.prepared = json({ input: { portfolioId: source.view.id }, connection, taskId: 0, snapshot: null, view: { id: 'prepared', status: 'QUEUED', receipt: { status: 'NOT_OFFERED' }, allocation: { status: 'SIZED', receipt: { id: 'never-offered-allocation', status: 'PENDING' } } } }); }, context);
    const prepared = await start('manual-must-wait-for-prepared-allocation', alias.id); assert.equal(prepared.statusCode, 409, prepared.body); assert.equal(prepared.json().error, 'target_delivery_requires_reconciliation'); assert.equal(posts, 0);
    await state.service.research.harness.commit(async tx => { delete (await tx.doc(Application)).observations.prepared; }, context);
    const accepted = await start('verified-zero-reservation'); assert.equal(accepted.statusCode, 202, accepted.body); await state.service.research.harness.waitForTask((await state.service.work.getObservation(accepted.json().id)).taskId as TaskId, context); assert.equal(posts, 1);
    await state.service.research.harness.commit(async tx => { const facts = await tx.doc(Application); const stopped = observation(json(facts.observations[accepted.json().id])); stopped.view.id = 'never-offered'; stopped.view.receipt!.status = 'NOT_OFFERED'; stopped.view.automaticPolicy = { enabled: false, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 }; stopped.view.allocation = { status: 'STOPPED', receipt: { id: 'never-offered-allocation', status: 'NOT_OFFERED' } } as any; facts.observations['never-offered'] = json(stopped); }, context);
    const refreshed = await state.service.work.refreshObservation('never-offered'); assert.equal(refreshed.error, null); assert.equal(refreshed.receipt!.status, 'NOT_OFFERED'); assert.equal(refreshed.allocation!.receipt!.status, 'NOT_OFFERED'); assert.equal(sequence, 2); assert.deepEqual(queries, []); assert.equal(posts, 1);
  } finally { await state.cleanup(); await remote.close(); }
});

for (const mode of ['paper', 'live'] as const) for (const variant of ['enable-original', 'late-alias'] as const) test(`manual receipt reconciliation precedes ${variant} automatic offers in ${mode}`, { timeout: 30_000 }, async () => {
  assert.ok(process.env.QZ_JOB_BINARY);
  const state = await setup(); const remote = Fastify(); const offered: any[] = []; const queries: string[] = [];
  let sequence = 0; let allowOriginalReceipt = false; let originalId = '';
  const identity = { clientId: 'TEST_ONLY-manual-client', accountId: 'TEST_ONLY-manual-account', mode, currency: 'USD' };
  const expiry = String(BigInt(nowNs()) + 300_000_000_000n);
  remote.get('/capabilities', () => ({ ...identity, capitalExit: { version: 1, readable: true, supported: true }, automaticTargets: { supported: true, revisionBound: true } }));
  remote.get('/capital-exits', () => ({ ...identity, accountSourceId: id('1'), observationId: id('2'), controlRevision: '7', reserved: '0', exits: [] }));
  remote.get('/account', () => { const clock = nowNs(); return { ...identity, sequence: String(++sequence), nativeClockNs: clock, accountSourceId: id('1'), observationId: id('2'), controlRevision: '7', equity: '1000', cash: '1000', positions: [], execution: { currency: 'USD', validUntilNs: expiry, liquidity: [{ instrumentId: 'TEST.SIM', currency: 'USD', eventNs: clock, availableNs: clock, notional: '100000', lotNotional: '1', minimumNotional: '1', takerFee: '0' }] } }; });
  remote.post('/targets', async (req, reply) => {
    offered.push(req.body);
    if (offered.length === 1) { reply.hijack(); reply.raw.destroy(); return; }
    assert.equal((await state.service.work.getObservation(originalId)).view.receipt!.status, 'ACKNOWLEDGED', 'new offers require actual original-receipt settlement, not local task termination');
    return { ...(req.body as object), status: 'ACKNOWLEDGED' };
  });
  remote.get<{ Params: { id: string } }>('/targets/:id', (req, reply) => { queries.push(req.params.id); return allowOriginalReceipt && req.params.id === offered[0]?.id ? { ...offered[0], status: 'ACKNOWLEDGED' } : reply.code(404).send({ error: 'TEST_ONLY-original-outcome-unknown' }); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  try {
    const source = await built(state);
    await state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application); const fact = portfolio(facts.portfolios[source.view.id]);
      fact.target!.valid_until_ns = expiry;
      fact.view.capacity = { status: 'ESTIMATED', reason: null, scope: 'TEST_ONLY', currency: 'USD', basis: 'protocol control, not market evidence', maximumCapital: '1000', minimumCapital: '1', bindingInstrument: null, observationStartNs: null, observationEndNs: null, decisionClockNs: nowNs(), validUntilNs: expiry, assumptions: { participation: '1', maximumAgeSeconds: 60, executionWindow: 'TEST_ONLY', exit: 'TEST_ONLY', fees: 'TEST_ONLY', depth: 'TEST_ONLY' }, scenarios: [] };
      facts.portfolios[source.view.id] = json(fact);
    }, context);
    state.options.config.jobBinary = process.env.QZ_JOB_BINARY!;
    state.options.config.connections.push({ ...identity, id: 'manual-original', name: 'TEST_ONLY original', url }); await state.restart();
    const first = await state.service.work.startObservation({ requestId: 'manual-original', portfolioId: source.view.id, mode, connectionId: 'manual-original' }); originalId = first.id;
    const task = await state.service.research.harness.waitForTask((await state.service.work.getObservation(first.id)).taskId as TaskId, context);
    assert.equal(task.state.status, 'terminal'); assert.equal((await state.service.work.getObservation(first.id)).view.receipt!.status, 'UNCERTAIN');
    const policy = { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 }; let automaticId: string;
    if (variant === 'enable-original') {
      const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${first.id}/automatic`, headers: state.headers, payload: policy });
      assert.equal(response.statusCode, 409, response.body); assert.equal(response.json().error, 'target_delivery_requires_reconciliation');
      // A restored old-Head binding can already have automatic policy with its original receipt unresolved.
      await state.service.research.harness.commit(async tx => {
        const facts = await tx.doc(Application); const fact = observation(facts.observations[first.id]); fact.view.automaticPolicy = policy;
        fact.taskId = await tx.createTask(AdjustAccount, { id: first.id, binary: process.env.QZ_JOB_BINARY!, dataDir: state.directory }, { conversationId: source.view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' }, background: true }); facts.observations[first.id] = json(fact);
      }, context); automaticId = first.id;
    } else {
      await state.service.app.inject({ url: `/api/v3/observations/${first.id}/capital-exits`, headers: state.headers });
      state.options.config.connections.push({ ...identity, id: 'manual-alias', name: 'TEST_ONLY same physical source', url: `${url}?alias=1` }); await state.restart();
      const next = await state.service.work.startObservation({ requestId: 'manual-alias', portfolioId: source.view.id, mode, connectionId: 'manual-alias', automatic: policy }); automaticId = next.id;
    }
    const blocked = await until(() => state.service.work.getObservation(automaticId), fact => fact.view.allocation?.reason === 'target_delivery_requires_reconciliation' || offered.length > 1);
    assert.equal(blocked.view.allocation!.status, 'BLOCKED'); assert.equal(blocked.view.allocation!.reason, 'target_delivery_requires_reconciliation'); assert.equal(offered.length, 1);
    const primary = (await state.service.work.getObservation(automaticId)).taskId; const beforeRefresh = queries.length;
    await state.service.work.refreshObservation(first.id); assert.ok(queries.length > beforeRefresh); assert.equal(queries.at(-1), offered[0].id); assert.equal(offered.length, 1);
    assert.equal((await state.service.work.getObservation(automaticId)).taskId, primary);
    allowOriginalReceipt = true; await state.service.work.refreshObservation(first.id);
    assert.equal((await state.service.work.getObservation(first.id)).view.receipt!.status, 'ACKNOWLEDGED');
    const delivered = await until(() => state.service.work.getObservation(automaticId), fact => fact.view.allocation?.status === 'DELIVERED');
    assert.equal(delivered.view.allocation!.receipt!.status, 'ACKNOWLEDGED'); assert.equal(offered.length, 2); assert.notEqual(offered[0].id, offered[1].id);
    const controls = (await state.service.research.harness.snapshot(Application, context))!.accountControls!;
    assert.equal(accountKey((await state.service.work.getObservation(first.id)).connection, controls), accountKey(delivered.connection, controls));
    await state.service.work.configureAutomatic(automaticId, { ...policy, enabled: false });
  } finally { await state.cleanup(); await remote.close(); }
});

test('prepared execution validity and identical physical aliases survive SQLite reopen before the official task offers', { timeout: 30_000 }, async () => {
  assert.ok(process.env.QZ_JOB_BINARY);
  const state = await setup(); const remote = Fastify(); let accountReads = 0; let posts = 0; const queries: string[] = [];
  let release!: () => void; const heldSnapshot = new Promise<void>(resolve => { release = resolve; });
  const identity = { clientId: 'TEST_ONLY-reopen', accountId: 'TEST_ONLY-reopen', mode: 'paper' as const, currency: 'USD' };
  const source = { accountSourceId: id('1'), observationId: id('2'), controlRevision: '7' };
  remote.get('/capabilities', () => ({ ...identity, capitalExit: { version: 1, readable: true, supported: true }, automaticTargets: { supported: true, revisionBound: true } }));
  remote.get('/capital-exits', () => ({ ...identity, ...source, reserved: '0', exits: [] }));
  remote.get('/account', async () => {
    const sequence = ++accountReads; if (sequence === 2) await heldSnapshot;
    const clock = nowNs(); return { ...identity, ...source, sequence: String(sequence), nativeClockNs: clock, equity: '1000', cash: '1000', positions: [], execution: { currency: 'USD', validUntilNs: String(BigInt(clock) + (sequence === 1 ? 2_000_000_000n : 120_000_000_000n)), liquidity: [{ instrumentId: 'TEST.SIM', currency: 'USD', eventNs: clock, availableNs: clock, notional: '100000', lotNotional: '1', minimumNotional: '1', takerFee: '0' }] } };
  });
  remote.post('/targets', req => { posts++; return { ...req.body as any, status: 'ACKNOWLEDGED' }; });
  remote.get<{ Params: { id: string } }>('/targets/:id', (req, reply) => { queries.push(req.params.id); return reply.code(404).send({ error: 'TEST_ONLY-never-received' }); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  try {
    const built_ = await built(state); const connection = { ...identity, id: 'TEST_ONLY-reopen', name: 'TEST_ONLY loopback', url };
    state.options.config.jobBinary = process.env.QZ_JOB_BINARY!; state.options.config.connections.push(connection);
    const deadline = String(BigInt(nowNs()) + 120_000_000_000n);
    await state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application); const selected = portfolio(facts.portfolios[built_.view.id]);
      selected.view.factors = [{ factorId: 'TEST_ONLY-reopen-factor', weight: '1' }]; selected.target!.valid_until_ns = deadline;
      selected.view.capacity = { status: 'ESTIMATED', currency: 'USD', maximumCapital: '1000', minimumCapital: '1', validUntilNs: deadline, decisionClockNs: nowNs(), assumptions: { participation: '0.01', maximumAgeSeconds: 60 } } as any;
      facts.portfolios[built_.view.id] = json(selected);
      facts.observations.reopen = json({ input: { portfolioId: built_.view.id }, connection, target: selected.target!, taskId: selected.view.taskId, snapshot: null, view: { id: 'reopen', currency: 'USD', status: 'QUEUED', positions: [], curve: [], receipt: { id: 'original', status: 'NOT_OFFERED' }, automaticPolicy: { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 } } });
    }, context);
    const task = { input: { id: 'reopen', binary: process.env.QZ_JOB_BINARY!, dataDir: state.directory } } as any;
    const runtime = { signal: new AbortController().signal, now: Date.now, commit: (operation: any) => state.service.research.harness.commit(operation, context) } as any;
    const read = async () => observation((await state.service.research.harness.snapshot(Application, context))!.observations.reopen);
    await AdjustAccount.definition.phases.snapshot(task, runtime, context);
    const prepared = (await read()).allocationContext!; const originalId = (await read()).view.allocation!.receipt!.id;
    assert.equal((await read()).view.allocation!.status, 'SIZED'); assert.ok(prepared.execution); assert.equal(posts, 0);
    const alias = { ...connection, id: 'TEST_ONLY-identical-alias', clientId: 'TEST_ONLY-alias-client', accountId: 'TEST_ONLY-alias-account', url: `${url}?alias=1` };
    const verified = await readCapitalControl(connection, 'USD');
    await state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application); const current = observation(facts.observations.reopen);
      recordCapitalControl(facts.accountControls!, alias, { ...verified, native: { ...verified.native, clientId: alias.clientId, accountId: alias.accountId } });
      facts.observations.peer = json({ ...current, connection: alias, view: { ...current.view, id: 'peer', status: 'STOPPED', automaticPolicy: undefined, allocation: undefined }, snapshot: { ...current.snapshot!, clientId: alias.clientId, accountId: alias.accountId } });
    }, context);
    await state.restart(); assert.deepEqual((await read()).allocationContext, prepared);
    assert.equal(accountKey(alias, (await state.service.research.harness.snapshot(Application, context))!.accountControls!), accountKey(connection, (await state.service.research.harness.snapshot(Application, context))!.accountControls!));
    await sleep(Math.max(0, Number(BigInt(String(prepared.validUntilNs)) / 1_000_000n) - Date.now() + 30));
    await state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application); const fact = observation(facts.observations.reopen);
      fact.taskId = await tx.createTask(AdjustAccount, task.input, { conversationId: built_.view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' }, background: true }); facts.observations.reopen = json(fact);
    }, context);
    await until(async () => accountReads, value => value >= 2);
    const expired = await read(); assert.equal(expired.view.allocation!.receipt!.id, originalId); assert.equal(expired.view.allocation!.receipt!.status, 'NOT_OFFERED'); assert.equal(expired.view.allocation!.reason, 'allocation_evidence_expired');
    assert.deepEqual(expired.allocationContext, prepared); assert.equal(expired.allocationTarget!.valid_until_ns, deadline); assert.equal(posts, 0); assert.deepEqual(queries, []);
    release(); const fresh = await until(read, fact => fact.view.allocation?.receipt?.status === 'ACKNOWLEDGED');
    assert.equal(posts, 1); assert.deepEqual(queries, []); assert.notEqual(fresh.view.allocation!.receipt!.id, originalId); assert.equal(fresh.view.allocation!.sequence, '2'); assert.notEqual(fresh.allocationContext!.validUntilNs, prepared.validUntilNs); assert.equal(fresh.allocationTarget!.valid_until_ns, deadline);
  } finally { release(); await state.cleanup(); await remote.close(); }
});

for (const variant of ['expiry', 'late-manual', 'transport', 'pending', 'rejected', 'disabled', 'abort-pending', 'execution-expired', 'execution-commit-gap', 'liquidity-expired', 'capacity-expired', 'account-expired', 'credentials', 'legacy-prepared', 'irrelevant-liquidity', 'scoped-liquidity', 'exit-liquidity-expired', 'rejected-intent', 'liquidity-clock-order', 'alias-conflict', 'alias-late-conflict', 'alias-late-newer', 'alias-current-advanced', 'alias-commit-gap', 'alias-commit-gap-sqlite', 'alias-after-post'] as const) test(`automatic pre-POST boundary retains ${variant} receipt truth`, { timeout: 15_000 }, async () => {
  assert.ok(process.env.QZ_JOB_BINARY);
  const directory = await mkdtemp(join(tmpdir(), 'qz-automatic-boundary-')); const remote = Fastify(); let posts = 0; let sequence = 1; let accountReads = 0; const queries: string[] = []; const offers: any[] = [];
  const interleaved = ['alias-commit-gap', 'alias-commit-gap-sqlite', 'alias-after-post'].includes(variant); let advanced = false;
  const state = variant === 'alias-commit-gap-sqlite' ? await setup() : null;
  const tokenEnv = 'QZ_R3_TEST_ONLY_TEMPORARY_TOKEN';
  if (variant === 'credentials') { assert.equal(process.env[tokenEnv], undefined); process.env[tokenEnv] = 'TEST_ONLY-loopback'; }
  const identity = { clientId: 'TEST_ONLY-boundary', accountId: 'TEST_ONLY-account', mode: 'paper', currency: 'USD' };
  const source = { accountSourceId: id('1'), observationId: id('2'), controlRevision: '7' };
  const clock = nowNs(); const shortExpiry = String(BigInt(clock) + 2_000_000_000n); const longExpiry = String(BigInt(clock) + 120_000_000_000n);
  const expiry = variant === 'expiry' ? shortExpiry : longExpiry;
  const executionExpiry = ['execution-expired', 'execution-commit-gap', 'transport'].includes(variant) ? shortExpiry : longExpiry;
  const capacityExpiry = variant === 'capacity-expired' ? shortExpiry : longExpiry;
  const target = { schema_version: 1, asof_ns: String(BigInt(clock) - 1_000_000_000n), valid_until_ns: expiry, cash_weight: variant === 'exit-liquidity-expired' ? '1' : '0', ...(variant === 'scoped-liquidity' ? { rebalance_instrument_ids: ['TEST_ONLY.SIM'] } : {}), targets: [{ instrument_id: 'TEST_ONLY.SIM', currency: 'USD', weight: variant === 'exit-liquidity-expired' ? '0' : variant === 'scoped-liquidity' ? '0.5' : '1' }, ...(variant === 'scoped-liquidity' ? [{ instrument_id: 'UNUSED.SIM', currency: 'USD', weight: '0.5' }] : [])] };
  remote.get('/capabilities', () => ({ ...identity, capitalExit: { version: 1, readable: true, supported: true }, automaticTargets: { supported: true, revisionBound: true } }));
  remote.get('/capital-exits', () => ({ ...identity, ...source, reserved: '0', exits: [] }));
  remote.get('/account', req => { accountReads++; const equity = interleaved && sequence > 1 ? '500' : '1000'; return { ...identity, ...(req.headers['x-qz-client-id'] === 'TEST_ONLY-peer-client' ? { clientId: 'TEST_ONLY-peer-client', accountId: 'TEST_ONLY-peer-account' } : {}), ...source, sequence: String(sequence), nativeClockNs: interleaved ? String(BigInt(clock) + BigInt(sequence - 1)) : clock, equity, cash: variant === 'exit-liquidity-expired' ? '0' : equity, positions: variant === 'exit-liquidity-expired' ? [{ instrumentId: 'TEST_ONLY.SIM', quantity: '1', value: '1000', pnl: null }] : [], execution: { currency: 'USD', validUntilNs: executionExpiry, liquidity: [{ instrumentId: 'TEST_ONLY.SIM', currency: 'USD', eventNs: clock, availableNs: variant === 'liquidity-clock-order' ? String(BigInt(clock) - 1n) : clock, notional: '100000', lotNotional: '1', minimumNotional: '1', takerFee: '0' }, ...(['irrelevant-liquidity', 'scoped-liquidity'].includes(variant) ? [{ instrumentId: 'UNUSED.SIM', currency: 'USD', eventNs: '0', availableNs: '0', notional: '0', lotNotional: '1', minimumNotional: '0', takerFee: '0' }] : [])] } }; });
  remote.post('/targets', async (req, reply) => { posts++; offers.push(json(req.body)); if (variant === 'alias-after-post') { advanced = true; await advancePeer(); } if (variant === 'transport' || variant === 'alias-after-post') { reply.hijack(); reply.raw.destroy(); return; } return { ...(req.body as object), status: variant === 'rejected-intent' || variant === 'rejected' && posts === 1 ? 'REJECTED' : 'ACKNOWLEDGED' }; });
  remote.get<{ Params: { id: string } }>('/targets/:id', (req, reply) => { queries.push(req.params.id); return reply.code(404).send({ error: 'TEST_ONLY-no-observed-receipt' }); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  let facts: any = { observations: { o: { input: { portfolioId: 'p' }, connection: { ...identity, id: 'TEST_ONLY', name: 'TEST_ONLY loopback', url, ...(variant === 'credentials' ? { tokenEnv } : {}) }, target, taskId: 1, snapshot: null, view: { id: 'o', currency: 'USD', status: 'QUEUED', positions: [], curve: [], receipt: { id: 'original', status: 'NOT_OFFERED' }, automaticPolicy: { enabled: true, cashBuffer: interleaved ? '200' : '0', maximumSnapshotAgeSeconds: variant === 'account-expired' ? 2 : 60 } } } }, portfolios: { p: { view: { factors: [{ factorId: 'TEST_ONLY-factor', weight: '1' }], capacity: { status: 'ESTIMATED', currency: 'USD', maximumCapital: '1000', minimumCapital: '1', validUntilNs: capacityExpiry, decisionClockNs: clock, assumptions: { participation: '1', maximumAgeSeconds: ['liquidity-expired', 'exit-liquidity-expired'].includes(variant) ? 2 : 60 } } }, evidence: [] } }, accountControls: {} };
  let checkpoint: any;
  const runtime: any = { signal: new AbortController().signal, now: Date.now,
    snapshot: async (definition: unknown) => { assert.equal(definition, Application); return state ? state.service.research.harness.snapshot(Application, context) : json(facts); },
    commit: async (operation: any) => {
      const value = state ? await state.service.research.harness.commit(operation, context) : await operation({ doc: async (definition: unknown) => { assert.equal(definition, Application); return facts; }, task: async () => ({ state: { status: 'running' } }) });
      if (state) facts = json(await state.service.research.harness.snapshot(Application, context));
      if (value?.checkpoint) checkpoint = value.checkpoint;
      if (variant.startsWith('alias-commit-gap') && !advanced && value?.checkpoint?.phase === 'reconcile') { advanced = true; await advancePeer(); }
      if (variant === 'execution-commit-gap' && checkpoint?.phase === 'reconcile' && posts === 0) await sleep(Math.max(0, Number(BigInt(shortExpiry) / 1_000_000n) - Date.now() + 30));
      return value;
    }, sleep: async () => {} };
  const task: any = { input: { id: 'o', binary: process.env.QZ_JOB_BINARY, dataDir: directory }, state: { checkpoint: { phase: 'snapshot' } } };
  async function joinPeer(snapshot: any) {
    const connection = { ...facts.observations.o.connection, id: 'TEST_ONLY-peer', clientId: 'TEST_ONLY-peer-client', accountId: 'TEST_ONLY-peer-account', url: `${url}?alias=1` };
    const fresh = await readCapitalControl(facts.observations.o.connection, 'USD');
    recordCapitalControl(facts.accountControls, facts.observations.o.connection, fresh);
    recordCapitalControl(facts.accountControls, connection, { ...fresh, native: { ...fresh.native, clientId: connection.clientId, accountId: connection.accountId } });
    facts.observations.peer = json({ ...facts.observations.o, connection, snapshot: { ...snapshot, clientId: connection.clientId, accountId: connection.accountId }, view: { ...facts.observations.o.view, id: 'peer', status: interleaved ? 'OBSERVING' : 'STOPPED', receipt: interleaved ? { ...facts.observations.o.view.receipt, id: 'peer-original', status: 'ACKNOWLEDGED' } : facts.observations.o.view.receipt, automaticPolicy: undefined, allocation: undefined } });
    assert.equal(accountKey(connection, facts.accountControls), accountKey(facts.observations.o.connection, facts.accountControls));
    if (state) await state.service.research.harness.commit(async tx => { const stored = await tx.doc(Application); stored.observations.peer = json(facts.observations.peer); stored.accountControls = facts.accountControls; }, context);
  }
  async function advancePeer() {
    sequence = 2; source.observationId = id('3');
    await ObserveAccount.definition.phases.observe({ input: { id: 'peer', connection: facts.observations.peer.connection, target, receiptId: 'peer-original', refresh: true, accountOnly: true } } as any, runtime, context);
    assert.equal(facts.observations.peer.snapshot.sequence, '2'); assert.equal(facts.observations.peer.view.error, null);
  }
  try {
    if (state) await state.service.research.harness.commit(async tx => { const stored = await tx.doc(Application); stored.observations = facts.observations; stored.portfolios = facts.portfolios; stored.accountControls = facts.accountControls; }, context);
    if (variant === 'rejected-intent') facts.portfolios.p.view.factors = [{ factorId: 'TEST_ONLY-factor-a', weight: '0.5' }, { factorId: 'TEST_ONLY-factor-b', weight: '0.5' }];
    if (variant === 'alias-conflict') await joinPeer({ ...(await remote.inject('/account')).json(), cash: '999' });
    await AdjustAccount.definition.phases.snapshot(task, runtime, context);
    if (variant === 'alias-conflict' || variant === 'liquidity-clock-order') {
      assert.equal(facts.observations.o.view.allocation.status, 'BLOCKED'); assert.equal(facts.observations.o.view.allocation.reason, variant === 'alias-conflict' ? 'account_snapshot_conflict' : 'current_liquidity_stale_or_mismatched'); assert.equal(posts, 0); assert.deepEqual(queries, []);
      if (variant === 'alias-conflict') {
        await ObserveAccount.definition.phases.observe({ input: { id: 'o', connection: facts.observations.o.connection, target, receiptId: 'original', refresh: true, accountOnly: true } } as any, runtime, context);
        assert.equal(facts.observations.o.view.error, 'account_snapshot_conflict'); assert.equal(facts.observations.o.snapshot, null);
      }
      return;
    }
    assert.equal(facts.observations.o.view.allocation.status, 'SIZED'); assert.equal(checkpoint.phase, 'deliver');
    const receiptId = facts.observations.o.view.allocation.receipt.id;
    const prepared = json(facts.observations.o.allocationContext);
    assert.equal(unsettledAdjustment(facts.observations.o), true);
    if (interleaved) { await joinPeer(facts.observations.o.snapshot); assert.equal(prepared.capital, '800'); assert.equal(prepared.cashBuffer, '200'); assert.equal(facts.observations.o.allocationTarget.targets[0].weight, '0.8'); }
    if (variant === 'pending') { await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(checkpoint.phase, 'deliver'); assert.equal(facts.observations.o.view.allocation.receipt.id, receiptId); assert.equal(posts, 0); assert.deepEqual(queries, []); }
    if (['expiry', 'execution-expired', 'liquidity-expired', 'capacity-expired', 'account-expired', 'exit-liquidity-expired'].includes(variant)) await sleep(Math.max(0, Number(BigInt(shortExpiry) / 1_000_000n) - Date.now() + 30));
    if (variant === 'credentials') delete process.env[tokenEnv];
    if (variant === 'account-expired') facts.observations.o.view.automaticPolicy.maximumSnapshotAgeSeconds = 60;
    if (variant === 'legacy-prepared') { delete facts.observations.o.allocationContext.validUntilNs; delete facts.observations.o.allocationContext.execution; }
    if (variant === 'alias-late-conflict') { const snapshot = json(facts.observations.o.snapshot); (snapshot.execution as any).liquidity[0].notional = '90000'; await joinPeer(snapshot); }
    if (variant === 'alias-late-newer') await joinPeer({ ...facts.observations.o.snapshot, sequence: '2' });
    if (variant === 'alias-current-advanced') facts.observations.o.snapshot.sequence = '2';
    if (variant === 'late-manual') facts.observations.manual = json({ ...facts.observations.o, view: { ...facts.observations.o.view, id: 'manual', allocation: undefined, automaticPolicy: undefined, receipt: { id: 'unresolved-manual', status: 'UNCERTAIN' } } });
    if (variant === 'disabled' || variant === 'abort-pending') facts.observations.o.view.automaticPolicy.enabled = false;
    if (variant === 'abort-pending') await AdjustAccount.definition.abort!(task, runtime, context); else await AdjustAccount.definition.phases.deliver(task, runtime, context);
    if (variant === 'disabled' || variant === 'abort-pending') { assert.equal(posts, 0); assert.equal(facts.observations.o.view.allocation.status, 'STOPPED'); assert.equal(facts.observations.o.view.allocation.receipt.status, 'NOT_OFFERED'); assert.equal(unsettledAdjustment(facts.observations.o), false); assert.deepEqual(queries, []); }
    else if (variant === 'pending' || variant === 'irrelevant-liquidity' || variant === 'scoped-liquidity') { assert.equal(posts, 1); assert.equal(facts.observations.o.view.allocation.receipt.status, 'ACKNOWLEDGED'); assert.deepEqual(queries, []); }
    else if (variant === 'credentials') {
      assert.equal(posts, 0); assert.equal(facts.observations.o.view.allocation.receipt.status, 'NOT_OFFERED'); assert.equal(facts.observations.o.view.allocation.reason, 'downstream_credentials_missing'); assert.equal(checkpoint.phase, 'snapshot');
      facts.observations.manual = json({ ...facts.observations.o, view: { ...facts.observations.o.view, id: 'manual', allocation: undefined, automaticPolicy: undefined, receipt: { id: 'manual', status: 'PENDING' } } });
      await ObserveAccount.definition.phases.deliver({ input: { id: 'manual', connection: facts.observations.manual.connection, target, receiptId: 'manual', refresh: false } } as any, runtime, context);
      assert.equal(facts.observations.manual.view.receipt.status, 'NOT_OFFERED'); assert.equal(facts.observations.manual.view.error, 'downstream_credentials_missing'); assert.equal(posts, 0);
      process.env[tokenEnv] = 'TEST_ONLY-loopback'; await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(accountReads, 2); assert.equal(checkpoint.phase, 'deliver'); assert.notEqual(facts.observations.o.view.allocation.receipt.id, receiptId);
      await AdjustAccount.definition.phases.deliver(task, runtime, context); assert.equal(posts, 1); assert.deepEqual(queries, []); assert.equal(facts.observations.o.view.allocation.receipt.status, 'ACKNOWLEDGED');
    }
    else if (['execution-expired', 'execution-commit-gap', 'liquidity-expired', 'capacity-expired', 'account-expired', 'legacy-prepared', 'exit-liquidity-expired'].includes(variant)) {
      assert.equal(posts, 0); assert.deepEqual(queries, []); assert.equal(facts.observations.o.view.allocation.receipt.status, 'NOT_OFFERED'); assert.equal(checkpoint.phase, 'snapshot'); assert.equal(unsettledAdjustment(facts.observations.o), false);
      assert.equal(facts.observations.o.view.allocation.reason, variant === 'legacy-prepared' ? 'allocation_evidence_unavailable' : 'allocation_evidence_expired'); assert.equal(facts.observations.o.allocationTarget.valid_until_ns, expiry);
      if (variant !== 'legacy-prepared') { assert.equal(facts.observations.o.allocationContext.execution.validUntilNs, executionExpiry); assert.equal(facts.observations.o.allocationContext.validUntilNs, shortExpiry); }
      await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(accountReads, 2); assert.equal(posts, 0); assert.deepEqual(queries, []); assert.equal(facts.observations.o.allocationTarget.valid_until_ns, expiry);
    }
    else if (variant.startsWith('alias-commit-gap')) {
      assert.equal(advanced, true); assert.equal(accountReads, 2); assert.equal(posts, 0); assert.deepEqual(queries, []);
      assert.equal(facts.observations.o.view.allocation.receipt.status, 'NOT_OFFERED'); assert.equal(facts.observations.o.view.allocation.reason, 'account_snapshot_out_of_order'); assert.equal(checkpoint.phase, 'snapshot');
      assert.deepEqual(facts.observations.o.allocationContext, prepared); assert.equal(facts.observations.o.allocationTarget.valid_until_ns, expiry);
      if (state) { const stored = (await state.service.research.harness.snapshot(Application, context))!; assert.equal(observation(stored.observations.peer).snapshot!.equity, '500'); assert.equal(observation(stored.observations.o).view.allocation!.receipt!.status, 'NOT_OFFERED'); assert.deepEqual(observation(stored.observations.o).allocationContext, prepared); }
      await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(accountReads, 3); assert.equal(checkpoint.phase, 'deliver'); assert.notEqual(facts.observations.o.view.allocation.receipt.id, receiptId);
      await AdjustAccount.definition.phases.deliver(task, runtime, context); assert.equal(posts, 1); assert.deepEqual(queries, []); assert.equal(facts.observations.o.view.allocation.receipt.status, 'ACKNOWLEDGED');
      assert.equal(offers[0].allocation.snapshotSequence, '2'); assert.equal(offers[0].allocation.capital, '300'); assert.equal(offers[0].allocation.cashBuffer, '200'); assert.equal(offers[0].target.targets[0].weight, '0.6'); assert.equal(offers[0].target.valid_until_ns, expiry); assert.deepEqual(offers[0].allocation, facts.observations.o.allocationContext);
    }
    else if (variant === 'alias-after-post') {
      assert.equal(advanced, true); assert.equal(posts, 1); assert.equal(facts.observations.o.view.allocation.receipt.status, 'UNCERTAIN'); assert.equal(checkpoint.phase, 'reconcile');
      await AdjustAccount.definition.phases.reconcile(task, runtime, context); await AdjustAccount.definition.phases.snapshot(task, runtime, context);
      assert.equal(checkpoint.phase, 'reconcile'); assert.equal(posts, 1); assert.equal(accountReads, 2); assert.deepEqual(queries, [receiptId]); assert.equal(facts.observations.o.view.allocation.receipt.id, receiptId); assert.deepEqual(facts.observations.o.allocationContext, prepared); assert.deepEqual(offers[0].allocation, prepared);
    }
    else if (['alias-late-conflict', 'alias-late-newer', 'alias-current-advanced'].includes(variant)) {
      assert.equal(posts, 0); assert.deepEqual(queries, []); assert.equal(facts.observations.o.view.allocation.receipt.status, 'NOT_OFFERED'); assert.equal(checkpoint.phase, 'snapshot');
      assert.equal(facts.observations.o.view.allocation.reason, variant === 'alias-late-conflict' ? 'account_snapshot_conflict' : 'account_snapshot_out_of_order');
      sequence = 3; await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(checkpoint.phase, 'deliver'); assert.notEqual(facts.observations.o.view.allocation.receipt.id, receiptId);
      await AdjustAccount.definition.phases.deliver(task, runtime, context); assert.equal(posts, 1); assert.equal(facts.observations.o.view.allocation.receipt.status, 'ACKNOWLEDGED'); assert.deepEqual(queries, []);
    }
    else if (variant === 'rejected-intent') {
      assert.equal(posts, 1); assert.equal(facts.observations.o.view.allocation.receipt.status, 'REJECTED');
      const work = workbench({ harness: { snapshot: async () => facts, commit: runtime.commit } } as any, { jobBinary: process.env.QZ_JOB_BINARY!, dataDir: directory } as any);
      const changes = [
        () => work.configureAutomatic('o', { ...facts.observations.o.view.automaticPolicy, cashBuffer: '100' }),
        () => work.configureAutomatic('o', { ...facts.observations.o.view.automaticPolicy, maximumSnapshotAgeSeconds: 90 }),
        () => { facts.portfolios.p.view.factors[0].weight = '0.25'; facts.portfolios.p.view.factors[1].weight = '0.75'; },
        () => { facts.portfolios.p.view.capacity.maximumCapital = '500'; },
        () => { facts.portfolios.p.view.capacity.assumptions.participation = '0.5'; },
        () => { facts.portfolios.p.view.capacity.assumptions.maximumAgeSeconds = 90; },
        () => { facts.observations.o.target.targets[0].weight = '0.5'; facts.observations.o.target.cash_weight = '0.5'; },
      ];
      for (const change of changes) {
        const previousId = facts.observations.o.view.allocation.receipt.id; const before: number = posts;
        for (let repeat = 0; repeat < 2; repeat++) { await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(checkpoint.phase, 'wait'); assert.equal(facts.observations.o.view.allocation.receipt.id, previousId); assert.equal(posts, before); }
        await change(); await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(checkpoint.phase, 'deliver'); assert.notEqual(facts.observations.o.view.allocation.receipt.id, previousId);
        await AdjustAccount.definition.phases.deliver(task, runtime, context); assert.equal(posts, before + 1); assert.equal(facts.observations.o.view.allocation.receipt.status, 'REJECTED'); assert.equal(facts.observations.o.view.allocation.sequence, '1');
      }
      await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(checkpoint.phase, 'wait'); assert.equal(posts, changes.length + 1); assert.deepEqual(queries, []);
    }
    else if (variant === 'rejected') {
      assert.equal(posts, 1); assert.equal(facts.observations.o.view.allocation.receipt.status, 'REJECTED');
      for (let retry = 0; retry < 3; retry++) { await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(checkpoint.phase, 'wait'); assert.equal(facts.observations.o.view.allocation.status, 'REJECTED'); assert.equal(facts.observations.o.view.allocation.receipt.id, receiptId); assert.equal(posts, 1); }
      sequence++; await AdjustAccount.definition.phases.snapshot(task, runtime, context); assert.equal(checkpoint.phase, 'deliver'); assert.notEqual(facts.observations.o.view.allocation.receipt.id, receiptId);
      await AdjustAccount.definition.phases.deliver(task, runtime, context); assert.equal(posts, 2); assert.equal(facts.observations.o.view.allocation.receipt.status, 'ACKNOWLEDGED');
    } else if (variant === 'transport') {
      assert.equal(posts, 1); assert.equal(facts.observations.o.view.allocation.receipt.status, 'UNCERTAIN'); assert.equal(checkpoint.phase, 'reconcile');
      await sleep(Math.max(0, Number(BigInt(shortExpiry) / 1_000_000n) - Date.now() + 30));
      await AdjustAccount.definition.phases.reconcile(task, runtime, context); await AdjustAccount.definition.phases.snapshot(task, runtime, context);
      assert.equal(checkpoint.phase, 'reconcile'); assert.equal(facts.observations.o.view.allocation.receipt.id, receiptId); assert.deepEqual(queries, [receiptId]); assert.equal(posts, 1);
    } else {
      assert.equal(posts, 0); assert.equal(facts.observations.o.view.allocation.receipt.status, 'NOT_OFFERED'); assert.equal(checkpoint.phase, 'snapshot');
      await AdjustAccount.definition.phases.snapshot(task, runtime, context);
      assert.equal(facts.observations.o.view.allocation.status, variant === 'expiry' ? 'PROJECTED' : 'BLOCKED');
      assert.equal(facts.observations.o.view.allocation.reason, variant === 'expiry' ? 'target_expired' : 'target_delivery_requires_reconciliation');
      assert.equal(facts.observations.o.allocationTarget.valid_until_ns, expiry); assert.equal(unsettledAdjustment(facts.observations.o), false); assert.equal(posts, 0); assert.equal(queries.length, 0);
    }
  } finally { if (variant === 'credentials') delete process.env[tokenEnv]; if (state) await state.cleanup(); await remote.close(); await rm(directory, { recursive: true, force: true }); }
});
