import type * as Pm from './pm.ts';
import { compareDecimal, pmCanonical, PmValidationError, validateCapacityEstimate, validateMetricObservation, validateOperatingComparison, validatePmEntity } from './pm-validation.ts';

export const PM_METRIC_KEYS = {
  pearsonIc: 'factor.pearson_ic', dailyNetReturn: 'account.daily_geometric_net_return', worstDailyReturn: 'account.worst_daily_net_return',
  maximumDrawdown: 'account.maximum_drawdown', maximumDailyDeviation: 'account.daily_maximum_deviation', negativeDayFraction: 'account.negative_day_fraction',
  negativeDays: 'account.negative_days', zeroDays: 'account.zero_days', expectedDays: 'data.expected_days', observedDays: 'data.observed_days',
  coverage: 'data.complete_day_coverage', sharpe: 'account.sharpe', commission: 'cost.recorded_commission', grossFillNotional: 'execution.gross_fill_notional', fillCount: 'execution.fill_count',
} as const;

export interface NativeValidationProjectionContext {
  role: 'SELECTION' | 'INNER_VALIDATION'; interval: Pm.Interval; asOfNs: Pm.Ns; availableAtNs: Pm.Ns;
  evidenceRefs: Pm.Id[]; methodRef: Pm.Id | null; currency?: Pm.Currency | null; calendarId?: Pm.Id | null;
}
type NativeRow = Record<string, unknown>;
function nativeRecord(value: unknown): NativeRow | undefined { return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as NativeRow : undefined; }
function nativeRows(value: unknown): NativeRow[] { return Array.isArray(value) ? value.flatMap(item => { const row = nativeRecord(item); return row ? [row] : []; }) : []; }
const NATIVE_STATISTICS: Readonly<Record<string, { key: string; unit: Pm.MetricDefinition['unit']; count?: boolean }>> = {
  [PM_METRIC_KEYS.dailyNetReturn]: { key: 'Calendar Geometric Daily Return', unit: 'RETURN_RATIO' },
  [PM_METRIC_KEYS.worstDailyReturn]: { key: 'Calendar Minimum Daily Return', unit: 'RETURN_RATIO' },
  [PM_METRIC_KEYS.maximumDrawdown]: { key: 'Max Drawdown', unit: 'LOSS_RATIO' },
  [PM_METRIC_KEYS.maximumDailyDeviation]: { key: 'Calendar Maximum Absolute Deviation', unit: 'RETURN_RATIO' },
  [PM_METRIC_KEYS.negativeDays]: { key: 'Calendar Negative Days', unit: 'COUNT', count: true },
  [PM_METRIC_KEYS.zeroDays]: { key: 'Calendar Zero Days', unit: 'COUNT', count: true },
  [PM_METRIC_KEYS.expectedDays]: { key: 'Calendar Expected Days', unit: 'COUNT', count: true },
  [PM_METRIC_KEYS.observedDays]: { key: 'Calendar Observed Return Days', unit: 'COUNT', count: true },
  [PM_METRIC_KEYS.coverage]: { key: 'Calendar Coverage Ratio', unit: 'RATIO' },
};
/** Project only explicitly labelled native Validation facts. This does not validate dataset provenance;
 * the caller must bind the original immutable native result and its clocks to the frozen scope. */
export function projectNativeValidationMetrics(result: unknown, definitions: readonly Pm.MetricDefinition[], context: NativeValidationProjectionContext): Pm.MetricObservation[] {
  return projectNativeMetrics(result, definitions, context, 'VALIDATION');
}
export type NativeIndependentProjectionContext = Omit<NativeValidationProjectionContext, 'role'> & { role: 'INDEPENDENT' };
/** Owner-only independent evidence. This entry never changes the input partition label,
 * never returns selection-role metrics and grants no permission to reopen research. */
