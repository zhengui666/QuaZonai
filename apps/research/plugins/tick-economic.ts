import { withoutExecutionQuotas } from '../../../packages/contracts/src/execution-quotas.ts';
import type { Context } from '@earendil-works/chord';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { validateNativeTickEconomicRequest, validateNativeTickEconomicResult, type NativeTickEconomicRequest, type NativeTickEconomicResult } from '../../../packages/contracts/src/economic-tick.ts';
import { canonical } from '../src/documents.ts';
import { executeNativeValue } from '../src/native-execution.ts';

/** Explicit retained signals share one account; this does not create a factor trial. */
export async function replayTickEconomic(target: NativeTarget, request: NativeTickEconomicRequest,
  artifacts: { catalog: string; fundingCatalog?: string; model: string | string[]; bookCores?: number }, directory: string, context: Context): Promise<NativeTickEconomicResult> {
  const admitted = validateNativeTickEconomicRequest(request);
  const models = Array.isArray(artifacts.model) ? artifacts.model : [artifacts.model];
  if (!models.length || (admitted.signal_weights?.length ?? 1) !== models.length) throw new Error('tick_economic_signal_weights_model_mismatch');
  const result = validateNativeTickEconomicResult(await executeNativeValue(target, 'replay-tick-economic', admitted,
    ['--catalog', artifacts.catalog, '--funding-catalog', artifacts.fundingCatalog ?? artifacts.catalog, ...models.flatMap(model => ['--model', model]), ...(artifacts.bookCores && artifacts.bookCores > 1 ? ['--book-cores', String(artifacts.bookCores)] : [])], directory, context));
  if (canonical(withoutExecutionQuotas(result.request)) !== canonical(withoutExecutionQuotas(admitted))) throw new Error('tick_economic_result_request_mismatch');
  return result;
}
