import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { PortfolioDecision, TargetCommand } from '../../../packages/contracts/src/pm.ts';
import { Application, json, nowNs, nsTime, object, observation } from '../src/facts.ts';
import { captureAccount } from '../src/pm-account.ts';
import { PmDocument, fact, pmRecord, pmState } from '../src/pm-facts.ts';
import { PmTargetTask } from '../src/pm-target.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

// Existing-command recovery protocol only: no scientific qualification,
// target permission, new delivery, fill, cash flow or market result is supplied.
// HTTP supplies the source/receipt; original official tasks and SQLite perform
// reconciliation and the real mandate monitor consumes the resulting facts.
async function fixture(initialReceipt: 'UNAVAILABLE' | 'PENDING' | 'LEGACY_ACKNOWLEDGED') {
  const state = await setup(); const source = accountSourceFixture(); const remote = Fastify();
  let receive: 'UNAVAILABLE' | 'PENDING' | 'ACKNOWLEDGED' | 'REJECTED' = initialReceipt === 'LEGACY_ACKNOWLEDGED' ? 'ACKNOWLEDGED' : initialReceipt;
  let snapshot = json(source.snapshot); let posts = 0, gets = 0;
  const expires = (BigInt(nowNs()) + 120_000_000_000n).toString();
  const originalProducerClock = nsTime(nowNs());
  const target = { schema_version: 1, asof_ns: nowNs(), valid_until_ns: expires, targets: [], cash_weight: '1' };
  const idempotencyKey = 'original-preexisting-target-receipt';
  remote.get('/account', () => snapshot);
  remote.get('/capabilities', () => ({ clientId: source.connection.clientId, accountId: source.connection.accountId,
    mode: 'paper', currency: 'USDC', capitalExit: { version: 1, supported: false, readable: true },
    automaticTargets: { version: 1, supported: false, revisionBound: false, snapshotBound: false } }));
  remote.get('/capital-exits', () => ({ clientId: source.connection.clientId, accountId: source.connection.accountId,
    mode: 'paper', currency: 'USDC', accountSourceId: snapshot.accountSourceId, observationId: snapshot.observationId,
    controlRevision: snapshot.controlRevision, reserved: '0', exits: [] }));
  remote.post('/targets', (_request, reply) => { posts++; return reply.code(403).send({ error: 'no_new_delivery_authorized' }); });
  remote.get<{ Params: { id: string } }>('/targets/:id', (request, reply) => {
    gets++; assert.equal(request.params.id, idempotencyKey);
    if (receive === 'UNAVAILABLE') return reply.code(503).send({ error: 'original_receipt_temporarily_unreadable' });
    return { id: idempotencyKey, clientId: source.connection.clientId, accountId: source.connection.accountId,
      expiresAt: nsTime(expires), target, status: receive, externalId: 'original-remote-target', acknowledgedAt: originalProducerClock };
  });
  try {
    source.connection.url = await remote.listen({ host: '127.0.0.1', port: 0 });
    state.options.config.connections = [source.connection];
    state.options.config.datasets[0]!.currency = 'USDC';
    state.options.config.datasets[0]!.data.request = { train_end_ns: '100', validation_end_ns: '200', forecast: {
      schema_version: 1, selection: { event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300',
        bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 }, parameters: {} } };
    await state.restart();
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.observations[source.connection.id] = json({ ...source.observation, connection: source.connection });
    }, context);
    const admitted = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: {
      requestId: 'existing-receipt-monitor', goal: 'Observe original transport facts only; no scientific or investment qualification.', datasetId: 'controlled',
      funding: { kind: 'ACCOUNT', accountBindingId: source.connection.id }, model: state.model, maxTrials: 1, operatingAuthority: 'RESEARCH_ONLY' } });
    assert.equal(admitted.statusCode, 202, admitted.body); const mandateId: string = admitted.json().mandate.id;
    const decisionId = 'existing-decision', commandId = 'existing-command';
    let originalTaskId!: TaskId;
    await state.service.research.harness.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), mandateId);
      const opening = captureAccount(await tx.doc(Application), source.connection.id);
      record.evidence[opening.evidence.id] = opening.evidence; record.artifacts[opening.evidence.objectKey] = opening.artifact;
      const decision: PortfolioDecision = { ...fact(decisionId), cycleId: 'prior-protocol-cycle', mandateRevisionId: mandateId,
        strategyVersionId: record.latestStrategyId, ensembleRevisionId: null, strategyAllocations: [], policyRevisionId: record.policy.id,
        basisRevision: record.basisRevision, incumbentDecisionId: null, accountBasis: opening.basis, researchStartingStateRef: null,
        action: 'CASH', status: 'SUPERSEDED', assetTargets: [], capital: null, comparisonRef: 'prior-protocol-comparison',
        solverEvidenceRef: null, jointReplayEvidenceRef: null, reasonCodes: ['EXISTING_COMMAND_PROTOCOL_ONLY_NO_SCIENTIFIC_QUALIFICATION'],
        explanationFacts: {}, validUntilNs: expires, nextInvalidationDependencies: [], targetCommandId: initialReceipt === 'LEGACY_ACKNOWLEDGED' ? null : commandId };
      const command: TargetCommand = { ...fact(commandId), decisionId, idempotencyKey, accountBindingId: source.connection.id,
        basisRevision: opening.evidence.sourceRevision, controlRevision: opening.basis.controlRevision,
        payloadEvidenceRef: 'original-target-payload', validUntilNs: expires, state: 'UNCERTAIN', externalReceiptRef: null };
      record.decisions[decisionId] = decision; record.artifacts[command.payloadEvidenceRef] = target;
      if (initialReceipt === 'LEGACY_ACKNOWLEDGED') {
        // Retained pre-clock-schema SQLite record. It contains the original
        // producer receipt, but no local receipt observation/receive timestamp.
        command.state = 'ACKNOWLEDGED'; command.externalReceiptRef = 'legacy-original-receipt';
        record.artifacts[command.externalReceiptRef] = { id: idempotencyKey, clientId: source.connection.clientId, accountId: source.connection.accountId,
          expiresAt: nsTime(expires), target, status: 'ACKNOWLEDGED', externalId: 'original-remote-target', acknowledgedAt: originalProducerClock };
      } else {
        originalTaskId = await tx.createTask(PmTargetTask, { mandateId, commandId, reconcileOnly: true },
          { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
        record.artifacts[`target-task:${commandId}`] = { taskId: originalTaskId };
      }
      record.commands[commandId] = command;
    }, context);
    const read = async () => pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), mandateId);
    if (initialReceipt === 'PENDING') await until(async () => (await read()).commands[commandId]!.state === 'PENDING', 'Original HTTP PENDING receipt must be retained.');
    // One genuine controlled closing source observation; all subsequent GETs
    // return this identical full snapshot, sequence, native clock and balances.
    snapshot = json(snapshot); snapshot.sequence = '9007199254740996'; snapshot.nativeClockNs = '199'; snapshot.observationId = 'one-closing-observation';
    const wrapped = object(snapshot.nativeObservation), native = object(wrapped.snapshot);
    wrapped.sequence = snapshot.sequence; wrapped.observed_at_ns = '201'; native.ts_event = '199'; native.ts_init = '200';
    object(snapshot.openOrders).asof_ns = '199'; object(snapshot.openOrders).available_ns = '201';
    const refresh = async () => {
      const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${encodeURIComponent(source.connection.id)}/refresh`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body);
    };
    let beforeFirstSampleNs = nowNs(); await refresh();
    if (initialReceipt === 'LEGACY_ACKNOWLEDGED') {
      // The account HTTP receipt predates loading the retained legacy link.
      // Keep actual wall clocks distinct rather than forging an older receipt.
      await new Promise(resolve => setTimeout(resolve, 20)); beforeFirstSampleNs = nowNs();
      await state.service.research.harness.commit(async tx => {
        pmRecord(pmState(await tx.doc(PmDocument)), mandateId).decisions[decisionId]!.targetCommandId = commandId;
      }, context);
    }
    await until(async () => Object.values((await read()).outcomes).some(value => value.receiptState === (initialReceipt === 'LEGACY_ACKNOWLEDGED' ? 'ACKNOWLEDGED' : initialReceipt === 'PENDING' ? 'PENDING' : 'UNCERTAIN')),
      'Original monitor must sample the initial receipt state.');
    return { state, source, read, refresh, commandId, decisionId, mandateId, originalTaskId, originalProducerClock, beforeFirstSampleNs,
      get posts() { return posts; }, get gets() { return gets; },
      settle(status: 'ACKNOWLEDGED' | 'REJECTED') { receive = status; },
      async replayReceipt() {
        const taskId = await state.service.research.harness.commit(async tx => {
          const record = pmRecord(pmState(await tx.doc(PmDocument)), mandateId);
          return tx.createTask(PmTargetTask, { mandateId, commandId, reconcileOnly: true },
            { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
        }, context);
        await state.service.research.harness.waitForTask(taskId, context);
      },
      async cleanup() { await state.cleanup(); await remote.close(); } };
  } catch (error) { await state.cleanup(); await remote.close(); throw error; }
}

async function until(check: () => Promise<boolean>, reason: string) {
  const deadline = Date.now() + 9000;
  while (Date.now() < deadline) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 20)); }
  assert.fail(reason);
}

for (const terminal of ['ACKNOWLEDGED', 'REJECTED'] as const) test(`unchanged account snapshot retains ${terminal} receipt-only settlement across real HTTP and SQLite recovery`, { timeout: 35_000 }, async () => {
  const f = await fixture(terminal === 'ACKNOWLEDGED' ? 'UNAVAILABLE' : 'PENDING');
  try {
    const before = await f.read(); const initial = Object.values(before.outcomes).at(-1)!;
    const originalAccount = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.source.connection.id]);
    f.settle(terminal); await f.state.restart();
    await until(async () => (await f.read()).commands[f.commandId]!.state === terminal, 'Original receipt task must settle from actual HTTP.');
    assert.equal((await f.read()).artifacts[`target-task:${f.commandId}`]!.taskId, f.originalTaskId);
    await until(async () => Object.values((await f.read()).outcomes).some(value => value.receiptState === terminal),
      'Same account snapshot must retain its newly settled original receipt, not collide with the prior outcome.');
    const settled = await f.read(); const outcomes = Object.values(settled.outcomes);
    assert.equal(outcomes.length, 2); assert.deepEqual(settled.outcomes[initial.id], initial);
    const latest = outcomes.at(-1)!; assert.deepEqual(latest.accountBasis, initial.accountBasis);
    assert.deepEqual(latest.realizedMetrics, initial.realizedMetrics, 'Receipt settlement does not invent a new account observation, fill, cash flow or PnL.');
    assert.ok(latest.realizedMetrics.every(metric => metric.value === null));
    const receiptClock = settled.artifacts[`target-receipt-clock:${f.commandId}`]!;
    const receiptObservation = settled.artifacts[String(receiptClock.observationRef)]!;
    assert.equal(object(receiptObservation.receipt).acknowledgedAt, f.originalProducerClock);
    assert.equal(latest.availableAtNs, BigInt(initial.accountBasis.availableAtNs) > BigInt(String(receiptObservation.availableAtNs))
      ? initial.accountBasis.availableAtNs : receiptObservation.availableAtNs);
    assert.equal(settled.evidence[String(receiptClock.observationRef)]!.availableAtNs, receiptObservation.availableAtNs);
    assert.equal(settled.artifacts[`outcome-sampling:${f.decisionId}`]!.receiptState, terminal);
    assert.equal(settled.artifacts[`outcome-sampling:${f.decisionId}`]!.receiptObservationRef, receiptClock.observationRef);
    const stable = json({ outcomes: settled.outcomes, attributions: settled.attributions, sampling: settled.artifacts[`outcome-sampling:${f.decisionId}`],
      receiptClock, receiptObservation });
    const firstGets = f.gets;
    await f.replayReceipt(); await f.replayReceipt(); await f.state.restart(); await f.replayReceipt();
    for (let index = 0; index < 3; index++) await f.refresh();
    await until(async () => f.gets >= firstGets + 3, 'Same original receipt must really be read again.');
    await new Promise(resolve => setTimeout(resolve, 5500));
    const repeated = await f.read();
    assert.deepEqual(json({ outcomes: repeated.outcomes, attributions: repeated.attributions, sampling: repeated.artifacts[`outcome-sampling:${f.decisionId}`],
      receiptClock: repeated.artifacts[`target-receipt-clock:${f.commandId}`], receiptObservation: repeated.artifacts[String(receiptClock.observationRef)] }), stable);
    const afterAccount = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.source.connection.id]);
    assert.deepEqual(afterAccount.snapshot, originalAccount.snapshot); assert.equal(afterAccount.snapshotReceivedAtNs, originalAccount.snapshotReceivedAtNs);
    assert.equal(f.posts, 0, 'Original receipt reconciliation cannot create another target delivery.');
  } finally { await f.cleanup(); }
});

test('legacy acknowledged receipt first sampling keeps a conservative local clock across HTTP rereads and SQLite reopen', { timeout: 25_000 }, async () => {
  const f = await fixture('LEGACY_ACKNOWLEDGED');
  try {
    const first = await f.read(); const outcome = Object.values(first.outcomes)[0]!;
    assert.equal(first.artifacts[`target-receipt-clock:${f.commandId}`], undefined, 'The fixture genuinely has no retained receive clock.');
    assert.ok(BigInt(outcome.availableAtNs) > BigInt(outcome.accountBasis.availableAtNs),
      'A legacy ACK first observed after the source snapshot cannot be backdated to the account receive clock.');
    assert.ok(BigInt(outcome.availableAtNs) >= BigInt(f.beforeFirstSampleNs));
    assert.ok(BigInt(outcome.availableAtNs) <= BigInt(nowNs()));
    assert.ok(outcome.realizedMetrics.every(metric => metric.availableAtNs === outcome.accountBasis.availableAtNs));
    const sampling = first.artifacts[`outcome-sampling:${f.decisionId}`]!;
    const observedRef = String(sampling.receiptObservationRef); const originalLocalObservation = first.artifacts[observedRef]!;
    assert.equal(originalLocalObservation.availableAtNs, outcome.availableAtNs);
    assert.equal(object(originalLocalObservation.receipt).acknowledgedAt, f.originalProducerClock);
    await f.state.restart(); await f.refresh(); await f.replayReceipt(); await f.replayReceipt();
    await f.state.restart(); await f.refresh(); await new Promise(resolve => setTimeout(resolve, 5500));
    const repeated = await f.read();
    assert.deepEqual(repeated.outcomes, first.outcomes);
    assert.deepEqual(repeated.artifacts[`outcome-sampling:${f.decisionId}`], sampling);
    assert.deepEqual(repeated.artifacts[observedRef], originalLocalObservation);
    assert.deepEqual(repeated.evidence[observedRef], first.evidence[observedRef]);
    assert.equal(f.posts, 0); assert.ok(f.gets >= 2);
  } finally { await f.cleanup(); }
});
