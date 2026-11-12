import { readResearchAdmissions } from '../src/admissions.ts';
import { assertFrozenInput } from '../../research/test/frozen-input.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { fauxAssistantMessage } from '@earendil-works/pi-ai';
import { setup, studied, built, context } from './support.ts';
import { Application, json } from '../src/facts.ts';
import { Requests } from '../../research/src/documents.ts';
import { retainTarget } from './retained-target.ts';
import { acceptSnapshot } from '../src/downstream.ts';

const api = '/api/v3';
test('owner research admission rejects retired execution quota fields', async () => {
  const state = await setup();
  try {
    const response = await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers,
      payload: { requestId: 'retired-quota', goal: 'Research', model: state.model, datasetId: 'controlled', maxTrials: 0 } });
    assert.equal(response.statusCode, 400); assert.equal(response.json().error, 'research_field_unsupported');
    assert.equal(state.faux.state.callCount, 0);
  } finally { await state.cleanup(); }
});
test('shared-source snapshot equality normalizes only known transport aliases, keeps native facts and validates every binding', () => {
  const connection = { id: 'TEST_ONLY', name: 'TEST_ONLY', url: 'http://127.0.0.1:1', mode: 'paper', clientId: 'current-client', accountId: 'current-account' };
  const fresh = () => ({ connection, snapshot: null, view: { currency: 'USD', positions: [], curve: [] } } as any);
  const snapshot = { clientId: connection.clientId, accountId: connection.accountId, mode: 'paper', currency: 'USD', accountSourceId: 'TEST_ONLY-source', observationId: 'TEST_ONLY-observation', controlRevision: '7', sequence: '9007199254740995', nativeClockNs: '1000000000', equity: '1000', cash: '1000', positions: [], execution: { currency: 'USD', validUntilNs: '2000000000', liquidity: [] } };
  const peer = { ...fresh(), connection: { ...connection, clientId: 'alias-client', accountId: 'alias-account' }, snapshot: { ...snapshot, clientId: 'alias-client', accountId: 'alias-account' } };
  const current = fresh(); acceptSnapshot(current, snapshot, [peer]); assert.deepEqual(current.snapshot, snapshot); assert.equal(current.view.curve.length, 1);
  acceptSnapshot(current, snapshot, [peer]); assert.equal(current.view.curve.length, 1); assert.equal(peer.snapshot.clientId, 'alias-client');
  for (const change of [{ nativeClockNs: '1000000001' }, { equity: '900' }, { cash: null }, { positions: [{ instrumentId: 'A', quantity: '1', value: '1', pnl: null }] }, { execution: { ...snapshot.execution, validUntilNs: '3000000000' } }, { accountSourceId: 'OTHER-source' }, { observationId: 'OTHER-observation' }, { controlRevision: '8' }]) {
    const untouched = fresh(); assert.throws(() => acceptSnapshot(untouched, json({ ...snapshot, ...change }), [peer]), /account_snapshot_conflict/); assert.equal(untouched.snapshot, null); assert.equal(untouched.view.curve.length, 0);
  }
  for (const change of [{ sequence: '9007199254740994' }, { nativeClockNs: '999999999' }]) assert.throws(() => acceptSnapshot(fresh(), { ...snapshot, ...change }, [peer]), /account_snapshot_out_of_order/);
  assert.throws(() => acceptSnapshot(current, snapshot, [{ ...peer, snapshot: { ...peer.snapshot, cash: '999' } }]), /account_snapshot_conflict/, 'a local same-sequence replay must still check every joined alias');
  assert.throws(() => acceptSnapshot(fresh(), { ...snapshot, sequence: '9007199254740996', accountSourceId: 'OTHER-source' }, [peer]), /account_identity_mismatch/);
  assert.throws(() => acceptSnapshot(fresh(), { ...snapshot, mode: 'live' }, [peer]), /account_identity_mismatch/);
});
test('owner cookie/device auth, read-only GET, original research identity/replay/conflict and restart', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const app = state.service.app;
    assert.equal((await app.inject('/health/live')).statusCode, 204);
    const origin = 'https://cross-origin.example';
    const preflight = await app.inject({ method: 'OPTIONS', url: `${api}/settings`, headers: { origin, 'access-control-request-method': 'PATCH', 'access-control-request-headers': 'authorization, content-type, x-qz-test' } });
    assert.equal(preflight.statusCode, 204);
    assert.equal(preflight.headers['access-control-allow-origin'], origin);
    assert.equal(preflight.headers['access-control-allow-credentials'], 'true');
    assert.equal(preflight.headers['access-control-allow-methods'], 'PATCH');
    assert.equal(preflight.headers['access-control-allow-headers'], 'authorization, content-type, x-qz-test');
    const crossOrigin = await app.inject({ url: `${api}/settings`, headers: { ...state.headers, origin } });
    assert.equal(crossOrigin.statusCode, 200, crossOrigin.body);
    assert.equal(crossOrigin.headers['access-control-allow-origin'], origin);
    const unauthenticatedCrossOrigin = await app.inject({ url: `${api}/research`, headers: { origin } });
    assert.equal(unauthenticatedCrossOrigin.statusCode, 401);
    assert.equal((await app.inject(`${api}/research`)).statusCode, 401);
    assert.equal((await app.inject({ method: 'POST', url: `${api}/session/login`, payload: { password: 'incorrect' } })).statusCode, 401);
    for (const path of ['research', 'research/models', 'datasets', 'usage', 'portfolios', 'observations', 'settings', 'extensions']) assert.equal((await app.inject({ url: `${api}/${path}`, headers: state.headers })).statusCode, 200, path);
    assert.equal(state.faux.state.callCount, 0);
    const { input, receipt } = await studied(state);
    const replay = await app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: input }); assert.deepEqual(replay.json(), receipt);
    assert.equal((await app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: { ...input, goal: 'Changed objective' } })).statusCode, 409);
    const detail = (await app.inject({ url: `${api}/research/${receipt.conversationId}`, headers: state.headers })).json();
    assert.equal(detail.trialRecords.length, 1); assert.equal(detail.final.metrics.find((item: { key: string }) => item.key.endsWith('pearson_ic')).value, null);
    assert.deepEqual(detail.submission, { id: receipt.submissionId, status: 'done', requestId: input.requestId });
    state.faux.setResponses([fauxAssistantMessage('No scientific result claimed.')]);
    const original = { goal: 'Original full input', model: state.model, data: state.options.config.datasets[0]!.data, policy: {} };
    const full = await app.inject({ method: 'POST', url: `${api}/research`, headers: { ...state.headers, origin }, payload: { requestId: 'full-input', input: original } });
    assert.equal(full.statusCode, 202, full.body);
    await (await state.service.research.harness.submission(full.json().submissionId, context))!.wait(context);
    const fullDetail = (await app.inject({ url: `${api}/research/${full.json().conversationId}`, headers: state.headers })).json();
    assertFrozenInput(fullDetail.input, original); assert.equal(fullDetail.dataset, null); assert.equal(fullDetail.datasetId, null);
    const invalid = await app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers,
      payload: { requestId: 'invalid-full-input', input: { ...original, policy: { budget: 9007199254740992 } } } });
    assert.equal(invalid.statusCode, 400); assert.equal(invalid.json().error, 'invalid_json_number');
    assert.equal((await readResearchAdmissions(state.service.research))['request:invalid-full-input'], undefined);
    assert.equal((await state.service.research.harness.snapshot(Requests, context))!.requests['request:invalid-full-input'], undefined);
    state.faux.setResponses([fauxAssistantMessage('Original conversation retained.')]);
    const legacyInput = { goal: 'Existing durable admission', model: state.model, data: { previousCatalog: 'retired-original-source' }, policy: {} };
    const legacy = await state.service.research.start('existing-native-request', legacyInput);
    await (await state.service.research.harness.submission(legacy.submissionId, context))!.wait(context);
    assert.deepEqual((await app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: { requestId: 'existing-native-request', input: legacyInput } })).json(), legacy);
    const metadata = (await readResearchAdmissions(state.service.research))[`request:${input.requestId}`]!;
    assert.deepEqual(metadata.original, input);
    const calls = state.faux.state.callCount;
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: input })).json(), receipt);
    assert.equal(state.faux.state.callCount, calls);
    const device = (await state.service.app.inject({ method: 'POST', url: `${api}/devices/login`, payload: { password: 'local-owner-test', name: 'CLI test' } })).json();
    const headers = { authorization: `Bearer ${device.token}` };
    assert.equal((await state.service.app.inject({ url: `${api}/settings`, headers })).statusCode, 200);
    assert.equal((await state.service.app.inject({ method: 'DELETE', url: `${api}/devices/current`, headers })).statusCode, 204);
    assert.equal((await state.service.app.inject({ url: `${api}/settings`, headers })).statusCode, 401);
    assert.equal((await state.service.app.inject({ method: 'DELETE', url: `${api}/session`, headers: state.headers })).statusCode, 200);
    assert.equal((await state.service.app.inject({ url: `${api}/settings`, headers: state.headers })).statusCode, 401);
  } finally { await state.cleanup(); }
});

