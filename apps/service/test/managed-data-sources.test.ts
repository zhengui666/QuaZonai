import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { openResearch } from '../../research/src/host.ts';
import { configDefaults } from '../src/config.ts';
import { dataSources } from '../src/data-sources.ts';
import { DataSourceLoads } from '../src/data-source-tasks.ts';

const local = 'qz.data.local-config';
const dataset = (id: string) => ({ id, name: id, assetClass: 'CRYPTO', currency: 'USD', source: 'local', start: null, end: null, ready: true, data: { catalog: '/original/catalog', request: {} } });

test('managed source retry reuses committed original load instead of rereading changed source', async () => {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-managed-source-')); const database = join(dataDir, 'sources.sqlite');
  const input = join(dataDir, 'dataset.json'); await writeFile(input, JSON.stringify(dataset('original')));
  let config = configDefaults({ dataDir }); let sources = await dataSources(config);
  let host = await openResearch({ database, models: createModels(), extensions: [sources.extension] }); sources.connect(host);
  try {
    await sources.save(local, { enabled: true, config: { path: input } });
    assert.deepEqual((await sources.refresh(local, 'load-original')).datasetIds, ['original']);
    const before = await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT);
    assert.equal(Object.keys(before!.records).length, 1);
    await writeFile(input, JSON.stringify(dataset('later')));
    assert.deepEqual((await sources.refresh(local, 'load-original')).datasetIds, ['original']);
    await sources.close(); await host.close();
    config = configDefaults({ dataDir }); sources = await dataSources(config);
    host = await openResearch({ database, models: createModels(), extensions: [sources.extension] }); sources.connect(host);
    assert.deepEqual((await sources.refresh(local, 'load-original')).datasetIds, ['original']);
    assert.deepEqual(await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT), before);
    assert.deepEqual((await sources.refresh(local, 'load-new')).datasetIds, ['later']);
    assert.deepEqual(config.datasets.map(row => row.id), ['later']);
  } finally { await sources.close(); await host.close(); await rm(dataDir, { recursive: true, force: true }); }
});

test('plugin credentials stay in private input while undeclared conversion is refused before load', async () => {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-private-source-')); const database = join(dataDir, 'sources.sqlite');
  const module = join(dataDir, 'provider.mjs'); const secret = 'private-plugin-password';
  await writeFile(module, `export default {name:'test.private',version:'1',recovery:'read-only',fields:[{key:'token',label:'Token',type:'password',required:true}],async load(){return []}};`);
  const config = configDefaults({ dataDir, extensions: [{ name: 'test.private', module, kind: 'data', enabled: true, config: { token: secret } }] });
  const sources = await dataSources(config); const host = await openResearch({ database, models: createModels(), extensions: [sources.extension] }); sources.connect(host);
  try {
    await sources.refresh('test.private', 'one-load');
    const facts = await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT);
    assert.equal(JSON.stringify(facts).includes(secret), false);
    const taskId = Object.values(facts!.records)[0]!.taskId as number;
    const tasks = await host.tasks(); assert.equal(JSON.stringify(tasks).includes(secret), false);
    const files = await readdir(join(dataDir, '.data-source-loads'));
    const privateFile = join(dataDir, '.data-source-loads', files[0]!);
    assert.ok((await readFile(privateFile, 'utf8')).includes(secret));
    assert.equal((await stat(privateFile)).mode & 0o777, 0o600);
    assert.ok(taskId > 0);
  } finally { await sources.close(); await host.close(); await rm(dataDir, { recursive: true, force: true }); }
  const other = await mkdtemp(join(tmpdir(), 'qz-unsafe-source-'));
  try {
    const file = join(other, 'unsafe.mjs'); await writeFile(file, `export default {name:'unsafe.convert',fields:[],async load(){throw new Error('must-not-run')}}`);
    const config = configDefaults({ dataDir: other, extensions: [{ name: 'unsafe.convert', module: file, kind: 'data', enabled: true }] });
    const sources = await dataSources(config); const host = await openResearch({ database: join(other, 'data.sqlite'), models: createModels(), extensions: [sources.extension] }); sources.connect(host);
    try { await assert.rejects(sources.refresh('unsafe.convert', 'unsupported'), /data_source_recovery_capability_required/); assert.equal(await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT), undefined); }
    finally { await sources.close(); await host.close(); }
  } finally { await rm(other, { recursive: true, force: true }); }
});


