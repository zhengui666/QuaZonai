import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { PmMandateView, PortfolioDecision } from '../../../packages/contracts/src/pm.ts';
import type { ObservationFact, PortfolioFact } from '../src/facts.ts';
import { Application, json, nowNs, nsTime, object, observation } from '../src/facts.ts';
import { accountKey, readCapitalControl, recordCapitalControl } from '../src/capital-exit.ts';
import { PmDocument, digest, fact, pmState } from '../src/pm-facts.ts';
import { captureAccount } from '../src/pm-account.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

// Independent control-protocol acceptance: real owner HTTP and official
// SQLite/Durable admission, original native-source-shaped protocol fixtures.
// No successful science, investment qualification or target fill is invented.
async function fixture(keepMonitor = false) {
  const state = await setup(); const remote = Fastify(); const posts: JsonObject[] = [];
  const source = accountSourceFixture();
  try {
    let receiverBasis: JsonObject | null = null;
    let remoteAccount = structuredClone(source.snapshot);
    const accepted: JsonObject[] = [], gets: string[] = [], receipts = new Map<string, JsonObject>();
    for (const prefix of ['', '/alias']) {
      const identity = { clientId: prefix ? 'alias-client' : source.connection.clientId, accountId: prefix ? 'alias-account' : source.connection.accountId, mode: 'paper', currency: 'USDC' };
      remote.get(`${prefix}/capabilities`, () => ({ ...identity, capitalExit: { version: 1, supported: false, readable: true },
        automaticTargets: { supported: true, revisionBound: true, snapshotBound: true } }));
      remote.get(`${prefix}/capital-exits`, () => ({ ...identity, accountSourceId: remoteAccount.accountSourceId, observationId: remoteAccount.observationId,
        controlRevision: remoteAccount.controlRevision, reserved: '0', blocked: false, exits: [] }));
      remote.post(`${prefix}/targets`, request => {
        const body = json(request.body); posts.push(body); const allocation = object(body.allocation);
        // An actual atomic receiver-side contract: it uses its own current
        // physical state, not an echoed caller assertion of readiness.
        const matches = receiverBasis !== null && ['accountSourceId', 'observationId', 'snapshotSequence', 'nativeClockNs', 'controlRevision', 'reserved']
          .every(key => allocation[key] === receiverBasis![key]);
        if (matches) accepted.push(body);
        const receipt = { ...body, status: matches ? 'ACKNOWLEDGED' : 'REJECTED', externalId: `receiver:${body.id}`,
          reason: matches ? 'original_snapshot_group_matched_atomically' : 'snapshot_group_changed' };
        receipts.set(String(body.id), receipt); return receipt;
      });
      remote.get<{ Params: { id: string } }>(`${prefix}/targets/:id`, (request, reply) => { gets.push(request.params.id); return request.params.id === 'controller:expired-prior'
        ? { id: request.params.id, clientId: prefix ? 'alias-client' : source.connection.clientId, accountId: prefix ? 'alias-account' : source.connection.accountId,
          expiresAt: nsTime('1000000'), status: 'EXPIRED', externalId: 'expired-prior' }
        : receipts.get(request.params.id) ?? reply.code(503).send({ error: 'original_receipt_still_unknown' }); });
      remote.get(`${prefix}/account`, () => ({ ...remoteAccount, clientId: prefix ? 'alias-client' : source.connection.clientId, accountId: prefix ? 'alias-account' : source.connection.accountId }));
    }
    const url = await remote.listen({ host: '127.0.0.1', port: 0 });
    const connections = [{ ...source.connection, id: 'controller:original', url },
      { ...source.connection, id: 'controller:alias', url: `${url}/alias`, clientId: 'alias-client', accountId: 'alias-account' }];
    state.options.config.connections = connections;
    const dataset = state.options.config.datasets[0]!; dataset.currency = 'USDC';
    dataset.data.request = { protocol_version: 1, train_end_ns: '100', validation_end_ns: '200', forecast: { schema_version: 1,
      selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300', maximum_rows: 100 }, parameters: {} } };
    await state.restart();
    const controls = await Promise.all(connections.map(connection => readCapitalControl(connection, 'USDC')));
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.accountControls ??= {};
      for (const connection of connections) {
        const fact: ObservationFact = { ...structuredClone(source.observation), connection, taskId: 0,
          input: { ...source.observation.input, connectionId: connection.id },
          view: { ...source.observation.view, id: connection.id },
          snapshot: { ...structuredClone(source.snapshot), clientId: connection.clientId, accountId: connection.accountId } };
        if (keepMonitor) fact.view.receipt = { id: 'controller:expired-prior', status: 'REJECTED', externalId: 'expired-prior', expiresAt: nsTime('1000000'), acknowledgedAt: null };
        app.observations[connection.id] = json(fact);
        // The actual existing alias reconciler joins both transport names to
        // one original physical account. No separate fake lock or alias table.
        recordCapitalControl(app.accountControls, connection, controls[connections.indexOf(connection)]!);
      }
      assert.equal(accountKey(connections[0]!, app.accountControls), accountKey(connections[1]!, app.accountControls));
    }, context);
    const start = (requestId: string, alias = 0, authority: 'AUTHORIZED_TARGETS' | 'RESEARCH_ONLY' = 'AUTHORIZED_TARGETS') => state.service.app.inject({
      method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: {
        requestId, goal: 'Control ownership protocol only; no investment qualification asserted.', datasetId: 'controlled', model: state.model,
        funding: { kind: 'ACCOUNT', accountBindingId: connections[alias]!.id }, maxTrials: 1, businessPreference: 'NET_RETURN', operatingAuthority: authority } });
    const stopMonitors = async () => {
      const document = pmState(await state.service.research.harness.snapshot(PmDocument, context));
      for (const record of Object.values(document.mandates)) if (record.monitorTaskId) {
        await state.service.research.harness.abortTask(record.monitorTaskId as TaskId, context);
        await state.service.research.harness.waitForTask(record.monitorTaskId as TaskId, context);
      }
    };
    return { state, connections, posts, accepted, gets, start, stopMonitors,
      receiptStatus(id: string, status: 'REVOKED') { const original = receipts.get(id); assert.ok(original); receipts.set(id, { ...original, status }); },
      receiverCurrentBasis(value: JsonObject) { receiverBasis = structuredClone(value); },
      accountSnapshot(value: JsonObject) { remoteAccount = structuredClone(value); },
      async cleanup() { await state.cleanup(); await remote.close(); } };
  } catch (error) { await state.cleanup(); await remote.close(); throw error; }
}