async function connection(state: Awaited<ReturnType<typeof setup>>) {
  const downstream = Fastify(); let posts = 0; let saved: Record<string, unknown> = {};
  let externalId = 'original-external-identity';
  const receipt = () => ({ ...saved, status: 'ACKNOWLEDGED', externalId, acknowledgedAt: '2026-09-30T00:00:00.123456789Z' });
  downstream.post('/targets', async (_request, reply) => {
    posts++; return reply.code(410).send({ error: 'unexpected_new_target' });
  });
  // TEST_ONLY readable native-owner facts; no capital-exit initiation capability.
  downstream.get('/capabilities', () => ({ clientId: 'client-original', accountId: 'account-original', mode: 'paper', currency: 'USD', capitalExit: { version: 2, supported: false, readable: true } }));
  downstream.get('/capital-exits', () => ({ clientId: 'client-original', accountId: 'account-original', mode: 'paper', currency: 'USD', accountSourceId: '01900000-0000-7000-8000-000000000001', observationId: '01900000-0000-7000-8000-000000000002', controlRevision: '1', reserved: '0', exits: [] }));
  downstream.get('/targets/:id', async () => receipt());
  downstream.get('/account', async () => ({ clientId: 'client-original', accountId: 'account-original', sequence: '9007199254740995', nativeClockNs: '1800000000123456789', currency: 'USD', equity: '1234.123456789012345678', cash: null, positions: [{ instrumentId: 'TEST.SIM', quantity: '0.000000000000000001', value: null, pnl: null }] }));
  const url = await downstream.listen({ host: '127.0.0.1', port: 0 });
  state.options.config.connections = [{ id: 'protocol-paper', name: 'Protocol only', mode: 'paper', url, clientId: 'client-original', accountId: 'account-original' }];
  await state.restart();
  return { downstream, setExternalId(value: string) { externalId = value; }, retain(value: Record<string, unknown>) { saved = value; }, get posts() { return posts; }, get saved() { return saved; } };
}

