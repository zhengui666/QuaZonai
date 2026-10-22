import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import type { ResearchInput } from '../src/documents.ts';
import { evaluateTrial, trialFeedback } from '../src/trials.ts';
import { CampaignResearchDoc, fact } from '../src/pm-campaign.ts';
import type { ResearchCampaignBinding } from '../src/pm-campaign.ts';
import research from '../plugins/research.ts';
import heldout from '../plugins/heldout-wasm.ts';
import { researchInput } from '../src/input.ts';

const context = BACKGROUND_CONTEXT;
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function last(transcript: { messages: readonly unknown[] }, name: string) {
  const entry = transcript.messages.filter(value => (value as { toolName?: string }).toolName === name).at(-1) as { content: { text?: string }[] };
  return JSON.parse(entry.content.map(item => item.text ?? '').join('')) as JsonObject;
}
function binding(maximumTrials: number, frontier = false): ResearchCampaignBinding {
  const definition = { id: 'test.ic/1', key: 'factor.pearson_ic', definitionVersion: '1', label: 'Protocol fixture Pearson IC', unit: 'DIMENSIONLESS' as const,
    valueEncoding: 'FLOAT64' as const, direction: 'MAXIMIZE' as const, aggregation: 'PEARSON', calendarId: null, annualizationDays: null, horizonRef: 'one-bar', requiredInputs: ['prediction', 'mature-label'] };
  return { campaignId: 'test-campaign', mandateRevisionId: 'test-mandate', protocolRef: 'test-protocol', datasetRevisionIds: ['test-dataset'],
    policy: { ...fact('test-policy', '0'), mandateRevisionId: 'test-mandate', methodFamily: 'protocol-test', nativeImplementationVersion: 'test',
      learnedParametersRef: null, trainingProtocolRef: 'test-protocol', validationEvidenceRefs: [], applicableCapabilityIds: [], validDomain: {}, numericalToleranceRef: 'test-exact',
      selectionRule: { mode: frontier ? 'FRONTIER_ONLY' : 'LEXICOGRAPHIC', objectives: [{ metricKey: definition.key, metricDefinitionId: definition.id, direction: 'MAXIMIZE', horizonRef: 'one-bar', scope: 'FORECAST' }],
        utilityModelRef: null, establishedBy: 'OWNER_GOAL', rationale: 'Protocol-only fixture; no market performance assertion' }, limitations: ['test-only'] },
    budget: { ...fact('test-budget', '0'), maximumTrials, nativeComputeSeconds: null, modelSpend: null, dataBytes: null, reservationForClosureRef: 'test-closure', usageEvidenceRefs: [] },
    scopes: [{ id: 'test-scope', allowedUniverseRef: 'test-universe', datasetRevisionIds: ['test-dataset'], interval: { startNs: '10', endExclusiveNs: '20' }, role: 'SELECTION', calendarId: 'UTC', labelDefinitionId: 'one-bar', accountingPolicyRef: null, costModelRef: null, startingStateRef: null, capital: null }],
    definitions: [definition], frontierDefinitionIds: [definition.id] };
}
const hypothesis = () => call('research_hypothesis', { economicFamilyId: 'protocol-family', mechanism: 'Protocol fixture mechanism', prediction: 'Fixture scalar orders candidates', falsification: 'Fixture can return a failed receipt', requiredDataCapabilities: [], incrementalQuestion: 'Recover and compare the original ledger' });
function trial(value: number, fail = false) { return (transcript: { messages: readonly unknown[] }) => call('pm_trial', { hypothesisId: last(transcript, 'research_hypothesis').id!, value, fail }); }
async function fixture(maximumTrials: number, frontier = false, sealed = false) {
  const directory = await mkdtemp(join(tmpdir(), 'qz-pm-protocol-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  let computed = 0;
  let paused = false; let computePaused = false; let committed!: () => void; let enteredCompute!: () => void;
  const afterCommit = new Promise<void>(resolve => { committed = resolve; });
  const duringCompute = new Promise<void>(resolve => { enteredCompute = resolve; });
  const science = defineExtension({ name: 'test.pm.science', tools: [defineTool({ name: 'pm_trial', description: 'Test-only ledger evidence, never market data',
    parameters: Type.Object({ hypothesisId: Type.String(), value: Type.Number(), fail: Type.Boolean() }), replay: 'safe', executionMode: 'sequential',
    async execute(args, api, ctx) { return trialFeedback(api, ctx, async () => { const result = await evaluateTrial(api, ctx, { hypothesisId: args.hypothesisId, hypothesis: 'Protocol fixture', model: '(module)', purpose: 'FEASIBILITY' }, async () => {
      computed++; if (args.fail) throw new Error('protocol_compute_failed');
      if (computePaused) { enteredCompute(); await awaitWithContext(new Promise<void>(() => {}), ctx); }
      return { partitions: [{ partition: 'VALIDATION', start_ns: '10', end_ns: '20', metric: { pearson_ic: { status: 'AVAILABLE', value: args.value }, counts: { observations: '10', included: '10' } } }] };
    }); if (paused) { committed(); await awaitWithContext(new Promise<void>(() => {}), ctx); } return result; }); },
  })] });
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [research, science, heldout] };
  let host = await openResearch(options);
  const input: ResearchInput = { goal: 'Protocol-only evidence; do not claim alpha', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: sealed ? { sealed: { marker: 'MUST_NOT_BE_READ' } } : {}, policy: { max_trials: maximumTrials }, campaign: binding(maximumTrials, frontier) };
  return { faux, input, get host() { return host; }, get computed() { return computed; }, async reopen() { await host.close(); host = await openResearch(options); },
    pauseAfterCommit() { paused = true; return afterCommit; }, resumeAfterCommit() { paused = false; },
    pauseDuringCompute() { computePaused = true; return duringCompute; },
    async close() { await host.close(); await rm(directory, { recursive: true, force: true }); } };
}

test('budget closes after the last allowed candidate and deterministically selects it without another model turn', async () => {
  const f = await fixture(2);
  try {
    f.faux.setResponses([hypothesis(), trial(0.1), trial(0.7), fauxAssistantMessage('The admitted experiments are complete.')]);
    const receipt = await f.host.start('two', f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId); const campaign = state.campaignResearch!;
    assert.equal(state.research!.final!.trialId, state.research!.trials[1]!.taskId, JSON.stringify(state));
    assert.equal(campaign.state, 'CONCLUDED'); assert.equal(campaign.comparisons.at(-1)!.recommendationStatus, 'SELECTED');
    assert.equal(campaign.trials.filter(trial => trial.countedAttempt).length, 2);
    assert.equal(campaign.budgetMovements.filter(item => item.resource === 'TRIAL').length, 2);
    assert.equal(campaign.resourceMeasurements.filter(item => item.resource === 'MODEL_COST').every(item => item.status === 'UNAVAILABLE' && item.value === null), true);
    assert.equal(f.computed, 2); assert.equal(f.faux.state.callCount, 4);
    await f.reopen(); assert.deepEqual(await f.host.start('two', f.input), receipt);
    assert.deepEqual((await f.host.state(receipt.conversationId)).campaignResearch, campaign); assert.equal(f.computed, 2);
  } finally { await f.close(); }
});

test('suspension after scientific commit restores one settled Study and Campaign receipt without duplicate consumption', async () => {
  const f = await fixture(1);
  try {
    const committed = f.pauseAfterCommit();
    f.faux.setResponses([hypothesis(), trial(0.6), fauxAssistantMessage('Finish the original candidate budget.')]);
    const receipt = await f.host.start('atomic-settlement', f.input); await committed;
    const before = await f.host.state(receipt.conversationId);
    assert.equal(before.research!.trials[0]!.status, 'COMPUTED');
    assert.equal(before.campaignResearch!.trials[0]!.state, 'COMPUTED');
    const usage = before.campaignResearch!.budgetMovements;
    f.resumeAfterCommit(); await f.reopen();
    await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const after = await f.host.state(receipt.conversationId);
    assert.equal(after.campaignResearch!.state, 'CONCLUDED');
    assert.deepEqual(after.campaignResearch!.budgetMovements, usage);
    assert.equal(f.computed, 1); assert.equal(after.research!.trials.length, 1);
  } finally { await f.close(); }
});

for (const mode of ['frontier', 'tie', 'all-failed'] as const) test(`closing ${mode} retains candidates/failures and never consumes sealed data`, async () => {
  const f = await fixture(2, mode === 'frontier', true);
  try {
    f.faux.setResponses([hypothesis(), trial(0.4, mode === 'all-failed'), trial(mode === 'frontier' ? 0.8 : 0.4, mode === 'all-failed'), fauxAssistantMessage('Close original experiment budget.')]);
    const receipt = await f.host.start(mode, f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId); const campaign = state.campaignResearch!;
    assert.equal(state.research!.final!.trialId, null); assert.equal(state.research!.final!.result, null);
    assert.equal(campaign.comparisons.at(-1)!.recommendationStatus, mode === 'all-failed' ? 'NEEDS_EVIDENCE' : 'FRONTIER_ONLY');
    assert.equal(campaign.trials.length, 2); assert.equal(campaign.trials.every(item => item.state === (mode === 'all-failed' ? 'FAILED' : 'COMPUTED')), true);
    assert.equal(f.computed, 2); assert.equal(f.faux.state.callCount, 4);
  } finally { await f.close(); }
});

test('an over-budget tool records rejection and closes existing evidence without computing again', async () => {
  const f = await fixture(1);
  try {
    f.faux.setResponses([hypothesis(), trial(0.4), trial(0.9)]);
    const receipt = await f.host.start('over', f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId);
    assert.equal(f.computed, 1); assert.deepEqual(state.research!.trials.map(item => item.status), ['COMPUTED', 'REJECTED']);
    assert.equal(state.campaignResearch!.trials.filter(item => item.countedAttempt).length, 1);
    assert.equal(state.research!.final!.trialId, state.research!.trials[0]!.taskId);
  } finally { await f.close(); }
});

test('PM portfolio consumer receives frozen selection without exposing sealed evidence for a different downstream method', async () => {
  const f = await fixture(1, false, true);
  f.input.campaign!.independentConsumer = 'PM_PORTFOLIO';
  try {
    f.faux.setResponses([hypothesis(), trial(0.4), fauxAssistantMessage('Freeze candidate for the registered PM consumer.')]);
    const receipt = await f.host.start('pm-selection-only', f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId);
    assert.equal(state.research!.final!.conclusion, 'SELECTED'); assert.equal(state.research!.final!.status, 'STOPPED');
    assert.equal(state.research!.final!.result, null); assert.equal(state.campaignResearch!.state, 'CONCLUDED');
    assert.equal((await f.host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context)).items.length, 0);
  } finally { await f.close(); }
});

