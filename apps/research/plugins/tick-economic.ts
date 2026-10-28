import type { Context } from '@earendil-works/chord';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { validateNativeTickEconomicRequest, validateNativeTickEconomicResult, type NativeTickEconomicRequest, type NativeTickEconomicResult } from '../../../packages/contracts/src/economic-tick.ts';
import { canonical } from '../src/documents.ts';
import { executeNativeValue } from '../src/native-execution.ts';

/** One frozen retained model and explicit scenario; this does not create a factor trial. */
export async function replayTickEconomic(target: NativeTarget, request: NativeTickEconomicRequest,
  artifacts: { catalog: string; fundingCatalog?: string; model: string }, directory: string, context: Context): Promise<NativeTickEconomicResult> {
  const admitted = validateNativeTickEconomicRequest(request);
  const result = validateNativeTickEconomicResult(await executeNativeValue(target, 'replay-tick-economic', admitted,
    ['--catalog', artifacts.catalog, '--funding-catalog', artifacts.fundingCatalog ?? artifacts.catalog, '--model', artifacts.model], directory, context));
  if (canonical(result.request) !== canonical(admitted)) throw new Error('tick_economic_result_request_mismatch');
  return result;
}