test('one physical account admits exactly one simultaneous authorized PM controller across joined aliases', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    const responses = await Promise.all([f.start('controller:first', 0), f.start('controller:second', 1)]);
    await f.stopMonitors();
    assert.deepEqual(responses.map(response => response.statusCode).sort(), [202, 409], responses.map(response => response.body).join('\n'));
    const winner = responses.findIndex(response => response.statusCode === 202);
    const view = responses[winner]!.json<PmMandateView>();
    const replay = await f.start(winner === 0 ? 'controller:first' : 'controller:second', winner);
    assert.equal(replay.statusCode, 202); assert.equal(replay.json<PmMandateView>().mandate.id, view.mandate.id);
    const research = await f.start('controller:research-only', 1 - winner, 'RESEARCH_ONLY');
    assert.equal(research.statusCode, 202, research.body); await f.stopMonitors();
    const paused = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/strategies/${encodeURIComponent(view.strategies[0]!.id)}/pause`,
      headers: f.state.headers, payload: { requestId: 'controller:pause', expectedRevision: view.mandate.revision } });
    assert.equal(paused.statusCode, 202, paused.body);
    assert.equal((await f.start('controller:no-handover-by-pause', 1 - winner)).statusCode, 409,
      'Pausing signal generation neither transfers ownership nor proves a flat account.');
    await f.state.restart();
    const blocked = await f.start('controller:new-after-restart', 1 - winner);
    assert.equal(blocked.statusCode, 409, blocked.body);
    const stored = pmState(await f.state.service.research.harness.snapshot(PmDocument, context));
    assert.equal(Object.values(stored.mandates).filter(record => record.mandate.operatingAuthority === 'AUTHORIZED_TARGETS').length, 1);
    assert.equal(f.posts.length, 0, 'Controller admission does not fabricate scientific qualification or offer a target.');
  } finally { await f.cleanup(); }
});

test('retiring an account controller cannot transfer ownership while residual-position responsibility remains open', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    const admitted = await f.start('controller:retiring'); assert.equal(admitted.statusCode, 202, admitted.body); await f.stopMonitors();
    const view = admitted.json<PmMandateView>();
    const response = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(view.mandate.id)}/retire`, headers: f.state.headers,
      payload: { requestId: 'controller:retire', expectedRevision: view.mandate.revision } });
    assert.equal(response.statusCode, 202, response.body);
    const retired = response.json<PmMandateView>();
    assert.equal(retired.retirements[0]!.signalGeneration, 'STOPPED');
    assert.equal(retired.retirements[0]!.reconciliationRequired, true);
    const next = await f.start('controller:premature-successor', 1); await f.stopMonitors();
    assert.equal(next.statusCode, 409, next.body);
    assert.equal(f.posts.length, 0);
  } finally { await f.cleanup(); }
});

