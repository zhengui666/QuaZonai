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
export type StrategyPolicy = ThresholdPolicy;
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

/** Static bindings from domain::portfolio::simulation_settings/models, before paid work. */
export function validateNativeSimulationSettings(value: unknown, instrumentIds: readonly string[] = []): JsonObject {
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
    NAUTILUS_STATIC_LATENCY: 'nautilus_execution::models::latency::StaticLatencyModel',
  };
  for (const field of ['fee_model', 'fill_model', 'latency_model']) {
    const model = object(settings[field], 'strategy_simulation_model');
    const adapters = field === 'fill_model' ? ['NAUTILUS_DEFAULT_FILL']
      : field === 'latency_model' ? ['NAUTILUS_STATIC_LATENCY'] : ['NAUTILUS_MAKER_TAKER'];
    if (model.schema_version !== 1 || typeof model.adapter_kind !== 'string' || !adapters.includes(model.adapter_kind)
      || model.upstream_class !== classes[model.adapter_kind] || model.upstream_version !== '0.69.0') throw new Error('strategy_simulation_model_invalid');
    const modelFields = ['schema_version', 'adapter_kind', 'upstream_class', 'upstream_version', 'parameters'];
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
  if (!Array.isArray(settings.fee_rates) || !settings.fee_rates.length) throw new Error('strategy_instrument_fee_missing');
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

function nativeCounter(value: unknown, name: string): bigint {
  const text = counter(value, name);
  if (!/^(0|[1-9][0-9]*)$/.test(text) || BigInt(text) > maxDbCounter) throw new Error('strategy_simulation_counter_invalid');
  return BigInt(text);
}

export function nativeDecimal(value: unknown): bigint {
  if (typeof value !== 'string' || value.length > 64 || !/^[+-]?(?:0*[0-9]{1,20}(?:\.[0-9]{0,18}0*)?|\.[0-9]{1,18}0*)$(?![\s\S])/.test(value)) throw new Error('strategy_simulation_decimal_required');
  const [whole, fraction = ''] = value.replace(/^[+-]/, '').split('.');
  const units = BigInt(whole || '0') * 10n ** 18n + BigInt(fraction.padEnd(18, '0').slice(0, 18));
  return value.startsWith('-') ? -units : units;
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

function counter(value: unknown, name: string): string {
  if (typeof value !== 'string' || !/^\d+$/.test(value)) throw new Error(`${name}_counter_required`);
  return value;
}

/** Match the original final request, never substitute the research-visible prefix. */
export function replayCatalog(data: JsonObject, request: JsonObject): string {
  const sealed = data.sealed ? object(data.sealed, 'heldout_data') : null;
  const original = normalizedRequest({ request });
  for (const binding of sealed ? [sealed, data] : [data]) {
    const expected = normalizedRequest(binding);
    if (canonical((expected)) === canonical((original))) {
      if (typeof binding.catalog !== 'string') throw new Error('catalog_required');
      return binding.catalog;
    }
  }
  throw new Error('strategy_request_binding_mismatch');
}
