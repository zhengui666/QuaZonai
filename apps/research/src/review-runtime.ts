import { createHash } from 'node:crypto';
import { canonical } from './documents.ts';
import { configure, defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import type { ReviewCheck, ReviewMetric, ReviewPolicy, ReviewReport, ReviewTarget } from '../../../packages/contracts/src/review.ts';
import { scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import { reviewExtension, reviewInstructions } from '../plugins/review.ts';
import { ReviewNativeTask } from './review-native-task.ts';
import type { ReviewNativePlan } from './review-native-task.ts';
import { ReviewSession } from './review-documents.ts';
import type { ResearchRuntime } from './research-runtime.ts';

export interface IndependentReviewInput {
  target: ReviewTarget; policy: ReviewPolicy;
  model: { provider: string; modelId: string };
  thinkingLevel?: import('@earendil-works/pi-ai').ModelThinkingLevel;
  sciencePlans?: ReviewNativePlan[];
  evidence: JsonObject; checks: ReviewCheck[]; metrics: ReviewMetric[]; createdAt: string;
}
const json = (value: unknown): JsonObject => JSON.parse(JSON.stringify(value)) as JsonObject;
const Binding = defineDoc<{ conversationId: ConversationId | null; submissionId: number | null; nativeTasks: Record<string, TaskId<JsonObject>> }>({
  kind: 'qz.review-task', version: 1, scope: 'task', initial: () => ({ conversationId: null, submissionId: null, nativeTasks: {} }),
});
const Child = defineDoc<{ bindings: Record<string, { taskId: TaskId<JsonObject>; digest: string }> }>({
  kind: 'qz.review-child', version: 1, scope: 'task', initial: () => ({ bindings: {} }),
});

/** Shared by research closure and optional portfolio consumers. All input is host-resolved evidence. */
export const IndependentReviewTask = defineTask<IndependentReviewInput, { phase: 'review' }, JsonObject>({
  name: 'qz.review.independent', version: 1, initial: () => ({ phase: 'review' }),
  phases: { async review(task, runtime, context) {
    let reviewer: ConversationId | undefined;
    try {
      await runtime.commit(async tx => {
        const binding = await tx.doc(Binding, task.id);
        if (binding.conversationId !== null) { reviewer = binding.conversationId; return undefined; }
        const conversation = await tx.createConversation({ ownership: { kind: 'task', taskId: task.id } });
        reviewer = conversation.id;
        await configure(tx, reviewer, { model: task.input.model, thinkingLevel: task.input.thinkingLevel,
          extensions: [reviewExtension], tools: null, instructions: reviewInstructions });
        const session = await tx.doc(ReviewSession, reviewer);
        session.binding = json({ target: task.input.target, policy: task.input.policy, evidenceRefs: task.input.target.sourceEvidenceRefs });
        session.evidence = json(task.input.evidence);
        binding.conversationId = reviewer;
        return undefined;
      }, context);
      for (const plan of task.input.sciencePlans ?? []) {
        let nativeId!: TaskId<JsonObject>;
        await runtime.commit(async tx => {
          const binding = await tx.doc(Binding, task.id);
          binding.nativeTasks[plan.kind] ??= await tx.createTask(ReviewNativeTask, plan, { ownership: { kind: 'task', taskId: task.id } });
          nativeId = binding.nativeTasks[plan.kind]!;
          return undefined;
        }, context);
        const native = await runtime.waitForTask(nativeId, context);
        await runtime.commit(async tx => {
          const session = await tx.doc(ReviewSession, reviewer!);
          const section = plan.kind === 'SELECTION' ? 'selection' : plan.kind === 'PORTFOLIO' ? 'portfolio' : 'factor';
          const result = native.state.outcome.status === 'completed' ? native.state.outcome.result : null;
          session.evidence![section] = result ? { status: 'COMPLETE', result, evidenceRef: plan.evidenceRef }
            : { status: 'INCOMPLETE', reason: 'review_science_unavailable', evidenceRef: plan.evidenceRef };
          if (result && !session.nativeResults.some(row => row.kind === plan.kind)) session.nativeResults.push({ kind: plan.kind, ...result });
          return undefined;
        }, context);
      }
      const handle = await runtime.conversation(reviewer!, context);
      if (!handle) throw new Error('review_conversation_missing');
      const submission = await handle.submit({ type: 'input', requestId: `review:${task.id}`,
        content: 'Review this immutable candidate. Read each of the six evidence tools and finish with the evidence-cited mechanism checklist, findings and narrative. Unsupported checks must remain explicit.' }, context);
      await runtime.commit(async tx => { (await tx.doc(Binding, task.id)).submissionId = submission.id; return undefined; }, context);
      await submission.wait(context);
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
        analyses: Object.entries(task.input.evidence).filter(([kind]) => kind !== 'context').map(([kind, value]) => ({
          kind, status: value && typeof value === 'object' && !Array.isArray(value) && value.status === 'COMPLETE' ? 'COMPLETE' : 'INCOMPLETE',
          reason: value && typeof value === 'object' && !Array.isArray(value) && typeof value.reason === 'string' ? value.reason : null,
          evidenceRef: task.input.target.sourceEvidenceRefs[0]!, metrics: [],
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
    const digest = createHash('sha256').update(canonical(json(input))).digest('hex');
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
