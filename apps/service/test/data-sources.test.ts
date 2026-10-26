import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import Fastify from 'fastify';
import { configDefaults, existingDataset } from '../src/config.ts';
import type { DatasetConfig } from '../src/config.ts';
import { dataSources, registerDataSources } from '../src/data-sources.ts';

const local = 'qz.data.local-config';
const dataset = (id: string): DatasetConfig => ({ id, name: id, assetClass: 'CRYPTO', currency: 'USD', source: 'local', start: null, end: null,
  ready: true, data: { catalog: './catalog', sealed: { catalog: './heldout' }, request: {} } });

test('local source registers catalogs, persists and applies enablement to new admissions only', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-data-source-'));
  const file = join(directory, 'export', 'dataset.json');
  const target = { id: 'science', url: 'https://science.example', tokenEnv: 'TEST_TOKEN' };
  const config = configDefaults({ dataDir: directory, datasets: [dataset('base')], nautilus: { backtest: target } });
  let sources = await dataSources(config);
  const app = Fastify(); registerDataSources(app, sources);
  try {
    assert.equal(sources.view().find(source => source.name === local)!.enabled, false);
    assert.equal(sources.extensions().find(source => source.name === 'qz.data.config')!.kind, 'data');
    await sources.toggle(local, true);
    await sources.save(local, { enabled: false, config: { path: '' } });
    assert.equal(sources.view().find(source => source.name === local)!.enabled, false, 'an unconfigured provider can be disabled');
    await mkdir(join(directory, 'export')); await writeFile(file, JSON.stringify(dataset('imported')));
    const saved = await app.inject({ method: 'PUT', url: `/api/v3/data-sources/${local}`, payload: { enabled: true, config: { path: 'export/dataset.json' } } });
    assert.equal(saved.statusCode, 200);
    assert.equal(config.datasets.length, 1, 'saving configuration does not read the source');
    const loaded = await app.inject({ method: 'POST', url: `/api/v3/data-sources/${local}/refresh` });
    assert.equal(loaded.statusCode, 200, loaded.body);
    assert.deepEqual(loaded.json().datasetIds, ['imported']);
    assert.equal(config.datasets[1]!.data.catalog, join(directory, 'export/catalog'));
    assert.deepEqual(config.datasets[1]!.data.nautilusTarget, target);
    const captured = structuredClone(config.datasets[1]);
    await sources.toggle(local, false);
    assert.deepEqual(config.datasets.map(value => value.id), ['base']);
    await assert.rejects(sources.refresh(local), /data_source_disabled/);
    await sources.toggle(local, true);
    assert.equal(config.datasets.length, 2);
    assert.deepEqual(captured, config.datasets[1]);
    await writeFile(file, '{invalid');
    await assert.rejects(sources.refresh(local), /data_source_refresh_failed/);
    assert.deepEqual(config.datasets[1], captured);
    await sources.close();
    const restarted = configDefaults({ dataDir: directory, datasets: [dataset('base')], nautilus: { backtest: target } });
    sources = await dataSources(restarted);
    assert.deepEqual(restarted.datasets[1], captured, 'restart recovers last successful data without loading the broken source');
    assert.equal(sources.view().find(source => source.name === local)!.error, 'data_source_refresh_failed');
    assert.ok(JSON.parse(await readFile(join(directory, 'data-sources.json'), 'utf8'))[local].datasets.length);
  } finally { await app.close(); await sources.close(); await rm(directory, { recursive: true, force: true }); }
});

