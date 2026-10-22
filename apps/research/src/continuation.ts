import { defineDoc, defineExtension, GenerationTask, hook } from '@earendil-works/pi-durable';
import type { Harness, TaskId } from '@earendil-works/pi-durable';
import { canonical, Study } from './documents.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { closeResearch, ResearchClosureTask } from './closure.ts';

// Conversation facts survive successor GenerationTasks and restarts. The last
// decision is replayable across a crash before native onYield places its input.
export const Continuation = defineDoc<{
  yields: number; noProgress: number; trials: number; taskId: TaskId | null; continue: boolean; stopReason: string | null;
  evidenceSignature?: string;
}>({
  kind: 'qz.research-continuation', version: 1, scope: 'conversation', history: 'latest', fork: 'current',
  initial: () => ({ yields: 0, noProgress: 0, trials: 0, taskId: null, continue: false, stopReason: null }),
});

export function researchContinuation(owner: () => Harness) {
  return defineExtension({ name: 'qz.research-continuation', tasks: [ResearchClosureTask], hooks: [hook(GenerationTask, {
    async afterResponse(_answer, api, context) {
      const harness = owner();
      const exhausted = await harness.commit(async tx => {
        const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
        if (!campaign.orchestration || (await tx.doc(Study, api.conversationId)).final) return false;
        if (!campaign.orchestration.generationTaskIds.includes(api.taskId)) campaign.orchestration.generationTaskIds.push(api.taskId);
        return campaign.orchestration.generationTaskIds.length >= campaign.orchestration.maximumModelTurns;
      }, context);
      if (exhausted) await closeResearch({ taskId: api.taskId, conversationId: api.conversationId,
        commit: harness.commit.bind(harness), snapshot: harness.snapshot.bind(harness), snapshotAsOf: harness.snapshotAsOf.bind(harness),
        getTask: harness.getTask.bind(harness), waitForTask: harness.waitForTask.bind(harness) }, context, 'orchestration_budget_exhausted', false);
    },
    async onYield(_answer, api, context) {
      const harness = owner();
      const conversation = await harness.conversation(api.conversationId, context);
      if (!(await conversation?.agent(context))?.extensions.some((extension) => extension.name === 'qz.research')) return;
      const decision = await harness.commit(async (tx) => {
        const study = await tx.doc(Study, api.conversationId);
        if (!study.input) return { again: false, reason: null };
        if (study.final) {
          const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
          return { again: false, reason: study.final.status === 'STARTED' && campaign.closureTaskId !== null ? campaign.closingReason : null };
        }
        const closing = await tx.doc(CampaignResearchDoc, api.conversationId);
        if (closing.state !== 'SEARCHING') return { again: false, reason: closing.closingReason };
        const state = await tx.doc(Continuation, api.conversationId);
        if (state.taskId === api.taskId) return { again: state.continue, reason: state.stopReason };
        const maximum = study.input.policy.max_trials;
        const trials = study.trials.filter((trial) => trial.status !== 'REJECTED').length;
        state.noProgress = trials > state.trials ? 0 : state.noProgress + 1;
        state.trials = trials; state.yields++; state.taskId = api.taskId;
        const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
        // New citations or reworded hypotheses alone are not scientific progress.
        const signature = canonical(study.trials.map(trial => ({ taskId: trial.taskId!, status: trial.status! })));
        const reason = typeof maximum !== 'number' ? 'continuation_requires_trial_budget'
          : trials >= maximum ? 'trial_budget_exhausted'
          : state.evidenceSignature === signature ? 'research_no_new_evidence' : null;
        state.evidenceSignature = signature;
        state.continue = reason === null; state.stopReason = reason;
        if (reason) { campaign.state = 'CLOSING'; campaign.closingReason = reason; }
        return { again: state.continue, reason };
      }, context);
      if (decision.reason) await closeResearch({ taskId: api.taskId, conversationId: api.conversationId,
        commit: harness.commit.bind(harness), snapshot: harness.snapshot.bind(harness), snapshotAsOf: harness.snapshotAsOf.bind(harness),
        getTask: harness.getTask.bind(harness), waitForTask: harness.waitForTask.bind(harness) }, context, decision.reason, false);
      if (decision.again) return { continue: 'The requested study is unfinished. The running native host supplies successor generations: a response boundary, tool-call count or finite invocation is not a scientific stop rule. Continue using the original research_brief, criteria and remaining trial budget; no separate background loop is required. Do not reset trials or budgets or promise execution after the host closes. Call research_finish only for an actual original stop condition, not because this response must end. Repeating an unchanged scientific ledger closes orchestration with NEEDS_EVIDENCE, not economic failure. Finalize in a single-tool round; never use final held-out evidence to tune.' };
    },
  })] });
}
