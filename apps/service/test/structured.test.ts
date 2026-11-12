import { createReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import { readResearchAdmissions } from '../src/admissions.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { researchCapital, researchSpecification, researchInstructions, researchCriterion } from '../../../packages/contracts/src/research.ts';
import { factorViews } from '../src/factors.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup, wat } from './support.ts';

const specification = researchSpecification({ version: 1, name: 'Volume hypothesis', question: 'Does volume predict the registered return?', rationale: 'Falsifiable original economic mechanism',
  evaluation: { metric: 'pearson_ic', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'single-factor', maxMembers: 1, stop: 'budget-or-selection' },
  capital: { scenarios: ['1000.000000000000000001'], participation: '0.01', maximumAgeSeconds: 120 } });

test('custom discussion input remains opaque and its original admission replays after dataset configuration changes', async () => {
  const state = await setup();
  try {
    const datasets = state.options.config.datasets;
    datasets[0]!.data = { catalog: 'custom protocol catalog', request: { schema_version: 2, forecast: { protocol: 'opaque' } }, catalog_scan_request: { schema_version: 2, protocol: 'opaque' } };
    await state.restart();
    state.faux.setResponses([fauxAssistantMessage('Custom research input retained; no native result claimed.')]);
    const body = { requestId: 'opaque-original', goal: specification.question, specification, model: state.model, datasetId: 'controlled' };
    const post = () => state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: body });
    const response = await post(); assert.equal(response.statusCode, 202, response.body); const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const original = (await state.service.research.state(receipt.conversationId)).research;
    const nativeInput = researchInput({ goal: specification.question, specification, model: state.model, data: datasets[0]!.data, policy: { } });
    state.faux.setResponses([fauxAssistantMessage('Original custom admission retained.')]);
    const nativeReceipt = await state.service.research.start('opaque-native-original', nativeInput);
    await (await state.service.research.harness.submission(nativeReceipt.submissionId, context))!.wait(context);
    state.options.config.datasets = [];
    await state.restart();
    const replay = await post(); assert.equal(replay.statusCode, 202, replay.body); assert.deepEqual(replay.json(), receipt);
    assert.deepEqual((await state.service.research.state(receipt.conversationId)).research, original);
    const nativeReplay = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'opaque-native-original', input: nativeInput } });
    assert.equal(nativeReplay.statusCode, 202, nativeReplay.body); assert.deepEqual(nativeReplay.json(), nativeReceipt);
    assert.equal(state.faux.state.callCount, 2);
    state.options.config.datasets = datasets;
    await state.restart();
    state.faux.setResponses([fauxAssistantMessage('Custom runner remains available beside native adapters.')]);
    const mixed = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...body, requestId: 'opaque-mixed' } });
    assert.equal(mixed.statusCode, 202, mixed.body);
    await (await state.service.research.harness.submission(mixed.json().submissionId, context))!.wait(context);
    assert.equal(state.faux.state.callCount, 3);
  } finally { await state.cleanup(); }
});

test('structured admission captures the mandate, rejects unsupported overrides and retains exact original identity through restart', async () => {
  const state = await setup();
  try {
    state.faux.setResponses([fauxAssistantMessage('Original discussion input retained, no scientific result claimed.')]);
    const input = { requestId: 'structured-original', goal: specification.question, specification, model: state.model, datasetId: 'controlled' };
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input });
    assert.equal(response.statusCode, 202, response.body); const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const detail = (await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json();
    assert.equal(detail.title, specification.name); assert.deepEqual(detail.input.specification, specification); assert.equal(detail.criterion.status, 'insufficient');
    assert.match(researchInstructions(detail.input), /Original structured research mandate/); assert.match(researchInstructions(detail.input), /Volume hypothesis/);
    assert.deepEqual((await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json(), []);
    assert.equal((await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...input, specification: { ...specification, name: 'changed' } } })).statusCode, 409);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input })).json(), receipt);
    assert.deepEqual((await state.service.app.inject({ url: '/api/v3/research/' + receipt.conversationId, headers: state.headers })).json().input, detail.input);
    assert.equal(state.faux.state.callCount, 1);
    const bad = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...input, requestId: 'unsupported', specification: { ...specification, horizon: 2 } } });
    assert.equal(bad.statusCode, 400); assert.equal(bad.json().error, 'research_override_unsupported');
  } finally { await state.cleanup(); }
});

