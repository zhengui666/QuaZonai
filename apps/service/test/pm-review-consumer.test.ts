import { obj, str } from '@earendil-works/pi-schema';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';
import { defineExtension, defineTask, defineTool } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { ReviewMetric, ReviewPolicy, ReviewReport } from '../../../packages/contracts/src/review.ts';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import research from '../../research/plugins/research.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { Study } from '../../research/src/documents.ts';
import { evaluateTrial } from '../../research/src/trials.ts';
import { ReviewLedger } from '../../research/src/review-documents.ts';
import { reviewTargetVersion } from '../../research/src/review-identity.ts';
import { json, object } from '../src/facts.ts';
import { pmRecord, pmState } from '../src/pm-facts.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

const directory = process.env.QZ_PM_NATIVE_FIXTURE_DIR;
const binary = process.env.QZ_JOB_BINARY;
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function result(transcript: { messages: readonly unknown[] }, tool: string): JsonObject {
  const message = (transcript.messages.filter(value => (value as { role?: string; toolName?: string }).role === 'toolResult'
    && (value as { toolName?: string }).toolName === tool) as { isError?: boolean; content: { type: string; text?: string }[] }[]).at(-1);
  assert.ok(message); const text = message.content.filter(item => item.type === 'text').map(item => item.text).join('');
  assert.ok(!message.isError, `${tool}: ${text}`); return object(JSON.parse(text));
}

// This is a controlled REPORT PRODUCER / real PM CONSUMER contract test. The
// producer's explicitly TEST_ONLY review observations are not claims about PIT,
// a market, a model or profitability. Native fit/replay, PM capital comparison,
// independent replay, exposure, budgeting and recovery use the actual kernel.
const SourceTask = defineTask<{ data: JsonObject; native: JsonObject }, { phase: 'retain' }, JsonObject>({
  name: 'test.pm-review-original-source', version: 1, initial: () => ({ phase: 'retain' }),
  phases: { async retain(task, runtime, ctx) {
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { native: task.input.native } } }), ctx);
  } },
  async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
});

async function registerControlledPolicy(state: Awaited<ReturnType<typeof setup>>, evaluationStartNs: string): Promise<ReviewPolicy> {
  const ids = await state.service.research.harness.commit(async tx => {
    const ids = [];
    for (let index = 0; index < 30; index++) {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } }); const trialId = index + 10000;
      const study = await tx.doc(Study, conversation.id);
      study.input = { goal: 'TEST_ONLY retained protocol calibration; no market or model claim.', model: state.model, data: {}, policy: {} };
      study.trials = [{ taskId: trialId, status: 'COMPUTED', proposal: { hypothesis: 'TEST_ONLY reference' }, result: { request: { validation_end_ns: '80' } } }];
      const policy = createReviewPolicy('PORTFOLIO', `TEST_ONLY_REFERENCE_POLICY:${index}`, '1000');
      const evidenceRef = `research:${conversation.id}:trial:${trialId}:validation`;
      const report = scoreReview({ id: `TEST_ONLY_REFERENCE_REPORT:${index}`, target: { kind: 'PORTFOLIO', id: `research:${conversation.id}:trial:${trialId}`,
        version: `TEST_ONLY_REFERENCE_VERSION:${index}`, sourceEvidenceRefs: [evidenceRef], evaluationStartNs: '10' }, policy,
        metrics: policy.metricRules.map(rule => ({ key: rule.key, value: rule.low + (rule.high - rule.low) * (index + 1) / 31,
          status: 'AVAILABLE', observationCount: 100, scope: rule.scope, method: rule.methods[0]!, unit: rule.unit, frequency: rule.frequency, evidenceRef })),
        checks: [], narrative: 'TEST_ONLY protocol anchors, deliberately retaining unsuccessful reports.', createdAt: '2026-11-02T00:00:00Z' });
      assert.notEqual(report.decision, 'PASS');
      (await tx.doc(ReviewLedger, conversation.id)).records[report.target.version] = { trialId, version: report.target.version, policyId: policy.id, status: 'COMPLETED', report: json(report) };
      ids.push(conversation.id);
    }
    return ids;
  }, context);
  const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/review-policies', headers: state.headers,
    payload: { requestId: 'TEST_ONLY_PM_CONSUMER_CALIBRATION', kind: 'PORTFOLIO', researchIds: ids, evaluationStartNs } });
  assert.equal(response.statusCode, 201, response.body);
  return response.json<ReviewPolicy>();
}

