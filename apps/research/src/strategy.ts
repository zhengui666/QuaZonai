import type { JsonObject } from '@earendil-works/pi-durable';
import { object } from './trials.ts';
import { canonical } from './documents.ts';
import { normalizedRequest } from '../plugins/native-wasm.ts';

/** Explicit score-to-target mapping, never an expected-return estimate. */
export interface ThresholdPolicy {
  name: string;
  kind: 'factor-threshold' | 'cross-market-threshold';
  instrumentId?: string;
  instrumentIds?: string[];
  allocations?: MarketAllocation[];
  threshold: number;
  investedWeight: string;
  targetTtlNs: string;
  simulationSettings: JsonObject;
  forecastMode?: 'price-volume';
  evaluationPartition?: 'HELDOUT';
  evidenceMode?: 'HISTORICAL_REASSESSMENT';
}
export type MarketAllocation = { instrumentId: string; weight: string };
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
  if (value.kind !== 'factor-threshold' && value.kind !== 'cross-market-threshold') throw new Error('strategy_adapter_unsupported');
  if (value.forecastMode !== undefined && value.forecastMode !== 'price-volume') throw new Error('strategy_forecast_mode_invalid');
  if (value.evaluationPartition !== undefined && value.evaluationPartition !== 'HELDOUT') throw new Error('strategy_evaluation_partition_invalid');
  if (value.evidenceMode !== undefined && value.evidenceMode !== 'HISTORICAL_REASSESSMENT') throw new Error('strategy_evidence_mode_invalid');
  if (value.instrumentIds !== undefined) {
    const ids = value.instrumentIds;
    if (!Array.isArray(ids) || ids.length < (value.kind === 'cross-market-threshold' ? 2 : 1) || ids.some((id) => typeof id !== 'string' || !id.trim())
      || new Set(ids).size !== ids.length) throw new Error('strategy_instrument_universe_invalid');
    if (value.kind === 'factor-threshold' && !ids.includes(value.instrumentId!)) throw new Error('strategy_base_instrument_not_allowed');
  }
  if (value.kind === 'cross-market-threshold' && value.instrumentIds === undefined) throw new Error('strategy_instrument_universe_invalid');
  return value as unknown as ThresholdPolicy;
}

/** Resolve only a registered proposal's choice inside the original admitted universe. */
export function resolveStrategyPolicy(proposal: JsonObject, original: JsonObject): ThresholdPolicy {
  const policy = thresholdPolicy(original);
  if (policy.kind === 'cross-market-threshold') {
    if (proposal.instrumentId !== undefined) throw new Error('strategy_market_single_choice_invalid');
    const allocations = marketAllocations(proposal.allocations, policy.instrumentIds!);
    return { ...policy, allocations, simulationSettings: selectedSettings(policy.simulationSettings, allocations.map((item) => item.instrumentId)) };
  }
  if (proposal.allocations !== undefined) throw new Error('strategy_market_policy_required');
  if (proposal.instrumentId === undefined && policy.instrumentIds === undefined) return policy;
  const selected = proposal.instrumentId === undefined ? policy.instrumentId : proposal.instrumentId;
  if (typeof selected !== 'string' || !selected.trim()) throw new Error('strategy_instrument_invalid');
  if (!(policy.instrumentIds ?? [policy.instrumentId]).includes(selected)) throw new Error('strategy_instrument_not_allowed');
  if (policy.instrumentIds === undefined) return { ...policy, instrumentId: selected };
  return { ...policy, instrumentId: selected, simulationSettings: selectedSettings(policy.simulationSettings, [selected]) };
}

function selectedSettings(original: JsonObject, ids: string[]): JsonObject {
  const settings = JSON.parse(JSON.stringify(original)) as JsonObject;
  if (!Array.isArray(settings.fee_rates)) throw new Error('strategy_instrument_fee_missing');
  const rates = settings.fee_rates;
  for (const id of ids) {
    const matched = rates.filter((rate) => rate !== null && typeof rate === 'object' && !Array.isArray(rate) && rate.instrument_id === id);
    if (!matched.length) throw new Error('strategy_instrument_fee_missing');
    if (matched.length !== 1) throw new Error('strategy_instrument_fee_duplicate');
  }
  settings.fee_rates = rates.filter((rate) => ids.includes(String(object(rate, 'fee_rate').instrument_id)));
  return settings;
}

function marketAllocations(value: unknown, universe: string[]): MarketAllocation[] {
  if (!Array.isArray(value) || value.length < 2) throw new Error('strategy_market_allocations_required');
  const allocations = value.map((entry) => {
    const item = object(entry, 'market_allocation');
    if (typeof item.instrumentId !== 'string' || !universe.includes(item.instrumentId)) throw new Error('strategy_market_not_allowed');
    return { instrumentId: item.instrumentId, weight: item.weight as string };
  });
  if (new Set(allocations.map((item) => item.instrumentId)).size !== allocations.length) throw new Error('strategy_market_duplicate');
  exactWeights(allocations.map((item) => item.weight));
  return allocations.sort((a, b) => universe.indexOf(a.instrumentId) - universe.indexOf(b.instrumentId));
}

