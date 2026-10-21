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
  if (!['pearson_ic', 'Sharpe Ratio (252 days)'].includes(String(evaluation.metric)) || evaluation.direction !== 'maximize' || evaluation.scheme !== 'registered-partitions') throw new Error('research_evaluation_unsupported');
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
  const selected = metrics.find(item => (item.partition === partition || item.key.startsWith(`${partition}.`)) && (metric === 'pearson_ic' ? item.component === undefined && item.key === `${partition}.pearson_ic` : item.key === `${partition}.portfolio.${metric}`));
  const value = typeof selected?.value === 'number' && Number.isFinite(selected.value) ? selected.value : null;
  const target = spec.evaluation.target ?? null;
  const state = status === 'FAILED' ? 'failed' : status !== 'COMPUTED' || value === null ? 'insufficient' : target === null ? 'computed' : (spec.evaluation.direction === 'maximize' ? value >= target : value <= target) ? 'met' : 'not_met';
  return { status: state, metric, value, target, partition, reason: value === null ? selected?.status ?? 'metric_unavailable' : null };
}

export function datasetResearchSettings(data: JsonObject, strategy?: JsonObject) {
  const catalog = data.catalog_scan_request !== undefined;
  const request = (catalog ? data.catalog_scan_request : data.request) as JsonObject | undefined;
  const forecast = (catalog ? request : request?.forecast) as JsonObject | undefined;
  const parameters = forecast?.parameters as JsonObject | undefined;
  const selection = (catalog ? request : forecast?.selection) as JsonObject | undefined;
  if (catalog) strategy = undefined;
  return { instruments: Array.isArray(strategy?.instrumentIds) ? strategy.instrumentIds as string[] : typeof strategy?.instrumentId === 'string' ? [strategy.instrumentId] : [],
    inputFamily: 'price-volume', horizon: typeof parameters?.label_horizon_observations === 'number' ? parameters.label_horizon_observations : null,
    partitions: request ? { trainEndNs: request.train_end_ns ?? null, validationEndNs: request.validation_end_ns ?? null, endNs: selection?.event_end_ns ?? null, historical: strategy?.evidenceMode === 'HISTORICAL_REASSESSMENT' } : null,
    strategy: strategy ? { kind: strategy.kind ?? null, threshold: strategy.threshold ?? null, investedWeight: strategy.investedWeight ?? null, targetTtlNs: strategy.targetTtlNs ?? null, simulationSettings: strategy.simulationSettings ?? null } : null };
}
