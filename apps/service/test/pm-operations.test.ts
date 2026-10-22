import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { PmMandateView, PortfolioDecision, TargetCommand } from '../../../packages/contracts/src/pm.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { Application, json, nowNs, nsTime, object, observation } from '../src/facts.ts';
import type { ObservationFact, PortfolioFact } from '../src/facts.ts';
import { readCapitalControl, recordCapitalControl } from '../src/capital-exit.ts';
import { captureAccount } from '../src/pm-account.ts';
import { PmDocument, digest, fact, pmRecord, pmState } from '../src/pm-facts.ts';
import { context, setup } from './support.ts';

// Protocol acceptance only: real HTTP, official Durable tasks and SQLite, with
// explicit controlled account/eligibility facts. No scientific result is
// synthesized by these tests or promoted into investment evidence. The full
// native research/qualification path has a separate acceptance suite.
async function fixture(keepMonitor = false, operatingAuthority: PmMandateView['mandate']['operatingAuthority'] = 'AUTHORIZED_TARGETS') {
  const state = await setup();
  const remote = Fastify();
  try {
    const posts: JsonObject[] = []; const accepted: JsonObject[] = []; const gets: string[] = [];
    let receive: 'UNAVAILABLE' | 'PENDING' | 'ACKNOWLEDGED' = 'UNAVAILABLE';
    let externalId = 'controlled-external-original';
    let receiptOverride: JsonObject = {};
    let remoteAccount: JsonObject = {};
    const priorExpiry = nsTime((BigInt(nowNs()) - 1_000_000_000n).toString());
    remote.post('/targets', (request, reply) => {
      const body = json(request.body); posts.push(body);
      const allocation = object(body.allocation);
      for (const [field, sourceField] of Object.entries({ accountSourceId: 'accountSourceId', observationId: 'observationId', snapshotSequence: 'sequence', nativeClockNs: 'nativeClockNs', controlRevision: 'controlRevision' })) {
        if (allocation[field] !== remoteAccount[sourceField]) return reply.code(409).send({ error: 'controlled_receiver_snapshot_changed' });
      }
      if (typeof allocation.reserved !== 'string' || compareDecimal(allocation.reserved, '0') !== 0) return reply.code(409).send({ error: 'controlled_receiver_reservation_changed' });
      accepted.push(body);
      return reply.code(503).send({ error: 'received_but_response_lost' });
    });
    remote.get<{ Params: { id: string } }>('/targets/:id', (request, reply) => {
      gets.push(request.params.id);
      if (request.params.id === 'controlled-prior-target') return { id: request.params.id, clientId: 'controlled-client', accountId: 'controlled-account',
        expiresAt: priorExpiry, status: 'REJECTED', externalId: 'controlled-prior-external' };
      if (receive === 'UNAVAILABLE') return reply.code(503).send({ error: 'receipt_not_yet_readable' });
      const original = accepted.find(body => body.id === request.params.id);
      if (!original) return reply.code(404).send({ error: 'original_request_not_accepted' });
      return { ...original, status: receive, externalId, ...receiptOverride };
    });
    remote.get('/account', () => remoteAccount);
    remote.get('/capabilities', () => ({ clientId: connection.clientId, accountId: connection.accountId, mode: connection.mode, currency: 'USD',
      capitalExit: { version: 1, supported: keepMonitor, readable: true }, automaticTargets: { supported: true, revisionBound: true, snapshotBound: true } }));
    remote.get('/capital-exits', () => ({ clientId: connection.clientId, accountId: connection.accountId, mode: connection.mode, currency: 'USD',
      accountSourceId: remoteAccount.accountSourceId, observationId: remoteAccount.observationId, controlRevision: remoteAccount.controlRevision, reserved: '0', exits: [] }));
    const url = await remote.listen({ host: '127.0.0.1', port: 0 });
    const connection = { id: 'pm-controlled-account', name: 'PM operations protocol only', mode: 'paper' as const,
      url, clientId: 'controlled-client', accountId: 'controlled-account' };
    state.options.config.connections = [connection];
    state.options.config.datasets[0]!.data.request = { protocol_version: 1, train_end_ns: '100', validation_end_ns: '200', forecast: { schema_version: 1,
      selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300', maximum_rows: 100 }, parameters: {} } };
    await state.restart();
    const observedAtNs = (BigInt(nowNs()) - 1_000_000_000n).toString();
    const account: ObservationFact = {
      input: { requestId: 'controlled-account-observation', portfolioId: 'controlled-existing-position', mode: 'paper', connectionId: connection.id },
      view: { id: connection.id, name: 'Controlled existing account', portfolioId: 'controlled-existing-position', mode: 'paper', status: 'OBSERVING',
        currency: 'USD', equity: '1000.000000000000000001', cash: '900.000000000000000001', updatedAt: null,
        positions: [{ instrumentId: 'TEST.SIM', quantity: '10', value: '100', pnl: null }], curve: [], receipt: null, error: null },
      connection, target: {}, taskId: 0, snapshotReceivedAtNs: nowNs(),
      snapshot: { clientId: connection.clientId, accountId: connection.accountId, accountSourceId: 'controlled-source', observationId: 'controlled-observation',
        sequence: '9007199254740995', nativeClockNs: observedAtNs, controlRevision: '3', currency: 'USD', equity: '1000.000000000000000001', cash: '900.000000000000000001',
        positions: [{ instrumentId: 'TEST.SIM', quantity: '10', value: '100', pnl: null }] },
    };
    // This protocol fixture has existing inventory, but no previously accepted
    // target authority. An expired ACK would require a later native flat proof.
    if (keepMonitor) account.view.receipt = { id: 'controlled-prior-target', status: 'REJECTED', externalId: 'controlled-prior-external', expiresAt: priorExpiry, acknowledgedAt: null };
    remoteAccount = json(account.snapshot);
    const initialControl = await readCapitalControl(connection, 'USD');
    await state.service.research.harness.commit(async tx => {
      const application = await tx.doc(Application); application.observations[connection.id] = json(account);
      application.accountControls ??= {};
      recordCapitalControl(application.accountControls, connection, initialControl);
    }, context);
    const input = {
      requestId: 'controlled-target-mandate', goal: 'Controlled target receipt operations; no investment qualification is asserted.', datasetId: 'controlled',
      funding: { kind: 'ACCOUNT', accountBindingId: connection.id }, model: state.model, maxTrials: 1,
      businessPreference: 'NET_RETURN', operatingAuthority,
    };
    const admitted = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: input });
    assert.equal(admitted.statusCode, 202, admitted.body);
    const id = admitted.json<PmMandateView>().mandate.id;
    const snapshot = pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), id);
    if (!keepMonitor) {
      // Isolate receipt/eligibility cases from unrelated research monitoring.
      // The delivery task remains real and independently recovers across reopen.
      await state.service.research.harness.abortTask(snapshot.monitorTaskId as TaskId, context);
      await state.service.research.harness.waitForTask(snapshot.monitorTaskId as TaskId, context);
    } else {
      const pause = await state.service.app.inject({ method: 'POST', url: `/api/v3/strategies/${encodeURIComponent(snapshot.latestStrategyId)}/pause`, headers: state.headers,
        payload: { requestId: 'controlled-research-pause', expectedRevision: '0' } });
      assert.equal(pause.statusCode, 202, pause.body);
      const campaignId = Object.keys(snapshot.campaigns)[0]!;
      await eventually(async () => pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), id)
        .artifacts[`campaign-control:${campaignId}`]?.state === 'APPLIED', 'Original campaign pause must settle before retirement monitoring.');
    }
    const decisionId = 'controlled-cash-decision';
    await state.service.research.harness.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), id);
      const basis = captureAccount(await tx.doc(Application), connection.id).basis;
      const money = (amount: string) => ({ amount, currency: 'USD' });
      const decision: PortfolioDecision = { ...fact(decisionId), cycleId: 'controlled-cycle', mandateRevisionId: id, strategyVersionId: record.latestStrategyId,
        ensembleRevisionId: null, strategyAllocations: [], policyRevisionId: record.policy.id, basisRevision: record.basisRevision, incumbentDecisionId: null,
        accountBasis: basis, researchStartingStateRef: null, action: 'CASH', status: 'CURRENT',
        assetTargets: [{ instrumentId: 'TEST.SIM', navWeight: '0', targetQuantity: '0', targetNotional: money('0'), currentNotional: money('100'),
          netTradeNotional: money('-100'), quantityRuleRef: 'controlled-quantity-rule', riskContribution: null }],
        capital: { equity: money('1000.000000000000000001'), reserved: money('0'), ownerLiquidityReserve: money('0'), deployableBudget: money('1000.000000000000000001'),
          committedCapital: money('0'), uncommittedCash: money('1000.000000000000000001'), projectedTradingCash: money('0'), capacityEstimateIds: [], sizingEvidenceRef: 'controlled-sizing' },
        comparisonRef: 'controlled-comparison', solverEvidenceRef: null, jointReplayEvidenceRef: null, reasonCodes: ['TEST_ONLY_CONTROLLED_ELIGIBILITY'],
        explanationFacts: { nativeCashWeight: '1' }, validUntilNs: (BigInt(nowNs()) + 120_000_000_000n).toString(), nextInvalidationDependencies: [], targetCommandId: null };
      // Explicitly controlled transport source; not a scientific native proof.
      const originalTargetRef = 'TEST_ONLY_CONTROLLED_NATIVE_TARGET';
      const originalTarget = { schema_version: 1, asof_ns: nowNs(), valid_until_ns: decision.validUntilNs!,
        targets: [{ instrument_id: 'TEST.SIM', currency: 'USD', weight: '0' }], cash_weight: '1' };
      decision.explanationFacts = { nativeCashWeight: '1', nativeTargetEvidenceRef: originalTargetRef };
      record.artifacts[originalTargetRef] = originalTarget;
      record.evidence[originalTargetRef] = { id: originalTargetRef, sourceKind: 'NATIVE_RESULT', role: 'CONTROL', runtime: null,
        objectKey: originalTargetRef, contentDigest: digest(originalTarget), sourceRevision: decision.basisRevision,
        mediaType: 'application/json', eventInterval: null, availableAtNs: nowNs(), clockPrecision: 'NANOSECOND', schemaId: 'TEST_ONLY_CONTROLLED_NATIVE_TARGET', visibility: 'OWNER_ONLY' };
      record.decisions[decisionId] = decision; record.currentDecisionId = decisionId; record.gaps = {};
      record.assessments['controlled-target-assessment'] = { ...fact('controlled-target-assessment'), strategyVersionId: record.latestStrategyId,
        stage: 'PAPER', outcome: 'READY', evaluatedAtNs: nowNs(), requirementsVersion: 'qz.pm.paper-shadow-source-target-qualification/1',
        evidenceRefs: [], reasonCodes: ['TEST_ONLY_CONTROLLED_ELIGIBILITY'], nextPermittedActions: [] };
      record.strategies[record.latestStrategyId]!.pipelineAssessmentId = 'controlled-target-assessment';
      if (keepMonitor) {
        const application = await tx.doc(Application);
        const portfolio: PortfolioFact = { input: { requestId: 'controlled-prior-portfolio', name: 'Controlled prior account binding',
          researchIds: [record.conversationId], datasetId: 'controlled', capital: '1000.000000000000000001' },
          view: { id: account.input.portfolioId, name: 'Controlled prior account binding', researchIds: [record.conversationId], datasetId: 'controlled',
            status: 'COMPUTED', capital: '1000.000000000000000001', currency: 'USD', createdAt: new Date().toISOString(), taskId: null,
            metrics: [], curve: [], targets: [], error: null }, evidence: [], policy: {}, native: null, target: {} };
        application.portfolios[account.input.portfolioId] = json(portfolio);
      }
    }, context);
    async function read() {
      const response = await state.service.app.inject({ url: `/api/v3/mandates/${encodeURIComponent(id)}`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body); return response.json<PmMandateView>();
    }
    async function offer(requestId = 'original-target-request') {
      return state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(id)}/targets`, headers: state.headers,
        payload: { requestId, expectedRevision: '0', decisionId } });
    }
    async function record() { return pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), id); }
    return { state, remote, posts, accepted, gets, id, connection, decisionId, read, offer, record,
      receipt(status: typeof receive, overrides: JsonObject = {}) { receive = status; receiptOverride = overrides; },
      external(value: string) { externalId = value; },
      accountSnapshot(value: JsonObject) { remoteAccount = structuredClone(value); },
      async cleanup() { await state.cleanup(); await remote.close(); } };
  } catch (error) { await state.cleanup(); await remote.close(); throw error; }
}

async function eventually(check: () => Promise<boolean>, reason: string) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 20)); }
  assert.fail(reason);
}
async function delivered(f: Awaited<ReturnType<typeof fixture>>): Promise<TargetCommand> {
  const response = await f.offer(); assert.equal(response.statusCode, 202, response.body);
  try {
    await eventually(async () => f.posts.length === 1 && (await f.read()).targetCommands[0]?.state === 'UNCERTAIN', 'Lost remote response must persist UNCERTAIN.');
  } catch (error) {
    const record = await f.record(); const command = Object.values(record.commands)[0]!;
    const taskId = record.artifacts[`target-task:${command.id}`]!.taskId as TaskId;
    assert.fail(`${String(error)} ${JSON.stringify({ posts: f.posts.length, command, task: await f.state.service.research.harness.getTask(taskId, context), error: record.artifacts[`target-error:${command.id}`] })}`);
  }
  assert.equal(f.accepted.length, 1, 'The actual receiver must have accepted the complete original snapshot group before losing its response.');
  return (await f.read()).targetCommands[0]!;
}

test('PM operations: uncertain delivery reopens the original task and receipt; a different request ID cannot resend the same decision', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    const command = await delivered(f); const originalPayload = structuredClone(f.posts[0]);
    assert.equal(originalPayload!.id, command.idempotencyKey);
    assert.deepEqual(originalPayload!.target, { schema_version: 1, asof_ns: (await f.record()).artifacts.TEST_ONLY_CONTROLLED_NATIVE_TARGET!.asof_ns,
      valid_until_ns: command.validUntilNs, targets: [{ instrument_id: 'TEST.SIM', currency: 'USD', weight: '0' }], cash_weight: '1' });
    assert.equal(object(originalPayload!.allocation).snapshotSequence, '9007199254740995');
    assert.equal(object(originalPayload!.allocation).accountSourceId, 'controlled-source');
    assert.equal(object(originalPayload!.allocation).controlRevision, '3');
    assert.deepEqual(originalPayload!.allocation, (await f.record()).artifacts[`target-allocation:${command.id}`]);
    const taskId = (await f.record()).artifacts[`target-task:${command.id}`]!.taskId as TaskId;
    assert.equal((await f.offer()).statusCode, 202, 'Original request replay must be accepted.');
    const duplicate = await f.offer('different-request-same-decision');
    assert.equal(duplicate.statusCode, 409, duplicate.body);
    assert.equal((await f.read()).targetCommands.length, 1);
    await f.state.restart();
    assert.equal((await f.record()).artifacts[`target-task:${command.id}`]!.taskId, taskId);
    assert.equal((await f.read()).targetCommands[0]!.idempotencyKey, command.idempotencyKey);
    f.receipt('ACKNOWLEDGED');
    await eventually(async () => (await f.read()).targetCommands[0]?.state === 'ACKNOWLEDGED', 'Official recovery must reconcile the original receipt.');
    const settled = await f.state.service.research.harness.waitForTask(taskId, context);
    assert.equal(settled.state.status, 'terminal');
    assert.equal(f.posts.length, 1); assert.deepEqual(f.posts[0], originalPayload);
    assert.ok(f.gets.length > 0 && f.gets.every(id => id === command.idempotencyKey));
    const view = await f.read(); assert.equal(view.currentDecision!.targetCommandId, command.id);
    assert.equal((await f.offer('new-request-after-acknowledgement')).statusCode, 409, 'An acknowledged decision cannot create a second delivery.');
    assert.equal(view.outcomes.filter(item => item.realizedMetrics.length > 0).length, 0, 'A target acknowledgement is not realized profit.');
    assert.deepEqual(observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]).view.positions,
      [{ instrumentId: 'TEST.SIM', quantity: '10', value: '100', pnl: null }], 'Acknowledgement must not invent a fill or zero the positions.');
  } finally { await f.cleanup(); }
});

test('PM operations: external receipt identity remains pinned through PENDING, a conflicting acknowledgement and restart', { timeout: 35_000 }, async () => {
  const f = await fixture();
  try {
    const command = await delivered(f); f.receipt('PENDING');
    await eventually(async () => (await f.read()).targetCommands[0]?.state === 'PENDING', 'Original external identity must be recorded.');
    const pinned = (await f.record()).artifacts[`target-receipt:${command.id}`];
    assert.equal(pinned!.externalId, 'controlled-external-original');
    f.external('conflicting-external-identity'); f.receipt('ACKNOWLEDGED');
    await f.state.restart();
    await eventually(async () => (await f.record()).artifacts[`target-error:${command.id}`]?.reason === 'target_receipt_external_identity_mismatch', 'Conflicting remote identity must be rejected.');
    assert.equal((await f.read()).targetCommands[0]!.state, 'PENDING');
    assert.deepEqual((await f.record()).artifacts[`target-receipt:${command.id}`], pinned);
    f.external('controlled-external-original');
    await eventually(async () => (await f.read()).targetCommands[0]?.state === 'ACKNOWLEDGED', 'Original external identity must remain reconcilable.');
    assert.equal(f.posts.length, 1);
  } finally { await f.cleanup(); }
});

test('PM operations: retirement preserves uncertain delivery and residual positions while the original receipt reconciles after restart', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    const command = await delivered(f);
    const request = { requestId: 'retire-with-uncertain-target', expectedRevision: '0' };
    const retire = () => f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(f.id)}/retire`, headers: f.state.headers, payload: request });
    const retired = await retire(); assert.equal(retired.statusCode, 202, retired.body);
    const view = retired.json<PmMandateView>();
    assert.equal(view.currentDecision, null); assert.equal(view.history.find(item => item.id === f.decisionId)!.status, 'REVOKED');
    assert.equal(view.retirements.length, 1);
    assert.deepEqual(view.retirements[0]!.outstandingTargetCommandIds, [command.id]);
    assert.equal(view.retirements[0]!.signalGeneration, 'STOPPED');
    assert.equal(view.retirements[0]!.positionDisposition, 'MONITORING_EXISTING');
    assert.equal(view.retirements[0]!.reconciliationRequired, true);
    assert.equal(view.targetCommands[0]!.state, 'UNCERTAIN');
    await f.state.restart();
    assert.equal((await retire()).statusCode, 202); assert.equal((await f.read()).retirements.length, 1);
    assert.equal((await f.offer('retired-decision-cannot-be-resent')).statusCode, 409);
    f.receipt('ACKNOWLEDGED');
    await eventually(async () => (await f.read()).targetCommands[0]?.state === 'ACKNOWLEDGED', 'Retirement must not abandon uncertain receipt reconciliation.');
    const final = await f.read();
    assert.equal(final.currentDecision, null); assert.equal(f.posts.length, 1);
    assert.equal(final.retirements[0]!.positionDisposition, 'MONITORING_EXISTING');
    assert.equal(final.retirements[0]!.reconciliationRequired, true, 'Acknowledgement does not prove liquidation.');
    assert.equal(final.retirements[0]!.closureEvidenceRefs.length, 0);
    const account = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]);
    assert.equal(account.view.positions[0]!.quantity, '10'); assert.equal(account.view.equity, '1000.000000000000000001');
  } finally { await f.cleanup(); }
});