test('discussion finish records an unscored stop without creating scientific trials', async () => {
  const state = await setup();
  try {
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Discussion complete' }), { stopReason: 'toolUse' })]);
    const receipt = await state.service.work.startResearch({ requestId: 'native-discussion-stop', goal: specification.question, specification, model: state.model, datasetId: 'controlled' });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.deepEqual(study.trials, []); assert.equal(study.final!.status, 'STOPPED');
    assert.equal(study.final!.trialId, null); assert.equal(study.final!.result, null);
    assert.equal(state.faux.state.callCount, 1);
  } finally { await state.cleanup(); }
});

test('structured native input accepts only the registered inherited policy and supported direction', async () => {
  const state = await setup();
  try {
    const registered = state.options.config.datasets[0]!;
    assert.ok(registered.strategy);
    registered.strategy = { ...registered.strategy!, instrumentIds: ['TEST.SIM'], evaluationPartition: 'HELDOUT' };
    await state.restart();
    const policy = { ...registered.policy, strategy: registered.strategy };
    const input = { goal: specification.question, specification, model: state.model, data: registered.data, policy };
    const post = (requestId: string, value: unknown) => state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId, input: value } });
    assert.throws(() => researchSpecification({ ...specification, evaluation: { ...specification.evaluation, direction: 'minimize' } }), /research_evaluation_unsupported/);
    for (const [name, strategy] of Object.entries({ instruments: { ...registered.strategy, instrumentIds: ['UNREGISTERED'] }, simulation: { ...registered.strategy, simulationSettings: { base_currency: 'OTHER' } }, partition: { ...registered.strategy, evaluationPartition: 'TRAIN' }, strategy: { replacement: true } })) {
      const bad = await post(`native-override-${name}`, { ...input, policy: { ...policy, strategy } });
      assert.equal(bad.statusCode, 400, bad.body); assert.equal(bad.json().error, 'research_policy_unregistered');
    }
    const extra = await post('native-extra-policy', { ...input, policy: { ...policy, partitionOverride: 'TRAIN' } }); assert.equal(extra.statusCode, 400); assert.equal(extra.json().error, 'research_policy_unregistered');
    state.faux.setResponses([fauxAssistantMessage('Original discussion input retained, no scientific result claimed.')]);
    const acceptedInput = input;
    const accepted = await post('native-registered-policy', acceptedInput); assert.equal(accepted.statusCode, 202, accepted.body);
    await (await state.service.research.harness.submission(accepted.json().submissionId, context))!.wait(context);
    const recorded = (await state.service.research.state(accepted.json().conversationId)).research!.input!;
    assert.deepEqual(recorded.policy.strategy, registered.strategy);
    await state.restart(); assert.equal((await post('native-registered-policy', acceptedInput)).json().conversationId, accepted.json().conversationId);
    const legacy = await post('native-legacy-policy', { ...input, specification: undefined, policy: { legacyOverride: true } }); assert.equal(legacy.statusCode, 202, legacy.body);
    await (await state.service.research.harness.submission(legacy.json().submissionId, context))!.wait(context);
  } finally { await state.cleanup(); }
});

test('identical discussion inputs retain their own original dataset admission provenance after restart', async () => {
  const state = await setup();
  try {
    const expected = new Map<number, { id: string; name: string; source: string; currency: string }>();
    for (const [index, currency] of ['USD', 'CNY'].entries()) {
      const registered = state.options.config.datasets[0]!;
      registered.id = `provenance-${index}`; registered.name = `Original dataset ${index}`; registered.source = `TEST_ONLY source ${index}`; registered.currency = currency;
      await state.restart();
      state.faux.setResponses([fauxAssistantMessage('Original discussion input retained, no scientific result claimed.')]);
        const receipt = await state.service.work.startResearch({ requestId: `identical-input-${index}`, goal: specification.question, specification, model: state.model, datasetId: registered.id });
      await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
      expected.set(receipt.conversationId, { id: registered.id, name: registered.name, source: registered.source, currency });
    }
    await state.restart();
    const rows = (await state.service.app.inject({ url: '/api/v3/research', headers: state.headers })).json().items;
    assert.equal(rows.length, 2);
    for (const [id, original] of expected) {
      const research = (await state.service.app.inject({ url: `/api/v3/research/${id}`, headers: state.headers })).json();
      assert.equal(research.datasetId, original.id); assert.equal(research.dataset.name, original.name);
      assert.equal(research.dataset.source, original.source); assert.equal(research.dataset.currency, original.currency);
    }
  } finally { await state.cleanup(); }
});