test('strategy intake preserves an unsupported event-driven goal and exact owner boundaries without converting it to alpha', async () => {
  const f = await fixture(1); f.input.goal = 'Research event settlement risk. Keep maximum loss below 5%.'; f.input.campaign!.intakeRequired = true;
  try {
    f.faux.setResponses([call('strategy_intake', { name: 'Event settlement', archetype: 'EVENT_DRIVEN', economicMechanism: 'Settlement outcomes affect cash', falsificationConditions: ['No settlement relation'],
      requiredCapabilities: ['event-settlement'], intendedHoldingHorizonRef: 'settlement-event', knownFailureModes: ['Settlement dispute'], ambiguities: [], ownerBoundaries: [{ kind: 'LOSS', sourceStatement: 'Keep maximum loss below 5%.' }] }),
      hypothesis(), trial(0.8), fauxAssistantMessage('Adapter evidence is missing.'), fauxAssistantMessage('Adapter evidence is still missing.')]);
    const receipt = await f.host.start('event-intake', f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId); const intake = state.campaignResearch!.intake!;
    assert.equal(intake.strategy.archetype, 'EVENT_DRIVEN'); assert.equal(intake.support, 'UNSUPPORTED'); assert.equal(intake.sourceGoal, f.input.goal);
    assert.deepEqual(intake.ownerBoundaryStatements, [{ kind: 'LOSS', sourceStatement: 'Keep maximum loss below 5%.' }]);
    assert.deepEqual(intake.ownerBoundaries, []); assert.equal(f.computed, 0); assert.equal(state.campaignResearch!.trials.length, 0);
  } finally { await f.close(); }
});

