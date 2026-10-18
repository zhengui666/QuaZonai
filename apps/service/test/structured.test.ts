import assert from 'node:assert/strict';
import { test } from 'node:test';
import { researchCapital, researchSpecification, researchInstructions, researchCriterion } from '../../../packages/contracts/src/research.ts';
import { factorViews, factorId } from '../src/factors.ts';
import { Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import research from '../../research/plugins/research.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup, wat } from './support.ts';

const specification = researchSpecification({ version: 1, name: 'Volume hypothesis', question: 'Does volume predict the registered return?', rationale: 'Falsifiable original economic mechanism',
  evaluation: { metric: 'pearson_ic', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'single-factor', maxMembers: 1, stop: 'budget-or-selection' },
  capital: { scenarios: ['1000.000000000000000001'], participation: '0.01', maximumAgeSeconds: 120 } });

test('structured admission captures the mandate, rejects unsupported overrides and retains exact original identity through restart', async () => {
  const state = await setup();
  try {
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
    const input = { requestId: 'structured-original', goal: specification.question, specification, model: state.model, datasetId: 'controlled', maxTrials: 1 };
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input });
    assert.equal(response.statusCode, 202, response.body); const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const detail = (await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json();
    assert.equal(detail.title, specification.name); assert.deepEqual(detail.input.specification, specification); assert.equal(detail.criterion.status, 'insufficient');
    assert.match(researchInstructions(detail.input), /Original structured research mandate/); assert.match(researchInstructions(detail.input), /Volume hypothesis/);
    const factors = (await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json();
    assert.equal(factors.length, 1); assert.equal(factors[0].id, factorId(receipt.conversationId, detail.final.trialId, 0)); assert.equal(factors[0].evidence, 'independent');
    const assumptions = { scenarios: ['20.000000000000000001'], participation: '0.02', maximumAgeSeconds: 60 };
    assert.deepEqual(researchCapital(assumptions), assumptions); assert.throws(() => researchCapital({ ...assumptions, horizon: 9 }), /research_override_unsupported/);
    const assessed = await state.service.app.inject({ method: 'POST', url: `/api/v3/factors/${factors[0].id}/capacity`, headers: state.headers, payload: assumptions }); assert.equal(assessed.statusCode, 200, assessed.body);
    assert.equal(assessed.json().status, 'UNAVAILABLE'); assert.equal(assessed.json().assumptions.participation, '0.02'); assert.equal(assessed.json().scenarios[0].capital, assumptions.scenarios[0]);
    assert.equal((await state.service.app.inject({ method: 'POST', url: `/api/v3/factors/${factors[0].id}/capacity`, headers: state.headers, payload: { ...assumptions, maximumAgeSeconds: 0 } })).statusCode, 400);
    assert.deepEqual((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).json().input, detail.input);
    const body = { requestId: 'atomic-original', name: 'Atomic shared-account configuration', factors: [{ factorId: factors[0].id, weight: '1' }], datasetId: 'controlled', capital: '1000' };
    const admitted = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: body }); assert.equal(admitted.statusCode, 202, admitted.body);
    await state.service.research.harness.waitForTask(admitted.json().taskId, context);
    const portfolio = (await state.service.work.getPortfolio(admitted.json().id)); assert.deepEqual(portfolio.view.factors, body.factors); assert.equal(portfolio.evidence[0].factorId, factors[0].id);
    assert.equal(portfolio.view.capacity?.status, 'UNAVAILABLE'); // Existing test-only protocol cannot measure BAR capacity.
    assert.equal((await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...input, specification: { ...specification, name: 'changed' } } })).statusCode, 409);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input })).json(), receipt);
    assert.equal((await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json()[0].id, factors[0].id);
    assert.equal(state.faux.state.callCount, 1);
    const bad = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...input, requestId: 'unsupported', specification: { ...specification, horizon: 2 } } });
    assert.equal(bad.statusCode, 400); assert.equal(bad.json().error, 'research_override_unsupported');
  } finally { await state.cleanup(); }
});

test('supported search scope is enforced in the original counted trial ledger, not only model prose', async () => {
  const state = await setup();
  try {
    const ensemble = researchSpecification({ ...specification, evaluation: { metric: 'Sharpe Ratio (252 days)', direction: 'maximize', scheme: 'registered-partitions' }, search: { scope: 'ensemble', maxMembers: 2, stop: 'budget-or-selection' } });
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
    const receipt = await state.service.work.startResearch({ requestId: 'scope', goal: ensemble.question, specification: ensemble, model: state.model, datasetId: 'controlled', maxTrials: 1 });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.equal(study.trials.length, 1); assert.equal(study.trials[0]!.status, 'FAILED'); assert.equal(study.trials[0]!.error, 'research_search_scope_exceeded');
  } finally { await state.cleanup(); }
});

test('structured budget-or-selection rejects an unselected model response boundary and continues to an actual selection', async () => {
  const state = await setup();
  try {
    state.options.extensions.push(research); await state.restart();
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Response boundary is not a scientific stop' }), { stopReason: 'toolUse' }),
      transcript => { assert.match(JSON.stringify(transcript), /research_stop_condition_not_met/); return fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' }); },
    ]);
    const receipt = await state.service.work.startResearch({ requestId: 'structured-stop', goal: specification.question, specification, model: state.model, datasetId: 'controlled', maxTrials: 3 });
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.equal(study.trials.length, 1); assert.equal(study.final!.status, 'COMPUTED'); assert.equal(study.final!.trialId, study.trials[0]!.taskId); assert.equal(state.faux.state.callCount, 2);
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Original zero trial budget' }), { stopReason: 'toolUse' })]);
    const zero = await state.service.work.startResearch({ requestId: 'structured-zero-stop', goal: specification.question, specification, model: state.model, datasetId: 'controlled', maxTrials: 0 });
    await (await state.service.research.harness.submission(zero.submissionId, context))!.wait(context);
    assert.equal((await state.service.research.harness.snapshot(Study, zero.conversationId, context))!.final!.reason, 'Original zero trial budget');
  } finally { await state.cleanup(); }
});

test('deterministic criterion never substitutes component diagnostics or missing metrics for a pass', () => {
  const spec = { ...specification, evaluation: { ...specification.evaluation, target: 0.1 } };
  const metric = { key: 'HELDOUT.pearson_ic', label: 'Pearson IC', partition: 'HELDOUT', value: 0.2 };
  assert.equal(researchCriterion(spec, 'COMPUTED', [metric])!.status, 'met');
  assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, value: 0.01 }])!.status, 'not_met');
  assert.equal(researchCriterion(spec, 'COMPUTED', [{ ...metric, component: 0 }])!.status, 'insufficient');
  assert.equal(researchCriterion(spec, 'FAILED', [metric])!.status, 'failed');
  const study = { input: { goal: specification.question, specification, model: { provider: 'test', modelId: 'test' }, data: {}, policy: { max_trials: 1, strategy: { simulationSettings: { base_currency: 'USD' } } } },
    trials: [{ taskId: 17, status: 'COMPUTED', proposal: { hypothesis: 'Controlled projection, not scientific evidence', members: [{ model: wat, weight: '0.25' }, { model: wat, weight: '0.75' }] }, result: {} }], final: { status: 'COMPUTED', trialId: 17, result: {} } };
  const factors = factorViews(3, { ...study, input: researchInput(study.input) }); assert.deepEqual(factors.map(row => row.id), ['r3.t17.m0', 'r3.t17.m1']);
  assert.ok(factors.every(row => row.evidence === 'component_only' && !row.combinable && row.metrics.length === 0));
});
