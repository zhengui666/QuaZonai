import assert from 'node:assert/strict';
import { constants } from 'node:buffer';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxAssistantMessage, fauxProvider, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import research from '../plugins/research.ts';
import strategy from '../plugins/strategy-portfolio.ts';
import { Exposures } from '../plugins/native-exposure.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
import { resolveStrategyPolicy, simulationSelection, thresholdTargets } from '../src/strategy.ts';
import { object } from '../src/trials.ts';
import { readNativeEvidence } from '../plugins/native-evidence.ts';

const context = BACKGROUND_CONTEXT;
const tool = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });

// One actual Harness trial; the temporary process speaks only the native protocol.
// No market data, Wasm execution, numerical blend, paid model or performance claim.
test('PortfolioEvaluationTask persists complete 64-market native evidence beyond MAX_STRING_LENGTH', { concurrency: false }, async (t) => {
  const instrumentIds = Array.from({ length: 64 }, (_, index) => `${String(index).padStart(77, '0')}.SIM`);
  const allocations = instrumentIds.map((instrumentId) => ({ instrumentId, weight: '0.015625' }));
  const members = [{ model: '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const -1))', weight: '1' }];
  const originalPolicy = { name: 'synthetic-persistence-only', kind: 'cross-market-threshold', instrumentIds, threshold: 0,
    investedWeight: '1', targetTtlNs: '1', forecastMode: 'price-volume', evaluationPartition: 'HELDOUT',
    simulationSettings: { base_currency: 'USD', starting_capital: '3000', fee_rates: instrumentIds.map((instrument_id) => ({ instrument_id, maker: '0', taker: '0' })) } };
  const policy = resolveStrategyPolicy({ allocations }, originalPolicy);
  const firstClock = 1782864000000000001n;
  const observation = (index: number) => ({ instrument_id: instrumentIds[0]!, event_ns: String(firstClock + BigInt(index)),
    available_ns: String(firstClock + BigInt(index)), forecast: -1, label_return: null, test_raw_marker: 'RAW_FORECAST_LABELS_SYNTHETIC_ONLY' });
  const sample = thresholdTargets({ points: [observation(0), observation(1)] }, policy, 'USD');
  const frameLength = JSON.stringify(sample[0]).length;
  assert.equal(JSON.stringify(sample[1]).length, frameLength);
  assert.equal((sample[0]!.targets as JsonObject[]).length, 64);
  // The complete vector occurs in components.target_points, rows, blend and
  // simulationRequest. Each native message has only ONE copy, well below V8's limit.
  const copies = 4;
  const count = Math.floor(constants.MAX_STRING_LENGTH / (copies * (frameLength + 1))) + 2;
  const vectorArrayLength = count * (frameLength + 1) + 1;
  const evidenceLowerBound = copies * vectorArrayLength;
  assert.ok(evidenceLowerBound > constants.MAX_STRING_LENGTH);
  assert.ok(vectorArrayLength < constants.MAX_STRING_LENGTH);
  t.diagnostic(JSON.stringify({ markets: instrumentIds.length, observedClocks: count, frameLength, vectorArrayLength,
    evidenceCopies: copies, evidenceLowerBound, MAX_STRING_LENGTH: constants.MAX_STRING_LENGTH }));

  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-large-native-evidence-'));
  const previousBinary = process.env.QZ_JOB_BINARY;
  let host: Awaited<ReturnType<typeof openResearch>> | undefined;
  try {
    const catalog = join(directory, 'prefix'); const sealed = join(directory, 'sealed');
    await mkdir(catalog); await mkdir(sealed);
    const request = { schema_version: 1, train_end_ns: String(firstClock - 1n), validation_end_ns: String(firstClock + BigInt(count)),
      forecast: { schema_version: 1, selection: { schema_version: 1, bar_types: instrumentIds.map((id) => `${id}-1-MINUTE-LAST-EXTERNAL`),
        event_start_ns: '0', event_end_ns: String(firstClock + BigInt(count) + 100n), decision_cutoff_ns: String(firstClock + BigInt(count) + 100n), maximum_rows: count },
      parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: null } } };
    const faux = fauxProvider({ tokensPerSecond: 100_000, models: [{ id: 'native-evidence-size-test-only', contextWindow: 1_000_000 }] }); const models = createModels(); models.setProvider(faux.provider);
    const input = { goal: 'Synthetic native-evidence persistence regression only; no scientific or market-performance claim.',
      model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog, request, sealed: { catalog: sealed, request } },
      policy: { max_trials: 1, strategy: originalPolicy } };
    const clippedRequest = iterationRequest(input.data, true);
    const selection = simulationSelection(clippedRequest, 'VALIDATION', policy);
    const log = join(directory, 'calls.jsonl'); const binary = join(directory, 'job.mjs');
    const config = { count, firstClock: String(firstClock), instrumentIds, catalog, log, request: clippedRequest, selection, settings: policy.simulationSettings };
    await writeFile(binary, String.raw`#!${process.execPath}
import assert from 'node:assert/strict';
import { constants } from 'node:buffer';
import { appendFileSync, readFileSync } from 'node:fs';
const config = ${JSON.stringify(config)};
const [command, ...args] = process.argv.slice(2);
const raw = readFileSync(0, 'utf8');
const inputBytes = Buffer.byteLength(raw);
assert.ok(inputBytes < constants.MAX_STRING_LENGTH, 'native input must fit one V8 string');
const input = JSON.parse(raw);
function validatePoints(points) {
  assert.equal(points.length, config.count);
  for (const [index, point] of points.entries()) {
    const clock = BigInt(config.firstClock) + BigInt(index);
    assert.equal(point.schema_version, 1);
    assert.equal(point.asof_ns, String(clock));
    assert.equal(point.valid_until_ns, String(clock + 1n));
    assert.equal(point.cash_weight, '1.000000000000000000');
    assert.deepEqual(point.rebalance_instrument_ids, [config.instrumentIds[0]], 'unobserved positions must remain untouched');
    assert.equal(point.targets.length, config.instrumentIds.length);
    for (const [asset, target] of point.targets.entries()) {
      assert.deepEqual(target, { instrument_id: config.instrumentIds[asset], currency: 'USD', weight: '0.000000000000000000' });
    }
  }
}
let result;
if (command === 'evaluate-factor') {
  assert.ok(args.includes('--summary-only'));
  assert.ok(args.includes('--model')); // Compilation is exercised by TS; never run Wasm here.
  assert.equal(args[args.indexOf('--catalog') + 1], config.catalog);
  assert.deepEqual(input, config.request);
  result = { request: input, native_versions: { protocol: 'SYNTHETIC_ONLY_NO_SCIENCE' }, consumed_fuel: null,
    partitions: ['TRAIN', 'VALIDATION', 'HELDOUT'].map(partition => ({ partition, metric: {
      counts: { observations: partition === 'HELDOUT' ? '0' : String(config.count), included: partition === 'HELDOUT' ? '0' : String(config.count),
        excluded: '0', indicator_warmup: '0', incomplete_labels: '0', cross_boundary_labels: '0', prediction_outside_partition: '0' },
      pearson_ic: { value: null, status: 'SYNTHETIC_ONLY_NO_SCIENCE' } } })) };
} else if (command === 'forecast-factor') {
  assert.ok(args.includes('--model'));
  assert.equal(args[args.indexOf('--catalog') + 1], config.catalog);
  assert.deepEqual(input, config.request.forecast);
  result = { native_versions: { protocol: 'SYNTHETIC_ONLY_NO_SCIENCE' }, points: Array.from({ length: config.count }, (_, index) => {
    const at = String(BigInt(config.firstClock) + BigInt(index));
    return { instrument_id: config.instrumentIds[0], event_ns: at, available_ns: at, forecast: -1,
      label_return: null, test_raw_marker: 'RAW_FORECAST_LABELS_SYNTHETIC_ONLY' };
  }) };
} else if (command === 'blend-targets') {
  assert.deepEqual(input.weights, ['1']);
  assert.equal(input.schema_version, 1);
  const points = input.rows.map(row => { assert.equal(row.length, 1); return row[0]; });
  validatePoints(points);
  result = { target_points: points, protocol: 'SYNTHETIC_ONLY_NOT_A_NUMERICAL_BLEND' };
} else if (command === 'simulate') {
  assert.equal(args[args.indexOf('--catalog') + 1], config.catalog, 'never read the sealed binding');
  assert.equal(input.schema_version, 1);
  assert.deepEqual(input.selection, config.selection);
  assert.deepEqual(input.settings, config.settings);
  validatePoints(input.target_points);
  result = { native_version: 'SYNTHETIC_ONLY_NO_SCIENCE', iterations: '1', events: String(config.count), orders: '0', positions: '0',
    consumed_target_points: String(input.target_points.length),
    statistics: [{ native_key: 'Sharpe Ratio (252 days)', value: null, reason_code: 'SYNTHETIC_ONLY_NO_SCIENCE' }],
    summary: { protocol: 'SYNTHETIC_VALIDATION_ONLY_NO_SCIENCE' }, returns: [],
    canonical_result: { portfolio_snapshots: [{ ts_event: config.selection.event_start_ns, total_equity: ['3000 USD'] }] } };
} else throw new Error('unsupported_synthetic_command:' + command);
const output = JSON.stringify(result);
const outputBytes = Buffer.byteLength(output);
assert.ok(outputBytes < constants.MAX_STRING_LENGTH, 'native stdout must fit one V8 string');
await new Promise((resolve, reject) => process.stdout.write(output, error => error ? reject(error) : resolve()));
appendFileSync(config.log, JSON.stringify({ command, inputBytes, outputBytes, completed: true }) + '\n');
`, { mode: 0o700 });
    process.env.QZ_JOB_BINARY = binary;
    faux.setResponses([
      tool('strategy_portfolio_evaluate', { hypothesis: 'Preserve the complete original 64-market account evidence.', members, allocations }),
      tool('research_finish', { reason: 'The one synthetic regression trial exhausted its original budget; no final evaluation or science claim.' }),
    ]);
    const options = { database: join(directory, 'research.sqlite'), models, extensions: [research, strategy] };
    host = await openResearch(options);
    const receipt = await host.start('synthetic-large-native-evidence', input);
    const submission = await host.harness.submission(receipt.submissionId, context);
    assert.equal((await submission!.wait(context)).status, 'done');
    const view = await host.view(receipt.conversationId); const facts = view.research!;
    const calls = (await readFile(log, 'utf8')).trim().split('\n').map((line) => JSON.parse(line) as JsonObject);
    assert.deepEqual(calls.map((call) => call.command), ['evaluate-factor', 'forecast-factor', 'blend-targets', 'simulate']);
    for (const call of calls) {
      assert.equal(call.completed, true);
      assert.ok((call.inputBytes as number) < constants.MAX_STRING_LENGTH);
      assert.ok((call.outputBytes as number) < constants.MAX_STRING_LENGTH);
      t.diagnostic(JSON.stringify(call));
    }
    const children = await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.strategy-portfolio.evaluate' }, 2), context);
    assert.equal(children.items.length, 1);
    const child = await host.harness.waitForTask(children.items[0]!.id, context);
    t.diagnostic(JSON.stringify({ trialStatuses: facts.trials.map((trial) => trial.status), trialErrors: facts.trials.map((trial) => trial.error),
      nativeChildStatus: child.state.outcome.status, nativeChildError: 'error' in child.state.outcome ? child.state.outcome.error : null }));
    assert.deepEqual(facts.input, input);
    assert.deepEqual(facts.trials.map((trial) => trial.status), ['COMPUTED'], JSON.stringify(facts.trials.map((trial) => ({ status: trial.status, error: trial.error }))));
    assert.equal(child.state.outcome.status, 'completed');
    assert.ok(child.owner);
    const proposal = object(facts.trials[0]!.proposal, 'proposal');
    assert.deepEqual(proposal, { hypothesis: 'Preserve the complete original 64-market account evidence.', members, allocations, request, strategy: originalPolicy });
    const reference = object(object(child.state.outcome.status === 'completed' ? child.state.outcome.result : null, 'native_result').native, 'native_evidence');
    assert.equal(reference.format, 'qz.native-evidence/gzip-json/v1');
    const native = await readNativeEvidence(host.harness, child.id, reference, context);
    assert.deepEqual(native.members, members);
    assert.deepEqual(object(native.policy, 'native_policy').allocations, allocations);
    const component = (native.components as JsonObject[])[0]!;
    const forecasts = object(component.forecasts, 'forecasts').points as JsonObject[];
    assert.equal(forecasts.length, count);
    assert.deepEqual(forecasts[0], observation(0)); assert.deepEqual(forecasts.at(-1), observation(count - 1));
    const rows = native.rows as JsonObject[][];
    assert.equal(rows.length, count);
    assert.ok(rows.every((row) => row.length === 1));
    for (const points of [component.target_points, rows.map((row) => row[0]!), object(native.blend, 'blend').target_points,
      object(native.simulationRequest, 'simulation_request').target_points] as JsonObject[][]) {
      assert.equal(points.length, count);
      assert.deepEqual(points[0], sample[0]);
      assert.deepEqual(points.at(-1), { ...sample[0], asof_ns: String(firstClock + BigInt(count - 1)), valid_until_ns: String(firstClock + BigInt(count)) });
    }
    assert.equal(object(native.simulation, 'simulation').consumed_target_points, String(count));
    assert.equal(object(facts.trials[0]!.result, 'trial_result').evidenceTaskId, child.id);
    assert.equal(facts.final!.status, 'STOPPED'); assert.equal(facts.final!.result, null);
    assert.deepEqual((await host.harness.snapshot(Exposures, context))!.windows.map((window) => window.kind), ['ITERATION']);
    assert.equal(faux.state.callCount, 2);
    const modelFacing = JSON.stringify(view.entries);
    for (const marker of ['RAW_FORECAST_LABELS_SYNTHETIC_ONLY', 'canonical_result', 'HELDOUT_OWNER_ONLY', 'target_points']) assert.equal(modelFacing.includes(marker), false, marker);
    await host.close();
    host = await openResearch(options);
    assert.deepEqual((await host.view(receipt.conversationId)).research, facts);
    const reopened = await host.harness.waitForTask(child.id, context);
    assert.equal(reopened.state.outcome.status, 'completed');
    const reopenedReference = object(object(reopened.state.outcome.status === 'completed' ? reopened.state.outcome.result : null, 'native_result').native, 'native_evidence');
    assert.deepEqual(reopenedReference, reference);
    assert.deepEqual(await readNativeEvidence(host.harness, child.id, reopenedReference, context), native);
    assert.equal(faux.state.callCount, 2, 'Reopening evidence must not recompute the candidate');
  } finally {
    try { await host?.close(); }
    finally {
      if (previousBinary === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previousBinary;
      await rm(directory, { recursive: true, force: true });
    }
  }
});
