import { createHash } from 'node:crypto';
import { canonical } from './documents.ts';
import { configure, defineDoc, defineTask, LiveDoc, ProviderDoc } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, SubmissionId } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import type { ReviewCheck, ReviewMetric, ReviewPolicy, ReviewReport, ReviewTarget } from '../../../packages/contracts/src/review.ts';
import { scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import { reviewExtension, reviewInstructions } from '../plugins/review.ts';
import { ReviewNativeTask } from './review-native-task.ts';
import type { ReviewNativePlan } from './review-native-task.ts';
import { ReviewSession } from './review-documents.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import { ModelTiers } from './models.ts';

export interface IndependentReviewInput {
  target: ReviewTarget; policy: ReviewPolicy;
  model: { provider: string; modelId: string };
  thinkingLevel?: import('@earendil-works/pi-ai').ModelThinkingLevel;
  serviceTier?: import('../../../packages/contracts/src/index.ts').ServiceTier;
  sciencePlans?: ReviewNativePlan[];
  evidence: JsonObject; checks: ReviewCheck[]; metrics: ReviewMetric[]; createdAt: string;
}
const json = (value: unknown): JsonObject => JSON.parse(JSON.stringify(value)) as JsonObject;
const Child = defineDoc<{ bindings: Record<string, { taskId: TaskId<JsonObject>; digest: string }> }>({
  kind: 'qz.review-child', checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, version: 1, scope: 'task', initial: () => ({ bindings: {} }),
});

/** Shared by research closure and optional portfolio consumers. All input is host-resolved evidence. */
type ReviewCheckpoint = { phase: 'create' }
  | { phase: 'science'; reviewer: ConversationId; nativeTasks: TaskId<JsonObject>[] }
  | { phase: 'complete'; reviewer: ConversationId; submissionId: SubmissionId };
export const IndependentReviewTask = defineTask<IndependentReviewInput, ReviewCheckpoint, JsonObject>({
  name: 'qz.review.independent', version: 1, initial: () => ({ phase: 'create' }),
  phases: {
    async create(task, runtime, context) {
      await runtime.commit(async tx => {
        const conversation = await tx.createConversation({ ownership: { kind: 'task', taskId: task.id } });
        await configure(tx, conversation.id, { model: task.input.model, thinkingLevel: task.input.thinkingLevel,
          extensions: [reviewExtension], tools: null, instructions: reviewInstructions });
        if (task.input.serviceTier) (await tx.doc(ModelTiers)).tiers[(await tx.doc(ProviderDoc, conversation.id)).sessionId] = task.input.serviceTier;
        const session = await tx.doc(ReviewSession, conversation.id);
        session.binding = json({ target: task.input.target, policy: task.input.policy, evidenceRefs: task.input.target.sourceEvidenceRefs });
        session.evidence = json(task.input.evidence);
        const nativeTasks: TaskId<JsonObject>[] = [];
        for (const plan of task.input.sciencePlans ?? []) nativeTasks.push(await tx.createTask(ReviewNativeTask, plan, { ownership: { kind: 'task', taskId: task.id } }));
        const checkpoint = { phase: 'science' as const, reviewer: conversation.id, nativeTasks };
        return nativeTasks.length ? { status: 'waiting', checkpoint, on: nativeTasks, policy: 'allSettled' } : { status: 'running', checkpoint };
      }, context);
    },
    async science(task, runtime, context) {
      const { reviewer, nativeTasks } = task.state.checkpoint;
      const native = await runtime.outcomes(nativeTasks, context);
      const handle = await runtime.conversation(reviewer, context);
      if (!handle) throw new Error('review_conversation_missing');
      await runtime.commit(async tx => {
        const submissionId = await handle.submit(tx, { type: 'input', requestId: `review:${task.id}`,
          content: 'Review this immutable candidate. Read each of the six evidence tools and finish with the evidence-cited mechanism checklist, findings and narrative. Unsupported checks must remain explicit.' });
        const session = await tx.doc(ReviewSession, reviewer);
        for (const [index, plan] of (task.input.sciencePlans ?? []).entries()) {
          const outcome = native[index]!;
          const result = outcome.status === 'completed' ? outcome.result : null;
          const section = plan.kind === 'SELECTION' ? 'selection' : plan.kind === 'PORTFOLIO' ? 'portfolio' : 'factor';
          session.evidence![section] = result ? { status: 'COMPLETE', result, evidenceRef: plan.evidenceRef }
            : { status: 'INCOMPLETE', reason: 'review_science_unavailable', evidenceRef: plan.evidenceRef };
          if (result) session.nativeResults.push({ kind: plan.kind, ...result });
        }
        const checkpoint = { phase: 'complete' as const, reviewer, submissionId };
        const generation = (await tx.doc(LiveDoc, reviewer)).run?.taskId;
        return generation ? { status: 'waiting', checkpoint, on: [generation], policy: 'allSettled' } : { status: 'running', checkpoint };
      }, context);
    },
    async complete(task, runtime, context) {
      const reviewer = task.state.checkpoint.reviewer;
      try {
        const live = await runtime.snapshot(LiveDoc, reviewer, context);
        if (live?.run) {
          await runtime.commit(() => ({ status: 'waiting', checkpoint: task.state.checkpoint, on: [live.run!.taskId], policy: 'allSettled' }), context);
          return;
        }
      const session = await runtime.snapshot(ReviewSession, reviewer!, context);
      const checks = structuredClone(task.input.checks); const metrics = structuredClone(task.input.metrics);
      for (const native of session?.nativeResults ?? []) if (Array.isArray(native.metrics)) {
        for (const metric of native.metrics as unknown as ReviewMetric[]) { const old = metrics.findIndex(row => row.key === metric.key); if (old < 0) metrics.push(metric); else metrics[old] = metric; }
      }
      const implementation = checks.find(check => check.id === 'implementation');
      if (!session?.finished || session.findings.some(finding => finding.severity === 'REJECT')) {
        if (implementation && implementation.status !== 'FAIL') { implementation.status = 'UNKNOWN';
          implementation.reason = session?.finished ? 'review_implementation_finding_requires_resolution' : 'review_agent_did_not_complete_required_checks'; }
      }
      const rubric = session?.evidence?.mechanism;
      if (session?.finished && Array.isArray(rubric)) metrics.push({ key: 'mechanism.evidence_fraction',
        value: rubric.filter(item => item && typeof item === 'object' && !Array.isArray(item) && item.supported === true).length / 5,
        status: 'AVAILABLE', observationCount: 5, scope: 'MECHANISM', method: 'qz.review-mechanism/1', unit: 'RATIO', frequency: 'REGISTERED',
        evidenceRef: task.input.target.sourceEvidenceRefs[0]! });
      const report = scoreReview({ id: `review:${task.id}`, target: task.input.target, policy: task.input.policy,
        checks, metrics, createdAt: task.input.createdAt,
        narrative: session?.narrative ?? 'The independent review agent did not complete its required evidence review.',
        analyses: Object.entries(session?.evidence ?? task.input.evidence).filter(([kind]) => kind !== 'context').map(([kind, value]) => ({
          kind, status: kind === 'mechanism' && session?.finished && Array.isArray(value) && value.length === 5 ? 'COMPLETE' : value && typeof value === 'object' && !Array.isArray(value) && value.status === 'COMPLETE' ? 'COMPLETE' : 'INCOMPLETE',
          reason: value && typeof value === 'object' && !Array.isArray(value) && typeof value.reason === 'string' ? value.reason : null,
          evidenceRef: value && typeof value === 'object' && !Array.isArray(value) && typeof value.evidenceRef === 'string' ? value.evidenceRef : task.input.target.sourceEvidenceRefs[0]!,
          metrics: metrics.filter(metric => metric.scope === (kind === 'selection' ? 'SELECTION_COHORT' : kind === 'portfolio' ? 'SHARED_ACCOUNT' : kind === 'factor' ? 'POOLED_TIME_SERIES' : kind === 'mechanism' ? 'MECHANISM' : 'DATA')).map(metric => metric.key),
        })) });
      if (session?.findings.length) report.narrative = `${report.narrative ?? ''}\n\nFindings:\n${session.findings.map(finding => `${finding.severity}: ${finding.detail} [${(finding.evidenceRefs as string[]).join(', ')}]`).join('\n')}`;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { report: json(report), reviewerConversationId: reviewer! } } }), context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      const reason = error instanceof Error ? error.message : 'review_agent_failed';
      const checks = task.input.checks.map(check => check.id === 'implementation' && check.status !== 'FAIL'
        ? { ...check, status: 'UNKNOWN' as const, reason: 'review_agent_failed' } : check);
      const report = scoreReview({ id: `review:${task.id}`, target: task.input.target, policy: task.input.policy,
        checks, metrics: task.input.metrics, createdAt: task.input.createdAt, narrative: 'Independent review could not complete. The original candidate was not released.' });
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { report: json(report),
        reviewerConversationId: reviewer ?? null, error: reason } } }), context);
    }
  } },
  async abort(_task, runtime, context) {
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});

export async function runIndependentReview(api: ResearchRuntime, context: Context, input: IndependentReviewInput): Promise<ReviewReport> {
  const id = await api.commit(async tx => {
    const child = await tx.doc(Child, api.taskId);
    // Execution speed keeps the existing review identity when older tasks resume.
    const { serviceTier: _serviceTier, ...identity } = input;
    const digest = createHash('sha256').update(canonical(json(identity))).digest('hex');
    const previous = child.bindings[input.target.id];
    if (previous) { if (previous.digest !== digest) throw new Error('review_identity_conflict'); return previous.taskId; }
    const taskId = await tx.createTask(IndependentReviewTask, input, { ownership: { kind: 'task', taskId: api.taskId } });
    child.bindings[input.target.id] = { taskId, digest };
    return taskId;
  }, context);
  const settled = await api.waitForTask(id, context);
  if (settled.state.outcome.status !== 'completed') throw new Error('review_task_not_completed');
  return settled.state.outcome.result.report as unknown as ReviewReport;
}
