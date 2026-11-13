import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { nativeCommands } from '../src/native-execution.ts';

test('retired automatic walk-forward never submits a new native execution', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-retired-wf-'));
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    assert.equal(nativeCommands.has('native-walk-forward'), false);
    const root = await host.harness.root(context);
    const request = { schema_version: 1, candidates: [], split: { n_splits: 2 }, objective: {} };
    const id = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, {
      request, walkForward: true, catalog: directory, target: { id: 'never-contact', url: 'http://127.0.0.1:1' },
    }, { ownership: { kind: 'conversation' } }), context);
    const task = await host.harness.waitForTask(id, context);
    assert.equal(task.state.outcome.status, 'failed');
    assert.equal(task.state.outcome.status === 'failed' && task.state.outcome.error.message, 'qz_automatic_walk_forward_removed');
    assert.deepEqual((task.input as { request: unknown }).request, request);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
