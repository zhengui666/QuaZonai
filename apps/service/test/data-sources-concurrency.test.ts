import assert from 'node:assert/strict';
import { test } from 'node:test';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DataSourceState } from '../src/data-sources.ts';
import { setup, context } from './support.ts';

test('concurrent retired provider writes cannot create tasks or change original saved sources', async () => {
  const state = await setup();
  try {
    const original = { enabled: false, config: { path: '/unread/descriptor.json' }, datasets: [], retainedDatasets: [], revision: 7,
      acquisition: { manifestPath: '/retained/original.json', planId: 'original-plan' }, lastLoadedAt: '2026-01-01T00:00:00.000Z', error: null };
    await state.service.research.harness.commit(async tx => { (await tx.doc(DataSourceState)).sources['old.source'] = original; }, context);
    const results = await Promise.all(Array.from({ length: 8 }, (_, index) => state.service.app.inject({ method: index % 2 ? 'POST' : 'PUT',
      url: `/api/v3/data-sources/old.source${index % 2 ? '/refresh' : ''}`, headers: state.headers, ...(index % 2 ? {} : { payload: { enabled: true, config: {} } }) })));
    assert.ok(results.every(result => result.statusCode === 410 && /data_source_plugins_retired/.test(result.body)));
    assert.deepEqual((await state.service.research.harness.snapshot(DataSourceState, context))!.sources['old.source'], original);
    assert.equal((await state.service.research.harness.commit(tx => tx.scanTasks({ kind: 'qz.data-source.refresh' }, 100), context)).items.length, 0);
  } finally { await state.cleanup(); }
});

test('native descriptor registration retains revision CAS under concurrent requests', async () => {
  const state = await setup();
  try {
    const registry = (await state.service.app.inject({ url: '/api/v3/datasets/registry', headers: state.headers })).json();
    for (const id of ['left', 'right']) await writeFile(join(state.directory, `${id}.json`), JSON.stringify({ ...state.options.config.datasets[0], id, name: id }));
    const replies = await Promise.all(['left', 'right'].map(id => state.service.app.inject({ method: 'POST', url: '/api/v3/datasets/register', headers: state.headers,
      payload: { requestId: `register-${id}`, expectedRevision: registry.revision, path: `${id}.json` } })));
    assert.deepEqual(replies.map(reply => reply.statusCode).sort(), [200, 409]);
    const latest = (await state.service.app.inject({ url: '/api/v3/datasets/registry', headers: state.headers })).json();
    assert.equal(latest.revision, registry.revision + 1); assert.equal(latest.datasetIds.length, registry.datasetIds.length + 1);
  } finally { await state.cleanup(); }
});
