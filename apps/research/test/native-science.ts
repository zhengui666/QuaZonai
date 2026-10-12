import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, mkdtemp, rm, cp } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { Exposures } from '../plugins/native-exposure.ts';
import type { JsonObject } from '@earendil-works/pi-durable';

const context = BACKGROUND_CONTEXT;
const fixture = JSON.parse(await readFile(process.env.QZ_NATIVE_FIXTURE!, 'utf8')) as JsonObject;
const ema = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) local.get 3 local.get 2 f64.sub f64.const 1000000 f64.mul))';
const tool = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });

function textResult(transcript: { messages: readonly unknown[] }, name: string) {
  const results = transcript.messages.filter((item) => (item as { role: string; toolName?: string }).role === 'toolResult' && (item as { toolName?: string }).toolName === name);
  const latest = results.at(-1) as { content: { type: string; text?: string }[] };
  return JSON.parse(latest.content.filter((item) => item.type === 'text').map((item) => item.text).join('')) as JsonObject;
}

async function setup(extra: string[] = []) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-native-research-'));
  const faux = fauxProvider({ models: [{ id: 'native-science-control' }], tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  const options = { database: join(directory, 'durable.sqlite'), models, modules: ['research', 'factor-wasm', 'heldout-wasm', ...extra].map((name) => resolve('plugins', `${name}.ts`)) };
  return { directory, faux, model, options };
}

function input(model: { provider: string; modelId: string }) {
  return { goal: 'Evaluate factors across two asset classes without targets. Select using validation IC, assess final unseen sample once and stop.', model, data: { ...fixture, sealed: fixture } as JsonObject, policy: { max_trials: 3, selection_metric: 'validation.pearson_ic' } };
}

test('registered factor/evaluator additions run unattended on real multi-asset Nautilus data', { timeout: 120_000 }, async () => {
  const setup_ = await setup(['catalog-input', 'factor-volume', 'validation-compare']);
  const host = await openResearch(setup_.options);
  const finalCalls: JsonObject[] = [];
  setup_.faux.setResponses([
    tool('research_brief', {}),
    (transcript) => { assert.equal('sealed' in (textResult(transcript, 'research_brief').data as JsonObject), false); return tool('catalog_input', {}); },
    (transcript) => { const source = textResult(transcript, 'catalog_input'); assert.equal(source.evidence, 'CONFIGURED_DIRECTORY_ONLY'); assert.equal((source.selection as JsonObject).event_end_ns, (fixture.request as JsonObject).validation_end_ns); return tool('factor_evaluate', { hypothesis: 'Causal price trend: EMA spread predicts future return.', model: ema }); },
    (transcript) => {
      const result = textResult(transcript, 'factor_evaluate');
      assert.equal(result.points, undefined);
      assert.equal(((result.partitions as JsonObject[])[2]!.metric as JsonObject).pearson_ic !== undefined, true);
      assert.equal((((result.partitions as JsonObject[])[2]!.metric as JsonObject).counts as JsonObject).observations, '0');
      return tool('factor_volume', { hypothesis: 'Signed volume deviation predicts future return.', center: 50 });
    },
    (transcript) => { assert.ok(textResult(transcript, 'factor_volume').trialId); return tool('validation_compare', {}); },
    (transcript) => {
      const result = textResult(transcript, 'validation_compare');
      assert.equal((result.compared as JsonObject[]).length, 2);
      assert.equal(typeof result.selectedTrialId, 'number');
      finalCalls.push(result);
      return tool('factor_finalize', { reason: 'Two disclosed structures compared under original criterion; stop before final result can tune another model.', trialId: result.selectedTrialId });
    },
  ]);
  try {
    const original = input(setup_.model);
    const receipt = await host.start('unattended-native-study', original);
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    const view = await host.view(receipt.conversationId);
    const study = view.research!;
    assert.deepEqual(study.input, original);
    assert.deepEqual(study.trials.map((trial) => trial.status), ['COMPUTED', 'COMPUTED']);
    for (const trial of study.trials) {
      const result = trial.result as JsonObject;
      assert.equal((result.native_versions as JsonObject)['nautilus-model'], '0.63.0');
      const validation = (result.partitions as JsonObject[])[1]!;
      assert.equal((validation.assets as JsonObject[]).length, 2);
      assert.equal(((validation.metric as JsonObject).pearson_ic as JsonObject).status, 'AVAILABLE');
      assert.equal('positions' in result || 'targets' in result || 'orders' in result, false);
    }
    assert.equal(study.final!.status, 'COMPUTED');
    assert.equal(study.final!.trialId, finalCalls[0]!.selectedTrialId);
    assert.deepEqual(study.final!.comparedTrials, study.trials.map((trial) => trial.taskId));
    const heldout = ((study.final!.result as JsonObject).partitions as JsonObject[])[2]!;
    assert.notEqual((((heldout.metric as JsonObject).counts as JsonObject).observations), '0');
    assert.equal(setup_.faux.state.callCount, 6);
    const terminalTools = JSON.stringify(view.entries.filter((entry) => JSON.stringify(entry).includes('Research ended.')));
    assert.equal(terminalTools.includes('pearson_ic'), false);
    const exposures = await host.harness.snapshot(Exposures, context);
    assert.deepEqual(exposures!.windows.map((window) => window.kind), ['ITERATION', 'ITERATION', 'FINAL']);
    assert.deepEqual(await host.start('unattended-native-study', original), receipt);
    const conversation = (await host.harness.conversation(receipt.conversationId, context))!;
    const fork = await conversation.fork(view.entries[0]!.id, { ownership: { kind: 'ownerless' } }, context);
    const inherited = (await host.view(fork.id)).research!;
    assert.deepEqual(inherited.trials, study.trials);
    assert.deepEqual(inherited.final, study.final);
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 3);
    assert.equal(exposures!.windows.length, 3);
    await host.close();
    const restored = await openResearch(setup_.options);
    try {
      assert.deepEqual((await restored.view(receipt.conversationId)).research, study);
      setup_.faux.setResponses([tool('factor_evaluate', { hypothesis: 'Attempt to reset exposed holdout with a new root.', model: ema }), tool('research_finish', { reason: 'Known exposure makes independent retuning invalid.' })]);
      const retry = await restored.start('new-root-same-exposed-data', original);
      await (await restored.harness.submission(retry.submissionId, context))!.wait(context);
      const facts = (await restored.view(retry.conversationId)).research!;
      assert.equal(facts.trials[0]!.status, 'FAILED');
      assert.equal(facts.trials[0]!.error, 'heldout_already_exposed');
      assert.equal((await restored.harness.snapshot(Exposures, context))!.windows.length, 3);
    } finally { await restored.close(); }
  } finally { await host.close(); await rm(setup_.directory, { recursive: true, force: true }); }
});

