import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, hook, section, ToolTask } from '@earendil-works/pi-durable';
import { Study } from '../src/documents.ts';
import { object, studyInput } from '../src/trials.ts';
import { modelFeedback } from '../src/feedback.ts';

export default defineExtension({
  name: 'qz.research',
  sections: [section('research', () => `Research factors, not mandatory trading strategies. Read research_brief; follow nextAfterTrialId using afterTrialId to disclose every page. Use research_trial(trialId) to recover original validation feedback instead of reevaluating a proposal. Propose a falsifiable hypothesis, use the registered data/factor/evaluation tools, inspect validation feedback and iterate. Keep failed and compared trials. Never tune against final held-out results, look ahead, invent data or call computation qualification. Respect the original criteria, explicit trial budget and stop rule. Use the registered final evaluator for a selected factor if an unseen holdout is configured; otherwise research_finish. Finalize in a single-tool round, without reads or computations in that round. Portfolios and target delivery are optional.`)],
  // Native terminate is unanimous per round, not an immediate global stop.
  // Mark subsequent read/error results terminal after a recorded final; never
  // return final metrics to the model or permit more scientific computation.
  hooks: [hook(ToolTask, {
    async afterTool(_call, result, api, context) {
      if ((await api.snapshot(Study, api.conversationId, context))?.final) return { ...result, control: { ...result.control, terminate: true } };
    },
  })],
  tools: [
    defineTool({
      name: 'research_brief', description: 'Original objective, available research data and fixed policy with a compact trial ledger page. Follow nextAfterTrialId with afterTrialId for complete disclosure. research_trial reads original validation feedback without computing. No final held-out values.',
      parameters: Type.Object({ afterTrialId: Type.Optional(Type.Integer({ minimum: 1 })), limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 20 })) }), replay: 'safe',
      async execute(args, api, context) {
        const input = await studyInput(api, context);
        const { sealed: _sealed, ...data } = input.data;
        const study = await api.snapshot(Study, api.conversationId, context);
        const trials = study?.trials ?? [];
        const available = trials.filter((trial) => (trial.taskId as number) > (args.afterTrialId ?? 0)).sort((a, b) => (a.taskId as number) - (b.taskId as number));
        const page = available.slice(0, args.limit ?? 10);
        return { content: [{ type: 'text', text: JSON.stringify({
          trials: page.map((trial) => { const proposal = object(trial.proposal, 'trial_proposal'); return {
            taskId: trial.taskId, status: trial.status, error: trial.error ?? null, hypothesis: proposal.hypothesis ?? null,
            instrumentId: proposal.instrumentId ?? null, memberCount: Array.isArray(proposal.members) ? proposal.members.length : null,
          }; }), totalTrials: trials.length, nextAfterTrialId: available.length > page.length ? page.at(-1)!.taskId : null,
          goal: input.goal, data, policy: input.policy, stopped: study?.final !== null,
        }) }] };
      },
    }),
    defineTool({
      name: 'research_trial', description: 'Read one original recorded trial in this conversation: status, failure and validation feedback only. Does not compute, consume a trial, read raw evidence or expose final held-out results. Use it to recover feedback, never resubmit merely to read a result.',
      parameters: Type.Object({ trialId: Type.Integer({ minimum: 1 }) }), replay: 'safe',
      async execute(args, api, context) {
        const study = await api.snapshot(Study, api.conversationId, context);
        const trial = study?.trials.find((item) => item.taskId === args.trialId);
        if (!trial) throw new Error('research_trial_not_found');
        const proposal = object(trial.proposal, 'trial_proposal');
        return { content: [{ type: 'text', text: JSON.stringify({ trialId: trial.taskId, status: trial.status, error: trial.error ?? null,
          hypothesis: proposal.hypothesis ?? null, instrumentId: proposal.instrumentId ?? null,
          result: trial.result === null || trial.result === undefined ? null : modelFeedback(object(trial.result, 'trial_result')),
        }) }] };
      },
    }),
    defineTool({
      name: 'research_finish', description: 'End research without exposing an unseen sample. Call alone in a final tool round. Record the stop reason and optional selected trial. Use a registered final evaluator when final held-out assessment is requested.',
      parameters: Type.Object({ reason: Type.String(), trialId: Type.Optional(Type.Integer()) }),
      replay: 'safe', executionMode: 'sequential',
      async execute(args, api, context) {
        await api.commit(async (tx) => {
          const study = await tx.doc(Study, api.conversationId);
          if (study.final !== null) {
            if (study.final.taskId !== api.taskId) throw new Error('research_already_final');
            return;
          }
          const selected = study.trials.find((item) => item.taskId === args.trialId);
          if (args.trialId !== undefined && selected?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
          if (args.trialId !== undefined && study.input?.data.sealed) throw new Error('registered_final_evaluator_required');
          study.final = { taskId: api.taskId, trialId: args.trialId ?? null, comparedTrials: study.trials.map((item) => item.taskId!), reason: args.reason, status: 'STOPPED', result: null };
        }, context);
        return { content: [{ type: 'text', text: 'Research stopped; original trials and selection are recorded.' }], control: { terminate: true } };
      },
    }),
  ],
});