function counter(value: unknown, name: string): string {
  if (typeof value !== 'string' || !/^\d+$/.test(value)) throw new Error(`${name}_counter_required`);
  return value;
}

// Extracted from the original portfolio consumer. Keep clocks and missing decisions intact.
export function thresholdTargets(forecasts: JsonObject, policy: ThresholdPolicy, currency: string): JsonObject[] {
  if (policy.kind !== 'factor-threshold' && policy.kind !== 'cross-market-threshold') throw new Error('strategy_adapter_unsupported');
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
  if (policy.kind === 'cross-market-threshold') return crossMarketTargets(forecasts.points, policy, currency, units, ttl);
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
      targets: [{ instrument_id: policy.instrumentId!, currency, weight: invested ? weight : '0' }], cash_weight: cash };
    if (points.at(-1)?.asof_ns === asof) points[points.length - 1] = target; else points.push(target);
  }
  return points;
}

/** Causal union-clock decisions, not independent accounts or synchronized/padded BARs. */
function crossMarketTargets(rows: JsonObject[string][], policy: ThresholdPolicy, currency: string, invested: bigint, ttl: bigint): JsonObject[] {
  const allocations = marketAllocations(policy.allocations, policy.instrumentIds!);
  const ids = allocations.map((item) => item.instrumentId);
  const budgets = allocations.map(({ weight }) => {
    const [whole, fraction = ''] = weight.split('.');
    return BigInt(whole + fraction) * invested / (10n ** BigInt(fraction.length));
  });
  const observations = rows.map((row) => object(row, 'forecast_point')).filter((row) => ids.includes(String(row.instrument_id)));
  observations.sort((a, b) => {
    const left = BigInt(counter(a.available_ns, 'available')); const right = BigInt(counter(b.available_ns, 'available'));
    return left < right ? -1 : left > right ? 1 : ids.indexOf(String(a.instrument_id)) - ids.indexOf(String(b.instrument_id));
  });
  if (!observations.length) throw new Error('strategy_market_observations_missing');
  const signals = new Map<string, { until: bigint; active: boolean }>();
  const decimal = (value: bigint) => `${value / 10n ** 18n}.${(value % 10n ** 18n).toString().padStart(18, '0')}`;
  const result: JsonObject[] = [];
  for (let index = 0; index < observations.length;) {
    const now = BigInt(counter(observations[index]!.available_ns, 'available'));
    const updated = new Set<string>();
    do {
      const row = observations[index++]!; const id = String(row.instrument_id);
      if (updated.has(id)) throw new Error('strategy_market_duplicate_clock');
      if (now < BigInt(counter(row.event_ns, 'event'))) throw new Error('strategy_clock_invalid');
      if (row.forecast !== null && (typeof row.forecast !== 'number' || !Number.isFinite(row.forecast))) throw new Error('strategy_forecast_nonfinite');
      updated.add(id);
      signals.set(id, { until: now + ttl, active: typeof row.forecast === 'number' && row.forecast > policy.threshold });
    } while (index < observations.length && BigInt(counter(observations[index]!.available_ns, 'available')) === now);
    let cash = 10n ** 18n;
    const targets = ids.map((id, asset) => {
      const signal = signals.get(id);
      const amount = signal?.active && signal.until > now ? budgets[asset]! : 0n;
      cash -= amount;
      return { instrument_id: id, currency, weight: decimal(amount) };
    });
    // A stale/expired desired target does not liquidate actual residual holdings.
    // Only the explicitly observed IDs can trade; native equity retains all positions.
    result.push({ schema_version: 1, asof_ns: String(now), valid_until_ns: String(now + ttl), targets,
      cash_weight: decimal(cash), rebalance_instrument_ids: ids.filter((id) => updated.has(id)) });
  }
  return result;
}

export function simulationSelection(request: JsonObject, partition?: StrategyPartition, policy?: ThresholdPolicy): JsonObject {
  const selection = structuredClone(object(object(request.forecast, 'forecast_request').selection, 'selection'));
  if (partition === 'VALIDATION') { selection.event_start_ns = request.train_end_ns!; selection.event_end_ns = request.validation_end_ns!; selection.decision_cutoff_ns = request.validation_end_ns!; }
  if (partition === 'HELDOUT') selection.event_start_ns = request.validation_end_ns!;
  if (policy?.instrumentIds !== undefined) {
    if (!Array.isArray(selection.bar_types) || selection.bar_types.some((bar) => typeof bar !== 'string')) throw new Error('strategy_bar_types_invalid');
    // A BAR type ends in step/aggregation/price/source; symbols themselves can contain hyphens.
    const ids = policy.kind === 'cross-market-threshold' ? marketAllocations(policy.allocations, policy.instrumentIds).map((item) => item.instrumentId) : [policy.instrumentId!];
    const bars = (selection.bar_types as string[]).filter((bar) => ids.includes(bar.split('-').slice(0, -4).join('-')));
    if (ids.some((id) => !bars.some((bar) => bar.split('-').slice(0, -4).join('-') === id))) throw new Error('strategy_instrument_bars_missing');
    selection.bar_types = bars;
  }
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
