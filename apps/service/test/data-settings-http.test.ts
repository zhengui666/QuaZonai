import assert from 'node:assert/strict';
import { test } from 'node:test';
import { writeFile, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DataSourceState } from '../src/data-sources.ts';
import { setup, studied, context } from './support.ts';

test('owner native registry registration and legacy alias retain research while plugin writes are retired', async () => {
  const state = await setup();
  const request = (method: 'GET' | 'PUT' | 'POST' | 'PATCH', url: string, payload?: object) => state.service.app.inject({ method, url, headers: state.headers, ...(payload === undefined ? {} : { payload }) });
  try {
    for (const url of ['/api/v3/data-sources', '/api/v3/datasets/registry']) assert.equal((await state.service.app.inject({ url })).statusCode, 401);
    assert.equal((await state.service.app.inject({ method: 'POST', url: '/api/v3/datasets/register', payload: {} })).statusCode, 401);
    const { receipt } = await studied(state);
    const captured = structuredClone((await state.service.research.state(receipt.conversationId)).research!.input);
    const current = (await request('GET', '/api/v3/datasets/registry')).json();
    const descriptor = { ...state.options.config.datasets[0], id: 'imported', name: 'Imported' };
    await writeFile(join(state.directory, 'dataset.json'), JSON.stringify(descriptor));
    const input = { requestId: 'native-catalog-reference', expectedRevision: current.revision, path: 'dataset.json' };
    const registered = await request('POST', '/api/v3/datasets/register', input);
    assert.equal(registered.statusCode, 200, registered.body);
    const alias = await request('POST', '/api/v3/data-sources/qz.data.registered/datasets', input);
    assert.deepEqual(alias.json(), registered.json());
    for (const [method, url] of [['PUT', '/api/v3/data-sources/qz.data.local-config'], ['POST', '/api/v3/data-sources/qz.data.local-config/refresh']] as const) {
      const response = await request(method, url, {}); assert.equal(response.statusCode, 410); assert.match(response.body, /data_source_plugins_retired/);
    }
    await state.restart();
    assert.deepEqual((await request('GET', '/api/v3/datasets/registry')).json().datasetIds, ['controlled', 'imported']);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input, captured);
  } finally { await state.cleanup(); }
});

test('configured retired data modules are never evaluated and historical projections remain readable', async () => {
  const state = await setup();
  try {
    const marker = join(state.directory, 'module-evaluated'), module = join(state.directory, 'custom-data.mjs');
    await writeFile(module, `import {writeFileSync} from 'node:fs';writeFileSync(${JSON.stringify(marker)},'bad');throw Error('must not evaluate');`);
    state.options.config.extensions.push({ name: 'custom.data', kind: 'data', module, enabled: true });
    const original = { enabled: false, config: { path: '/not-read' }, datasets: [], retainedDatasets: [], revision: 2, lastLoadedAt: null, error: null };
    await state.service.research.harness.commit(async tx => { (await tx.doc(DataSourceState)).sources['custom.data'] = original; }, context);
    await state.restart();
    await assert.rejects(readFile(marker), /ENOENT/);
    const sources = (await state.service.app.inject({ url: '/api/v3/data-sources', headers: state.headers })).json();
    assert.equal(sources.find((row: { name: string }) => row.name === 'custom.data').enabled, false);
    assert.deepEqual((await state.service.research.harness.snapshot(DataSourceState, context))!.sources['custom.data'], original);
    assert.equal(state.service.research.registry.snapshot().extension('custom.data'), undefined);
  } finally { await state.cleanup(); }
});