for (const changed of ['positions', 'openOrders'] as const) {
  test(`PM operations: changed ${changed} cannot reuse a target basis even when account clocks and headline equity match`, { timeout: 15_000 }, async () => {
    const f = await fixture();
    try {
      const before = (await f.read()).currentDecision!.accountBasis!;
      await f.state.service.research.harness.commit(async tx => {
        const application = await tx.doc(Application); const account = observation(application.observations[f.connection.id]);
        // Defensive consumer check: the original account-ingress validator also
        // rejects contradictory same-sequence snapshots. A target must bind its
        // complete retained source, even if such a contradiction reaches it.
        account.snapshot![changed] = changed === 'positions'
          ? [{ instrumentId: 'TEST.SIM', quantity: '11', value: '100', pnl: null }]
          : [{ id: 'new-controlled-outstanding-order', instrumentId: 'TEST.SIM', quantity: '1' }];
      }, context);
      const captured = captureAccount((await f.state.service.research.harness.snapshot(Application, context))!, f.connection.id).basis;
      for (const field of ['accountSourceId', 'observationId', 'sequence', 'observedAtNs', 'controlRevision', 'currency', 'equity', 'reserved'] as const) assert.deepEqual(captured[field], before[field]);
      assert.notEqual(captured.positionsEvidenceRef, before.positionsEvidenceRef);
      assert.notEqual(captured.outstandingCommandsRef, before.outstandingCommandsRef);
      const rejected = await f.offer(); assert.equal(rejected.statusCode, 409, rejected.body);
      assert.equal(f.posts.length, 0); assert.equal((await f.read()).targetCommands.length, 0);
    } finally { await f.cleanup(); }
  });
}

