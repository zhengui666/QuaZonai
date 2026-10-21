import type { JsonObject } from '@earendil-works/pi-durable';
import { readFile } from 'node:fs/promises';
import { object } from './trials.ts';
import { canonical } from './documents.ts';
import { normalizedRequest } from '../plugins/native-wasm.ts';

const nativeSchemas = JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8')).components.schemas;
const nativeSimulationSchema = nativeSchemas.NativeSimulationSettingsV1;
const nativeCurrencies = new Set<string>(nativeSimulationSchema.properties.base_currency.oneOf.flatMap((choice: { enum: string[] }) => choice.enum));
export const maxDbCounter = 9223372036854775807n;

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
export interface CatalogEnsemblePolicy {
  name: string;
  kind: 'catalog-ensemble';
  evaluationPartition?: 'HELDOUT';
  evidenceMode?: 'HISTORICAL_REASSESSMENT';
}
export type StrategyPolicy = ThresholdPolicy | CatalogEnsemblePolicy;
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
  const policy = value as unknown as ThresholdPolicy;
  thresholdParameters(policy);
  return policy;
}

/** Static bindings from domain::portfolio::simulation_settings/models, before paid work. */
export function validateNativeSimulationSettings(value: unknown, instrumentIds: readonly string[] = [], mode: 'explicit' | 'catalog-ensemble' = 'explicit'): JsonObject {
  const settings = object(value as JsonObject, 'strategy_simulation_settings');
  if (Object.keys(settings).some(key => !Object.hasOwn(nativeSimulationSchema.properties, key))) throw new Error('strategy_simulation_settings_invalid');
  if (settings.multi_currency_spot_cash !== undefined && settings.multi_currency_spot_cash !== null) throw new Error('multi_currency_spot_cash_not_integrated');
  if (typeof settings.base_currency !== 'string' || !settings.base_currency.trim()) throw new Error('strategy_currency_required');
  if (!nativeCurrencies.has(settings.base_currency)) throw new Error('strategy_currency_unsupported');
  if (typeof settings.starting_capital !== 'string') throw new Error('strategy_starting_capital_required');
  if (settings.schema_version !== 1 || typeof settings.account_kind !== 'string' || !['CASH', 'MARGIN'].includes(settings.account_kind)
    || typeof settings.snapshot_interval_ms !== 'number' || !Number.isInteger(settings.snapshot_interval_ms)
    || settings.snapshot_interval_ms < 1 || settings.snapshot_interval_ms > 86_400_000) throw new Error('strategy_simulation_settings_invalid');
  const one = 10n ** 18n;
  const capital = nativeDecimal(settings.starting_capital); const leverage = nativeDecimal(settings.leverage); const tolerance = nativeDecimal(settings.exposure_tolerance);
  if (capital <= 0n || leverage <= 0n || leverage > 100n * one || tolerance <= 0n || tolerance > one / 1000n
    || settings.account_kind === 'CASH' && leverage !== one) throw new Error('strategy_simulation_settings_invalid');
  const classes: Record<string, string> = {
    NAUTILUS_DEFAULT_FILL: 'nautilus_execution::models::fill::DefaultFillModel',
    NAUTILUS_MAKER_TAKER: 'nautilus_execution::models::fee::MakerTakerFeeModel',
    NAUTILUS_POLYMARKET: 'nautilus_polymarket::models::PolymarketFeeModel',
    NAUTILUS_STATIC_LATENCY: 'nautilus_execution::models::latency::StaticLatencyModel',
  };
  for (const field of ['fee_model', 'fill_model', 'latency_model']) {
    const model = object(settings[field], 'strategy_simulation_model');
    const adapters = field === 'fill_model' ? ['NAUTILUS_DEFAULT_FILL', 'POLYMARKET_V2_RECORDED_LIQUIDITY']
      : field === 'latency_model' ? ['NAUTILUS_STATIC_LATENCY'] : ['NAUTILUS_MAKER_TAKER', 'NAUTILUS_POLYMARKET', 'POLYMARKET_V2_RECORDED_FEES'];
    if (model.schema_version !== 1 || typeof model.adapter_kind !== 'string' || !adapters.includes(model.adapter_kind)
      || !(model.adapter_kind.startsWith('POLYMARKET_')
        ? model.native_version === '0.63.0'
        : model.upstream_class === classes[model.adapter_kind] && model.upstream_version === '0.63.0')) throw new Error('strategy_simulation_model_invalid');
    const modelFields = model.adapter_kind.startsWith('POLYMARKET_') ? ['schema_version', 'adapter_kind', 'native_version', 'parameters']
      : ['schema_version', 'adapter_kind', 'upstream_class', 'upstream_version', 'parameters'];
    if (Object.keys(model).some(key => !modelFields.includes(key))) throw new Error('strategy_simulation_model_invalid');
    const parameters = object(model.parameters, 'strategy_simulation_model_parameters');
    const clocks = field === 'fill_model' ? ['random_seed'] : field === 'latency_model'
      ? ['base_latency_ns', 'insert_latency_ns', 'update_latency_ns', 'cancel_latency_ns'] : [];
    if (field === 'fill_model') {
      for (const key of ['prob_fill_on_limit', 'prob_slippage']) {
        const probability = nativeDecimal(parameters[key]);
        if (probability < 0n || probability > one) throw new Error('strategy_simulation_model_parameters_invalid');
      }
    }
    for (const key of clocks) nativeCounter(parameters[key], `strategy_simulation_${key}`);
    if (field === 'latency_model' && BigInt(parameters.base_latency_ns as string) + BigInt(parameters.insert_latency_ns as string) === 0n) throw new Error('strategy_simulation_model_parameters_invalid');
    const fields = field === 'fill_model' ? [...clocks, 'prob_fill_on_limit', 'prob_slippage'] : clocks;
    if (Object.keys(parameters).some(key => !fields.includes(key))) throw new Error('strategy_simulation_model_parameters_invalid');
  }
  const fee = object(settings.fee_model, 'strategy_simulation_model'); const fill = object(settings.fill_model, 'strategy_simulation_model');
  if (fill.adapter_kind === 'POLYMARKET_V2_RECORDED_LIQUIDITY' && fee.adapter_kind !== 'POLYMARKET_V2_RECORDED_FEES'
    || fee.adapter_kind === 'POLYMARKET_V2_RECORDED_FEES' && (settings.account_kind !== 'MARGIN' || leverage !== one || settings.base_currency !== 'pUSD')) throw new Error('strategy_simulation_model_invalid');
  if (!Array.isArray(settings.fee_rates) || !settings.fee_rates.length && mode !== 'catalog-ensemble') throw new Error('strategy_instrument_fee_missing');
  const instruments = new Set<string>();
  for (const value of settings.fee_rates) {
    const rate = object(value, 'strategy_instrument_fee');
    if (Object.keys(rate).some(key => !['instrument_id', 'maker', 'taker'].includes(key))) throw new Error('strategy_instrument_fee_invalid');
    if (typeof rate.instrument_id !== 'string' || !rate.instrument_id.trim() || [...rate.instrument_id].length > 200
      || /[\u0000-\u001f\u007f-\u009f]/.test(rate.instrument_id)) throw new Error('strategy_instrument_invalid');
    if (instruments.has(rate.instrument_id)) throw new Error('strategy_instrument_fee_duplicate');
    instruments.add(rate.instrument_id);
    nativeDecimal(rate.maker); nativeDecimal(rate.taker);
  }
  instrumentFeeRates(settings.fee_rates, instrumentIds);
  return settings;
}

