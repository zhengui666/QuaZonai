import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import research from '../../research/plugins/research.ts';
import strategy from '../../research/plugins/strategy-portfolio.ts';
import { context, setup, wat } from './support.ts';
import { object } from '../src/facts.ts';
import { metrics, nativeCurve } from '../src/projections.ts';
import { computePortfolio } from '../src/native.ts';
import type { ServiceOptions } from '../src/config.ts';

// Actual durable/HTTP orchestration with a labelled wire probe; no numerical/market-performance claim.
for (const universe of [false, true]) test(`${universe ? 'fixed-universe' : 'legacy'} selected ensemble consumer retains original sealed binding and replays HELDOUT only`, async () => {
  const state = await setup(); const previous = process.env.QZ_JOB_BINARY;
  try {
    await state.service.close();
    const prefix = join(state.directory, 'prefix'); const sealed = join(state.directory, 'original-sealed');
    await mkdir(prefix); await mkdir(sealed);
    await writeFile(join(prefix, 'binding.json'), JSON.stringify({ name: 'PRIMARY', end: '20' }));
    await writeFile(join(sealed, 'binding.json'), JSON.stringify({ name: 'SEALED', end: '30' }));
    const log = join(state.directory, 'ensemble-calls.jsonl');
    const binary = state.options.config.jobBinary;
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('../../research/test/portfolio-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
    process.env.QZ_JOB_BINARY = binary;
    const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
      selection: { schema_version: 1, bar_types: universe ? ['BASE.SIM-1-MINUTE-LAST-EXTERNAL', 'TEST.SIM-1-MINUTE-LAST-EXTERNAL'] : ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '20', decision_cutoff_ns: '20', maximum_rows: 50 },
      parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: null } } };
    const finalRequest = structuredClone(request); Object.assign(finalRequest.forecast.selection, { event_end_ns: '30', decision_cutoff_ns: '40' });
    const dataset = state.options.config.datasets[0]!;
    dataset.data = { catalog: prefix, request, sealed: { catalog: sealed, request: finalRequest } };
    Object.assign(dataset.strategy!, { forecastMode: 'price-volume', evaluationPartition: 'HELDOUT',
      simulationSettings: { base_currency: 'USD', starting_capital: '3000', original_policy_marker: 'TEST_ONLY' } });
    if (universe) {
      Object.assign(dataset.strategy!, { instrumentId: 'BASE.SIM', instrumentIds: ['BASE.SIM', 'TEST.SIM'] });
      dataset.strategy!.simulationSettings.fee_rates = [{ instrument_id: 'BASE.SIM', maker: '0.004000', taker: '0.006000' },
        { instrument_id: 'TEST.SIM', maker: '0.001234567890123456', taker: '0.009876543210987654' }];
    }
    const selectedSettings = universe ? { ...dataset.strategy!.simulationSettings,
      fee_rates: (dataset.strategy!.simulationSettings.fee_rates as JsonObject[]).filter((rate) => rate.instrument_id === 'TEST.SIM') } : dataset.strategy!.simulationSettings;
    (state.options as ServiceOptions).extensions = [research, strategy];
    await state.restart();
    const members = [{ model: wat, weight: '0.25' }, { model: wat.replace('f64.const 1', 'f64.const -1'), weight: '0.75' }];
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('strategy_portfolio_evaluate', { hypothesis: 'One ensemble wire protocol, not a profitability claim.', members, ...(universe ? { instrumentId: 'TEST.SIM' } : {}) }), { stopReason: 'toolUse' }),
      async () => {
        const page = await state.service.research.list(); const view = await state.service.research.view(page.items[0]!.id);
        return fauxAssistantMessage(fauxToolCall('strategy_portfolio_finalize', { trialId: view.research!.trials[0]!.taskId, reason: 'Selection precedes final exposure.' }), { stopReason: 'toolUse' });
      },
    ]);
    const receipt = await state.service.work.startResearch({ requestId: 'ensemble-original', goal: 'Protocol-only selected ensemble.', model: state.model, datasetId: dataset.id, maxTrials: 1 });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const original = (await state.service.research.view(receipt.conversationId)).research!;
    assert.equal(original.final!.status, 'COMPUTED', JSON.stringify(original.final));
    const detail = (await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json();
    assert.ok(detail.trialRecords[0].metrics.some((metric: { key: string }) => metric.key === 'VALIDATION.portfolio.Sharpe Ratio (252 days)'));
    assert.ok(detail.final.metrics.some((metric: { key: string }) => metric.key === 'HELDOUT.portfolio.Sharpe Ratio (252 days)'));
    assert.ok(detail.final.metrics.some((metric: { key: string }) => metric.key === 'HELDOUT.pearson_ic'));
    const input = { requestId: 'ensemble-replay', name: 'Original selected ensemble', datasetId: dataset.id, researchIds: [receipt.conversationId], capital: '3000' };
    const view = await state.service.work.startPortfolio(input);
    await state.service.research.harness.waitForTask(view.taskId as TaskId, context);
    const fact = await state.service.work.getPortfolio(view.id);
    assert.equal(fact.view.status, 'COMPUTED', fact.view.error ?? '');
    assert.deepEqual(object(object(fact.evidence[0]!.selected).proposal).members, members);
    const proposal = object(object(fact.evidence[0]!.selected).proposal);
    assert.deepEqual(proposal.strategy, dataset.strategy);
    assert.deepEqual(proposal.request, dataset.data.request);
    assert.equal(proposal.instrumentId, universe ? 'TEST.SIM' : undefined);
    assert.equal(object(fact.policy).instrumentId, universe ? 'BASE.SIM' : 'TEST.SIM');
    const native = fact.native!; const captured = String(native.catalog);
    assert.deepEqual(JSON.parse(await readFile(join(captured, 'binding.json'), 'utf8')), { name: 'SEALED', end: '30' });
    assert.deepEqual(dataset.data, original.input!.data); // No sealed values were copied into primary.
    const simulation = object(native.simulationRequest); const selection = object(simulation.selection);
    assert.equal(selection.event_start_ns, '20'); assert.equal(selection.event_end_ns, '30');
    assert.deepEqual(selection.bar_types, ['TEST.SIM-1-MINUTE-LAST-EXTERNAL']);
    assert.deepEqual(simulation.settings, selectedSettings);
    assert.deepEqual((simulation.target_points as JsonObject[]).map((point) => point.asof_ns), ['20', '23', '29']);
    const source = object((native.sources as JsonObject[])[0]);
    assert.deepEqual(source.members, members); assert.deepEqual(source.weights, ['0.25', '0.75']);
    assert.deepEqual(source.policy, { ...dataset.strategy, instrumentId: 'TEST.SIM', simulationSettings: selectedSettings });
    assert.ok((source.target_points as JsonObject[]).every((point) => (point.targets as JsonObject[]).every((target) => target.instrument_id === 'TEST.SIM')));
    assert.equal(object(object(source.forecastRequest).selection).event_start_ns, '0');
    assert.equal(object(object(source.forecastRequest).selection).event_end_ns, '30');
    assert.deepEqual(object(object(source.forecastRequest).selection).bar_types, request.forecast.selection.bar_types);
    const calls = (await readFile(log, 'utf8')).trim().split('\n').map((line) => JSON.parse(line) as JsonObject);
    assert.equal(calls.filter((call) => call.command === 'simulate').length, 3); // iteration + independent final + replay, one account each
    for (const call of calls.filter((call) => ['evaluate-factor', 'forecast-factor'].includes(String(call.command)))) {
      const request = object(call.input);
      const selection = object(call.command === 'evaluate-factor' ? object(request.forecast).selection : request.selection);
      assert.deepEqual(selection.bar_types, finalRequest.forecast.selection.bar_types);
      assert.equal(selection.event_start_ns, '0');
    }
    assert.equal((await state.service.work.startPortfolio(input)).id, view.id);
    assert.equal((await readFile(log, 'utf8')).trim().split('\n').length, calls.length);
    assert.equal(state.faux.state.callCount, 2);
    // Protocol-only duplicate record exercises nested command delegation; it is not independent scientific evidence.
    const combined = await computePortfolio({ id: 'test-only-nested', dataDir: state.directory, catalog: captured, binary, capital: '3000', currency: 'USD',
      evidence: [fact.evidence[0]!, { ...fact.evidence[0]!, researchId: 999 }], weights: ['0.125', '0.875'], policy: dataset.strategy! }, context);
    const sources = combined.native.sources as JsonObject[];
    assert.deepEqual(sources.map((item) => item.weights), [['0.25', '0.75'], ['0.25', '0.75']]);
    assert.ok(sources.every((item) => object(item.policy).instrumentId === 'TEST.SIM'));
    assert.deepEqual(object(combined.native.simulationRequest).settings, selectedSettings);
    assert.deepEqual(combined.native.weights, ['0.125', '0.875']);
    const nested = (await readFile(log, 'utf8')).trim().split('\n').map((line) => JSON.parse(line) as JsonObject).slice(calls.length);
    assert.equal(nested.filter((call) => call.command === 'blend-targets').length, 3);
    assert.equal(nested.filter((call) => call.command === 'simulate').length, 1);
    if (universe) {
      for (const evidence of [fact.evidence, fact.evidence[0]!]) {
        await assert.rejects(computePortfolio({ id: 'test-only-override', dataDir: state.directory, catalog: captured, binary, capital: '3000', currency: 'USD',
          evidence, policy: { ...dataset.strategy!, instrumentId: 'TEST.SIM' } }, context), /strategy_policy_mismatch/);
      }
      const { instrumentIds: _universe, ...removedUniverse } = dataset.strategy!;
      await assert.rejects(computePortfolio({ id: 'test-only-removed-universe', dataDir: state.directory, catalog: captured, binary, capital: '3000', currency: 'USD',
        evidence: fact.evidence[0]!, policy: removedUniverse }, context), /strategy_policy_mismatch/);
      const after = (await readFile(log, 'utf8')).trim().split('\n').length;
      assert.equal(after, calls.length + nested.length); // Rejected overrides start no native work.
    }
  } finally { if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await state.cleanup(); }
});

