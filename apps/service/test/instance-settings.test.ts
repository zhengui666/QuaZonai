import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdir, readFile, rename, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import type { InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import { scienceTarget } from '../src/config.ts';
import { setup, studied } from './support.ts';

const route = '/api/v3/settings/instances';
const target = (id: string) => ({ id, url: `https://${id}.example/nautilus`, tokenEnv: 'QZ_TEST_REMOTE_TOKEN' });

test('instance settings persist, clear explicitly and bind only new research admissions', { timeout: 20_000 }, async () => {
  const state = await setup();
  const read = () => state.service.app.inject({ method: 'GET', url: route, headers: state.headers });
  const save = (expectedRevision: number, backtest: ReturnType<typeof target> | null) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload: { expectedRevision, backtest, connections: [] } });
  try {
    assert.equal((await state.service.app.inject(route)).statusCode, 401);
    assert.equal((await state.service.app.inject({ method: 'PUT', url: route, payload: { expectedRevision: 0, backtest: null, connections: [] } })).statusCode, 401);
    assert.deepEqual((await read()).json(), { revision: 0, backtest: null, connections: [] });
    state.service.config.datasets[0]!.data.sealed = { catalog: '/controlled-heldout' };
    assert.equal((await save(0, target('original'))).statusCode, 200);
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

test('concurrent writes, invalid input and failed storage leave the last committed instances intact', { timeout: 20_000 }, async () => {
  const state = await setup();
  const request = (payload: object) => state.service.app.inject({ method: 'PUT', url: route, headers: state.headers, payload });
  const file = join(state.directory, 'instances.json'); const retainedFile = `${file}.retained`;
  try {
    for (const backtest of [{ ...target('invalid'), token: 'literal-token-must-not-be-accepted' }, { ...target('invalid'), url: 'https://name:credential@example.com' }, { ...target('invalid'), tokenEnv: 'literal credential' }]) {
      const invalid = await request({ expectedRevision: 0, backtest, connections: [] });
      assert.equal(invalid.statusCode, 400, invalid.body);
    }
    const results = await Promise.all(['first', 'second'].map(id => request({ expectedRevision: 0, backtest: target(id), connections: [] })));
    assert.deepEqual(results.map(result => result.statusCode).sort(), [200, 409]);
    const original = JSON.parse(await readFile(file, 'utf8')) as InstanceSettingsView;
    assert.equal(original.revision, 1);
    assert.deepEqual(state.service.config.nautilus?.backtest, original.backtest);
    await rename(file, retainedFile); await mkdir(file);
    const failed = await request({ expectedRevision: 1, backtest: target('failed'), connections: [] });
    assert.equal(failed.statusCode, 503, failed.body); assert.equal(failed.json().error, 'instance_settings_save_failed');
    assert.deepEqual(state.service.config.nautilus?.backtest, original.backtest);
    assert.deepEqual((await state.service.app.inject({ method: 'GET', url: route, headers: state.headers })).json(), original);
    await rm(file, { recursive: true }); await rename(retainedFile, file);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ method: 'GET', url: route, headers: state.headers })).json(), original);
    const recovered = await request({ expectedRevision: 1, backtest: target('recovered'), connections: [] });
    assert.equal(recovered.statusCode, 200); assert.equal(recovered.json().revision, 2);
  } finally { await state.cleanup(); }
});
