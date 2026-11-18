import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { openResearch } from '../../research/src/host.ts';
import { configDefaults, type DatasetConfig } from '../src/config.ts';
import { dataSources, DataSourceState } from '../src/data-sources.ts';

const dataset = (id: string): DatasetConfig => ({ id, name: id, assetClass: 'CRYPTO', currency: 'USD', source: 'retained-source', start: null, end: null,
  ready: true, data: { catalog: `/retained/${id}/catalog`, sealed: { catalog: `/retained/${id}/heldout` }, request: {} } });
async function open(directory: string, extensions: ReturnType<typeof configDefaults>['extensions'] = []) {
  const config = configDefaults({ dataDir: directory, datasets: [], extensions });
  const sources = await dataSources(config);
  const host = await openResearch({ database: join(directory, 'sources.sqlite'), models: createModels(), extensions: [sources.extension], deferResume: true });
  await sources.connect(host);
  return { sources, host, config, async close() { await sources.close(); await host.close(); } };
}

test('opening registry retains all saved source datasets without importing modules or rereading descriptors', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-retained-sources-'));
  const module = join(directory, 'must-not-import.mjs'), marker = join(directory, 'executed');
  await writeFile(module, `import {writeFileSync} from 'node:fs'; writeFileSync(${JSON.stringify(marker)},'executed'); throw Error('retired module executed');`);
  const extensions = [{ name: 'old.external', kind: 'data' as const, enabled: true, module }];
  let state = await open(directory, extensions);
  try {
    const rows = Array.from({ length: 12 }, (_, index) => dataset(`retained-${index}`));
    const original = { config: { path: '/missing/old-source.json' }, enabled: true, datasets: rows, retainedDatasets: rows,
      lastLoadedAt: '2026-01-02T00:00:00.000Z', error: null, revision: 9,
      acquisition: { manifestPath: '/missing/old-manifest.json', planId: 'original-plan' } };
    await state.host.harness.commit(async tx => { (await tx.doc(DataSourceState)).sources['old.external'] = JSON.parse(JSON.stringify(original)); }, context);
    await state.close(); state = await open(directory, extensions);
    assert.deepEqual(state.config.datasets.map(row => row.id), rows.map(row => row.id));
    assert.deepEqual(state.config.retainedDatasets!.map(row => row.id), rows.map(row => row.id));
    assert.deepEqual((await state.host.harness.snapshot(DataSourceState, context))!.sources['old.external'], original);
    assert.deepEqual(state.sources.view().find(row => row.name === 'old.external')!.acquisition, original.acquisition);
    await assert.rejects(readFile(marker), /ENOENT/);
    assert.equal((await state.host.harness.commit(tx => tx.scanTasks({ kind: 'qz.data-source.refresh' }, 100), context)).items.length, 0);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});

test('disabled and removed historical dataset references stay retained across restart', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-disabled-sources-'));
  let state = await open(directory);
  try {
    const held = dataset('disabled-catalog'), removed = dataset('removed-catalog');
    const original = { config: {}, enabled: false, datasets: [held], retainedDatasets: [held, removed], lastLoadedAt: null, error: 'original-provider-error', revision: 3 };
    await state.host.harness.commit(async tx => { (await tx.doc(DataSourceState)).sources['old.disabled'] = JSON.parse(JSON.stringify(original)); }, context);
    await state.close(); state = await open(directory);
    assert.deepEqual(state.config.datasets, []);
    assert.deepEqual(state.config.retainedDatasets!.map(row => row.id), [held.id, removed.id]);
    assert.deepEqual((await state.host.harness.snapshot(DataSourceState, context))!.sources['old.disabled'], original);
    assert.equal(state.sources.view().find(row => row.name === 'old.disabled')!.enabled, false);
  } finally { await state.close(); await rm(directory, { recursive: true, force: true }); }
});

test('original refresh identities recover without running a retired loader; published checkpoints can finish', async () => {
  const { defineTask, defineExtension } = await import('@earendil-works/pi-durable');
  for (const phase of ['load', 'publish'] as const) {
    const directory = await mkdtemp(join(tmpdir(), 'qz-source-original-task-'));
    const loaded = JSON.parse(JSON.stringify([dataset('already-loaded')]));
    const Original = defineTask<{ name: string; revision: number; pluginVersion: string | null }, { phase: 'load' } | { phase: 'publish'; loaded: typeof loaded }, Record<string, never>>({
      name: 'qz.data-source.refresh', version: 1,
      initial: () => phase === 'publish' ? { phase, loaded } : { phase },
      phases: { async load() { throw Error('original loader must not execute'); }, async publish() { throw Error('original writer must not execute'); } },
      abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
    });
    const prior = await openResearch({ database: join(directory, 'sources.sqlite'), models: createModels(), deferResume: true,
      extensions: [defineExtension({ name: 'qz.data-source-runtime', tasks: [Original] })] });
    let next: Awaited<ReturnType<typeof open>> | undefined;
    try {
      const id = await prior.harness.commit(async tx => {
        (await tx.doc(DataSourceState)).sources['old.source'] = { enabled: true, config: {}, datasets: [], retainedDatasets: [], revision: 3, lastLoadedAt: null, error: null };
        return tx.createTask(Original, { name: 'old.source', revision: 3, pluginVersion: '1' }, { ownership: { kind: 'conversation' } });
      }, context);
      await prior.close(); next = await open(directory); next.host.harness.resume();
      const recovered = await next.host.harness.waitForTask(id, context);
      assert.equal(recovered.id, id);
      assert.equal(recovered.state.outcome.status, phase === 'publish' ? 'completed' : 'failed');
      if (recovered.state.outcome.status === 'failed') assert.match(recovered.state.outcome.error.message, /data_source_plugins_retired/);
      assert.deepEqual(next.config.datasets.map(row => row.id), phase === 'publish' ? ['already-loaded'] : []);
    } finally { await next?.close(); if (!next) await prior.close(); await rm(directory, { recursive: true, force: true }); }
  }
});