export function projectNativeIndependentMetrics(result: unknown, definitions: readonly Pm.MetricDefinition[], context: NativeIndependentProjectionContext): Pm.MetricObservation[] {
  return projectNativeMetrics(result, definitions, context, 'HELDOUT');
}
function projectNativeMetrics(result: unknown, definitions: readonly Pm.MetricDefinition[], context: NativeValidationProjectionContext | NativeIndependentProjectionContext, partition: 'VALIDATION' | 'HELDOUT'): Pm.MetricObservation[] {
  definitionsById(definitions); validatePmEntity('Interval', context.interval); clock(context.asOfNs); clock(context.availableAtNs);
  if (partition === 'VALIDATION' ? !['SELECTION', 'INNER_VALIDATION'].includes(context.role) : context.role !== 'INDEPENDENT') reject('pm_native_projection_role_invalid', 'context.role');
  const root = nativeRecord(result); const portfolio = nativeRecord(root?.portfolio);
  const isPortfolio = portfolio !== undefined; const statistics = portfolio?.partition === partition ? nativeRows(portfolio.statistics) : [];
  const partitions = isPortfolio || root?.partitions_component !== undefined ? [] : nativeRows(root?.partitions).filter(p => p.partition === partition);
  const feeTotals = portfolio?.partition === partition ? nativeRecord(portfolio.recorded_fee_totals) : undefined;
  const count = (value: unknown): string | null => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? String(value) : typeof value === 'string' && /^(0|[1-9]\d*)$/.test(value) ? value : null;
  const statistic = (key: string): NativeRow[] => statistics.filter(row => row.native_key === key);
  const available = (row: NativeRow | undefined, explicit = false): boolean => !!row && (row.status === 'AVAILABLE' || !explicit && row.status === undefined) && (row.reason_code === undefined || row.reason_code === null) && typeof row.value === 'number' && Number.isFinite(row.value);
  const singleCount = (key: string): string | null => { const rows = statistic(key); return rows.length === 1 && available(rows[0]) ? count(rows[0]!.value) : null; };
  const observed = singleCount('Calendar Observed Return Days'); const expected = singleCount('Calendar Expected Days');
  const coverage = statistic('Calendar Coverage Ratio');
  return definitions.map(definition => {
    const out: Pm.MetricObservation = { definition, status: 'UNAVAILABLE', value: null, currency: definition.unit === 'CURRENCY' ? context.currency ?? (typeof feeTotals?.currency === 'string' ? feeTotals.currency : null) : null,
      reasonCodes: ['native_metric_not_available'], role: context.role, interval: { ...context.interval }, asOfNs: context.asOfNs, availableAtNs: context.availableAtNs,
      origin: 'DERIVED', methodRef: context.methodRef, evidenceRefs: [...context.evidenceRefs], sampleCount: null, expectedCount: null, uncertainty: null, raw: null };
    const unavailable = (status: Pm.Availability, reason: string): Pm.MetricObservation => {
      const observation = { ...out, status, value: null, reasonCodes: [reason] };
      if (observation.sampleCount !== null && observation.expectedCount !== null && BigInt(observation.sampleCount) > BigInt(observation.expectedCount)) { observation.sampleCount = null; observation.expectedCount = null; }
      return validateMetricObservation(observation, definitions);
    };
    if (context.calendarId !== undefined && definition.calendarId !== null && context.calendarId !== definition.calendarId) return unavailable('INCOMPARABLE', 'native_calendar_mismatch');
    const spec = NATIVE_STATISTICS[definition.key];
    let rows: NativeRow[] = []; let nativeKey: string; let expectedUnit: Pm.MetricDefinition['unit']; let countMetric = false;
    if (definition.key === PM_METRIC_KEYS.pearsonIc) {
      nativeKey = 'pearson_ic'; expectedUnit = 'DIMENSIONLESS';
      if (partitions.length > 1) return unavailable('AMBIGUOUS', 'duplicate_native_partition');
      const partition = partitions[0];
      if (partition && (partition.start_ns !== undefined && partition.start_ns !== context.interval.startNs || partition.end_ns !== undefined && partition.end_ns !== context.interval.endExclusiveNs)) return unavailable('INCOMPARABLE', 'native_partition_interval_mismatch');
      const metric = nativeRecord(partition?.metric); const ic = nativeRecord(metric?.pearson_ic); rows = ic ? [ic] : [];
      const counts = nativeRecord(metric?.counts); out.sampleCount = count(counts?.included); out.expectedCount = count(counts?.observations);
    } else if (definition.key === PM_METRIC_KEYS.negativeDayFraction) {
      if (definition.unit !== 'RATIO' || definition.valueEncoding !== 'FLOAT64' || definition.annualizationDays !== null) return unavailable('INVALID', 'native_metric_definition_incompatible');
      const negativeRows = statistic('Calendar Negative Days');
      if (negativeRows.length > 1 || coverage.length > 1 || statistic('Calendar Observed Return Days').length > 1 || statistic('Calendar Expected Days').length > 1) return unavailable('AMBIGUOUS', 'duplicate_native_metric');
      const negative = singleCount('Calendar Negative Days');
      if (negative === null || expected === null || observed === null || observed !== expected || BigInt(expected) === 0n || BigInt(negative) > BigInt(observed) || coverage.length !== 1 || !available(coverage[0]) || coverage[0]!.value !== 1) return unavailable('INSUFFICIENT', 'complete_calendar_days_required');
      if (portfolio?.returns_status !== undefined && portfolio.returns_status !== null && portfolio.returns_status !== 'OK' && portfolio.returns_status !== 'AVAILABLE') return unavailable('UNAVAILABLE', 'native_account_returns_unavailable');
      out.status = 'AVAILABLE'; out.value = Number(negative) / Number(observed); out.reasonCodes = []; out.sampleCount = observed; out.expectedCount = expected;
      return validateMetricObservation(out, definitions);
    } else if ([PM_METRIC_KEYS.commission, PM_METRIC_KEYS.grossFillNotional, PM_METRIC_KEYS.fillCount].includes(definition.key as typeof PM_METRIC_KEYS.commission)) {
      nativeKey = definition.key === PM_METRIC_KEYS.commission ? 'commission_total' : definition.key === PM_METRIC_KEYS.grossFillNotional ? 'gross_fill_notional' : 'fills';
      const isCount = definition.key === PM_METRIC_KEYS.fillCount;
      if (definition.unit !== (isCount ? 'COUNT' : 'CURRENCY') || definition.valueEncoding !== (isCount ? 'INTEGER_STRING' : 'DECIMAL') || definition.annualizationDays !== null) return unavailable('INVALID', 'native_metric_definition_incompatible');
      const value = feeTotals?.[nativeKey];
      if (typeof feeTotals?.currency !== 'string' || context.currency !== undefined && context.currency !== null && context.currency !== feeTotals.currency) return unavailable('INCOMPARABLE', 'native_fee_currency_missing_or_mismatched');
      if (typeof value !== 'string' || isCount && count(value) === null) return unavailable('UNAVAILABLE', 'native_recorded_fee_totals_missing');
      try { if (compareDecimal(value, '0') < 0) return unavailable('INVALID', 'native_recorded_fee_total_negative'); } catch { return unavailable('INVALID', 'native_recorded_fee_total_invalid'); }
      out.status = 'AVAILABLE'; out.value = value; out.reasonCodes = []; out.raw = { nativeKey, value, unit: isCount ? 'COUNT' : 'CURRENCY', conversionVersion: null };
      return validateMetricObservation(out, definitions);
    } else if (definition.key === PM_METRIC_KEYS.sharpe) {
      nativeKey = `Sharpe Ratio (${definition.annualizationDays} days)`; expectedUnit = 'DIMENSIONLESS'; rows = statistic(nativeKey);
    } else if (spec) { nativeKey = spec.key; expectedUnit = spec.unit; countMetric = spec.count === true; rows = statistic(nativeKey); }
    else return unavailable('UNAVAILABLE', 'native_metric_mapping_not_registered');
    if (definition.unit !== expectedUnit || definition.valueEncoding !== (countMetric ? 'INTEGER_STRING' : 'FLOAT64') || definition.key !== PM_METRIC_KEYS.sharpe && definition.annualizationDays !== null) return unavailable('INVALID', 'native_metric_definition_incompatible');
    if (rows.length > 1) return unavailable('AMBIGUOUS', 'duplicate_native_metric');
    const source = rows[0];
    if (!available(source, definition.key === PM_METRIC_KEYS.pearsonIc)) return unavailable('UNAVAILABLE', typeof source?.reason_code === 'string' ? source.reason_code : typeof source?.status === 'string' ? `native_metric_${source.status.toLowerCase()}` : 'native_metric_not_available');
    // Status-less account statistics are the original native wire. An explicit unavailable
    // report or per-statistic status always takes precedence over a finite numeric payload.
    if (isPortfolio && ![PM_METRIC_KEYS.expectedDays, PM_METRIC_KEYS.observedDays, PM_METRIC_KEYS.coverage].includes(definition.key as typeof PM_METRIC_KEYS.expectedDays) && portfolio.returns_status !== undefined && portfolio.returns_status !== null && portfolio.returns_status !== 'OK' && portfolio.returns_status !== 'AVAILABLE') return unavailable('UNAVAILABLE', 'native_account_returns_unavailable');
    if (source!.group !== undefined && source!.group !== 'RETURNS') return unavailable('INCOMPARABLE', 'native_statistic_group_mismatch');
    if (source!.currency !== undefined && source!.currency !== null) return unavailable('INCOMPARABLE', 'native_dimensionless_metric_currency_mismatch');
    const value = source!.value as number; out.raw = { nativeKey, value, unit: definition.key === PM_METRIC_KEYS.maximumDrawdown ? 'SIGNED_RETURN_RATIO' : expectedUnit, conversionVersion: null };
    if (countMetric) { const encoded = count(value); if (encoded === null) return unavailable('INVALID', 'native_count_invalid'); out.value = encoded; }
    else if (definition.key === PM_METRIC_KEYS.maximumDrawdown) { if (value > 0) return unavailable('INVALID', 'native_drawdown_sign_invalid'); out.value = -value; out.raw.conversionVersion = 'qz.signed-drawdown-to-loss/1'; }
    else out.value = value;
    out.status = 'AVAILABLE'; out.reasonCodes = [];
    if (definition.key !== PM_METRIC_KEYS.pearsonIc && observed !== null && expected !== null && BigInt(observed) <= BigInt(expected)) { out.sampleCount = observed; out.expectedCount = expected; }
    try { return validateMetricObservation(out, definitions); } catch (error) { if (!(error instanceof PmValidationError)) throw error; return unavailable('INVALID', error.code); }
  });
}

