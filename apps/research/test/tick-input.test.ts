import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { nativeResearchInput, nativeResearchRequest, validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import type { NativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { researchInput } from '../src/input.ts';
import { modelBriefInput } from '../src/feedback.ts';
import { evaluateWasm, iterationRequest, normalizedRequest } from '../plugins/native-wasm.ts';
import { openResearch } from '../src/host.ts';
import { Exposures } from '../plugins/native-exposure.ts';
import research from '../plugins/research.ts';
import factor from '../plugins/factor-wasm.ts';
import volume from '../plugins/factor-volume.ts';
import heldout from '../plugins/heldout-wasm.ts';

const context = BACKGROUND_CONTEXT;
const tick = (instrument = 'A.SIM', end = '40'): NativeTickFactorRequest => ({ schema_version: 1,
  input: { modality: 'TRADE_TICK', instrument_ids: [instrument], event_start_ns: '0', event_end_ns: end,
    decision_cutoff_ns: '40', maximum_rows: 1_000_000, source_revision: 'pinned-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
  sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100_000 },
  target: { kind: 'FIXED_TRADE_COHORTS', horizon_cohorts: 1 }, parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 },
  train_end_ns: '10', validation_end_ns: '20' });
const tagged = (request = tick()): JsonObject => ({ nativeInput: { modality: 'TRADE_TICK', request } });
const bar = (instrument = 'A.SIM', validation = '20'): JsonObject => ({ schema_version: 1, train_end_ns: '10', validation_end_ns: validation,
  forecast: { schema_version: 1, parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1 },
    selection: { schema_version: 1, bar_types: [`${instrument}-1-MINUTE-LAST-EXTERNAL`], event_start_ns: '0', event_end_ns: '40', decision_cutoff_ns: '40', maximum_rows: 100 } } });
const model = '(module (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)) (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) f64.const 1))';
const legacyModel = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';
const tool = (name: string, args: JsonObject = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });

test('explicit tick contract preserves nanosecond strings and rejects incompatible or mixed modalities', () => {
  const request = tick();
  request.input.decision_cutoff_ns = '9007199254740993';
  assert.deepEqual(validateNativeTickFactorRequest(request), request);
  assert.notEqual(validateNativeTickFactorRequest(request), request);
  assert.equal(validateNativeTickFactorRequest({ ...request, input: { ...request.input, instrument_ids: Array.from({ length: 257 }, (_, index) => `A${index}.SIM`) } }).input.instrument_ids.length, 257);
  for (const invalid of [
    { ...request, schema_version: '1' }, { ...request, hidden: true },
    { ...request, input: { ...request.input, availability: 'VERIFIED' } },
    { ...request, input: { ...request.input, instrument_ids: ['A.SIM', 'A.SIM'] } },
    { ...request, input: { ...request.input, event_end_ns: '20', decision_cutoff_ns: '19' } },
    { ...request, sampling: { ...request.sampling, maximum_cohort_rows: 100_001 } },
    { ...request, target: { kind: 'FIXED_BARS', horizon_cohorts: 1 } },
    { ...request, parameters: { ...request.parameters, total_fuel: '0' } },
  ]) assert.throws(() => validateNativeTickFactorRequest(invalid));
  assert.throws(() => nativeResearchInput({ ...tagged(), request: bar() }), /ambiguous_factor_request/);
  assert.throws(() => nativeResearchInput({ nativeInput: { modality: 'BAR', request: tick() } }), /native_bar_forecast_required/);
  const original = { goal: 'Tick study', model: { provider: 'test', modelId: 'test' }, data: tagged(), policy: {} };
  assert.deepEqual(researchInput(original), original);
  assert.throws(() => researchInput({ ...original, policy: { strategy: { kind: 'factor-threshold' } } }), /tick_factor_only_required/);
  assert.throws(() => researchInput({ ...original, data: { ...tagged(), sealed: { request: bar() } } }), /native_research_modality_mismatch/);
});

