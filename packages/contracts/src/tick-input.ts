import type { JsonObject } from './index.ts';

export type NativeTickFactorRequest = {
  schema_version: 1;
  input: {
    modality: 'TRADE_TICK'; instrument_ids: string[];
    event_start_ns: string; event_end_ns: string; decision_cutoff_ns: string;
    source_revision: string;
    availability: 'EVENT_TIME_PROXY_UNVERIFIED';
    representation?: 'EVENT_TIME_COHORT_CACHE_V1';
    maximum_rows?: number; maximum_source_rows?: string;
  };
  sampling: { kind: 'EVENT_TIME_COHORT_V1'; maximum_cohort_rows: number };
  target: { kind: 'FIXED_TRADE_COHORTS'; horizon_cohorts: number } | { kind: 'FIXED_EVENT_TIME'; horizon_ns: string };
  parameters: { fast_period_cohorts: number; slow_period_cohorts: number; total_fuel?: string };
  train_end_ns: string; validation_end_ns: string;
};

/** Tagged BAR currently wraps the existing explicit-selection factor request.
 * Catalog scans and ensemble requests retain their existing legacy contracts. */
export type NativeResearchInput = { modality: 'TRADE_TICK'; request: NativeTickFactorRequest }
  | { modality: 'BAR'; request: JsonObject };

export const TICK_FEATURE_ABI = 'qz_set_feature_v2(i32,f64,i32,i64,i64)->();qz_predict_v2(i64,i64,i32,i32,i32)->f64';
export const TICK_FEATURE_KEYS = ['cohort_vwap', 'previous_cohort_vwap', 'fast_ema', 'slow_ema', 'cohort_quantity', 'trade_count', 'elapsed_event_ns'] as const;

