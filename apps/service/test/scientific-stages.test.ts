import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { awaitWithContext } from '@earendil-works/chord/context';
import { defineExtension, defineTask, type ConversationId, type JsonObject } from '@earendil-works/pi-durable';
import { researchRuntime } from '../../research/src/research-runtime.ts';
import { runScientificStage, ScientificStages } from '../src/scientific-stages.ts';
import { context, setup, studied } from './support.ts';

test('a resumed parent consumes its original completed official scientific child instead of executing again', { timeout: 20_000 }, async () => {
  const state = await setup();
  const log = join(state.directory, 'native-calls'); const binary = join(state.directory, 'native-stage.mjs');
  await writeFile(binary, `#!${process.execPath}\nimport { appendFileSync } from 'node:fs';\nfor await (const chunk of process.stdin) {}\nappendFileSync(${JSON.stringify(log)}, 'run\\n');\nconsole.log(JSON.stringify({ verified: true }));\n`, { mode: 0o700 });
  let suspend = true; let arrived!: () => void; const checkpoint = new Promise<void>(resolve => { arrived = resolve; });
  const Parent = defineTask<{ binary: string; dataDir: string }, { phase: 'compute' }, JsonObject>({
    name: 'test.scientific-stage.parent', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      const result = await runScientificStage(researchRuntime(runtime), ctx, 'original-native', 1,
        { kind: 'native', binary: task.input.binary, dataDir: task.input.dataDir, command: 'simulate', input: { original: true } });
      if (suspend) { arrived(); await awaitWithContext(new Promise<void>(() => {}), ctx); }
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), ctx);
    } },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  state.options.extensions.push(defineExtension({ name: 'test.scientific-stages', tasks: [Parent] }));
  await state.restart();
  try {
    const { receipt } = await studied(state);
    const parent = await state.service.research.harness.commit(tx => tx.createTask(Parent, { binary, dataDir: state.directory },
      { conversationId: receipt.conversationId as ConversationId, ownership: { kind: 'conversation' } }), context);
    await checkpoint;
    const before = await state.service.research.harness.snapshot(ScientificStages, String(parent), context);
    assert.equal(Object.keys(before!.children).length, 1);
    assert.equal((await readFile(log, 'utf8')).trim(), 'run');
    suspend = false; await state.restart();
    const settled = await state.service.research.harness.waitForTask(parent, context);
    assert.equal(settled.state.outcome.status, 'completed');
    assert.deepEqual(await state.service.research.harness.snapshot(ScientificStages, String(parent), context), before);
    assert.equal((await readFile(log, 'utf8')).trim(), 'run');
  } finally { await state.cleanup(); }
});