test('PM operations: a historical decision without an account basis returns a business refusal without creating a target', { timeout: 15_000 }, async () => {
  const f = await fixture();
  try {
    await f.state.service.research.harness.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), f.id);
      record.decisions[f.decisionId]!.accountBasis = null;
    }, context);
    const rejected = await f.offer(); assert.equal(rejected.statusCode, 409, rejected.body);
    assert.equal(f.posts.length, 0); assert.equal((await f.read()).targetCommands.length, 0);
  } finally { await f.cleanup(); }
});

test('PM operations: a superseded READY assessment cannot qualify a strategy whose current assessment needs evidence', { timeout: 15_000 }, async () => {
  const f = await fixture();
  try {
    await f.state.service.research.harness.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), f.id);
      const previous = record.assessments['controlled-target-assessment']!;
      record.assessments['current-missing-evidence'] = { ...previous, ...fact('current-missing-evidence'), supersedesId: previous.id,
        outcome: 'NEEDS_EVIDENCE', reasonCodes: ['controlled-current-source-invalidated'] };
      record.strategies[record.latestStrategyId]!.pipelineAssessmentId = 'current-missing-evidence';
    }, context);
    const rejected = await f.offer(); assert.equal(rejected.statusCode, 409, rejected.body);
    assert.equal(f.posts.length, 0); assert.equal((await f.read()).targetCommands.length, 0);
    assert.equal((await f.record()).assessments['controlled-target-assessment']!.outcome, 'READY', 'History remains intact; it simply cannot authorize a current target.');
  } finally { await f.cleanup(); }
});

