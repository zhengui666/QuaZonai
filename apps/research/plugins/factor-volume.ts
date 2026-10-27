import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { evaluateTrial, object, studyInput, trialFeedback } from '../src/trials.ts';
import { runFactor } from './native-factor-task.ts';
import { iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';
import { catalogScope } from './native-catalog-scope.ts';
import { isTickResearchInput, nativeResearchRequest, withNativeResearchRequest } from '../../../packages/contracts/src/tick-input.ts';

export default defineExtension({
  name: 'qz.factor-volume',
  tools: [defineTool({
    name: 'factor_volume', description: 'Additional factor plugin: signed volume deviation, computed by the same native Wasm evaluator. No positions or portfolio required.',
    parameters: Type.Object({ hypothesis: Type.String(), center: Type.Number() }), replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      const input = await studyInput(api, context);
      const model = `(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) local.get 4 f64.const ${args.center} f64.sub))`;
      return trialFeedback(api, context, () => evaluateTrial(api, context, { hypothesis: args.hypothesis, model, request: input.data.nativeInput !== undefined ? nativeResearchRequest(input.data) : input.data.catalog_scan_request ?? input.data.request ?? null }, async () => {
        if (isTickResearchInput(input.data)) throw new Error('tick_factor_volume_unsupported');
        const request = iterationRequest(input.data);
        const catalog = input.data.catalog_scan_request !== undefined;
        const data = catalog ? await catalogScope(api, context, input.data) : input.data;
        await checkExposure(api, context, data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined);
        return runFactor(api, context, model, withNativeResearchRequest(data, request));
      }));
    },
  })],
});