test('external modules load through data registry and failed or conflicting refresh keeps prior datasets', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-data-module-'));
  const module = join(directory, 'plugin.mjs');
  await writeFile(module, `export default {name:'custom.data',version:'2',fields:[{key:'id',label:'ID',type:'text',required:true},{key:'limit',label:'Limit',type:'number'}],async load(config){if(config.id==='throw')throw new Error('credential-must-not-escape');return [{id:config.id,name:'Custom',assetClass:'CRYPTO',currency:'USD',source:'custom',start:null,end:null,ready:true,data:config.id==='bad-number'?{bad:Infinity}:{catalog:'/custom'}}];}}`);
  const config = configDefaults({ dataDir: directory, datasets: [dataset('base')], extensions: [{ name: 'custom.data', module, kind: 'data', enabled: true, config: { id: 'first' } }] });
  const sources = await dataSources(config);
  try {
    assert.deepEqual(config.datasets.map(value => value.id), ['base'], 'startup does not call load');
    await sources.refresh('custom.data');
    await sources.save('custom.data', { enabled: true, config: { id: 'second' } }); await sources.refresh('custom.data');
    assert.deepEqual(config.datasets.map(value => value.id), ['base', 'second']);
    const cleared = await sources.save('custom.data', { enabled: true, config: { id: 'second', limit: null } });
    assert.deepEqual(cleared.config, { id: 'second' });
    for (const id of ['base', 'throw', 'bad-number']) {
      await sources.save('custom.data', { enabled: true, config: { id } });
      await assert.rejects(sources.refresh('custom.data'), /data_source_refresh_failed/);
      assert.deepEqual(config.datasets.map(value => value.id), ['base', 'second']);
      const persisted = JSON.parse(await readFile(join(directory, 'data-sources.json'), 'utf8'));
      assert.deepEqual(persisted['custom.data'].datasets[0].data, { catalog: '/custom' });
      assert.equal(sources.view().find(value => value.name === 'custom.data')!.error, 'data_source_refresh_failed');
    }
    await assert.rejects(sources.save('custom.data', { enabled: true, config: { id: '' } }), /data_source_config_required/);
    const savedPath = join(directory, 'data-sources.json');
    await rename(savedPath, `${savedPath}.retained`); await mkdir(savedPath);
    await assert.rejects(sources.toggle('custom.data', false), /data_source_save_failed/);
    assert.equal(sources.view().find(value => value.name === 'custom.data')!.enabled, true);
    assert.deepEqual(config.datasets.map(value => value.id), ['base', 'second']);
    await rm(savedPath, { recursive: true }); await rename(`${savedPath}.retained`, savedPath);
    await sources.toggle('qz.data.config', false);
    assert.deepEqual(config.datasets.map(value => value.id), ['second']);
  } finally { await sources.close(); await rm(directory, { recursive: true, force: true }); }
});


test('existing mandates retain source lookup after disable, omitted datasets and restart', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-retained-source-'));
  const file = join(directory, 'dataset.json');
  const original = { ...dataset('tracked'), revisionManifest: './source/current.json' };
  await writeFile(file, JSON.stringify(original));
  let config = configDefaults({ dataDir: directory });
  let sources = await dataSources(config);
  try {
    await sources.save(local, { enabled: true, config: { path: file } });
    await sources.refresh(local);
    const captured = structuredClone(existingDataset(config, 'tracked'));
    await sources.toggle(local, false);
    assert.deepEqual(config.datasets, [], 'disabled source is unavailable to new admissions');
    assert.deepEqual(existingDataset(config, 'tracked'), captured);
    await sources.close();
    config = configDefaults({ dataDir: directory }); sources = await dataSources(config);
    assert.deepEqual(config.datasets, []);
    assert.deepEqual(existingDataset(config, 'tracked'), captured, 'disabled source survives restart');
    await sources.toggle(local, true);
    await writeFile(file, '[]'); await sources.refresh(local);
    assert.deepEqual(config.datasets, [], 'an omitted dataset is unavailable to new admissions');
    assert.deepEqual(existingDataset(config, 'tracked'), captured);
    await sources.close();
    config = configDefaults({ dataDir: directory }); sources = await dataSources(config);
    assert.deepEqual(config.datasets, []);
    assert.deepEqual(existingDataset(config, 'tracked'), captured, 'omitted source manifest remains available after restart');
  } finally { await sources.close(); await rm(directory, { recursive: true, force: true }); }
});
