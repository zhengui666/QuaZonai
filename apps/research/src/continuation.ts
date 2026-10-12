import { defineDoc, defineExtension, GenerationTask, hook } from '@earendil-works/pi-durable';
import type { Harness, TaskId } from '@earendil-works/pi-durable';
import { Study } from './documents.ts';

// Conversation facts survive successor GenerationTasks and restarts. The last
// decision is replayable across a crash before native onYield places its input.
export const Continuation = defineDoc<{
  yields: number; noProgress: number; trials: number; taskId: TaskId | null; continue: boolean; stopReason: string | null;
}>({
  kind: 'qz.research-continuation', version: 1, scope: 'conversation', history: 'latest', fork: 'current',
  initial: () => ({ yields: 0, noProgress: 0, trials: 0, taskId: null, continue: false, stopReason: null }),
});

export function researchContinuation(owner: () => Harness) {
  return defineExtension({ name: 'qz.research-continuation', hooks: [hook(GenerationTask, {
    async onYield(_answer, api, context) {
      const harness = owner();
      const conversation = await harness.conversation(api.conversationId, context);
      if (!(await conversation?.agent(context))?.extensions.some((extension) => extension.name === 'qz.research')) return;
      const again = await harness.commit(async (tx) => {
        const study = await tx.doc(Study, api.conversationId);
        if (!study.input || study.final) return false;
        const state = await tx.doc(Continuation, api.conversationId);
        if (state.taskId === api.taskId) return state.continue;
        const maximum = study.input.policy.max_trials;
        const trials = study.trials.filter((trial) => trial.status !== 'REJECTED').length;
        state.noProgress = trials > state.trials ? 0 : state.noProgress + 1;
        state.trials = trials; state.yields++; state.taskId = api.taskId;
        const reason = typeof maximum !== 'number' ? 'continuation_requires_trial_budget'
          : trials >= maximum ? 'trial_budget_exhausted'
          : state.noProgress >= 2 ? 'no_progress_continuation_limit'
          : state.yields > maximum + 2 ? 'continuation_limit' : null;
        state.continue = reason === null; state.stopReason = reason;
        if (reason) study.final = { taskId: api.taskId, trialId: null, comparedTrials: study.trials.map((trial) => trial.taskId!), status: 'STOPPED', reason, result: null };
        return state.continue;
      }, context);
      if (again) return { continue: 'The requested study is unfinished. Continue using the original research_brief, criteria and remaining trial budget. Do not reset trials or budgets. If the objective cannot be completed, call research_finish with the actual reason. Finalize in a single-tool round; never use final held-out evidence to tune.' };
    },
  })] });
}
