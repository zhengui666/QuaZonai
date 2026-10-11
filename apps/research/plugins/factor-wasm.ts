import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { evaluateTrial, studyInput, trialFeedback } from '../src/trials.ts';
import { FactorTask, runFactor } from './native-factor-task.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';

export default defineExtension({
  name: 'qz.factor-wasm', tasks: [FactorTask],
  tools: [defineTool({
    name: 'factor_evaluate',
    description: 'Evaluate an arbitrary factor, not a position: Wasm text exports predict(close,previous_close,fastEMA,slowEMA,volume,open,high,low)->f64. Values may have any finite sign or scale. Native validation statistics are feedback, not profitability or qualification.',
    parameters: Type.Object({ hypothesis: Type.String(), model: Type.String() }), replay: 'safe',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      // Record the proposal before input preparation, so missing/invalid data is
      // a disclosed failed attempt, not an erased experiment or free retry.
      const proposal = { hypothesis: args.hypothesis, model: args.model, request: input.data.request ?? null };
      return trialFeedback(api, context, () => evaluateTrial(api, context, proposal, async () => {
        const request = iterationRequest(input.data);
        await checkExposure(api, context, input.data, false);
        return runFactor(api, context, args.model, { ...input.data, request });
      }));
    },
  })],
});
