import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import research from '../plugins/research.ts';
import strategy from '../plugins/strategy-portfolio.ts';
import { Exposures } from '../plugins/native-exposure.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
import { exactWeights, intervalTargets, replayCatalog, resolveStrategyPolicy, simulationSelection, strategyMembers, thresholdPolicy, thresholdTargets } from '../src/strategy.ts';
import { object } from '../src/trials.ts';

const context = BACKGROUND_CONTEXT;
const positive = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';
const members = [{ model: positive, weight: '0.2500000000000000001' }, { model: positive.replace('f64.const 1', 'f64.const -1'), weight: '0.7499999999999999999' }];
const tool = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function textResult(transcript: { messages: readonly unknown[] }, name: string) {
  const latest = transcript.messages.filter((item) => (item as { role: string; toolName?: string }).role === 'toolResult' && (item as { toolName?: string }).toolName === name).at(-1) as { content: { type: string; text?: string }[] };
  return JSON.parse(latest.content.filter((item) => item.type === 'text').map((item) => item.text).join('')) as JsonObject;
}

async function setup(universe = false) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-ensemble-protocol-'));
  const catalog = join(directory, 'prefix'); const sealed = join(directory, 'sealed');
  await mkdir(catalog); await mkdir(sealed);
  await writeFile(join(catalog, 'binding.json'), JSON.stringify({ name: 'PRIMARY', end: '20' }));
  await writeFile(join(sealed, 'binding.json'), JSON.stringify({ name: 'SEALED', end: '30' }));
  const binary = join(directory, 'job'); const log = join(directory, 'calls.jsonl');
  await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('./portfolio-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
  const previous = process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY = binary;
  process.env.QZ_TEST_PROVIDER_SECRET = 'test-only-isolation-marker';
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
    selection: { schema_version: 1, bar_types: universe ? ['BASE.SIM-1-MINUTE-LAST-EXTERNAL', 'TEST.SIM-1-MINUTE-LAST-EXTERNAL'] : ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '20', decision_cutoff_ns: '20', maximum_rows: 50 },
    parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: null } } };
  const finalRequest = structuredClone(request); Object.assign(finalRequest.forecast.selection, { event_end_ns: '30', decision_cutoff_ns: '40' });
  const input = { goal: 'Protocol-only ensemble selection; no scientific or market performance claim.', model: { provider: faux.provider.id, modelId: faux.getModel().id },
    data: { catalog, request, sealed: { catalog: sealed, request: finalRequest } }, policy: { max_trials: 2, strategy: { name: 'original-policy', kind: 'factor-threshold', instrumentId: universe ? 'BASE.SIM' : 'TEST.SIM', ...(universe ? { instrumentIds: ['BASE.SIM', 'TEST.SIM'] } : {}), threshold: 0,
      investedWeight: '0.25', targetTtlNs: '5', forecastMode: 'price-volume', evaluationPartition: 'HELDOUT', simulationSettings: { base_currency: 'USD', starting_capital: '3000', original_policy_marker: 'TEST_ONLY',
        ...(universe ? { fee_rates: [{ instrument_id: 'BASE.SIM', maker: '0.004000', taker: '0.006000' }, { instrument_id: 'TEST.SIM', maker: '0.001234567890123456', taker: '0.009876543210987654' }] } : {}) } } } };
  const options = { database: join(directory, 'original.sqlite'), models, extensions: [research, strategy] };
  return { directory, input, options, faux, log, async calls() { return (await readFile(log, 'utf8')).trim().split('\n').map((line) => JSON.parse(line) as JsonObject); },
    async cleanup() { if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; delete process.env.QZ_TEST_PROVIDER_SECRET; await rm(directory, { recursive: true, force: true }); } };
}