export interface DominanceExplanation {
  leftId: Pm.Id; rightId: Pm.Id; scopeId: Pm.Id;
  relation: 'DOMINATES' | 'DOMINATED' | 'EQUAL' | 'TRADEOFF' | 'INCOMPARABLE';
  improvedDefinitionIds: Pm.Id[]; worseDefinitionIds: Pm.Id[]; equalDefinitionIds: Pm.Id[]; unavailableDefinitionIds: Pm.Id[];
  reasonCodes: string[];
}
export interface ResearchComparisonInput {
  policy: Pm.DecisionPolicyRevision;
  scopes: readonly Pm.ComparisonScope[];
  trials: readonly Pm.ResearchTrial[];
  /** Planned selection candidates; training/fit attempts remain visible but cannot become account candidates. */
  candidateTrialIds?: readonly Pm.Id[];
  metricsByTrial: Readonly<Record<Pm.Id, readonly Pm.MetricObservation[]>>;
  scopeByTrial: Readonly<Record<Pm.Id, Pm.Id>>;
  definitions: readonly Pm.MetricDefinition[];
  frontierDefinitionIds: readonly Pm.Id[];
  asOfNs: Pm.Ns;
}
export type ResearchComparisonResult = Pick<Pm.CandidateComparison, 'scopes' | 'candidates' | 'objectiveRankings' | 'recommendedTrialId' | 'recommendationStatus' | 'reasonCodes'> & { pairwise: DominanceExplanation[] };
function reject(code: string, path: string): never { throw new PmValidationError(code, path); }
function idsUnique(ids: readonly string[], path: string): void { if (new Set(ids).size !== ids.length) reject('pm_duplicate_identity', path); }
function definitionsById(definitions: readonly Pm.MetricDefinition[]): Map<Pm.Id, Pm.MetricDefinition> {
  idsUnique(definitions.map(d => d.id), 'definitions');
  return new Map(definitions.map(definition => { const d = validatePmEntity('MetricDefinition', definition); return [d.id, d]; }));
}
function validateObjectives(selection: Pm.SelectionRule, registry: Map<Pm.Id, Pm.MetricDefinition>): void {
  validatePmEntity('SelectionRule', selection);
  for (const objective of selection.objectives) {
    const definition = registry.get(objective.metricDefinitionId);
    if (!definition || definition.key !== objective.metricKey || definition.horizonRef !== objective.horizonRef || definition.direction !== objective.direction) reject('pm_objective_definition_mismatch', objective.metricDefinitionId);
  }
}
function clock(value: string): bigint { if (!/^(0|[1-9]\d*)$/.test(value)) reject('pm_clock_invalid', 'asOfNs'); return BigInt(value); }
function compatible(metric: Pm.MetricObservation, scope: Pm.ComparisonScope, at: bigint): string | null {
  if (metric.role !== scope.role || metric.role === 'INDEPENDENT' || metric.role === 'TRAINING') return 'metric_role_not_permitted';
  if (pmCanonical(metric.interval) !== pmCanonical(scope.interval)) return 'metric_interval_mismatch';
  if (metric.definition.calendarId !== null && metric.definition.calendarId !== scope.calendarId) return 'metric_calendar_mismatch';
  if (clock(metric.availableAtNs) > at || clock(metric.asOfNs) > at) return 'metric_not_available_at_decision';
  if (metric.currency !== null && scope.capital !== null && metric.currency !== scope.capital.currency) return 'metric_currency_mismatch';
  return null;
}
function compareValues(left: Pm.MetricObservation, right: Pm.MetricObservation): number {
  if (typeof left.value === 'number' && typeof right.value === 'number') return left.value < right.value ? -1 : left.value > right.value ? 1 : 0;
  if (typeof left.value === 'string' && typeof right.value === 'string') return compareDecimal(left.value, right.value);
  return reject('pm_metric_encoding_mismatch', left.definition.id);
}
function metricMap(metrics: readonly Pm.MetricObservation[]): Map<Pm.Id, Pm.MetricObservation> { return new Map(metrics.map(m => [m.definition.id, m])); }
function readMetrics(input: readonly Pm.MetricObservation[], scope: Pm.ComparisonScope, definitions: readonly Pm.MetricDefinition[], at: bigint): { metrics: Pm.MetricObservation[]; reasons: string[] } {
  const metrics: Pm.MetricObservation[] = []; const reasons: string[] = [];
  for (const source of input) {
    // Excluded evidence is not copied into research-facing feedback, including raw values.
    if (source.role === 'INDEPENDENT' || source.role === 'TRAINING' || source.role !== scope.role) { reasons.push('metric_role_not_permitted'); continue; }
    try {
      const metric = validateMetricObservation(source, definitions); const mismatch = compatible(metric, scope, at);
      if (mismatch) { reasons.push(mismatch); metrics.push({ ...metric, status: mismatch === 'metric_not_available_at_decision' ? 'STALE' : 'INCOMPARABLE', value: null, raw: null, reasonCodes: [...metric.reasonCodes, mismatch] }); }
      else metrics.push(metric);
    } catch (error) { if (!(error instanceof PmValidationError)) throw error; reasons.push(error.code); }
  }
  const counts = new Map<string, number>(); for (const m of metrics) counts.set(m.definition.id, (counts.get(m.definition.id) ?? 0) + 1);
  for (let i = 0; i < metrics.length; i++) { const m = metrics[i]!; if (counts.get(m.definition.id)! > 1) { reasons.push(`metric_ambiguous:${m.definition.id}`); metrics[i] = { ...m, status: 'AMBIGUOUS', value: null, reasonCodes: [...m.reasonCodes, 'metric_ambiguous'] }; } }
  return { metrics, reasons: [...new Set(reasons)] };
}
function pair(left: Pm.CandidateScorecard, right: Pm.CandidateScorecard, dimensions: readonly Pm.Id[]): DominanceExplanation {
  const result: DominanceExplanation = { leftId: left.trialId, rightId: right.trialId, scopeId: left.comparisonScopeId!, relation: 'INCOMPARABLE', improvedDefinitionIds: [], worseDefinitionIds: [], equalDefinitionIds: [], unavailableDefinitionIds: [], reasonCodes: [] };
  const a = metricMap(left.metrics); const b = metricMap(right.metrics);
  if (left.comparisonScopeId !== right.comparisonScopeId || left.status !== 'AVAILABLE' || right.status !== 'AVAILABLE' || dimensions.length === 0) { result.reasonCodes.push('candidate_not_comparable'); return result; }
  for (const id of dimensions) {
    const x = a.get(id); const y = b.get(id);
    if (!x || !y || x.status !== 'AVAILABLE' || y.status !== 'AVAILABLE' || pmCanonical(x.definition) !== pmCanonical(y.definition) || x.currency !== y.currency) { result.unavailableDefinitionIds.push(id); continue; }
    const comparison = compareValues(x, y) * (x.definition.direction === 'MAXIMIZE' ? 1 : -1);
    (comparison > 0 ? result.improvedDefinitionIds : comparison < 0 ? result.worseDefinitionIds : result.equalDefinitionIds).push(id);
  }
  if (result.unavailableDefinitionIds.length) { result.reasonCodes.push('comparison_dimension_unavailable'); return result; }
  result.relation = result.improvedDefinitionIds.length && result.worseDefinitionIds.length ? 'TRADEOFF' : result.improvedDefinitionIds.length ? 'DOMINATES' : result.worseDefinitionIds.length ? 'DOMINATED' : 'EQUAL';
  result.reasonCodes.push('empirical_point_estimate_not_statistical_significance'); return result;
}
function rank(candidates: readonly Pm.CandidateScorecard[], objective: Pm.Objective): Pm.Id[][] {
  const eligible = candidates.filter(c => {
    const found = c.metrics.filter(m => m.definition.id === objective.metricDefinitionId);
    return c.status === 'AVAILABLE' && found.length === 1 && found[0]!.status === 'AVAILABLE';
  });
  eligible.sort((a, b) => compareValues(metricMap(a.metrics).get(objective.metricDefinitionId)!, metricMap(b.metrics).get(objective.metricDefinitionId)!) * (objective.direction === 'MAXIMIZE' ? -1 : 1));
  const groups: Pm.Id[][] = [];
  for (const candidate of eligible) {
    const previous = groups.at(-1)?.[0]; const prior = eligible.find(row => row.trialId === previous);
    if (prior && compareValues(metricMap(candidate.metrics).get(objective.metricDefinitionId)!, metricMap(prior.metrics).get(objective.metricDefinitionId)!) === 0) groups.at(-1)!.push(candidate.trialId);
    else groups.push([candidate.trialId]);
  }
  return groups;
}
/** Pure selection over original, role-bound facts. No target threshold, composite score, or held-out lookup. */
export function compareResearchCandidates(input: ResearchComparisonInput): ResearchComparisonResult {
  const policy = validatePmEntity('DecisionPolicyRevision', input.policy); const registry = definitionsById(input.definitions); validateObjectives(policy.selectionRule, registry);
  const scopes = input.scopes.map(scope => validatePmEntity('ComparisonScope', scope)); idsUnique(scopes.map(s => s.id), 'scopes'); idsUnique(input.trials.map(t => t.id), 'trials'); idsUnique(input.frontierDefinitionIds, 'frontierDefinitionIds');
  for (const id of input.frontierDefinitionIds) { const def = registry.get(id); if (!def || def.direction === 'DIAGNOSTIC') reject('pm_frontier_definition_invalid', id); }
  const candidateIds = input.candidateTrialIds ?? input.trials.map(t => t.id); idsUnique(candidateIds, 'candidateTrialIds');
  if (candidateIds.some(id => !input.trials.some(t => t.id === id))) reject('pm_candidate_trial_missing', 'candidateTrialIds');
  const eligibleIds = new Set(candidateIds);
  const at = clock(input.asOfNs); const needed = policy.selectionRule.objectives.map(o => o.metricDefinitionId);
  const candidates = input.trials.map(source => {
    const trial = validatePmEntity('ResearchTrial', source); const scope = scopes.find(s => s.id === input.scopeByTrial[trial.id]);
    const result: Pm.CandidateScorecard = { trialId: trial.id, comparisonScopeId: scope?.id ?? null, status: 'INSUFFICIENT', metrics: [], frontierMembership: 'UNDETERMINED', dominatedByTrialIds: [], missingEvidence: [], baselineComparisonRef: null };
    if (!eligibleIds.has(trial.id)) { result.status = 'UNAVAILABLE'; result.missingEvidence.push('not_a_selection_candidate'); return result; }
    if (trial.state !== 'COMPUTED') { result.status = trial.state === 'FAILED' || trial.state === 'REJECTED' ? 'UNAVAILABLE' : 'INSUFFICIENT'; result.missingEvidence.push(`trial_${trial.state.toLowerCase()}`); return result; }
    if (!scope || !['SELECTION', 'INNER_VALIDATION'].includes(scope.role)) { result.status = 'INCOMPARABLE'; result.missingEvidence.push('selection_scope_required'); return result; }
    if (pmCanonical([...trial.datasetRevisionIds].sort()) !== pmCanonical([...scope.datasetRevisionIds].sort())) { result.status = 'INCOMPARABLE'; result.missingEvidence.push('trial_scope_dataset_mismatch'); return result; }
    const protocols = new Set(input.trials.filter(t => eligibleIds.has(t.id) && input.scopeByTrial[t.id] === scope.id).map(t => t.protocolRef));
    if (protocols.size !== 1) { result.status = 'INCOMPARABLE'; result.missingEvidence.push('trial_evaluation_protocol_mismatch'); return result; }
    const read = readMetrics(input.metricsByTrial[trial.id] ?? [], scope, input.definitions, at); result.metrics = read.metrics; result.missingEvidence = read.reasons;
    for (const id of needed) { const found = read.metrics.filter(m => m.definition.id === id); if (found.length !== 1 || found[0]!.status !== 'AVAILABLE') result.missingEvidence.push(`metric_unavailable:${id}`); }
    result.missingEvidence = [...new Set(result.missingEvidence)]; result.status = result.missingEvidence.length === 0 ? 'AVAILABLE' : read.metrics.some(m => m.status === 'AMBIGUOUS') ? 'AMBIGUOUS' : read.metrics.some(m => m.status === 'INCOMPARABLE') ? 'INCOMPARABLE' : 'INSUFFICIENT';
    return result;
  });
  const pairwise: DominanceExplanation[] = [];
  for (let i = 0; i < candidates.length; i++) for (let j = i + 1; j < candidates.length; j++) {
    const a = candidates[i]!; const b = candidates[j]!; if (a.comparisonScopeId !== b.comparisonScopeId || a.comparisonScopeId === null) continue;
    const comparison = pair(a, b, input.frontierDefinitionIds); pairwise.push(comparison);
    if (comparison.relation === 'DOMINATES') b.dominatedByTrialIds.push(a.trialId);
    if (comparison.relation === 'DOMINATED') a.dominatedByTrialIds.push(b.trialId);
  }
  for (const candidate of candidates) if (candidate.status === 'AVAILABLE' && input.frontierDefinitionIds.length && input.frontierDefinitionIds.every(id => candidate.metrics.filter(m => m.definition.id === id && m.status === 'AVAILABLE').length === 1)) candidate.frontierMembership = candidate.dominatedByTrialIds.length ? 'DOMINATED' : 'MEMBER';
  const objectiveRankings = scopes.flatMap(scope => policy.selectionRule.objectives.map(objective => ({ scopeId: scope.id, metricKey: objective.metricKey, metricDefinitionId: objective.metricDefinitionId, ties: rank(candidates.filter(c => c.comparisonScopeId === scope.id), objective) })));
  let recommendedTrialId: Pm.Id | null = null; let recommendationStatus: Pm.CandidateComparison['recommendationStatus'] = 'NEEDS_EVIDENCE'; const reasonCodes = ['empirical_point_estimate_frontier'];
  // A diagnostic frontier must never override the frozen business preference.
  const comparable = candidates.filter(c => c.status === 'AVAILABLE');
  if (comparable.length) {
    recommendationStatus = 'FRONTIER_ONLY';
    if (policy.selectionRule.mode === 'LEXICOGRAPHIC') {
      if (new Set(comparable.map(c => c.comparisonScopeId)).size > 1) reasonCodes.push('multiple_comparison_scopes');
      else if (candidates.some(c => eligibleIds.has(c.trialId) && input.trials.find(t => t.id === c.trialId)!.state === 'COMPUTED' && c.status !== 'AVAILABLE')) { recommendationStatus = 'NEEDS_EVIDENCE'; reasonCodes.push('computed_candidate_evidence_incomplete'); }
      else {
        let remaining = comparable;
        for (const objective of policy.selectionRule.objectives) { const top = rank(remaining, objective)[0] ?? []; remaining = remaining.filter(c => top.includes(c.trialId)); if (remaining.length <= 1) break; }
        if (remaining.length === 1) { recommendedTrialId = remaining[0]!.trialId; recommendationStatus = 'SELECTED'; reasonCodes.push('frozen_lexicographic_preference'); }
        else reasonCodes.push('objective_tie_retained');
      }
    } else if (policy.selectionRule.mode === 'EXPECTED_UTILITY') { recommendationStatus = 'NEEDS_EVIDENCE'; reasonCodes.push('utility_model_evaluation_required'); }
    else reasonCodes.push('frontier_only_preference');
  } else reasonCodes.push('no_comparable_scientific_evidence');
  return { scopes, candidates, objectiveRankings, recommendedTrialId, recommendationStatus, reasonCodes, pairwise };
}

