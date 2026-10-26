import assert from 'node:assert/strict';
import { test } from 'node:test';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setup, studied } from './support.ts';

test('owner data settings load extensions, survive restart and preserve captured research', async () => {
  const state = await setup();
  const local = 'qz.data.local-config';
  const path = `/api/v3/data-sources/${local}`;
  const request = (method: 'GET' | 'PUT' | 'POST' | 'PATCH', url: string, payload?: object) => state.service.app.inject({ method, url, headers: state.headers, ...(payload === undefined ? {} : { payload }) });
  try {
    assert.equal((await state.service.app.inject('/api/v3/data-sources')).statusCode, 401);
    assert.equal((await state.service.app.inject({ method: 'POST', url: `${path}/refresh` })).statusCode, 401);
    const module = join(state.directory, 'custom-data.mjs');
    await writeFile(module, `export default {name:'custom.data',fields:[],async load(){return []}};`);
    state.options.config.extensions.push({ name: 'custom.data', kind: 'data', module, enabled: true });
    await state.restart();
    const extensions = (await request('GET', '/api/v3/extensions')).json();
    assert.equal(extensions.find((value: { name: string }) => value.name === 'custom.data').kind, 'data');
    assert.equal(state.service.research.registry.snapshot().extension('custom.data'), undefined);
    const { receipt } = await studied(state);
    const before = await state.service.research.state(receipt.conversationId);
    const captured = structuredClone(before.research!.input);
    await writeFile(join(state.directory, 'dataset.json'), JSON.stringify({ ...state.options.config.datasets[0], id: 'imported', name: 'Imported' }));
    const saved = await request('PUT', path, { enabled: true, config: { path: 'dataset.json' } });
    assert.equal(saved.statusCode, 200, saved.body);
    const refreshed = await request('POST', `${path}/refresh`);
    assert.equal(refreshed.statusCode, 200, refreshed.body);
    assert.deepEqual((await request('GET', '/api/v3/datasets')).json().map((value: { id: string }) => value.id), ['controlled', 'imported']);
    const disabled = await request('PATCH', `/api/v3/extensions/${local}`, { enabled: false });
    assert.equal(disabled.statusCode, 200, disabled.body);
    assert.deepEqual({ kind: disabled.json().kind, state: disabled.json().state }, { kind: 'data', state: 'disabled' });
    assert.deepEqual((await request('GET', '/api/v3/datasets')).json().map((value: { id: string }) => value.id), ['controlled']);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input, captured);
    await request('PATCH', `/api/v3/extensions/${local}`, { enabled: true });
    await state.restart();
    assert.deepEqual((await request('GET', '/api/v3/datasets')).json().map((value: { id: string }) => value.id), ['controlled', 'imported']);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input, captured);
  } finally { await state.cleanup(); }
});

test('data providers cannot shadow core or host extension routes', async () => {
  const state = await setup();
  try {
    const host = state.service.research.registry.snapshot().installed().find(extension => extension.name !== 'qz.science')!.name;
    for (const name of ['qz.science', host]) {
      const module = join(state.directory, `${name}.mjs`);
      await writeFile(module, `export default {name:${JSON.stringify(name)},fields:[],async load(){return []}};`);
      state.options.config.extensions = [{ name, module, kind: 'data', enabled: true }];
      await assert.rejects(state.restart(), /extension_name_conflict/);
    }
  } finally { await state.cleanup(); }
});