for (const existing of ['UNCERTAIN_RECEIPT', 'ACTIVE_AUTOMATIC', 'ACKNOWLEDGED_UNEXPIRED'] as const) {
  test(`a legacy ${existing} blocks new PM control across the physical-account alias`, { timeout: 30_000 }, async () => {
    const f = await fixture();
    try {
      await f.state.service.research.harness.commit(async tx => {
        const app = await tx.doc(Application); const prior = observation(app.observations[f.connections[0]!.id]);
        if (existing !== 'ACTIVE_AUTOMATIC') prior.view.receipt = { id: 'legacy:original-uncertain', status: existing === 'UNCERTAIN_RECEIPT' ? 'UNCERTAIN' : 'ACKNOWLEDGED', externalId: null,
          expiresAt: nsTime((BigInt(nowNs()) + 60_000_000_000n).toString()), acknowledgedAt: null };
        else prior.view.automaticPolicy = { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 };
        app.observations[f.connections[0]!.id] = json(prior);
      }, context);
      const result = await f.start(`controller:blocked-by-${existing}`, 1); await f.stopMonitors();
      assert.equal(result.statusCode, 409, result.body);
      assert.equal(Object.keys(pmState(await f.state.service.research.harness.snapshot(PmDocument, context)).mandates).length, 0);
      const prior = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connections[0]!.id]);
      if (existing !== 'ACTIVE_AUTOMATIC') assert.equal(prior.view.receipt?.id, 'legacy:original-uncertain');
      else assert.equal(prior.view.automaticPolicy?.enabled, true);
      assert.equal(f.posts.length, 0);
    } finally { await f.cleanup(); }
  });
}

