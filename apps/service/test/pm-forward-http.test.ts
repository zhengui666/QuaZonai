import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { TaskId } from '@earendil-works/pi-durable';
import { Application, json } from '../src/facts.ts';
import { fact, PmDocument, pmRecord, pmState, type PmRecord } from '../src/pm-facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

test('forward strategy pause revokes its pending decision without liquidating inventory and survives original Durable recovery', async () => {
  const state = await setup(); const source = accountSourceFixture(); const id = 'existing:account'; const strategyId = 'existing:strategy';
  try {
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.observations[source.connection.id] = json(source.observation);
      Object.assign(app.accountControls ??= {}, source.application.accountControls);
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      pmState(await tx.doc(PmDocument)).mandates[id] = {
        input: { datasetId: 'controlled' }, conversationId: conversation.id,
        mandate: { ...fact(id), capitalSource: { kind: 'ACCOUNT', accountBindingId: source.connection.id }, operatingAuthority: 'RESEARCH_ONLY' },
        policy: { id: 'existing:policy', validDomain: {} }, datasets: [],
        campaigns: { original: { ...fact('original'), state: 'CONCLUDED', datasetRevisionIds: [], runtime: { conversationId: conversation.id, taskId: null } } },
        strategies: { [strategyId]: { ...fact(strategyId), definitionId: 'existing:definition' } }, assessments: {}, events: {}, cycles: {},
        decisions: { decision: { ...fact('decision'), status: 'CURRENT', strategyVersionId: strategyId, reasonCodes: [], targetCommandId: null } },
        comparisons: {}, capacities: {}, ensembles: {}, evidence: {}, artifacts: {}, outcomes: {}, attributions: {}, retirements: {}, commands: {}, gaps: {},
        derivedTrials: {}, budgetMovements: {}, searchFamily: {}, latestStrategyId: strategyId, currentDecisionId: 'decision', latestCycleId: null,
        basisRevision: 'existing:basis', monitorTaskId: null,
      } as unknown as PmRecord;
    }, context);
    const account = (await state.service.research.harness.snapshot(Application, context))!;
    const payload = { requestId: 'pause-original', expectedRevision: '0' }; const url = `/api/v3/strategies/${encodeURIComponent(strategyId)}/pause`;
    assert.equal((await state.service.app.inject({ method: 'POST', url, payload })).statusCode, 401);
    const paused = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload });
    assert.equal(paused.statusCode, 202, paused.body);
    const read = async () => pmRecord(pmState(await state.service.pm.snapshot()), id);
    const pending = await read(); const control = pending.artifacts['campaign-control:original']!;
    await state.service.research.harness.waitForTask(control.taskId as TaskId, context);
    const completed = await read();
    assert.equal(completed.decisions.decision!.status, 'REVOKED');
    assert.ok(completed.decisions.decision!.reasonCodes.includes('owner_strategy_paused'));
    assert.equal(completed.currentDecisionId, null); assert.deepEqual(completed.commands, {});
    assert.equal(completed.artifacts['campaign-control:original']!.state, 'APPLIED');
    assert.deepEqual((await state.service.research.harness.snapshot(Application, context))!.observations, account.observations);
    assert.deepEqual((await state.service.research.harness.snapshot(Application, context))!.accountControls, account.accountControls);
    await state.restart(); assert.deepEqual(await read(), completed);
    const replay = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload });
    assert.equal(replay.statusCode, 202, replay.body); assert.deepEqual(await read(), completed);
    const removed = await state.service.app.inject({ method: 'POST', url: '/api/v3/campaigns/original/resume', headers: state.headers, payload });
    assert.equal(removed.statusCode, 410); assert.deepEqual(await read(), completed);
  } finally { await state.cleanup(); }
});