test('retained original target receipt/account observation preserves precision and conflicts without new delivery', { timeout: 20_000 }, async () => {
  const state = await setup(); const remote = await connection(state);
  try {
    const { view } = await built(state);
    const retained = await retainTarget(state, state.options.config.connections[0]!, view.id); remote.retain(retained);
    const refresh = () => state.service.app.inject({ method: 'POST', url: `${api}/observations/${retained.observationId}/refresh`, headers: state.headers });
    const response = await refresh(); assert.equal(response.statusCode, 200, response.body);
    const observed = response.json();
    assert.equal(observed.status, 'OBSERVING', observed.error ?? ''); assert.equal(observed.receipt.externalId, 'original-external-identity');
    assert.equal(observed.equity, '1234.123456789012345678'); assert.equal(observed.cash, null);
    assert.equal(observed.updatedAt, '2027-01-15T08:00:00.123456789Z');
    const displayed = (await state.service.app.inject({ url: `${api}/observations`, headers: state.headers })).json().find((item: { id: string }) => item.id === observed.id);
    assert.equal(displayed.connectionId, 'protocol-paper'); assert.equal(displayed.connectionName, 'Protocol only');
    assert.deepEqual(displayed.provenance, { clientId: 'client-original', accountId: 'account-original', sequence: '9007199254740995', nativeClockNs: '1800000000123456789' });
    assert.equal('url' in displayed, false); assert.equal('tokenEnv' in displayed, false);
    await refresh(); assert.equal((await state.service.work.getObservation(observed.id)).view.curve.length, 1);
    remote.setExternalId('conflicting-external-identity');
    const conflict = (await refresh()).json();
    assert.equal(conflict.error, 'downstream_receipt_external_id_conflict'); assert.equal(conflict.receipt.externalId, 'original-external-identity');
    const stopped = await state.service.app.inject({ method: 'POST', url: `${api}/observations/${observed.id}/stop`, headers: state.headers });
    assert.equal(stopped.json().status, 'STOPPED'); assert.equal(stopped.json().receipt.status, 'ACKNOWLEDGED');
    assert.equal(remote.posts, 0);
  } finally { await state.cleanup(); await remote.downstream.close(); }
});

