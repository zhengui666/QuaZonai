import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { test } from 'node:test';
import { mkdir, readFile, rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import type { InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import type { RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { remoteNativeCapabilities, setRemoteNativeCredential } from '../../research/src/native-execution.ts';
import { scienceTarget } from '../src/config.ts';
import { downstreamRequest } from '../src/downstream.ts';
import { setup, studied } from './support.ts';

const route = '/api/v3/settings/instances';
const target = (id: string) => ({ id, url: `https://${id}.example/nautilus`, tokenEnv: 'QZ_TEST_REMOTE_TOKEN' });
const viewTarget = (id: string) => ({ id, url: `https://${id}.example/nautilus` });

test('instance settings persist, clear explicitly and bind only new research admissions', { timeout: 20_000 }, async () => {
  const state = await setup();
  const read = () => state.service.app.inject({ method: 'GET', url: route, headers: state.headers });
  const save = (expectedRevision: number, backtest: ReturnType<typeof target> | null) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload: { expectedRevision, backtest, connections: [] } });
  try {
    assert.equal((await state.service.app.inject(route)).statusCode, 401);
    assert.equal((await state.service.app.inject({ method: 'PUT', url: route, payload: { expectedRevision: 0, backtest: null, connections: [] } })).statusCode, 401);
    assert.deepEqual((await read()).json(), { revision: 0, backtest: null, connections: [] });
    state.service.config.datasets[0]!.data.sealed = { catalog: '/controlled-heldout' };
    const originalSave = await save(0, target('original')); assert.equal(originalSave.statusCode, 200);
    assert.deepEqual(originalSave.json().backtest, viewTarget('original'));
    assert.deepEqual(state.service.config.datasets[0]!.data.nautilusTarget, target('original'));
    assert.deepEqual((state.service.config.datasets[0]!.data.sealed as Record<string, unknown>).nautilusTarget, target('original'));
    const original = await studied(state, 'instance-original');
    assert.equal((await save(1, target('replacement'))).statusCode, 200);
    const replacement = await studied(state, 'instance-replacement');
    assert.deepEqual((await state.service.research.state(original.receipt.conversationId)).research!.input!.data.nautilusTarget, target('original'));
    assert.deepEqual((await state.service.research.state(replacement.receipt.conversationId)).research!.input!.data.nautilusTarget, target('replacement'));
    assert.deepEqual(scienceTarget(state.service.config), target('replacement'));
    const retained = (await read()).json<InstanceSettingsView>();
    assert.equal((await stat(join(state.directory, 'instances.json'))).mode & 0o777, 0o600);
    await state.restart();
    assert.deepEqual((await read()).json(), retained, 'saved settings override the original service file after restart');
    assert.deepEqual(state.service.config.datasets[0]!.data.nautilusTarget, target('replacement'));
    // A lost response may be retried using the original expected revision.
    const replay = await save(1, target('replacement')); assert.equal(replay.statusCode, 200); assert.equal(replay.json().revision, 2);
    assert.equal((await save(2, null)).statusCode, 200);
    assert.equal(state.service.config.nautilus?.backtest, undefined);
    assert.equal(state.service.config.datasets[0]!.data.nautilusTarget, undefined);
    await state.restart();
    assert.deepEqual((await read()).json(), { revision: 3, backtest: null, connections: [] });
    assert.equal(state.service.config.datasets[0]!.data.nautilusTarget, undefined);
    assert.deepEqual((await state.service.research.state(original.receipt.conversationId)).research!.input!.data.nautilusTarget, target('original'));
  } finally { await state.cleanup(); }
});

test('direct credentials retain versioned bindings across rotation and restart', { timeout: 20_000 }, async () => {
  const authorizations: Array<string | undefined> = [];
  const paths: Array<string | undefined> = [];
  const worker = createServer((request, response) => {
    authorizations.push(request.headers.authorization);
    paths.push(request.url);
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify(request.url?.endsWith('/v1/capabilities')
      ? { schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [] }
      : { ok: true }));
  });
  worker.listen(0, '127.0.0.1'); await once(worker, 'listening');
  const url = `http://127.0.0.1:${(worker.address() as AddressInfo).port}`;
  const state = await setup();
  const backtest = { id: 'direct-backtest', url, token: 'TEST_ONLY-backtest-secret' };
  const connection = { id: 'direct-paper', name: 'Direct paper', mode: 'paper' as const, url, clientId: 'qz-test', accountId: 'test-account', token: 'TEST_ONLY-account-secret' };
  const read = () => state.service.app.inject({ method: 'GET', url: route, headers: state.headers });
  const save = (expectedRevision: number, nextBacktest = backtest, nextConnection = connection) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers,
    payload: { expectedRevision, backtest: nextBacktest, connections: [nextConnection] } });
  let references: string[] = [];
  try {
    const saved = await save(0); assert.equal(saved.statusCode, 200, saved.body);
    const view = { revision: 1, backtest, connections: [connection] };
    assert.deepEqual(saved.json(), view);
    const file = JSON.parse(await readFile(join(state.directory, 'instances.json'), 'utf8'));
    references = [file.credentialState.backtestRef, ...Object.values(file.credentialState.connectionRefs)];
    assert.deepEqual({ revision: file.revision, backtest: file.backtest, connections: file.connections }, view);
    assert.deepEqual(Object.keys(file).sort(), ['backtest', 'connections', 'credentialState', 'revision']);
    assert.equal(file.backtest.token, backtest.token);
    assert.equal(file.connections[0].token, connection.token);
    const firstTarget = structuredClone(state.service.config.datasets[0]!.data.nautilusTarget) as RemoteNativeTarget;
    const firstConnection = structuredClone(state.service.config.connections[0]!);
    assert.equal(firstTarget.id, backtest.id); assert.equal(firstTarget.url, url);
    assert.ok(firstTarget.credentialRef); assert.equal('token' in firstTarget, false);
    assert.equal('token' in firstConnection, false); assert.ok(firstConnection.credentialRef);

    const { receipt } = await studied(state, 'direct-token-input');
    const research = await state.service.research.state(receipt.conversationId);
    assert.deepEqual(research.research!.input!.data.nautilusTarget, firstTarget);
    assert.equal(JSON.stringify(research.research!.input).includes(backtest.token), false);
    assert.equal(JSON.stringify(research.research!.input).includes(connection.token), false);

    await remoteNativeCapabilities(firstTarget);
    await downstreamRequest(firstConnection, 'account', 'GET');
    assert.deepEqual(authorizations, [`Bearer ${backtest.token}`, `Bearer ${connection.token}`]);
    assert.deepEqual(paths, ['/v1/capabilities', '/account']);

    const rotatedBacktest = { ...backtest, url: `${url}/rotated`, token: 'TEST_ONLY-backtest-secret-rotated' };
    const rotatedConnection = { ...connection, url: `${url}/rotated`, token: 'TEST_ONLY-account-secret-rotated' };
    const rotated = await save(1, rotatedBacktest, rotatedConnection); assert.equal(rotated.statusCode, 200, rotated.body);
    const latest = rotated.json();
    assert.deepEqual(latest, { revision: 2, backtest: rotatedBacktest, connections: [rotatedConnection] });
    const secondTarget = structuredClone(state.service.config.datasets[0]!.data.nautilusTarget) as RemoteNativeTarget;
    const secondConnection = structuredClone(state.service.config.connections[0]!);
    assert.notEqual(secondTarget.credentialRef, firstTarget.credentialRef);
    assert.notEqual(secondConnection.credentialRef, firstConnection.credentialRef);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input!.data.nautilusTarget, firstTarget);
    const rotatedFile = JSON.parse(await readFile(join(state.directory, 'instances.json'), 'utf8'));
    assert.equal(rotatedFile.credentialState.history[firstTarget.credentialRef!], backtest.token);
    assert.equal(rotatedFile.credentialState.history[firstConnection.credentialRef!], connection.token);
    assert.equal(Object.hasOwn(rotatedFile.credentialState.history, firstConnection.id), false);
    references = [...references, ...Object.keys(rotatedFile.credentialState.history), rotatedFile.credentialState.backtestRef, ...Object.values(rotatedFile.credentialState.connectionRefs)];
    await remoteNativeCapabilities(secondTarget);
    await downstreamRequest(secondConnection, 'account', 'GET');
    await remoteNativeCapabilities(firstTarget);
    await downstreamRequest(firstConnection, 'account', 'GET');
    assert.deepEqual(authorizations.slice(2), [
      `Bearer ${rotatedBacktest.token}`, `Bearer ${rotatedConnection.token}`, `Bearer ${backtest.token}`, `Bearer ${connection.token}`,
    ]);
    assert.deepEqual(paths.slice(2), ['/rotated/v1/capabilities', '/rotated/account', '/v1/capabilities', '/account']);

    for (const reference of references) setRemoteNativeCredential(reference);
    await state.restart();
    assert.deepEqual((await read()).json(), latest);
    await remoteNativeCapabilities(firstTarget);
    await downstreamRequest(firstConnection, 'account', 'GET');
    await remoteNativeCapabilities(secondTarget);
    await downstreamRequest(secondConnection, 'account', 'GET');
    assert.deepEqual(authorizations.slice(6), [
      `Bearer ${backtest.token}`, `Bearer ${connection.token}`, `Bearer ${rotatedBacktest.token}`, `Bearer ${rotatedConnection.token}`,
    ]);
    assert.deepEqual(paths.slice(6), ['/v1/capabilities', '/account', '/rotated/v1/capabilities', '/rotated/account']);
  } finally {
    for (const reference of references) setRemoteNativeCredential(reference);
    await state.cleanup(); worker.closeAllConnections(); await new Promise<void>(resolve => worker.close(() => resolve()));
  }
});

