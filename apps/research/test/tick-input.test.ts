import { startLegacyResearch } from './legacy-study.ts';
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
    decision_cutoff_ns: '40', source_revision: 'pinned-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
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
  const request = tick(); const input = request.input;
  return { ...request, input: { ...input, representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1',
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
  invalid(value => { delete value.input.representation; });
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

test('received-time contract binds sampling, label clock, cache layout and a conservative delay bound', () => {
  const request: NativeTickFactorRequest = { ...tick(), input: { ...tick().input,
    availability: 'RECEIVED_TIME_OBSERVED', maximum_receive_delay_ns: '2000000000', representation: 'RECEIVED_TIME_COHORT_FLOW_CACHE_V1' },
    sampling: { kind: 'RECEIVED_TIME_COHORT_V1', maximum_cohort_rows: 100_000 },
    target: { kind: 'FIXED_RECEIVED_TIME', horizon_ns: '9007199254740993' } };
  assert.deepEqual(validateNativeTickFactorRequest(request), request);
  assert.equal(validateNativeTickFactorRequest({ ...request, input: { ...request.input, maximum_receive_delay_ns: '0' } }).input.maximum_receive_delay_ns, '0');
  assert.equal(iterationRequest(tagged(request)).input && (iterationRequest(tagged(request)).input as JsonObject).maximum_receive_delay_ns, '2000000000');
  const brief = modelBriefInput({ data: { ...tagged(request), source_event_start_ns: '7', source_event_end_ns: '55' }, policy: {} });
  const features = brief.data.feature_model as JsonObject;
  assert.match(JSON.stringify(features.definitions), /original collector ts_init analysis clock/);
  assert.match(features.evidence as string, /older reconnect deliveries are excluded and counted/);
  assert.match(features.evidence as string, /negative venue\/collector clock skew is retained/);
  assert.equal(brief.data.source_event_end_ns, '55');
  for (const invalid of [
    { ...request, input: { ...request.input, maximum_receive_delay_ns: undefined } },
    { ...request, input: { ...request.input, maximum_receive_delay_ns: '-1' } },
    { ...request, input: { ...request.input, maximum_receive_delay_ns: '9223372036854775808' } },
    { ...request, input: { ...request.input, representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1' } },
    { ...request, sampling: { ...request.sampling, kind: 'EVENT_TIME_COHORT_V1' } },
    { ...request, target: { ...request.target, kind: 'FIXED_EVENT_TIME' } },
    { ...tick(), target: request.target },
    { ...tick(), input: { ...tick().input, maximum_receive_delay_ns: '0' } },
  ]) assert.throws(() => validateNativeTickFactorRequest(invalid));
});

test('tick requests reject removed row allowances and retain actual scientific parameters', () => {
  const original = tick(); const input = original.input;
  const cache: NativeTickFactorRequest = { ...original, input: { ...input, representation: 'EVENT_TIME_COHORT_CACHE_V1' } };
  assert.deepEqual(validateNativeTickFactorRequest(cache), cache);
  for (const field of [{ maximum_rows: 0 }, { maximum_rows: 1_000_001 }, { maximum_source_rows: '0' }, { maximum_source_rows: '1308222597' }]) {
    assert.throws(() => validateNativeTickFactorRequest({ ...cache, input: { ...cache.input, ...field } }));
  }
  assert.deepEqual(validateNativeTickFactorRequest({ ...original, parameters: { ...original.parameters, total_fuel: '0' } }).input, input);
  const clipped = iterationRequest({ catalog: '/controlled/cache-view', ...tagged(cache) });
  assert.equal((clipped.input as JsonObject).event_end_ns, original.validation_end_ns);
  assert.equal(Object.hasOwn(clipped.input as JsonObject, 'maximum_source_rows'), false);
});

test('signed-flow cache exposes an explicit eighth feature and preserves old seven-feature briefs', () => {
  const original = tick(); const input = original.input;
  const flow: NativeTickFactorRequest = { ...original, input: { ...input,
    representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1' } };
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
  assert.throws(() => validateNativeTickFactorRequest({ ...flow, input: { ...flow.input, maximum_rows: 100 } }));
  assert.throws(() => validateNativeTickFactorRequest({ ...flow, input: { ...flow.input, maximum_source_rows: '0' } }));
});

test('book profile appends four received-flow features and preserves legacy profiles', () => {
  const request: NativeTickFactorRequest = { ...tick(), input: { ...tick().input,
    availability: 'RECEIVED_TIME_OBSERVED', maximum_receive_delay_ns: '2000000000', representation: 'RECEIVED_TIME_COHORT_FLOW_CACHE_V1' },
    sampling: { kind: 'RECEIVED_TIME_COHORT_V1', maximum_cohort_rows: 100_000 } };
  const before = structuredClone(request);
  const legacy = modelBriefInput({ data: tagged(request), policy: {} }).data.feature_model as JsonObject;
  const book: NativeTickFactorRequest = { ...request, parameters: { ...request.parameters, feature_profile: 'RECEIVED_FLOW_BOOK_V1' } };
  assert.deepEqual(validateNativeTickFactorRequest(book), book);
  assert.deepEqual(iterationRequest(tagged(book)).parameters, book.parameters);
  const brief = modelBriefInput({ data: tagged(book), policy: {} }).data.feature_model as JsonObject;
  assert.equal(brief.abi, legacy.abi);
  assert.equal(brief.schema, 'qz.tick-received-cohort-flow-book-features.v1');
  assert.deepEqual(brief.features, [...legacy.features as string[], 'native_book_imbalance_ratio', 'book_spread_bps', 'book_bid_depth_5_quote', 'book_ask_depth_5_quote']);
  assert.match(JSON.stringify(brief.definitions), /min\(size\)\/max\(size\)/);
  assert.match(JSON.stringify(brief.definitions), /original availability timestamp/);
  assert.match(JSON.stringify(brief.definitions), /BookLevel::exposure.*quote units/);
  assert.match(JSON.stringify(brief.definitions), /trade-cohort-triggered/);
  assert.match(String(brief.missing), /one-sided book snapshots use the existing feature masks/);
  for (const original of [tick(), { ...tick(), input: { ...tick().input, representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1' as const } },
    { ...request, input: { ...request.input, representation: 'RECEIVED_TIME_COHORT_CACHE_V1' as const } }, request]) {
    const explicit: NativeTickFactorRequest = { ...original, parameters: { ...original.parameters, feature_profile: 'RECEIVED_FLOW_V1' } };
    assert.deepEqual(validateNativeTickFactorRequest(explicit), explicit);
    assert.deepEqual(modelBriefInput({ data: tagged(explicit), policy: {} }).data.feature_model,
      modelBriefInput({ data: tagged(original), policy: {} }).data.feature_model);
  }
  for (const invalid of [
    { ...book, parameters: { ...book.parameters, feature_profile: 'UNKNOWN' } },
    { ...book, parameters: { ...book.parameters, feature_profile: null } },
    { ...book, input: { ...book.input, representation: undefined } },
    { ...book, input: { ...book.input, representation: 'RECEIVED_TIME_COHORT_CACHE_V1' } },
    { ...tick(), parameters: book.parameters },
  ]) assert.throws(() => validateNativeTickFactorRequest(invalid), /native_tick_(feature_profile_unsupported|book_features_require_received_flow)/);
  assert.deepEqual(request, before);
  assert.equal((legacy.features as string[]).length, 8);
  assert.equal(Object.hasOwn(validateNativeTickFactorRequest(request).parameters, 'feature_profile'), false);
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
    data: { catalog, ...tagged(tick('A.SIM', '20')), sealed: { catalog, ...tagged(tick()) } }, policy: { } };
  let host = await openResearch(options);
  try {
    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Protocol only', model }), async () => {
      const trialId = (await host.harness.snapshot(Exposures, context))!.windows.at(-1)!.taskId;
      return tool('factor_finalize', { trialId, reason: 'Preselect before owner-only final result' });
    }]);
    const first = await startLegacyResearch(host, 'tick-protocol', input);
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
    assert.deepEqual(await startLegacyResearch(host, 'tick-protocol', input), first);
    assert.deepEqual((await host.state(first.conversationId)).research, original);
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 2);

    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Reuse tick history for BAR development', model: legacyModel }), async () => {
      const trialId = (await host.harness.snapshot(Exposures, context))!.windows.at(-1)!.taskId;
      return tool('factor_finalize', { trialId, reason: 'Previously exposed tick history cannot be independent BAR evidence' });
    }]);
    const second = await startLegacyResearch(host, 'copied-bar', { ...input, data: { catalog: copied, request: bar(), sealed: { catalog: copied, request: bar() } } });
    await (await host.harness.submission(second.submissionId, context))!.wait(context);
    const rejected = (await host.state(second.conversationId)).research!;
    assert.equal(rejected.trials[0]!.status, 'COMPUTED');
    assert.equal(rejected.final!.error, 'heldout_already_exposed');
    assert.equal(rejected.final!.status, 'FAILED');
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 3);

    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Original BAR feedback', model: legacyModel }), tool('research_finish', { reason: 'End BAR protocol' })]);
    const third = await startLegacyResearch(host, 'bar-first', { ...input, data: { catalog, request: bar('B.SIM', '40') } });
    await (await host.harness.submission(third.submissionId, context))!.wait(context);
    assert.equal((await host.state(third.conversationId)).research!.trials[0]!.status, 'COMPUTED');
    faux.setResponses([tool('factor_evaluate', { hypothesis: 'Tick suffix was already exposed by BAR feedback', model }), async () => {
      const trialId = (await host.harness.snapshot(Exposures, context))!.windows.at(-1)!.taskId;
      return tool('factor_finalize', { trialId, reason: 'Must fail because BAR validation already exposed the final suffix' });
    }]);
    const fourth = await startLegacyResearch(host, 'tick-after-bar', { ...input, data: { catalog: copied, ...tagged(tick('B.SIM', '20')), sealed: { catalog: copied, ...tagged(tick('B.SIM')) } } });
    await (await host.harness.submission(fourth.submissionId, context))!.wait(context);
    const fourthState = (await host.state(fourth.conversationId)).research!;
    // Prior iterative feedback is permitted for another iterative prefix, but it
    // cannot be represented as a fresh independent suffix at final reservation.
    assert.equal(fourthState.trials[0]!.status, 'COMPUTED');
    assert.equal(fourthState.final!.status, 'FAILED');
    assert.equal(fourthState.final!.error, 'heldout_already_exposed');

    const beforeUnsupported = (await readFile(calls, 'utf8')).trim().split('\n').length;
    faux.setResponses([tool('factor_volume', { hypothesis: 'BAR volume is not implicitly cohort quantity', center: 0 }), tool('research_finish', { reason: 'Unsupported factor remains a disclosed attempt' })]);
    const fifth = await startLegacyResearch(host, 'tick-volume-rejected', { ...input, data: { catalog, ...tagged(tick('C.SIM', '20')) } });
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

test('full-market tick feedback keeps coverage totals and availability without repeated source reports or asset rows', () => {
  const coverage: JsonObject = { modality: 'TRADE_TICK', source_revision: 'original-source', trade_count: '1308222597',
    cohort_count: '614393279', first_event_ns: '1', last_event_ns: '9007199254740993', availability: 'EVENT_TIME_PROXY_UNVERIFIED',
    instrument_ids: Array.from({ length: 523 }, (_, i) => `A${i}.SIM`),
    instrument_coverage: Array.from({ length: 523 }, (_, i) => ({ instrument_id: `A${i}.SIM`, trade_count: '1', cohort_count: '1', first_event_ns: null, last_event_ns: null })),
    limitations: ['UNQUALIFIED_EXPLORATORY', 'VWAP_IS_NOT_EXECUTABLE_PRICE'],
    source_evidence: { representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1', source_report: {
      instruments: Array.from({ length: 523 }, () => ({ definition: 'SOURCE_REPORT_OWNER_ONLY'.repeat(100) })) } } };
  for (const regime of [false, true]) {
    const result: JsonObject = { coverage, partitions: [], ...(regime ? { regime_partitions: [] } : {}) };
    const original = structuredClone(result); const feedback = modelFeedback(result); const projected = feedback.coverage as JsonObject;
    assert.equal(projected.trade_count, '1308222597'); assert.equal(projected.cohort_count, '614393279');
    assert.equal(projected.last_event_ns, '9007199254740993'); assert.equal(projected.instrument_count, 523);
    assert.equal(projected.availability, 'EVENT_TIME_PROXY_UNVERIFIED'); assert.deepEqual(projected.limitations, coverage.limitations);
    assert.deepEqual(projected.source_evidence, { representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1' });
    assert.equal('instrument_ids' in projected, false); assert.equal('instrument_coverage' in projected, false);
    assert.ok(JSON.stringify(feedback).length < 2000); assert.deepEqual(result, original);
  }
});