test('PM account intake preserves pure factor research intent and asks for the correct consumer instead of forcing allocation', async () => {
  const f = await fixture(1); f.input.goal = 'Research whether this factor predicts returns; no portfolio mandate.';
  f.input.campaign!.intakeRequired = true; f.input.campaign!.independentConsumer = 'PM_PORTFOLIO';
  try {
    f.faux.setResponses([call('strategy_intake', { name: 'Factor research only', archetype: 'FACTOR_RESEARCH', economicMechanism: 'Test a forecasting relation', falsificationConditions: ['No heldout predictive evidence'],
      requiredCapabilities: [], intendedHoldingHorizonRef: 'one-bar', knownFailureModes: ['Selection bias'], ambiguities: [], ownerBoundaries: [] }),
      hypothesis(), trial(0.8), fauxAssistantMessage('Use the factor research consumer.'), fauxAssistantMessage('No account mandate was requested.')]);
    const receipt = await f.host.start('factor-only-intake', f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId); const intake = state.campaignResearch!.intake!;
    assert.equal(intake.strategy.archetype, 'FACTOR_RESEARCH'); assert.equal(intake.support, 'NEEDS_CLARIFICATION'); assert.equal(intake.sourceGoal, f.input.goal);
    assert.deepEqual(intake.missingCapabilities, ['pm_account_consumer_not_requested']); assert.equal(f.computed, 0); assert.equal(state.campaignResearch!.trials.length, 0);
  } finally { await f.close(); }
});