export interface OperatingComparisonInput {
  comparison: Pm.OperatingComparison; selection: Pm.SelectionRule; definitions: readonly Pm.MetricDefinition[];
  asOfNs: Pm.Ns; expectedBasis?: { cycleId: Pm.Id; basisRevision: string };
}
/** Operating configurations retain their own identities; they are never fabricated research trials. */
export function compareOperatingCandidates(input: OperatingComparisonInput): { comparison: Pm.OperatingComparison; pairwise: DominanceExplanation[] } {
  const comparison = validateOperatingComparison(input.comparison, { ...input.expectedBasis, asOfNs: input.asOfNs, definitions: input.definitions });
  const registry = definitionsById(input.definitions); validateObjectives(input.selection, registry); const at = clock(input.asOfNs);
  const cards: Pm.CandidateScorecard[] = comparison.candidates.map(c => {
    const read = readMetrics([...c.netOutcome, ...c.downside], comparison.scope, input.definitions, at);
    const unavailable = input.selection.objectives.filter(o => read.metrics.filter(m => m.definition.id === o.metricDefinitionId && m.status === 'AVAILABLE').length !== 1);
    return { trialId: c.id, comparisonScopeId: comparison.scope.id, status: c.status === 'AVAILABLE' && c.feasibility?.status !== 'UNKNOWN' && read.reasons.length === 0 && unavailable.length === 0 ? 'AVAILABLE' : 'INSUFFICIENT', metrics: read.metrics, frontierMembership: 'UNDETERMINED', dominatedByTrialIds: [], missingEvidence: [...read.reasons, ...unavailable.map(o => `metric_unavailable:${o.metricDefinitionId}`)], baselineComparisonRef: null };
  });
  const dimensions = input.selection.objectives.map(o => o.metricDefinitionId); const pairwise: DominanceExplanation[] = [];
  for (let i = 0; i < cards.length; i++) for (let j = i + 1; j < cards.length; j++) pairwise.push(pair(cards[i]!, cards[j]!, dimensions));
  const provenInfeasible = new Set(comparison.candidates.filter(c => c.feasibility?.status === 'INFEASIBLE').map(c => c.id));
  let remaining = cards.filter(c => c.status === 'AVAILABLE' && !provenInfeasible.has(c.trialId)); comparison.selectedCandidateId = null;
  comparison.conclusion = remaining.length ? 'UNRESOLVED_TRADEOFF' : cards.length > 0 && cards.every(c => provenInfeasible.has(c.trialId)) ? 'NO_FEASIBLE_SOLUTION' : 'NEEDS_EVIDENCE'; comparison.reasonCodes = ['operating_configuration_comparison', 'estimated_and_observed_outcomes_kept_distinct'];
  if (provenInfeasible.size) comparison.reasonCodes.push('proven_infeasible_configurations_excluded_from_preference');
  if (input.selection.mode === 'EXPECTED_UTILITY' && comparison.conclusion !== 'NO_FEASIBLE_SOLUTION') { comparison.conclusion = 'NEEDS_EVIDENCE'; comparison.reasonCodes.push('utility_model_evaluation_required'); }
  else if (input.selection.mode === 'LEXICOGRAPHIC') {
    if (cards.some(c => c.status !== 'AVAILABLE' && !provenInfeasible.has(c.trialId))) { comparison.conclusion = 'NEEDS_EVIDENCE'; comparison.reasonCodes.push('operating_candidate_evidence_incomplete'); }
    else { for (const objective of input.selection.objectives) { const top = rank(remaining, objective)[0] ?? []; remaining = remaining.filter(c => top.includes(c.trialId)); if (remaining.length <= 1) break; }
      if (remaining.length === 1) { comparison.selectedCandidateId = remaining[0]!.trialId; comparison.conclusion = 'SELECTED'; comparison.reasonCodes.push('frozen_lexicographic_preference'); }
    }
  }
  return { comparison, pairwise };
}

