import assert from 'node:assert/strict';
import { test } from 'node:test';
import fs from 'node:fs';
import Fastify from 'fastify';
import { syncBuiltinESMExports } from 'node:module';
import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { awaitWithContext } from '@earendil-works/chord/context';
import type { ConversationId } from '@earendil-works/pi-durable';
import type { DeletionReceipt } from '../../../packages/contracts/src/index.ts';
import { Study, Requests } from '../../research/src/documents.ts';
import { Application, json, observation, portfolio } from '../src/facts.ts';
import type { ObservationFact } from '../src/facts.ts';
import { context, setup, studied, built, fauxAssistantMessage, fauxToolCall } from './support.ts';

const api = '/api/v3';
type State = Awaited<ReturnType<typeof setup>>;
async function remove(state: State, kind: string, id: string | number) {
  return state.service.app.inject({ method: 'DELETE', url: `${api}/${kind}/${encodeURIComponent(String(id))}`, headers: state.headers });
}
async function factors(state: State) {
  const response = await state.service.app.inject({ url: `${api}/factors`, headers: state.headers });
  assert.equal(response.statusCode, 200, response.body); return response.json() as { id: string; researchId: number }[];
}
async function expectError(response: Awaited<ReturnType<typeof remove>>, status: number, error: string) {
  assert.equal(response.statusCode, status, response.body); assert.equal(response.json().error, error);
}
async function expectReceipt(response: Awaited<ReturnType<typeof remove>>, kind: DeletionReceipt['kind'], id: string | number) {
  assert.equal(response.statusCode, 200, response.body);
  const value = response.json() as DeletionReceipt; assert.equal(value.kind, kind); assert.equal(value.id, String(id));
  assert.ok(Number.isFinite(Date.parse(value.deletedAt))); return value;
}

test('factor deletion removes only its owner projection; original ledger, request identity and tombstone survive restart', async () => {
  const state = await setup();
  try {
    const { input, receipt } = await studied(state); const id = receipt.conversationId as ConversationId;
    const factor = (await factors(state))[0]!;
    const before = structuredClone((await state.service.research.harness.snapshot(Study, id, context))!);
    const deleted = await expectReceipt(await remove(state, 'factors', factor.id), 'factor', factor.id);
    assert.deepEqual(await factors(state), []);
    for (const suffix of ['', '/capacity']) await expectError(await state.service.app.inject({ url: `${api}/factors/${factor.id}${suffix}`, headers: state.headers }), 404, 'factor_not_found');
    const detail = await state.service.app.inject({ url: `${api}/research/${id}`, headers: state.headers });
    assert.equal(detail.statusCode, 200, detail.body); assert.deepEqual(detail.json().factors ?? [], []);
    const after = (await state.service.research.harness.snapshot(Study, id, context))!;
    assert.deepEqual(after.trials, before.trials); assert.deepEqual(after.final, before.final); assert.deepEqual(after.input, before.input);
    assert.equal(after.deletedFactors?.[factor.id], deleted.deletedAt);
    const legacy = { requestId: 'deleted-member-legacy', name: 'Must reject deleted original member', researchIds: [id], datasetId: 'controlled', capital: '1' };
    for (const payload of [legacy, { ...legacy, requestId: 'deleted-member-atomic', researchIds: undefined, factors: [{ factorId: factor.id, weight: '1' }] }]) {
      await expectError(await state.service.app.inject({ method: 'POST', url: `${api}/portfolios`, headers: state.headers, payload }), 409, 'factor_deleted');
    }
    await state.restart();
    assert.deepEqual(await factors(state), []);
    assert.deepEqual((await remove(state, 'factors', factor.id)).json(), deleted);
    const replay = await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: input });
    assert.equal(replay.statusCode, 202, replay.body); assert.deepEqual(replay.json(), receipt);
    assert.deepEqual(await factors(state), []);
    assert.deepEqual((await state.service.research.harness.snapshot(Study, id, context))!.trials, before.trials);
  } finally { await state.cleanup(); }
});