test('legacy v1 single-evidence replay retains the consumer policy and whole original interval', async () => {
  const state = await setup();
  try {
    const dataset = state.options.config.datasets[0]!;
    const policy = { ...dataset.strategy!, name: 'legacy-consumer-policy', threshold: -1 };
    const evidence = { input: { data: dataset.data }, selected: { proposal: { model: wat } }, final: { result: { request: dataset.data.request! } } };
    const result = await computePortfolio({ id: 'legacy-v1', dataDir: state.directory, catalog: String(dataset.data.catalog), binary: state.options.config.jobBinary,
      capital: '3000', currency: 'USD', evidence, policy }, context);
    assert.deepEqual(object(result.native.simulationRequest).selection, object(object(dataset.data.request).forecast).selection);
    assert.deepEqual(object((result.native.sources as JsonObject[])[0]).policy, policy);
    assert.equal(object((result.target.targets as JsonObject[])[0]).instrument_id, 'TEST.SIM');
    assert.equal(result.curve[0]!.value, '3000');
  } finally { await state.cleanup(); }
});

test('research metric projection keeps component IC and actual native account keys, including unavailable values', () => {
  const result = { partitions: [{ partition: 'VALIDATION', metric: { pearson_ic: { value: null }, counts: { included: '7' } } }],
    portfolio: { partition: 'VALIDATION', statistics: [{ native_key: 'Sharpe Ratio (252 days)', value: 1.25 }, { native_key: 'unavailable', value: null }] } };
  assert.deepEqual(metrics(result).map(({ key, value }) => [key, value]), [
    ['VALIDATION.pearson_ic', null], ['VALIDATION.counts.included', '7'], ['VALIDATION.portfolio.Sharpe Ratio (252 days)', 1.25], ['VALIDATION.portfolio.unavailable', null],
  ]);
  assert.equal(metrics({ ...result, portfolio: { partition: 'VALIDATION', statistics: [] } }).length, 2);
});

