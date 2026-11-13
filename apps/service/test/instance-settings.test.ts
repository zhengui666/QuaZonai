import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { randomUUID } from 'node:crypto';
import { test } from 'node:test';
import { mkdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import type { RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { remoteNativeCapabilities, remoteNativeCredential, setRemoteNativeCredential } from '../../research/src/native-execution.ts';
import { downstreamRequest } from '../src/downstream.ts';
import { scienceTarget } from '../src/config.ts';
import { setup, studied } from './support.ts';

const route = '/api/v3/settings/instances';
const environment = (id: string) => ({ kind: 'RUST_PROCESS' as const, id, cargoBinary: '/owner/bin/cargo', manifestPath: `/owner/${id}/Cargo.toml`, packageName: 'owner-strategy', backtestFactory: 'owner_strategy::backtest', accountFactory: 'owner_strategy::account' });
const target = { id: 'historical', url: 'https://historical.example/nautilus', tokenEnv: 'QZ_TEST_REMOTE_TOKEN' };

test('Rust environment settings persist and clear without rewriting admitted research or historical transports', { timeout: 20_000 }, async () => {
  const state = await setup();
  const read = () => state.service.app.inject({ method: 'GET', url: route, headers: state.headers });
  const save = (expectedRevision: number, selected: ReturnType<typeof environment> | null) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload: { expectedRevision, environment: selected } });
  try {
    assert.equal((await state.service.app.inject(route)).statusCode, 401);
    assert.equal((await state.service.app.inject({ method: 'PUT', url: route, payload: { expectedRevision: 0, environment: null } })).statusCode, 401);
    assert.deepEqual((await read()).json(), { revision: 0, backtest: null, connections: [] });
    // An existing persisted binding is a historical fact, not a new HTTP admission.
    await writeFile(join(state.directory, 'instances.json'), JSON.stringify({ revision: 0, backtest: target, connections: [] }), { mode: 0o600 });
    await state.restart();
    const original = await studied(state, 'instance-original');
    const originalInput = (await state.service.research.state(original.receipt.conversationId)).research!.input!;
    const originalTarget = structuredClone(originalInput.data.nautilusTarget);
    const saved = await save(0, environment('original')); assert.equal(saved.statusCode, 200, saved.body);
    assert.deepEqual(saved.json().environment, environment('original'));
    assert.equal((await save(1, environment('replacement'))).statusCode, 200);
    assert.deepEqual((await state.service.research.state(original.receipt.conversationId)).research!.input!.data.nautilusTarget, originalTarget);
    assert.deepEqual(scienceTarget(state.service.config), target, 'saving a launcher must not relabel the historical HTTP binding');
    const retained = (await read()).json<InstanceSettingsView>();
    assert.equal((await stat(join(state.directory, 'instances.json'))).mode & 0o777, 0o600);
    await state.restart(); assert.deepEqual((await read()).json(), retained);
    const replay = await save(1, environment('replacement')); assert.equal(replay.statusCode, 200); assert.equal(replay.json().revision, 2);
    assert.equal((await save(2, null)).statusCode, 200);
    await state.restart();
    const cleared = (await read()).json<InstanceSettingsView>();
    assert.equal(cleared.revision, 3); assert.equal(cleared.environment, null);
    assert.deepEqual(cleared.backtest, retained.backtest); assert.deepEqual(cleared.connections, retained.connections);
    assert.deepEqual((await state.service.research.state(original.receipt.conversationId)).research!.input!.data.nautilusTarget, originalTarget);
    assert.equal((await state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload: { expectedRevision: 3, backtest: target, connections: [] } })).statusCode, 400, 'new HTTP configuration remains retired');
  } finally { await state.cleanup(); }
});