test('concurrent writes, invalid input and failed storage leave the last committed instances intact', { timeout: 20_000 }, async () => {
  const state = await setup();
  const request = (payload: object) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload });
  const file = join(state.directory, 'instances.json'); const retainedFile = `${file}.retained`;
  try {
    for (const backtest of [{ ...target('invalid'), extra: 'unexpected' }, { ...target('invalid'), url: 'https://name:credential@example.com' }, { ...target('invalid'), tokenEnv: 'literal credential' }, { ...target('invalid'), token: 4 }]) {
      const invalid = await request({ expectedRevision: 0, backtest, connections: [] });
      assert.equal(invalid.statusCode, 400, invalid.body);
    }
    const results = await Promise.all(['first', 'second'].map(id => request({ expectedRevision: 0, backtest: target(id), connections: [] })));
    assert.deepEqual(results.map(result => result.statusCode).sort(), [200, 409]);
    const persisted = JSON.parse(await readFile(file, 'utf8')) as InstanceSettingsView & { credentialState: object };
    const original: InstanceSettingsView = { revision: persisted.revision, backtest: persisted.backtest, connections: persisted.connections };
    assert.equal(original.revision, 1);
    assert.ok(persisted.credentialState);
    assert.deepEqual(state.service.config.nautilus?.backtest, target(String(original.backtest?.id)));
    await rename(file, retainedFile); await mkdir(file);
    const failed = await request({ expectedRevision: 1, backtest: target('failed'), connections: [] });
    assert.equal(failed.statusCode, 503, failed.body); assert.equal(failed.json().error, 'instance_settings_save_failed');
    assert.deepEqual(state.service.config.nautilus?.backtest, target(String(original.backtest?.id)));
    assert.deepEqual((await state.service.app.inject({ method: 'GET', url: route, headers: state.headers })).json(), original);
    await rm(file, { recursive: true }); await rename(retainedFile, file);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ method: 'GET', url: route, headers: state.headers })).json(), original);
    const recovered = await request({ expectedRevision: 1, backtest: target('recovered'), connections: [] });
    assert.equal(recovered.statusCode, 200); assert.equal(recovered.json().revision, 2);
  } finally { await state.cleanup(); }
});