test('an admitted PM controller also blocks legacy automatic enablement on its account alias', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    const first = await f.start('controller:pm-first'); assert.equal(first.statusCode, 202, first.body); await f.stopMonitors();
    const record = pmState(await f.state.service.research.harness.snapshot(PmDocument, context)).mandates[first.json<PmMandateView>().mandate.id]!;
    // The legacy path needs its original portfolio owner. This explicit
    // protocol fixture makes a missing portfolio unable to hide the conflict.
    await f.state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); const prior = observation(app.observations[f.connections[1]!.id]);
      const portfolio: PortfolioFact = { input: { requestId: 'legacy:portfolio', name: 'Legacy control protocol', researchIds: [record.conversationId], datasetId: 'controlled', capital: '1000' },
        view: { id: prior.input.portfolioId, name: 'Legacy control protocol', researchIds: [record.conversationId], datasetId: 'controlled',
          status: 'COMPUTED', capital: '1000', currency: 'USDC', createdAt: new Date().toISOString(), taskId: null,
          metrics: [], curve: [], targets: [], error: null }, evidence: [], policy: {}, native: null, target: {} };
      app.portfolios[prior.input.portfolioId] = json(portfolio);
    }, context);
    const response = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${encodeURIComponent(f.connections[1]!.id)}/automatic`, headers: f.state.headers,
      payload: { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 } });
    assert.equal(response.statusCode, 409, response.body);
    const prior = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connections[1]!.id]);
    assert.notEqual(prior.view.automaticPolicy?.enabled, true);
    assert.equal(f.posts.length, 0);
  } finally { await f.cleanup(); }
});

async function controlledTarget(f: Awaited<ReturnType<typeof fixture>>, id: string, decisionId: string, ttlSeconds = 60) {
  let originalBasis!: JsonObject;
  await f.state.service.research.harness.commit(async tx => {
    const record = pmState(await tx.doc(PmDocument)).mandates[id]!;
    const app = await tx.doc(Application);
    const source = observation(app.observations[f.connections[1]!.id]);
    app.portfolios[source.input.portfolioId] = json({ input: { requestId: 'controller:prior-portfolio', name: 'Control protocol original portfolio', researchIds: [record.conversationId], datasetId: 'controlled', capital: '1000' },
      view: { id: source.input.portfolioId, name: 'Control protocol original portfolio', researchIds: [record.conversationId], datasetId: 'controlled', status: 'COMPUTED', capital: '1000', currency: 'USDC', createdAt: new Date().toISOString(), taskId: null, metrics: [], curve: [], targets: [], error: null },
      evidence: [], policy: {}, native: null, target: {} } satisfies PortfolioFact);
    const basis = captureAccount(app, f.connections[1]!.id).basis;
    originalBasis = { accountSourceId: basis.accountSourceId, observationId: basis.observationId, snapshotSequence: basis.sequence,
      nativeClockNs: basis.observedAtNs, controlRevision: basis.controlRevision, reserved: basis.reserved!.amount };
    const money = (amount: string) => ({ amount, currency: basis.currency });
    // As in the existing operations protocol suite, explicit controlled
    // eligibility isolates transport. It is never a native investment claim.
    const decision: PortfolioDecision = { ...fact(decisionId), cycleId: 'TEST_ONLY_CYCLE', mandateRevisionId: id,
      strategyVersionId: record.latestStrategyId, ensembleRevisionId: null, strategyAllocations: [], policyRevisionId: record.policy.id,
      basisRevision: record.basisRevision, incumbentDecisionId: null, accountBasis: basis, researchStartingStateRef: null, action: 'CASH', status: 'CURRENT',
      assetTargets: [{ instrumentId: 'BTC.VENUE', navWeight: '0', targetQuantity: '0', targetNotional: money('0'), currentNotional: money('100'),
        netTradeNotional: money('-100'), quantityRuleRef: 'TEST_ONLY_RULE', riskContribution: null }],
      capital: { equity: money('1000'), reserved: money('0'), ownerLiquidityReserve: money('0'), deployableBudget: money('1000'),
        committedCapital: money('0'), uncommittedCash: money('1000'), projectedTradingCash: money('0'), capacityEstimateIds: [], sizingEvidenceRef: 'TEST_ONLY_SIZING' },
      comparisonRef: 'TEST_ONLY_COMPARISON', solverEvidenceRef: null, jointReplayEvidenceRef: null, reasonCodes: ['TEST_ONLY_TRANSPORT_ELIGIBILITY'],
      explanationFacts: { nativeTargetEvidenceRef: 'TEST_ONLY_ORIGINAL_TARGET' }, validUntilNs: (BigInt(nowNs()) + BigInt(ttlSeconds) * 1_000_000_000n).toString(),
      nextInvalidationDependencies: [], targetCommandId: null };
    const target = { schema_version: 1, asof_ns: nowNs(), valid_until_ns: decision.validUntilNs!, cash_weight: '1',
      targets: [{ instrument_id: 'BTC.VENUE', currency: basis.currency, weight: '0' }] };
    record.artifacts.TEST_ONLY_ORIGINAL_TARGET = target;
    record.evidence.TEST_ONLY_ORIGINAL_TARGET = { id: 'TEST_ONLY_ORIGINAL_TARGET', sourceKind: 'NATIVE_RESULT', role: 'CONTROL', runtime: null,
      objectKey: 'TEST_ONLY_ORIGINAL_TARGET', contentDigest: digest(target), mediaType: 'application/json', sourceRevision: decision.basisRevision,
      eventInterval: null, availableAtNs: nowNs(), clockPrecision: 'NANOSECOND', schemaId: 'TEST_ONLY_CONTROLLED_TARGET', visibility: 'OWNER_ONLY' };
    record.decisions[decisionId] = decision; record.currentDecisionId = decisionId; record.gaps = {};
    const assessment = { ...fact('TEST_ONLY_PAPER_ASSESSMENT'), strategyVersionId: record.latestStrategyId, stage: 'PAPER' as const, outcome: 'READY' as const,
      evaluatedAtNs: nowNs(), requirementsVersion: 'qz.pm.paper-shadow-source-target-qualification/1', evidenceRefs: [],
      reasonCodes: ['TEST_ONLY_TRANSPORT_ELIGIBILITY'], nextPermittedActions: [] };
    record.assessments[assessment.id] = assessment; record.strategies[record.latestStrategyId]!.pipelineAssessmentId = assessment.id;
  }, context);
  return originalBasis;
}

test('snapshot-bound receiver rejects a newer inventory sequence even when the capital-control revision is unchanged', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    const response = await f.start('controller:snapshot-bound', 1); assert.equal(response.statusCode, 202, response.body); await f.stopMonitors();
    const view = response.json<PmMandateView>(), id = view.mandate.id, decisionId = 'TEST_ONLY_CONTROLLER_DELIVERY';
    const originalBasis = await controlledTarget(f, id, decisionId);
    // The provider moves to a new inventory revision after QZ's last receipt.
    // QZ cannot know that change yet; only an actual receiver fence can reject it.
    f.receiverCurrentBasis({ ...originalBasis, snapshotSequence: (BigInt(String(originalBasis.snapshotSequence)) + 1n).toString() });
    const offered = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(id)}/targets`, headers: f.state.headers,
      payload: { requestId: 'controller:original-send', expectedRevision: '0', decisionId } });
    assert.equal(offered.statusCode, 202, offered.body);
    const deadline = Date.now() + 10_000; let record = pmState(await f.state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!;
    while (Date.now() < deadline && Object.values(record.commands)[0]?.state !== 'REJECTED') {
      await new Promise(resolve => setTimeout(resolve, 20));
      record = pmState(await f.state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!;
    }
    assert.equal(f.posts.length, 1); assert.equal(f.accepted.length, 0);
    const command = Object.values(record.commands)[0]!; assert.equal(command.state, 'REJECTED');
    assert.equal(object(f.posts[0]!.allocation).snapshotSequence, originalBasis.snapshotSequence);
    assert.equal(object(f.posts[0]!.allocation).controlRevision, originalBasis.controlRevision);
    assert.deepEqual(record.artifacts[command.externalReceiptRef!]!.allocation, f.posts[0]!.allocation);
    assert.equal(record.artifacts[command.externalReceiptRef!]!.reason, 'snapshot_group_changed');
    await f.state.restart();
    assert.equal(Object.values(pmState(await f.state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!.commands)[0]!.state, 'REJECTED');
    assert.equal(f.posts.length, 1, 'Definitive rejection cannot turn into an unsafe replacement after restart.');
  } finally { await f.cleanup(); }
});


