import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { Study, canonical } from '../src/documents.ts';
import { object, studyInput } from '../src/trials.ts';
import { runFactor } from './native-factor-task.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';

export default defineExtension({
  name: 'qz.heldout-wasm',
  tools: [defineTool({
    name: 'factor_finalize', description: 'Select one recorded Wasm factor before a single unseen held-out assessment. End research; final metrics are only published to the owner, never returned for tuning.',
    parameters: Type.Object({ reason: Type.String(), trialId: Type.Integer() }),
    replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      const data = object(input.data.sealed, 'heldout_data');
      const selected = await api.commit(async (tx) => {
        const study = await tx.doc(Study, api.conversationId);
        if (study.final !== null && study.final.taskId !== api.taskId) throw new Error('research_already_final');
        const trial = study.trials.find((item) => item.taskId === args.trialId);
        if (trial?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
        const proposal = object(trial.proposal, 'proposal');
        if (typeof proposal.model !== 'string') throw new Error('selected_factor_evaluator_unavailable');
        const iterative = iterationRequest(data);
        if (canonical(iterative) !== canonical(object(trial.result, 'trial_result').request!)) throw new Error('heldout_criteria_mismatch');
        study.final ??= { taskId: api.taskId, trialId: args.trialId, comparedTrials: study.trials.map((item) => item.taskId!), reason: args.reason, status: 'STARTED', result: null, error: null };
        return { model: proposal.model, settled: study.final.status !== 'STARTED' };
      }, context);
      if (!selected.settled) {
        try {
          await checkExposure(api, context, data, true);
          const result = await runFactor(api, context, selected.model, data);
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!; final.result = result; final.status = 'COMPUTED'; }, context);
        } catch (error) {
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!; final.status = 'FAILED'; final.error = error instanceof Error ? error.message : 'heldout_failed'; }, context);
        }
      }
      return { content: [{ type: 'text', text: 'Research ended. Selection, exposure and final outcome are recorded for the owner; no final tuning feedback is returned.' }], control: { terminate: true } };
    },
  })],
});
