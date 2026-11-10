import { arr, int, lit, obj, opt, str, union } from '@earendil-works/pi-schema';
import { nativeTargetForData } from '../src/native-execution.ts';
import { finalizePortfolio } from './final-evaluation.ts';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { evaluateTrial, object, studyInput, trialFeedback, trialFeedbackSchema } from '../src/trials.ts';
import { resolvePortfolioPolicy, strategyMembers } from '../src/strategy.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';
import { PortfolioEvaluationTask, runPortfolio } from './native-portfolio-task.ts';
import { isTickResearchInput } from '../../../packages/contracts/src/tick-input.ts';

export default defineExtension({
  name: 'qz.strategy-portfolio', tasks: [PortfolioEvaluationTask],
  tools: [defineTool({
    name: 'strategy_portfolio_evaluate', structuredOutputSchema: trialFeedbackSchema,
    description: 'Register one complete causal Wasm ensemble with exact positive member weights summing to 1. Proposed member count and weights are search variables. Scientific execution is unmetered; historical fuel settings do not limit proposed members. For cross-market-threshold use allocations:"ALL_EQUAL" or explicit admitted allocations; legacy factor-threshold optionally selects instrumentId. All markets share the original TOTAL account capital, never capital per market. ONE native shared account trades VALIDATION; conditional source/proxy assumptions remain explicit and computation is not qualification. Historical mode and all failures/controls retain the original trial ledger.',
    parameters: obj({ hypothesis: str(), hypothesisId: opt(str()), parentTrialIds: opt(arr(str())), members: arr(obj({ model: str(), weight: str() })), instrumentId: opt(str()),
      allocations: opt(union([lit('ALL_EQUAL'), arr(obj({ instrumentId: str(), weight: str() }))])) }),
    replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      const proposal = { hypothesis: args.hypothesis, members: args.members, request: input.data.catalog_scan_request ?? input.data.request ?? null, strategy: input.policy.strategy ?? null,
        ...(args.hypothesisId ? { hypothesisId: args.hypothesisId } : {}), ...(args.parentTrialIds ? { parentTrialIds: args.parentTrialIds } : {}),
        ...(args.instrumentId !== undefined ? { instrumentId: args.instrumentId } : {}),
        ...(args.allocations !== undefined ? { allocations: args.allocations } : {}) };
      return trialFeedback(api, context, () => evaluateTrial(api, context, proposal, async () => {
        if (isTickResearchInput(input.data)) throw new Error('tick_portfolio_unsupported');
        if (input.campaign) throw new Error('campaign_native_fitted_ensemble_required');
        if (input.data.catalog_scan_request !== undefined) throw new Error('catalog_factor_only_account_binding_required');
        const members = strategyMembers(proposal);
        const policy = resolvePortfolioPolicy(proposal, object(input.policy.strategy, 'strategy_policy'));
        const request = iterationRequest(input.data, true);
        const data = input.data;
        await checkExposure(api, context, data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined, policy.evidenceMode);
        return runPortfolio(api, context, { members, policy, data: { ...data, request }, partition: 'VALIDATION', binary: nativeTargetForData(input.data) });
      }), true);
    },
  }), defineTool({
    name: 'strategy_portfolio_finalize',
    description: 'Select an original COMPUTED ensemble for ONE owner-only HELDOUT shared-account final evaluation using the original sealed binding/policy. Default exposure rules require independent data; an original HISTORICAL_REASSESSMENT binding permits only explicitly exposed historical reassessment, not unseen evidence. Ends research. Final metrics are NEVER returned for tuning. Call alone in the final round.',
    parameters: obj({ trialId: int(), reason: str() }), replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      await finalizePortfolio(args, api, context);
      return { output: [{ type: 'text', text: 'Research ended. Original ensemble selection, exposure and final outcome are recorded for the owner; no final tuning feedback is returned.' }], control: { terminate: true } };
    },
  })],
});