test('retained uncertain delivery reconciles after restart without POST and preserves stop', { timeout: 20_000 }, async () => {
  const state = await setup(); const remote = await connection(state);
  try {
    const { view } = await built(state);
    const retained = await retainTarget(state, state.options.config.connections[0]!, view.id); remote.retain(retained);
    const before = await state.service.work.getObservation(retained.observationId);
    assert.equal(before.view.receipt!.status, 'UNCERTAIN'); assert.equal(before.view.equity, null);
    const denied = await state.service.app.inject({ method: 'POST', url: `${api}/observations`, headers: state.headers, payload: { requestId: 'second-unknown', portfolioId: view.id, mode: 'paper', connectionId: 'protocol-paper' } });
    assert.equal(denied.statusCode, 410); assert.equal(denied.json().error, 'native_strategy_only');
    await state.service.app.inject({ method: 'POST', url: `${api}/observations/${retained.observationId}/stop`, headers: state.headers });
    await state.restart();
    const response = await state.service.app.inject({ method: 'POST', url: `${api}/observations/${retained.observationId}/refresh`, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body);
    const after = response.json();
    assert.equal(after.receipt.id, before.view.receipt!.id); assert.equal(after.receipt.status, 'ACKNOWLEDGED');
    assert.equal(after.status, 'STOPPED'); assert.equal(after.equity, null); assert.equal(remote.posts, 0);
    assert.equal((await state.service.app.inject({ method: 'PATCH', url: `${api}/extensions/qz.science`, headers: state.headers, payload: { enabled: false } })).statusCode, 409);
  } finally { await state.cleanup(); await remote.downstream.close(); }
});

test('retired portfolio creation cannot invent a curve or target for missing evidence', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    state.faux.setResponses([fauxAssistantMessage('No evaluation was performed.')]);
    const start = await state.service.app.inject({ method: 'POST', url: `${api}/research`, headers: state.headers, payload: { requestId: 'no-evidence', goal: 'Missing evidence', model: state.model, datasetId: 'controlled' } });
    await (await state.service.research.harness.submission(start.json().submissionId, context))!.wait(context);
    const absent = await state.service.app.inject({ method: 'POST', url: `${api}/portfolios`, headers: state.headers, payload: { requestId: 'absent', name: 'Absent', researchIds: [start.json().conversationId], datasetId: 'controlled', capital: '1' } });
    assert.equal(absent.statusCode, 404); assert.equal(absent.json().error, 'not_found');
    assert.deepEqual((await state.service.research.harness.snapshot(Application, context))!.portfolios, {});
    assert.equal((await state.service.app.inject({ method: 'PATCH', url: `${api}/settings`, headers: state.headers, payload: { nativeTimeoutSeconds: 0 } })).statusCode, 400);
  } finally { await state.cleanup(); }
});
