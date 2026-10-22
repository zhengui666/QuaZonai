import assert from 'node:assert/strict';
import { test } from 'node:test';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { TaskId } from '@earendil-works/pi-durable';
import { Type } from '@earendil-works/pi-ai';
import type { CapabilityDescriptor, DecisionCycle, PipelineAssessment, PmMandateView, PortfolioDecision, ResearchCampaign, StrategyVersion } from '../../../packages/contracts/src/pm.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';
import research from '../../research/plugins/research.ts';
import { evaluateTrial } from '../../research/src/trials.ts';
import { registerHypothesis } from '../../research/src/pm-ledger.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';

// Actual Fastify, owner authentication and official SQLite/Durable admission.
// No route handler, work function or positive scientific result is mocked.
async function fixture() {
  const state = await setup();
  state.options.config.datasets[0]!.data.request = { train_end_ns: '140', validation_end_ns: '180', forecast: {
    schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200',
      bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 }, parameters: {},
  } };
  await state.restart();
  const start = async (requestId: string, maxTrials = 1) => {
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { requestId, goal: `Original ${requestId} evidence scope`, datasetId: 'controlled', model: state.model,
        funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, maxTrials } });
    assert.equal(response.statusCode, 202, response.body);
    return response.json<PmMandateView>();
  };
  const read = async <T>(path: string): Promise<T> => {
    const response = await state.service.app.inject({ url: `/api/v3${path}`, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body); return response.json<T>();
  };
  return { state, start, read };
}
const resource = encodeURIComponent;

test('PM routes: protected resource reads preserve admitted campaign and strategy identities without generating decisions', { timeout: 30_000 }, async () => {
  const { state, start, read } = await fixture();
  try {
    const first = await start('route-first'); const second = await start('route-second');
    const campaign = first.campaigns[0]!; const strategy = first.strategies[0]!;
    const capabilities = await read<CapabilityDescriptor[]>('/capabilities');
    assert.deepEqual(capabilities, first.capabilities);
    const stored = await read<ResearchCampaign>(`/campaigns/${resource(campaign.id)}`);
    assert.equal(stored.id, campaign.id); assert.equal(stored.mandateRevisionId, first.mandate.id);
    assert.equal(stored.strategyDefinitionId, strategy.definitionId);
    assert.deepEqual(await read(`/campaigns/${resource(campaign.id)}/trials`), []);
    assert.equal(await read(`/campaigns/${resource(campaign.id)}/decision`), null);
    const versions = await read<StrategyVersion[]>(`/strategies/${resource(strategy.id)}/versions`);
    assert.equal(versions.length, 1); assert.equal(versions[0]!.id, strategy.id);
    assert.ok(versions.every(version => version.mandateRevisionId === first.mandate.id));
    assert.deepEqual(await read(`/strategies/${resource(strategy.definitionId)}/versions`), versions);
    const assessments = await read<PipelineAssessment[]>(`/strategies/${resource(strategy.id)}/assessments`);
    assert.ok(assessments.length > 0); assert.ok(assessments.every(assessment => assessment.strategyVersionId === strategy.id));
    assert.equal(await read(`/mandates/${resource(first.mandate.id)}/decision`), null);
    assert.deepEqual(await read(`/mandates/${resource(first.mandate.id)}/decisions`), []);
    assert.equal(await read(`/mandates/${resource(second.mandate.id)}/decision`), null);
    const after = await read<PmMandateView>(`/mandates/${resource(first.mandate.id)}`);
    assert.deepEqual(after.history, []); assert.deepEqual(after.cycles, []);
    assert.ok(after.campaigns.every(item => item.id !== second.campaigns[0]!.id));
  } finally { await state.cleanup(); }
});