test('research deletion hides its factors without erasing admissions or trials and cannot be resumed or resurrected', async () => {
  const state = await setup();
  try {
    const { input, receipt } = await studied(state); const id = receipt.conversationId as ConversationId;
    const original = structuredClone((await state.service.research.harness.snapshot(Study, id, context))!);
    assert.equal((await factors(state)).length, 1);
    const calls = state.faux.state.callCount;
    const deleted = await expectReceipt(await remove(state, 'research', id), 'research', id);
    assert.deepEqual(await factors(state), []);
    assert.deepEqual((await state.service.app.inject({ url: `${api}/research`, headers: state.headers })).json().items, []);
    for (const suffix of ['', '/diagnostics']) await expectError(await state.service.app.inject({ url: `${api}/research/${id}${suffix}`, headers: state.headers }), 404, 'research_not_found');
    const stored = (await state.service.research.harness.snapshot(Study, id, context))!;
    assert.deepEqual(stored.trials, original.trials); assert.deepEqual(stored.final, original.final); assert.deepEqual(stored.input, original.input);
    assert.equal(stored.deletedAt, deleted.deletedAt);
    assert.ok((await state.service.research.harness.snapshot(Requests, context))!.requests[`request:${input.requestId}`]);
    assert.deepEqual((await state.service.work.snapshot())!.researchRequests[`request:${input.requestId}`]!.original, input);
    await state.restart();
    assert.deepEqual((await remove(state, 'research', id)).json(), deleted);
    await expectError(await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: input }), 409, 'research_deleted');
    await expectError(await state.service.app.inject({ method: 'POST', url: `${api}/research/${id}/resume`, headers: state.headers, payload: { submissionId: receipt.submissionId } }), 409, 'research_deleted');
    await expectError(await state.service.app.inject({ method: 'POST', url: `${api}/portfolios`, headers: state.headers,
      payload: { requestId: 'deleted-study-consumer', name: 'Reject old research', researchIds: [id], datasetId: 'controlled', capital: '1' } }), 409, 'research_deleted');
    assert.equal(state.faux.state.callCount, calls);
  } finally { await state.cleanup(); }
});

test('portfolio references block member deletion; deleting portfolio retains frozen evidence and releases that dependency', async () => {
  const state = await setup();
  try {
    const { input, view } = await built(state); const id = view.researchIds[0]!;
    const factor = (await factors(state))[0]!;
    const original = structuredClone(await state.service.work.getPortfolio(view.id));
    await expectError(await remove(state, 'research', id), 409, 'research_delete_portfolio_in_use');
    await expectError(await remove(state, 'factors', factor.id), 409, 'factor_delete_portfolio_in_use');
    const deleted = await expectReceipt(await remove(state, 'portfolios', view.id), 'portfolio', view.id);
    assert.deepEqual((await state.service.app.inject({ url: `${api}/portfolios`, headers: state.headers })).json(), []);
    await expectError(await state.service.app.inject({ url: `${api}/portfolios/${view.id}`, headers: state.headers }), 404, 'portfolio_not_found');
    const stored = portfolio((await state.service.work.snapshot())!.portfolios[view.id]);
    assert.deepEqual(stored.input, original.input); assert.deepEqual(stored.evidence, original.evidence);
    assert.deepEqual(stored.native, original.native); assert.deepEqual(stored.target, original.target); assert.deepEqual(stored.view, original.view);
    await state.restart();
    assert.deepEqual((await remove(state, 'portfolios', view.id)).json(), deleted);
    await expectError(await state.service.app.inject({ method: 'POST', url: `${api}/portfolios`, headers: state.headers, payload: input }), 409, 'portfolio_deleted');
    await expectError(await state.service.app.inject({ method: 'POST', url: `${api}/observations`, headers: state.headers,
      payload: { requestId: 'deleted-portfolio-consumer', portfolioId: view.id, mode: 'paper', connectionId: 'not-configured' } }), 404, 'portfolio_not_found');
    await expectReceipt(await remove(state, 'factors', factor.id), 'factor', factor.id);
    await expectReceipt(await remove(state, 'research', id), 'research', id);
  } finally { await state.cleanup(); }
});