test('versioned model-turn allowance bounds an endless tool-read loop without calling it economic rejection', async () => {
  const f = await fixture(1);
  try {
    f.faux.setResponses(Array.from({ length: 30 }, () => call('research_brief', {})));
    const receipt = await f.host.start('tool-loop', f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId);
    assert.equal(state.research!.final!.reason, 'orchestration_budget_exhausted'); assert.equal(state.research!.final!.conclusion, 'NEEDS_EVIDENCE');
    assert.equal(f.faux.state.callCount, 24); assert.equal(f.computed, 0);
    assert.equal((await f.host.harness.snapshot(CampaignResearchDoc, receipt.conversationId, context))!.orchestration!.generationTaskIds.length, 24);
  } finally { await f.close(); }
});

test('a rejected finalization leaves search admissible instead of persisting an orphaned independent phase', async () => {
  const f = await fixture(2, false, true);
  try {
    f.faux.setResponses([hypothesis(), trial(0.2), transcript => call('research_finish', { reason: 'Wrong finalization entry', trialId: last(transcript, 'pm_trial').trialId! }),
      transcript => {
        const entry = transcript.messages.filter(value => (value as { toolName?: string }).toolName === 'research_finish').at(-1) as { content: { text?: string }[] };
        assert.match(JSON.stringify(entry.content), /registered_final_evaluator_required/);
        return trial(0.6)(transcript);
      }, fauxAssistantMessage('Original budget is complete.')]);
    const receipt = await f.host.start('rejected-finalizer', f.input); await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId);
    assert.equal(f.computed, 2); assert.equal(state.campaignResearch!.trials.length, 2);
    assert.equal(state.campaignResearch!.state, 'CONCLUDED');
  } finally { await f.close(); }
});

test('legacy group finalization cannot bypass a campaign comparison or its independent consumer', async () => {
  const f = await fixture(2, false, true); f.input.campaign!.independentConsumer = 'PM_PORTFOLIO';
  let conversationId = 0;
  try {
    f.faux.setResponses([hypothesis(), trial(0.2), transcript => call('factor_finalize_group', { reason: 'Wrong grouped consumer',
      selections: [{ researchId: conversationId, trialId: last(transcript, 'pm_trial').trialId! }, { researchId: 999999, trialId: 999999 }] }),
      transcript => {
        const entry = transcript.messages.filter(value => (value as { toolName?: string }).toolName === 'factor_finalize_group').at(-1) as { content: { text?: string }[] };
        assert.match(JSON.stringify(entry.content), /campaign_group_finalization_unsupported/);
        return trial(0.6)(transcript);
      }, fauxAssistantMessage('Freeze for the registered consumer.')]);
    const receipt = await f.host.start('group-consumer', f.input); conversationId = receipt.conversationId;
    await (await f.host.harness.submission(receipt.submissionId, context))!.wait(context); await f.host.harness.waitForIdle(context);
    const state = await f.host.state(receipt.conversationId);
    assert.equal(f.computed, 2); assert.equal(state.research!.final!.conclusion, 'SELECTED');
    assert.equal((await f.host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 100), context)).items.length, 0);
  } finally { await f.close(); }
});

test('registered comparison scope must match actual native validation interval and scenario capital', () => {
  const campaign = binding(2); campaign.scopes[0]!.capital = { amount: '3000.00', currency: 'USD' };
  const input = { goal: 'Input admission protocol', model: { provider: 'fixture', modelId: 'fixture' }, campaign,
    data: { request: { train_end_ns: '10', validation_end_ns: '20' } }, policy: { max_trials: 2, strategy: { simulationSettings: { starting_capital: '3000', base_currency: 'USD' } } } };
  assert.deepEqual(researchInput(input), input);
  assert.throws(() => researchInput({ ...input, data: { request: { train_end_ns: '10', validation_end_ns: '30' } } }), /campaign_native_interval_mismatch/);
  const mismatched = structuredClone(input); mismatched.campaign.scopes[0]!.capital!.amount = '1000';
  assert.throws(() => researchInput(mismatched), /campaign_native_capital_mismatch/);
  mismatched.campaign.scopes[0]!.capital = { amount: '3000', currency: 'EUR' };
  assert.throws(() => researchInput(mismatched), /campaign_native_capital_mismatch/);
});