/** Native request rules shared by both registered catalog bindings before model admission. */
export function validateCatalogEnsembleRequest(data: JsonObject): JsonObject {
  if (data.catalog_ensemble_request === undefined) throw new Error('catalog_ensemble_request_required');
  const request = normalizedRequest(data);
  if (Object.keys(request).some(key => !Object.hasOwn(nativeSchemas.NativeCatalogEnsembleRequestV1.properties, key))
    || request.schema_version !== 1 || request.source !== 'TimeSeventeen/Polymarket-v2'
    || typeof request.revision !== 'string' || !request.revision.trim()) throw new Error('catalog_ensemble_request_invalid');
  const start = nativeCounter(request.event_start_ns, 'catalog_ensemble_start');
  const evaluation = nativeCounter(request.evaluation_start_ns, 'catalog_ensemble_evaluation');
  const end = nativeCounter(request.event_end_ns, 'catalog_ensemble_end');
  const cutoff = nativeCounter(request.decision_cutoff_ns, 'catalog_ensemble_cutoff');
  const lag = nativeCounter(request.modeled_lag_ns, 'catalog_ensemble_lag');
  const ttl = nativeCounter(request.target_ttl_ns, 'catalog_ensemble_ttl');
  const train = nativeCounter(data.train_end_ns, 'catalog_ensemble_train'); const validation = nativeCounter(data.validation_end_ns, 'catalog_ensemble_validation');
  if (!(start < evaluation && evaluation < end && end <= cutoff) || evaluation + lag > cutoff
    || train + lag > validation || end > validation && validation + lag > cutoff
    || cutoff + ttl > maxDbCounter) throw new Error('catalog_ensemble_clock_invalid');
  if (request.score_max_age_ns !== undefined && nativeCounter(request.score_max_age_ns, 'catalog_ensemble_score_age') === 0n) throw new Error('catalog_ensemble_score_age_invalid');
  if (typeof request.score_threshold !== 'number' || !Number.isFinite(request.score_threshold)) throw new Error('catalog_ensemble_threshold_invalid');
  if (!Array.isArray(request.members) || request.members.length < 2) throw new Error('catalog_ensemble_members_required');
  let weight = 0;
  for (const value of request.members) {
    const member = object(value, 'catalog_ensemble_member');
    if (Object.keys(member).some(key => !Object.hasOwn(nativeSchemas.NativeCatalogEnsembleMemberV1.properties, key))) throw new Error('catalog_ensemble_member_invalid');
    if (nativeDecimal(member.weight) <= 0n) throw new Error('catalog_ensemble_member_weight_invalid');
    weight += Number(member.weight);
    if (member.total_fuel !== undefined && nativeCounter(member.total_fuel, 'catalog_ensemble_fuel') === 0n) throw new Error('catalog_ensemble_fuel_invalid');
  }
  if (!Number.isFinite(weight) || weight <= 0) throw new Error('catalog_ensemble_member_weight_invalid');
  const gross = nativeDecimal(request.gross_exposure);
  if (gross <= 0n || gross > 10n ** 18n || ttl === 0n) throw new Error('catalog_ensemble_exposure_invalid');
  const settings = validateNativeSimulationSettings(request.settings, [], 'catalog-ensemble');
  if (object(settings.fee_model, 'fee_model').adapter_kind !== 'POLYMARKET_V2_RECORDED_FEES'
    || object(settings.fill_model, 'fill_model').adapter_kind !== 'POLYMARKET_V2_RECORDED_LIQUIDITY') throw new Error('catalog_ensemble_conditional_account_required');
  const latency = object(object(settings.latency_model, 'latency_model').parameters, 'latency_parameters');
  const insert = nativeCounter(latency.base_latency_ns, 'base_latency') + nativeCounter(latency.insert_latency_ns, 'insert_latency');
  if (ttl <= 2n * insert) throw new Error('catalog_ensemble_target_expires_before_insert');
  return request;
}

