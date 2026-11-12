import type { JsonObject } from '@earendil-works/pi-durable';
import wabtFactory from 'wabt';
import { object } from '../src/trials.ts';
import { partitionBoundaries } from './native-exposure.ts';
import { ExecutionFailure } from '../src/execution-errors.ts';
import { clipTickRegimeEvaluation, isTickResearchInput, nativeResearchRequest, validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';

const wabt = await wabtFactory();

export function compileWasm(model: string, filename = 'factor.wat', operation = 'evaluate-factor'): Uint8Array {
  let module: ReturnType<typeof wabt.parseWat> | undefined;
  try {
    module = wabt.parseWat(filename, model);
    module.validate();
    return module.toBinary({}).buffer;
  } catch (error) { throw new ExecutionFailure(error, { operation, stage: 'compile' }); }
  finally { module?.destroy(); }
}

export function normalizedRequest(data: JsonObject): JsonObject {
  // Native transaction drafts are JSON proxies, which structuredClone cannot copy.
  const catalog = data.catalog_scan_request !== undefined;
  if ([data.catalog_scan_request, data.request].filter(value => value !== undefined).length > 1) throw new Error('ambiguous_factor_request');
  const request = nativeResearchRequest(data);
  if (isTickResearchInput(data)) {
    partitionBoundaries(request);
    return request;
  }
  partitionBoundaries(request, catalog);
  const parameters = object(catalog ? request.parameters : object(request.forecast, 'forecast').parameters, 'parameters');
  // The native optional-fuel contract serializes both null and absence as omitted.
  if (parameters.total_fuel === null) delete parameters.total_fuel;
  return request;
}

export function iterationRequest(data: JsonObject, clipDecisionCutoff = false): JsonObject {
  const request = normalizedRequest(data);
  if (isTickResearchInput(data)) {
    const clipped = clipTickRegimeEvaluation(validateNativeTickFactorRequest(request), String(request.validation_end_ns));
    if (clipDecisionCutoff) clipped.input.decision_cutoff_ns = clipped.validation_end_ns;
    return clipped;
  }
  const selection = data.catalog_scan_request !== undefined ? request : object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  selection.event_end_ns = request.validation_end_ns!;
  if (clipDecisionCutoff) selection.decision_cutoff_ns = request.validation_end_ns!;
  return request;
}