test('native account curve preserves the original mixed-case pUSD collateral and exact amounts', () => {
  const native = { canonical_result: { portfolio_snapshots: [{ ts_event: '20', total_equity: ['3000.000000000000000001 pUSD'] }] } };
  assert.equal(nativeCurve(native, 'pUSD')[0]!.value, '3000.000000000000000001');
  assert.throws(() => nativeCurve(native, 'USD'), /native_equity_currency_mismatch/);
});

test('native curve sorts exact clocks while retaining same-clock observations and original amounts', () => {
  const snapshots = [
    { ts_event: '1000000020', total_equity: ['3002.000000000000000001 pUSD'] },
    { ts_event: '999999999', total_equity: ['3000 pUSD'] },
    { ts_event: '1000000010', total_equity: [null] },
    { ts_event: '1000000020', total_equity: ['3003 pUSD'] },
  ];
  const original = structuredClone(snapshots);
  const curve = nativeCurve({ canonical_result: { portfolio_snapshots: snapshots } }, 'pUSD');
  assert.deepEqual(curve.map(point => point.value), ['3000', null, '3002.000000000000000001', '3003']);
  assert.deepEqual(curve.map(point => point.at), ['1970-01-01T00:00:00.999999999Z', '1970-01-01T00:00:01.000000010Z', '1970-01-01T00:00:01.000000020Z', '1970-01-01T00:00:01.000000020Z']);
  assert.deepEqual(snapshots, original);
});