test('Rust environment changes preserve current and rotated historical credential references across restart', { timeout: 20_000 }, async () => {
  const requests: Array<{ method: string | undefined; path: string | undefined; authorization: string | undefined }> = [];
  const worker = createServer((request, response) => {
    requests.push({ method: request.method, path: request.url, authorization: request.headers.authorization });
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify(request.url?.endsWith('/v1/capabilities')
      ? { schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [] }
      : { ok: true }));
  });
  worker.listen(0, '127.0.0.1'); await once(worker, 'listening');
  const url = `http://127.0.0.1:${(worker.address() as AddressInfo).port}`;
  const state = await setup();
  const refs = { backtest: randomUUID(), connection: randomUUID(), oldBacktest: randomUUID(), oldConnection: randomUUID() };
  const backtest = { id: 'historical-backtest', url: `${url}/rotated`, token: 'TEST_ONLY-current-backtest' };
  const connection = { id: 'historical-paper', name: 'Historical paper', mode: 'paper', url: `${url}/rotated`, clientId: 'qz-test', accountId: 'test-account', token: 'TEST_ONLY-current-account' };
  const credentialState = { backtestRef: refs.backtest, connectionRefs: { [connection.id]: refs.connection }, history: { [refs.oldBacktest]: 'TEST_ONLY-old-backtest', [refs.oldConnection]: 'TEST_ONLY-old-account' }, connectionTokenEnvs: {} };
  const filePath = join(state.directory, 'instances.json');
  const save = (expectedRevision: number, selected: ReturnType<typeof environment> | null) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload: { expectedRevision, environment: selected } });
  try {
    // Reconstruct the original persisted credential history; never save a new HTTP endpoint through the API.
    await writeFile(filePath, JSON.stringify({ revision: 7, backtest, connections: [connection], credentialState }), { mode: 0o600 });
    await state.restart();
    const firstTarget = structuredClone(state.service.config.datasets[0]!.data.nautilusTarget) as RemoteNativeTarget;
    assert.equal(firstTarget.credentialRef, refs.backtest); assert.equal('token' in firstTarget, false);
    assert.equal(state.service.config.connections[0]!.credentialRef, refs.connection);
    assert.equal('token' in state.service.config.connections[0]!, false);
    const firstConnection = structuredClone(state.service.config.connections[0]!);
    const oldTarget: RemoteNativeTarget = { id: backtest.id, url, credentialRef: refs.oldBacktest };
    const oldConnection = { ...firstConnection, url, credentialRef: refs.oldConnection };
    const queryHistorical = async () => {
      await remoteNativeCapabilities(oldTarget);
      await downstreamRequest(oldConnection, 'account', 'GET');
      await remoteNativeCapabilities(firstTarget);
      await downstreamRequest(firstConnection, 'account', 'GET');
    };
    const expectedReads = [
      { method: 'GET', path: '/v1/capabilities', authorization: 'Bearer TEST_ONLY-old-backtest' },
      { method: 'GET', path: '/account', authorization: 'Bearer TEST_ONLY-old-account' },
      { method: 'GET', path: '/rotated/v1/capabilities', authorization: `Bearer ${backtest.token}` },
      { method: 'GET', path: '/rotated/account', authorization: `Bearer ${connection.token}` },
    ];
    await queryHistorical(); assert.deepEqual(requests, expectedReads);
    const { receipt } = await studied(state, 'historical-token-input');
    const input = (await state.service.research.state(receipt.conversationId)).research!.input!;
    assert.deepEqual(input.data.nautilusTarget, firstTarget);
    for (const token of [backtest.token, connection.token, ...Object.values(credentialState.history)]) assert.equal(JSON.stringify(input).includes(token), false);
    for (const [revision, selected] of [[7, environment('first')], [8, environment('second')], [9, null]] as const) {
      const saved = await save(revision, selected); assert.equal(saved.statusCode, 200, saved.body);
      const file = JSON.parse(await readFile(filePath, 'utf8'));
      assert.deepEqual(file.backtest, backtest); assert.deepEqual(file.connections, [connection]);
      assert.deepEqual(file.credentialState, credentialState, 'environment replacement/clear cannot rotate, drop or reassign original credentials');
      assert.equal((await stat(filePath)).mode & 0o777, 0o600);
    }
    for (const reference of Object.values(refs)) setRemoteNativeCredential(reference);
    await state.restart();
    assert.equal(remoteNativeCredential(refs.backtest), backtest.token);
    assert.equal(remoteNativeCredential(refs.connection), connection.token);
    assert.equal(remoteNativeCredential(refs.oldBacktest), credentialState.history[refs.oldBacktest]);
    assert.equal(remoteNativeCredential(refs.oldConnection), credentialState.history[refs.oldConnection]);
    await queryHistorical(); assert.deepEqual(requests, [...expectedReads, ...expectedReads], 'restart retains each historical URL and Bearer; no POST is issued');
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input!.data.nautilusTarget, firstTarget);
    const retained = (await state.service.app.inject({ method: 'GET', url: route, headers: state.headers })).json();
    assert.equal(retained.revision, 10); assert.equal(retained.environment, null);
    assert.deepEqual(retained.backtest, backtest); assert.deepEqual(retained.connections, [connection]);
  } finally {
    for (const reference of Object.values(refs)) setRemoteNativeCredential(reference);
    await state.cleanup(); worker.closeAllConnections(); await new Promise<void>(resolve => worker.close(() => resolve()));
  }
});

test('concurrent Rust environment writes, invalid input and failed storage preserve the committed revision', { timeout: 20_000 }, async () => {
  const state = await setup();
  const request = (payload: object) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload });
  const file = join(state.directory, 'instances.json'); const retainedFile = `${file}.retained`;
  try {
    for (const selected of [{ ...environment('invalid'), extra: true }, { ...environment('invalid'), cargoBinary: 'cargo' }, { ...environment('invalid'), manifestPath: 'relative/Cargo.toml' }, { ...environment('invalid'), features: [4] }, { ...environment('invalid'), backtestFactory: 'bad;code' }]) {
      assert.equal((await request({ expectedRevision: 0, environment: selected })).statusCode, 400);
    }
    const results = await Promise.all(['first', 'second'].map(id => request({ expectedRevision: 0, environment: environment(id) })));
    assert.deepEqual(results.map(result => result.statusCode).sort(), [200, 409]);
    const persisted = JSON.parse(await readFile(file, 'utf8'));
    const { credentialState, ...original } = persisted;
    assert.equal(original.revision, 1); assert.ok(credentialState);
    assert.deepEqual(original.environment, environment(original.environment.id));
    await rename(file, retainedFile); await mkdir(file);
    const failed = await request({ expectedRevision: 1, environment: environment('failed') });
    assert.equal(failed.statusCode, 503, failed.body); assert.equal(failed.json().error, 'instance_settings_save_failed');
    assert.deepEqual((await state.service.app.inject({ method: 'GET', url: route, headers: state.headers })).json(), original);
    await rm(file, { recursive: true }); await rename(retainedFile, file);
    await state.restart(); assert.deepEqual((await state.service.app.inject({ method: 'GET', url: route, headers: state.headers })).json(), original);
    assert.equal((await request({ expectedRevision: 0, environment: environment('stale') })).statusCode, 409);
    const recovered = await request({ expectedRevision: 1, environment: environment('recovered') });
    assert.equal(recovered.statusCode, 200); assert.equal(recovered.json().revision, 2);
  } finally { await state.cleanup(); }
});