test('closing the connected host suspends an uncommitted read and restart resumes the same official task', async () => {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-source-suspend-')); const database = join(dataDir, 'sources.sqlite');
  const module = join(dataDir, 'reader.mjs'); const globalKey = `qzSuspend${Date.now()}`;
  let entered!: () => void; const started = new Promise<void>(resolve => { entered = resolve; });
  const state = { first: true, calls: 0, entered };
  Object.assign(globalThis, { [globalKey]: state });
  await writeFile(module, `export default {name:'test.suspend',version:'1',recovery:'read-only',fields:[],async load(_,{signal}){const state=globalThis[${JSON.stringify(globalKey)}];state.calls++;if(state.first){state.entered();await new Promise((resolve,reject)=>{if(signal.aborted)reject(new Error('aborted'));else signal.addEventListener('abort',()=>reject(new Error('aborted')),{once:true})});}return []}};`);
  const options = { dataDir, extensions: [{ name: 'test.suspend', module, kind: 'data' as const, enabled: true }] };
  let config = configDefaults(options); let sources = await dataSources(config);
  let host = await openResearch({ database, models: createModels(), extensions: [sources.extension] }); sources.connect(host);
  try {
    const pending = sources.refresh('test.suspend', 'original-operation');
    const suspended = assert.rejects(pending);
    await started;
    const original = await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT);
    await sources.close(); await host.close(); await suspended;
    state.first = false;
    config = configDefaults(options); sources = await dataSources(config);
    host = await openResearch({ database, models: createModels(), extensions: [sources.extension] }); sources.connect(host);
    assert.deepEqual((await sources.refresh('test.suspend', 'original-operation')).datasetIds, []);
    const restored = await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT);
    assert.equal(Object.keys(restored!.records).length, 1);
    assert.equal(Object.values(restored!.records)[0]!.taskId, Object.values(original!.records)[0]!.taskId);
    assert.equal(state.calls, 2, 'only the interrupted, uncommitted read is retried');
  } finally { await sources.close(); await host.close(); delete (globalThis as Record<string, unknown>)[globalKey]; await rm(dataDir, { recursive: true, force: true }); }
});

test('a late refresh cannot overwrite a newer source configuration and replays the superseded receipt', async () => {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-source-race-')); const database = join(dataDir, 'sources.sqlite');
  const module = join(dataDir, 'reader.mjs'); const globalKey = `qzSourceRace${Date.now()}`;
  let entered!: () => void; let release!: () => void;
  const started = new Promise<void>(resolve => { entered = resolve; }); const gate = new Promise<void>(resolve => { release = resolve; });
  Object.assign(globalThis, { [globalKey]: { entered, gate } });
  await writeFile(module, `export default {name:'test.race',version:'1',recovery:'read-only',fields:[{key:'revision',label:'Revision',type:'text'}],async load(config){globalThis[${JSON.stringify(globalKey)}].entered();await globalThis[${JSON.stringify(globalKey)}].gate;return []}};`);
  const config = configDefaults({ dataDir, extensions: [{ name: 'test.race', module, kind: 'data', enabled: true, config: { revision: 'one' } }] });
  const sources = await dataSources(config); const host = await openResearch({ database, models: createModels(), extensions: [sources.extension] }); sources.connect(host);
  try {
    const pending = sources.refresh('test.race', 'original-operation'); const superseded = assert.rejects(pending, /data_source_refresh_configuration_changed/);
    await started; await sources.save('test.race', { enabled: true, config: { revision: 'two' } }); release(); await superseded;
    assert.equal(sources.view().find(row => row.name === 'test.race')!.config.revision, 'two');
    await assert.rejects(sources.refresh('test.race', 'original-operation'), /data_source_refresh_configuration_changed/);
    assert.equal(Object.keys((await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT))!.records).length, 1);
  } finally { release(); await sources.close(); await host.close(); delete (globalThis as Record<string, unknown>)[globalKey]; await rm(dataDir, { recursive: true, force: true }); }
});