async function activeControlFixture(maxTrials: number) {
  const f = await fixture();
  let entered!: () => void;
  const waiting = new Promise<void>(resolve => { entered = resolve; });
  const probe = defineExtension({ name: 'test.pm-control-probe', tools: [defineTool({
    name: 'pm_control_probe', description: 'Protocol ledger and cancellation probe, no performance evidence.', parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, ctx) {
      const hypothesis = await registerHypothesis(api, ctx, { economicFamilyId: 'protocol:control', mechanism: 'Exercise original task ownership.',
        prediction: 'Owner control preserves the original ledger.', falsification: 'Recorded attempts disappear or execute twice.', requiredDataCapabilities: [], incrementalQuestion: 'Does pause retain the original attempt?' });
      await evaluateTrial(api, ctx, { hypothesisId: hypothesis.id, hypothesis: 'Only a control protocol observation.' }, async () => ({ protocolOnly: true }));
      entered();
      await new Promise<never>((_resolve, reject) => {
        const stop = () => reject(ctx.abortSignal?.reason ?? new Error('protocol_aborted'));
        if (ctx.abortSignal?.aborted) stop(); else ctx.abortSignal?.addEventListener('abort', stop, { once: true });
      });
      return { content: [{ type: 'text', text: 'Unreachable after owner cancellation.' }] };
    },
  })] });
  f.state.options.extensions = [research, probe]; await f.state.restart();
  f.state.faux.setResponses([
    fauxAssistantMessage(fauxToolCall('strategy_intake', { name: 'Control protocol', archetype: 'PREDICTIVE_ALPHA',
      economicMechanism: 'Only test owner control of a research protocol.', falsificationConditions: ['Original trial ledger changes.'],
      requiredCapabilities: [], intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['protocol_only'], ambiguities: [], ownerBoundaries: [] }), { stopReason: 'toolUse' }),
    fauxAssistantMessage(fauxToolCall('pm_control_probe', {}), { stopReason: 'toolUse' }),
  ]);
  const admitted = await f.start(`control-${maxTrials}`, maxTrials); await waiting;
  const current = await f.read<PmMandateView>(`/mandates/${resource(admitted.mandate.id)}`);
  return { ...f, view: current, campaign: current.campaigns[0]! };
}

async function controlRecord(state: Awaited<ReturnType<typeof setup>>, mandateId: string, key: string) {
  const doc = pmState(await state.service.research.harness.snapshot(PmDocument, context));
  return doc.mandates[mandateId]!.artifacts[key]!;
}
async function control(f: Awaited<ReturnType<typeof activeControlFixture>>, action: 'pause' | 'resume' | 'close', requestId: string, strategy = false) {
  const id = strategy ? f.view.strategies[0]!.id : f.campaign.id;
  const path = strategy ? 'strategies' : 'campaigns';
  const payload = { requestId, expectedRevision: f.view.mandate.revision };
  const response = await f.state.service.app.inject({ method: 'POST', url: `/api/v3/${path}/${resource(id)}/${action}`, headers: f.state.headers, payload });
  assert.equal(response.statusCode, 202, response.body);
  const key = `campaign-command:${f.campaign.id}:${strategy ? id : ''}:${action}:${requestId}`;
  const record = await controlRecord(f.state, f.view.mandate.id, key); assert.equal(typeof record.taskId, 'number');
  const task = await f.state.service.research.harness.waitForTask(record.taskId as TaskId, context);
  return { key, task, record: await controlRecord(f.state, f.view.mandate.id, key), response, payload, url: `/api/v3/${path}/${resource(id)}/${action}` };
}

