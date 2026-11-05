import type { JsonObject } from './index.ts';
import { addDecimal, compareDecimal } from './pm-validation.ts';
import { validateNativeTickFactorRequest, type NativeTickFactorRequest } from './tick-input.ts';

export type NativeTickEconomicScenario = {
  execution_model?: 'SYNTHETIC_QUOTE_IOC_V1';
  initial_usdt: string; usdt0_per_usdt: string; spot_fraction: string;
  gross_fraction: string; maximum_asset_weight: string; participation_fraction: string;
  perp_taker_fee_bps: string; spot_taker_fee_bps: string; extra_cost_bps: string;
  initial_margin_fraction: string; maintenance_margin_fraction: string;
  decision_interval_ns: string; base_latency_ns: string; jitter_min_ns: string; jitter_max_ns: string;
  random_seed: string; maximum_mark_age_ns: string;
};
export type NativeTickEconomicRequest = {
  schema_version: 1; research: NativeTickFactorRequest; scenario: NativeTickEconomicScenario;
  fx_instrument_ids: string[]; funding_source: string; context_source?: string; spot_valuation_source?: string;
  small_capital_policy?: { maximum_entry_notional_usdc: string; maximum_holding_ns: string };
};

const decimalFields = ['initial_usdt', 'usdt0_per_usdt', 'spot_fraction', 'gross_fraction', 'maximum_asset_weight',
  'participation_fraction', 'perp_taker_fee_bps', 'spot_taker_fee_bps', 'extra_cost_bps', 'initial_margin_fraction', 'maintenance_margin_fraction'] as const;
const counterFields = ['decision_interval_ns', 'base_latency_ns', 'jitter_min_ns', 'jitter_max_ns', 'random_seed', 'maximum_mark_age_ns'] as const;
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('tick_economic_object_required');
  return value as Record<string, unknown>;
}
function fields(value: Record<string, unknown>, names: readonly string[]) {
  if (Object.keys(value).some(key => !names.includes(key))) throw new Error('tick_economic_unknown_field');
}