test('one ensemble trial retains members/weights, validation-only account feedback and independent owner-only final evidence', async () => {
  const state = await setup(); const host = await openResearch(state.options);
  state.faux.setResponses([
    tool('strategy_portfolio_evaluate', { hypothesis: 'Disclosed invalid WAT consumes an attempt.', members: [{ model: 'invalid wat', weight: '1' }] }),
    tool('strategy_portfolio_evaluate', { hypothesis: 'One complete original ensemble, not two free member probes.', members }),
    (transcript) => {
      const result = textResult(transcript, 'strategy_portfolio_evaluate');
      assert.equal(object(result.portfolio, 'portfolio').partition, 'VALIDATION');
      assert.deepEqual((result.components as JsonObject[]).map((component) => object(component.summary, 'summary').diagnostic_prediction), [1, -1]);
      assert.deepEqual(object(result.request, 'request'), iterationRequest(state.input.data, true));
      assert.equal(JSON.stringify(result).includes('RAW_FORECAST'), false);
      assert.equal(JSON.stringify(result).includes('canonical_result'), false);
      assert.equal(JSON.stringify(result).includes('HELDOUT_OWNER_ONLY'), false);
      return tool('research_brief', {});
    },
    (transcript) => {
      const brief = textResult(transcript, 'research_brief');
      assert.equal('sealed' in object(brief.data, 'data'), false);
      assert.equal(JSON.stringify(brief).includes('RAW_FORECAST'), false);
      const trials = brief.trials as JsonObject[];
      return tool('strategy_portfolio_finalize', { trialId: trials[1]!.taskId, reason: 'Select before independent exposure; never tune against final feedback.' });
    },
  ]);
  try {
    const receipt = await host.start('ensemble', state.input);
    assert.equal((await (await host.harness.submission(receipt.submissionId, context))!.wait(context)).status, 'done');
    const view = await host.view(receipt.conversationId); const facts = view.research!;
    assert.deepEqual(facts.input, state.input);
    assert.deepEqual(facts.trials.map((trial) => trial.status), ['FAILED', 'COMPUTED']);
    assert.deepEqual(object(facts.trials[1]!.proposal, 'proposal').members, members);
    assert.deepEqual(object(facts.trials[1]!.proposal, 'proposal').request, state.input.data.request);
    assert.deepEqual(facts.trials[1]!.proposal, { hypothesis: 'One complete original ensemble, not two free member probes.', members, request: state.input.data.request, strategy: state.input.policy.strategy });
    assert.equal(facts.final!.status, 'COMPUTED', JSON.stringify(facts.final));
    assert.deepEqual(facts.final!.comparedTrials, facts.trials.map((trial) => trial.taskId));
    const final = object(facts.final!.result, 'final');
    assert.equal(object(final.portfolio, 'portfolio').partition, 'HELDOUT');
    assert.equal(object(final.partitions_component, 'component').index, 0);
    assert.equal(object(object((final.partitions as JsonObject[])[2]!.metric, 'metric').counts, 'counts').included, '2');
    assert.notEqual(object(facts.trials[1]!.result, 'iteration').evidenceTaskId, final.evidenceTaskId);
    const child = await host.harness.waitForTask(final.evidenceTaskId as TaskId<JsonObject>, context);
    assert.equal(child.state.outcome.status, 'completed');
    const evidence = object(child.state.outcome.status === 'completed' ? child.state.outcome.result.native : null, 'native_evidence');
    assert.deepEqual(evidence.members, members);
    assert.equal(object(object(evidence.simulationRequest, 'request').selection, 'selection').event_start_ns, '20');
    assert.ok(JSON.stringify(evidence).includes('RAW_FORECAST_AND_LABELS_PROTOCOL_ONLY'));
    const calls = await state.calls();
    assert.equal(calls.filter((call) => call.command === 'simulate').length, 2);
    assert.deepEqual(calls.filter((call) => call.command === 'forecast-factor').map((call) => call.prediction), [1, -1, 1, -1]);
    assert.deepEqual(calls.filter((call) => call.command === 'blend-targets').map((call) => object(call.input, 'input').weights), [members.map((member) => member.weight), members.map((member) => member.weight)]);
    const simulations = calls.filter((call) => call.command === 'simulate');
    assert.deepEqual(simulations.map((call) => object(call.binding, 'binding').name), ['PRIMARY', 'SEALED']);
    assert.deepEqual(simulations.map((call) => (object(call.input, 'input').target_points as JsonObject[]).map((point) => point.asof_ns)), [['10', '12', '19'], ['20', '23', '29']]);
    assert.deepEqual((await host.harness.snapshot(Exposures, context))!.windows.map((window) => window.kind), ['ITERATION', 'ITERATION', 'FINAL']);
    const children = await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.strategy-portfolio.evaluate' }, 100), context);
    assert.equal(children.items.length, 3); assert.ok(children.items.every((child) => child.owner));
    assert.equal(state.faux.state.callCount, 4);
    assert.equal(JSON.stringify(view.entries).includes('HELDOUT_OWNER_ONLY_PROTOCOL'), false);
    assert.equal(JSON.stringify(view.entries).includes('canonical_result'), false);
    await host.close();
    const restored = await openResearch(state.options);
    try {
      assert.deepEqual((await restored.view(receipt.conversationId)).research, facts);
      state.faux.setResponses([tool('strategy_portfolio_evaluate', { hypothesis: 'No exposure reset through a new root.', members }), tool('research_finish', { reason: 'Previously exposed final is not unseen.' })]);
      const reset = await restored.start('not-a-fresh-exposure', state.input);
      await (await restored.harness.submission(reset.submissionId, context))!.wait(context);
      assert.equal((await restored.view(reset.conversationId)).research!.trials[0]!.error, 'heldout_already_exposed');
      assert.equal((await state.calls()).length, calls.length);
    } finally { await restored.close(); }
  } finally { await host.close(); await state.cleanup(); }
});

