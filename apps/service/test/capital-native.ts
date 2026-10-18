import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as sleep } from 'node:timers/promises';
import Fastify from 'fastify';
import { Application, decimalUnits, json, nowNs, observation, portfolio } from '../src/facts.ts';
import { accountKey, capitalStatus, control } from '../src/capital-exit.ts';
import { built, context, setup } from './support.ts';

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