function flatSnapshot(original: JsonObject, sequence: string, orders?: number): JsonObject {
  const snapshot: JsonObject = { ...original, sequence, nativeClockNs: nowNs(), positions: [], cash: '1000.000000000000000001' };
  const money = (amount: string) => ({ amount, currency: 'USD' });
  snapshot.controlRevision = '3';
  snapshot.nativeObservation = { schema_version: 1, binding: { schema_version: 1, project_id: '01900000-0000-7000-8000-000000000002', environment: 'PAPER',
    native_trader_id: 'CONTROLLED-TRADER', native_session_id: 'CONTROLLED-SESSION', native_account_id: 'CONTROLLED-ACCOUNT', native_version: '0.63.0' },
    sequence, dropped_events: '0', observed_at_ns: snapshot.nativeClockNs!, connection: 'CONNECTED',
    snapshot: { account_id: 'CONTROLLED-ACCOUNT', account_type: 'CASH', base_currency: null,
      balances: [{ total: money('1000.000000000000000001'), locked: money('0'), free: money('1000.000000000000000001') }],
      margins: [], unrealized_pnls: [], realized_pnls: [], total_equity: [money('1000.000000000000000001')], base_currency_equity: null,
      is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [], event_id: '01900000-0000-7000-8000-000000000001',
      ts_event: snapshot.nativeClockNs!, ts_init: snapshot.nativeClockNs! } };
  if (orders !== undefined) snapshot.openOrders = { schema_version: 1, account_id: 'CONTROLLED-ACCOUNT', asof_ns: snapshot.nativeClockNs!, available_ns: snapshot.nativeClockNs!, count: orders };
  return snapshot;
}

