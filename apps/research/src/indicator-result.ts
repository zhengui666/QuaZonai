import { readFile } from 'node:fs/promises';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { JsonValue } from '@earendil-works/chord';
import { canonical } from './documents.ts';
import { object } from './trials.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';

// Source of truth is the generated official-execution contract, not a second version pin.
const executionSchemas = JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8')).components.schemas;
type ExecutionVariant = { properties: { adapter_kind: { enum: string[] }; upstream_version?: { enum: string[] } } };
export const executionVersions = new Set<string>((executionSchemas.NativeModelRefV1.oneOf as ExecutionVariant[])
  .filter(variant => variant.properties.adapter_kind.enum[0]?.startsWith('NAUTILUS_'))
  .flatMap(variant => variant.properties.upstream_version?.enum ?? []));

// These are DecimalValue fields in this request, not arbitrary string identifiers.
const decimals = new Set(['long_weight', 'starting_capital', 'leverage', 'exposure_tolerance', 'maker', 'taker', 'prob_fill_on_limit', 'prob_slippage']);
function equalRequest(left: JsonValue, right: JsonValue, key = ''): boolean {
  if (decimals.has(key) && typeof left === 'string' && typeof right === 'string') return compareDecimal(left, right) === 0;
  if (Array.isArray(left) && Array.isArray(right)) return left.length === right.length && left.every((item, i) => equalRequest(item, right[i]!));
  if (left && right && typeof left === 'object' && typeof right === 'object' && !Array.isArray(left) && !Array.isArray(right)) {
    const keys = Object.keys(left);
    return keys.length === Object.keys(right).length && keys.every(name => name in right && equalRequest(left[name]!, right[name]!, name));
  }
  return canonical(left) === canonical(right);
}
function normalizedRequest(value: JsonObject): JsonObject {
  const result = structuredClone(value);
  const selection = object(result.selection, 'indicator_strategy_selection');
  if (selection.maximum_rows === undefined) selection.maximum_rows = 0; // Existing Rust serde default.
  if (result.observations === null) delete result.observations;
  const settings = object(result.settings, 'indicator_strategy_settings');
  if (settings.multi_currency_spot_cash === null) delete settings.multi_currency_spot_cash;
  return result;
}
const fail = (): never => { throw new Error('indicator_strategy_result_invalid'); };
function counter(value: JsonValue | undefined): bigint {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value)) return fail();
  const parsed = BigInt(value);
  if (parsed > 9223372036854775807n) return fail();
  return parsed;
}
const nullableText = (value: JsonValue | undefined) => value === null || typeof value === 'string';
const nullableNumber = (value: JsonValue | undefined) => value === null || typeof value === 'number' && Number.isFinite(value);

/** Validate the native wire evidence before a completed Task or Study can be committed. */
export function indicatorStrategyResult(value: JsonValue, request: JsonObject): JsonObject {
  const result = object(value, 'indicator_strategy_result');
  const echoed = object(result.request, 'indicator_strategy_echoed_request');
  if (!equalRequest(normalizedRequest(request), normalizedRequest(echoed))) throw new Error('indicator_strategy_request_mismatch');
  if (result.performance_scope !== 'FULL_SELECTION_INCLUDING_PRE_DECISION_CASH' || result.decision_policy !== 'BATCH_LAST_SETTLED_ACCOUNT_ONLY') fail();
  counter(result.skipped_unsettled_decisions); counter(result.merged_same_time_bars);
  const simulation = object(result.simulation, 'indicator_strategy_simulation');
  if (simulation.schema_version !== 1 || typeof simulation.native_version !== 'string' || !executionVersions.has(simulation.native_version)) fail();
  const settings = object(request.settings, 'indicator_strategy_settings');
  for (const key of ['fee_model', 'fill_model', 'latency_model']) {
    if (object(settings[key], 'indicator_strategy_execution_model').upstream_version !== simulation.native_version) throw new Error('indicator_strategy_native_version_mismatch');
  }
  for (const key of ['iterations', 'events', 'orders', 'positions', 'consumed_target_points']) counter(simulation[key]);
  if (counter(simulation.iterations) === 0n || counter(simulation.consumed_target_points) === 0n) fail();
  counter(simulation.settled_replay_ns);
  const summary = object(simulation.summary, 'indicator_strategy_summary');
  if (Object.values(summary).some(item => typeof item !== 'string') || summary['orders.open'] !== '0' || summary['orders.inflight'] !== '0') fail();
  const canonicalResult = object(simulation.canonical_result, 'indicator_strategy_canonical_result');
  if (canonicalResult.schema !== 'nautilus-backtest-result/v1') fail();
  const run = object(canonicalResult.run, 'indicator_strategy_canonical_run');
  if (!['completed', 'stopped'].includes(String(run.outcome))) fail();
  for (const [native, wire] of [['iterations', 'iterations'], ['total_events', 'events'], ['total_orders', 'orders'], ['total_positions', 'positions']]) {
    if (counter(run[native!]) !== counter(simulation[wire!])) fail();
  }
  for (const key of ['accounts', 'diagnostics', 'fills', 'orders', 'portfolio_snapshots', 'position_snapshots', 'positions']) if (!Array.isArray(canonicalResult[key])) fail();
  object(canonicalResult.components, 'indicator_strategy_canonical_components');
  object(canonicalResult.statistics, 'indicator_strategy_canonical_statistics');
  const nativeSummary = object(canonicalResult.summary, 'indicator_strategy_canonical_summary');
  if (Object.entries(nativeSummary).some(([key, item]) => summary[key] !== item)) fail();
  if (!Array.isArray(simulation.statistics) || !Array.isArray(simulation.returns) || !Array.isArray(result.observations)) fail();
  for (const raw of simulation.statistics as JsonValue[]) {
    const row = object(raw, 'indicator_strategy_statistic');
    if (!['PNL', 'RETURNS', 'GENERAL'].includes(String(row.group)) || typeof row.native_key !== 'string' || !row.native_key
      || !nullableText(row.currency) || !nullableText(row.reason_code) || !nullableNumber(row.value)) fail();
  }
  if (simulation.returns_kind !== 'PORTFOLIO_DAILY' || !['OK', 'INSUFFICIENT_DATA', 'UNSUPPORTED', 'INVALID_INPUT', 'FAILED'].includes(String(simulation.returns_status)) || !nullableText(simulation.returns_reason)) fail();
  let previousReturn = -1n;
  for (const raw of simulation.returns as JsonValue[]) {
    const row = object(raw, 'indicator_strategy_return');
    const time = counter(row.timestamp_ns);
    if (time <= previousReturn || !nullableNumber(row.value) || !nullableText(row.reason_code)) fail();
    previousReturn = time;
  }
  const observations = result.observations as JsonValue[];
  if (request.observations ? observations.length === 0 : observations.length !== 0) fail();
  let previous = -1n;
  for (const raw of observations) {
    const row = object(raw, 'indicator_strategy_observation');
    const event = counter(row.event_ns); const available = counter(row.available_ns);
    if (event > available || available < previous) fail();
    for (const key of ['atr', 'vwap', 'stochastics_k', 'stochastics_d', 'donchian_upper', 'donchian_middle', 'donchian_lower']) if (!nullableNumber(row[key])) fail();
    previous = available;
  }
  return result;
}
