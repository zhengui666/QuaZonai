import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { Study } from '../src/documents.ts';
import { evaluateTrial, object, studyInput, trialFeedback } from '../src/trials.ts';
import { resolvePortfolioPolicy } from '../src/strategy.ts';
import { checkExposure } from './native-exposure.ts';
import { FittedEnsembleTask, runFittedEnsemble } from './fitted-ensemble-task.ts';

export default defineExtension({ name: 'qz.fitted-ensemble', tasks: [FittedEnsembleTask], tools: [defineTool({
  name: 'strategy_ensemble_fit', description: 'Fit calibrated nonnegative signal weights from at least two original COMPUTED Wasm factor trials. Native TRAIN-only expanding folds fit calibration and weights; unavailable full prediction panels fail explicitly. No user/model supplied weights. Consumes one ENSEMBLE_FIT attempt, including failure. This fit is not account validation.',
  parameters: Type.Object({ hypothesis: Type.String(), hypothesisId: Type.Optional(Type.String()), sourceTrialIds: Type.Array(Type.Integer(), { minItems: 2 }) }), replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const input = await studyInput(api, context); const study = await api.snapshot(Study, api.conversationId, context);
    const sources = args.sourceTrialIds.map(id => {
      const trial = study?.trials.find(item => item.taskId === id); const proposal = trial ? object(trial.proposal, 'proposal') : null;
      if (trial?.status !== 'COMPUTED' || typeof proposal?.model !== 'string') throw new Error('ensemble_source_factor_not_computed');
      return { factorVersionId: `factor:${api.conversationId}:${id}`, model: proposal.model, sourceTrialId: id };
    });
    if (new Set(args.sourceTrialIds).size !== args.sourceTrialIds.length) throw new Error('ensemble_duplicate_source');
    const proposal = { hypothesis: args.hypothesis, ...(args.hypothesisId ? { hypothesisId: args.hypothesisId } : {}),
      parentTrialIds: input.campaign ? args.sourceTrialIds.map(id => `${input.campaign!.campaignId}:trial:${id}`) : [], purpose: 'ENSEMBLE_FIT',
      members: sources.map(member => ({ ...member })), request: input.data.request ?? null, method: 'qz-expanding-train-three-fold-v1' };
    return trialFeedback(api, context, () => evaluateTrial(api, context, proposal, async () => {
      await checkExposure(api, context, input.data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined);
      return runFittedEnsemble(api, context, { mode: 'FIT', data: input.data, members: sources, binary: process.env.QZ_JOB_BINARY ?? 'job' });
    }));
  },
}), defineTool({
  name: 'strategy_ensemble_evaluate', description: 'Jointly replay one original native fitted ensemble on VALIDATION in one shared account. Reuses the frozen calibration and exact native weights; never refits or averages individual PnLs. Consumes a separate JOINT_REPLAY attempt including failure.',
  parameters: Type.Object({ hypothesis: Type.String(), hypothesisId: Type.Optional(Type.String()), fitTrialId: Type.Integer(), instrumentId: Type.Optional(Type.String()), allocations: Type.Optional(Type.Union([Type.Literal('ALL_EQUAL'), Type.Array(Type.Object({ instrumentId: Type.String(), weight: Type.String() }))])) }), replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const input = await studyInput(api, context); const study = await api.snapshot(Study, api.conversationId, context);
    const fitTrial = study?.trials.find(item => item.taskId === args.fitTrialId); const fit = fitTrial ? object(fitTrial.result, 'fit_result') : null;
    if (fitTrial?.status !== 'COMPUTED' || fit?.kind !== 'ENSEMBLE_FIT' || typeof fit.evidenceTaskId !== 'number') throw new Error('registered_ensemble_fit_missing');
    const original = object(fitTrial.proposal, 'fit_proposal');
    const proposal = { hypothesis: args.hypothesis, ...(args.hypothesisId ? { hypothesisId: args.hypothesisId } : {}), purpose: 'JOINT_REPLAY',
      parentTrialIds: input.campaign ? [`${input.campaign.campaignId}:trial:${args.fitTrialId}`] : [], fitTrialId: args.fitTrialId, fitTaskId: fit.evidenceTaskId,
      members: original.members!, request: input.data.request ?? null, strategy: input.policy.strategy ?? null,
      ...(args.instrumentId ? { instrumentId: args.instrumentId } : {}), ...(args.allocations ? { allocations: args.allocations } : {}) };
    return trialFeedback(api, context, () => evaluateTrial(api, context, proposal, async () => {
      const policy = resolvePortfolioPolicy(proposal, object(input.policy.strategy, 'strategy_policy'));
      if (policy.kind === 'catalog-ensemble') throw new Error('ensemble_full_prediction_panel_required');
      await checkExposure(api, context, input.data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined, policy.evidenceMode);
      return runFittedEnsemble(api, context, { mode: 'REPLAY', data: input.data, fitTaskId: fit.evidenceTaskId as number, policy, partition: 'VALIDATION', binary: process.env.QZ_JOB_BINARY ?? 'job' });
    }));
  },
})] });