test('deletes require owner authentication and reject invalid/nonexistent identities without creating facts', async () => {
  const state = await setup();
  try {
    for (const [kind, id] of [['research', 'no-number'], ['factors', 'r999999.t1.m0'], ['portfolios', 'missing'], ['mandates', 'missing']]) {
      const response = await state.service.app.inject({ method: 'DELETE', url: `${api}/${kind}/${id}` });
      assert.equal(response.statusCode, 401, response.body);
    }
    await expectError(await remove(state, 'research', 'no-number'), 400, 'invalid_research_id');
    await expectError(await remove(state, 'research', '999999'), 404, 'research_not_found');
    await expectError(await remove(state, 'factors', 'invalid-factor'), 400, 'factor_id_invalid');
    await expectError(await remove(state, 'factors', 'r999999.t1.m0'), 404, 'research_not_found');
    await expectError(await remove(state, 'portfolios', 'missing'), 404, 'portfolio_not_found');
    assert.deepEqual((await state.service.app.inject({ url: `${api}/research`, headers: state.headers })).json().items, []);
    assert.deepEqual((await state.service.work.snapshot())!.portfolios, {});
  } finally { await state.cleanup(); }
});

test('running native submission rejects deletion and remains running until the distinct owner stop completes', { timeout: 15_000 }, async () => {
  const state = await setup();
  let entered!: () => void; const running = new Promise<void>(resolve => { entered = resolve; });
  const blocker = defineExtension({ name: 'test.deletion-blocker', tools: [defineTool({ name: 'deletion_block', description: 'TEST_ONLY active native submission', parameters: Type.Object({}),
    async execute(_args, _api, ctx) { entered(); await awaitWithContext(new Promise<void>(() => {}), ctx); return {}; },
  })] });
  try {
    state.options.extensions.push(blocker); await state.restart();
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('deletion_block', {}), { stopReason: 'toolUse' })]);
    const start = await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers,
      payload: { requestId: 'running-deletion', goal: 'Controlled active task', model: state.model, datasetId: 'controlled', maxTrials: 1 } });
    assert.equal(start.statusCode, 202, start.body); const { conversationId } = start.json(); await running;
    await expectError(await remove(state, 'research', conversationId), 409, 'research_delete_tasks_active');
    assert.equal((await state.service.research.state(conversationId)).submission!.status, 'placed');
    assert.equal((await state.service.research.harness.snapshot(Study, conversationId, context))!.deletedAt, undefined);
    const stop = await state.service.app.inject({ method: 'POST', url: `${api}/research/${conversationId}/stop`, headers: state.headers });
    assert.equal(stop.statusCode, 202, stop.body);
    await expectReceipt(await remove(state, 'research', conversationId), 'research', conversationId);
  } finally { await state.cleanup(); }
});

// Suspend only the real filesystem copy, then delete its original source member
// before the final transaction. This is software admission-race evidence.
test('portfolio final admission rechecks deletion after its actual catalog capture', async t => {
  const state = await setup(); let release!: () => void; const resumed = new Promise<void>(resolve => { release = resolve; });
  let reached!: () => void; const capturing = new Promise<void>(resolve => { reached = resolve; });
  const originalCp = fs.promises.cp;
  try {
    const { receipt } = await studied(state); const id = receipt.conversationId;
    const mocked = t.mock.method(fs.promises, 'cp', async (...args: Parameters<typeof originalCp>) => {
      await originalCp(...args); reached(); await resumed;
    });
    syncBuiltinESMExports();
    const admission = state.service.app.inject({ method: 'POST', url: `${api}/portfolios`, headers: state.headers,
      payload: { requestId: 'capture-race', name: 'Original member deleted during capture', researchIds: [id], datasetId: 'controlled', capital: '1' } }).then(value => value);
    await capturing;
    await expectReceipt(await remove(state, 'research', id), 'research', id);
    release(); const rejected = await admission;
    await expectError(rejected, 409, 'research_deleted');
    assert.deepEqual((await state.service.work.snapshot())!.portfolios, {});
    mocked.mock.restore(); syncBuiltinESMExports();
  } finally { release(); t.mock.restoreAll(); syncBuiltinESMExports(); await state.cleanup(); }
});