test('unavailable hard resource meters reject admission rather than silently enforcing only a later attempt', () => {
  const campaign = binding(2);
  const input = { goal: 'Meter admission protocol', model: { provider: 'fixture', modelId: 'fixture' }, data: {}, policy: { max_trials: 2 }, campaign };
  for (const budget of [{ ...campaign.budget, nativeComputeSeconds: 10 }, { ...campaign.budget, modelSpend: { amount: '1', currency: 'USD' } }, { ...campaign.budget, dataBytes: '1000' }]) {
    assert.throws(() => researchInput({ ...input, campaign: { ...campaign, budget } }), /campaign_resource_meter_unavailable/);
  }
});

test('owner stop settles the campaign lifecycle while preserving a previously committed trial and usage', async () => {
  const f = await fixture(2);
  try {
    const committed = f.pauseAfterCommit(); f.faux.setResponses([hypothesis(), trial(0.4)]);
    const receipt = await f.host.start('owner-stop-campaign', f.input); await committed;
    const before = (await f.host.state(receipt.conversationId)).campaignResearch!;
    await f.host.stop(receipt.conversationId);
    const stopped = await f.host.state(receipt.conversationId);
    assert.equal(stopped.campaignResearch!.state, 'CONCLUDED'); assert.equal(stopped.research!.final!.reason, 'owner_stop');
    assert.deepEqual(stopped.campaignResearch!.trials, before.trials); assert.deepEqual(stopped.campaignResearch!.budgetMovements, before.budgetMovements);
    assert.equal(stopped.research!.final!.trialId, null); assert.equal(f.computed, 1);
  } finally { await f.close(); }
});

test('owner stop aborts original running trial without erasing its attempt or inventing elapsed time', async () => {
  const f = await fixture(2);
  try {
    const entered = f.pauseDuringCompute(); f.faux.setResponses([hypothesis(), trial(0.4)]);
    const receipt = await f.host.start('owner-abort-trial', f.input); await entered;
    await f.host.stop(receipt.conversationId); const stopped = await f.host.state(receipt.conversationId);
    assert.equal(stopped.research!.trials[0]!.status, 'FAILED'); assert.equal(stopped.campaignResearch!.trials[0]!.state, 'FAILED');
    assert.equal(stopped.campaignResearch!.trials[0]!.errorCode, 'owner_stop'); assert.equal(stopped.campaignResearch!.trials[0]!.countedAttempt, true);
    assert.equal(stopped.campaignResearch!.budgetMovements.filter(item => item.resource === 'TRIAL').length, 1);
    const elapsed = stopped.campaignResearch!.resourceMeasurements.find(item => item.resource === 'NATIVE_SECONDS')!;
    assert.equal(elapsed.status, 'UNAVAILABLE'); assert.equal(elapsed.value, null);
    assert.equal(stopped.campaignResearch!.state, 'CONCLUDED'); assert.equal(f.computed, 1);
  } finally { await f.close(); }
});

test('reopening repairs original campaign owner-stop intent interrupted before the official abort', async () => {
  const f = await fixture(2); let release!: () => void; let entered!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; }); const boundary = new Promise<void>(resolve => { entered = resolve; });
  try {
    const committed = f.pauseAfterCommit(); f.faux.setResponses([hypothesis(), trial(0.4)]);
    const receipt = await f.host.start('interrupted-owner-stop', f.input); await committed;
    const original = f.host.harness.conversation.bind(f.host.harness);
    f.host.harness.conversation = async (id, ctx) => { const handle = await original(id, ctx); if (!handle) return handle;
      const abort = handle.abort.bind(handle); handle.abort = async (inner, options) => { entered(); await gate; return abort(inner, options); }; return handle; };
    const stopping = f.host.stop(receipt.conversationId).catch(() => null); await boundary;
    assert.equal((await f.host.state(receipt.conversationId)).campaignResearch!.state, 'CLOSING');
    await f.host.close(); release(); await stopping; await f.reopen();
    const repaired = await f.host.state(receipt.conversationId);
    assert.equal(repaired.campaignResearch!.state, 'CONCLUDED'); assert.equal(repaired.campaignResearch!.closingReason, 'owner_stop');
    assert.equal(repaired.research!.trials[0]!.status, 'COMPUTED'); assert.equal(repaired.research!.final!.reason, 'owner_stop');
    assert.equal(f.computed, 1); assert.deepEqual(await f.host.start('interrupted-owner-stop', f.input), receipt);
  } finally { release(); await f.close(); }
});