/** Exact native wire values; no floating-point conversion or inferred costs/capital. */
export function validateNativeTickEconomicRequest(value: unknown): NativeTickEconomicRequest {
  const request = object(value);
  fields(request, ['schema_version', 'research', 'scenario', 'fx_instrument_ids', 'funding_source', 'context_source', 'spot_valuation_source', 'small_capital_policy']);
  if (request.schema_version !== 1) throw new Error('tick_economic_schema_version_unsupported');
  const research = validateNativeTickFactorRequest(request.research);
  if ([research.input.event_start_ns, research.input.event_end_ns, research.train_end_ns, research.validation_end_ns]
    .some(value => BigInt(value) % 86_400_000_000_000n !== 0n)) throw new Error('tick_economic_utc_daily_boundaries_required');
  const scenario = object(request.scenario); fields(scenario, [...decimalFields, ...counterFields, 'execution_model']);
  if (scenario.execution_model !== undefined && scenario.execution_model !== 'SYNTHETIC_QUOTE_IOC_V1') throw new Error('tick_economic_execution_model_invalid');
  for (const key of decimalFields) {
    const value = scenario[key];
    if (typeof value !== 'string' || value.length > 64 || !/^[+-]?(?:0*\d{1,20}(?:\.\d{0,18}0*)?|\.\d{1,18}0*)(?![\s\S])/.test(value)) throw new Error(`tick_economic_${key}_invalid`);
  }
  for (const key of counterFields) {
    const value = scenario[key];
    if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error(`tick_economic_${key}_invalid`);
  }
  const s = scenario as NativeTickEconomicScenario;
  for (const key of ['initial_usdt', 'usdt0_per_usdt'] as const) if (compareDecimal(s[key], '0') <= 0) throw new Error(`tick_economic_${key}_invalid`);
  for (const key of ['spot_fraction', 'gross_fraction', 'maximum_asset_weight', 'participation_fraction', 'initial_margin_fraction', 'maintenance_margin_fraction'] as const) {
    if (compareDecimal(s[key], '0') <= 0 || compareDecimal(s[key], '1') > 0) throw new Error(`tick_economic_${key}_invalid`);
  }
  if (compareDecimal(s.spot_fraction, '1') >= 0 || compareDecimal(s.maintenance_margin_fraction, s.initial_margin_fraction) >= 0) throw new Error('tick_economic_allocation_or_margin_invalid');
  if (compareDecimal(s.extra_cost_bps, '0') < 0 || (['perp_taker_fee_bps', 'spot_taker_fee_bps'] as const).some(key => {
    const fee = s[key];
    return compareDecimal(fee, '0') < 0 || compareDecimal(addDecimal(fee, s.extra_cost_bps), '10000') >= 0;
  })) throw new Error('tick_economic_costs_invalid');
  if (s.decision_interval_ns === '0' || s.maximum_mark_age_ns === '0' || BigInt(s.jitter_min_ns) > BigInt(s.jitter_max_ns)) throw new Error('tick_economic_clocks_invalid');
  if (!Array.isArray(request.fx_instrument_ids) || !request.fx_instrument_ids.length || request.fx_instrument_ids.some(id => typeof id !== 'string' || !id || id.length > 200 || /[\s\u0000-\u001f\u007f]/.test(id)) || new Set(request.fx_instrument_ids).size !== request.fx_instrument_ids.length) throw new Error('tick_economic_fx_instrument_ids_invalid');
  if (typeof request.funding_source !== 'string' || !request.funding_source.endsWith('.jsonl') || /[/\\\u0000-\u001f\u007f]/.test(request.funding_source)) throw new Error('tick_economic_funding_source_invalid');
  if (request.context_source !== undefined && request.context_source !== 'context-prices.manifest.json') throw new Error('tick_economic_context_source_invalid');
  if (request.spot_valuation_source !== undefined && request.spot_valuation_source !== 'spot-valuation.jsonl') throw new Error('tick_economic_spot_valuation_source_invalid');
  let policy: NativeTickEconomicRequest['small_capital_policy'];
  if (request.small_capital_policy !== undefined) {
    const value = object(request.small_capital_policy);
    fields(value, ['maximum_entry_notional_usdc', 'maximum_holding_ns']);
    const notional = value.maximum_entry_notional_usdc, holding = value.maximum_holding_ns;
    if (typeof notional !== 'string' || notional.length > 64 || !/^[+]?(?:0*\d{1,20}(?:\.\d{0,18}0*)?|\.\d{1,18}0*)(?![\s\S])/.test(notional) || compareDecimal(notional, '0') <= 0) throw new Error('tick_economic_maximum_entry_notional_usdc_invalid');
    if (typeof holding !== 'string' || !/^[1-9]\d*$/.test(holding) || BigInt(holding) > 9223372036854775807n) throw new Error('tick_economic_maximum_holding_ns_invalid');
    policy = { maximum_entry_notional_usdc: addDecimal(notional, '0'), maximum_holding_ns: holding };
  }
  const normalized = { ...s }; for (const key of decimalFields) normalized[key] = addDecimal(s[key], '0');
  return { schema_version: 1, research, scenario: normalized,
    fx_instrument_ids: [...request.fx_instrument_ids] as string[], funding_source: request.funding_source,
    ...(request.context_source === undefined ? {} : { context_source: request.context_source }),
    ...(request.spot_valuation_source === undefined ? {} : { spot_valuation_source: request.spot_valuation_source }),
    ...(policy === undefined ? {} : { small_capital_policy: policy }) };
}