test('capital scenarios match positive native NUMERIC(38,18) and raw length bounds without changing original strings', () => {
  const accepted = ['99999999999999999999.999999999999999999', '0.000000000000000001', '0'.repeat(63) + '1', '1.' + '0'.repeat(62), '+.1', '1.', '000000000000000000001.2300000000000000000000'];
  for (const scenario of accepted) assert.deepEqual(researchCapital({ scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 }).scenarios, [scenario]);
  for (const scenario of ['100000000000000000000', '0'.repeat(64) + '1', '1.' + '0'.repeat(63), '0.0000000000000000001', '1.0000000000000000001', '1\n', '1e2', '-1', '0']) {
    assert.throws(() => researchCapital({ scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 }), /research_capital_scenarios_invalid/, scenario);
  }
});

test('HTTP rejects out-of-native-range capacity scenarios before research, factor lookup or native execution', async () => {
  const state = await setup();
  try {
    for (const scenario of ['100000000000000000000', '0'.repeat(64) + '1', '0.0000000000000000001']) {
      const capital = { scenarios: [scenario], participation: '0.01', maximumAgeSeconds: 60 };
      for (const [url, payload] of [
        ['/api/v3/factors/r999.t1.m0/capacity', capital],
        ['/api/v3/research', { requestId: `invalid-${scenario}`, goal: specification.question, specification: { ...specification, capital }, model: state.model, datasetId: 'controlled' }],
      ] as const) {
        const response = await state.service.app.inject({ method: 'POST', url, headers: state.headers, payload });
        assert.equal(response.statusCode, 400, response.body); assert.equal(response.json().error, 'research_capital_scenarios_invalid');
      }
    }
    assert.equal(state.faux.state.callCount, 0);
  } finally { await state.cleanup(); }
});

test('structured factor qualification requires an actual requested heldout metric with or without a threshold; legacy remains readable', () => {
  for (const target of [undefined, 0.1]) for (const value of [null, 0.2]) {
    const spec = { ...specification, evaluation: { ...specification.evaluation, ...(target === undefined ? {} : { target }) } };
    const result = { partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '2' }, pearson_ic: { value, status: value === null ? 'CONSTANT_SCORE' : 'AVAILABLE' } } }] };
    const trial = { taskId: 17, status: 'COMPUTED', proposal: { hypothesis: 'TEST_ONLY qualification protocol, not science', model: wat }, result };
    const study = { input: { goal: spec.question, specification: spec, model: { provider: 'test', modelId: 'test' }, data: {}, policy: { strategy: { simulationSettings: { base_currency: 'USD' } } } }, trials: [trial], final: { status: 'COMPUTED', trialId: 17, result } };
    const input = researchInput(study.input); const factor = factorViews(3, { ...study, input })[0]!; assert.equal(factor.combinable, value !== null); assert.equal(factor.reason, value === null ? 'research_criterion_insufficient' : null);
    const legacy = { ...input }; delete legacy.specification;
    assert.equal(factorViews(3, { ...study, input: legacy })[0]!.combinable, true);
  }
});

test('deterministic criterion never substitutes component diagnostics or missing metrics for a pass', () => {
  const spec = { ...specification, evaluation: { ...specification.evaluation, target: 0.1 } };
  const metric = { key: 'HELDOUT.pearson_ic', label: 'Pearson IC', partition: 'HELDOUT', value: 0.2 };
  assert.equal(researchCriterion(spec, 'COMPUTED', [metric])!.status, 'met');
  assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, value: 0.01 }])!.status, 'not_met');
  assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, component: 0 }])!.status, 'insufficient');
  assert.equal(researchCriterion(spec, 'FAILED', [metric])!.status, 'failed');
  const study = { input: { goal: specification.question, specification, model: { provider: 'test', modelId: 'test' }, data: {}, policy: { strategy: { simulationSettings: { base_currency: 'USD' } } } },
    trials: [{ taskId: 17, status: 'COMPUTED', proposal: { hypothesis: 'Controlled projection, not scientific evidence', members: [{ model: wat, weight: '0.25' }, { model: wat, weight: '0.75' }] }, result: {} }], final: { status: 'COMPUTED', trialId: 17, result: {} } };
  const factors = factorViews(3, { ...study, input: researchInput(study.input) }); assert.deepEqual(factors.map(row => row.id), ['r3.t17.m0', 'r3.t17.m1']);
  assert.ok(factors.every(row => row.evidence === 'component_only' && !row.combinable && row.metrics.length === 0));
});

