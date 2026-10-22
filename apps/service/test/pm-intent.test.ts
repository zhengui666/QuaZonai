import assert from 'node:assert/strict';
import { test } from 'node:test';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { Type } from '@earendil-works/pi-ai';
import type { ConversationId, JsonObject } from '@earendil-works/pi-durable';
import type { PmMandateView } from '../../../packages/contracts/src/pm.ts';
import type { PmIntentClarification } from '../../../packages/contracts/src/pm-intent.ts';
import research from '../../research/plugins/research.ts';
import { CampaignResearchDoc } from '../../research/src/pm-campaign.ts';
import { registerHypothesis } from '../../research/src/pm-ledger.ts';
import { evaluateTrial } from '../../research/src/trials.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { pmRecord, pmState } from '../src/pm-facts.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

// This exercises real authenticated HTTP and official Durable facts. The one
// explicitly marked protocol attempt proves admission/accounting, not returns,
// native no-borrow enforcement or investment qualification.
const probe = defineExtension({ name: 'test.pm-intent-protocol', tools: [defineTool({
  name: 'pm_intent_protocol_attempt', description: 'A protocol-only attempt with no scientific or economic qualification.',
  parameters: Type.Object({}), replay: 'safe',
  async execute(_args, api, ctx) {
    const hypothesis = await registerHypothesis(api, ctx, { economicFamilyId: 'protocol:intent-recovery',
      mechanism: 'Verify authenticated clarification restores permitted research admission.',
      prediction: 'The original shared budget records one attempt.', falsification: 'An unresolved constraint is ignored or the budget resets.',
      requiredDataCapabilities: [], incrementalQuestion: 'Can the clarified campaign preserve its inherited constraints and ledger?' });
    await evaluateTrial(api, ctx, { hypothesisId: hypothesis.id, hypothesis: 'Only a protocol observation.' }, async () => ({ protocolOnly: true }));
    return { content: [{ type: 'text', text: 'Protocol attempt recorded; no performance or no-borrow evidence.' }] };
  },
})] });
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function intake(boundaries: JsonObject[] = []) {
  return call('strategy_intake', { name: 'Original constrained account research', archetype: 'PREDICTIVE_ALPHA',
    economicMechanism: 'Research causal bar factors for the original account objective.',
    falsificationConditions: ['Matched net account evidence does not support the original objective.'], requiredCapabilities: [],
    intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['fees', 'unsupported_constraints'],
    ambiguities: [], ownerBoundaries: boundaries });
}
function finish() { return call('research_finish', { reason: 'The current intent and protocol evidence are complete; preserve any remaining gaps.' }); }
async function fixture() {
  const state = await setup();
  state.options.extensions = [research, probe];
  state.options.config.datasets[0]!.data.request = { schema_version: 1, train_end_ns: '100', validation_end_ns: '200',
    forecast: { schema_version: 1, selection: { schema_version: 1, event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300',
      bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 }, parameters: {} } };
  await state.restart();
  return state;
}
type Fixture = Awaited<ReturnType<typeof fixture>>;
const route = (id: string) => `/api/v3/mandates/${encodeURIComponent(id)}/intent-clarifications`;
async function read(state: Fixture, id: string) {
  const response = await state.service.app.inject({ url: `/api/v3/mandates/${encodeURIComponent(id)}`, headers: state.headers });
  assert.equal(response.statusCode, 200, response.body); return response.json<PmMandateView>();
}
async function start(state: Fixture, goal: string, requestId: string) {
  const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: {
    // One owner-authorized protocol attempt gives the accepted research a real
    // finite stop condition. This budget cannot qualify a native PM strategy.
    requestId, goal, datasetId: 'controlled', model: state.model, maxTrials: 1, businessPreference: 'NET_RETURN',
    funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, operatingAuthority: 'RESEARCH_ONLY' } });
  assert.equal(response.statusCode, 202, response.body); return response.json<PmMandateView>();
}
async function settled(state: Fixture, id: string, campaignCount: number) {
  const deadline = Date.now() + 25_000;
  while (Date.now() < deadline) {
    const view = await read(state, id);
    if (view.campaigns.length === campaignCount && view.campaigns.at(-1)!.state === 'CONCLUDED'
      && view.currentDecision?.cycleId === view.lastCycle?.id && ['CONCLUDED', 'WAITING_EVIDENCE'].includes(view.lastCycle?.state ?? '')) return view;
    await new Promise(resolve => setTimeout(resolve, 40));
  }
  const view = await read(state, id);
  const research = await Promise.all(view.campaigns.filter(campaign => campaign.runtime.conversationId > 0).map(async campaign => ({
    campaignId: campaign.id, state: campaign.state,
    research: await state.service.research.state(campaign.runtime.conversationId),
    diagnostics: await state.service.research.diagnostics(campaign.runtime.conversationId),
  })));
  assert.fail(JSON.stringify({ reason: 'clarified_campaign_did_not_settle', revision: view.mandate.revision,
    lastCycle: view.lastCycle, currentDecision: view.currentDecision, gaps: view.gaps, research }));
}
async function clarify(state: Fixture, id: string, payload: PmIntentClarification) {
  return state.service.app.inject({ method: 'POST', url: route(id), headers: state.headers, payload });
}
async function campaignState(state: Fixture, view: PmMandateView, index: number) {
  return (await state.service.research.harness.snapshot(CampaignResearchDoc, view.campaigns[index]!.runtime.conversationId as ConversationId, context))!;
}

