import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { researchInput } from '../src/input.ts';
import { modelBriefInput } from '../src/feedback.ts';
import { nativeCommands } from '../src/native-execution.ts';

test('native indicator research keeps its explicit binding without requiring a Wasm forecast or heldout', () => {
  const data = {
    catalog: '/registered/catalog',
    indicator_strategy_request: {
      schema_version: 1,
      selection: { schema_version: 1, bar_types: ['BTCUSD.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '1000000000000000000', event_end_ns: '1000100000000000000', decision_cutoff_ns: '1000100000000000000' },
      signal: { kind: 'EMA_CROSS', fast_period: 12, slow_period: 26 },
      decision_start_ns: '1000002000000000000', long_weight: '0.5', target_ttl_seconds: 300,
    },
  };
  const input = researchInput({ goal: 'Evaluate native EMA strategy', model: { provider: 'test', modelId: 'test' }, data, policy: {} });
  const brief = modelBriefInput(input);
  assert.deepEqual(brief.data, data);
  assert.equal('feature_model' in brief.data, false);
  assert.equal('forecast' in brief.data, false);
  assert.equal(nativeCommands.has('simulate-indicator-strategy'), true);
});


// Exercise the official registry, conversation configuration and stored admission identity.
test('new conversations expose only native or ordinary research tools; explicit old scoring cannot enter', async () => {
  const { mkdtemp, rm } = await import('node:fs/promises');
  const { join } = await import('node:path'); const { tmpdir } = await import('node:os');
  const { BACKGROUND_CONTEXT: context } = await import('@earendil-works/chord/context');
  const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } = await import('@earendil-works/pi-ai');
  const { openResearch } = await import('../src/host.ts');
  const { default: research } = await import('../plugins/research.ts');
  const { createReviewPolicy } = await import('../../../packages/contracts/src/review-scoring.ts');
  const { Requests, Study } = await import('../src/documents.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-tools-'));
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('factor_evaluate', { hypothesis: 'forbidden new legacy computation', model: '(module)' }), { stopReason: 'toolUse' }), fauxAssistantMessage('Natural-language exploration; no computation claimed.'), fauxAssistantMessage('Bound indicator tool is available; no computation claimed.')]);
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [research] };
  let host = await openResearch(options);
  const input = { goal: 'Explore a Nautilus strategy', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} };
  try {
    const explicit = { ...input, policy: { reviewPolicy: createReviewPolicy('FACTOR', 'old-policy', '1') } };
    await assert.rejects(host.start('reject-scoring', explicit as any), /legacy_research_scoring_retired/);
    assert.equal((await host.harness.snapshot(Requests, context))?.requests['request:reject-scoring'], undefined);
    assert.equal(faux.state.callCount, 0);
    const plain = await host.start('natural-language', input);
    await (await host.harness.submission(plain.submissionId, context))!.wait(context);
    const ordinary = await (await host.harness.conversation(plain.conversationId, context))!.agent(context);
    assert.deepEqual(ordinary.extensions.map(extension => extension.name), ['qz.research-native', 'qz.indicator-strategy', 'qz.research-assessment']);
    assert.ok(ordinary.tools.some(tool => tool.name === 'research_brief'));
    assert.ok(ordinary.tools.some(tool => tool.name === 'research_literature'));
    assert.ok(!ordinary.tools.some(tool => /factor_evaluate|ensemble|review_finish|strategy_portfolio/.test(tool.name)));
    const study = (await host.harness.snapshot(Study, plain.conversationId, context))!;
    assert.equal(study.input!.policy.reviewPolicies, undefined);
    assert.equal(study.trials.length, 0, 'an attempted legacy tool call cannot execute or admit a trial');
    assert.ok(!ordinary.callable.some(tool => /factor_evaluate|ensemble|review_finish/.test(tool.name)));
    const bound = await host.start('bound-native', { ...input, data: { catalog: '/registered/catalog', indicator_strategy_request: {} } });
    await (await host.harness.submission(bound.submissionId, context))!.wait(context);
    const agent = await (await host.harness.conversation(bound.conversationId, context))!.agent(context);
    assert.deepEqual(agent.extensions.map(extension => extension.name), ['qz.indicator-strategy', 'qz.research-assessment']);
    assert.deepEqual(agent.tools.map(tool => tool.name), ['native_indicator_strategy_configure', 'native_indicator_strategy_evaluate', 'native_strategy_logic_review', 'native_strategy_assessment_report']);
    const calls = faux.state.callCount;
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('natural-language', input), plain);
    assert.equal(faux.state.callCount, calls);
    assert.deepEqual((await (await host.harness.conversation(bound.conversationId, context))!.agent(context)).tools.map(tool => tool.name), ['native_indicator_strategy_configure', 'native_indicator_strategy_evaluate', 'native_strategy_logic_review', 'native_strategy_assessment_report']);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