function object(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${name}_required`);
  return value as Record<string, unknown>;
}
// Durable transaction values can be JSON proxies, which structuredClone rejects.
function copy<T>(value: T): T { return JSON.parse(JSON.stringify(value)) as T; }
function fields(value: Record<string, unknown>, allowed: string[]) {
  if (Object.keys(value).some(key => !allowed.includes(key))) throw new Error('native_tick_unknown_field');
}
function counter(value: unknown, name: string, positive = false): bigint {
  if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 9223372036854775807n || positive && BigInt(value) === 0n) throw new Error(`native_tick_${name}_invalid`);
  return BigInt(value);
}
function integer(value: unknown, name: string, maximum: number): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 1 || value > maximum) throw new Error(`native_tick_${name}_invalid`);
  return value;
}

export function validateNativeTickFactorRequest(value: unknown): NativeTickFactorRequest {
  const request = object(value, 'native_tick_request');
  fields(request, ['schema_version', 'input', 'sampling', 'target', 'parameters', 'train_end_ns', 'validation_end_ns']);
  if (request.schema_version !== 1) throw new Error('native_tick_schema_version_unsupported');
  const input = object(request.input, 'native_tick_input');
  fields(input, ['modality', 'instrument_ids', 'event_start_ns', 'event_end_ns', 'decision_cutoff_ns', 'maximum_rows', 'source_revision', 'availability', 'representation', 'maximum_source_rows']);
  if (input.modality !== 'TRADE_TICK') throw new Error('native_tick_modality_required');
  if (input.availability !== 'EVENT_TIME_PROXY_UNVERIFIED') throw new Error('native_tick_availability_unsupported');
  if (!Array.isArray(input.instrument_ids) || !input.instrument_ids.length || input.instrument_ids.length > 1_000_000 || input.instrument_ids.some(id => typeof id !== 'string' || !id || id.length > 200 || /[\s\u0000-\u001f\u007f]/.test(id)) || new Set(input.instrument_ids).size !== input.instrument_ids.length) throw new Error('native_tick_instrument_ids_invalid');
  if (typeof input.source_revision !== 'string' || !input.source_revision.trim() || input.source_revision.length > 512 || /[\u0000-\u001f\u007f]/.test(input.source_revision)) throw new Error('native_tick_source_revision_required');
  if (input.representation === 'EVENT_TIME_COHORT_CACHE_V1') {
    if (input.maximum_rows !== undefined) throw new Error('native_tick_cache_row_budget_ambiguous');
    counter(input.maximum_source_rows, 'maximum_source_rows', true);
  } else {
    if (input.representation !== undefined) throw new Error('native_tick_representation_unsupported');
    if (input.maximum_source_rows !== undefined) throw new Error('native_tick_cache_row_budget_ambiguous');
    integer(input.maximum_rows, 'maximum_rows', 1_000_000);
  }
  const start = counter(input.event_start_ns, 'event_start');
  const train = counter(request.train_end_ns, 'train_end');
  const validation = counter(request.validation_end_ns, 'validation_end');
  const end = counter(input.event_end_ns, 'event_end');
  const cutoff = counter(input.decision_cutoff_ns, 'decision_cutoff');
  if (!(start < train && train < validation && validation <= end && end <= cutoff)) throw new Error('native_tick_partition_boundaries');
  const sampling = object(request.sampling, 'native_tick_sampling');
  fields(sampling, ['kind', 'maximum_cohort_rows']);
  if (sampling.kind !== 'EVENT_TIME_COHORT_V1') throw new Error('native_tick_sampling_unsupported');
  integer(sampling.maximum_cohort_rows, 'maximum_cohort_rows', 100_000);
  const target = object(request.target, 'native_tick_target');
  if (target.kind === 'FIXED_TRADE_COHORTS') {
    fields(target, ['kind', 'horizon_cohorts']);
    integer(target.horizon_cohorts, 'horizon_cohorts', 1_000_000);
  } else if (target.kind === 'FIXED_EVENT_TIME') {
    fields(target, ['kind', 'horizon_ns']);
    const horizon = counter(target.horizon_ns, 'horizon_ns', true);
    if (end - 1n + horizon > 9223372036854775807n) throw new Error('native_tick_horizon_ns_overflow');
  } else throw new Error('native_tick_target_unsupported');
  const parameters = object(request.parameters, 'native_tick_parameters');
  fields(parameters, ['fast_period_cohorts', 'slow_period_cohorts', 'total_fuel']);
  const fast = integer(parameters.fast_period_cohorts, 'fast_period_cohorts', 1_000_000);
  const slow = integer(parameters.slow_period_cohorts, 'slow_period_cohorts', 1_000_000);
  if (fast >= slow) throw new Error('native_tick_ema_periods_invalid');
  if (parameters.total_fuel !== undefined) counter(parameters.total_fuel, 'total_fuel', true);
  return copy(request) as NativeTickFactorRequest;
}

/** No directory inference and no tagged/legacy joins. Legacy absence stays legacy. */
export function nativeResearchInput(data: Record<string, unknown>): NativeResearchInput | null {
  if (data.nativeInput === undefined) return null;
  if (['request', 'catalog_scan_request', 'catalog_ensemble_request'].some(key => data[key] !== undefined)) throw new Error('ambiguous_factor_request');
  const tagged = object(data.nativeInput, 'native_research_input');
  fields(tagged, ['modality', 'request']);
  if (tagged.modality === 'TRADE_TICK') return { modality: 'TRADE_TICK', request: validateNativeTickFactorRequest(tagged.request) };
  if (tagged.modality !== 'BAR') throw new Error('native_research_modality_unsupported');
  const request = object(tagged.request, 'native_bar_request');
  const forecast = object(request.forecast, 'native_bar_forecast');
  const selection = object(forecast.selection, 'native_bar_selection');
  if (!Array.isArray(selection.bar_types) || !selection.bar_types.length || selection.bar_types.some(value => typeof value !== 'string')) throw new Error('bar_types_required');
  return { modality: 'BAR', request: copy(request) as JsonObject };
}

export function isTickResearchInput(data: Record<string, unknown>): boolean {
  return nativeResearchInput(data)?.modality === 'TRADE_TICK';
}

export function nativeResearchRequest(data: Record<string, unknown>): JsonObject {
  const tagged = nativeResearchInput(data);
  const request = tagged?.request ?? data.catalog_ensemble_request ?? data.catalog_scan_request ?? data.request;
  return copy(object(request, 'native_request')) as JsonObject;
}

/** Preserve the original tagged representation when clipping iterative reads. */
export function withNativeResearchRequest(data: JsonObject, request: JsonObject): JsonObject {
  const tagged = nativeResearchInput(data);
  if (tagged) return { ...data, nativeInput: { modality: tagged.modality, request } };
  return data.catalog_scan_request !== undefined ? { ...data, catalog_scan_request: request } : { ...data, request };
}
