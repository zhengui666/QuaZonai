import type { JsonObject } from '@earendil-works/pi-durable';
import { object } from './trials.ts';
import { canonical } from './documents.ts';
import { normalizedRequest } from '../plugins/native-wasm.ts';

/** Explicit score-to-target mapping, never an expected-return estimate. */
export interface ThresholdPolicy {
  name: string;
  kind: 'factor-threshold';
  instrumentId: string;
  threshold: number;
  investedWeight: string;
  targetTtlNs: string;
  simulationSettings: JsonObject;
  forecastMode?: 'price-volume';
  evaluationPartition?: 'HELDOUT';
}
export interface StrategyMember { model: string; weight: string }
export type StrategyPartition = 'VALIDATION' | 'HELDOUT';

export function exactWeights(weights: unknown[]): string[] {
  if (!weights.length || weights.some((weight) => typeof weight !== 'string' || !/^\d+(?:\.\d+)?$/.test(weight))) throw new Error('portfolio_weights_invalid');
  const values = weights as string[];
  const precision = values.reduce((maximum, weight) => Math.max(maximum, weight.split('.')[1]?.length ?? 0), 0);
  const one = 10n ** BigInt(precision);
  const units = values.map((weight) => { const [whole, fraction = ''] = weight.split('.'); return BigInt(whole + fraction.padEnd(precision, '0')); });
  if (units.some((weight) => weight <= 0n || weight > one) || units.reduce((sum, weight) => sum + weight, 0n) !== one) throw new Error('portfolio_weights_invalid');
  return values;
}

export function strategyMembers(proposal: JsonObject): StrategyMember[] {
  if (proposal.members === undefined) {
    if (typeof proposal.model !== 'string' || !proposal.model.trim()) throw new Error('selected_factor_model_required');
    return [{ model: proposal.model, weight: '1' }];
  }
  if (!Array.isArray(proposal.members) || !proposal.members.length) throw new Error('strategy_members_required');
  const members = proposal.members.map((value) => {
    const member = object(value, 'strategy_member');
    if (typeof member.model !== 'string' || !member.model.trim()) throw new Error('selected_factor_model_required');
    return { model: member.model, weight: member.weight as string };
  });
  exactWeights(members.map((member) => member.weight));
  return members;
}

export function thresholdPolicy(value: JsonObject): ThresholdPolicy {
  if (value.kind !== 'factor-threshold') throw new Error('strategy_adapter_unsupported');
  if (value.forecastMode !== undefined && value.forecastMode !== 'price-volume') throw new Error('strategy_forecast_mode_invalid');
  if (value.evaluationPartition !== undefined && value.evaluationPartition !== 'HELDOUT') throw new Error('strategy_evaluation_partition_invalid');
  return value as unknown as ThresholdPolicy;
}

function counter(value: unknown, name: string): string {
  if (typeof value !== 'string' || !/^\d+$/.test(value)) throw new Error(`${name}_counter_required`);
  return value;
}

// Extracted from the original portfolio consumer. Keep clocks and missing decisions intact.
export function thresholdTargets(forecasts: JsonObject, policy: ThresholdPolicy, currency: string): JsonObject[] {
  if (policy.kind !== 'factor-threshold') throw new Error('strategy_adapter_unsupported');
  if (!Number.isFinite(policy.threshold)) throw new Error('strategy_threshold_invalid');
  const weight = policy.investedWeight;
  if (typeof weight !== 'string' || !/^\d+(?:\.\d+)?$/.test(weight)) throw new Error('policy_weight_decimal_required');
  const [whole, fraction = ''] = weight.split('.');
  if (fraction.length > 18) throw new Error('decimal_precision_unsupported');
  const units = BigInt(whole + fraction.padEnd(18, '0'));
  if (units <= 0n || units > 10n ** 18n) throw new Error('strategy_weight_invalid');
  const ttl = BigInt(counter(policy.targetTtlNs, 'target_ttl')); if (ttl <= 0n) throw new Error('target_ttl_invalid');
  if (policy.simulationSettings.base_currency !== currency) throw new Error('strategy_currency_mismatch');
  if (!Array.isArray(forecasts.points)) throw new Error('native_forecast_points_missing');
  const available = forecasts.points.map((point) => object(point, 'forecast_point')).filter((point) => point.instrument_id === policy.instrumentId && typeof point.forecast === 'number');
  if (!available.length) throw new Error('strategy_observations_missing');
  available.sort((a, b) => {
    for (const clock of ['available_ns', 'event_ns']) {
      const left = BigInt(counter(a[clock], clock)); const right = BigInt(counter(b[clock], clock));
      if (left !== right) return left < right ? -1 : 1;
    }
    return 0;
  });
  const points: JsonObject[] = [];
  for (const point of available) {
    const asof = counter(point.available_ns, 'available');
    if (BigInt(asof) < BigInt(counter(point.event_ns, 'event'))) throw new Error('strategy_clock_invalid');
    const invested = (point.forecast as number) > policy.threshold;
    const cashUnits = 10n ** 18n - (invested ? units : 0n);
    const cash = `${cashUnits / 10n ** 18n}.${(cashUnits % 10n ** 18n).toString().padStart(18, '0')}`;
    const target: JsonObject = { schema_version: 1, asof_ns: asof, valid_until_ns: (BigInt(asof) + ttl).toString(),
      targets: [{ instrument_id: policy.instrumentId, currency, weight: invested ? weight : '0' }], cash_weight: cash };
    if (points.at(-1)?.asof_ns === asof) points[points.length - 1] = target; else points.push(target);
  }
  return points;
}

export function simulationSelection(request: JsonObject, partition?: StrategyPartition): JsonObject {
  const selection = structuredClone(object(object(request.forecast, 'forecast_request').selection, 'selection'));
  if (partition === 'VALIDATION') { selection.event_start_ns = request.train_end_ns!; selection.event_end_ns = request.validation_end_ns!; selection.decision_cutoff_ns = request.validation_end_ns!; }
  if (partition === 'HELDOUT') selection.event_start_ns = request.validation_end_ns!;
  return selection;
}

export function intervalTargets(points: JsonObject[], selection: JsonObject): JsonObject[] {
  const start = BigInt(counter(selection.event_start_ns, 'simulation_start')); const end = BigInt(counter(selection.event_end_ns, 'simulation_end'));
  return points.filter((point) => { const clock = BigInt(counter(point.asof_ns, 'decision')); return start <= clock && clock < end; });
}

/** Match the original final request, never substitute the research-visible prefix. */
export function replayCatalog(data: JsonObject, request: JsonObject): string {
  const sealed = data.sealed ? object(data.sealed, 'heldout_data') : null;
  const original = normalizedRequest({ request });
  for (const binding of sealed ? [sealed, data] : [data]) {
    if (canonical(normalizedRequest(binding)) === canonical(original)) {
      if (typeof binding.catalog !== 'string') throw new Error('catalog_required');
      return binding.catalog;
    }
  }
  throw new Error('strategy_request_binding_mismatch');
}