test('pure factor research needs no final sample, qualification or portfolio', { timeout: 60_000 }, async () => {
  const setup_ = await setup();
  setup_.faux.setResponses([tool('factor_evaluate', { hypothesis: 'Causal price spread in an explicitly factor-only study.', model: ema }), tool('research_finish', { reason: 'Factor-only evidence recorded; no targets requested.' })]);
  const host = await openResearch(setup_.options);
  try {
    const original = input(setup_.model); delete (original.data as JsonObject).sealed;
    const receipt = await host.start('pure-factor', original);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.equal(facts.trials[0]!.status, 'COMPUTED');
    assert.equal(facts.final!.status, 'STOPPED');
    assert.equal(facts.final!.result, null);
    assert.deepEqual((await host.harness.snapshot(Exposures, context))!.windows.map((window) => window.kind), ['ITERATION']);
  } finally { await host.close(); await rm(setup_.directory, { recursive: true, force: true }); }
});

test('renamed dataset labels and copied catalogs cannot turn iterative feedback into unseen final evidence after SQLite reopen', { timeout: 120_000 }, async () => {
  const setup_ = await setup();
  const original = input(setup_.model);
  original.data = { ...original.data, datasetId: 'shared-exposure-data', sealed: { ...fixture, datasetId: 'shared-exposure-data' } };
  const broad = structuredClone(original);
  delete (broad.data as JsonObject).sealed;
  const request = objectRequest(broad.data);
  request.validation_end_ns = ((request.forecast as JsonObject).selection as JsonObject).event_end_ns!;
  setup_.faux.setResponses([
    tool('factor_evaluate', { hypothesis: 'Expose the suffix as validation feedback in root B.', model: ema }),
    (transcript) => {
      const result = textResult(transcript, 'factor_evaluate');
      assert.notEqual((((result.partitions as JsonObject[])[1]!.metric as JsonObject).counts as JsonObject).observations, '0');
      return tool('research_finish', { reason: 'Validation feedback is recorded and cannot later be called unseen.' });
    },
  ]);
  const host = await openResearch(setup_.options);
  try {
    const b = await host.start('exposure-root-b', broad);
    await (await host.harness.submission(b.submissionId, context))!.wait(context);
    assert.equal((await host.view(b.conversationId)).research!.trials[0]!.status, 'COMPUTED');
    const copied = join(setup_.directory, 'copied-catalog');
    await cp(fixture.catalog as string, copied, { recursive: true });
    original.data.catalog = copied;
    original.data.datasetId = 'renamed-exposure-data';
    (original.data.sealed as JsonObject).catalog = copied;
    (original.data.sealed as JsonObject).datasetId = 'another-final-label';
    await host.close();
    const restored = await openResearch(setup_.options);
    try {
      setup_.faux.setResponses([
        tool('factor_evaluate', { hypothesis: 'Root A retains its original narrower validation cut.', model: ema }),
        (transcript) => tool('factor_finalize', { reason: 'Attempt to assess the previously exposed suffix.', trialId: textResult(transcript, 'factor_evaluate').trialId }),
      ]);
      const a = await restored.start('exposure-root-a', original);
      await (await restored.harness.submission(a.submissionId, context))!.wait(context);
      const study = (await restored.view(a.conversationId)).research!;
      assert.equal(study.trials[0]!.status, 'COMPUTED');
      assert.equal(study.final!.status, 'FAILED');
      assert.equal(study.final!.error, 'heldout_already_exposed');
      assert.equal(study.final!.result, null);
      const windows = (await restored.harness.snapshot(Exposures, context))!.windows;
      assert.deepEqual(windows.map((window) => window.kind), ['ITERATION', 'ITERATION']);
      assert.equal(windows[0]!.end, request.validation_end_ns);
      assert.equal(windows[1]!.end, objectRequest(original.data).validation_end_ns);
    } finally { await restored.close(); }
  } finally { await host.close(); await rm(setup_.directory, { recursive: true, force: true }); }
});