test('derived cohort cache has an exact full-source budget without changing the native tick row limit', () => {
  const original = tick(); const { maximum_rows: _rows, ...input } = original.input;
  const cache: NativeTickFactorRequest = { ...original, input: { ...input, representation: 'EVENT_TIME_COHORT_CACHE_V1', maximum_source_rows: '1308222597' } };
  assert.deepEqual(validateNativeTickFactorRequest(cache), cache);
  const clipped = iterationRequest({ catalog: '/controlled/cache-view', ...tagged(cache) });
  assert.equal((clipped.input as JsonObject).maximum_source_rows, '1308222597');
  assert.equal((clipped.input as JsonObject).event_end_ns, original.validation_end_ns);
  for (const invalid of [
    { ...cache, input: { ...cache.input, maximum_rows: 1_000_000 } },
    { ...cache, input: { ...cache.input, maximum_source_rows: 1308222597 } },
    { ...cache, input: { ...cache.input, maximum_source_rows: '0' } },
    { ...cache, input: { ...cache.input, maximum_source_rows: '9223372036854775808' } },
    { ...original, input: { ...original.input, maximum_rows: 1_000_001 } },
    { ...original, input: { ...original.input, maximum_source_rows: '100' } },
  ]) assert.throws(() => validateNativeTickFactorRequest(invalid));
});

test('tagged BAR keeps the legacy native request; tick clipping retains semantics and hides sealed data', () => {
  const request = bar();
  const legacy = { request };
  const explicit = { nativeInput: { modality: 'BAR', request } };
  assert.deepEqual(normalizedRequest(legacy), normalizedRequest(explicit));
  assert.deepEqual(iterationRequest(legacy), iterationRequest(explicit));
  const data = { ...tagged(), sealed: { catalog: 'private-heldout', ...tagged() } };
  const clipped = iterationRequest(data);
  assert.equal((clipped.input as JsonObject).event_end_ns, '20');
  assert.equal((clipped.target as JsonObject).kind, 'FIXED_TRADE_COHORTS');
  assert.deepEqual(nativeResearchRequest(data), tick());
  assert.equal((iterationRequest(data, true).input as JsonObject).decision_cutoff_ns, '20');
  const brief = modelBriefInput({ data, policy: {} });
  assert.equal('sealed' in brief.data, false);
  assert.equal(JSON.stringify(brief).includes('private-heldout'), false);
  assert.match(String((brief.data.feature_model as JsonObject).evidence), /UNVERIFIED/);
});