for (const end of ['TTL', 'REVOKED'] as const) {
test(`an acknowledged target requires actual ${end} authority termination and a later flat report before account handover`, { timeout: 45_000 }, async () => {
  const f = await fixture(true);
  try {
    const response = await f.start('controller:ack-before-execution', 1); assert.equal(response.statusCode, 202, response.body);
    const initial = response.json<PmMandateView>(), id = initial.mandate.id, decisionId = 'TEST_ONLY_ACK_RETIREMENT';
    const campaignId = initial.campaigns[0]!.id;
    const paused = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/campaigns/${encodeURIComponent(campaignId)}/pause`, headers: f.state.headers,
      payload: { requestId: 'controller:pause-research-only', expectedRevision: initial.mandate.revision } });
    assert.equal(paused.statusCode, 202, paused.body);
    await until(async () => pmState(await f.state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!
      .artifacts[`campaign-control:${campaignId}`]?.state === 'APPLIED', 'Original research must pause while its real PM monitor continues.');
    f.receiverCurrentBasis(await controlledTarget(f, id, decisionId, end === 'TTL' ? 15 : 60));
    const offered = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(id)}/targets`, headers: f.state.headers,
      payload: { requestId: 'controller:accepted-unexpired', expectedRevision: '0', decisionId } });
    assert.equal(offered.statusCode, 202, offered.body);
    const read = async () => pmState(await f.state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!;
    let record = await read(); const deadline = Date.now() + 10_000;
    while (Date.now() < deadline && Object.values(record.commands)[0]?.state !== 'ACKNOWLEDGED') {
      await new Promise(resolve => setTimeout(resolve, 20)); record = await read();
    }
    const command = Object.values(record.commands)[0]!;
    assert.equal(command.state, 'ACKNOWLEDGED'); assert.equal(f.accepted.length, 1);
    assert.ok(BigInt(command.validUntilNs) > BigInt(nowNs()));
    const retired = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(id)}/retire`, headers: f.state.headers,
      payload: { requestId: 'controller:retire-accepted', expectedRevision: record.mandate.revision } });
    assert.equal(retired.statusCode, 202, retired.body);
    const disposition = retired.json<PmMandateView>().retirements[0]!;
    assert.equal(disposition.reconciliationRequired, true);
    const original = observation((await f.state.service.research.harness.snapshot(Application, context))!.observations[f.connections[1]!.id]).snapshot!;
    const flat = structuredClone(original), atNs = nowNs(), sequence = (BigInt(String(original.sequence)) + 1n).toString();
    flat.sequence = sequence; flat.nativeClockNs = atNs; flat.positions = [];
    const nativeObservation = object(flat.nativeObservation), native = object(nativeObservation.snapshot);
    nativeObservation.sequence = sequence; nativeObservation.observed_at_ns = atNs; native.ts_event = atNs; native.ts_init = atNs;
    const money = (amount: string) => ({ amount, currency: 'USDC' });
    native.balances = [{ total: money('1000'), free: money('1000'), locked: money('0') }]; native.total_equity = [money('1000')];
    flat.openOrders = { schema_version: 1, account_id: native.account_id!, asof_ns: atNs, available_ns: atNs, count: 0 };
    f.accountSnapshot(flat);
    await until(async () => {
      const app = (await f.state.service.research.harness.snapshot(Application, context))!;
      if (observation(app.observations[f.connections[1]!.id]).snapshot?.sequence !== sequence) return false;
      const capture = captureAccount(app, f.connections[1]!.id);
      return Object.values((await read()).retirements)[0]!.residualPositionsEvidenceRef === capture.evidence.id;
    }, 'The actual HTTP account reader and PM monitor must consume the flat original native inventory.');
    const observed = Object.values((await read()).retirements)[0]!;
    assert.equal(observed.reconciliationRequired, true, 'Flat inventory before original target expiry cannot terminate an accepted receiver instruction.');
    assert.notEqual(observed.positionDisposition, 'FLAT_CONFIRMED');
    assert.ok(observed.outstandingTargetCommandIds.includes(command.id));
    assert.ok(disposition.outstandingTargetCommandIds.includes(command.id),
      'ACK only confirms receipt. An original unexpired receiver target can execute after a flat account snapshot; it is still an exit obligation.');
    assert.equal((await f.start('controller:ack-is-not-handover')).statusCode, 409);
    await f.state.restart();
    assert.equal((await f.start('controller:ack-is-not-handover-after-restart')).statusCode, 409);
    assert.equal(f.posts.length, 1, 'Retirement must not submit a substitute liquidation or resend the acknowledged target.');
    if (end === 'TTL') {
      const remainingMs = Number((BigInt(command.validUntilNs) - BigInt(nowNs())) / 1_000_000n);
      if (remainingMs > 0) await new Promise(resolve => setTimeout(resolve, remainingMs + 20));
    } else {
      f.receiptStatus(command.idempotencyKey, 'REVOKED');
      await until(async () => (await read()).commands[command.id]!.state === 'REJECTED', 'The original retirement receipt watcher must GET and retain the authoritative revocation.');
      assert.ok(f.gets.includes(command.idempotencyKey));
      // A flat report may predate a fill which happened before revocation.
      // Only a later source report can close the possible resulting position.
      const lateOldFlat = structuredClone(flat), lateSequence = (BigInt(sequence) + 1n).toString();
      lateOldFlat.sequence = lateSequence; object(lateOldFlat.nativeObservation).sequence = lateSequence;
      f.accountSnapshot(lateOldFlat);
      await until(async () => {
        const capture = object(object((await read()).artifacts['latest-account-pending']).capture);
        return object(capture.basis).sequence === lateSequence;
      }, 'The real monitor must consume a later received heartbeat whose economic flat snapshot still predates revocation.');
    }
    assert.equal((await f.start('controller:termination-alone-is-not-flat-evidence')).statusCode, 409,
      'Expiry or a new revocation receipt cannot upgrade the older flat snapshot into evidence of the final post-target account.');
    assert.equal(Object.values((await read()).retirements)[0]!.reconciliationRequired, true);
    const afterExpiry = structuredClone(flat), closedAt = nowNs(), closedSequence = (BigInt(sequence) + 2n).toString();
    afterExpiry.sequence = closedSequence; afterExpiry.nativeClockNs = closedAt;
    const afterObservation = object(afterExpiry.nativeObservation), afterNative = object(afterObservation.snapshot);
    afterObservation.sequence = closedSequence; afterObservation.observed_at_ns = closedAt; afterNative.ts_event = closedAt; afterNative.ts_init = closedAt;
    afterExpiry.openOrders = { schema_version: 1, account_id: afterNative.account_id!, asof_ns: closedAt, available_ns: closedAt, count: 0 };
    f.accountSnapshot(afterExpiry);
    await until(async () => Object.values((await read()).retirements)[0]!.reconciliationRequired === false,
      'An actual post-expiry flat account report must settle the original target responsibility.');
    const closed = Object.values((await read()).retirements)[0]!;
    assert.equal(closed.positionDisposition, 'FLAT_CONFIRMED'); assert.deepEqual(closed.outstandingTargetCommandIds, []);
    const source = await f.state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(closed.residualPositionsEvidenceRef!)}`, headers: f.state.headers });
    assert.equal(source.statusCode, 200, source.body);
    assert.equal(object(object(source.json().payload).snapshot).sequence, closedSequence);
    const successor = await f.start('controller:authoritative-flat-handover');
    assert.equal(successor.statusCode, 202, successor.body); await f.stopMonitors();
    assert.equal(f.posts.length, 1, 'A reconciled handover does not resend or liquidate the historical target.');
  } finally { await f.cleanup(); }
});
}

async function until(check: () => Promise<boolean>, reason: string) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) { if (await check()) return; await new Promise(resolve => setTimeout(resolve, 20)); }
  assert.fail(reason);
}