test('invalid ensemble weights consume the existing budget and rejected proposals never compute', async () => {
  const state = await setup(); state.input.policy.max_trials = 1;
  state.faux.setResponses([
    tool('strategy_portfolio_evaluate', { hypothesis: 'Invalid complete proposal.', members: [{ model: positive, weight: '0.7' }] }),
    tool('strategy_portfolio_evaluate', { hypothesis: 'Not a free retry.', members }),
  ]);
  const host = await openResearch(state.options);
  try {
    const receipt = await host.start('budget', state.input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.deepEqual(facts.trials.map((trial) => trial.status), ['FAILED', 'REJECTED']);
    assert.equal(facts.trials[0]!.error, 'portfolio_weights_invalid');
    assert.equal(facts.final!.reason, 'trial_budget_exhausted');
    assert.equal((await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.strategy-portfolio.evaluate' }, 100), context)).items.length, 0);
    await assert.rejects(readFile(state.log), { code: 'ENOENT' });
  } finally { await host.close(); await state.cleanup(); }
});

test('an allowed non-base instrument stays in the original proposal and resolved owner-only final evidence', async () => {
  const state = await setup(true); const host = await openResearch(state.options);
  state.faux.setResponses([
    tool('strategy_portfolio_evaluate', { hypothesis: 'Choose only from the original fixed pool.', members, instrumentId: 'TEST.SIM' }),
    (transcript) => {
      const result = textResult(transcript, 'strategy_portfolio_evaluate');
      assert.deepEqual(result.request, iterationRequest(state.input.data, true));
      assert.equal(JSON.stringify(result).includes('RAW_FORECAST'), false);
      assert.equal(JSON.stringify(result).includes('canonical_result'), false);
      return tool('strategy_portfolio_finalize', { trialId: result.trialId, reason: 'Retain the original selected instrument.' });
    },
  ]);
  try {
    const receipt = await host.start('selected-instrument', state.input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.deepEqual(facts.input, state.input);
    assert.equal(facts.trials[0]!.status, 'COMPUTED', JSON.stringify(facts.trials));
    assert.deepEqual(facts.trials[0]!.proposal, { hypothesis: 'Choose only from the original fixed pool.', members, instrumentId: 'TEST.SIM', request: state.input.data.request, strategy: state.input.policy.strategy });
    assert.equal(facts.final!.status, 'COMPUTED', JSON.stringify(facts.final));
    for (const result of [object(facts.trials[0]!.result, 'trial_result'), object(facts.final!.result, 'final_result')]) {
      const child = await host.harness.waitForTask(result.evidenceTaskId as TaskId<JsonObject>, context);
      assert.equal(child.state.outcome.status, 'completed');
      const native = object(child.state.outcome.status === 'completed' ? child.state.outcome.result.native : null, 'native_evidence');
      const settings = state.input.policy.strategy.simulationSettings;
      const selectedSettings = { ...settings, fee_rates: settings.fee_rates!.filter((rate) => rate.instrument_id === 'TEST.SIM') };
      assert.deepEqual(native.policy, { ...state.input.policy.strategy, instrumentId: 'TEST.SIM', simulationSettings: selectedSettings });
      assert.deepEqual(native.members, members);
      const simulation = object(native.simulationRequest, 'simulation_request');
      assert.deepEqual(object(simulation.selection, 'selection').bar_types, ['TEST.SIM-1-MINUTE-LAST-EXTERNAL']);
      assert.deepEqual(simulation.settings, selectedSettings);
      assert.ok((simulation.target_points as JsonObject[]).every((point) => (point.targets as JsonObject[]).every((target) => target.instrument_id === 'TEST.SIM')));
    }
    const calls = await state.calls();
    for (const call of calls.filter((call) => ['evaluate-factor', 'forecast-factor'].includes(String(call.command)))) {
      const request = object(call.input, 'native_input');
      const selection = object(call.command === 'evaluate-factor' ? object(request.forecast, 'forecast').selection : request.selection, 'selection');
      assert.deepEqual(selection.bar_types, state.input.data.request.forecast.selection.bar_types);
      assert.equal(selection.event_start_ns, '0');
    }
    assert.deepEqual(calls.filter((call) => call.command === 'simulate').map((call) => object(call.binding, 'binding').name), ['PRIMARY', 'SEALED']);
    assert.equal(state.input.policy.strategy.instrumentId, 'BASE.SIM');
    assert.deepEqual((await host.harness.snapshot(Exposures, context))!.windows.map((window) => window.kind), ['ITERATION', 'FINAL']);
  } finally { await host.close(); await state.cleanup(); }
});

for (const universe of [false, true]) test(`outside ${universe ? 'declared universe' : 'legacy base instrument'} durably fails and consumes the original budget`, async () => {
  const state = await setup(universe); state.input.policy.max_trials = 1;
  state.faux.setResponses([
    tool('strategy_portfolio_evaluate', { hypothesis: 'Not an admitted instrument.', members, instrumentId: 'OUTSIDE.SIM' }),
    tool('strategy_portfolio_evaluate', { hypothesis: 'No free instrument retry.', members, instrumentId: 'TEST.SIM' }),
  ]);
  const host = await openResearch(state.options);
  try {
    const receipt = await host.start(`invalid-instrument-${universe}`, state.input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.deepEqual(facts.trials.map((trial) => trial.status), ['FAILED', 'REJECTED']);
    assert.equal(facts.trials[0]!.error, 'strategy_instrument_not_allowed');
    assert.equal(object(facts.trials[0]!.proposal, 'proposal').instrumentId, 'OUTSIDE.SIM');
    assert.deepEqual(object(facts.trials[0]!.proposal, 'proposal').strategy, state.input.policy.strategy);
    assert.equal(facts.final!.reason, 'trial_budget_exhausted');
    assert.equal((await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.strategy-portfolio.evaluate' }, 100), context)).items.length, 0);
    await assert.rejects(readFile(state.log), { code: 'ENOENT' });
    await host.close();
    const restored = await openResearch(state.options);
    try { assert.deepEqual((await restored.view(receipt.conversationId)).research, facts); }
    finally { await restored.close(); }
  } finally { await host.close(); await state.cleanup(); }
});

for (const explicit of [false, true]) for (const duplicate of [false, true]) test(`${explicit ? 'explicit choice' : 'implicit base'} with ${duplicate ? 'duplicate' : 'missing'} original fee entries durably consumes an attempt`, async () => {
  const state = await setup(true); state.input.policy.max_trials = 1;
  const selected = explicit ? 'TEST.SIM' : 'BASE.SIM';
  const settings = state.input.policy.strategy.simulationSettings;
  const fee = settings.fee_rates!.find((rate) => rate.instrument_id === selected)!;
  settings.fee_rates = duplicate ? [...settings.fee_rates!, { ...fee }] : settings.fee_rates!.filter((rate) => rate.instrument_id !== selected);
  state.faux.setResponses([
    tool('strategy_portfolio_evaluate', { hypothesis: 'Exact original reserve required.', members, ...(explicit ? { instrumentId: selected } : {}) }),
    tool('strategy_portfolio_evaluate', { hypothesis: 'No free reserve retry.', members }),
  ]);
  const host = await openResearch(state.options);
  try {
    const receipt = await host.start(`invalid-fees-${explicit}-${duplicate}`, state.input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.deepEqual(facts.trials.map((trial) => trial.status), ['FAILED', 'REJECTED']);
    assert.equal(facts.trials[0]!.error, duplicate ? 'strategy_instrument_fee_duplicate' : 'strategy_instrument_fee_missing');
    assert.deepEqual(object(facts.trials[0]!.proposal, 'proposal').strategy, state.input.policy.strategy);
    assert.deepEqual(facts.input, state.input);
    assert.equal(facts.final!.reason, 'trial_budget_exhausted');
    assert.equal((await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.strategy-portfolio.evaluate' }, 100), context)).items.length, 0);
    await assert.rejects(readFile(state.log), { code: 'ENOENT' });
  } finally { await host.close(); await state.cleanup(); }
});

test('universe resolution selects exactly one unchanged original fee reserve for explicit and implicit instruments', () => {
  const fees = [{ instrument_id: 'BASE.SIM', maker: '0.004000', taker: '0.006000' },
    { instrument_id: 'ALT-YES.SIM', maker: '0.001234567890123456', taker: '0.009876543210987654' }];
  const settings = { base_currency: 'pUSD', starting_capital: '3000.000000000000000001', fee_rates: fees, leverage: '1',
    fee_model: { adapter_kind: 'POLYMARKET_V2_RECORDED_FEES', parameters: { original_source: 'unchanged' } },
    latency_model: { parameters: { insert_latency_ns: '1000000' } }, exposure_tolerance: '0.000001' };
  const original = { name: 'original', kind: 'factor-threshold', instrumentId: 'BASE.SIM', instrumentIds: ['BASE.SIM', 'ALT-YES.SIM'], simulationSettings: settings };
  const before = structuredClone(original);
  const proposals: JsonObject[] = [{}, { instrumentId: 'BASE.SIM' }, { instrumentId: 'ALT-YES.SIM' }];
  for (const proposal of proposals) {
    const selected = proposal.instrumentId ?? original.instrumentId;
    const resolved = resolveStrategyPolicy(proposal, original);
    assert.deepEqual(resolved, { ...original, instrumentId: selected, simulationSettings: { ...settings, fee_rates: fees.filter((fee) => fee.instrument_id === selected) } });
    assert.notEqual(resolved.simulationSettings, settings);
    assert.notEqual(resolved.simulationSettings.fee_rates, fees);
    assert.deepEqual(original, before);
  }
  for (const fee_rates of [null, [], [fees[0]!], [{ ...fees[1]!, instrument_id: 'ALT-YES.SIM-OTHER' }]]) {
    assert.throws(() => resolveStrategyPolicy({ instrumentId: 'ALT-YES.SIM' }, { ...original, simulationSettings: { ...settings, fee_rates } }), /strategy_instrument_fee_missing/);
  }
  assert.throws(() => resolveStrategyPolicy({}, { ...original, simulationSettings: { ...settings, fee_rates: [fees[0]!, fees[0]!] } }), /strategy_instrument_fee_duplicate/);
  const { instrumentIds: _universe, ...legacy } = original;
  assert.equal(resolveStrategyPolicy({}, legacy), legacy);
  assert.equal(resolveStrategyPolicy({ instrumentId: 'BASE.SIM' }, legacy).simulationSettings, settings);
});

test('fixed-universe validation, exact BAR matching and legacy absence never rewrite the original policy/request', () => {
  const legacy: JsonObject = { name: 'original', kind: 'factor-threshold', instrumentId: 'BASE.SIM', threshold: 0, investedWeight: '1', targetTtlNs: '5', simulationSettings: { base_currency: 'USD' } };
  assert.equal(resolveStrategyPolicy({}, legacy), legacy);
  assert.deepEqual(resolveStrategyPolicy({ instrumentId: 'BASE.SIM' }, legacy), legacy);
  assert.throws(() => resolveStrategyPolicy({ instrumentId: 'ALT-YES.SIM' }, legacy), /strategy_instrument_not_allowed/);
  const original: JsonObject = { ...legacy, instrumentIds: ['BASE.SIM', 'ALT-YES.SIM'], simulationSettings: { ...object(legacy.simulationSettings, 'settings'),
    fee_rates: [{ instrument_id: 'BASE.SIM', maker: '0.004', taker: '0.006' }, { instrument_id: 'ALT-YES.SIM', maker: '0.001', taker: '0.002' }] } };
  for (const ids of [null, 'BASE.SIM', [], ['BASE.SIM', 'BASE.SIM'], ['BASE.SIM', ''], ['BASE.SIM', '  '], ['BASE.SIM', 2]]) {
    assert.throws(() => thresholdPolicy({ ...original, instrumentIds: ids }), /strategy_instrument_universe_invalid/);
  }
  assert.throws(() => thresholdPolicy({ ...original, instrumentIds: ['ALT-YES.SIM'] }), /strategy_base_instrument_not_allowed/);
  for (const instrumentId of ['', '  ', 2, null]) assert.throws(() => resolveStrategyPolicy({ instrumentId }, original), /strategy_instrument_invalid/);
  const policy = resolveStrategyPolicy({ instrumentId: 'ALT-YES.SIM' }, original);
  assert.deepEqual(policy, { ...original, instrumentId: 'ALT-YES.SIM', simulationSettings: { ...object(original.simulationSettings, 'settings'),
    fee_rates: [{ instrument_id: 'ALT-YES.SIM', maker: '0.001', taker: '0.002' }] } });
  assert.equal(original.instrumentId, 'BASE.SIM');
  const bars = ['BASE.SIM-1-MINUTE-LAST-EXTERNAL', 'ALT-YES.SIM-1-MINUTE-LAST-EXTERNAL', 'ALT-YES.SIM-5-MINUTE-LAST-EXTERNAL', 'ALT-YES.SIM-OTHER-1-MINUTE-LAST-EXTERNAL'];
  const selection = { bar_types: bars, event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '40' };
  const request = { train_end_ns: '10', validation_end_ns: '20', forecast: { selection } };
  assert.deepEqual(simulationSelection(request, 'VALIDATION', thresholdPolicy(legacy)).bar_types, bars);
  assert.deepEqual(simulationSelection(request, 'VALIDATION', policy), { ...selection, bar_types: bars.slice(1, 3), event_start_ns: '10', event_end_ns: '20', decision_cutoff_ns: '20' });
  assert.deepEqual(simulationSelection(request, 'HELDOUT', policy), { ...selection, bar_types: bars.slice(1, 3), event_start_ns: '20' });
  assert.deepEqual(request.forecast.selection, selection);
  for (const bar_types of [[], [bars[0]!], [bars[3]!]]) {
    assert.throws(() => simulationSelection({ ...request, forecast: { selection: { ...selection, bar_types } } }, 'VALIDATION', policy), /strategy_instrument_bars_missing/);
  }
  assert.throws(() => simulationSelection({ ...request, forecast: { selection: {} } }, 'VALIDATION', policy), /strategy_bar_types_invalid/);
  assert.throws(() => thresholdTargets({ points: [{ instrument_id: 'BASE.SIM', forecast: 1, event_ns: '10', available_ns: '10' }] }, policy, 'USD'), /strategy_observations_missing/);
});

test('exact proposals, half-open target filtering and original sealed catalog matching do not rewrite inputs', () => {
  assert.deepEqual(strategyMembers({ members }), members);
  assert.deepEqual(strategyMembers({ model: positive }), [{ model: positive, weight: '1' }]);
  assert.deepEqual(exactWeights(members.map((member) => member.weight)), members.map((member) => member.weight));
  assert.throws(() => exactWeights(['0.2500000000000000001', '0.7499999999999999998']), /weights_invalid/);
  assert.deepEqual(intervalTargets(['9', '10', '19', '20'].map((asof_ns) => ({ asof_ns })), { event_start_ns: '10', event_end_ns: '20' }), [{ asof_ns: '10' }, { asof_ns: '19' }]);
  const request = { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters: { total_fuel: null } } };
  const data = { catalog: '/primary', request: { ...request, forecast: { ...request.forecast, selection: { event_start_ns: '0', event_end_ns: '20' } } }, sealed: { catalog: '/original-sealed', request } };
  assert.equal(replayCatalog(data, { ...request, forecast: { ...request.forecast, parameters: {} } }), '/original-sealed');
  assert.equal(request.forecast.parameters.total_fuel, null);
});

test('owner stop aborts the ensemble child and joins its provider-free native process', { timeout: 10_000 }, async () => {
  const state = await setup(); const pidPath = join(state.directory, 'native.pid'); const script = join(state.directory, 'wait.mjs');
  await writeFile(script, `import {writeFileSync} from 'node:fs'; if (process.env.QZ_TEST_PROVIDER_SECRET) throw new Error('provider_environment_leaked'); writeFileSync(${JSON.stringify(pidPath)}, String(process.pid)); setInterval(() => {}, 1000);`);
  await writeFile(process.env.QZ_JOB_BINARY!, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
  state.faux.setResponses([tool('strategy_portfolio_evaluate', { hypothesis: 'Owned cancellation protocol, no computation result.', members })]);
  const host = await openResearch(state.options);
  try {
    const receipt = await host.start('ensemble-owner-stop', state.input);
    let pid: number;
    for (;;) {
      try { pid = Number(await readFile(pidPath, 'utf8')); break; }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; await new Promise((resolve) => setTimeout(resolve, 5)); }
    }
    await host.stop(receipt.conversationId);
    assert.throws(() => process.kill(pid, 0), (error: unknown) => (error as NodeJS.ErrnoException).code === 'ESRCH');
    assert.equal((await host.tasks()).tasks.length, 0);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.equal(facts.final!.status, 'STOPPED'); assert.equal(facts.final!.result, null);
    assert.deepEqual(object(facts.trials[0]!.proposal, 'proposal').members, members);
    const children = await host.harness.commit((tx) => tx.scanTasks({ kind: 'qz.strategy-portfolio.evaluate' }, 100), context);
    assert.equal(children.items.length, 1);
    const child = children.items[0]!;
    assert.equal(child.state.status, 'terminal');
    if (child.state.status === 'terminal') assert.equal(child.state.outcome.status, 'aborted');
  } finally { await host.close(); await state.cleanup(); }
});

test('explicit historical reassessment preserves prior exposure and cannot become fresh independent evidence', async () => {
  const state = await setup();
  Object.assign(state.input.policy.strategy, { evidenceMode: 'HISTORICAL_REASSESSMENT' });
  const host = await openResearch(state.options);
  const original = { catalog: state.input.data.sealed.catalog, barTypes: state.input.data.request.forecast.selection.bar_types, start: '20', end: '30', kind: 'FINAL', taskId: 9000, conversationId: 8000 };
  try {
    await host.harness.commit(async (tx) => { (await tx.doc(Exposures)).windows.push(original); }, context);
    state.faux.setResponses([
      tool('strategy_portfolio_evaluate', { hypothesis: 'Owner-bound exposed history is not unseen evidence.', members }),
      (transcript) => {
        const result = textResult(transcript, 'strategy_portfolio_evaluate');
        assert.equal(object(result.portfolio, 'portfolio').evidenceMode, 'HISTORICAL_REASSESSMENT');
        return tool('strategy_portfolio_finalize', { trialId: result.trialId, reason: 'Once-only owner history reassessment, not independent qualification.' });
      },
    ]);
    const receipt = await host.start('explicit-history', state.input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.equal(facts.trials[0]!.status, 'COMPUTED', JSON.stringify(facts.trials));
    assert.equal(facts.final!.status, 'COMPUTED', JSON.stringify(facts.final));
    assert.equal(object(object(facts.final!.result, 'final').portfolio, 'portfolio').evidenceMode, 'HISTORICAL_REASSESSMENT');
    assert.deepEqual(object(facts.trials[0]!.proposal, 'proposal').strategy, state.input.policy.strategy);
    const windows = (await host.harness.snapshot(Exposures, context))!.windows;
    assert.deepEqual(windows[0], original);
    assert.deepEqual(windows.map((window) => window.kind), ['FINAL', 'HISTORICAL_ITERATION', 'HISTORICAL_FINAL']);
    for (const window of windows.slice(1)) {
      assert.equal(window.evidenceMode, 'HISTORICAL_REASSESSMENT');
      assert.deepEqual(window.priorExposureTaskIds, [9000]);
    }
    const calls = await state.calls();
    const strict = JSON.parse(JSON.stringify(state.input)) as typeof state.input;
    delete (strict.policy.strategy as JsonObject).evidenceMode;
    state.faux.setResponses([tool('strategy_portfolio_evaluate', { hypothesis: 'Renaming cannot undo historical exposure.', members }), tool('research_finish', { reason: 'Still exposed.' })]);
    const reset = await host.start('still-not-fresh', strict);
    await (await host.harness.submission(reset.submissionId, context))!.wait(context);
    assert.equal((await host.view(reset.conversationId)).research!.trials[0]!.error, 'heldout_already_exposed');
    assert.deepEqual((await host.harness.snapshot(Exposures, context))!.windows, windows);
    assert.equal((await state.calls()).length, calls.length);
  } finally { await host.close(); await state.cleanup(); }
});

test('unsupported evidence modes do not silently authorize historical reuse', async () => {
  const state = await setup();
  try { assert.throws(() => thresholdPolicy({ ...state.input.policy.strategy, evidenceMode: 'REUSE_ANYTHING' }), /strategy_evidence_mode_invalid/); }
  finally { await state.cleanup(); }
});
