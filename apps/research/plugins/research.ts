import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, hook, section, ToolTask } from '@earendil-works/pi-durable';
import { Study } from '../src/documents.ts';
import { studyInput } from '../src/trials.ts';

export default defineExtension({
  name: 'qz.research',
  sections: [section('research', () => `Research factors, not mandatory trading strategies. Read research_brief, propose a falsifiable hypothesis, use the registered data/factor/evaluation tools, inspect validation feedback and iterate. Keep failed and compared trials. Never tune against final held-out results, look ahead, invent data or call computation qualification. Respect the original criteria, explicit trial budget and stop rule. Use the registered final evaluator for a selected factor if an unseen holdout is configured; otherwise research_finish. Finalize in a single-tool round, without reads or computations in that round. Portfolios and target delivery are optional.`)],
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
      name: 'research_brief', description: 'Original objective, available research data, fixed policy and complete trial disclosure; no final held-out values.',
      parameters: Type.Object({}), replay: 'safe',
      async execute(_args, api, context) {
        const input = await studyInput(api, context);
        const { sealed: _sealed, ...data } = input.data;
        const study = await api.snapshot(Study, api.conversationId, context);
        return { content: [{ type: 'text', text: JSON.stringify({ goal: input.goal, data, policy: input.policy, trials: study?.trials, stopped: study?.final !== null }) }] };
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