for (const mode of ['complete', 'missing', 'negative'] as const) test(`PM consumes a formally scored TEST_ONLY review through real Durable and native science: ${mode}`, {
  timeout: 240_000, skip: !directory || !binary ? 'Requires original exported multi-day native evidence and matching executable.' : false,
}, async t => {
  const source = object(JSON.parse(await readFile(join(directory!, 'source.json'), 'utf8')));
  const replay = object(JSON.parse(await readFile(join(directory!, 'registered-policy-replay.json'), 'utf8')));
  assert.match(String(source.fixtureProvenance), /SYNTHETIC_TEST_ONLY/);
  const state = await setup();
  try {
    const profile = await registerControlledPolicy(state, String(object(source.request).train_end_ns));
    const dataset = state.options.config.datasets[0]!;
    dataset.data = { ...object(source.data), sealed: object(source.heldoutData) };
    dataset.strategy = object(source.policy) as unknown as NonNullable<typeof dataset.strategy>;
    dataset.policy = { reviewPolicy: json(profile) };
    state.options.config.jobBinary = binary!;
    let produced: ReviewReport | undefined;
    const producer = defineExtension({ name: 'test.pm-controlled-review-producer', tasks: [SourceTask], tools: [defineTool({
      name: 'test_reviewed_fitted_trial', description: 'TEST_ONLY reviewed producer fixture, not market/PIT/model evidence.',
      parameters: obj({ hypothesisId: str() }), replay: 'safe',
      async execute(args, api, ctx) {
        const input = (await api.snapshot(Study, api.conversationId, ctx))!.input!;
        const original = { ...source, ...object(replay.evidence), fitRequest: source.fitRequest!, data: json(input.data), fixtureProvenance: 'TEST_ONLY_CONTROLLED_REVIEW_PRODUCER' };
        const sourceId = await api.commit(tx => tx.createTask(SourceTask, { data: json(input.data), native: original },
          { conversationId: api.conversationId, ownership: { kind: 'conversation' } }), ctx);
        const receipt = await api.waitForTask(sourceId, ctx); assert.equal(receipt.state.outcome.status, 'completed');
        const proposal = { hypothesis: 'TEST_ONLY original fitted joint consumer input', hypothesisId: args.hypothesisId,
          purpose: 'JOINT_REPLAY', members: source.members!, request: source.request!, strategy: json(input.policy.strategy) };
        await evaluateTrial(api, ctx, proposal, async () => ({ ...object(replay.summary), evidenceTaskId: sourceId }));
        await api.commit(async tx => {
          const study = await tx.doc(Study, api.conversationId); const selected = study.trials.find(trial => trial.taskId === api.taskId)!;
          const policy = object(study.input!.policy.reviewPolicy) as unknown as ReviewPolicy;
          assert.deepEqual(policy, profile, 'consumer policy is the originally registered, admitted object');
          const version = reviewTargetVersion(json(study.input), json(selected), study.trials.map(trial => json(trial)), policy);
          const evidenceRef = `research:${api.conversationId}:trial:${api.taskId}:validation`;
          const metrics: ReviewMetric[] = policy.metricRules.map(rule => ({ key: rule.key, value: rule.direction === 'LOWER' ? rule.low : rule.high, status: 'AVAILABLE',
            observationCount: Math.max(100, rule.minimumObservations), scope: rule.scope, method: rule.methods[0]!, unit: rule.unit, frequency: rule.frequency, evidenceRef }));
          const net = metrics.find(metric => metric.key === 'account.net_return_lower')!;
          if (mode === 'missing') { net.value = null; net.status = 'INSUFFICIENT'; net.reason = 'TEST_ONLY_REQUIRED_OBSERVATIONS_ABSENT'; }
          if (mode === 'negative') net.value = -0.1;
          produced = scoreReview({ id: `TEST_ONLY_SCORED_REVIEW:${api.taskId}`, target: { kind: 'PORTFOLIO', id: `research:${api.conversationId}:trial:${api.taskId}`,
            version, sourceEvidenceRefs: [evidenceRef], evaluationStartNs: String(object(source.request).train_end_ns) }, policy,
            checks: policy.requiredChecks.map(id => ({ id, status: 'PASS', reason: 'TEST_ONLY_CONTROLLED_PRODUCER_CONTRACT', evidenceRefs: [evidenceRef] })),
            metrics, narrative: 'TEST_ONLY producer fixture for PM consumer protocol. No market/PIT/model quality or profitability is established.', createdAt: '2026-11-02T00:00:00Z' });
          (await tx.doc(ReviewLedger, api.conversationId)).records[version] = { trialId: api.taskId, version, policyId: policy.id, status: 'COMPLETED', report: json(produced), fixture: 'TEST_ONLY_CONTROLLED_PRODUCER' };
        }, ctx);
        return { output: [{ type: 'text', text: JSON.stringify({ trialId: api.taskId, fixture: 'TEST_ONLY_CONTROLLED_PRODUCER' }) }] };
      },
    })] });
    state.options.extensions = [research, producer]; await state.restart();
    state.faux.setResponses([
      call('strategy_intake', { name: 'TEST_ONLY consumer protocol', archetype: 'PREDICTIVE_ALPHA',
        economicMechanism: 'TEST_ONLY causal native account fixture.', falsificationConditions: ['Original native independent result is unavailable.'],
        requiredCapabilities: [], intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['selection_bias'], ambiguities: [], ownerBoundaries: [] }),
      call('research_hypothesis', { economicFamilyId: 'TEST_ONLY_CONSUMER', mechanism: 'TEST_ONLY causal native fitted protocol.',
        prediction: 'The actual native account comparison completes.', falsification: 'The original account evidence is absent.', requiredDataCapabilities: ['BAR'], incrementalQuestion: 'Can PM consume the formally scored report?' }),
      transcript => call('test_reviewed_fitted_trial', { hypothesisId: result(transcript, 'research_hypothesis').id! }),
      transcript => call('research_finish', { trialId: result(transcript, 'test_reviewed_fitted_trial').trialId!, reason: 'TEST_ONLY controlled producer is complete; consume the original report.' }),
    ]);
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { requestId: `TEST_ONLY_PM_REVIEW_${mode}`, goal: 'TEST_ONLY portfolio account capital decision consumer protocol; compare original native shared account with cash.',
        datasetId: dataset.id, funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, model: state.model, businessPreference: 'NET_RETURN' } });
    assert.equal(response.statusCode, 202, response.body); const id = response.json<Pm.PmMandateView>().mandate.id;
    const deadline = Date.now() + 210_000;
    let record = pmRecord(pmState(await state.service.pm.snapshot()), id);
    while (Date.now() < deadline) {
      record = pmRecord(pmState(await state.service.pm.snapshot()), id);
      if (record.currentDecisionId && (mode === 'complete'
        ? record.artifacts[`independent-task:${record.currentDecisionId}`]
        : record.cycles[record.decisions[record.currentDecisionId]!.cycleId]!.reasonCodes.includes('independent_review_not_passed'))) break;
      if (Object.values(record.cycles).some(cycle => cycle.state === 'FAILED')) break;
      await new Promise(resolve => setTimeout(resolve, 40));
    }
    assert.ok(produced, JSON.stringify({ campaigns: record.campaigns, cycles: record.cycles }));
    assert.equal(produced.decision, mode === 'complete' ? 'PASS' : mode === 'negative' ? 'REJECT' : 'INCONCLUSIVE', JSON.stringify(produced));
    assert.deepEqual(produced.reasonCodes, mode === 'complete' ? [] : [mode === 'negative'
      ? 'account.net_return_lower:below_absolute_admission_minimum' : 'account.net_return_lower:TEST_ONLY_REQUIRED_OBSERVATIONS_ABSENT']);
    assert.ok(record.currentDecisionId, JSON.stringify({ cycles: record.cycles, errors: Object.values(record.artifacts).filter(value => value.stage) }));
    const decision = record.decisions[record.currentDecisionId]!;
    const cycle = record.cycles[decision.cycleId]!; const frozen = record.artifacts[cycle.id]!;
    const originalStudy = object(frozen.research); const originalInput = object(originalStudy.input);
    const originalTrials = originalStudy.trials as JsonObject[]; const selectedId = object(originalStudy.final).trialId;
    const selected = originalTrials.find(trial => trial.taskId === selectedId)!;
    assert.deepEqual(object(originalInput.policy).reviewPolicy, json(profile), 'modern policy is retained before and after admission');
    assert.equal(produced.target.version, reviewTargetVersion(originalInput, selected, originalTrials, profile));
    const reports = object(object(frozen.reviews).records);
    assert.deepEqual(object(reports[produced.target.version]).report, json(produced), 'PM consumes the formal report from its frozen real ReviewLedger');
    const independentId = record.artifacts[`independent-task:${decision.id}`]?.taskId;
    if (mode !== 'complete') {
      assert.equal(decision.action, 'NEEDS_EVIDENCE'); assert.equal(cycle.state, 'WAITING_EVIDENCE');
      assert.ok(decision.reasonCodes.includes('independent_review_not_passed'));
      assert.equal(independentId, undefined);
      assert.equal(decision.targetCommandId, null); assert.deepEqual(record.commands, {}, 'blocked review cannot issue a target delivery command');
      const exposure = await state.service.research.harness.snapshot(Exposures, context);
      assert.equal(exposure?.windows.some(window => window.kind === 'FINAL') ?? false, false, 'nonpassed report cannot consume Sealed');
    } else {
      assert.equal(decision.action, 'REBALANCE'); assert.equal(typeof independentId, 'number');
      const settled = await state.service.research.harness.waitForTask(independentId as TaskId, context);
      assert.equal(settled.state.outcome.status, 'completed', JSON.stringify(settled.state));
      record = pmRecord(pmState(await state.service.pm.snapshot()), id);
      const referenceId = String(object(record.decisions[decision.id]!.explanationFacts).independentEvidenceRef);
      const evidence = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(referenceId)}`, headers: state.headers });
      assert.equal(evidence.statusCode, 200, evidence.body.slice(0, 200));
      const reference = object(evidence.json().reference); const independent = object(evidence.json().payload);
      assert.equal(reference.schemaId, 'qz.pm.independent-portfolio/1'); assert.equal(reference.role, 'INDEPENDENT'); assert.equal(reference.visibility, 'OWNER_ONLY');
      const metrics = [...independent.metrics as unknown as Pm.MetricObservation[], ...independent.baselineMetrics as unknown as Pm.MetricObservation[]];
      assert.ok(metrics.length > 0); assert.deepEqual([...new Set(metrics.map(metric => metric.methodRef))], [decision.policyRevisionId]);
      assert.ok(metrics.every(metric => metric.role === 'INDEPENDENT'));
      const exposure = await state.service.research.harness.snapshot(Exposures, context);
      assert.equal(exposure!.windows.filter(window => window.kind === 'FINAL').length, 1);
    }
    const stored = json(record); const counted = Object.values(record.derivedTrials).filter(trial => trial.countedAttempt).length;
    const modelCalls = state.faux.state.callCount;
    await state.restart();
    if (typeof independentId === 'number') await state.service.research.harness.waitForTask(independentId as TaskId, context);
    const restored = pmRecord(pmState(await state.service.pm.snapshot()), id);
    assert.equal(restored.decisions[decision.id]!.action, mode === 'complete' ? 'REBALANCE' : 'NEEDS_EVIDENCE');
    if (mode !== 'complete') assert.ok(restored.decisions[decision.id]!.reasonCodes.includes('independent_review_not_passed'));
    assert.equal(Object.values(restored.derivedTrials).filter(trial => trial.countedAttempt).length, counted);
    assert.deepEqual(restored.artifacts[cycle.id], object(stored.artifacts)[cycle.id]);
    const exposures = await state.service.research.harness.snapshot(Exposures, context);
    assert.equal(exposures?.windows.filter(window => window.kind === 'FINAL').length ?? 0, mode === 'complete' ? 1 : 0);
    assert.equal(state.faux.state.callCount, modelCalls, 'review/independent result is not fed back as another research turn');
    t.diagnostic(JSON.stringify({ fixture: 'TEST_ONLY_CONTROLLED_REVIEW_PRODUCER_REAL_NATIVE_PM_CONSUMER', mode, decision: produced.decision, independentId: independentId ?? null, countedAttempts: counted }));
  } finally { await state.cleanup(); }
});
