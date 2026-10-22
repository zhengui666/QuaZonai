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
import { Study } from '../src/documents.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
import { finalizeGroup } from '../plugins/heldout-group.ts';
import { FactorTask, runFactor } from '../plugins/native-factor-task.ts';
import research from '../plugins/research.ts';

const context = BACKGROUND_CONTEXT;
const tool = (name: string, args: JsonObject = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
const request = { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: {} } };
function toolText(transcript: { messages: readonly unknown[] }, name: string): string {
  const result = transcript.messages.filter(item => (item as { role: string; toolName?: string }).role === 'toolResult' && (item as { toolName?: string }).toolName === name).at(-1) as { content: { text?: string }[] };
  return result.content.map(item => item.text ?? '').join('');
}
function textResult(transcript: { messages: readonly unknown[] }, name: string): JsonObject {
  return JSON.parse(toolText(transcript, name)) as JsonObject;
}

test('native exit status persists from the original child to the failed trial and reopen', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-exit-diagnostic-')); const binary = join(directory, 'job');
  const previous = process.env.QZ_JOB_BINARY;
  await writeFile(binary, `#!${process.execPath}\nfor await (const chunk of process.stdin) {}\nprocess.stderr.write('catalog_ensemble: partition=2/909 observations=20 markets=3\\n'); process.exit(137);\n`, { mode: 0o700 });
  process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const data = { catalog: directory, request }; const model = '(module)';
  const compute = defineExtension({ name: 'test.exit-failure', tasks: [FactorTask], tools: [defineTool({
    name: 'compute', description: 'Controlled original process exit', parameters: Type.Object({}), replay: 'safe',
    execute(_args, api, ctx) { return trialFeedback(api, ctx, () => evaluateTrial(api, ctx, { model, hypothesis: 'Original failed process' }, () => runFactor(api, ctx, model, data))); },
  })] });
  const options = { database: join(directory, 'original.sqlite'), models, extensions: [compute] };
  faux.setResponses([tool('compute'), fauxAssistantMessage('Original failure retained.')]);
  let host = await openResearch(options);
  try {
    const input = { goal: 'Protocol-only process exit', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: { max_trials: 1 } };
    const receipt = await host.start('original-process-exit', input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await host.state(receipt.conversationId)).research!;
    assert.equal(study.trials.length, 1); assert.equal(study.trials[0]!.status, 'FAILED');
    const detail = study.trials[0]!.errorDetail as JsonObject;
    assert.equal(detail.exitCode, 137); assert.equal(detail.operation, 'evaluate-factor'); assert.equal(detail.stage, 'execute');
    const children = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    assert.equal(children.items.length, 1);
    const child = children.items[0]!;
    assert.equal(child.state.outcome!.status, 'failed');
    if (child.state.outcome?.status === 'failed') assert.deepEqual(child.state.outcome.error.detail, detail);
    await host.close(); host = await openResearch(options);
    assert.deepEqual((await host.state(receipt.conversationId)).research, study);
    assert.deepEqual(await host.start('original-process-exit', input), receipt);
    assert.equal(faux.state.callCount, 2);
  } finally {
    await host.close(); if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous;
    await rm(directory, { recursive: true, force: true });
  }
});