/** A tested upper point is never used to assume that all lower capital is feasible. */
export function assessCapacityCurve(value: Pm.CapacityEstimate, capital: Pm.Money): { status: Pm.Availability; feasible: boolean | null; reasonCodes: string[]; testedCapitalPoints: Pm.Money[] } {
  const estimate = validateCapacityEstimate(value); validatePmEntity('Money', capital);
  if (compareDecimal(capital.amount, '0') < 0) reject('pm_negative_amount', 'capital');
  const testedCapitalPoints = estimate.curve.map(p => p.capital);
  if (capital.currency !== estimate.currency) return { status: 'INCOMPARABLE', feasible: null, reasonCodes: ['capital_currency_mismatch'], testedCapitalPoints };
  if (estimate.status !== 'AVAILABLE') return { status: estimate.status, feasible: null, reasonCodes: ['capacity_evidence_unavailable', ...estimate.missingEvidence], testedCapitalPoints };
  const matched = estimate.curve.filter(p => compareDecimal(p.capital.amount, capital.amount) === 0);
  if (matched.some(p => p.status !== 'AVAILABLE') && matched.some(p => p.status === 'AVAILABLE')) return { status: 'AMBIGUOUS', feasible: null, reasonCodes: ['capital_configuration_results_conflict'], testedCapitalPoints };
  const covered = estimate.feasibleRegions.some(region => compareDecimal(capital.amount, region.lower.amount) >= 0 && compareDecimal(capital.amount, region.upper.amount) <= 0);
  if (covered) return { status: 'AVAILABLE', feasible: true, reasonCodes: ['explicit_feasible_region', 'no_monotonicity_assumption'], testedCapitalPoints };
  return { status: 'INSUFFICIENT', feasible: null, reasonCodes: [matched.length ? 'tested_point_has_no_feasibility_evidence' : 'capital_not_in_validated_region', 'maximum_tested_capital_is_not_capacity'], testedCapitalPoints };
}