function objectRequest(data: JsonObject): JsonObject { return data.request as JsonObject; }

for (const fuel of ['omitted', 'null'] as const) {
  test(`${fuel} fuel receives a disclosed finite native budget through iteration and finalization`, { timeout: 120_000 }, async () => {
    const setup_ = await setup();
    const original = input(setup_.model);
    original.data = structuredClone(original.data);
    for (const data of [original.data, original.data.sealed as JsonObject]) {
      const parameters = ((data.request as JsonObject).forecast as JsonObject).parameters as JsonObject;
      if (fuel === 'null') parameters.total_fuel = null;
      else delete parameters.total_fuel;
    }
    setup_.faux.setResponses([
      tool('factor_evaluate', { hypothesis: 'Evaluate with the model-code adapter default finite fuel.', model: ema }),
      (transcript) => {
        const result = textResult(transcript, 'factor_evaluate');
        assert.equal(typeof result.consumed_fuel, 'string');
        assert.equal((((result.request as JsonObject).forecast as JsonObject).parameters as JsonObject).total_fuel, '10000000');
        return tool('factor_finalize', { reason: 'The effective finite request and recorded result match.', trialId: result.trialId });
      },
    ]);
    const host = await openResearch(setup_.options);
    try {
      const receipt = await host.start(`unmetered-${fuel}`, original);
      await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      const study = (await host.view(receipt.conversationId)).research!;
      assert.deepEqual(study.input, original);
      assert.equal(study.trials[0]!.status, 'COMPUTED');
      assert.equal(study.final!.status, 'COMPUTED');
      assert.equal(typeof (study.final!.result as JsonObject).consumed_fuel, 'string');
      const echoed = (((study.final!.result as JsonObject).request as JsonObject).forecast as JsonObject).parameters as JsonObject;
      assert.equal(echoed.total_fuel, '10000000');
    } finally { await host.close(); await rm(setup_.directory, { recursive: true, force: true }); }
  });
}

for (const [name, model] of [
  ['infinite-predict', '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) (loop $again br $again) f64.const 0))'],
  ['oversized-memory', '(module (memory 65536) (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 0))'],
] as const) {
  test(`model-generated ${name} fails finitely and keeps the original failed trial`, { timeout: 30_000 }, async () => {
    const setup_ = await setup();
    const original = input(setup_.model);
    original.data = structuredClone(original.data);
    delete (((original.data.request as JsonObject).forecast as JsonObject).parameters as JsonObject).total_fuel;
    delete original.data.sealed;
    setup_.faux.setResponses([
      tool('factor_evaluate', { hypothesis: 'Protocol-only invalid model: preserve the actual native failure.', model }),
      tool('research_finish', { reason: 'Stop after the recorded failure; no result or qualification claimed.' }),
    ]);
    const host = await openResearch(setup_.options);
    try {
      const receipt = await host.start(name, original);
      await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      const study = (await host.view(receipt.conversationId)).research!;
      assert.deepEqual(study.input, original);
      assert.equal(study.trials.length, 1);
      assert.equal(study.trials[0]!.status, 'FAILED');
      assert.equal(study.trials[0]!.result, null);
      assert.ok(study.trials[0]!.error);
      assert.equal(study.final!.status, 'STOPPED');
      assert.equal((await host.tasks()).tasks.length, 0);
    } finally { await host.close(); await rm(setup_.directory, { recursive: true, force: true }); }
  });
}