test('legacy single-evidence replay cannot remove the original historical classification', async () => {
  const state = await setup();
  try {
    const dataset = state.options.config.datasets[0]!;
    const original = { ...dataset.strategy!, evidenceMode: 'HISTORICAL_REASSESSMENT' as const };
    const evidence = { input: { data: dataset.data, policy: { strategy: original } }, selected: { proposal: { model: wat } }, final: { result: { request: dataset.data.request! } } };
    await assert.rejects(computePortfolio({ id: 'historical-v1-no-relabel', dataDir: state.directory, catalog: String(dataset.data.catalog), binary: state.options.config.jobBinary,
      capital: '3000', currency: 'USD', evidence, policy: dataset.strategy! }, context), /strategy_policy_mismatch/);
  } finally { await state.cleanup(); }
});

test('cross-market selected replay binds original market allocations and trades their HELDOUT union in one total-capital account', async () => {
  const state = await setup();
  try {
    const catalog = join(state.directory, 'cross-market-sealed'); await mkdir(catalog);
    await writeFile(join(catalog, 'binding.json'), JSON.stringify({ name: 'SEALED', end: '30', cross_market: true }));
    const log = join(state.directory, 'cross-market-calls.jsonl'); const binary = state.options.config.jobBinary;
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('../../research/test/portfolio-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
    const policy = { name: 'original-cross-market', kind: 'cross-market-threshold' as const, instrumentIds: ['BASE.SIM', 'TEST.SIM', 'UNSELECTED.SIM'],
      threshold: 0, investedWeight: '0.75', targetTtlNs: '5', forecastMode: 'price-volume' as const, evaluationPartition: 'HELDOUT' as const,
      simulationSettings: { base_currency: 'USD', starting_capital: '3000', fee_rates: ['BASE.SIM', 'TEST.SIM', 'UNSELECTED.SIM'].map((instrument_id) => ({ instrument_id, maker: '0.001', taker: '0.001' })) } };
    const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
      selection: { schema_version: 1, bar_types: policy.instrumentIds.map((id) => id + '-1-MINUTE-LAST-EXTERNAL'), event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '30', maximum_rows: 50 },
      parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: null } } };
    const members = [{ model: wat, weight: '1' }];
    const allocations = [{ instrumentId: 'BASE.SIM', weight: '0.4' }, { instrumentId: 'TEST.SIM', weight: '0.6' }];
    const evidence = { input: { policy: { strategy: policy } }, selected: { proposal: { members, allocations, strategy: policy } }, final: { result: { request } } };
    const input = { id: 'cross-market-replay', dataDir: state.directory, catalog, binary, capital: '3000', currency: 'USD', evidence, policy };
    const result = await computePortfolio(input, context);
    const simulation = object(result.native.simulationRequest);
    assert.equal(object(simulation.settings).starting_capital, '3000');
    assert.equal((object(simulation.settings).fee_rates as JsonObject[]).length, 2);
    assert.deepEqual(object(simulation.selection).bar_types, request.forecast.selection.bar_types.slice(0, 2));
    assert.equal(object(simulation.selection).event_start_ns, '20');
    assert.deepEqual((simulation.target_points as JsonObject[]).map((point) => point.asof_ns), ['20', '21', '25', '26', '29']);
    assert.deepEqual((simulation.target_points as JsonObject[]).map((point) => point.rebalance_instrument_ids), [['BASE.SIM'], ['TEST.SIM'], ['BASE.SIM'], ['TEST.SIM'], ['BASE.SIM', 'TEST.SIM']]);
    assert.deepEqual(object((result.native.sources as JsonObject[])[0]!.policy).allocations, allocations);
    assert.equal(result.curve[0]!.value, '3000');
    const calls = (await readFile(log, 'utf8')).trim().split('\n').map((line) => JSON.parse(line) as JsonObject);
    assert.equal(calls.filter((call) => call.command === 'simulate').length, 1);
    assert.deepEqual(object(object(calls.find((call) => call.command === 'forecast-factor')!.input).selection).bar_types, request.forecast.selection.bar_types.slice(0, 2));
    await assert.rejects(computePortfolio({ ...input, policy: { ...policy, investedWeight: '1' } }, context), /strategy_policy_mismatch/);
    assert.equal((await readFile(log, 'utf8')).trim().split('\n').length, calls.length);
  } finally { await state.cleanup(); }
});

