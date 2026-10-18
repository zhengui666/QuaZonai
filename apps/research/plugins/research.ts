import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, hook, section, ToolTask } from '@earendil-works/pi-durable';
import { Study } from '../src/documents.ts';
import { object, studyInput } from '../src/trials.ts';
import { marketPage, modelBriefInput, modelFeedback } from '../src/feedback.ts';

export default defineExtension({
  name: 'qz.research',
  sections: [section('research', () => `Research factors or the owner's explicitly admitted strategy, not a mandatory downstream consumer. Read research_brief; follow nextAfterTrialId using afterTrialId for the trial ledger. For cross-market policy use research_markets(offset,limit) and nextOffset to read the complete original training-market universe and fee reserves; pages are response bounds, never market/trial caps. Use research_trial(trialId) to recover validation feedback instead of reevaluating. Propose falsifiable complete candidates and preserve failed/compared trials. Never tune against final results, look ahead, invent data or call computation qualification. Respect original criteria, budget and stop rule. The running native host supplies successor generations when an unfinished response yields; a response boundary, tool-call count or finite invocation is not a scientific stop rule and requires no separate loop. Do not claim execution after host closure. Use research_finish only at an actual original stop condition. Use the registered final evaluator when requested; finalize alone without other reads/computations in that round. Target delivery remains optional.`)],
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
        const { data, policy } = modelBriefInput(input);
        const study = await api.snapshot(Study, api.conversationId, context);
        const trials = study?.trials ?? [];
        const available = trials.filter((trial) => (trial.taskId as number) > (args.afterTrialId ?? 0)).sort((a, b) => (a.taskId as number) - (b.taskId as number));
        const page = available.slice(0, args.limit ?? 10);
        return { content: [{ type: 'text', text: JSON.stringify({
          trials: page.map((trial) => { const proposal = object(trial.proposal, 'trial_proposal');
            const scope = proposal.allocations === 'ALL_EQUAL' ? proposal.strategy : null;
            const selected = scope && typeof scope === 'object' && !Array.isArray(scope) ? scope.instrumentIds : proposal.allocations;
            return {
            taskId: trial.taskId, status: trial.status, error: trial.error ?? null, hypothesis: proposal.hypothesis ?? null,
            instrumentId: proposal.instrumentId ?? null, marketCount: Array.isArray(selected) ? selected.length : null,
            memberCount: Array.isArray(proposal.members) ? proposal.members.length : null,
          }; }), totalTrials: trials.length, nextAfterTrialId: available.length > page.length ? page.at(-1)!.taskId : null,
          goal: input.goal, data, policy, stopped: study?.final !== null,
        }) }] };
      },
    }),
    defineTool({
      name: 'research_markets', description: 'Read a page of the original admitted cross-market universe in its original training order, with original training-only metadata and fee sizing reserves. Follow nextOffset to read all markets. No computation, attempt consumption, final-period performance, inferred event independence or external data.',
      parameters: Type.Object({ offset: Type.Optional(Type.Integer({ minimum: 0 })), limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 50 })) }), replay: 'safe',
      async execute(args, api, context) {
        const input = await studyInput(api, context);
        return { content: [{ type: 'text', text: JSON.stringify(marketPage(input, args.offset ?? 0, args.limit ?? 20)) }] };
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
      name: 'research_finish', description: 'End research at an actual original stop condition without exposing an unseen sample. A response boundary, tool-call count or finite invocation is not a stop condition: the running native host supplies successor generations for unfinished research. Call alone in a final tool round. Record the actual stop reason and optional selected trial. Use a registered final evaluator when final held-out assessment is requested.',
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