test('PM operations: the real retirement monitor requires a zero-count native order report and flat balances before confirming closure', { timeout: 40_000 }, async () => {
  const f = await fixture(true);
  try {
    const retired = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(f.id)}/retire`, headers: f.state.headers,
      payload: { requestId: 'retire-with-native-orders', expectedRevision: '0' } });
    assert.equal(retired.statusCode, 202, retired.body);
    const original = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]).snapshot!;
    let sequence = BigInt(String(original.sequence));
    for (const orders of [undefined, 1, 0]) {
      sequence++;
      const snapshot = flatSnapshot(original, sequence.toString(), orders);
      f.accountSnapshot(snapshot);
      // No direct outcome/disposition writes: original HTTP account ingestion
      // and the actual periodic PM task must consume this source revision.
      await eventually(async () => {
        const application = (await f.state.service.research.harness.snapshot(Application, context))!;
        const observed = observation(application.observations[f.connection.id]);
        if (observed.snapshot?.sequence !== sequence.toString()) return false;
        const captured = captureAccount(application, f.connection.id);
        return (await f.read()).retirements[0]?.residualPositionsEvidenceRef === captured.evidence.id;
      }, 'Original HTTP observation and PM monitor must reconcile the new account sequence.');
      const disposition = (await f.read()).retirements[0]!;
      const explicitlyFlat = orders === 0;
      assert.equal(disposition.positionDisposition, explicitlyFlat ? 'FLAT_CONFIRMED' : 'MONITORING_EXISTING');
      assert.equal(disposition.reconciliationRequired, !explicitlyFlat);
      assert.equal(disposition.closureEvidenceRefs.length > 0, explicitlyFlat);
      assert.equal(f.posts.length, 0, 'Retirement and observation must never send liquidation orders.');
    }
  } finally { await f.cleanup(); }
});


test('PM operations: retired and draining monitors coalesce ten transport heartbeats but preserve original residual and flat-closure evidence', { timeout: 90_000 }, async () => {
  const f = await fixture(true);
  try {
    const retired = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(f.id)}/retire`, headers: f.state.headers,
      payload: { requestId: 'retire-before-heartbeats', expectedRevision: '0' } });
    assert.equal(retired.statusCode, 202, retired.body);
    const original = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]).snapshot!;
    let sequence = BigInt(String(original.sequence));
    // Actual HTTP ingestion of unchanged cash balances and one working order:
    // this is still an unresolved retirement, never a synthetic flat closure.
    const economicSnapshot = flatSnapshot(original, (++sequence).toString(), 1);
    async function observedByMonitor(value: JsonObject) {
      f.accountSnapshot(value);
      await eventually(async () => {
        const application = (await f.state.service.research.harness.snapshot(Application, context))!;
        if (observation(application.observations[f.connection.id]).snapshot?.sequence !== value.sequence) return false;
        const latest = object((await f.record()).artifacts['latest-account-pending']);
        return object(object(latest.capture).basis).sequence === value.sequence;
      }, 'The actual periodic MonitorMandate must consume each new HTTP transport sequence.');
    }
    await observedByMonitor(economicSnapshot);
    const before = await f.record();
    const disposition = Object.values(before.retirements)[0]!;
    assert.equal(disposition.reconciliationRequired, true);
    assert.equal(disposition.positionDisposition, 'MONITORING_EXISTING');
    const residualRef = disposition.residualPositionsEvidenceRef!;
    const residual = await f.state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(residualRef)}`, headers: f.state.headers });
    assert.equal(residual.statusCode, 200, residual.body);
    const immutableSource = residual.json();
    const history = (record: typeof before) => ({ evidence: Object.keys(record.evidence).sort(), artifacts: Object.keys(record.artifacts).sort(),
      cycles: Object.keys(record.cycles).sort(), events: Object.keys(record.events).sort(), outcomes: Object.keys(record.outcomes).sort() });
    const initialHistory = history(before);
    for (let heartbeat = 0; heartbeat < 10; heartbeat++) {
      if (heartbeat === 5) {
        const entered = await f.state.service.app.inject({ method: 'POST', url: '/api/v3/maintenance/enter', headers: f.state.headers,
          payload: { requestId: 'drain-between-heartbeats' } });
        assert.equal(entered.statusCode, 202, entered.body);
        assert.equal((await f.state.service.app.inject({ url: '/api/v3/maintenance', headers: f.state.headers })).json().state, 'DRAINING');
      }
      const next = structuredClone(economicSnapshot);
      next.sequence = (++sequence).toString();
      object(next.nativeObservation).sequence = next.sequence;
      await observedByMonitor(next);
      const current = await f.record();
      assert.deepEqual(history(current), initialHistory, 'Transport sequence changes cannot append PM evidence/artifact/cycle/event/outcome history.');
      assert.deepEqual(current.retirements[disposition.id], disposition, 'Unchanged economic responsibility cannot rewrite its original immutable evidence reference.');
      assert.deepEqual(current.evidence[residualRef], before.evidence[residualRef]);
      assert.equal(f.posts.length, 0);
    }
    // A later authoritative zero-order report is a material source change and
    // must still create immutable closure evidence while DRAINING.
    await observedByMonitor(flatSnapshot(original, (++sequence).toString(), 0));
    await eventually(async () => (await f.read()).retirements[0]?.positionDisposition === 'FLAT_CONFIRMED', 'Actual zero-order HTTP evidence must close the retirement.');
    await eventually(async () => (await f.state.service.app.inject({ url: '/api/v3/maintenance', headers: f.state.headers })).json().state === 'QUIESCED', 'Only the actual closure may release the maintenance obligation.');
    const closure = (await f.read()).retirements[0]!;
    assert.equal(closure.reconciliationRequired, false);
    assert.ok(closure.closureEvidenceRefs.length > 0);
    assert.notEqual(closure.residualPositionsEvidenceRef, residualRef);
    const finalEvidence = await f.state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(closure.residualPositionsEvidenceRef!)}`, headers: f.state.headers });
    assert.equal(finalEvidence.statusCode, 200, finalEvidence.body);
    assert.equal(object(object(finalEvidence.json().payload).snapshot).sequence, sequence.toString());
    await f.state.restart();
    assert.deepEqual((await f.read()).retirements[0], closure);
    const recoveredResidual = await f.state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(residualRef)}`, headers: f.state.headers });
    assert.equal(recoveredResidual.statusCode, 200, recoveredResidual.body);
    assert.deepEqual(recoveredResidual.json(), immutableSource, 'Restart retains the original unresolved-order source alongside the later closure.');
    assert.equal(f.posts.length, 0, 'Read-only retirement and maintenance never create a target or liquidation request.');
  } finally { await f.cleanup(); }
});

test('PM operations: maintenance parks original automatic sizing while retirement continues read-only account reconciliation', { timeout: 35_000 }, async () => {
  // This PM observes retirement responsibility without claiming target
  // control of the independently enabled legacy automatic controller.
  const f = await fixture(true, 'RESEARCH_ONLY');
  try {
    const automatic = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${encodeURIComponent(f.connection.id)}/automatic`, headers: f.state.headers,
      payload: { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 } });
    assert.equal(automatic.statusCode, 200, automatic.body);
    const originalAccount = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]);
    const automaticTaskId = originalAccount.taskId as TaskId;
    assert.ok(automaticTaskId > 0);
    const retired = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(f.id)}/retire`, headers: f.state.headers,
      payload: { requestId: 'retire-before-maintenance', expectedRevision: '0' } });
    assert.equal(retired.statusCode, 202, retired.body);
    assert.equal(retired.json<PmMandateView>().retirements[0]!.reconciliationRequired, true);
    const entered = await f.state.service.app.inject({ method: 'POST', url: '/api/v3/maintenance/enter', headers: f.state.headers,
      payload: { requestId: 'maintain-with-residual-account' } });
    assert.equal(entered.statusCode, 202, entered.body);
    await eventually(async () => (await f.state.service.research.harness.getTask(automaticTaskId, context))?.state.status === 'waiting',
      'Original automatic task must park on the official maintenance gate.');
    const parked = await f.state.service.research.harness.getTask(automaticTaskId, context);
    assert.ok(parked);
    assert.equal(parked.kind, 'qz.account-adjustment');
    assert.equal(parked.state.status, 'waiting');
    const statusBefore = await f.state.service.app.inject({ url: '/api/v3/maintenance', headers: f.state.headers });
    assert.equal(statusBefore.statusCode, 200, statusBefore.body);
    const maintenance = statusBefore.json();
    assert.equal(maintenance.state, 'DRAINING', 'Unresolved retirement responsibility must prevent premature quiescence.');
    assert.deepEqual(parked.state.status === 'waiting' ? parked.state.on : [], [maintenance.barrierTaskId]);
    assert.equal(object(parked.state.checkpoint).phase, 'maintenance');
    assert.equal(object(parked.state.checkpoint).maintenanceEpoch, maintenance.epoch);
    const sequence = (BigInt(String(originalAccount.snapshot!.sequence)) + 1n).toString();
    f.accountSnapshot(flatSnapshot(originalAccount.snapshot!, sequence, 0));
    await eventually(async () => (await f.read()).retirements[0]?.positionDisposition === 'FLAT_CONFIRMED',
      'DRAINING must permit the original read-only monitor to observe authoritative flat balances and orders while automatic sizing stays parked.');
    await eventually(async () => (await f.state.service.app.inject({ url: '/api/v3/maintenance', headers: f.state.headers })).json().state === 'QUIESCED',
      'Only reconciled retirement and parked original tasks may enter QUIESCED.');
    const observed = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]);
    assert.equal(observed.snapshot!.sequence, sequence);
    assert.equal(observed.view.automaticPolicy!.enabled, true, 'Maintenance preserves the existing automatic policy.');
    assert.equal(observed.taskId, automaticTaskId, 'Read-only refresh cannot replace the parked original adjustment task.');
    assert.deepEqual((await f.state.service.research.harness.getTask(automaticTaskId, context))?.state, parked.state);
    assert.equal((await f.read()).retirements[0]!.reconciliationRequired, false);
    assert.equal(f.posts.length, 0, 'Draining does not send new targets or liquidation requests.');
  } finally { await f.cleanup(); }
});

for (const mismatched of ['missing', 'changed'] as const) {
  test(`PM operations: ${mismatched} allocation echo cannot settle an original revision-bound command`, { timeout: 35_000 }, async () => {
    const f = await fixture();
    try {
      const command = await delivered(f);
      const original = structuredClone(f.posts[0]!);
      f.receipt('ACKNOWLEDGED', { allocation: mismatched === 'missing' ? null : { ...object(original.allocation), controlRevision: '4' } });
      await f.state.restart();
      await eventually(async () => (await f.record()).artifacts[`target-error:${command.id}`]?.reason === 'target_receipt_allocation_mismatch', 'Revision-bound receipts must echo the original complete allocation.');
      assert.equal((await f.read()).targetCommands[0]!.state, 'UNCERTAIN');
      f.receipt('ACKNOWLEDGED');
      await eventually(async () => (await f.read()).targetCommands[0]!.state === 'ACKNOWLEDGED', 'The actual original receipt remains reconcilable.');
      assert.equal(f.posts.length, 1); assert.deepEqual(f.posts[0], original);
    } finally { await f.cleanup(); }
  });
}

test('PM operations: a legacy uncertain command without an allocation envelope still reconciles only its original receipt', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    const command = await delivered(f);
    // Controlled stored v1 shape from before revision-bound PM delivery. Do not
    // retrofit new source evidence onto a request which may already be remote.
    await f.state.service.research.harness.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), f.id);
      delete record.artifacts[`target-allocation:${command.id}`];
      delete record.evidence[`target-allocation:${command.id}`];
    }, context);
    f.receipt('ACKNOWLEDGED', { allocation: null });
    await f.state.restart();
    await eventually(async () => (await f.read()).targetCommands[0]!.state === 'ACKNOWLEDGED', 'Legacy uncertain requests retain their original read-only reconciliation path.');
    assert.equal(f.posts.length, 1);
    assert.ok(f.gets.length > 0 && f.gets.every(id => id === command.idempotencyKey));
  } finally { await f.cleanup(); }
});

test('PM operations: a paused strategy cannot offer its retained target while original uncertain receipts remain independent', { timeout: 15_000 }, async () => {
  const f = await fixture();
  try {
    await f.state.service.research.harness.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), f.id);
      record.artifacts[`strategy-control:${record.decisions[f.decisionId]!.strategyVersionId}`] = { action: 'pause', requestId: 'original-pause' };
    }, context);
    const response = await f.offer();
    assert.equal(response.statusCode, 409, response.body); assert.match(response.body, /target_strategy_paused/);
    assert.equal(f.posts.length, 0); assert.equal((await f.read()).targetCommands.length, 0);
  } finally { await f.cleanup(); }
});