test('tick native echo must bind the exact original source and request', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-tick-echo-'));
  const binary = join(directory, 'job.mjs');
  try {
    await writeFile(binary, `#!${process.execPath}\nlet input=''; for await (const chunk of process.stdin) input+=chunk; const request=JSON.parse(input); request.input.source_revision='different-source'; console.log(JSON.stringify({request}));\n`, { mode: 0o700 });
    await assert.rejects(evaluateWasm(model, { catalog: directory, ...tagged() }, context, undefined, binary), /native_result_request_mismatch/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

// Controlled child protocol verifies durable isolation and lineage, not numerical/PIT evidence.
test('tick studies retain restart identity, final isolation and exposure across copied BAR catalogs', { timeout: 30_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-tick-durable-'));
  const catalog = join(directory, 'catalog'); const copied = join(directory, 'copied');
  await mkdir(catalog); await mkdir(copied);
  const binary = join(directory, 'job.mjs'); const calls = join(directory, 'calls.jsonl');
  await writeFile(binary, `#!${process.execPath}
import {appendFileSync} from 'node:fs';
let input=''; for await (const chunk of process.stdin) input+=chunk;
const request=JSON.parse(input); const operation=process.argv[2];
appendFileSync(${JSON.stringify(calls)}, JSON.stringify({operation,request})+'\\n');
console.log(JSON.stringify({request, ...(request.input?.event_end_ns==='40'?{owner_only:'NEVER_RETURN_FINAL'}:{}), test_only:'CONTROLLED_PROTOCOL_NOT_NUMERICAL_EVIDENCE'}));
`, { mode: 0o700 });
  const previous = process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const ownerModel = { provider: faux.provider.id, modelId: faux.getModel().id };
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [research, factor, volume, heldout] };
  const input = { goal: 'Tick protocol without scientific qualification', model: ownerModel,
    data: { catalog, ...tagged(tick('A.SIM', '20')), sealed: { catalog, ...tagged(tick()) } }, policy: { max_trials: 2 } };
  let host = await openResearch(options);
  try {
    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Protocol only', model }), async () => {
      const trialId = (await host.harness.snapshot(Exposures, context))!.windows.at(-1)!.taskId;
      return tool('factor_finalize', { trialId, reason: 'Preselect before owner-only final result' });
    }]);
    const first = await host.start('tick-protocol', input);
    await (await host.harness.submission(first.submissionId, context))!.wait(context);
    const original = (await host.state(first.conversationId)).research!;
    assert.equal(original.trials[0]!.status, 'COMPUTED', JSON.stringify(original.trials));
    assert.equal(original.final!.status, 'COMPUTED', JSON.stringify(original.final));
    assert.deepEqual(original.input, input);
    const requests = (await readFile(calls, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(requests.map(item => [item.operation, item.request.input.event_end_ns]), [['evaluate-tick-factor', '20'], ['evaluate-tick-factor', '40']]);
    const entries = JSON.stringify((await host.view(first.conversationId)).entries);
    assert.equal(entries.includes('final tuning feedback is returned'), true);
    assert.equal(entries.includes('NEVER_RETURN_FINAL'), false);
    assert.equal((original.final!.result as JsonObject).owner_only, 'NEVER_RETURN_FINAL');
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('tick-protocol', input), first);
    assert.deepEqual((await host.state(first.conversationId)).research, original);
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 2);

    faux.setResponses([tool('factor_evaluate', { hypothesis: 'A copied BAR path cannot reset tick history', model: legacyModel }), tool('research_finish', { reason: 'Failed attempt remains disclosed' })]);
    const second = await host.start('copied-bar', { ...input, data: { catalog: copied, request: bar() } });
    await (await host.harness.submission(second.submissionId, context))!.wait(context);
    const rejected = (await host.state(second.conversationId)).research!;
    assert.equal(rejected.trials[0]!.status, 'FAILED');
    assert.equal(rejected.trials[0]!.error, 'heldout_already_exposed');
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 2);

    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Original BAR feedback', model: legacyModel }), tool('research_finish', { reason: 'End BAR protocol' })]);
    const third = await host.start('bar-first', { ...input, data: { catalog, request: bar('B.SIM', '40') } });
    await (await host.harness.submission(third.submissionId, context))!.wait(context);
    assert.equal((await host.state(third.conversationId)).research!.trials[0]!.status, 'COMPUTED');
    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Tick suffix was already exposed by BAR feedback', model }), async () => {
      const trialId = (await host.harness.snapshot(Exposures, context))!.windows.at(-1)!.taskId;
      return tool('factor_finalize', { trialId, reason: 'Must fail because BAR validation already exposed the final suffix' });
    }]);
    const fourth = await host.start('tick-after-bar', { ...input, data: { catalog: copied, ...tagged(tick('B.SIM', '20')), sealed: { catalog: copied, ...tagged(tick('B.SIM')) } } });
    await (await host.harness.submission(fourth.submissionId, context))!.wait(context);
    const fourthState = (await host.state(fourth.conversationId)).research!;
    // Prior iterative feedback is permitted for another iterative prefix, but it
    // cannot be represented as a fresh independent suffix at final reservation.
    assert.equal(fourthState.trials[0]!.status, 'COMPUTED');
    assert.equal(fourthState.final!.status, 'FAILED');
    assert.equal(fourthState.final!.error, 'heldout_already_exposed');

    const beforeUnsupported = (await readFile(calls, 'utf8')).trim().split('\n').length;
    faux.setResponses([tool('factor_volume', { hypothesis: 'BAR volume is not implicitly cohort quantity', center: 0 }), tool('research_finish', { reason: 'Unsupported factor remains a disclosed attempt' })]);
    const fifth = await host.start('tick-volume-rejected', { ...input, data: { catalog, ...tagged(tick('C.SIM', '20')) } });
    await (await host.harness.submission(fifth.submissionId, context))!.wait(context);
    const fifthState = (await host.state(fifth.conversationId)).research!;
    assert.equal(fifthState.trials[0]!.status, 'FAILED');
    assert.equal(fifthState.trials[0]!.error, 'tick_factor_volume_unsupported');
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, beforeUnsupported);
  } finally {
    await host.close();
    if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous;
    await rm(directory, { recursive: true, force: true });
  }
});