test('portfolio deletion preserves stopped observation facts and refuses unresolved account/receipt responsibilities', async () => {
  const state = await setup();
  try {
    const { view } = await built(state);
    const source = await state.service.work.getPortfolio(view.id);
    // Explicit TEST_ONLY saved owner facts exercise deletion admission; they do
    // not claim downstream cancellation, position flatness or real account use.
    const saved: ObservationFact = {
      input: { requestId: 'archived-observation', portfolioId: view.id, mode: 'paper', connectionId: 'saved-test-binding' },
      view: { id: 'saved-test-observation', name: 'Protocol responsibility fixture', portfolioId: view.id, mode: 'paper', status: 'OBSERVING',
        currency: view.currency, equity: null, cash: null, updatedAt: null, positions: [], curve: [], error: null,
        receipt: { id: 'original-saved-target-receipt', status: 'ACKNOWLEDGED', externalId: 'original-external', expiresAt: '2999-01-01T00:00:00Z', acknowledgedAt: null } },
      connection: { id: 'saved-test-binding', name: 'TEST_ONLY', mode: 'paper', url: 'http://127.0.0.1:1', clientId: 'test-client', accountId: 'test-account' },
      target: source.target!, taskId: view.taskId!, snapshot: null,
    };
    async function save(fact: ObservationFact) {
      await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).observations[fact.view.id] = json(fact); }, context);
    }
    await save(saved);
    await expectError(await remove(state, 'portfolios', view.id), 409, 'portfolio_delete_observation_in_use');
    saved.view.status = 'STOPPED'; await save(saved);
    await expectError(await remove(state, 'portfolios', view.id), 409, 'portfolio_delete_observation_in_use');
    saved.view.receipt!.expiresAt = '2000-01-01T00:00:00Z';
    for (const receiptStatus of ['PENDING', 'UNCERTAIN']) {
      saved.view.receipt!.status = receiptStatus; await save(saved);
      await expectError(await remove(state, 'portfolios', view.id), 409, 'portfolio_delete_observation_in_use');
    }
    saved.view.receipt!.status = 'ACKNOWLEDGED';
    saved.view.automaticPolicy = { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 60 }; await save(saved);
    await expectError(await remove(state, 'portfolios', view.id), 409, 'portfolio_delete_observation_in_use');
    saved.view.automaticPolicy.enabled = false; await save(saved);
    await expectReceipt(await remove(state, 'portfolios', view.id), 'portfolio', view.id);
    assert.deepEqual(observation((await state.service.work.snapshot())!.observations[saved.view.id]), saved);
    const observations = await state.service.app.inject({ url: `${api}/observations`, headers: state.headers });
    assert.equal(observations.statusCode, 200, observations.body); assert.equal(observations.json()[0].receipt.id, saved.view.receipt!.id);
    await state.restart();
    assert.deepEqual(observation((await state.service.work.snapshot())!.observations[saved.view.id]), saved);
  } finally { await state.cleanup(); }
});


test('observation final admission rechecks a portfolio deleted during the real downstream capability read', async () => {
  const state = await setup(); const downstream = Fastify(); let offers = 0;
  let reached!: () => void; const reading = new Promise<void>(resolve => { reached = resolve; });
  let release!: () => void; const released = new Promise<void>(resolve => { release = resolve; });
  downstream.get('/capabilities', () => ({ clientId: 'test-client', accountId: 'test-account', mode: 'paper', currency: 'USD',
    capitalExit: { version: 1, supported: false, readable: true } }));
  downstream.get('/capital-exits', async () => {
    reached(); await released;
    return { clientId: 'test-client', accountId: 'test-account', mode: 'paper', currency: 'USD',
      accountSourceId: '01900000-0000-7000-8000-000000000001', observationId: '01900000-0000-7000-8000-000000000002', controlRevision: '1', reserved: '0', exits: [] };
  });
  downstream.post('/targets', () => { offers++; return { error: 'must_not_offer_deleted_portfolio' }; });
  try {
    const url = await downstream.listen({ host: '127.0.0.1', port: 0 });
    state.options.config.connections = [{ id: 'delete-race', name: 'TEST_ONLY read race', mode: 'paper', url, clientId: 'test-client', accountId: 'test-account' }];
    await state.restart(); const { view } = await built(state);
    const admission = state.service.app.inject({ method: 'POST', url: `${api}/observations`, headers: state.headers,
      payload: { requestId: 'original-observation-race', portfolioId: view.id, mode: 'paper', connectionId: 'delete-race' } }).then(value => value);
    await reading; await expectReceipt(await remove(state, 'portfolios', view.id), 'portfolio', view.id);
    release(); await expectError(await admission, 409, 'portfolio_deleted');
    assert.deepEqual((await state.service.work.snapshot())!.observations, {}); assert.equal(offers, 0);
  } finally { release(); await state.cleanup(); await downstream.close(); }
});
