import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { evaluateTrial, trialFeedback } from '../src/trials.ts';
import { FactorTask } from '../plugins/native-factor-task.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
import research from '../plugins/research.ts';

const context = BACKGROUND_CONTEXT;
for (const mode of ['reopen', 'owner-stop'] as const) test(mode === 'reopen'
  ? 'closing during owned independent computation resumes the same STARTED selection, exposure and child after reopen'
  : 'owner stop during independent computation settles the original final as failed and never resumes its exposure', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-closure-recovery-'));
  const binary = join(directory, 'job.mjs'); const pidPath = join(directory, 'pid'); const allow = join(directory, 'continue'); const calls = join(directory, 'calls');
  const previous = process.env.QZ_JOB_BINARY;
  await writeFile(binary, `#!${process.execPath}\nimport { existsSync, writeFileSync, appendFileSync } from 'node:fs';
let input = ''; for await (const chunk of process.stdin) input += chunk;
appendFileSync(${JSON.stringify(calls)}, process.pid + '\\n'); writeFileSync(${JSON.stringify(pidPath)}, String(process.pid));
while (!existsSync(${JSON.stringify(allow)})) await new Promise(resolve => setTimeout(resolve, 5));
console.log(JSON.stringify({ request: JSON.parse(input), protocolOnly: true }));\n`, { mode: 0o700 });
  process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const request = { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '40' }, parameters: {} } };
  const data = { catalog: directory, request, sealed: { catalog: directory, request } };
  const trial = defineExtension({ name: 'test.closure-source', tasks: [FactorTask], tools: [defineTool({ name: 'candidate', description: 'Protocol-only registered candidate; no market evidence claim', parameters: Type.Object({}), replay: 'safe',
    execute(_args, api, ctx) { return trialFeedback(api, ctx, () => evaluateTrial(api, ctx, { model: '(module)', hypothesis: 'Original protocol candidate' }, async () => ({ request: iterationRequest(data),
      partitions: [{ partition: 'VALIDATION', metric: { pearson_ic: { status: 'AVAILABLE', value: 0.5 } } }] }))); },
  })] });
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [research, trial] };
  let host = await openResearch(options);
  try {
    faux.setResponses([fauxAssistantMessage(fauxToolCall('candidate', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('The original budget is complete.')]);
    const input = { goal: 'Recovery protocol only; not model-quality or performance evidence', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: { max_trials: 1, selection_metric: 'validation.pearson_ic' } };
    const receipt = await host.start('original', input);
    for (;;) {
      try { if (Number(await readFile(pidPath, 'utf8')) > 0) break; } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
      await new Promise(resolve => setTimeout(resolve, 5));
    }
    const before = (await host.state(receipt.conversationId)).research!;
    assert.equal(before.final!.status, 'STARTED');
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    assert.equal(tasks.items.length, 1);
    if (mode === 'owner-stop') {
      await host.stop(receipt.conversationId);
      const stopped = (await host.state(receipt.conversationId)).research!;
      assert.equal(stopped.final!.status, 'FAILED'); assert.equal(stopped.final!.error, 'research_closure_aborted');
      assert.equal(stopped.final!.taskId, before.final!.taskId); assert.deepEqual(stopped.final!.comparison, before.final!.comparison);
      const child = await host.harness.getTask(tasks.items[0]!.id, context);
      assert.equal(child!.state.status, 'terminal'); assert.equal(child!.state.outcome!.status, 'aborted');
      await host.close(); host = await openResearch(options); await host.harness.waitForIdle(context);
      assert.deepEqual((await host.state(receipt.conversationId)).research!.final, stopped.final);
      assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 1); assert.equal(faux.state.callCount, 2);
      return;
    }
    await host.close(); await writeFile(allow, 'resume'); host = await openResearch(options);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    await host.harness.waitForIdle(context);
    const after = (await host.state(receipt.conversationId)).research!;
    assert.equal(after.final!.status, 'COMPUTED', JSON.stringify({ final: after.final, tasks: await host.tasks(), submission: await host.submission(receipt.submissionId) }));
    assert.equal(after.final!.taskId, before.final!.taskId); assert.equal(after.final!.trialId, before.final!.trialId);
    assert.deepEqual(after.final!.comparison, before.final!.comparison);
    const restoredTasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    assert.deepEqual(restoredTasks.items.map(task => task.id), tasks.items.map(task => task.id));
    assert.equal((await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.research.close' }, 100), context)).items.length, 1);
    assert.equal(faux.state.callCount, 2);
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 2);
    assert.deepEqual(await host.start('original', input), receipt);
  } finally { await host.close(); if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await rm(directory, { recursive: true, force: true }); }
});