export type NativeTickEconomicResult = {
  schema_version: 1; request: NativeTickEconomicRequest; model: string;
  small_capital_diagnostics?: {
    entry_attempts: string; unfilled_entries: string; positions_opened: string; positions_closed: string; exit_attempts: string;
    maximum_exit_delay_ns: string | null;
    remaining_positions: { instrument_id: string; quantity: string; entry_decision_ns: string; first_fill_ns: string; exit_decision_ns: string | null }[];
  };
  methods: Record<string, string>; coverage: JsonObject;
  assets: { instrument_id: string; native_kind: string; cohorts: string; decisions: string; execution_attempts: string }[];
  days: { day_start_ns: string; equity_usdt: string | null; net_return: number | null; unknown_reason: string | null }[];
  metrics: { partition: 'TRAIN' | 'VALIDATION' | 'HELDOUT'; regime: 'BULL' | 'BEAR' | 'RANGE' | 'UNKNOWN'; days: string; valid_return_days: string; sharpe_365: number | null; conditional_annualized_return_365: number | null }[];
  conversion_ns: string | null; initial_usdc_after_cost: string | null; funding_usdc: string;
  fees_usdc_equivalent: string | null; fees_by_currency: Record<string, string>; initial_conversion_cost_usdc: string; nominal_usd_settled_usdc: string;
  native_fills: string; native_rejections: string;
  native_perp_pnl_rounding_events: string; perp_pnl_rounding_bound_usdc: string; funding_missing_instruments: string[];
  targets: { instrument_id: string; currency: string; weight: string }[]; target_asof_ns: string | null; target_cash_weight: string | null;
  skipped_cash: string; skipped_minimum: string; consumed_fuel: string | null; limitations: string[];
};
export type TickEconomicReplayStart = { requestId: string; datasetId: string; factorId: string; fundingCatalog?: string; request: NativeTickEconomicRequest };
export type TickEconomicReplayView = {
  id: string; requestId: string; datasetId: string; factorId: string; taskId: number; fundingCatalog?: string;
  status: 'QUEUED' | 'RUNNING' | 'COMPUTED' | 'FAILED' | 'STOPPED';
  scientificQualification: 'UNQUALIFIED_EXPLORATORY'; result: NativeTickEconomicResult | null; error: string | null;
};

