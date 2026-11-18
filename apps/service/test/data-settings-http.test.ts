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

test('dataset selection persists without deleting historical datasets or rebinding research', async () => {
  const state = await setup();
  const request = (method: 'GET' | 'PUT' | 'POST', url: string, payload?: object) => state.service.app.inject({ method, url, headers: state.headers, ...(payload === undefined ? {} : { payload }) });
  try {
    assert.equal((await state.service.app.inject({ method: 'PUT', url: '/api/v3/datasets/selection', payload: { datasetIds: [] } })).statusCode, 401);
    const { receipt } = await studied(state);
    const originalInput = structuredClone((await state.service.research.state(receipt.conversationId)).research!.input);
    const originalDataset = (await request('GET', '/api/v3/datasets/controlled')).json();
    const registry = (await request('GET', '/api/v3/datasets/registry')).json();
    assert.equal(registry.selectedDatasetIds, null);
    await writeFile(join(state.directory, 'selection-dataset.json'), JSON.stringify({ ...state.options.config.datasets[0], id: 'selected-native', name: 'Selected native catalog' }));
    const registered = await request('POST', '/api/v3/datasets/register', { requestId: 'selection-registration', expectedRevision: registry.revision, path: 'selection-dataset.json' });
    assert.equal(registered.statusCode, 200, registered.body);

    const selected = await request('PUT', '/api/v3/datasets/selection', { datasetIds: ['selected-native'] });
    assert.equal(selected.statusCode, 200, selected.body);
    const expectedIds = ['controlled', 'selected-native'];
    const assertSelection = async () => {
      const current = (await request('GET', '/api/v3/datasets/registry')).json();
      assert.deepEqual(current.availableDatasetIds.slice().sort(), expectedIds);
      assert.deepEqual(current.selectedDatasetIds, ['selected-native']);
      assert.deepEqual(current.datasetIds, ['selected-native']);
      for (const url of ['/api/v3/datasets', '/api/v3/datasets?view=summary']) {
        assert.deepEqual((await request('GET', url)).json().map((row: { id: string }) => row.id), ['selected-native']);
      }
      const historical = await request('GET', '/api/v3/datasets/controlled');
      assert.equal(historical.statusCode, 200, historical.body);
      assert.deepEqual(historical.json(), originalDataset);
      assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input, originalInput);
    };
    await assertSelection();
    const savedDoc = structuredClone(await state.service.research.harness.snapshot(DataSourceState, context));
    const savedConfig = structuredClone({ datasets: state.options.config.datasets, retainedDatasets: state.options.config.retainedDatasets });
    const unknown = await request('PUT', '/api/v3/datasets/selection', { datasetIds: ['missing-native'] });
    assert.equal(unknown.statusCode, 422, unknown.body);
    assert.match(unknown.body, /dataset_selection_unknown_id/);
    assert.deepEqual(await state.service.research.harness.snapshot(DataSourceState, context), savedDoc);
    assert.deepEqual({ datasets: state.options.config.datasets, retainedDatasets: state.options.config.retainedDatasets }, savedConfig);
    await assertSelection();
    await state.restart();
    await assertSelection();

    const reset = await request('PUT', '/api/v3/datasets/selection', { datasetIds: null });
    assert.equal(reset.statusCode, 200, reset.body);
    await state.restart();
    const restored = (await request('GET', '/api/v3/datasets/registry')).json();
    assert.equal(restored.selectedDatasetIds, null);
    assert.deepEqual(restored.availableDatasetIds.slice().sort(), expectedIds);
    assert.deepEqual(restored.datasetIds.slice().sort(), expectedIds);
    assert.deepEqual((await request('GET', '/api/v3/datasets')).json().map((row: { id: string }) => row.id).sort(), expectedIds);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research!.input, originalInput);
  } finally { await state.cleanup(); }
});