function nativeCounter(value: unknown, name: string): bigint {
  const text = counter(value, name);
  if (!/^(0|[1-9][0-9]*)$/.test(text) || BigInt(text) > maxDbCounter) throw new Error('strategy_simulation_counter_invalid');
  return BigInt(text);
}

function nativeDecimal(value: unknown): bigint {
  if (typeof value !== 'string' || value.length > 64 || !/^[+-]?(?:0*[0-9]{1,20}(?:\.[0-9]{0,18}0*)?|\.[0-9]{1,18}0*)$(?![\s\S])/.test(value)) throw new Error('strategy_simulation_decimal_required');
  const [whole, fraction = ''] = value.replace(/^[+-]/, '').split('.');
  const units = BigInt(whole || '0') * 10n ** 18n + BigInt(fraction.padEnd(18, '0').slice(0, 18));
  return value.startsWith('-') ? -units : units;
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

export function resolvePortfolioPolicy(proposal: JsonObject, original: JsonObject): StrategyPolicy {
  if (original.kind !== 'catalog-ensemble') return resolveStrategyPolicy(proposal, original);
  if (proposal.instrumentId !== undefined) throw new Error('strategy_market_single_choice_invalid');
  if (proposal.allocations !== undefined) throw new Error('strategy_market_policy_required');
  if (original.evaluationPartition !== undefined && original.evaluationPartition !== 'HELDOUT') throw new Error('strategy_evaluation_partition_invalid');
  if (original.evidenceMode !== undefined && original.evidenceMode !== 'HISTORICAL_REASSESSMENT') throw new Error('strategy_evidence_mode_invalid');
  return original as unknown as CatalogEnsemblePolicy;
}

function selectedSettings(original: JsonObject, ids: string[]): JsonObject {
  const settings = JSON.parse(JSON.stringify(original)) as JsonObject;
  const rates = instrumentFeeRates(settings.fee_rates, ids);
  settings.fee_rates = rates.filter((rate) => ids.includes(String(object(rate, 'fee_rate').instrument_id)));
  return settings;
}

function instrumentFeeRates(value: unknown, ids: readonly string[]): JsonObject[string][] {
  if (!Array.isArray(value)) throw new Error('strategy_instrument_fee_missing');
  const rates = value as JsonObject[string][];
  for (const id of ids) {
    const matched = rates.filter((rate) => rate !== null && typeof rate === 'object' && !Array.isArray(rate) && rate.instrument_id === id);
    if (!matched.length) throw new Error('strategy_instrument_fee_missing');
    if (matched.length !== 1) throw new Error('strategy_instrument_fee_duplicate');
  }
  return rates;
}

function marketAllocations(value: unknown, universe: string[]): MarketAllocation[] {
  if (value === 'ALL_EQUAL') {
    const one = 10n ** 18n; const share = one / BigInt(universe.length);
    value = universe.map((instrumentId, index) => {
      const amount = index === universe.length - 1 ? one - share * BigInt(index) : share;
      return { instrumentId, weight: `0.${amount.toString().padStart(18, '0')}` };
    });
  }
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

function thresholdParameters(policy: ThresholdPolicy): { units: bigint; ttl: bigint } {
  if (policy.kind !== 'factor-threshold' && policy.kind !== 'cross-market-threshold') throw new Error('strategy_adapter_unsupported');
  if (policy.kind === 'factor-threshold' && (typeof policy.instrumentId !== 'string' || !policy.instrumentId.trim())) throw new Error('strategy_instrument_invalid');
  if (!Number.isFinite(policy.threshold)) throw new Error('strategy_threshold_invalid');
  const weight = policy.investedWeight;
  if (typeof weight !== 'string' || !/^\d+(?:\.\d+)?$/.test(weight)) throw new Error('policy_weight_decimal_required');
  const [whole, fraction = ''] = weight.split('.');
  if (fraction.length > 18) throw new Error('decimal_precision_unsupported');
  const units = BigInt(whole + fraction.padEnd(18, '0'));
  if (units <= 0n || units > 10n ** 18n) throw new Error('strategy_weight_invalid');
  const ttl = BigInt(counter(policy.targetTtlNs, 'target_ttl')); if (ttl <= 0n || ttl > maxDbCounter) throw new Error('target_ttl_invalid');
  const settings = object(policy.simulationSettings, 'strategy_simulation_settings');
  if (typeof settings.base_currency !== 'string' || !settings.base_currency.trim()) throw new Error('strategy_currency_required');
  return { units, ttl };
}

// Extracted from the original portfolio consumer. Keep clocks and missing decisions intact.
export function thresholdTargets(forecasts: JsonObject, policy: ThresholdPolicy, currency: string, selection?: JsonObject): JsonObject[] {
  const { units, ttl } = thresholdParameters(policy);
  const weight = policy.investedWeight;
  if (policy.simulationSettings.base_currency !== currency) throw new Error('strategy_currency_mismatch');
  if (!Array.isArray(forecasts.points)) throw new Error('native_forecast_points_missing');
  if (policy.kind === 'cross-market-threshold') {
    let rows = forecasts.points;
    if (selection) {
      const start = BigInt(counter(selection.event_start_ns, 'simulation_start')); const end = BigInt(counter(selection.event_end_ns, 'simulation_end'));
      const cutoff = BigInt(counter(selection.decision_cutoff_ns, 'simulation_cutoff'));
      // Partition source observations, not their later decision clocks. Preserve
      // the full original forecasts and causal warmup in native evidence.
      rows = rows.filter((entry) => {
        const point = object(entry, 'forecast_point'); const event = BigInt(counter(point.event_ns, 'event'));
        return start <= event && event < end && BigInt(counter(point.available_ns, 'available')) <= cutoff;
      });
    }
    return crossMarketTargets(rows, policy, currency, units, ttl);
  }
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
    const bars = ids.flatMap((id) => (selection.bar_types as string[]).filter((bar) => bar.split('-').slice(0, -4).join('-') === id));
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
  const ensemble = data.catalog_ensemble_request !== undefined;
  const original = ensemble ? object(request, 'native_request') : normalizedRequest({ request });
  for (const binding of sealed ? [sealed, data] : [data]) {
    const expected = normalizedRequest(binding);
    if (ensemble) expected.evaluation_start_ns = binding.validation_end_ns!;
    if (canonical(expected) === canonical(original)) {
      if (typeof binding.catalog !== 'string') throw new Error('catalog_required');
      return binding.catalog;
    }
  }
  throw new Error('strategy_request_binding_mismatch');
}
