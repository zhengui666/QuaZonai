import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import type { TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { configDefaults } from '../src/config.ts';
import type { DatasetConfig } from '../src/config.ts';
import { dataSources } from '../src/data-sources.ts';
const context = BACKGROUND_CONTEXT;
const local = 'qz.data.local-config';
const dataset = (id: string): DatasetConfig => ({ id, name: id, assetClass: 'CRYPTO', currency: 'USD', source: 'local', start: null, end: null,
  ready: true, data: { catalog: './catalog', sealed: { catalog: './heldout' }, request: {} } });
async function open(directory: string, extensions: ReturnType<typeof configDefaults>['extensions'] = []) {
  const config = configDefaults({ dataDir: directory, datasets: [dataset('base')], extensions });
  const sources = await dataSources(config);
  const host = await openResearch({ database: join(directory, 'sources.sqlite'), models: createModels(), extensions: [sources.extension], deferResume: true });
  await sources.connect(host);
  return { sources, host, config, async close() { await sources.close(); await host.close(); } };
}
async function settle(state: Awaited<ReturnType<typeof open>>, id: number) {
  const task = await state.host.harness.waitForTask(id as TaskId, context);
  assert.equal(task.state.status, 'terminal'); return task.state.outcome;
}
test('durable source publication survives reopen and broken input preserves successful immutable admissions', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-durable-'));
  let state = await open(directory);
  try {
    await mkdir(join(directory, 'export')); const path = join(directory, 'export/dataset.json');
    await writeFile(path, JSON.stringify(dataset('imported')));
    await state.sources.save(local, { enabled: true, config: { path: 'export/dataset.json' } });
    const { taskId } = await state.sources.refresh(local);
    assert.equal((await settle(state, taskId)).status, 'completed');
    const captured = structuredClone(state.config.datasets[1]);
    assert.equal(captured.id, 'imported'); assert.equal(captured.data.catalog, join(directory, 'export/catalog'));
    await state.close(); state = await open(directory);
    assert.deepEqual(state.config.datasets[1], captured);
    await writeFile(path, '{invalid');
    assert.equal((await settle(state, (await state.sources.refresh(local)).taskId)).status, 'failed');
    assert.deepEqual(state.config.datasets[1], captured);
    assert.equal(state.sources.view().find(v => v.name === local)!.error, 'data_source_refresh_failed');
    await state.sources.toggle(local, false); assert.deepEqual(state.config.datasets.map(v => v.id), ['base']);
    await assert.rejects(state.sources.refresh(local), /data_source_disabled/);
    await state.sources.toggle(local, true); assert.deepEqual(state.config.datasets[1], captured);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});
test('an admitted source refresh resumes from official task storage after host close without resubmission', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-restart-'));
  let state = await open(directory);
  try {
    await writeFile(join(directory, 'dataset.json'), JSON.stringify(dataset('resumed')));
    await state.sources.save(local, { enabled: true, config: { path: 'dataset.json' } });
    const { taskId } = await state.sources.refresh(local);
    await state.close(); state = await open(directory);
    assert.equal((await settle(state, taskId)).status, 'completed');
    assert.equal(state.config.datasets[1]!.id, 'resumed');
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});
test('same revision refresh reuses the official active task and disabling explicitly aborts it', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-abort-')); const module = join(directory, 'slow.mjs');
  await writeFile(module, `export default {name:'test.slow',fields:[],async load(_config,{signal}) { await new Promise((_,reject)=>signal.addEventListener('abort',()=>reject(new Error('aborted')),{once:true}));return []}}`);
  const state = await open(directory, [{ name: 'test.slow', kind: 'data', enabled: true, module }]);
  try {
    const first = await state.sources.refresh('test.slow'); const second = await state.sources.refresh('test.slow');
    assert.equal(first.taskId, second.taskId);
    state.host.harness.resume(); await new Promise(resolve => setImmediate(resolve));
    await state.sources.toggle('test.slow', false);
    assert.equal((await settle(state, first.taskId)).status, 'aborted');
    assert.equal(state.sources.view().find(v => v.name === 'test.slow')!.enabled, false);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});
test('source publication conflicts and provider failures cannot corrupt base datasets or disclose private errors', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-conflict-')); const module = join(directory, 'custom.mjs');
  await writeFile(module, `export default {name:'test.custom',fields:[{key:'fail',label:'fail',type:'boolean'}],async load(c) {if(c.fail)throw new Error('private-credential');return [${JSON.stringify(dataset('base'))}]}}`);
  const state = await open(directory, [{ name: 'test.custom', kind: 'data', enabled: true, module }]);
  try {
    assert.equal((await settle(state, (await state.sources.refresh('test.custom')).taskId)).status, 'failed');
    assert.deepEqual(state.config.datasets.map(v => v.id), ['base']);
    await state.sources.save('test.custom', { enabled: true, config: { fail: true } });
    const outcome = await settle(state, (await state.sources.refresh('test.custom')).taskId);
    assert.equal(outcome.status, 'failed'); assert.doesNotMatch(JSON.stringify(outcome), /private-credential/);
    await assert.rejects(state.sources.save('test.custom', { enabled: true, config: { unknown: 1 } }), /field_invalid/);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});
