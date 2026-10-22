import type { CapitalAssumptions, CriterionView, JsonObject, Metric, ResearchSpecification } from './index.ts';

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('research_specification_required');
  return value as Record<string, unknown>;
}
function fields(value: Record<string, unknown>, names: string[]) {
  if (Object.keys(value).some(key => !names.includes(key))) throw new Error('research_override_unsupported');
}
export function researchSpecification(value: unknown): ResearchSpecification {
  const spec = object(value); fields(spec, ['version', 'name', 'question', 'rationale', 'exclusions', 'evaluation', 'search', 'capital']);
  if (spec.version !== 1) throw new Error('research_specification_version_unsupported');
  for (const key of ['name', 'question', 'rationale']) if (typeof spec[key] !== 'string' || !spec[key].trim()) throw new Error(`research_${key}_required`);
  if ((spec.name as string).length > 120 || (spec.name as string).includes('\n')) throw new Error('research_short_name_required');
  if (spec.exclusions !== undefined && typeof spec.exclusions !== 'string') throw new Error('research_exclusions_invalid');
  const evaluation = object(spec.evaluation); fields(evaluation, ['metric', 'direction', 'target', 'scheme']);
  if (!['pearson_ic', 'Sharpe Ratio (252 days)', 'Sharpe Ratio (365 days)'].includes(String(evaluation.metric)) || evaluation.direction !== 'maximize' || evaluation.scheme !== 'registered-partitions') throw new Error('research_evaluation_unsupported');
  if (evaluation.target !== undefined && (typeof evaluation.target !== 'number' || !Number.isFinite(evaluation.target))) throw new Error('research_target_invalid');
  const search = object(spec.search); fields(search, ['scope', 'maxMembers', 'stop']);
  if (!['single-factor', 'ensemble'].includes(String(search.scope)) || !Number.isSafeInteger(search.maxMembers) || Number(search.maxMembers) < 1 || search.scope === 'single-factor' && search.maxMembers !== 1 || search.stop !== 'budget-or-selection') throw new Error('research_search_unsupported');
  if (search.scope === 'ensemble' && evaluation.metric === 'pearson_ic') throw new Error('ensemble_account_metric_required');
  if (search.scope === 'single-factor' && evaluation.metric !== 'pearson_ic') throw new Error('factor_metric_required');
  if (spec.capital !== undefined) researchCapital(spec.capital);
  return structuredClone(spec) as unknown as ResearchSpecification;
}

export function researchCapital(value: unknown): CapitalAssumptions {
  const capital = object(value); fields(capital, ['scenarios', 'participation', 'maximumAgeSeconds']);
  if (!Array.isArray(capital.scenarios) || !capital.scenarios.length || capital.scenarios.some(item => typeof item !== 'string' || item.length > 64 || !/^\+?(?:0*\d{1,20}(?:\.\d{0,18}0*)?|\.\d{1,18}0*)(?![\s\S])/.test(item) || !/[1-9]/.test(item))) throw new Error('research_capital_scenarios_invalid');
  if (typeof capital.participation !== 'string' || !/^(?:0\.\d{1,18}|1(?:\.0{1,18})?)$/.test(capital.participation) || !/[1-9]/.test(capital.participation)) throw new Error('research_participation_invalid');
  if (!Number.isSafeInteger(capital.maximumAgeSeconds) || Number(capital.maximumAgeSeconds) < 1 || Number(capital.maximumAgeSeconds) > 4294967295) throw new Error('research_liquidity_age_invalid');
  return structuredClone(capital) as unknown as CapitalAssumptions;
}

export function researchInstructions(input: { goal: string; specification?: ResearchSpecification }): string {
  if (!input.specification) return input.goal;
  return `Original structured research mandate (authoritative; do not replace with prose):\n${JSON.stringify(input.specification)}\nUse research_brief for captured data, policy and remaining attempts. Only original registered partitions and supported tools are permitted. Final selection ends search; the attempt budget also stops it. Capital assumptions do not establish profitability or executable depth.`;
}

/** Compare only the named partition's actual metric; component diagnostics are not account evidence. */
export function researchCriterion(spec: ResearchSpecification | undefined, status: string, metrics: Metric[], partition = 'HELDOUT'): CriterionView | undefined {
  if (!spec) return;
  const metric = spec.evaluation.metric;
  const key = metric === 'pearson_ic' ? `${partition}.pearson_ic` : `${partition}.portfolio.${metric}`;
  const candidates = metrics.filter(item => item.component === undefined && item.key === key);
  const selected = candidates.length === 1 ? candidates[0] : undefined;
  // Native factor IC uses AVAILABLE. Older scalar projections and native
  // account statistics omit status; retain those without overriding an explicit
  // unavailable/unknown status. Duplicate or contradictory bindings are not a
  // license to pick whichever value happens to appear first.
  const reason = candidates.length > 1 ? 'metric_ambiguous' : !selected ? 'metric_unavailable'
    : selected.partition !== undefined && selected.partition !== partition ? 'metric_partition_mismatch'
      : selected.status !== undefined && selected.status !== 'AVAILABLE' ? selected.status || 'metric_status_unavailable'
        : typeof selected.value !== 'number' || !Number.isFinite(selected.value) ? 'metric_unavailable' : null;
  const value = reason === null ? selected!.value as number : null;
  const target = spec.evaluation.target ?? null;
  const state = status === 'FAILED' ? 'failed' : status !== 'COMPUTED' || value === null ? 'insufficient' : target === null ? 'computed' : (spec.evaluation.direction === 'maximize' ? value >= target : value <= target) ? 'met' : 'not_met';
  return { status: state, metric, value, target, partition, reason };
}

export function datasetResearchSettings(data: JsonObject, strategy?: JsonObject) {
  const ensemble = data.catalog_ensemble_request !== undefined;
  const catalog = ensemble || data.catalog_scan_request !== undefined;
  const request = (ensemble ? data.catalog_ensemble_request : catalog ? data.catalog_scan_request : data.request) as JsonObject | undefined;
  const forecast = (catalog ? request : request?.forecast) as JsonObject | undefined;
  const parameters = forecast?.parameters as JsonObject | undefined;
  const selection = (catalog ? request : forecast?.selection) as JsonObject | undefined;
  if (catalog && !ensemble) strategy = undefined;
  return { instruments: Array.isArray(strategy?.instrumentIds) ? strategy.instrumentIds as string[] : typeof strategy?.instrumentId === 'string' ? [strategy.instrumentId] : [],
    inputFamily: ensemble ? 'feature-abi-v2: completed_bar_close, previous_observed_bar_return, source_taker_quantity_imbalance, source_taker_cash_imbalance, source_trade_count, source_fee_per_cash, source_within_minute_price_response' : 'price-volume', horizon: typeof parameters?.label_horizon_observations === 'number' ? parameters.label_horizon_observations : null,
    partitions: request ? { trainEndNs: (ensemble ? data : request).train_end_ns ?? null, validationEndNs: (ensemble ? data : request).validation_end_ns ?? null, endNs: selection?.event_end_ns ?? null, historical: strategy?.evidenceMode === 'HISTORICAL_REASSESSMENT' } : null,
    strategy: strategy ? { kind: strategy.kind ?? null, threshold: (ensemble ? request?.score_threshold : strategy.threshold) ?? null, investedWeight: (ensemble ? request?.gross_exposure : strategy.investedWeight) ?? null, targetTtlNs: (ensemble ? request?.target_ttl_ns : strategy.targetTtlNs) ?? null, simulationSettings: (ensemble ? request?.settings : strategy.simulationSettings) ?? null } : null };
}
