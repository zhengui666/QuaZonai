import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import type { TaskGraph, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { configDefaults, type DatasetConfig } from '../src/config.ts';
import { dataSources, DataSourceState } from '../src/data-sources.ts';

const context = BACKGROUND_CONTEXT;
const dataset = (id: string): DatasetConfig => ({ id, name: id, assetClass: 'CRYPTO', currency: 'USD', source: 'local', start: null, end: null,
  ready: true, data: { catalog: './catalog', sealed: { catalog: './heldout' }, request: {} } });

async function open(directory: string, extensions: ReturnType<typeof configDefaults>['extensions'] = []) {
  const config = configDefaults({ dataDir: directory, datasets: [dataset('base')], extensions });
  const sources = await dataSources(config);
  const host = await openResearch({ database: join(directory, 'sources.sqlite'), models: createModels(), extensions: [sources.extension], deferResume: true });
  await sources.connect(host);
  return { sources, host, config, async close() { await sources.close(); await host.close(); } };
}

test('two sources cannot atomically publish the same dataset after both load phases succeeded', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-owner-race-'));
  const key = `qz-source-race-${directory}`;
  let release!: () => void; const gate = new Promise<void>(resolve => { release = resolve; });
  let started!: () => void; const bothStarted = new Promise<void>(resolve => { started = resolve; });
  const controls = { count: 0, gate, started };
  (globalThis as unknown as Record<string, unknown>)[key] = controls;
  const extensions: ReturnType<typeof configDefaults>['extensions'] = [];
  for (const name of ['test.left', 'test.right']) {
    const module = join(directory, `${name}.mjs`);
    await writeFile(module, `export default {name:${JSON.stringify(name)},fields:[],async load(){ const gate=globalThis[${JSON.stringify(key)}]; if(++gate.count===2)gate.started(); await gate.gate; return [${JSON.stringify(dataset('shared'))}]; }};`);
    extensions.push({ name, kind: 'data', enabled: true, module });
  }
  let state = await open(directory, extensions);
  try {
    const left = (await state.sources.refresh('test.left')).taskId as TaskId;
    const right = (await state.sources.refresh('test.right')).taskId as TaskId;
    const loaded = new Set<number>();
    const inspect = (graph: TaskGraph) => {
      for (const id of [left, right]) {
        const task = graph.tasks[String(id)];
        if (task && task.state.status !== 'completing' && task.state.phase === 'publish') loaded.add(id);
      }
    };
    const observer = await state.host.harness.observeTaskGraph({ advance(call) { if (call.kind === 'value') inspect(call.value); }, end() {} }, context);
    state.host.harness.resume(); await bothStarted; release();
    const settled = await Promise.all([left, right].map(id => state.host.harness.waitForTask(id, context)));
    observer.detach();
    assert.equal(loaded.size, 2, 'both independently loaded sources must reach the durable publication phase');
    assert.deepEqual(settled.map(task => task.state.outcome.status).sort(), ['completed', 'failed']);
    const saved = await state.host.harness.snapshot(DataSourceState, context);
    assert.equal(Object.values(saved!.sources).filter(value => (value as unknown as { datasets: DatasetConfig[] }).datasets.some(row => row.id === 'shared')).length, 1);
    assert.deepEqual(state.config.datasets.map(value => value.id).sort(), ['base', 'shared']);
    await state.close(); state = await open(directory, extensions);
    assert.deepEqual(state.config.datasets.map(value => value.id).sort(), ['base', 'shared']);
  } finally { release(); delete (globalThis as unknown as Record<string, unknown>)[key]; await state.close(); await rm(directory, { recursive: true, force: true }); }
});

