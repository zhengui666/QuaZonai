import { nativeTargetForData } from '../src/native-execution.ts';
import type { Context } from '@earendil-works/chord';
import { canonical, Study } from '../src/documents.ts';
import { object, studyInput } from '../src/trials.ts';
import { resolvePortfolioPolicy, strategyMembers } from '../src/strategy.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';
import { runPortfolio } from './native-portfolio-task.ts';
import { runFactor } from './native-factor-task.ts';
import { classifyExecutionError, executionDiagnostic } from '../src/execution-errors.ts';
import { catalogScope } from './native-catalog-scope.ts';
import { freezeSelection } from '../src/pm-comparison.ts';
import { CampaignResearchDoc } from '../src/pm-campaign.ts';
import type { ResearchRuntime } from '../src/research-runtime.ts';
import { runFittedEnsemble } from './fitted-ensemble-task.ts';
import { isTickResearchInput, nativeResearchRequest } from '../../../packages/contracts/src/tick-input.ts';

export async function finalizePortfolio(args: {trialId: number; reason: string}, api: ResearchRuntime, context: Context): Promise<void> {
      const input = await studyInput(api, context);
      if (isTickResearchInput(input.data)) throw new Error('tick_portfolio_unsupported');
      if (input.campaign?.independentConsumer === 'PM_PORTFOLIO') throw new Error('independent_evaluation_reserved_for_pm');
      if (input.data.catalog_scan_request !== undefined) throw new Error('catalog_factor_only_account_binding_required');
      const data = object(input.data.sealed, 'heldout_data');
      const selected = await api.commit(async (tx) => {
        const study = await tx.doc(Study, api.conversationId);
        if (study.final !== null && study.final.taskId !== api.taskId) throw new Error('research_already_final');
        const trial = study.trials.find((item) => item.taskId === args.trialId);
        if (trial?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
        const proposal = object(trial.proposal, 'proposal');
        if (!Array.isArray(proposal.members)) throw new Error('selected_portfolio_evaluator_unavailable');
        const fitted = typeof proposal.fitTaskId === 'number';
        const members = fitted ? [] : strategyMembers(proposal);
        const result = object(trial.result, 'trial_result');
        const registeredRequest = input.data.catalog_ensemble_request !== undefined ? result.registered_request ?? result.request : result.request;
        if (canonical(proposal.request!) !== canonical(input.data.catalog_ensemble_request ?? input.data.request!) || canonical(proposal.strategy!) !== canonical(input.policy.strategy!)
          || canonical(iterationRequest(data, true)) !== canonical(registeredRequest!)
          || canonical(iterationRequest(input.data, true)) !== canonical(registeredRequest!)) throw new Error('heldout_criteria_mismatch');
        const comparison = await freezeSelection(tx, api, args.trialId);
        study.final ??= { taskId: api.taskId, trialId: args.trialId, comparedTrials: study.trials.map((item) => item.taskId!), reason: args.reason, comparison, status: 'STARTED', result: null, error: null };
        const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
        if (campaign.binding && study.final.status === 'STARTED') campaign.state = 'EVALUATING_INDEPENDENT';
        return { members, fitTaskId: fitted ? proposal.fitTaskId as number : null, policy: resolvePortfolioPolicy(proposal, object(input.policy.strategy, 'strategy_policy')), settled: study.final.status !== 'STARTED' };
      }, context);
      if (!selected.settled) {
        try {
          const scoped = selected.policy.kind === 'catalog-ensemble' ? await catalogScope(api, context, data) : data;
          await checkExposure(api, context, scoped, true, undefined, selected.policy.evidenceMode);
          if (selected.fitTaskId !== null && selected.policy.kind === 'catalog-ensemble') throw new Error('ensemble_full_prediction_panel_required');
          const result = selected.fitTaskId !== null && selected.policy.kind !== 'catalog-ensemble'
            ? await runFittedEnsemble(api, context, { mode: 'REPLAY', fitTaskId: selected.fitTaskId, policy: selected.policy, data: scoped, partition: 'HELDOUT', binary: nativeTargetForData(scoped) })
            : await runPortfolio(api, context, { members: selected.members, policy: selected.policy, data: scoped, partition: 'HELDOUT', binary: nativeTargetForData(scoped) });
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!; final.result = result; final.status = 'COMPUTED'; }, context);
        } catch (error) {
          if (context.abortSignal?.aborted) throw error;
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!;
            final.status = 'FAILED'; final.error = executionDiagnostic(error, 'heldout_failed'); final.errorDetail = { ...classifyExecutionError(error) };
          }, context);
        }
      }
      await api.commit(async tx => { const doc = await tx.doc(CampaignResearchDoc, api.conversationId); if (doc.binding) doc.state = 'CONCLUDED'; }, context);
}

export async function finalizeFactor(args: {trialId: number; reason: string}, api: ResearchRuntime, context: Context): Promise<void> {
      const input = await studyInput(api, context);
      if (input.campaign?.independentConsumer === 'PM_PORTFOLIO') throw new Error('independent_evaluation_reserved_for_pm');
      const data = object(input.data.sealed, 'heldout_data');
      const selected = await api.commit(async (tx) => {
        const study = await tx.doc(Study, api.conversationId);
        if (study.final !== null && study.final.taskId !== api.taskId) throw new Error('research_already_final');
        const trial = study.trials.find((item) => item.taskId === args.trialId);
        if (trial?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
        const proposal = object(trial.proposal, 'proposal');
        if (typeof proposal.model !== 'string') throw new Error('selected_factor_evaluator_unavailable');
        const iterative = iterationRequest(data);
        if (canonical(iterative) !== canonical(object(trial.result, 'trial_result').request!)
          || canonical(iterative) !== canonical(iterationRequest(input.data))
          || input.data.nativeInput !== undefined && canonical(proposal.request!) !== canonical(nativeResearchRequest(input.data))) throw new Error('heldout_criteria_mismatch');
        const comparison = await freezeSelection(tx, api, args.trialId);
        study.final ??= { taskId: api.taskId, trialId: args.trialId, comparedTrials: study.trials.map((item) => item.taskId!), reason: args.reason, comparison, status: 'STARTED', result: null, error: null };
        const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
        if (campaign.binding && study.final.status === 'STARTED') campaign.state = 'EVALUATING_INDEPENDENT';
        return { model: proposal.model, settled: study.final.status !== 'STARTED' };
      }, context);
      if (!selected.settled) {
        try {
          const scoped = data.catalog_scan_request !== undefined ? await catalogScope(api, context, data) : data;
          await checkExposure(api, context, scoped, true);
          const result = await runFactor(api, context, selected.model, scoped);
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!; final.result = result; final.status = 'COMPUTED'; }, context);
        } catch (error) {
          if (context.abortSignal?.aborted) throw error;
          await api.commit(async (tx) => { const final = (await tx.doc(Study, api.conversationId)).final!;
            final.status = 'FAILED'; final.error = executionDiagnostic(error, 'heldout_failed'); final.errorDetail = { ...classifyExecutionError(error) };
          }, context);
        }
      }
      await api.commit(async tx => { const doc = await tx.doc(CampaignResearchDoc, api.conversationId); if (doc.binding) doc.state = 'CONCLUDED'; }, context);
}
