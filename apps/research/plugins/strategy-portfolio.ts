import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { canonical, Study } from '../src/documents.ts';
import { evaluateTrial, object, studyInput, trialFeedback } from '../src/trials.ts';
import { resolveStrategyPolicy, strategyMembers } from '../src/strategy.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';
import { PortfolioEvaluationTask, runPortfolio } from './native-portfolio-task.ts';

export default defineExtension({
  name: 'qz.strategy-portfolio', tasks: [PortfolioEvaluationTask],
  tools: [defineTool({
    name: 'strategy_portfolio_evaluate',
    description: 'Propose one complete Wasm ensemble with exact positive decimal weights summing to 1. Optionally select instrumentId inside the original fixed instrumentIds universe; absence retains the admitted base instrument. Fixed threshold/fees/capital map each component to targets; native exact blending and ONE shared account trade VALIDATION only. Feedback is native account statistics and component IC/count summaries, never heldout or raw arrays. Each proposal, including failure, consumes one original trial.',
    parameters: Type.Object({ hypothesis: Type.String(), members: Type.Array(Type.Object({ model: Type.String(), weight: Type.String() })), instrumentId: Type.Optional(Type.String()) }),
    replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      const proposal = { hypothesis: args.hypothesis, members: args.members, request: input.data.request ?? null, strategy: input.policy.strategy ?? null,
        ...(args.instrumentId !== undefined ? { instrumentId: args.instrumentId } : {}) };
      return trialFeedback(api, context, () => evaluateTrial(api, context, proposal, async () => {
        const members = strategyMembers(proposal);
        const policy = resolveStrategyPolicy(proposal, object(input.policy.strategy, 'strategy_policy'));
        const request = iterationRequest(input.data, true);
        await checkExposure(api, context, input.data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined);
        return runPortfolio(api, context, { members, policy, data: { ...input.data, request }, partition: 'VALIDATION', binary: process.env.QZ_JOB_BINARY ?? 'job' });
      }));
    },
  }), defineTool({
    name: 'strategy_portfolio_finalize',
    description: 'Select an original COMPUTED ensemble for ONE independent HELDOUT shared-account evaluation using the original sealed binding/policy. Ends research. Final metrics are owner-only and are NEVER returned for tuning. Call alone in the final round.',
    parameters: Type.Object({ trialId: Type.Integer(), reason: Type.String() }), replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      const data = object(input.data.sealed, 'heldout_data');
      const selected = await api.commit(async (tx) => {
        const study = await tx.doc(Study, api.conversationId);
        if (study.final !== null && study.final.taskId !== api.taskId) throw new Error('research_already_final');
        const trial = study.trials.find((item) => item.taskId === args.trialId);
        if (trial?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
        const proposal = object(trial.proposal, 'proposal');
        if (!Array.isArray(proposal.members)) throw new Error('selected_portfolio_evaluator_unavailable');
        const members = strategyMembers(proposal);
        if (canonical(proposal.request!) !== canonical(input.data.request!) || canonical(proposal.strategy!) !== canonical(input.policy.strategy!)
          || canonical(iterationRequest(data, true)) !== canonical(object(trial.result, 'trial_result').request!)
          || canonical(iterationRequest(input.data, true)) !== canonical(object(trial.result, 'trial_result').request!)) throw new Error('heldout_criteria_mismatch');
        study.final ??= { taskId: api.taskId, trialId: args.trialId, comparedTrials: study.trials.map((item) => item.taskId!), reason: args.reason, status: 'STARTED', result: null, error: null };
        return { members, policy: resolveStrategyPolicy(proposal, object(input.policy.strategy, 'strategy_policy')), settled: study.final.status !== 'STARTED' };
      }, context);
      if (!selected.settled) {
        try {
          await checkExposure(api, context, data, true);
          const result = await runPortfolio(api, context, { members: selected.members, policy: selected.policy, data, partition: 'HELDOUT', binary: process.env.QZ_JOB_BINARY ?? 'job' });
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!; final.result = result; final.status = 'COMPUTED'; }, context);
        } catch (error) {
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!; final.status = 'FAILED'; final.error = error instanceof Error ? error.message : 'heldout_failed'; }, context);
        }
      }
      return { content: [{ type: 'text', text: 'Research ended. Original ensemble selection, exposure and independent outcome are recorded for the owner; no final tuning feedback is returned.' }], control: { terminate: true } };
    },
  })],
});