test('criterion requires an unambiguous available metric and preserves legacy statusless observations', () => {
  const spec = { ...specification, evaluation: { ...specification.evaluation, target: 0.1 } };
  const metric = { key: 'HELDOUT.pearson_ic', label: 'Pearson IC', partition: 'HELDOUT', value: 0.9 };
  for (const status of ['INSUFFICIENT_SAMPLES', 'CONSTANT_SCORE', 'CONSTANT_LABEL', 'NONFINITE_DATA', 'NONFINITE_RESULT', 'FAILED', 'UNSUPPORTED', 'DEFINED']) {
    const actual = researchCriterion(spec, 'COMPUTED', [{ ...metric, status }])!;
    assert.equal(actual.status, 'insufficient', status); assert.equal(actual.value, null); assert.equal(actual.reason, status);
  }
  assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, status: 'AVAILABLE' }])!.status, 'met');
  assert.equal(researchCriterion(spec, 'COMPUTED', [metric])!.status, 'met');
  for (const duplicate of [metric, { ...metric, value: 0.01 }, { ...metric, status: 'INSUFFICIENT_SAMPLES' }]) {
    const actual = researchCriterion(spec, 'COMPUTED', [metric, duplicate])!;
    assert.equal(actual.status, 'insufficient'); assert.equal(actual.value, null); assert.equal(actual.reason, 'metric_ambiguous');
  }
  const conflict = researchCriterion(spec, 'COMPUTED', [{ ...metric, partition: 'VALIDATION' }])!;
  assert.equal(conflict.status, 'insufficient'); assert.equal(conflict.reason, 'metric_partition_mismatch');
  for (const value of [NaN, Infinity, -Infinity, null]) assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, value }])!.status, 'insufficient');
  assert.equal(researchCriterion(spec, 'FAILED', [{ ...metric, status: 'AVAILABLE' }])!.status, 'failed');
  const account = researchSpecification({ ...spec, evaluation: { metric: 'Sharpe Ratio (252 days)', direction: 'maximize', target: 1, scheme: 'registered-partitions' }, search: { scope: 'ensemble', maxMembers: 2, stop: 'budget-or-selection' } });
  const sharpe = { key: 'HELDOUT.portfolio.Sharpe Ratio (252 days)', label: 'Sharpe', partition: 'HELDOUT', value: 1.2 };
  assert.equal(researchCriterion(account, 'COMPUTED', [sharpe])!.status, 'met');
  assert.equal(researchCriterion(account, 'COMPUTED', [{ ...sharpe, component: 0 }])!.status, 'insufficient');
  assert.equal(researchCriterion(account, 'COMPUTED', [sharpe, { ...sharpe, value: 0.1 }])!.status, 'insufficient');
});

test('registered annualization selects only its exact native 252-day or 365-day Sharpe statistic', () => {
  for (const days of [252, 365]) {
    const metric = `Sharpe Ratio (${days} days)`;
    const other = `Sharpe Ratio (${days === 252 ? 365 : 252} days)`;
    const spec = researchSpecification({ ...specification,
      evaluation: { metric, direction: 'maximize', target: 1, scheme: 'registered-partitions' },
      search: { scope: 'ensemble', maxMembers: 2, stop: 'budget-or-selection' } });
    assert.equal(spec.evaluation.metric, metric);
    const own = { key: `HELDOUT.portfolio.${metric}`, label: metric, partition: 'HELDOUT', value: 1.2 };
    const different = { key: `HELDOUT.portfolio.${other}`, label: other, partition: 'HELDOUT', value: 2 };
    assert.equal(researchCriterion(spec, 'COMPUTED', [own])!.status, 'met');
    assert.equal(researchCriterion(spec, 'COMPUTED', [different])!.status, 'insufficient');
    const exact = researchCriterion(spec, 'COMPUTED', [{ ...own, value: 0.9 }, different])!;
    assert.equal(exact.status, 'not_met'); assert.equal(exact.value, 0.9);
  }
});

test('retired scoring inputs are rejected before a Pi admission or model call', async () => {
  const state = await setup();
  try {
    for (const policy of JSON.parse(JSON.stringify([{ reviewPolicy: createReviewPolicy('FACTOR', 'retired-factor', '0') }, { reviewPolicies: { FACTOR: createReviewPolicy('FACTOR', 'retired-factor', '0'), PORTFOLIO: createReviewPolicy('PORTFOLIO', 'retired-portfolio', '0') } }, { reviewedFactorSources: [] }])) as import('@earendil-works/pi-durable').JsonObject[]) {
      const requestId = `retired-${Object.keys(policy)[0]}`;
      await assert.rejects(state.service.research.start(requestId, { goal: 'No custom scoring', model: state.model, data: {}, policy }), /legacy_research_scoring_retired/);
      assert.equal((await state.service.research.harness.snapshot(Requests, context))?.requests[`request:${requestId}`], undefined);
    }
    assert.equal(state.faux.state.callCount, 0);
  } finally { await state.cleanup(); }
});