test('actual WABT parser detail reaches model failure feedback and original-trial reads without another attempt', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-wat-diagnostic-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const model = '(module\n  (func (export "predict") (result f64)\n    unknown_instruction))';
  const data = { catalog: directory, request };
  const compute = defineExtension({ name: 'test.wat-failure', tasks: [FactorTask], tools: [defineTool({
    name: 'compute', description: 'Controlled invalid WAT protocol', parameters: Type.Object({}), replay: 'safe',
    execute(_args, api, ctx) { return trialFeedback(api, ctx, () => evaluateTrial(api, ctx, { model, hypothesis: 'Invalid original model' }, () => runFactor(api, ctx, model, data))); },
  })] });
  const options = { database: join(directory, 'original.sqlite'), models, extensions: [research, compute] };
  let trialId: number; let diagnostic: string; let failureFeedback: string; let readDiagnostic: JsonObject[string] | undefined;
  faux.setResponses([
    tool('compute'),
    transcript => { failureFeedback = toolText(transcript, 'compute'); return tool('research_brief'); },
    transcript => {
      const trial = (textResult(transcript, 'research_brief').trials as JsonObject[])[0]!;
      diagnostic = String(trial.error); trialId = trial.taskId as number; return tool('research_trial', { trialId });
    },
    transcript => { readDiagnostic = textResult(transcript, 'research_trial').error; return tool('research_finish', { reason: 'Stop with the original failed attempt retained.' }); },
  ]);
  let host = await openResearch(options);
  try {
    const input = { goal: 'Protocol-only invalid model', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: { max_trials: 1 } };
    const receipt = await host.start('original-wat-failure', input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await host.state(receipt.conversationId)).research!;
    assert.match(failureFeedback!, /parseWat failed:/); assert.match(failureFeedback!, /factor\.wat:3:\d+: error:/);
    assert.match(diagnostic!, /unknown_instruction/); assert.ok(failureFeedback!.includes(diagnostic!));
    assert.equal(readDiagnostic, diagnostic!);
    assert.equal(study.trials.length, 1); assert.equal(study.trials[0]!.taskId, trialId!); assert.equal(study.trials[0]!.status, 'FAILED');
    assert.equal(study.trials[0]!.error, diagnostic!);
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

test('group finalization retains its failed child decoding metadata and original selections', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-group-decode-')); const binary = join(directory, 'job');
  const previous = process.env.QZ_JOB_BINARY;
  await writeFile(binary, `#!/bin/sh\nprintf '%s' '{"broken":'\n`, { mode: 0o700 });
  process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const data = { catalog: directory, request: { ...request, forecast: { ...request.forecast,
    selection: { ...request.forecast.selection, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] } } } };
  const prepare = defineTool({ name: 'prepare', description: 'Counted test-only selection protocol', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) {
      await evaluateTrial(api, ctx, { model: '(module)', hypothesis: 'Original controlled selection' }, async () => ({ request: iterationRequest(data) }));
      return { content: [{ type: 'text', text: 'Original test-only selection recorded.' }], control: { terminate: true } };
    },
  });
  const options = { database: join(directory, 'original.sqlite'), models,
    extensions: [defineExtension({ name: 'test.group-decode', tasks: [FactorTask], tools: [prepare, finalizeGroup] })] };
  let host = await openResearch(options);
  try {
    const input = { goal: 'Protocol-only grouped final', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { ...data, sealed: data }, policy: { max_trials: 1 } };
    const originals = [];
    for (const id of ['group-first', 'group-second']) {
      faux.setResponses([tool('prepare')]);
      const receipt = await host.start(id, input);
      await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      const study = (await host.state(receipt.conversationId)).research!;
      originals.push({ receipt, study, selection: { researchId: receipt.conversationId, trialId: Number(study.trials[0]!.taskId) } });
    }
    faux.setResponses([tool('factor_finalize_group', { reason: 'Original selections precede the final protocol failure.', selections: originals.map(item => item.selection) })]);
    const conversation = (await host.harness.conversation(originals[0]!.receipt.conversationId, context))!;
    const submission = await conversation.submit({ type: 'input', requestId: 'original-group-final', content: 'Assess the original selected group.' }, context);
    await submission.wait(context);
    const first = (await host.harness.snapshot(Study, conversation.id, context))!;
    const child = await host.harness.waitForTask(first.final!.nativeTaskId as TaskId, context);
    assert.equal(child.state.outcome.status, 'failed');
    if (child.state.outcome.status === 'failed') {
      const detail = child.state.outcome.error.detail as JsonObject;
      assert.equal(detail.code, 'native_result_invalid'); assert.equal(detail.operation, 'evaluate-factor'); assert.equal(detail.stage, 'decode');
      for (const original of originals) {
        const study = (await host.state(original.receipt.conversationId)).research!;
        assert.deepEqual(study.trials, original.study.trials); assert.equal(study.final!.status, 'FAILED');
        assert.equal(study.final!.trialId, original.selection.trialId); assert.equal(study.final!.error, child.state.outcome.error.message);
        assert.deepEqual(study.final!.errorDetail, detail);
      }
    }
    const children = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    assert.equal(children.items.length, 2);
    await host.close(); host = await openResearch(options);
    const restored = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context);
    assert.deepEqual(restored.items, children.items);
    assert.equal((await host.harness.waitForTask(child.id, context)).state.outcome.status, 'failed');
  } finally {
    await host.close(); if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous;
    await rm(directory, { recursive: true, force: true });
  }
});