test('cross-market owner portfolio command accepts the originally admitted policy after owner-isolated final evidence', async () => {
  const state = await setup(); const previous = process.env.QZ_JOB_BINARY;
  try {
    await state.service.close();
    const prefix = join(state.directory, 'multi-prefix'); const sealed = join(state.directory, 'multi-sealed');
    await mkdir(prefix); await mkdir(sealed);
    await writeFile(join(prefix, 'binding.json'), JSON.stringify({ name: 'PRIMARY', end: '20', cross_market: true }));
    await writeFile(join(sealed, 'binding.json'), JSON.stringify({ name: 'SEALED', end: '30', cross_market: true }));
    const log = join(state.directory, 'multi-owner-calls.jsonl'); const binary = state.options.config.jobBinary;
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('../../research/test/portfolio-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
    process.env.QZ_JOB_BINARY = binary;
    const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
      selection: { schema_version: 1, bar_types: ['BASE.SIM-1-MINUTE-LAST-EXTERNAL', 'TEST.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: '20', decision_cutoff_ns: '20', maximum_rows: 50 },
      parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: null } } };
    const finalRequest = structuredClone(request); finalRequest.forecast.selection.event_end_ns = '30'; finalRequest.forecast.selection.decision_cutoff_ns = '30';
    const dataset = state.options.config.datasets[0]!;
    dataset.data = { catalog: prefix, request, sealed: { catalog: sealed, request: finalRequest } };
    dataset.strategy = { name: 'owner-many-markets', kind: 'cross-market-threshold', instrumentIds: ['BASE.SIM', 'TEST.SIM'], threshold: 0,
      investedWeight: '0.75', targetTtlNs: '5', forecastMode: 'price-volume', evaluationPartition: 'HELDOUT',
      simulationSettings: { base_currency: 'USD', starting_capital: '3000', fee_rates: ['BASE.SIM', 'TEST.SIM'].map((instrument_id) => ({ instrument_id, maker: '0.001', taker: '0.001' })) } };
    (state.options as ServiceOptions).extensions = [research, strategy]; await state.restart();
    const allocations = [{ instrumentId: 'BASE.SIM', weight: '0.4' }, { instrumentId: 'TEST.SIM', weight: '0.6' }];
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('strategy_portfolio_evaluate', { hypothesis: 'Multi-market owner-command protocol, not profitability.', members: [{ model: wat, weight: '1' }], allocations }), { stopReason: 'toolUse' }),
      async () => {
        const page = await state.service.research.list(); const view = await state.service.research.view(page.items[0]!.id);
        return fauxAssistantMessage(fauxToolCall('strategy_portfolio_finalize', { trialId: view.research!.trials[0]!.taskId, reason: 'Protocol-only original selection before independent assessment.' }), { stopReason: 'toolUse' });
      },
    ]);
    const receipt = await state.service.work.startResearch({ requestId: 'multi-market-owner-original', goal: 'Owner multi-market shared-account protocol.', model: state.model, datasetId: dataset.id, maxTrials: 1 });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.equal((await state.service.research.view(receipt.conversationId)).research!.final!.status, 'COMPUTED');
    const input = { requestId: 'multi-market-owner-portfolio', name: 'Original shared-account markets', datasetId: dataset.id, researchIds: [receipt.conversationId], capital: '3000' };
    const admitted = await state.service.work.startPortfolio(input);
    await state.service.research.harness.waitForTask(admitted.taskId as TaskId, context);
    const result = await state.service.work.getPortfolio(admitted.id);
    assert.equal(result.view.status, 'COMPUTED', result.view.error ?? '');
    assert.deepEqual(object(object(result.evidence[0]!.selected).proposal).allocations, allocations);
    assert.equal(object(object(result.native!.simulationRequest).settings).starting_capital, '3000');
    assert.equal(((object(result.native!.simulationRequest).target_points as JsonObject[])[0]!.rebalance_instrument_ids as string[]).length, 1);
    assert.equal((await state.service.work.startPortfolio(input)).id, admitted.id);
    assert.equal(state.faux.state.callCount, 2);
  } finally { if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await state.cleanup(); }
});
