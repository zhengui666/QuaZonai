import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { evaluateTrial, object, studyInput, trialFeedback } from '../src/trials.ts';
import { FactorTask, runFactor } from './native-factor-task.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';
import { CatalogScopeTask, catalogScope } from './native-catalog-scope.ts';
import { nativeResearchRequest, withNativeResearchRequest } from '../../../packages/contracts/src/tick-input.ts';

export default defineExtension({
  name: 'qz.factor-wasm', tasks: [FactorTask, CatalogScopeTask],
  tools: [defineTool({
    name: 'factor_evaluate',
    description: 'Evaluate an arbitrary factor, not a position. BAR models export predict(close,previous_close,fastEMA,slowEMA,volume,open,high,low)->f64. Explicit TRADE_TICK models use feature ABI v2 and the seven cohort features in research_brief; BAR slots are never reinterpreted. Values may have any finite sign or scale. Validation statistics are feedback, not profitability or qualification.',
    parameters: Type.Object({ hypothesis: Type.String(), model: Type.String(), hypothesisId: Type.Optional(Type.String()), parentTrialIds: Type.Optional(Type.Array(Type.String())) }), replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      // Record the proposal before input preparation, so missing/invalid data is
      // a disclosed failed attempt, not an erased experiment or free retry.
      const proposal = { hypothesis: args.hypothesis, model: args.model, request: input.data.nativeInput !== undefined ? nativeResearchRequest(input.data) : input.data.catalog_scan_request ?? input.data.request ?? null,
        ...(args.hypothesisId ? { hypothesisId: args.hypothesisId } : {}), ...(args.parentTrialIds ? { parentTrialIds: args.parentTrialIds } : {}) };
      return trialFeedback(api, context, () => evaluateTrial(api, context, proposal, async () => {
        const request = iterationRequest(input.data);
        const catalog = input.data.catalog_scan_request !== undefined;
        const data = catalog ? await catalogScope(api, context, input.data) : input.data;
        await checkExposure(api, context, data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined);
        return runFactor(api, context, args.model, withNativeResearchRequest(data, request));
      }));
    },
  })],
});
