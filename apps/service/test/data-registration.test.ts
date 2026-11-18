import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile, mkdir, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { openResearch } from '../../research/src/host.ts';
import { configDefaults, type DatasetConfig } from '../src/config.ts';
import { dataSources, DataSourceState } from '../src/data-sources.ts';
const context = BACKGROUND_CONTEXT;
const dataset = (id: string): DatasetConfig => ({ id, name: id, assetClass: 'CRYPTO', currency: 'USD', source: 'local', start: null, end: null,
  ready: true, data: { catalog: './catalog' } });
async function open(directory: string) {
  const config = configDefaults({ dataDir: directory, datasets: [dataset('base')] });
  const sources = await dataSources(config);
  const host = await openResearch({ database: join(directory, 'sources.sqlite'), models: createModels(), extensions: [sources.extension], deferResume: true });
  await sources.connect(host);
  return { sources, host, config, async close() { await sources.close(); await host.close(); } };
}
test('append registration preserves sources, replays without rereading and survives SQLite reopen', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-registration-'));
  let state = await open(directory);
  try {
    const before = structuredClone(state.config.datasets);
    const initial = await state.host.harness.snapshot(DataSourceState, context);
    const local = structuredClone(initial!.sources['qz.data.local-config']);
    await writeFile(join(directory, 'new.json'), JSON.stringify(dataset('new')));
    const request = { requestId: 'one', expectedRevision: 1, path: 'new.json' };
    const receipt = await state.sources.registerDatasets(request);
    assert.deepEqual(receipt, { requestId: 'one', revision: 2, datasetIds: ['new'] });
    assert.deepEqual(state.config.datasets.slice(0, 1), before);
    assert.equal(state.config.datasets[1]!.data.catalog, join(directory, 'catalog'));
    assert.deepEqual((await state.host.harness.snapshot(DataSourceState, context))!.sources['qz.data.local-config'], local);
    await writeFile(join(directory, 'new.json'), '{broken');
    assert.deepEqual(await state.sources.registerDatasets(request), receipt);
    await assert.rejects(state.sources.registerDatasets({ ...request, path: 'different.json' }), /request_id_conflict/);
    const tasks = await state.host.harness.commit(tx => tx.scanTasks({ kind: 'qz.data-source.refresh' }, 100), context);
    assert.equal(tasks.items.length, 0);
    await state.close(); state = await open(directory);
    assert.deepEqual(state.config.datasets.map(d => d.id), ['base', 'new']);
    assert.deepEqual(await state.sources.registerDatasets(request), receipt);
    await writeFile(join(directory, 'same.json'), JSON.stringify(dataset('new')));
    await assert.rejects(state.sources.registerDatasets({ requestId: 'two', expectedRevision: 2, path: 'same.json' }), /data_source_dataset_id_conflict/);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});
test('registration CAS publishes one concurrent append and rejects retained/cross-source identities', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-registration-race-'));
  const state = await open(directory);
  try {
    for (const id of ['left', 'right', 'base', 'retained']) await writeFile(join(directory, `${id}.json`), JSON.stringify(dataset(id)));
    const results = await Promise.allSettled(['left', 'right'].map(id => state.sources.registerDatasets({ requestId: id, expectedRevision: 1, path: `${id}.json` })));
    assert.equal(results.filter(r => r.status === 'fulfilled').length, 1);
    const rejected = results.find(r => r.status === 'rejected') as PromiseRejectedResult;
    assert.match(String(rejected.reason), /data_source_revision_conflict/);
    await assert.rejects(state.sources.registerDatasets({ requestId: 'base', expectedRevision: 2, path: 'base.json' }), /data_source_dataset_id_conflict/);
    await state.host.harness.commit(async tx => {
      (await tx.doc(DataSourceState)).sources['qz.data.local-config'] = { enabled: false, config: {}, datasets: [], retainedDatasets: [dataset('retained')], lastLoadedAt: null, error: null } as never;
    }, context);
    await assert.rejects(state.sources.registerDatasets({ requestId: 'retained', expectedRevision: 2, path: 'retained.json' }), /data_source_dataset_id_conflict/);
    assert.equal(state.config.datasets.length, 2);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});