import { indicatorStrategyResult } from '../src/indicator-result.ts';
import type { JsonObject } from '@earendil-works/pi-durable';

test('indicator result wire validation rejects empty, mismatched, missing and failed evidence', async () => {
  const version = JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8')).components.schemas.NativeModelRefV1.oneOf[0].properties.upstream_version.enum[0] as string;
  // Wire-contract cases only; these do not claim a backtest has executed.
  const request: JsonObject = { schema_version: 1, selection: { maximum_rows: 0 }, settings: { fee_model: { upstream_version: version }, fill_model: { upstream_version: version }, latency_model: { upstream_version: version } }, signal: { kind: 'RSI', period: 14, entry_below: 30, exit_above: 70 }, long_weight: '0.50' };
  const summary = { 'orders.open': '0', 'orders.inflight': '0' };
  const result: JsonObject = {
    request: { ...request, long_weight: '0.5' }, observations: [],
    performance_scope: 'FULL_SELECTION_INCLUDING_PRE_DECISION_CASH', decision_policy: 'BATCH_LAST_SETTLED_ACCOUNT_ONLY', skipped_unsettled_decisions: '0', merged_same_time_bars: '0',
    simulation: {
      schema_version: 1, native_version: version, iterations: '20', events: '0', orders: '0', positions: '0', consumed_target_points: '1', settled_replay_ns: '200', summary,
      statistics: [], returns_kind: 'PORTFOLIO_DAILY', returns_status: 'INSUFFICIENT_DATA', returns_reason: 'PORTFOLIO_DAILY_RETURNS_UNAVAILABLE', returns: [],
      canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'stopped', iterations: '20', total_events: '0', total_orders: '0', total_positions: '0' }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], components: {}, statistics: {}, summary },
    },
  };
  assert.equal(indicatorStrategyResult(result, request), result);
  assert.throws(() => indicatorStrategyResult({}, request));
  assert.throws(() => indicatorStrategyResult({ ...result, request: { ...request, long_weight: '0.6' } }, request), /request_mismatch/);
  const sim = result.simulation as JsonObject;
  for (const wrongVersion of ['', 'fake-version', version === '0.69.0' ? '0.68.0' : '0.69.0']) assert.throws(() => indicatorStrategyResult({ ...result, simulation: { ...sim, native_version: wrongVersion } }, request));
  const wrongModel = structuredClone(request); ((wrongModel.settings as JsonObject).fee_model as JsonObject).upstream_version = 'wrong-model-version';
  assert.throws(() => indicatorStrategyResult({ ...result, request: wrongModel }, wrongModel), /native_version_mismatch/);
  for (const key of ['canonical_result', 'statistics', 'settled_replay_ns', 'consumed_target_points']) {
    const missing = structuredClone(sim); delete missing[key];
    assert.throws(() => indicatorStrategyResult({ ...result, simulation: missing }, request));
  }
  assert.throws(() => indicatorStrategyResult({ ...result, simulation: { ...sim, statistics: [{ group: 'RETURNS', native_key: 'Sharpe', currency: null, reason_code: null, value: Infinity }] } }, request));
  const failed = structuredClone(result); ((failed.simulation as JsonObject).canonical_result as JsonObject).run = { outcome: 'failed' };
  assert.throws(() => indicatorStrategyResult(failed, request));
  const omittedDefault = structuredClone(request); delete (omittedDefault.selection as JsonObject).maximum_rows;
  assert.equal(indicatorStrategyResult(result, omittedDefault), result);
});