export function assessCapabilities(descriptors: readonly Pm.CapabilityDescriptor[], operation: string, availableEvidence: readonly Pm.Id[] = []): { status: Pm.SupportState; capabilityIds: Pm.Id[]; missingEvidence: string[]; reasonCodes: string[] } {
  const validated = descriptors.map(d => validatePmEntity('CapabilityDescriptor', d)); idsUnique(validated.map(d => d.id), 'capabilities');
  const applicable = validated.filter(d => d.supportedOperations.includes(operation));
  if (!applicable.length) return { status: 'UNSUPPORTED', capabilityIds: [], missingEvidence: [], reasonCodes: ['operation_not_supported'] };
  const missing = new Set<string>(); const reasons = new Set<string>(); const usable: string[] = [];
  for (const descriptor of applicable) {
    const contract = descriptor.operationContracts.find(c => c.name === operation)!; const boundary = descriptor.boundaries.find(b => b.operation === operation)!;
    const absentCapabilities = contract.requiredCapabilityIds.filter(id => !validated.some(d => d.id === id));
    const conflict = contract.incompatibleCapabilityIds.filter(id => validated.some(d => d.id === id));
    const absentEvidence = boundary.requiredEvidence.filter(id => !availableEvidence.includes(id));
    [...absentCapabilities, ...absentEvidence].forEach(id => missing.add(id)); boundary.reasonCodes.forEach(r => reasons.add(r));
    if (conflict.length) reasons.add('incompatible_capability');
    if (!conflict.length && !absentCapabilities.length && !absentEvidence.length && boundary.state === 'SUPPORTED') usable.push(descriptor.id);
  }
  return { status: usable.length ? 'SUPPORTED' : 'CONDITIONAL', capabilityIds: usable.length ? usable : applicable.map(d => d.id), missingEvidence: [...missing], reasonCodes: [...reasons] };
}