test('PM controls: pause owns a separate durable task, resumes the original study, and close preserves its ledger', { timeout: 30_000 }, async () => {
  const f = await activeControlFixture(3);
  try {
    const original = (await f.state.service.research.state(f.campaign.runtime.conversationId)).research!;
    assert.equal(original.trials.length, 1); assert.equal(original.trials[0]!.status, 'COMPUTED');
    const paused = await control(f, 'pause', 'pause-original');
    assert.equal(paused.record.state, 'APPLIED'); assert.notEqual(paused.task.conversationId, f.campaign.runtime.conversationId);
    assert.equal(paused.task.state.status === 'terminal' && paused.task.state.outcome.status, 'completed');
    assert.deepEqual((await f.state.service.research.state(f.campaign.runtime.conversationId)).research!.trials, original.trials);
    const replay = await f.state.service.app.inject({ method: 'POST', url: paused.url, headers: f.state.headers, payload: paused.payload });
    assert.equal(replay.statusCode, 202); assert.equal((await controlRecord(f.state, f.view.mandate.id, paused.key)).taskId, paused.record.taskId);
    f.state.faux.setResponses([fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 controlled unavailable provider' })]);
    const resumed = await control(f, 'resume', 'resume-original'); assert.equal(resumed.record.state, 'APPLIED');
    const resumedState = await f.state.service.research.state(f.campaign.runtime.conversationId);
    await (await f.state.service.research.harness.submission(resumedState.submission!.id, context))!.wait(context);
    assert.deepEqual((await f.state.service.research.state(f.campaign.runtime.conversationId)).research!.trials, original.trials);
    const closed = await control(f, 'close', 'close-original'); assert.equal(closed.record.state, 'APPLIED');
    const final = (await f.state.service.research.state(f.campaign.runtime.conversationId)).research!;
    assert.deepEqual(final.trials, original.trials); assert.ok(final.final); assert.equal(final.final.trialId, null);
    const strategyPause = await control(f, 'pause', 'pause-concluded-strategy', true); assert.equal(strategyPause.record.state, 'APPLIED');
    assert.deepEqual((await f.state.service.research.state(f.campaign.runtime.conversationId)).research!.trials, original.trials);
    await f.state.restart();
    assert.equal((await controlRecord(f.state, f.view.mandate.id, strategyPause.key)).state, 'APPLIED');
  } finally { await f.state.cleanup(); }
});

test('PM controls: an exhausted-budget resume reports its actual failed task instead of claiming a restarted campaign', { timeout: 30_000 }, async () => {
  const f = await activeControlFixture(1);
  try {
    const original = (await f.state.service.research.state(f.campaign.runtime.conversationId)).research!.trials;
    assert.equal((await control(f, 'pause', 'pause-exhausted')).record.state, 'APPLIED');
    const resumed = await control(f, 'resume', 'resume-exhausted');
    assert.equal(resumed.record.state, 'FAILED'); assert.equal(resumed.record.reason, 'trial_budget_exhausted');
    assert.equal(resumed.task.state.status === 'terminal' && resumed.task.state.outcome.status, 'failed');
    const current = await f.read<PmMandateView>(`/mandates/${resource(f.view.mandate.id)}`);
    assert.ok(current.gaps.some(gap => gap.requiredCapability === 'campaign_control' && gap.reasonCode === 'trial_budget_exhausted'));
    assert.deepEqual((await f.state.service.research.state(f.campaign.runtime.conversationId)).research!.trials, original);
    const retry = await f.state.service.app.inject({ method: 'POST', url: resumed.url, headers: f.state.headers, payload: resumed.payload });
    assert.equal(retry.statusCode, 202); assert.equal((await controlRecord(f.state, f.view.mandate.id, resumed.key)).taskId, resumed.record.taskId);
  } finally { await f.state.cleanup(); }
});

test('PM routes: real recompute exposes its exact durable cycle and decision only in its owning mandate', { timeout: 30_000 }, async () => {
  const { state, start, read } = await fixture();
  try {
    const first = await start('route-cycle'); const other = await start('route-no-cycle');
    const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${resource(first.mandate.id)}/recompute`, headers: state.headers,
      payload: { requestId: 'original-cycle-command', expectedRevision: first.mandate.revision } });
    assert.equal(response.statusCode, 202, response.body);
    const admitted = response.json<PmMandateView>(); assert.ok(admitted.lastCycle?.runtime.taskId);
    await state.service.research.harness.waitForTask(admitted.lastCycle.runtime.taskId as TaskId, context);
    const current = await read<PmMandateView>(`/mandates/${resource(first.mandate.id)}`);
    assert.ok(current.currentDecision); assert.ok(current.lastCycle);
    assert.equal(current.currentDecision.action, 'NEEDS_EVIDENCE', 'The controlled source does not supply an investment decision.');
    const cycle = await read<DecisionCycle>(`/cycles/${resource(current.lastCycle.id)}`);
    const decision = await read<PortfolioDecision>(`/mandates/${resource(first.mandate.id)}/decision`);
    const history = await read<PortfolioDecision[]>(`/mandates/${resource(first.mandate.id)}/decisions`);
    assert.equal(cycle.id, current.lastCycle.id); assert.equal(cycle.mandateRevisionId, first.mandate.id);
    assert.equal(cycle.outputDecisionId, decision.id); assert.equal(decision.cycleId, cycle.id);
    assert.equal(decision.id, current.currentDecision.id); assert.deepEqual(history, current.history);
    assert.equal(await read(`/mandates/${resource(other.mandate.id)}/decision`), null);
    assert.deepEqual(await read(`/mandates/${resource(other.mandate.id)}/decisions`), []);
    await state.restart();
    assert.deepEqual(await read(`/cycles/${resource(cycle.id)}`), cycle);
    assert.deepEqual(await read(`/mandates/${resource(first.mandate.id)}/decision`), decision);
  } finally { await state.cleanup(); }
});

test('PM routes: missing typed identities return actual 404 and every read and command stays owner authenticated', { timeout: 30_000 }, async () => {
  const { state, start } = await fixture();
  try {
    const admitted = await start('route-authorization');
    const missing = resource('absent:original-resource');
    const paths = ['/capabilities', `/mandates/${missing}/decision`, `/mandates/${missing}/decisions`, `/cycles/${missing}`,
      `/capacity-estimates/${missing}`, `/pm-evidence/${missing}`, `/campaigns/${missing}`, `/campaigns/${missing}/trials`, `/campaigns/${missing}/decision`,
      `/strategies/${missing}/versions`, `/strategies/${missing}/assessments`];
    for (const path of paths) {
      const unauthorized = await state.service.app.inject({ url: `/api/v3${path}` });
      assert.equal(unauthorized.statusCode, 401, `${path}: ${unauthorized.body}`);
      if (path === '/capabilities') continue;
      const response = await state.service.app.inject({ url: `/api/v3${path}`, headers: state.headers });
      assert.equal(response.statusCode, 404, `${path}: ${response.body}`);
      assert.match(response.json().error, /not_found$/, `${path} must reach its real typed resource lookup.`);
    }
    // A real ID of another entity type is not permission to return an unrelated object.
    const wrongKind = await state.service.app.inject({ url: `/api/v3/cycles/${resource(admitted.mandate.id)}`, headers: state.headers });
    assert.equal(wrongKind.statusCode, 404);
    for (const path of ['/campaigns', ...['close', 'pause', 'resume'].map(action => `/campaigns/${missing}/${action}`),
      ...['pause', 'resume'].map(action => `/strategies/${missing}/${action}`)]) {
      const payload = { requestId: 'original-command', expectedRevision: '0', ...(path === '/campaigns' ? { mandateId: 'absent:mandate' } : path.endsWith('/resume') ? { predecessorReceiptId: 'absent:receipt' } : { reason: 'Owner request' }) };
      const unauthorized = await state.service.app.inject({ method: 'POST', url: `/api/v3${path}`, payload });
      assert.equal(unauthorized.statusCode, 401, `${path}: ${unauthorized.body}`);
      const response = await state.service.app.inject({ method: 'POST', url: `/api/v3${path}`, payload, headers: state.headers });
      assert.equal(response.statusCode, 404, `${path}: ${response.body}`);
      assert.match(response.json().error, /not_found$/);
    }
  } finally { await state.cleanup(); }
});
