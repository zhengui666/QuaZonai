import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import research from '../plugins/research.ts';
import factor from '../plugins/factor-wasm.ts';
import fitted from '../plugins/fitted-ensemble.ts';
import portfolio from '../plugins/strategy-portfolio.ts';
import { object } from '../src/trials.ts';
import { readNativeEvidence } from '../plugins/native-evidence.ts';

const context = BACKGROUND_CONTEXT;
const tool = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function last(transcript: { messages: readonly unknown[] }, name: string): JsonObject {
  const message = transcript.messages.filter(item => (item as { toolName?: string }).toolName === name).at(-1) as { content: { text?: string }[] };
  return JSON.parse(message.content.map(item => item.text ?? '').join('')) as JsonObject;
}
for (const spot of [false, true]) test(`native fitted ${spot ? 'spot restart bridge' : 'account'} replay preserves original calibration, dataset identity and separate attempts`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-fitted-wire-')); const log = join(directory, 'calls.jsonl'); const binary = join(directory, 'job');
  const previous = process.env.QZ_JOB_BINARY;
  const pauseFile = join(directory, 'pause-simulation'); if (spot) await writeFile(pauseFile, 'controlled process checkpoint');
  await writeFile(binary, `#!/bin/sh\nQZ_TEST_SIMULATE_PAUSE_FILE='${pauseFile}' exec '${process.execPath}' '${new URL('./fitted-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
  process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const model = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 99))';
  const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1, selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30' }, parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1 } } };
  const input = { goal: 'Protocol-only fitted ensemble; no alpha claim', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, request, sealed: { catalog: directory, request } },
    policy: { max_trials: 4, strategy: { name: 'protocol-policy', kind: 'factor-threshold', instrumentId: 'TEST.SIM', threshold: 0, investedWeight: '0.5', targetTtlNs: '5', forecastMode: 'price-volume', simulationSettings: { base_currency: 'USD', starting_capital: '3000', ...(spot ? { account_kind: 'CASH', multi_currency_spot_cash: { schema_version: 1, mode: 'MULTI_CURRENCY_CASH', report_currency: 'USD' } } : {}) } } } };
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [research, factor, fitted, portfolio] };
  let host = await openResearch(options); const sources: number[] = [];
  faux.setResponses([
    tool('factor_evaluate', { hypothesis: 'Original model A', model }),
    transcript => { sources.push(last(transcript, 'factor_evaluate').trialId as number); return tool('factor_evaluate', { hypothesis: 'Original model B', model: model.replace('99', '98') }); },
    transcript => { sources.push(last(transcript, 'factor_evaluate').trialId as number); return tool('strategy_ensemble_fit', { hypothesis: 'TRAIN-only calibrated fit', sourceTrialIds: sources }); },
    transcript => { const fit = last(transcript, 'strategy_ensemble_fit'); assert.equal(fit.kind, 'ENSEMBLE_FIT'); return tool('strategy_ensemble_evaluate', { hypothesis: 'One joint shared account', fitTrialId: fit.trialId! }); },
    transcript => tool('strategy_portfolio_finalize', { trialId: last(transcript, 'strategy_ensemble_evaluate').trialId!, reason: 'Protocol-only independent replay with the frozen native fit' }),
  ]);
  try {
    const receipt = await host.start('fitted-original', input);
    if (spot) {
      const deadline = Date.now() + 10_000;
      while (!(await readFile(log, 'utf8').catch(() => '')).includes('"command":"simulate"')) { assert.ok(Date.now() < deadline, 'original replay reached native boundary'); await delay(15); }
      await host.close(); await rm(pauseFile); host = await openResearch(options);
    }
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context); await host.harness.waitForIdle(context);
    const state = await host.state(receipt.conversationId); const study = state.research!;
    assert.deepEqual(study.trials.map(trial => trial.status), ['COMPUTED', 'COMPUTED', 'COMPUTED', 'COMPUTED'], JSON.stringify(study));
    assert.equal(study.final!.status, 'COMPUTED', JSON.stringify(study.final));
    const result = object(study.trials[3]!.result, 'joint-result');
    const child = await host.harness.getTask(result.evidenceTaskId as TaskId<JsonObject>, context);
    assert.equal(child!.state.outcome!.status, 'completed');
    const native = await readNativeEvidence(host.harness, receipt.conversationId, child!.id, object(child!.state.outcome!.status === 'completed' ? child!.state.outcome!.result.native : null, 'native'), context);
    assert.ok(object(native.fitRequest, 'fit-request').members); assert.equal(object(native.catalogPrefix, 'original-prefix').method, 'NAUTILUS_SELECTED_PREFIX_SHA256_V2'); assert.equal(object(native.fit, 'fit').fit_end_ns, '10');
    const calls = (await readFile(log, 'utf8')).trim().split('\n').map(line => JSON.parse(line) as { command: string; input: JsonObject; args: string[] });
    assert.equal(calls.filter(call => call.command === 'fit-ensemble').length, 1); assert.equal(calls.filter(call => call.command === 'apply-ensemble').length, spot ? 3 : 2);
    const simulations = calls.filter(call => call.command === 'simulate'); assert.equal(simulations.length, spot ? 3 : 2);
    assert.ok(calls.filter(call => call.command === 'apply-ensemble').every(call => JSON.stringify(call.input.fit) === JSON.stringify(native.fit)));
    if (spot) {
      const bridge = object(native.datasetBridge, 'dataset_bridge'); assert.equal(bridge.kind, 'RESEARCH_DATASET_REVISION_BRIDGE');
      const portfolio = object(result.portfolio, 'spot_report_summary');
      assert.equal(portfolio.report_currency, 'USD'); assert.equal(portfolio.returns_kind, 'REPORT_CURRENCY_DAILY'); assert.equal(portfolio.returns_status, 'OK');
      assert.deepEqual(portfolio.statistics, [{ native_key: 'Calendar Geometric Daily Return', value: 0.01, status: 'AVAILABLE' }], 'use native report-currency statistics, not unavailable top-level analyzer statistics');
      const id = (call: typeof simulations[number]) => call.args[call.args.indexOf('--dataset-revision-id') + 1];
      assert.equal(id(simulations[0]!), bridge.id); assert.equal(id(simulations[1]!), bridge.id, 'recovery reuses the committed bridge UUID');
      assert.notEqual(id(simulations[2]!), bridge.id, 'sealed replay has its own scoped bridge artifact');
      assert.deepEqual(object(simulations[1]!.input.settings, 'settings').multi_currency_spot_cash, input.policy.strategy.simulationSettings.multi_currency_spot_cash);
    }
    assert.equal(JSON.stringify((await host.view(receipt.conversationId)).entries).includes('RAW_TRAIN_PANEL_NOT_MODEL_VISIBLE'), false);
    const count = calls.length; await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('fitted-original', input), receipt); assert.equal((await readFile(log, 'utf8')).trim().split('\n').length, count);
  } finally { await host.close(); if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await rm(directory, { recursive: true, force: true }); }
});

test('a changed native prefix cannot produce a fit or replay under the original evidence identity', { timeout: 10_000 }, async () => {
  const { fitNativeEnsemble, replayFittedEnsemble } = await import('../plugins/native-fitted-portfolio.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-fitted-prefix-')); const log = join(directory, 'calls.jsonl');
  const prefixFile = join(directory, 'controlled-prefix'); const binary = join(directory, 'job');
  const model = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 99))';
  const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
    selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30' },
    parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1 } } };
  const input = { data: { catalog: directory, request }, members: [{ factorVersionId: 'a', model, sourceTrialId: 1 }, { factorVersionId: 'b', model, sourceTrialId: 2 }], binary, evidenceIdentity: 'native-task:controlled:original' };
  const configure = async (mutate = false) => writeFile(binary, `#!/bin/sh\nQZ_TEST_PREFIX_FILE='${prefixFile}' QZ_TEST_PREFIX_MUTATE_DURING_FIT='${mutate ? '1' : ''}' exec '${process.execPath}' '${new URL('./fitted-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
  try {
    await writeFile(prefixFile, 'original complete prefix'); await configure(true);
    await assert.rejects(fitNativeEnsemble(input, context), /catalog_prefix_changed_during_fit/);
    let calls = (await readFile(log, 'utf8')).trim().split('\n').map(line => JSON.parse(line) as { command: string });
    assert.equal(calls.filter(call => call.command === 'fit-ensemble').length, 0, 'changed panel source must not be fitted');
    await configure(); await writeFile(prefixFile, 'original complete prefix');
    const fit = await fitNativeEnsemble(input, context);
    assert.equal(object(fit.evidence.catalogPrefix, 'original-prefix').method, 'NAUTILUS_SELECTED_PREFIX_SHA256_V2');
    await writeFile(prefixFile, 'changed historical rows');
    await assert.rejects(replayFittedEnsemble({ ...input, fit: fit.evidence.fit,
      catalogPrefix: fit.evidence.catalogPrefix, partition: 'VALIDATION', policy: { name: 'wire-policy', kind: 'factor-threshold', instrumentId: 'TEST.SIM', threshold: 0,
        investedWeight: '0.5', targetTtlNs: '5', forecastMode: 'price-volume', simulationSettings: { base_currency: 'USD', starting_capital: '3000' } } }, context), /ensemble_replay_original_catalog_prefix_mismatch/);
    calls = (await readFile(log, 'utf8')).trim().split('\n').map(line => JSON.parse(line) as { command: string });
    assert.equal(calls.filter(call => call.command === 'simulate' || call.command === 'apply-ensemble').length, 0, 'invalid original source is rejected before replay science');
  } finally { await rm(directory, { recursive: true, force: true }); }
});