import { configuredIndicatorRequest, IndicatorConfiguration } from '../plugins/indicator-strategy.ts';
const indicatorParameters = { signal: { kind: 'EMA_CROSS', fast_period: 2, slow_period: 3 }, decision_start_ns: '120', long_weight: '0.5', target_ttl_seconds: 60 };
function ordinaryIndicatorInput(catalog: string) {
  return { goal: 'Research an EMA strategy using native indicators', model: { provider: 'test', modelId: 'test' },
    data: { catalog, request: { forecast: { selection: { schema_version: 1, bar_types: ['BTCUSD.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '100', event_end_ns: '300', decision_cutoff_ns: '300' } }, train_end_ns: '150', validation_end_ns: '200' }, sealed: { catalog: '/must-never-read-sealed' } },
    policy: { strategy: { simulationSettings: { fee_rates: [{ instrument_id: 'BTCUSD.SIM', maker: '0.001', taker: '0.002' }], upstream_version: 'registered-native-version', fee_model: { kind: 'MAKER_TAKER' }, fill_model: { kind: 'DEFAULT' }, latency_model: { kind: 'DEFAULT' } } } } };
}

test('natural-language configuration preserves native settings and narrows only registered BAR scope', () => {
  const input = ordinaryIndicatorInput('/registered'); const original = structuredClone(input);
  const configured = configuredIndicatorRequest(input, indicatorParameters);
  assert.equal(configured.validationPrefix, true);
  assert.deepEqual(configured.request.selection, { ...input.data.request.forecast.selection, event_end_ns: '200', decision_cutoff_ns: '200' });
  assert.deepEqual(configured.request.settings, input.policy.strategy.simulationSettings);
  assert.deepEqual(input, original);
  const untrustedExtra = configuredIndicatorRequest(input, { ...indicatorParameters, selection: { event_start_ns: '0' }, settings: {} } as any);
  assert.deepEqual(untrustedExtra.request, configured.request, 'extra tool arguments cannot replace registered selection/settings');
  assert.throws(() => configuredIndicatorRequest(input, { ...indicatorParameters, decision_start_ns: '200' }), /outside_registered_scope/);
  assert.throws(() => configuredIndicatorRequest(input, { ...indicatorParameters, bar_type: 'UNREGISTERED' }), /outside_registered_scope/);
  input.data.request.forecast.selection.bar_types.push('ETHUSD.SIM-1-MINUTE-LAST-EXTERNAL');
  input.policy.strategy.simulationSettings.fee_rates.push({ instrument_id: 'ETHUSD.SIM', maker: '0.003', taker: '0.004' });
  assert.throws(() => configuredIndicatorRequest(input, indicatorParameters), /single_series/);
  const selected = configuredIndicatorRequest(input, { ...indicatorParameters, bar_type: input.data.request.forecast.selection.bar_types[1] });
  assert.deepEqual((selected.request.selection as JsonObject).bar_types, [input.data.request.forecast.selection.bar_types[1]]);
  assert.equal('bar_type' in selected.request, false);
  assert.deepEqual((selected.request.settings as JsonObject).fee_rates, [input.policy.strategy.simulationSettings.fee_rates[1]]);
  assert.equal(input.policy.strategy.simulationSettings.fee_rates.length, 2);
  input.policy.strategy.simulationSettings.fee_rates.push({ ...input.policy.strategy.simulationSettings.fee_rates[1]! });
  assert.throws(() => configuredIndicatorRequest(input, { ...indicatorParameters, bar_type: input.data.request.forecast.selection.bar_types[1] }), /fee_duplicate/);
  const hyphenated = ordinaryIndicatorInput('/registered');
  hyphenated.data.request.forecast.selection.bar_types = ['ETH-USDT.SIM-1-MINUTE-LAST-EXTERNAL'];
  hyphenated.policy.strategy.simulationSettings.fee_rates = [{ instrument_id: 'ETH-USDT.SIM', maker: '0.003', taker: '0.004' }];
  assert.deepEqual((configuredIndicatorRequest(hyphenated, indicatorParameters).request.settings as JsonObject).fee_rates, hyphenated.policy.strategy.simulationSettings.fee_rates);
  input.policy.strategy.simulationSettings.fee_rates = [];
  assert.throws(() => configuredIndicatorRequest(input, { ...indicatorParameters, bar_type: input.data.request.forecast.selection.bar_types[1] }), /fee_missing/);
});

test('same Pi conversation configures native request, dispatches exact bounded request and persists genuine failure', async () => {
  // Protocol-only executable intentionally fails. This verifies wiring, not a scientific/backtest result.
  const { mkdtemp, writeFile, readFile, rm } = await import('node:fs/promises');
  const { join } = await import('node:path'); const { tmpdir } = await import('node:os');
  const { BACKGROUND_CONTEXT: context } = await import('@earendil-works/chord/context');
  const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } = await import('@earendil-works/pi-ai');
  const { openResearch } = await import('../src/host.ts'); const { Study } = await import('../src/documents.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-indicator-configure-')); const binary = join(directory, 'protocol-failure');
  const oldBinary = process.env.QZ_JOB_BINARY;
  await writeFile(binary, `#!${process.execPath}\nconst fs=require('node:fs');const a=process.argv.slice(2);fs.writeFileSync(${JSON.stringify(join(directory, 'received.json'))},JSON.stringify({args:a,request:JSON.parse(fs.readFileSync(0,'utf8'))}));process.stderr.write('intentional_protocol_failure');process.exit(1);\n`, { mode: 0o755 });
  process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const parameters = { ...indicatorParameters, bar_type: 'ETHUSD.SIM-1-MINUTE-LAST-EXTERNAL' };
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_indicator_strategy_configure', parameters), { stopReason: 'toolUse' }), fauxAssistantMessage(fauxToolCall('native_indicator_strategy_evaluate', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Native execution failed; no performance or approval claimed.')]);
  const options = { database: join(directory, 'research.sqlite'), models }; let host = await openResearch(options);
  const input = { ...ordinaryIndicatorInput(directory), model: { provider: faux.provider.id, modelId: faux.getModel().id } };
  input.data.request.forecast.selection.bar_types.push(parameters.bar_type);
  input.policy.strategy.simulationSettings.fee_rates.push({ instrument_id: 'ETHUSD.SIM', maker: '0.003', taker: '0.004' });
  try {
    const receipt = await host.start('configure-from-language', input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const stored = (await host.harness.snapshot(IndicatorConfiguration, receipt.conversationId, context))!;
    assert.deepEqual(stored.request, configuredIndicatorRequest(input, parameters).request);
    assert.ok(stored.evaluationTaskId);
    const execution = await host.harness.waitForTask(stored.evaluationTaskId!, context);
    assert.match(JSON.stringify(execution.state.outcome), /intentional_protocol_failure/);
    const dispatched = JSON.parse(await readFile(join(directory, 'received.json'), 'utf8'));
    assert.deepEqual(dispatched.request, stored.request);
    assert.deepEqual(dispatched.request.settings.fee_rates, [input.policy.strategy.simulationSettings.fee_rates[1]]);
    assert.deepEqual((await host.harness.snapshot(Study, receipt.conversationId, context))!.input!.policy, input.policy);
    assert.equal(dispatched.args[0], 'simulate-indicator-strategy');
    assert.equal(dispatched.args[dispatched.args.indexOf('--catalog') + 1], directory);
    const task = await host.harness.waitForTask(stored.evaluationTaskId!, context);
    assert.equal(task.state.outcome.status, 'failed');
    assert.equal((await host.harness.snapshot(Study, receipt.conversationId, context))!.final, null);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(IndicatorConfiguration, receipt.conversationId, context), stored);
    assert.deepEqual(await host.start('configure-from-language', input), receipt);
    assert.equal((await host.harness.snapshot(Study, receipt.conversationId, context))!.final, null);
    const changed = { ...parameters, long_weight: '0.75' };
    faux.setResponses([fauxAssistantMessage(fauxToolCall('native_indicator_strategy_configure', changed), { stopReason: 'toolUse' }), fauxAssistantMessage(fauxToolCall('native_indicator_strategy_evaluate', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Second run also failed honestly.')]);
    const resumed = await host.resume(receipt.conversationId, receipt.submissionId, 'Try long weight 0.75');
    await (await host.harness.submission(resumed.submissionId, context))!.wait(context);
    const next = (await host.harness.snapshot(IndicatorConfiguration, receipt.conversationId, context))!;
    assert.deepEqual(next.previousTaskIds, [stored.evaluationTaskId]);
    assert.notEqual(next.evaluationTaskId, stored.evaluationTaskId);
    assert.equal(next.request!.long_weight, '0.75');
    assert.deepEqual((await host.harness.waitForTask(stored.evaluationTaskId!, context)).state.outcome, execution.state.outcome);
    assert.deepEqual(await host.resume(receipt.conversationId, receipt.submissionId, 'Try long weight 0.75'), resumed);
    assert.equal((await host.harness.snapshot(Study, receipt.conversationId, context))!.final, null);
    // A failed local Task can still have unresolved remote work: never dispatch a duplicate.
    const { NativeRemoteUncertainty } = await import('../src/native-execution.ts');
    await host.harness.commit(async tx => { (await tx.doc(NativeRemoteUncertainty, receipt.conversationId, String(next.evaluationTaskId), null)).requests.push({ taskId: next.evaluationTaskId!, requestId: 'unresolved-invalid-reference', status: 'UNKNOWN' }); }, context);
    const calls = faux.state.callCount;
    await assert.rejects(host.resume(receipt.conversationId, resumed.submissionId, 'Try another weight'), /native_remote_execution_unsettled/);
    assert.equal(faux.state.callCount, calls);
  } finally { await host.close(); if (oldBinary === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = oldBinary; await rm(directory, { recursive: true, force: true }); }
});

test('explicit indicator batches preserve original native requests and advertise one native operation', () => {
  const request = { schema_version: 1, selection: { bar_types: ['BTCUSD.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '1790000000000000001' },
    signal: { kind: 'RSI', period: 14, entry_below: 30, exit_above: 70 }, settings: {}, long_weight: '0.500000000000000001' };
  const data = { catalog: '/registered/catalog', indicator_strategy_batch_request: { schema_version: 1, requests: [request, { ...request, long_weight: '0.6' }] } };
  const input = researchInput({ goal: 'Explicit same-data native batch', model: { provider: 'test', modelId: 'test' }, data, policy: {} });
  assert.deepEqual(modelBriefInput(input).data, data);
  assert.equal(nativeCommands.has('simulate-indicator-strategy-batch'), true);
});

test('new native conversations accept follow-up after ordinary dialogue and preserve recorded native finals on continuation', async () => {
  const { mkdtemp, rm } = await import('node:fs/promises'); const { join } = await import('node:path'); const { tmpdir } = await import('node:os');
  const { BACKGROUND_CONTEXT: context } = await import('@earendil-works/chord/context');
  const { createModels, fauxProvider, fauxAssistantMessage } = await import('@earendil-works/pi-ai');
  const { openResearch } = await import('../src/host.ts'); const { Study } = await import('../src/documents.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-followup-'));
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage('Which parameters should we use?'), fauxAssistantMessage('Recorded the owner clarification.'), fauxAssistantMessage('Continuing with the additional request.')]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    const started = await host.start('native-followup', { goal: 'Discuss native strategies', data: {}, policy: {}, model: { provider: faux.provider.id, modelId: faux.getModel().id } });
    await (await host.harness.submission(started.submissionId, context))!.wait(context);
    const clarified = await host.resume(started.conversationId, started.submissionId, 'Use EMA periods 12 and 26');
    await (await host.harness.submission(clarified.submissionId, context))!.wait(context);
    // Serialization-only historical final fixture: no executable or scientific output is mocked as success.
    const final = { status: 'COMPUTED', taskId: 999, scientificClaim: 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION', reason: 'retained_test_record', result: null };
    await host.harness.commit(async tx => { (await tx.doc(Study, started.conversationId)).final = final; }, context);
    const adjusted = await host.resume(started.conversationId, clarified.submissionId, 'Compare EMA periods 10 and 30');
    await (await host.harness.submission(adjusted.submissionId, context))!.wait(context);
    const study = (await host.harness.snapshot(Study, started.conversationId, context))!;
    assert.deepEqual(study.previousNativeResults, [{ previousSubmissionId: clarified.submissionId, final }]);
    assert.equal(study.final, null);
    assert.equal(adjusted.conversationId, started.conversationId);
    assert.notEqual(adjusted.submissionId, clarified.submissionId);
    assert.deepEqual(await host.resume(started.conversationId, clarified.submissionId, 'Compare EMA periods 10 and 30'), adjusted);
    await assert.rejects(host.resume(started.conversationId, clarified.submissionId, 'Different request'), /recovery_request_conflict/);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});


test('historical templates permit delayed decisions through native cutoff without widening iterative sealed boundaries', () => {
  const ordinary = ordinaryIndicatorInput('/registered');
  const template = { ...ordinary, data: { catalog: ordinary.data.catalog, indicator_strategy_request: {
    schema_version: 1, selection: { ...ordinary.data.request.forecast.selection, decision_cutoff_ns: '400' },
    settings: ordinary.policy.strategy.simulationSettings,
  } } };
  for (const decision_start_ns of ['300', '350', '400']) {
    const configured = configuredIndicatorRequest(template, { ...indicatorParameters, decision_start_ns });
    assert.equal(configured.request.decision_start_ns, decision_start_ns);
    assert.deepEqual(configured.request.selection, template.data.indicator_strategy_request.selection);
    assert.equal(configured.validationPrefix, false);
  }
  assert.throws(() => configuredIndicatorRequest(template, { ...indicatorParameters, decision_start_ns: '401' }), /outside_registered_scope/);
  ordinary.data.request.forecast.selection.decision_cutoff_ns = '400';
  assert.throws(() => configuredIndicatorRequest(ordinary, { ...indicatorParameters, decision_start_ns: '200' }), /outside_registered_scope/);
  assert.throws(() => configuredIndicatorRequest(ordinary, { ...indicatorParameters, decision_start_ns: '350' }), /outside_registered_scope/);
});

test('batch bindings reuse common native scope for configuration and reject conflicting original bindings', () => {
  const ordinary = ordinaryIndicatorInput('/registered');
  const request = configuredIndicatorRequest(ordinary, indicatorParameters).request;
  const data = { catalog: '/registered', indicator_strategy_batch_request: { schema_version: 1, requests: [request, { ...request, long_weight: '0.75' }] } };
  const input = { ...ordinary, data }; const original = structuredClone(input);
  const configured = configuredIndicatorRequest(input, { ...indicatorParameters, long_weight: '0.25' });
  assert.equal(configured.validationPrefix, false);
  assert.deepEqual(configured.request.selection, request.selection);
  assert.deepEqual(configured.request.settings, request.settings);
  assert.equal(configured.request.long_weight, '0.25');
  assert.deepEqual(input, original);
  assert.throws(() => configuredIndicatorRequest({ ...input, data: { ...data, indicator_strategy_request: request } }, indicatorParameters), /binding_ambiguous/);
  assert.throws(() => configuredIndicatorRequest({ ...input, data: { ...data, sealed: {} } }, indicatorParameters), /must_be_historical/);
  const mismatched = structuredClone(input);
  mismatched.data.indicator_strategy_batch_request.requests[1]!.settings = {};
  assert.throws(() => configuredIndicatorRequest(mismatched, indicatorParameters), /batch_scope_mismatch/);
});

test('real Pi batch admission and settled reconfiguration preserve current identities', async () => {
  // Intentional protocol failure checks the actual native command/request path, not backtest performance.
  const { mkdtemp, writeFile, readFile, rm } = await import('node:fs/promises'); const { join } = await import('node:path'); const { tmpdir } = await import('node:os');
  const { BACKGROUND_CONTEXT: context } = await import('@earendil-works/chord/context');
  const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } = await import('@earendil-works/pi-ai');
  const { openResearch } = await import('../src/host.ts'); const { Study } = await import('../src/documents.ts');
  const { default: research } = await import('../plugins/research.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-batch-reconfigure-')); const binary = join(directory, 'protocol-failure'); const received = join(directory, 'received.jsonl');
  const previousBinary = process.env.QZ_JOB_BINARY;
  await writeFile(binary, `#!${process.execPath}\nconst fs=require('node:fs');const args=process.argv.slice(2);fs.appendFileSync(${JSON.stringify(received)},JSON.stringify({args,request:JSON.parse(fs.readFileSync(0,'utf8'))})+'\\n');process.stderr.write('intentional_batch_protocol_failure');process.exit(1);\n`, { mode: 0o755 });
  process.env.QZ_JOB_BINARY = binary;
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_indicator_strategy_evaluate', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Original batch failed; no metrics claimed.')]);
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [research] }; let host = await openResearch(options);
  const ordinary = ordinaryIndicatorInput(directory); const request = configuredIndicatorRequest(ordinary, indicatorParameters).request;
  const input = { ...ordinary, model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, indicator_strategy_batch_request: { schema_version: 1, requests: [request, { ...request, long_weight: '0.75' }] } } };
  try {
    const receipt = await host.start('original-batch', input);
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const agent = await (await host.harness.conversation(receipt.conversationId, context))!.agent(context);
    assert.deepEqual(agent.extensions.map(extension => extension.name), ['qz.indicator-strategy', 'qz.research-assessment']);
    const originalRun = (await host.harness.snapshot(IndicatorConfiguration, receipt.conversationId, context))!;
    assert.ok(originalRun.evaluationTaskId);
    const originalTask = await host.harness.waitForTask(originalRun.evaluationTaskId!, context);
    assert.equal(originalTask.state.outcome.status, 'failed');
    const changed = { ...indicatorParameters, long_weight: '0.25' };
    faux.setResponses([fauxAssistantMessage(fauxToolCall('native_indicator_strategy_configure', changed), { stopReason: 'toolUse' }), fauxAssistantMessage(fauxToolCall('native_indicator_strategy_evaluate', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Changed single run failed honestly.')]);
    const next = await host.resume(receipt.conversationId, receipt.submissionId, 'Try a single run with weight 0.25');
    await (await host.harness.submission(next.submissionId, context))!.wait(context);
    const calls = (await readFile(received, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(calls.map(call => call.args[0]), ['simulate-indicator-strategy-batch', 'simulate-indicator-strategy']);
    assert.deepEqual(calls[0].request, input.data.indicator_strategy_batch_request);
    assert.deepEqual(calls[1].request, configuredIndicatorRequest(input, changed).request);
    const configuration = (await host.harness.snapshot(IndicatorConfiguration, receipt.conversationId, context))!;
    assert.deepEqual(configuration.previousTaskIds, [originalRun.evaluationTaskId]);
    assert.notEqual(configuration.evaluationTaskId, originalRun.evaluationTaskId);
    assert.deepEqual((await host.harness.snapshot(Study, receipt.conversationId, context))!.input, input);
    assert.equal((await host.harness.snapshot(Study, receipt.conversationId, context))!.final, null);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.resume(receipt.conversationId, receipt.submissionId, 'Try a single run with weight 0.25'), next);
    assert.deepEqual(await host.harness.snapshot(IndicatorConfiguration, receipt.conversationId, context), configuration);
  } finally { await host.close(); if (previousBinary === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previousBinary; await rm(directory, { recursive: true, force: true }); }
});
