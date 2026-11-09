import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import type { TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { configDefaults } from '../src/config.ts';
import { dataSources } from '../src/data-sources.ts';
import { DataSourceLoads } from '../src/data-source-tasks.ts';

for (const failure of ['connection', 'rate-limit', 'server'] as const) test(`read-only source ${failure} interruption waits and retries the original task`, { timeout: 15_000 }, async () => {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-source-network-')); const module = join(dataDir, 'reader.mjs');
  const key = `qzSourceNetwork${failure}${Date.now()}`;
  Object.assign(globalThis, { [key]: { available: false, calls: 0 } });
  await writeFile(module, `export default {name:'test.network',version:'1',recovery:'read-only',fields:[],async load(){const state=globalThis[${JSON.stringify(key)}];state.calls++;if(!state.available){const error=new Error('temporary source failure');${failure === 'connection' ? "error.cause=Object.assign(new Error('connect failed'),{code:'ECONNRESET'});" : `error.status=${failure === 'rate-limit' ? 429 : 503};`}throw error;}return []}};`);
  const config = configDefaults({ dataDir, extensions: [{ name: 'test.network', module, kind: 'data', enabled: true }] });
  const sources = await dataSources(config); const host = await openResearch({ database: join(dataDir, 'state.sqlite'), models: createModels(), extensions: [sources.extension] }); sources.connect(host);
  try {
    const pending = sources.refresh('test.network', 'original-read');
    let originalTaskId: number | undefined;
    for (let i = 0; i < 100; i++) {
      const rows = await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT);
      originalTaskId = rows && Number(Object.values(rows.records)[0]?.taskId);
      const task = originalTaskId ? await host.harness.getTask(originalTaskId as TaskId, BACKGROUND_CONTEXT) : undefined;
      if (task && 'checkpoint' in task.state && (task.state.checkpoint as { phase?: string }).phase === 'wait') break;
      await setTimeout(10);
    }
    assert.ok(originalTaskId);
    const waiting = await host.harness.getTask(originalTaskId as TaskId, BACKGROUND_CONTEXT);
    assert.notEqual(waiting!.state.status, 'terminal', 'temporary failure remains live instead of becoming a failed load');
    assert.equal('checkpoint' in waiting!.state && (waiting!.state.checkpoint as { phase?: string }).phase, 'wait');
    (globalThis as unknown as Record<string, { available: boolean; calls: number }>)[key]!.available = true;
    assert.deepEqual((await pending).datasetIds, []);
    const records = (await host.harness.snapshot(DataSourceLoads, BACKGROUND_CONTEXT))!.records;
    assert.equal(Object.keys(records).length, 1); assert.equal(Object.values(records)[0]!.taskId, originalTaskId);
    assert.equal((globalThis as unknown as Record<string, { calls: number }>)[key]!.calls, 2);
  } finally { await sources.close(); await host.close(); delete (globalThis as Record<string, unknown>)[key]; await rm(dataDir, { recursive: true, force: true }); }
});
