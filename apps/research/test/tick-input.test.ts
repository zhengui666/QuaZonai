import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { clipTickRegimeEvaluation, nativeResearchInput, nativeResearchRequest, validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import type { NativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { datasetResearchSettings } from '../../../packages/contracts/src/research.ts';
import { researchInput } from '../src/input.ts';
import { modelBriefInput, modelFeedback } from '../src/feedback.ts';
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

const dayNs = 86_400_000_000_000n;
function regimeRequest(): NativeTickFactorRequest {
  const request = tick(); const { maximum_rows: _rows, ...input } = request.input;
  return { ...request, input: { ...input, representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1', maximum_source_rows: '100',
    event_start_ns: String(31n * dayNs), event_end_ns: String(35n * dayNs), decision_cutoff_ns: String(35n * dayNs) },
    train_end_ns: String(32n * dayNs), validation_end_ns: String(33n * dayNs),
    evaluation_regimes: { kind: 'BTC_COMPLETED_30_DAY_RETURN_V1', days: Array.from({ length: 4 }, (_, index) => {
      const beginning = BigInt(index + 31) * dayNs;
      return { day_start_ns: String(beginning), previous: { event_ns: String(beginning - 1n), price: index === 3 ? '91314159.987654321' : '1.050000000000000000000000000000000001' },
        lookback: index === 0 ? null : { event_ns: String(beginning - 30n * dayNs - 1n), price: '1' } };
    }) } };
}

test('tick regime evaluation binds a complete causal UTC calendar and exact positive decimal marks', () => {
  const request = regimeRequest();
  assert.deepEqual(validateNativeTickFactorRequest(request), request);
  const invalid = (mutate: (value: NativeTickFactorRequest) => void) => {
    const value = structuredClone(request); mutate(value); assert.throws(() => validateNativeTickFactorRequest(value));
  };
  invalid(value => { delete value.input.representation; value.input.maximum_rows = 100; delete value.input.maximum_source_rows; });
  invalid(value => { value.evaluation_regimes!.days.pop(); });
  invalid(value => { value.evaluation_regimes!.days[1]!.day_start_ns = value.evaluation_regimes!.days[0]!.day_start_ns; });
  invalid(value => { value.evaluation_regimes!.days[1]!.previous!.event_ns = value.evaluation_regimes!.days[1]!.day_start_ns; });
  invalid(value => { value.evaluation_regimes!.days[1]!.lookback!.event_ns = String(2n * dayNs); });
  invalid(value => { value.validation_end_ns = String(33n * dayNs - 1n); });
  for (const price of ['0', '-0.01', '1e3', 'NaN', '1'.repeat(513)]) invalid(value => { value.evaluation_regimes!.days[0]!.previous!.price = price; });
  const early = structuredClone(request);
  early.input.event_start_ns = '0'; early.input.event_end_ns = String(4n * dayNs); early.input.decision_cutoff_ns = String(4n * dayNs);
  early.train_end_ns = String(dayNs); early.validation_end_ns = String(2n * dayNs);
  early.evaluation_regimes!.days = Array.from({ length: 4 }, (_, index) => ({ day_start_ns: String(BigInt(index) * dayNs), previous: null, lookback: null }));
  assert.deepEqual(validateNativeTickFactorRequest(early), early);
  early.evaluation_regimes!.days[0]!.previous = { event_ns: '0', price: '1' };
  assert.throws(() => validateNativeTickFactorRequest(early), /mark_clock_invalid/);
});

test('regime iteration and feedback preserve evaluation data without exposing sealed days or changing originals', () => {
  const request = regimeRequest(); const before = structuredClone(request);
  const clipped = iterationRequest(tagged(request));
  assert.deepEqual(clipped, clipTickRegimeEvaluation(request, request.validation_end_ns));
  assert.equal(((clipped.evaluation_regimes as JsonObject).days as unknown[]).length, 2);
  assert.deepEqual(validateNativeTickFactorRequest(clipped), clipped);
  const brief = modelBriefInput({ data: { ...tagged(request), sealed: { ...tagged(request), catalog: 'PRIVATE_HELDOUT' } }, policy: {} });
  assert.equal(JSON.stringify(brief).includes('evaluation_regimes'), false);
  assert.equal(JSON.stringify(brief).includes('91314159.987654321'), false);
  assert.equal(JSON.stringify(brief).includes('PRIVATE_HELDOUT'), false);
  assert.equal(((brief.data.feature_model as JsonObject).features as string[]).length, 8);
  const result: JsonObject = { request, partitions: [{ partition: 'TRAIN' }, { partition: 'VALIDATION' }, { partition: 'HELDOUT', secret: 'OWNER_ONLY' }],
    regime_partitions: [{ partition: 'TRAIN', regimes: [{ regime: 'UNKNOWN', observed_assets: '0', assets: [{ instrument_id: 'OWNER_ASSET_DETAIL' }], metric: { counts: { observations: '0' }, pearson_ic: { value: null, status: 'INSUFFICIENT_SAMPLES' } } }] },
      { partition: 'VALIDATION', regimes: [{ regime: 'BULL' }] }, { partition: 'HELDOUT', secret: 'OWNER_ONLY' }] };
  const original = structuredClone(result); const feedback = modelFeedback(result);
  assert.equal(JSON.stringify(feedback).includes('OWNER_ONLY'), false);
  assert.equal(JSON.stringify(feedback).includes('91314159.987654321'), false);
  assert.equal(JSON.stringify(feedback).includes('OWNER_ASSET_DETAIL'), false);
  const projected = ((feedback.regime_partitions as JsonObject[])[0]!.regimes as JsonObject[])[0]!;
  assert.equal(projected.regime, 'UNKNOWN'); assert.equal(projected.observed_assets, '0');
  assert.deepEqual(projected.metric, (((result.regime_partitions as JsonObject[])[0]!.regimes as JsonObject[])[0]!).metric);
  assert.deepEqual(result, original); assert.deepEqual(request, before);
});

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

test('fixed event-time labels preserve exact duration through clipping and expose a true duration unit', () => {
  const request: NativeTickFactorRequest = { ...tick(), target: { kind: 'FIXED_EVENT_TIME', horizon_ns: '10000000000' } };
  assert.deepEqual(validateNativeTickFactorRequest(request), request);
  assert.deepEqual(iterationRequest(tagged(request)).target, request.target);
  assert.deepEqual(nativeResearchRequest(tagged(request)).target, request.target);
  const settings = datasetResearchSettings(tagged(request));
  assert.equal(settings.horizon, null); assert.equal(settings.horizonNs, '10000000000');
  assert.equal(settings.horizonUnit, 'EVENT_TIME_NS');
  const precise = { ...request, target: { kind: 'FIXED_EVENT_TIME', horizon_ns: '9007199254740993' } };
  assert.deepEqual(validateNativeTickFactorRequest(precise), precise);
  const maximum = { ...request, target: { kind: 'FIXED_EVENT_TIME', horizon_ns: String(9223372036854775807n - 39n) } };
  assert.deepEqual(validateNativeTickFactorRequest(maximum), maximum);
  for (const target of [
    { kind: 'FIXED_EVENT_TIME', horizon_ns: '0' }, { kind: 'FIXED_EVENT_TIME', horizon_ns: 10 },
    { kind: 'FIXED_EVENT_TIME', horizon_ns: '01' }, { kind: 'FIXED_EVENT_TIME', horizon_ns: '1.5' },
    { kind: 'FIXED_EVENT_TIME', horizon_ns: '9223372036854775808' },
    { kind: 'FIXED_EVENT_TIME', horizon_ns: '9223372036854775769' },
    { kind: 'FIXED_EVENT_TIME', horizon_ns: '1', horizon_cohorts: 1 },
    { kind: 'FIXED_TRADE_COHORTS', horizon_cohorts: 1, horizon_ns: '1' },
  ]) assert.throws(() => validateNativeTickFactorRequest({ ...request, target }));
  const original = datasetResearchSettings(tagged());
  assert.equal(original.horizon, 1); assert.equal(original.horizonUnit, 'TRADE_COHORTS');
  assert.equal('horizonNs' in original, false);
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

test('signed-flow cache exposes an explicit eighth feature and preserves old seven-feature briefs', () => {
  const original = tick(); const { maximum_rows: _rows, ...input } = original.input;
  const flow: NativeTickFactorRequest = { ...original, input: { ...input,
    representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1', maximum_source_rows: '1308222597' } };
  assert.deepEqual(validateNativeTickFactorRequest(flow), flow);
  const brief = modelBriefInput({ data: tagged(flow), policy: {} }).data.feature_model as JsonObject;
  assert.equal(brief.schema, 'qz.tick-cohort-flow-features.v1');
  assert.match(String(brief.setter), /mask=0 means an observed finite value/);
  assert.match(String(brief.setter), /mask=1 means NotYetAvailable, 2 Expired, 3 SourceMissing/);
  assert.match(String(brief.prediction), /must return a finite f64/);
  assert.match(String(brief.missing), /skips prediction/);
  assert.equal((brief.features as string[]).length, 8); assert.equal((brief.features as string[])[7], 'signed_quantity');
  assert.match(JSON.stringify(brief.definitions), /unknown aggressors are rejected/);
  assert.match(datasetResearchSettings(tagged(flow)).inputFamily, /signed_quantity/);
  assert.equal(((modelBriefInput({ data: tagged(original), policy: {} }).data.feature_model as JsonObject).features as string[]).length, 7);
  assert.throws(() => validateNativeTickFactorRequest({ ...flow, input: { ...flow.input, maximum_rows: 100 } }), /ambiguous/);
  assert.throws(() => validateNativeTickFactorRequest({ ...flow, input: { ...flow.input, maximum_source_rows: '0' } }), /invalid/);
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