test('PM intent HTTP: authenticated clarification restores research without deleting constraints, history, attempts or prior exposure', { timeout: 85_000 }, async () => {
  const state = await fixture();
  try {
    const phrase = 'Keep leverage restrained.';
    const goal = `Research predictive alpha and compare net account outcomes. ${phrase}`;
    state.faux.setResponses([intake([{ kind: 'LEVERAGE', sourceStatement: phrase }]), call('pm_intent_protocol_attempt', {}), finish()]);
    const admitted = await start(state, goal, 'intent-original-goal');
    const before = await settled(state, admitted.mandate.id, 1);
    assert.equal(before.researchTrials.filter(trial => trial.countedAttempt).length, 0);
    assert.ok(before.gaps.some(gap => gap.reasonCode === 'owner_boundary_normalization_required' && gap.state !== 'RESOLVED'));
    assert.equal(before.currentDecision!.action, 'NEEDS_EVIDENCE');
    const priorDecision = structuredClone(before.currentDecision!);
    const priorStudy = await campaignState(state, before, 0);
    const originalRecord = pmRecord(pmState(await state.service.pm.snapshot()), before.mandate.id);
    const originalBudget = structuredClone(originalRecord.budget);
    const originalFamily = before.searchFamily.id;
    const initialExposure = (await state.service.research.harness.snapshot(Exposures, context))?.windows ?? [];
    const request: PmIntentClarification = { requestId: 'clarify-original-restraint', expectedRevision: before.mandate.revision,
      statement: 'By restrained leverage I mean gross exposure at most one times NAV and no borrowing.',
      boundaries: [{ kind: 'LEVERAGE', sourceStatement: phrase, value: { maximumGrossNavRatio: '1', borrowingAllowed: false } }], resolvedAmbiguities: [] };
    // The model deliberately omits the boundary: an authenticated constraint
    // must survive that omission, rather than rely on another model promise.
    state.faux.setResponses([intake(), call('pm_intent_protocol_attempt', {}), finish()]);
    const response = await clarify(state, before.mandate.id, request);
    assert.equal(response.statusCode, 202, response.body);
    const clarified = await settled(state, before.mandate.id, 2);
    assert.equal(clarified.mandate.ownerGoal, goal);
    assert.notEqual(clarified.activePolicy.id, before.activePolicy.id);
    assert.notEqual(clarified.lastCycle!.basisRevision, before.lastCycle!.basisRevision);
    assert.equal(clarified.mandate.researchBudgetRef, before.mandate.researchBudgetRef);
    assert.equal(clarified.searchFamily.id, originalFamily);
    assert.equal(clarified.researchTrials.filter(trial => trial.countedAttempt).length, 1);
    assert.equal(clarified.searchFamily.countedAttempts, '1');
    assert.ok(clarified.searchFamily.methodVariantRefs.includes(before.activePolicy.id));
    assert.ok(clarified.searchFamily.methodVariantRefs.includes(clarified.activePolicy.id));
    assert.deepEqual(clarified.mandate.ownerBoundaries.map(boundary => ({ kind: boundary.kind, sourceStatement: boundary.sourceStatement, value: boundary.value })), request.boundaries);
    assert.deepEqual((clarified.activePolicy.validDomain as JsonObject).ownerBoundaries, clarified.mandate.ownerBoundaries);
    const nextStudy = await campaignState(state, clarified, 1);
    assert.deepEqual(nextStudy.intake!.ownerBoundaries, clarified.mandate.ownerBoundaries);
    assert.ok(nextStudy.binding!.ownerClarificationStatements!.includes(request.statement));
    assert.deepEqual(await campaignState(state, clarified, 0), priorStudy, 'Clarification must not rewrite the previous campaign or interpretation.');
    const retainedDecision = clarified.history.find(decision => decision.id === priorDecision.id)!;
    assert.equal(retainedDecision.status, 'SUPERSEDED');
    assert.equal(retainedDecision.policyRevisionId, priorDecision.policyRevisionId);
    assert.deepEqual(retainedDecision.accountBasis, priorDecision.accountBasis);
    assert.deepEqual((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? [], initialExposure);
    const afterRecord = pmRecord(pmState(await state.service.pm.snapshot()), before.mandate.id);
    assert.deepEqual(afterRecord.budget, originalBudget, 'A clarification cannot replenish or replace the original research budget.');

    const replay = await clarify(state, before.mandate.id, request);
    assert.equal(replay.statusCode, 202, replay.body);
    assert.equal(replay.json<PmMandateView>().campaigns.length, 2);
    assert.equal(replay.json<PmMandateView>().activePolicy.id, clarified.activePolicy.id);
    const conflict = await clarify(state, before.mandate.id, { ...request, statement: 'A different command under the original receipt.' });
    assert.equal(conflict.statusCode, 409, conflict.body);
    const stale = await clarify(state, before.mandate.id, { ...request, requestId: 'stale-owner-intent' });
    assert.equal(stale.statusCode, 409, stale.body);
    assert.equal((await state.service.app.inject({ method: 'POST', url: route(before.mandate.id), payload: request })).statusCode, 401);

    // Tightening after a real admitted attempt must retain the nonempty prior
    // budget and research exposure records, including their original policy ID.
    const spent = structuredClone(clarified.researchTrials.filter(trial => trial.countedAttempt));
    const exposed = structuredClone(nextStudy.exposureRecords);
    assert.ok(exposed.length > 0);
    const tighter: PmIntentClarification = { requestId: 'tighten-original-restraint', expectedRevision: clarified.mandate.revision,
      statement: 'Tighten the gross exposure cap to 75% of NAV; borrowing remains prohibited.',
      boundaries: [{ kind: 'LEVERAGE', sourceStatement: phrase, value: { maximumGrossNavRatio: '0.75', borrowingAllowed: false } }], resolvedAmbiguities: [] };
    const tightenedResponse = await clarify(state, before.mandate.id, tighter);
    assert.equal(tightenedResponse.statusCode, 202, tightenedResponse.body);
    const tightened = tightenedResponse.json<PmMandateView>();
    assert.equal(tightened.campaigns.length, 3);
    assert.equal(tightened.campaigns.at(-1)!.state, 'PAUSED');
    assert.equal(tightened.campaigns.at(-1)!.conclusionReason, 'intent_revision_requires_new_unexposed_data',
      'Changed investment intent cannot wash an already observed selection into a new independent research window.');
    assert.ok(tightened.gaps.some(gap => gap.reasonCode === 'intent_revision_requires_new_unexposed_data' && gap.permittedRemedy === 'COLLECT_AUTHORIZED_DATA'));
    assert.deepEqual(tightened.researchTrials.filter(trial => trial.countedAttempt), spent);
    assert.deepEqual((await campaignState(state, tightened, 1)).exposureRecords, exposed);
    assert.equal(tightened.searchFamily.countedAttempts, '1');
    assert.deepEqual(tightened.mandate.ownerBoundaries[0]!.value, { maximumGrossNavRatio: '0.75', borrowingAllowed: false });
    const relaxed = await clarify(state, before.mandate.id, { ...tighter, requestId: 'cannot-relax-original-cap', expectedRevision: tightened.mandate.revision,
      boundaries: [{ kind: 'LEVERAGE', sourceStatement: phrase, value: { maximumGrossNavRatio: '1', borrowingAllowed: false } }] });
    assert.ok([400, 409].includes(relaxed.statusCode), relaxed.body);
    assert.deepEqual((await read(state, before.mandate.id)).mandate.ownerBoundaries, tightened.mandate.ownerBoundaries);
    await state.restart();
    const restored = await read(state, before.mandate.id);
    assert.equal(restored.mandate.ownerGoal, goal);
    assert.deepEqual(restored.mandate.ownerBoundaries, tightened.mandate.ownerBoundaries);
    assert.deepEqual(restored.researchTrials.filter(trial => trial.countedAttempt), spent);
    assert.ok(restored.history.every(decision => decision.targetCommandId === null));
  } finally { await state.cleanup(); }
});

test('PM intent HTTP: recording a future drawdown boundary cannot manufacture execution capability or a guarantee', { timeout: 45_000 }, async () => {
  const state = await fixture();
  try {
    const phrase = 'Guarantee future drawdown never exceeds two percent.';
    state.faux.setResponses([intake([{ kind: 'LOSS', sourceStatement: phrase }]), call('pm_intent_protocol_attempt', {}), finish()]);
    const admitted = await start(state, `Research predictive alpha for the account. ${phrase}`, 'intent-unsupported-guarantee');
    const before = await settled(state, admitted.mandate.id, 1);
    assert.equal(before.researchTrials.filter(trial => trial.countedAttempt).length, 0);
    const payload: PmIntentClarification = { requestId: 'unsupported-future-guarantee', expectedRevision: before.mandate.revision,
      statement: phrase, boundaries: [{ kind: 'LOSS', sourceStatement: phrase, value: { metricKey: 'account.maximum_drawdown',
        metricDefinitionId: 'qz.metric.maximum_drawdown/1', limit: 0.02, unit: 'LOSS_RATIO', horizonRef: 'EVALUATION_INTERVAL' } }], resolvedAmbiguities: [] };
    state.faux.setResponses([intake(), finish()]);
    const response = await clarify(state, admitted.mandate.id, payload);
    assert.equal(response.statusCode, 202, response.body);
    const after = await settled(state, admitted.mandate.id, 2);
    assert.equal(after.mandate.ownerGoal, before.mandate.ownerGoal);
    assert.notEqual(after.activePolicy.id, before.activePolicy.id);
    assert.deepEqual(after.mandate.ownerBoundaries.map(boundary => ({ kind: boundary.kind, sourceStatement: boundary.sourceStatement, value: boundary.value })), payload.boundaries);
    assert.equal(after.researchTrials.filter(trial => trial.countedAttempt).length, 0);
    assert.ok(after.gaps.some(gap => gap.state !== 'RESOLVED' && gap.reasonCode === 'owner_boundary_execution_unavailable' && gap.permittedRemedy === 'UNSUPPORTED'));
    assert.equal(after.currentDecision!.action, 'NEEDS_EVIDENCE');
    assert.equal(after.currentDecision!.targetCommandId, null);
    assert.ok(!after.assessments.some(assessment => ['SHADOW', 'PAPER', 'TARGET_OPERATION'].includes(assessment.stage) && assessment.outcome === 'READY'));
  } finally { await state.cleanup(); }
});