export function validateNativeTickEconomicResult(value: unknown): NativeTickEconomicResult {
  const result = object(value);
  const names = ['schema_version', 'request', 'model', 'small_capital_diagnostics', 'methods', 'coverage', 'assets', 'days', 'metrics', 'conversion_ns', 'initial_usdc_after_cost',
    'funding_usdc', 'fees_usdc_equivalent', 'fees_by_currency', 'initial_conversion_cost_usdc', 'nominal_usd_settled_usdc', 'native_fills', 'native_rejections', 'native_perp_pnl_rounding_events', 'perp_pnl_rounding_bound_usdc',
    'funding_missing_instruments', 'targets', 'target_asof_ns', 'target_cash_weight', 'skipped_cash', 'skipped_minimum', 'consumed_fuel', 'limitations'];
  fields(result, names);
  if (result.schema_version !== 1 || names.some(key => key !== 'small_capital_diagnostics' && !Object.hasOwn(result, key)) || typeof result.model !== 'string' || !result.model) throw new Error('tick_economic_result_invalid');
  validateNativeTickEconomicRequest(result.request); object(result.coverage);
  if (Object.values(object(result.methods)).some(value => typeof value !== 'string')) throw new Error('tick_economic_result_methods_invalid');
  const counter = (value: unknown) => typeof value === 'string' && /^(0|[1-9]\d*)$/.test(value) && BigInt(value) <= 9223372036854775807n;
  const decimal = (value: unknown) => typeof value === 'string' && value.length <= 80 && /^[+-]?(?:0*\d{1,29}(?:\.\d{0,28})?|\.\d{1,28})(?![\s\S])/.test(value);
  if (result.small_capital_diagnostics !== undefined) {
    const d = object(result.small_capital_diagnostics);
    fields(d, ['entry_attempts', 'unfilled_entries', 'positions_opened', 'positions_closed', 'exit_attempts', 'maximum_exit_delay_ns', 'remaining_positions']);
    if (['entry_attempts', 'unfilled_entries', 'positions_opened', 'positions_closed', 'exit_attempts'].some(key => !counter(d[key]))
      || d.maximum_exit_delay_ns !== null && !counter(d.maximum_exit_delay_ns)
      || !Array.isArray(d.remaining_positions) || d.remaining_positions.some(value => {
        const p = object(value); fields(p, ['instrument_id', 'quantity', 'entry_decision_ns', 'first_fill_ns', 'exit_decision_ns']);
        return typeof p.instrument_id !== 'string' || !p.instrument_id || !decimal(p.quantity) || !counter(p.entry_decision_ns)
          || !counter(p.first_fill_ns) || p.exit_decision_ns !== null && !counter(p.exit_decision_ns);
      })) throw new Error('tick_economic_small_capital_diagnostics_invalid');
  }
  if (!Array.isArray(result.assets) || result.assets.some(value => {
    const row = object(value); fields(row, ['instrument_id', 'native_kind', 'cohorts', 'decisions', 'execution_attempts']);
    return typeof row.instrument_id !== 'string' || !row.instrument_id || typeof row.native_kind !== 'string' || !row.native_kind || !counter(row.cohorts) || !counter(row.decisions) || !counter(row.execution_attempts);
  })) throw new Error('tick_economic_result_assets_invalid');
  if (!Array.isArray(result.days) || result.days.some(value => {
    const row = object(value); fields(row, ['day_start_ns', 'equity_usdt', 'net_return', 'unknown_reason']);
    return !counter(row.day_start_ns) || row.equity_usdt !== null && !decimal(row.equity_usdt)
      || row.net_return !== null && (typeof row.net_return !== 'number' || !Number.isFinite(row.net_return))
      || row.unknown_reason !== null && typeof row.unknown_reason !== 'string';
  })) throw new Error('tick_economic_result_days_invalid');
  if (!Array.isArray(result.metrics) || result.metrics.some(value => {
    const row = object(value); fields(row, ['partition', 'regime', 'days', 'valid_return_days', 'sharpe_365', 'conditional_annualized_return_365']);
    return !['TRAIN', 'VALIDATION', 'HELDOUT'].includes(String(row.partition)) || !['BULL', 'BEAR', 'RANGE', 'UNKNOWN'].includes(String(row.regime))
      || !counter(row.days) || !counter(row.valid_return_days)
      || [row.sharpe_365, row.conditional_annualized_return_365].some(value => value !== null && (typeof value !== 'number' || !Number.isFinite(value)));
  }) || Object.entries(object(result.fees_by_currency)).some(([currency, amount]) => !currency || !decimal(amount))
    || !decimal(result.initial_conversion_cost_usdc) || !decimal(result.nominal_usd_settled_usdc)) throw new Error('tick_economic_result_metrics_or_costs_invalid');
  if (!Array.isArray(result.targets) || result.targets.some(value => {
    const row = object(value); fields(row, ['instrument_id', 'currency', 'weight']);
    return typeof row.instrument_id !== 'string' || !row.instrument_id || typeof row.currency !== 'string' || !row.currency || !decimal(row.weight);
  }) || result.target_asof_ns !== null && !counter(result.target_asof_ns) || result.target_cash_weight !== null && !decimal(result.target_cash_weight)
    || !counter(result.native_perp_pnl_rounding_events) || !decimal(result.perp_pnl_rounding_bound_usdc)
    || !counter(result.skipped_cash) || !counter(result.skipped_minimum) || !Array.isArray(result.funding_missing_instruments)
    || result.funding_missing_instruments.some(value => typeof value !== 'string' || !value)) throw new Error('tick_economic_result_targets_or_coverage_invalid');
  if (result.conversion_ns !== null && !counter(result.conversion_ns) || result.consumed_fuel !== null && !counter(result.consumed_fuel)
    || !counter(result.native_fills) || !counter(result.native_rejections) || !decimal(result.funding_usdc)
    || result.initial_usdc_after_cost !== null && !decimal(result.initial_usdc_after_cost)
    || result.fees_usdc_equivalent !== null && !decimal(result.fees_usdc_equivalent)
    || !Array.isArray(result.limitations) || result.limitations.some(value => typeof value !== 'string')) throw new Error('tick_economic_result_invalid');
  return JSON.parse(JSON.stringify(result)) as NativeTickEconomicResult;
}