test('registration HTTP route validates input and exposes the current CAS revision without execution', async () => {
  const { setup } = await import('./support.ts');
  const state = await setup();
  const url = '/api/v3/datasets/register';
  try {
    const path = join(state.directory, 'registered.json');
    await writeFile(path, JSON.stringify(dataset('http-new')));
    const payload = { requestId: 'http-one', expectedRevision: 1, path };
    assert.equal((await state.service.app.inject({ method: 'POST', url, payload })).statusCode, 401);
    for (const invalid of [{ ...payload, expectedRevision: 0 }, { ...payload, unexpected: true }, { ...payload, requestId: ' ' }]) {
      assert.equal((await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: invalid })).statusCode, 422);
    }
    const added = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload });
    assert.equal(added.statusCode, 200, added.body);
    assert.deepEqual(added.json().datasetIds, ['http-new']);
    const registry = (await state.service.app.inject({ url: '/api/v3/datasets/registry', headers: state.headers })).json();
    assert.equal(registry.revision, 2); assert.ok(registry.datasetIds.includes('http-new'));
    await writeFile(path, '[]');
    const replay = await state.service.app.inject({ method: 'POST', url: '/api/v3/data-sources/qz.data.registered/datasets', headers: state.headers, payload });
    assert.deepEqual(replay.json(), added.json());
    const empty = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload: { ...payload, requestId: 'empty', expectedRevision: 2 } });
    assert.equal(empty.statusCode, 422);
  } finally { await state.cleanup(); }
});

test('research descriptor registration respects real workspace and replays original receipt without file reread',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-registration-scope-'));const state=await open(directory);
 try{
  const workspace=join(directory,'source');await mkdir(workspace);
  const outside=join(directory,'outside.json');await writeFile(outside,JSON.stringify(dataset('outside')));
  await symlink(outside,join(workspace,'escape.json'));
  for(const path of [outside,join(workspace,'../outside.json'),join(workspace,'escape.json')])await assert.rejects(state.sources.registerDatasets({requestId:'outside',expectedRevision:1,path},workspace),/research_descriptor_outside_workspace/);
  const path=join(workspace,'prepared.json');await writeFile(path,JSON.stringify(dataset('prepared')));
  const request={requestId:'prepared',expectedRevision:1,path};const receipt=await state.sources.registerDatasets(request,workspace);
  await rm(path);assert.deepEqual(await state.sources.registerDatasets(request,workspace),receipt);
  await assert.rejects(state.sources.registerDatasets({requestId:'stale',expectedRevision:1,path},workspace),/data_source_revision_conflict/);
  assert.deepEqual(receipt.datasetIds,['prepared']);
 }finally{await state.close();await rm(directory,{recursive:true,force:true});}
});

test('mixed native catalog metadata is preserved without a fabricated BAR research modality', async () => {
  const { setup } = await import('./support.ts'); const state = await setup();
  try {
    const path = join(state.directory, 'mixed.json');
    await writeFile(path, JSON.stringify({ ...dataset('mixed'), inputCoverage: { native_data_types: ['OrderBookDelta', 'TradeTick'], coverage_kind: 'DISCONTINUOUS_EXPOSED_TRAINING_WINDOWS' } }));
    const added = await state.service.app.inject({ method: 'POST', url: '/api/v3/datasets/register', headers: state.headers, payload: { requestId: 'mixed-types', expectedRevision: 1, path } });
    assert.equal(added.statusCode, 200, added.body);
    for (const url of ['/api/v3/datasets/mixed', '/api/v3/datasets?view=summary']) {
      const response = (await state.service.app.inject({ url, headers: state.headers })).json();
      const row = Array.isArray(response) ? response.find(value => value.id === 'mixed') : response;
      assert.deepEqual(row.nativeDataTypes, ['OrderBookDelta', 'TradeTick']);
      assert.equal(row.nativeModality, undefined); assert.equal(row.researchSettings, undefined);
      assert.equal(row.start, null); assert.equal(row.end, null);
    }
  } finally { await state.cleanup(); }
});