test('saving and toggling a source retains its completed raw acquisition artifact', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-artifact-'));
  let state = await open(directory);
  const name = 'qz.data.acquisition-plan';
  try {
    const artifact = { manifestPath: join(directory, 'raw-market-data.json'), planId: 'a'.repeat(64) };
    await writeFile(artifact.manifestPath, JSON.stringify({ source_root: directory }));
    // Seed a completed provider result through the same official document used by publication.
    // This regression targets metadata retention, not the independently tested transfer protocol.
    await state.host.harness.commit(async tx => {
      const saved = (await tx.doc(DataSourceState)).sources[name] as Record<string, unknown>;
      saved.acquisition = artifact;
    }, context);
    const update = { enabled: false, config: { kind: 'hub', planPath: 'plan.json', cacheDir: 'cache' } };
    assert.deepEqual((await state.sources.save(name, update)).acquisition, artifact);
    assert.deepEqual((await state.sources.toggle(name, true)).acquisition, artifact);
    assert.deepEqual((await state.sources.toggle(name, false)).acquisition, artifact);
    await state.close(); state = await open(directory);
    assert.deepEqual(state.sources.view().find(source => source.name === name)!.acquisition, artifact);
    assert.ok(await readFile(artifact.manifestPath, 'utf8'));
    assert.deepEqual(state.config.datasets.map(value => value.id), ['base']);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});

test('unchanged static configuration preserves the admitted refresh revision across a restart', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-static-restart-'));
  let state = await open(directory);
  const name = 'qz.data.config';
  try {
    const { taskId } = await state.sources.refresh(name);
    const before = (await state.host.harness.snapshot(DataSourceState, context))!.sources[name] as { revision: number; taskId: number };
    assert.equal(before.taskId, taskId);
    await state.close(); state = await open(directory);
    const after = (await state.host.harness.snapshot(DataSourceState, context))!.sources[name] as { revision: number; taskId: number };
    assert.equal(after.revision, before.revision); assert.equal(after.taskId, taskId);
    assert.equal((await state.host.harness.waitForTask(taskId as TaskId, context)).state.outcome.status, 'completed');
    assert.deepEqual(state.config.datasets.map(value => value.id), ['base']);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});

test('changing configuration joins the old revision and disabling joins its replacement refresh', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-source-config-abort-'));
  const key = `qz-source-config-abort-${directory}`;
  const entered = new Map<string, () => void>();
  const firstEntered = new Promise<void>(resolve => { entered.set('first', resolve); });
  const secondEntered = new Promise<void>(resolve => { entered.set('second', resolve); });
  (globalThis as unknown as Record<string, unknown>)[key] = { started(revision: string) { entered.get(revision)!(); } };
  const module = join(directory, 'blocked.mjs');
  await writeFile(module, `export default {name:'test.config-abort',fields:[{key:'revision',label:'revision',type:'text'}],async load(config,{signal}){ globalThis[${JSON.stringify(key)}].started(config.revision); await new Promise((_,reject)=>{ if(signal.aborted)reject(new Error('aborted'));else signal.addEventListener('abort',()=>reject(new Error('aborted')),{once:true}); }); return []; }};`);
  const state = await open(directory, [{ name: 'test.config-abort', kind: 'data', enabled: true, module, config: { revision: 'first' } }]);
  try {
    const first = (await state.sources.refresh('test.config-abort')).taskId as TaskId;
    state.host.harness.resume(); await firstEntered;
    await state.sources.save('test.config-abort', { enabled: true, config: { revision: 'second' } });
    const original = await state.host.harness.getTask(first, context);
    assert.ok(original); assert.equal(original.state.status, 'terminal');
    if (original.state.status === 'terminal') assert.equal(original.state.outcome.status, 'aborted');
    const replacement = (await state.sources.refresh('test.config-abort')).taskId as TaskId;
    assert.notEqual(replacement, first); await secondEntered;
    await state.sources.toggle('test.config-abort', false);
    const stopped = await state.host.harness.getTask(replacement, context);
    assert.ok(stopped); assert.equal(stopped.state.status, 'terminal');
    if (stopped.state.status === 'terminal') assert.equal(stopped.state.outcome.status, 'aborted');
    const live = await state.host.harness.commit(tx => tx.scanTasks({ kind: 'qz.data-source.refresh' }, 100), context);
    assert.ok(live.items.every(task => task.state.status === 'terminal'));
    assert.deepEqual(state.config.datasets.map(value => value.id), ['base']);
  } finally { delete (globalThis as unknown as Record<string, unknown>)[key]; await state.close(); await rm(directory, { recursive: true, force: true }); }
});
