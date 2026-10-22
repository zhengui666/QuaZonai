import { defineTask } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { Study } from './documents.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { compareCampaign, legacySelection } from './pm-comparison.ts';
import { researchRuntime } from './research-runtime.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import { finalizeFactor, finalizePortfolio } from '../plugins/final-evaluation.ts';

/** One official owned task closes search; no model call or secondary scheduler. */
export const ResearchClosureTask = defineTask<{ reason: string }, { phase: 'close' }, JsonObject>({
  name: 'qz.research.close', version: 1, initial: () => ({ phase: 'close' }),
  phases: { async close(task, runtime, context) {
    const api = researchRuntime(runtime);
    try {
      // Already-admitted tools own their computation and must settle before the
      // common comparison basis freezes. Rejected closing callers are not waited.
      const pending = (await runtime.snapshot(Study, runtime.conversationId, context))?.trials.filter(item => item.status === 'STARTED') ?? [];
      for (const trial of pending) await runtime.waitForTask(trial.taskId as Parameters<typeof runtime.waitForTask>[0], context);
      const selection = await api.commit(async tx => {
        const study = await tx.doc(Study, api.conversationId); const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
        if (study.final) {
          if (study.final.taskId !== task.id || study.final.status !== 'STARTED' || typeof study.final.trialId !== 'number') return null;
          const selected = study.final.trialId;
          return { trialId: selected, proposal: JSON.parse(JSON.stringify(study.trials.find(item => item.taskId === selected)?.proposal ?? null)) as JsonObject | null };
        }
        const comparison = await compareCampaign(tx, api);
        const selected = comparison ? comparison.recommendationStatus === 'SELECTED'
          ? campaign.trials.find(item => item.id === comparison.recommendedTrialId)?.runtime.taskId ?? null : null
          : study.input ? legacySelection(study.input, study.trials) : null;
        // Technical lack of progress never authorizes consuming independent data.
        const allowIndependent = !['research_no_new_evidence', 'orchestration_budget_exhausted', 'continuation_requires_trial_budget'].includes(task.input.reason);
        if (selected !== null && study.input?.data.sealed && allowIndependent && campaign.binding?.independentConsumer !== 'PM_PORTFOLIO') {
          campaign.state = 'EVALUATING_INDEPENDENT';
          return { trialId: selected, proposal: JSON.parse(JSON.stringify(study.trials.find(item => item.taskId === selected)?.proposal ?? null)) as JsonObject | null };
        }
        study.final = { taskId: task.id, trialId: selected, comparedTrials: study.trials.map(item => item.taskId!),
          status: 'STOPPED', reason: task.input.reason, result: null, comparisonId: comparison?.id ?? null,
          conclusion: comparison?.recommendationStatus ?? (selected === null ? 'NEEDS_EVIDENCE' : 'SELECTED'),
          scientificClaim: 'SELECTION_ONLY_NOT_INDEPENDENT_QUALIFICATION' };
        campaign.state = 'CONCLUDED';
        return null;
      }, context);
      if (selection) {
        const args = { trialId: selection.trialId, reason: task.input.reason };
        const proposal = selection.proposal;
        if (proposal && typeof proposal === 'object' && !Array.isArray(proposal) && Array.isArray(proposal.members)) await finalizePortfolio(args, api, context);
        else await finalizeFactor(args, api, context);
      }
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { closed: true } } }), context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      const message = error instanceof Error ? error.message : String(error);
      await runtime.commit(async tx => {
        const study = await tx.doc(Study, runtime.conversationId);
        study.final ??= { taskId: task.id, trialId: null, comparedTrials: study.trials.map(item => item.taskId!), status: 'STOPPED', reason: 'closure_needs_evidence',
          result: null, error: message, conclusion: 'NEEDS_EVIDENCE' };
        (await tx.doc(CampaignResearchDoc, runtime.conversationId)).state = 'CONCLUDED';
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
