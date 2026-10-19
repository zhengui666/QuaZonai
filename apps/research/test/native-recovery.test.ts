import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { evaluateTrial, trialFeedback } from '../src/trials.ts';
import { FactorTask, runFactor } from '../plugins/native-factor-task.ts';
import research from '../plugins/research.ts';

const context = BACKGROUND_CONTEXT;
const tool = (name: string, args: JsonObject = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
const request = { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: {} } };
function toolText(transcript: { messages: readonly unknown[] }, name: string): string {
  const result = transcript.messages.filter(item => (item as { role: string; toolName?: string }).role === 'toolResult' && (item as { toolName?: string }).toolName === name).at(-1) as { content: { text?: string }[] };
  const text = result.content.map(item => item.text ?? '').join('');
  assert.doesNotMatch(text, /private_source_marker|factor\.wat/);
  return text;
}
function textResult(transcript: { messages: readonly unknown[] }, name: string): JsonObject {
  return JSON.parse(toolText(transcript, name)) as JsonObject;
}

test('WAT diagnostics remain in the original child and trial, not model-facing failure/read feedback', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-wat-diagnostic-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const model = '(module (func (export "predict") (result f64) private_source_marker))';
  const data = { catalog: directory, request };
  const compute = defineExtension({ name: 'test.wat-failure', tasks: [FactorTask], tools: [defineTool({
    name: 'compute', description: 'Controlled invalid WAT protocol', parameters: Type.Object({}), replay: 'safe',
    execute(_args, api, ctx) { return trialFeedback(api, ctx, () => evaluateTrial(api, ctx, { model, hypothesis: 'Invalid original model' }, () => runFactor(api, ctx, model, data))); },
  })] });
  const options = { database: join(directory, 'original.sqlite'), models, extensions: [research, compute] };
  let trialId: number;
  faux.setResponses([
    tool('compute'),
    transcript => { assert.match(toolText(transcript, 'compute'), /wasm_compile_failed/); return tool('research_brief'); },
    transcript => { const trial = (textResult(transcript, 'research_brief').trials as JsonObject[])[0]!; assert.equal(trial.error, 'wasm_compile_failed'); trialId = trial.taskId as number; return tool('research_trial', { trialId }); },
    transcript => { assert.equal(textResult(transcript, 'research_trial').error, 'wasm_compile_failed'); return tool('research_finish', { reason: 'Stop with the original failed attempt retained.' }); },
  ]);
  let host = await openResearch(options);
  try {
    const input = { goal: 'Protocol-only invalid model', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: { max_trials: 1 } };
    const receipt = await host.start('original-wat-failure', input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await host.state(receipt.conversationId)).research!;
    assert.equal(study.trials.length, 1); assert.equal(study.trials[0]!.taskId, trialId!); assert.equal(study.trials[0]!.status, 'FAILED');
    assert.match(String(study.trials[0]!.error), /private_source_marker/);
    assert.equal((study.trials[0]!.errorDetail as JsonObject).stage, 'compile');
    assert.equal((await host.harness.waitForTask(trialId! as TaskId, context)).state.outcome.status, 'failed');
    const children = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    const child = children.items[0]!;
    assert.equal(children.items.length, 1); assert.equal(child.state.outcome!.status, 'failed');
    if (child.state.outcome?.status === 'failed') {
      assert.equal(child.state.outcome.error.message, study.trials[0]!.error);
      assert.equal((child.state.outcome.error.detail as JsonObject).operation, 'evaluate-factor');
    }
    await host.close(); host = await openResearch(options);
    assert.deepEqual((await host.state(receipt.conversationId)).research, study);
    assert.deepEqual(await host.start('original-wat-failure', input), receipt);
    assert.equal(faux.state.callCount, 4);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('closing during real native execution joins the child and resumes the same admitted trial/task on reopen', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-factor-suspend-'));
  const binary = join(directory, 'job.mjs'); const pidPath = join(directory, 'pid'); const allow = join(directory, 'continue'); const calls = join(directory, 'calls');
  const previous = process.env.QZ_JOB_BINARY;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const data = { catalog: directory, request }; const model = '(module)';
  await writeFile(binary, `#!${process.execPath}
import { existsSync, writeFileSync, appendFileSync } from 'node:fs';
let input = ''; for await (const chunk of process.stdin) input += chunk;
appendFileSync(${JSON.stringify(calls)}, process.pid + '\\n');
writeFileSync(${JSON.stringify(pidPath)}, String(process.pid));
while (!existsSync(${JSON.stringify(allow)})) await new Promise(resolve => setTimeout(resolve, 5));
console.log(JSON.stringify({ request: JSON.parse(input) }));
`, { mode: 0o700 });
  process.env.QZ_JOB_BINARY = binary;
  const compute = defineExtension({ name: 'test.factor-suspend', tasks: [FactorTask], tools: [defineTool({
    name: 'compute', description: 'Native suspend protocol', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) {
      await evaluateTrial(api, ctx, { model, hypothesis: 'Original interrupted attempt' }, () => runFactor(api, ctx, model, data));
      return { content: [{ type: 'text', text: 'Original task completed.' }], control: { terminate: true } };
    },
  })] });
  const options = { database: join(directory, 'original.sqlite'), models, extensions: [compute] };
  faux.setResponses([tool('compute')]);
  let host = await openResearch(options);
  try {
    const input = { goal: 'Protocol-only native suspension', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: { max_trials: 1 } };
    const receipt = await host.start('original-interrupted-factor', input);
    let pid: number;
    for (;;) {
      try { pid = Number(await readFile(pidPath, 'utf8')); if (pid > 0) break; }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
      await new Promise(resolve => setTimeout(resolve, 5));
    }
    const original = (await host.state(receipt.conversationId)).research!.trials[0]!;
    assert.equal(original.status, 'STARTED');
    const children = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    await host.close();
    assert.throws(() => process.kill(pid, 0), (error: unknown) => (error as NodeJS.ErrnoException).code === 'ESRCH');
    await writeFile(allow, 'resume-original'); host = await openResearch(options);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await host.state(receipt.conversationId)).research!;
    assert.equal(study.trials.length, 1); assert.equal(study.trials[0]!.taskId, original.taskId); assert.equal(study.trials[0]!.status, 'COMPUTED');
    assert.equal(study.trials[0]!.error, null); assert.deepEqual(study.input, input);
    const restored = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    assert.equal(restored.items.length, 1); assert.equal(restored.items[0]!.id, children.items[0]!.id); assert.equal(restored.items[0]!.state.outcome!.status, 'completed');
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 2, 'safe replay runs only the interrupted native computation, not a new trial');
    assert.deepEqual(await host.start('original-interrupted-factor', input), receipt); assert.equal(faux.state.callCount, 1);
  } finally {
    await host.close(); if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous;
    await rm(directory, { recursive: true, force: true });
  }
});
