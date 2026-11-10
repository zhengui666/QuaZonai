import { requireTrialReview } from './review-workflow.ts';
import { defineTask } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { Study } from './documents.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { compareCampaign, legacySelection } from './pm-comparison.ts';
import { researchRuntime } from './research-runtime.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import { finalizeFactor, finalizePortfolio } from '../plugins/final-evaluation.ts';

/** One official owned task closes search; no model call or secondary scheduler. */
export const ResearchClosureTask = defineTask<{ reason: string }, { phase: 'wait' | 'close' }, JsonObject>({
  name: 'qz.research.close', version: 1, initial: () => ({ phase: 'wait' }),
  phases: { async wait(_task, runtime, context) {
    const pending = (await runtime.snapshot(Study, runtime.conversationId, context))?.trials.filter(item => item.status === 'STARTED').map(item => item.taskId as TaskId) ?? [];
    await runtime.commit(() => pending.length ? { status: 'waiting', checkpoint: { phase: 'close' }, on: pending, policy: 'allSettled' } : { status: 'running', checkpoint: { phase: 'close' } }, context);
  }, async close(task, runtime, context) {
    const api = researchRuntime(runtime);
    try {
      const selection = await api.commit(async tx => {
        const study = await tx.doc(Study, api.conversationId); const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
        if (study.final) {
          if (study.final.taskId !== task.id || typeof study.final.trialId !== 'number') return null;
          const selected = study.final.trialId;
          if (study.final.status === 'STARTED') return { trialId: selected, proposal: JSON.parse(JSON.stringify(study.trials.find(item => item.taskId === selected)?.proposal ?? null)) as JsonObject | null, independent: true };
          // Selection is persisted before the owned reviewer runs. Resume only this original pending closure, never a stop/refusal or another final owner.
          if (study.final.status === 'STOPPED' && study.final.scientificClaim === 'SELECTION_ONLY_NOT_INDEPENDENT_QUALIFICATION'
            && !study.deletedAt && campaign.state === 'CLOSING' && campaign.closureTaskId === task.id
            && campaign.closingReason === task.input.reason && campaign.closingReason !== 'owner_stop'
            && study.trials.some(item => item.taskId === selected && item.status === 'COMPUTED')) return { trialId: selected, proposal: null, independent: false };
          return null;
        }
        const comparison = await compareCampaign(tx, api);
        const selected = comparison ? comparison.recommendationStatus === 'SELECTED'
          ? campaign.trials.find(item => item.id === comparison.recommendedTrialId)?.runtime.taskId ?? null : null
          : study.input ? legacySelection(study.input, study.trials) : null;
        // Technical lack of progress never authorizes consuming independent data.
        const allowIndependent = !['research_no_new_evidence', 'orchestration_budget_exhausted', 'continuation_requires_trial_budget', 'strategy_intake_needs_clarification', 'strategy_intake_unsupported'].includes(task.input.reason);
        if (selected !== null && study.input?.data.sealed && allowIndependent && campaign.binding?.independentConsumer !== 'PM_PORTFOLIO') {
          campaign.state = 'EVALUATING_INDEPENDENT';
          return { trialId: selected, proposal: JSON.parse(JSON.stringify(study.trials.find(item => item.taskId === selected)?.proposal ?? null)) as JsonObject | null, independent: true };
        }
        study.final = { taskId: task.id, trialId: selected, comparedTrials: study.trials.map(item => item.taskId!),
          status: 'STOPPED', reason: task.input.reason, result: null, comparisonId: comparison?.id ?? null,
          conclusion: comparison?.recommendationStatus ?? (selected === null ? 'NEEDS_EVIDENCE' : 'SELECTED'),
          scientificClaim: 'SELECTION_ONLY_NOT_INDEPENDENT_QUALIFICATION' };
        campaign.state = selected === null ? 'CONCLUDED' : 'CLOSING';
        return selected === null ? null : { trialId: selected, proposal: null, independent: false };
      }, context);
      if (selection && await requireTrialReview(api, context, selection.trialId) && selection.independent) {
        const args = { trialId: selection.trialId, reason: task.input.reason };
        const proposal = selection.proposal;
        if (proposal && typeof proposal === 'object' && !Array.isArray(proposal) && Array.isArray(proposal.members)) await finalizePortfolio(args, api, context);
        else await finalizeFactor(args, api, context);
      }
      await runtime.commit(async tx => {
        const campaign = await tx.doc(CampaignResearchDoc, api.conversationId); const final = (await tx.doc(Study, api.conversationId)).final;
        if (campaign.closureTaskId === task.id && campaign.closingReason !== 'owner_stop' && final?.taskId === task.id) campaign.state = 'CONCLUDED';
        return { status: 'terminal', outcome: { status: 'completed', result: { closed: true } } };
      }, context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      const message = error instanceof Error ? error.message : String(error);
      await runtime.commit(async tx => {
        const study = await tx.doc(Study, runtime.conversationId);
        study.final ??= { taskId: task.id, trialId: null, comparedTrials: study.trials.map(item => item.taskId!), status: 'STOPPED', reason: 'closure_needs_evidence',
          result: null, error: message, conclusion: 'NEEDS_EVIDENCE' };
        const campaign = await tx.doc(CampaignResearchDoc, runtime.conversationId);
        if (campaign.closureTaskId === task.id && campaign.closingReason !== 'owner_stop' && study.final.taskId === task.id) campaign.state = 'CONCLUDED';
        return { status: 'terminal', outcome: { status: 'failed', error: { message } } };
      }, context);
    }
  } },
  async abort(task, runtime, context) {
    await runtime.commit(async tx => {
      const study = await tx.doc(Study, runtime.conversationId);
      if (study.final?.taskId === task.id && study.final.status === 'STARTED') {
        study.final.status = 'FAILED'; study.final.error = 'research_closure_aborted';
      }
      const campaign = await tx.doc(CampaignResearchDoc, runtime.conversationId);
      // Owner stop reconciliation must also settle original admitted trial tasks.
      if (campaign.binding && campaign.closingReason !== 'owner_stop') campaign.state = 'CONCLUDED';
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, context);
  },
});

export async function closeResearch(api: ResearchRuntime, context: Context, reason: string, wait = true): Promise<void> {
  const id = await api.commit(async tx => {
    const doc = await tx.doc(CampaignResearchDoc, api.conversationId);
    if (doc.closureTaskId !== null) return doc.closureTaskId;
    if ((await tx.doc(Study, api.conversationId)).final) return null;
    doc.state = 'CLOSING'; doc.closingReason = reason;
    // A generation response is not checkpointed until its hooks return. Closing
    // belongs to the conversation so a restarted/provider-failed generation
    // cannot cancel already admitted independent evaluation.
    doc.closureTaskId = await tx.createTask(ResearchClosureTask, { reason }, { conversationId: api.conversationId, ownership: { kind: 'conversation' } });
    return doc.closureTaskId;
  }, context);
  if (id !== null && wait) await api.waitForTask(id, context);
}
