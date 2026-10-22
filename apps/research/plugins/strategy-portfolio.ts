import { finalizePortfolio } from './final-evaluation.ts';
import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { evaluateTrial, object, studyInput, trialFeedback } from '../src/trials.ts';
import { resolvePortfolioPolicy, strategyMembers } from '../src/strategy.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';
import { PortfolioEvaluationTask, runPortfolio } from './native-portfolio-task.ts';
import { catalogScope } from './native-catalog-scope.ts';

export default defineExtension({
  name: 'qz.strategy-portfolio', tasks: [PortfolioEvaluationTask],
  tools: [defineTool({
    name: 'strategy_portfolio_evaluate',
    description: 'Register one complete causal Wasm ensemble with exact positive member weights summing to 1. For catalog-ensemble use the registered v2 feature ABI and at least two members: every source market is scanned and capital follows fresh opportunities dynamically; do not supply instrumentId or allocations. Proposed member count and weights are search variables. Original explicitly metered member slots must match the proposed count; absent fuel stays unmetered. For cross-market-threshold use allocations:"ALL_EQUAL" or explicit admitted allocations; legacy factor-threshold optionally selects instrumentId. All markets share the original TOTAL account capital, never capital per market. ONE native shared account trades VALIDATION; conditional source/proxy assumptions remain explicit and computation is not qualification. Historical mode and all failures/controls retain the original counted budget.',
    parameters: Type.Object({ hypothesis: Type.String(), hypothesisId: Type.Optional(Type.String()), parentTrialIds: Type.Optional(Type.Array(Type.String())), members: Type.Array(Type.Object({ model: Type.String(), weight: Type.String() })), instrumentId: Type.Optional(Type.String()),
      allocations: Type.Optional(Type.Union([Type.Literal('ALL_EQUAL'), Type.Array(Type.Object({ instrumentId: Type.String(), weight: Type.String() }))])) }),
    replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      const proposal = { hypothesis: args.hypothesis, members: args.members, request: input.data.catalog_ensemble_request ?? input.data.catalog_scan_request ?? input.data.request ?? null, strategy: input.policy.strategy ?? null,
        ...(args.hypothesisId ? { hypothesisId: args.hypothesisId } : {}), ...(args.parentTrialIds ? { parentTrialIds: args.parentTrialIds } : {}),
        ...(args.instrumentId !== undefined ? { instrumentId: args.instrumentId } : {}),
        ...(args.allocations !== undefined ? { allocations: args.allocations } : {}) };
      return trialFeedback(api, context, () => evaluateTrial(api, context, proposal, async () => {
        if (input.campaign) throw new Error('campaign_native_fitted_ensemble_required');
        if (input.data.catalog_scan_request !== undefined) throw new Error('catalog_factor_only_account_binding_required');
        const members = strategyMembers(proposal);
        const policy = resolvePortfolioPolicy(proposal, object(input.policy.strategy, 'strategy_policy'));
        const request = iterationRequest(input.data, true);
        const data = policy.kind === 'catalog-ensemble' ? await catalogScope(api, context, input.data) : input.data;
        await checkExposure(api, context, data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined, policy.evidenceMode);
        return runPortfolio(api, context, { members, policy, data: policy.kind === 'catalog-ensemble' ? data : { ...data, request }, partition: 'VALIDATION', binary: process.env.QZ_JOB_BINARY ?? 'job' });
      }));
    },
  }), defineTool({
    name: 'strategy_portfolio_finalize',
    description: 'Select an original COMPUTED ensemble for ONE owner-only HELDOUT shared-account final evaluation using the original sealed binding/policy. Default exposure rules require independent data; an original HISTORICAL_REASSESSMENT binding permits only explicitly exposed historical reassessment, not unseen evidence. Ends research. Final metrics are NEVER returned for tuning. Call alone in the final round.',
    parameters: Type.Object({ trialId: Type.Integer(), reason: Type.String() }), replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      await finalizePortfolio(args, api, context);
      return { content: [{ type: 'text', text: 'Research ended. Original ensemble selection, exposure and final outcome are recorded for the owner; no final tuning feedback is returned.' }], control: { terminate: true } };
    },
  })],
});
