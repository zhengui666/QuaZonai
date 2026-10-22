import type * as Pm from './pm.ts';

export class PmValidationError extends Error {
  readonly code: string;
  readonly path: string;
  constructor(code: string, path: string) { super(`${code}: ${path}`); this.name = 'PmValidationError'; this.code = code; this.path = path; }
}
function fail(code: string, path: string): never { throw new PmValidationError(code, path); }
function requireFact(ok: unknown, code: string, path: string): asserts ok { if (!ok) fail(code, path); }
export function pmCanonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(pmCanonical).join(',')}]`;
  if (value !== null && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${pmCanonical((value as Record<string, unknown>)[key])}`).join(',')}}`;
  return JSON.stringify(value);
}
/** Exact decimal arithmetic for accounting invariants; no binary floating point conversion. */
function decimal(value: string): { coefficient: bigint; scale: number } {
  requireFact(typeof value === 'string' && value.length <= 512 && /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(value), 'pm_decimal_invalid', 'decimal');
  const negative = value.startsWith('-'); const unsigned = value.replace(/^[+-]/, ''); const [whole, fraction = ''] = unsigned.split('.');
  return { coefficient: BigInt((whole || '0') + fraction) * (negative ? -1n : 1n), scale: fraction.length };
}
function format(coefficient: bigint, scale: number): string {
  if (coefficient === 0n) return '0';
  const digits = (coefficient < 0n ? -coefficient : coefficient).toString().padStart(scale + 1, '0');
  const body = scale ? `${digits.slice(0, -scale)}.${digits.slice(-scale)}`.replace(/\.?0+$/, '') : digits;
  return (coefficient < 0n ? '-' : '') + body;
}
export function compareDecimal(left: string, right: string): -1 | 0 | 1 {
  const a = decimal(left); const b = decimal(right); const scale = Math.max(a.scale, b.scale);
  const x = a.coefficient * 10n ** BigInt(scale - a.scale); const y = b.coefficient * 10n ** BigInt(scale - b.scale);
  return x < y ? -1 : x > y ? 1 : 0;
}
export function addDecimal(left: string, right: string): string {
  const a = decimal(left); const b = decimal(right); const scale = Math.max(a.scale, b.scale);
  return format(a.coefficient * 10n ** BigInt(scale - a.scale) + b.coefficient * 10n ** BigInt(scale - b.scale), scale);
}
export function subtractDecimal(left: string, right: string): string { const b = decimal(right); return addDecimal(left, format(-b.coefficient, b.scale)); }
export function multiplyDecimal(left: string, right: string): string { const a = decimal(left); const b = decimal(right); return format(a.coefficient * b.coefficient, a.scale + b.scale); }
function nonnegative(value: string, path: string): void { requireFact(compareDecimal(value, '0') >= 0, 'pm_negative_amount', path); }
function unique(values: readonly unknown[], path: string): void { requireFact(new Set(values).size === values.length, 'pm_duplicate_identity', path); }
function sameCurrency(values: readonly Pm.Money[], path: string): void { requireFact(new Set(values.map(value => value.currency)).size <= 1, 'pm_currency_mismatch', path); }
function sum(values: readonly string[]): string { return values.reduce(addDecimal, '0'); }

type Rule = string | ['literal', unknown] | ['array', Rule] | ['union', ...Rule[]] | ['object', Record<string, Rule>] | ['optional', Rule];
function json(value: unknown, path: string, depth: number): void {
  requireFact(depth <= 64, 'pm_value_too_deep', path);
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return;
  if (typeof value === 'number') { requireFact(Number.isFinite(value), 'pm_nonfinite', path); return; }
  if (Array.isArray(value)) { value.forEach((item, index) => json(item, `${path}[${index}]`, depth + 1)); return; }
  requireFact(typeof value === 'object', 'pm_json_invalid', path);
  for (const [key, item] of Object.entries(value)) json(item, `${path}.${key}`, depth + 1);
}
function check(rule: Rule, value: unknown, path: string, depth = 0): void {
  requireFact(depth <= 64, 'pm_value_too_deep', path);
  if (typeof rule === 'string') {
    if (rule === '@Json') { json(value, path, depth); return; }
    if (rule.startsWith('@')) {
      const name = rule.slice(1); requireFact(schemas[name], 'pm_schema_unknown', path);
      check(schemas[name]!, value, path, depth + 1); semantics(name, value, path); return;
    }
    requireFact(typeof value === rule, 'pm_type_invalid', path);
    if (rule === 'number') requireFact(Number.isFinite(value), 'pm_nonfinite', path);
    return;
  }
  const kind = rule[0];
  if (kind === 'optional') { if (value !== undefined) check(rule[1], value, path, depth + 1); return; }
  if (kind === 'literal') { requireFact(value === rule[1], 'pm_literal_invalid', path); return; }
  if (kind === 'array') { requireFact(Array.isArray(value), 'pm_array_required', path); value.forEach((item, index) => check(rule[1], item, `${path}[${index}]`, depth + 1)); return; }
  if (kind === 'union') {
    for (const variant of rule.slice(1) as Rule[]) { try { check(variant, value, path, depth + 1); return; } catch (error) { if (!(error instanceof PmValidationError)) throw error; } }
    fail('pm_union_invalid', path);
  }
  requireFact(value !== null && typeof value === 'object' && !Array.isArray(value), 'pm_object_required', path);
  const record = value as Record<string, unknown>; const fields = rule[1];
  for (const key of Object.keys(record)) requireFact(Object.hasOwn(fields, key), 'pm_unknown_field', `${path}.${key}`);
  for (const [key, field] of Object.entries(fields)) check(field, record[key], `${path}.${key}`, depth + 1);
}
function semantics(name: string, value: unknown, path: string): void {
  if (name === 'Id' || name === 'Currency') { requireFact(typeof value === 'string' && value.trim().length > 0 && value.length <= 1024, 'pm_identity_invalid', path); return; }
  if (name === 'Ns') { requireFact(typeof value === 'string' && /^(0|[1-9]\d*)$/.test(value) && value.length <= 32, 'pm_clock_invalid', path); return; }
  if (name === 'Decimal') { try { decimal(value as string); } catch { fail('pm_decimal_invalid', path); } return; }
  if (!value || typeof value !== 'object' || Array.isArray(value)) return;
  const x = value as Record<string, unknown>;
  if ('schemaVersion' in x && 'revision' in x) {
    requireFact(typeof x.revision === 'string' && /^(0|[1-9]\d*)$/.test(x.revision), 'pm_revision_invalid', path);
    requireFact(x.supersedesId !== x.id, 'pm_self_supersession', path);
    requireFact(typeof x.producerVersion === 'string' && x.producerVersion.trim(), 'pm_producer_required', path);
  }
  if (name === 'Interval') { const v = value as Pm.Interval; requireFact(BigInt(v.startNs) < BigInt(v.endExclusiveNs), 'pm_interval_invalid', path); }
  if (name === 'RuntimeRef') { const v = value as Pm.RuntimeRef; requireFact(Number.isSafeInteger(v.conversationId) && v.conversationId > 0 && (v.taskId === null || Number.isSafeInteger(v.taskId) && v.taskId > 0), 'pm_runtime_identity_invalid', path); }
  if (name === 'EstimateInterval') { const v = value as Pm.EstimateInterval; requireFact(v.lower <= v.upper && v.coverageProbability > 0 && v.coverageProbability < 1 && (v.effectiveSampleSize === null || v.effectiveSampleSize > 0), 'pm_uncertainty_invalid', path); }
  if (name === 'MetricDefinition') {
    const v = value as Pm.MetricDefinition;
    requireFact(v.key.trim() && v.definitionVersion.trim() && v.label.trim() && v.aggregation.trim(), 'pm_metric_definition_incomplete', path);
    requireFact(v.annualizationDays === null || Number.isSafeInteger(v.annualizationDays) && v.annualizationDays > 0, 'pm_metric_annualization_invalid', path);
    if (v.key === 'account.sharpe') requireFact(v.annualizationDays === 252 || v.annualizationDays === 365, 'pm_sharpe_period_required', path);
    if (v.key === 'account.maximum_drawdown') requireFact(v.unit === 'LOSS_RATIO' && v.direction === 'MINIMIZE', 'pm_drawdown_definition_invalid', path);
  }
  if (name === 'MetricObservation') {
    const v = value as Pm.MetricObservation;
    requireFact(v.status === 'AVAILABLE' ? v.value !== null : v.value === null && v.reasonCodes.length > 0, 'pm_metric_availability_invalid', path);
    if (v.value !== null) {
      if (v.definition.valueEncoding === 'FLOAT64') requireFact(typeof v.value === 'number' && Number.isFinite(v.value), 'pm_metric_encoding_invalid', path);
      else { requireFact(typeof v.value === 'string', 'pm_metric_encoding_invalid', path); if (v.definition.valueEncoding === 'INTEGER_STRING') requireFact(/^(0|[1-9]\d*)$/.test(v.value), 'pm_metric_count_invalid', path); else decimal(v.value); }
      if (v.definition.unit === 'LOSS_RATIO') requireFact(typeof v.value === 'number' ? v.value >= 0 : compareDecimal(v.value, '0') >= 0, 'pm_loss_ratio_negative', path);
      if (['account.negative_day_fraction', 'data.complete_day_coverage'].includes(v.definition.key)) requireFact(typeof v.value === 'number' && v.value >= 0 && v.value <= 1, 'pm_fraction_invalid', path);
    }
    requireFact(v.definition.unit !== 'CURRENCY' || v.currency !== null, 'pm_metric_currency_required', path);
    for (const count of [v.sampleCount, v.expectedCount]) requireFact(count === null || /^(0|[1-9]\d*)$/.test(count), 'pm_sample_count_invalid', path);
    if (v.sampleCount !== null && v.expectedCount !== null) requireFact(BigInt(v.sampleCount) <= BigInt(v.expectedCount), 'pm_sample_coverage_invalid', path);
    if (v.definition.key === 'account.maximum_drawdown' && v.status === 'AVAILABLE' && v.raw?.nativeKey === 'Max Drawdown') requireFact(typeof v.raw.value === 'number' && v.raw.value <= 0 && v.value === -v.raw.value && !!v.raw.conversionVersion, 'pm_drawdown_conversion_invalid', path);
  }
  if (name === 'SelectionRule') { const v = value as Pm.SelectionRule; requireFact(v.objectives.length > 0 && !!v.rationale.trim(), 'pm_selection_incomplete', path); unique(v.objectives.map(item => item.metricDefinitionId), path); requireFact(v.mode === 'EXPECTED_UTILITY' ? v.utilityModelRef !== null : v.utilityModelRef === null, 'pm_utility_binding_invalid', path); }
  if (name === 'OwnerBoundary') validateBoundary(value as Pm.OwnerBoundary, path);
  if (name === 'MandateRevision') { const v = value as Pm.MandateRevision; requireFact(v.ownerGoal.trim() && v.interpretation.trim(), 'pm_goal_required', path); unique(v.ownerBoundaries.map(row => row.id), path); if (v.capitalSource.kind === 'RESEARCH_SCENARIO') { sameCurrency([{ amount: '0', currency: v.baseCurrency }, v.capitalSource.capital], path); nonnegative(v.capitalSource.capital.amount, path); } }
  if (name === 'ComparisonScope') { const v = value as Pm.ComparisonScope; unique(v.datasetRevisionIds, path); requireFact(v.datasetRevisionIds.length > 0, 'pm_dataset_required', path); if (v.capital) nonnegative(v.capital.amount, path); }
  if (name === 'ResearchBudget') { const v = value as Pm.ResearchBudget; requireFact(Number.isSafeInteger(v.maximumTrials) && v.maximumTrials > 0, 'pm_trial_budget_invalid', path); requireFact(v.nativeComputeSeconds === null || v.nativeComputeSeconds >= 0, 'pm_resource_budget_invalid', path); if (v.modelSpend) nonnegative(v.modelSpend.amount, path); requireFact(v.dataBytes === null || /^(0|[1-9]\d*)$/.test(v.dataBytes), 'pm_resource_budget_invalid', path); }
  if (name === 'ResearchTrial') { const v = value as Pm.ResearchTrial; unique(v.parentTrialIds, path); requireFact(!v.parentTrialIds.includes(v.id), 'pm_trial_parent_invalid', path); requireFact(v.state !== 'FAILED' || !!v.errorCode, 'pm_trial_failure_reason_required', path); }
  if (name === 'ResearchWorkAllocation') { const v = value as Pm.ResearchWorkAllocation; unique(v.consideredHypothesisIds, path); unique(v.selectedHypothesisIds, path); requireFact(v.selectedHypothesisIds.every(id => v.consideredHypothesisIds.includes(id)) && (v.action !== 'EXPERIMENT' || v.selectedHypothesisIds.length > 0 && v.nextExperimentPurpose !== null), 'pm_work_allocation_invalid', path); }
  if (name === 'EnsembleRevision') { const v = value as Pm.EnsembleRevision; unique(v.members.map(row => row.factorVersionId), path); requireFact(v.members.length > 0, 'pm_ensemble_empty', path); v.members.forEach(row => nonnegative(row.weight, path)); requireFact(compareDecimal(sum(v.members.map(row => row.weight)), '1') === 0, 'pm_factor_weights_not_normalized', path); }
  if (name === 'AccountBasis') {
    const v = value as Pm.AccountBasis;
    requireFact(BigInt(v.observedAtNs) <= BigInt(v.availableAtNs), 'pm_account_clock_order_invalid', path);
    sameCurrency([{ amount: '0', currency: v.currency }, ...[v.equity, v.reserved].filter((item): item is Pm.Money => item !== null)], path);
    if (v.valuation) {
      sameCurrency([{ amount: '0', currency: v.currency }, v.valuation.reportValue], path); nonnegative(v.valuation.reportValue.amount, path);
      if (v.valuation.timeBasis === 'DECISION_CUTOFF') requireFact(v.valuation.priceEvidenceRef !== undefined && BigInt(v.valuation.asOfNs) >= BigInt(v.observedAtNs)
        && BigInt(v.valuation.availableAtNs) <= BigInt(v.valuation.asOfNs), 'pm_current_valuation_price_basis_required', path);
      else requireFact(v.valuation.asOfNs === v.observedAtNs, 'pm_account_valuation_clock_mismatch', path);
      requireFact(BigInt(v.valuation.availableAtNs) >= BigInt(v.availableAtNs), 'pm_account_valuation_clock_mismatch', path);
    }
  }
  if (name === 'CapitalPlan') capitalPlan(value as Pm.CapitalPlan, path);
  if (name === 'AssetTarget') {
    const v = value as Pm.AssetTarget;
    requireFact((v.currentNotional === null) === (v.netTradeNotional === null), 'pm_current_and_net_trade_availability_mismatch', path);
    sameCurrency([v.targetNotional, ...[v.currentNotional, v.netTradeNotional].filter((m): m is Pm.Money => m !== null)], path);
    if (v.currentNotional !== null && v.netTradeNotional !== null) requireFact(compareDecimal(subtractDecimal(v.targetNotional.amount, v.currentNotional.amount), v.netTradeNotional.amount) === 0, 'pm_net_trade_inconsistent', path);
    if (v.executionEstimate) {
      const estimate = v.executionEstimate;
      sameCurrency([v.targetNotional, estimate.resultingNotional, estimate.netTradeNotional], path);
      if (v.currentNotional !== null) requireFact(compareDecimal(addDecimal(v.currentNotional.amount, estimate.netTradeNotional.amount), estimate.resultingNotional.amount) === 0, 'pm_estimated_trade_not_conserved', path);
      // Signed quantities retain generic short-position representation. The
      // currently supported positive-price native spot path cannot report a
      // negative quantity against a nonnegative marked position or delta.
      requireFact(compareDecimal(estimate.resultingQuantity, '0') === compareDecimal(estimate.resultingNotional.amount, '0')
        && compareDecimal(estimate.netTradeQuantity, '0') === compareDecimal(estimate.netTradeNotional.amount, '0'), 'pm_estimated_quantity_direction_mismatch', path);
    }
  }
  if (name === 'CapacityPoint') { const v = value as Pm.CapacityPoint; nonnegative(v.capital.amount, path); sameCurrency([v.capital, ...[v.entryDemand, v.exitDemand, v.estimatedCost].filter((m): m is Pm.Money => m !== null)], path); requireFact(v.fillRatio === null || v.fillRatio >= 0 && v.fillRatio <= 1, 'pm_fill_ratio_invalid', path); requireFact(v.status === 'AVAILABLE' ? v.replayEvidenceRef !== null : v.reasonCodes.length > 0, 'pm_capacity_point_evidence_required', path); }
  if (name === 'CapacityEstimate') capacityEstimate(value as Pm.CapacityEstimate, path);
  if (name === 'CandidateComparison') candidateComparison(value as Pm.CandidateComparison, path);
  if (name === 'ModeledOperatingOutcome') {
    const v = value as Pm.ModeledOperatingOutcome;
    requireFact(v.variance >= 0 && v.linearTurnoverCost >= 0, 'pm_modeled_risk_or_cost_negative', path);
    requireFact(float64Consistent(v.netExpectedReturn, v.grossExpectedReturn - v.linearTurnoverCost), 'pm_modeled_net_return_inconsistent', path);
  }
  if (name === 'ModeledOperatingComparison') {
    const v = value as Pm.ModeledOperatingComparison;
    nonnegative(v.riskAversion, path); const lambda = Number(v.riskAversion);
    requireFact(Number.isFinite(lambda), 'pm_modeled_parameter_not_finite', path);
    requireFact(BigInt(v.availableAtNs) <= BigInt(v.asOfNs), 'pm_modeled_inputs_not_available_at_cutoff', path);
    requireFact(v.hold.linearTurnoverCost === 0, 'pm_hold_has_active_turnover_cost', path);
    requireFact(v.proposed !== null || typeof v.targetBlockReason === 'string' && v.targetBlockReason.trim().length > 0, 'pm_modeled_proposal_missing_reason', path);
    requireFact(v.targetBlockReason === null || v.targetBlockReason.trim().length > 0, 'pm_modeled_block_reason_empty', path);
    for (const outcome of [v.hold, v.proposed]) if (outcome) requireFact(float64Consistent(outcome.objectiveValue, lambda * outcome.variance + outcome.linearTurnoverCost - outcome.grossExpectedReturn), 'pm_modeled_objective_inconsistent', path);
    requireFact(v.limitations.length > 0, 'pm_modeled_limitations_required', path);
  }
  if (name === 'OperatingComparison') operatingComparison(value as Pm.OperatingComparison, path);
  if (name === 'PortfolioDecision') portfolioDecision(value as Pm.PortfolioDecision, path);
  if (name === 'CapabilityDescriptor') { const v = value as Pm.CapabilityDescriptor; unique(v.supportedOperations, path); unique(v.operationContracts.map(o => o.name), path); unique(v.boundaries.map(o => o.operation), path); requireFact(v.supportedOperations.every(op => v.operationContracts.some(row => row.name === op) && v.boundaries.some(row => row.operation === op && row.state !== 'UNSUPPORTED')), 'pm_operation_contract_missing', path); }
  if (name === 'OperationContract') { const v = value as Pm.OperationContract; requireFact(v.effect !== 'EXTERNAL_EFFECT' || v.replayPolicy === 'RECONCILE_REQUIRED', 'pm_effect_replay_invalid', path); }
  if (name === 'BudgetMovement') { const v = value as Pm.BudgetMovement; requireFact(compareDecimal(v.amount, '0') > 0, 'pm_budget_amount_invalid', path); requireFact(v.resource === 'MODEL_COST' ? v.currency !== null : v.currency === null, 'pm_budget_currency_invalid', path); if (v.resource === 'TRIAL' || v.resource === 'DATA_BYTES') requireFact(/^\d+$/.test(v.amount), 'pm_budget_integer_required', path); }
  if (name === 'ExposureRecord') { const v = value as Pm.ExposureRecord; requireFact(v.allowedUse !== 'FIT' || v.roleAtExposure === 'TRAINING', 'pm_exposure_role_invalid', path); requireFact(v.allowedUse !== 'SELECT' || ['INNER_VALIDATION', 'SELECTION'].includes(v.roleAtExposure), 'pm_exposure_role_invalid', path); requireFact(v.allowedUse !== 'EVALUATE_ONCE' || v.roleAtExposure === 'INDEPENDENT', 'pm_exposure_role_invalid', path); }
  if (name === 'EvaluationProtocol') { const v = value as Pm.EvaluationProtocol; unique(v.folds.map(f => f.id), path); requireFact(v.folds.length > 0, 'pm_folds_required', path); for (const fold of v.folds) requireFact(BigInt(fold.train.endExclusiveNs) <= BigInt(fold.select.startNs) && (fold.independent === null || BigInt(fold.select.endExclusiveNs) <= BigInt(fold.independent.startNs)), 'pm_fold_overlap', path); }
  if (name === 'PmMandateStart') { const v = value as Pm.PmMandateStart; requireFact(v.requestId.trim() && v.goal.trim() && v.datasetId.trim() && Number.isSafeInteger(v.maxTrials) && v.maxTrials > 0, 'pm_mandate_start_invalid', path); }
  if (name === 'PmCommand') requireFact(/^(0|[1-9]\d*)$/.test((value as Pm.PmCommand).expectedRevision), 'pm_revision_invalid', path);
}
function validateBoundary(v: Pm.OwnerBoundary, path: string): void {
  const row = v.value; requireFact(row && typeof row === 'object' && !Array.isArray(row), 'pm_boundary_shape_invalid', path);
  const fields: Record<Pm.OwnerBoundary['kind'], string[]> = { LOSS: ['metricKey', 'metricDefinitionId', 'limit', 'unit', 'horizonRef'], LEVERAGE: ['maximumGrossNavRatio', 'borrowingAllowed'], MARKET_SCOPE: ['universeRef', 'excludedCapabilityIds'], LIQUIDITY_NEED: ['amount', 'currency', 'neededByNs'], CAPITAL_COMMITMENT: ['maximumAmount', 'currency', 'validUntilNs'], DATA_USE: ['sourceIds', 'permittedPurposes', 'validUntilNs'] };
  requireFact(fields[v.kind].every(key => Object.hasOwn(row, key)) && Object.keys(row).every(key => fields[v.kind].includes(key)), 'pm_boundary_shape_invalid', path);
  if (v.kind === 'LEVERAGE') { requireFact(typeof row.maximumGrossNavRatio === 'string', 'pm_boundary_leverage_invalid', path); nonnegative(row.maximumGrossNavRatio as string, path); requireFact(typeof row.borrowingAllowed === 'boolean', 'pm_boundary_borrowing_invalid', path); }
  if (v.kind === 'LOSS') { requireFact(typeof row.metricDefinitionId === 'string' && row.metricDefinitionId.trim() && typeof row.metricKey === 'string' && typeof row.horizonRef === 'string' && typeof row.unit === 'string', 'pm_boundary_metric_invalid', path); requireFact(typeof row.limit === 'number' && Number.isFinite(row.limit) || typeof row.limit === 'string', 'pm_boundary_limit_invalid', path); if (typeof row.limit === 'number') requireFact(row.limit >= 0, 'pm_boundary_limit_invalid', path); else nonnegative(row.limit as string, path); }
}
function capitalPlan(v: Pm.CapitalPlan, path: string): void {
  const amounts = [v.equity, v.reserved, v.ownerLiquidityReserve, v.deployableBudget, v.committedCapital, v.uncommittedCash, v.projectedTradingCash]; sameCurrency(amounts, path); amounts.forEach(m => nonnegative(m.amount, path));
  requireFact(compareDecimal(v.equity.amount, sum([v.reserved.amount, v.ownerLiquidityReserve.amount, v.deployableBudget.amount])) === 0, 'pm_deployable_budget_inconsistent', path);
  requireFact(compareDecimal(v.deployableBudget.amount, addDecimal(v.committedCapital.amount, v.uncommittedCash.amount)) === 0, 'pm_capital_not_conserved', path);
  requireFact(compareDecimal(v.projectedTradingCash.amount, v.committedCapital.amount) <= 0, 'pm_trading_cash_exceeds_capital', path); unique(v.capacityEstimateIds, path);
}
function capacityEstimate(v: Pm.CapacityEstimate, path: string): void {
  const monies = [v.minimumTestedCapital, v.maximumTestedCapital, v.supportedCapitalUpperBound, ...v.curve.map(p => p.capital), ...v.feasibleRegions.flatMap(r => [r.lower, r.upper])].filter((m): m is Pm.Money => m !== null);
  sameCurrency([{ amount: '0', currency: v.currency }, ...monies], path); monies.forEach(m => nonnegative(m.amount, path));
  requireFact(v.validUntilNs === null || BigInt(v.validUntilNs) > BigInt(v.asOfNs), 'pm_capacity_expiry_invalid', path);
  requireFact(v.curve.every(p => p.strategyVersionId === v.strategyVersionId), 'pm_capacity_strategy_mismatch', path);
  if (v.curve.length) { const sorted = v.curve.map(p => p.capital.amount).sort(compareDecimal); requireFact(v.minimumTestedCapital !== null && v.maximumTestedCapital !== null && compareDecimal(v.minimumTestedCapital.amount, sorted[0]!) === 0 && compareDecimal(v.maximumTestedCapital.amount, sorted.at(-1)!) === 0, 'pm_capacity_tested_range_mismatch', path); }
  else requireFact(v.minimumTestedCapital === null && v.maximumTestedCapital === null, 'pm_capacity_tested_range_mismatch', path);
  for (const region of v.feasibleRegions) {
    requireFact(compareDecimal(region.lower.amount, region.upper.amount) <= 0 && v.evidenceRefs.includes(region.evidenceRef), 'pm_capacity_region_invalid', path);
    if (region.kind === 'TESTED_POINT') requireFact(compareDecimal(region.lower.amount, region.upper.amount) === 0 && v.curve.some(point => point.status === 'AVAILABLE' && compareDecimal(point.capital.amount, region.lower.amount) === 0 && point.replayEvidenceRef === region.evidenceRef), 'pm_capacity_point_unproven', path);
    if (region.kind === 'VALIDATED_INTERVAL') requireFact(v.minimumTestedCapital !== null && v.maximumTestedCapital !== null && compareDecimal(region.lower.amount, v.minimumTestedCapital.amount) >= 0 && compareDecimal(region.upper.amount, v.maximumTestedCapital.amount) <= 0 && !v.curve.some(point => point.status !== 'AVAILABLE' && compareDecimal(point.capital.amount, region.lower.amount) >= 0 && compareDecimal(point.capital.amount, region.upper.amount) <= 0), 'pm_capacity_interval_unproven', path);
    if (v.supportedCapitalUpperBound) requireFact(compareDecimal(region.upper.amount, v.supportedCapitalUpperBound.amount) <= 0, 'pm_capacity_bound_contradicted', path);
  }
  if (v.supportedCapitalUpperBound !== null) requireFact(v.status === 'AVAILABLE' && v.evidenceRefs.length > 0 && (v.level !== 'ECONOMIC' || v.impactModelRef !== null && v.exitModelRef !== null && v.missingEvidence.length === 0), 'pm_capacity_upper_bound_unproven', path);
}
function candidateComparison(v: Pm.CandidateComparison, path: string): void {
  unique(v.scopes.map(s => s.id), path); unique(v.candidates.map(c => c.trialId), path);
  for (const candidate of v.candidates) requireFact(candidate.comparisonScopeId === null || v.scopes.some(s => s.id === candidate.comparisonScopeId), 'pm_candidate_scope_missing', path);
  for (const rank of v.objectiveRankings) { unique(rank.ties.flat(), path); requireFact(rank.ties.every(tie => tie.length > 0) && rank.ties.flat().every(id => v.candidates.some(c => c.trialId === id && c.comparisonScopeId === rank.scopeId && c.status === 'AVAILABLE')), 'pm_ranking_identity_invalid', path); }
  requireFact(v.recommendationStatus === 'SELECTED' ? v.recommendedTrialId !== null && v.candidates.some(c => c.trialId === v.recommendedTrialId && c.status === 'AVAILABLE') : v.recommendedTrialId === null, 'pm_recommendation_invalid', path);
}
/** Rounding consistency of native FLOAT64 arithmetic only. Money remains exact. */
function float64Consistent(left: number, right: number): boolean {
  return Number.isFinite(left) && Number.isFinite(right) && Math.abs(left - right) <= 16 * Number.EPSILON * Math.max(1, Math.abs(left), Math.abs(right));
}
function operatingComparison(v: Pm.OperatingComparison, path: string): void {
  if (v.modeledComparison) {
    const model = v.modeledComparison;
    requireFact(v.evaluationContext === 'FORWARD_MODEL_ESTIMATE', 'pm_model_not_historical_or_observed', path);
    requireFact(v.scope.startingStateRef === model.startingStateRef && v.policyRevisionId === model.methodPolicyRef, 'pm_modeled_operating_basis_mismatch', path);
    requireFact(BigInt(v.scope.interval.startNs) <= BigInt(model.asOfNs) && BigInt(model.asOfNs) < BigInt(v.scope.interval.endExclusiveNs), 'pm_modeled_scope_clock_mismatch', path);
    if (v.selectedCandidateId !== null && (model.proposed === null || model.targetBlockReason !== null)) requireFact(v.candidates.some(candidate => candidate.id === v.selectedCandidateId && candidate.kind === 'HOLD_CURRENT'), 'pm_blocked_modeled_target_selected', path);
  }
  unique(v.candidates.map(c => c.id), path);
  for (const c of v.candidates) {
    if (c.feasibility) {
      requireFact(c.feasibility.status === 'UNKNOWN' || c.feasibility.evidenceRefs.length > 0, 'pm_operating_feasibility_evidence_required', path);
      if (c.feasibility.status === 'INFEASIBLE') requireFact(c.status !== 'AVAILABLE' && c.feasibility.reasonCodes.length > 0, 'pm_operating_infeasibility_invalid', path);
    }
    unique([...c.transitionCost, ...c.netOutcome, ...c.downside].map(m => m.definition.id), `${path}.candidates.${c.id}.metrics`);
    requireFact(c.status === 'AVAILABLE' || c.reasonCodes.length > 0, 'pm_operating_candidate_reason_required', path);
    for (const metric of [...c.transitionCost, ...c.netOutcome, ...c.downside]) {
      requireFact(metric.role === v.scope.role && pmCanonical(metric.interval) === pmCanonical(v.scope.interval), 'pm_operating_metric_scope_mismatch', path);
      if (v.evaluationContext === 'FORWARD_MODEL_ESTIMATE') requireFact(metric.origin === 'ESTIMATED', 'pm_modeled_estimates_mixed_with_realized_metrics', path);
      if (v.evaluationContext === 'OBSERVED_FORWARD') requireFact(metric.origin !== 'ESTIMATED', 'pm_estimate_not_observed', path);
    }
  }
  requireFact(v.conclusion === 'SELECTED' ? v.selectedCandidateId !== null && v.candidates.some(c => c.id === v.selectedCandidateId && c.status === 'AVAILABLE' && c.feasibility?.status !== 'INFEASIBLE') : v.selectedCandidateId === null, 'pm_operating_selection_invalid', path);
}
function portfolioDecision(v: Pm.PortfolioDecision, path: string): void {
  requireFact((v.accountBasis === null) !== (v.researchStartingStateRef === null), 'pm_account_basis_invalid', path);
  if (v.targetCommandId !== null) requireFact(v.accountBasis !== null && v.assetTargets.every(a => a.currentNotional !== null && a.netTradeNotional !== null), 'pm_target_delivery_account_state_unknown', path);
  unique(v.assetTargets.map(a => a.instrumentId), path); unique(v.strategyAllocations.map(a => a.strategyVersionId), path);
  if (v.capital === null) requireFact(v.assetTargets.length === 0 && v.strategyAllocations.length === 0 && v.targetCommandId === null, 'pm_unknown_capital_has_targets', path);
  else {
    sameCurrency([v.capital.equity, ...v.strategyAllocations.map(a => a.committedCapital), ...v.assetTargets.flatMap(a => [a.targetNotional, a.currentNotional, a.netTradeNotional].filter((m): m is Pm.Money => m !== null))], path);
    if (v.accountBasis) {
      const equity = v.accountBasis.valuation?.reportValue ?? v.accountBasis.equity;
      requireFact(equity !== null && v.accountBasis.reserved !== null, 'pm_account_money_unknown', path);
      sameCurrency([v.capital.equity, equity, v.accountBasis.reserved], path);
      requireFact(compareDecimal(v.capital.equity.amount, equity.amount) === 0 && compareDecimal(v.capital.reserved.amount, v.accountBasis.reserved.amount) === 0, 'pm_account_capital_mismatch', path);
    }
    requireFact(compareDecimal(sum(v.strategyAllocations.map(a => a.committedCapital.amount)), v.capital.committedCapital.amount) === 0, 'pm_strategy_capital_not_conserved', path);
    for (const allocation of v.strategyAllocations) {
      nonnegative(allocation.committedCapital.amount, path);
      if (compareDecimal(v.capital.committedCapital.amount, '0') === 0) requireFact(allocation.capitalFraction === null, 'pm_zero_capital_fraction_invalid', path);
      else requireFact(allocation.capitalFraction === null || compareDecimal(allocation.capitalFraction, '0') >= 0 && compareDecimal(allocation.capitalFraction, '1') <= 0 && compareDecimal(multiplyDecimal(allocation.capitalFraction, v.capital.committedCapital.amount), allocation.committedCapital.amount) === 0, 'pm_strategy_fraction_inconsistent', path);
    }
  }
}
export function validatePmEntity<K extends Pm.PmEntityName>(kind: K, value: unknown): Pm.PmEntities[K] { check(`@${kind}`, value, kind); return structuredClone(value) as Pm.PmEntities[K]; }
export function validateMetricObservation(value: unknown, definitions?: readonly Pm.MetricDefinition[]): Pm.MetricObservation {
  const metric = validatePmEntity('MetricObservation', value);
  if (definitions) { const matches = definitions.filter(d => d.id === metric.definition.id); requireFact(matches.length === 1 && pmCanonical(matches[0]) === pmCanonical(metric.definition), 'pm_metric_definition_mismatch', 'MetricObservation.definition'); }
  return metric;
}
export function validateCapitalPlan(value: unknown): Pm.CapitalPlan { return validatePmEntity('CapitalPlan', value); }
export function validateCapacityEstimate(value: unknown): Pm.CapacityEstimate { return validatePmEntity('CapacityEstimate', value); }
export function validateOperatingComparison(value: unknown, context?: { cycleId?: Pm.Id; basisRevision?: string; asOfNs?: Pm.Ns; definitions?: readonly Pm.MetricDefinition[] }): Pm.OperatingComparison {
  const comparison = validatePmEntity('OperatingComparison', value);
  if (comparison.modeledComparison && context?.asOfNs !== undefined) requireFact(BigInt(comparison.modeledComparison.availableAtNs) <= BigInt(context.asOfNs) && BigInt(comparison.modeledComparison.asOfNs) <= BigInt(context.asOfNs), 'pm_modeled_evidence_not_available_at_decision', 'OperatingComparison.modeledComparison');
  if (context?.cycleId !== undefined) requireFact(comparison.cycleId === context.cycleId, 'pm_operating_cycle_mismatch', 'OperatingComparison.cycleId');
  if (context?.basisRevision !== undefined) requireFact(comparison.basisRevision === context.basisRevision, 'pm_operating_basis_mismatch', 'OperatingComparison.basisRevision');
  for (const candidate of comparison.candidates) for (const metric of [...candidate.transitionCost, ...candidate.netOutcome, ...candidate.downside]) {
    validateMetricObservation(metric, context?.definitions);
    if (context?.asOfNs !== undefined) requireFact(BigInt(metric.availableAtNs) <= BigInt(context.asOfNs) && BigInt(metric.asOfNs) <= BigInt(context.asOfNs), 'pm_metric_not_available_at_decision', 'OperatingComparison.candidates');
  }
  return comparison;
}

// Structural schema for the public DTOs above; cross-field scientific/accounting rules live in semantics().
const schemas: Record<string, Rule> = {
  "Id": "string",
  "Decimal": "string",
  "Ns": "string",
  "Currency": "string",
  "Direction": ["union",["literal","MAXIMIZE"],["literal","MINIMIZE"],["literal","DIAGNOSTIC"]],
  "ClockPrecision": ["union",["literal","SECOND"],["literal","MILLISECOND"],["literal","MICROSECOND"],["literal","NANOSECOND"],["literal","UNKNOWN"]],
  "EvidenceRole": ["union",["literal","TRAINING"],["literal","INNER_VALIDATION"],["literal","SELECTION"],["literal","INDEPENDENT"],["literal","PREQUENTIAL"],["literal","OBSERVED"],["literal","CONTROL"]],
  "Availability": ["union",["literal","AVAILABLE"],["literal","INSUFFICIENT"],["literal","UNAVAILABLE"],["literal","STALE"],["literal","INCOMPARABLE"],["literal","AMBIGUOUS"],["literal","INVALID"]],
  "FactOrigin": ["union",["literal","SOURCE"],["literal","ACCOUNT"],["literal","OWNER_INTENT"],["literal","ESTIMATED"],["literal","DERIVED"],["literal","METHOD_POLICY"]],
  "Interval": ["object",{"startNs":"@Ns","endExclusiveNs":"@Ns"}],
  "RuntimeRef": ["object",{"conversationId":"number","taskId":["union","number",["literal",null]]}],
  "VersionedFact": ["object",{"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "Money": ["object",{"amount":"@Decimal","currency":"@Currency"}],
  "EvidenceRef": ["object",{"id":"@Id","sourceKind":["union",["literal","NATIVE_RESULT"],["literal","DATASET"],["literal","MODEL"],["literal","ACCOUNT"],["literal","DERIVED"]],"role":"@EvidenceRole","runtime":["union","@RuntimeRef",["literal",null]],"objectKey":"string","contentDigest":["union","string",["literal",null]],"mediaType":"string","sourceRevision":"string","eventInterval":["union","@Interval",["literal",null]],"availableAtNs":"@Ns","clockPrecision":"@ClockPrecision","schemaId":"string","visibility":["union",["literal","RESEARCH_VISIBLE"],["literal","OWNER_ONLY"]]}],
  "EstimateInterval": ["object",{"lower":"number","upper":"number","coverageProbability":"number","methodRef":"@Id","effectiveSampleSize":["union","number",["literal",null]]}],
  "MetricDefinition": ["object",{"id":"@Id","key":"string","definitionVersion":"string","label":"string","unit":["union",["literal","RETURN_RATIO"],["literal","LOSS_RATIO"],["literal","RATIO"],["literal","COUNT"],["literal","SECONDS"],["literal","CURRENCY"],["literal","NATIVE_QUANTITY"],["literal","DIMENSIONLESS"]],"valueEncoding":["union",["literal","FLOAT64"],["literal","DECIMAL"],["literal","INTEGER_STRING"]],"direction":"@Direction","aggregation":"string","calendarId":["union","@Id",["literal",null]],"annualizationDays":["union","number",["literal",null]],"horizonRef":["union","@Id",["literal",null]],"requiredInputs":["array","string"]}],
  "MetricObservation": ["object",{"definition":"@MetricDefinition","status":"@Availability","value":["union","number","@Decimal",["literal",null]],"currency":["union","@Currency",["literal",null]],"reasonCodes":["array","string"],"role":"@EvidenceRole","interval":"@Interval","asOfNs":"@Ns","availableAtNs":"@Ns","origin":"@FactOrigin","methodRef":["union","@Id",["literal",null]],"evidenceRefs":["array","@Id"],"sampleCount":["union","string",["literal",null]],"expectedCount":["union","string",["literal",null]],"uncertainty":["union","@EstimateInterval",["literal",null]],"raw":["union",["object",{"nativeKey":"string","value":["union","number","@Decimal",["literal",null]],"unit":"string","conversionVersion":["union","string",["literal",null]]}],["literal",null]]}],
  "Objective": ["object",{"metricKey":"string","metricDefinitionId":"@Id","direction":["union",["literal","MAXIMIZE"],["literal","MINIMIZE"]],"horizonRef":"@Id","scope":["union",["literal","ACCOUNT"],["literal","FORECAST"],["literal","RESEARCH"]]}],
  "OwnerBoundary": ["object",{"id":"@Id","kind":["union",["literal","LOSS"],["literal","LEVERAGE"],["literal","MARKET_SCOPE"],["literal","LIQUIDITY_NEED"],["literal","CAPITAL_COMMITMENT"],["literal","DATA_USE"]],"value":"@Json","sourceStatement":"string","effectiveFromNs":"@Ns"}],
  "SelectionRule": ["object",{"mode":["union",["literal","LEXICOGRAPHIC"],["literal","EXPECTED_UTILITY"],["literal","FRONTIER_ONLY"]],"objectives":["array","@Objective"],"utilityModelRef":["union","@Id",["literal",null]],"establishedBy":["union",["literal","OWNER_GOAL"],["literal","ACCEPTED_PROFILE"]],"rationale":"string"}],
  "MandateRevision": ["object",{"ownerGoal":"string","interpretation":"string","baseCurrency":"@Currency","capitalSource":["union",["object",{"kind":["literal","ACCOUNT"],"accountBindingId":"@Id"}],["object",{"kind":["literal","RESEARCH_SCENARIO"],"capital":"@Money"}]],"allowedUniverseRef":"@Id","ownerBoundaries":["array","@OwnerBoundary"],"selection":"@SelectionRule","operatingAuthority":["union",["literal","RESEARCH_ONLY"],["literal","SHADOW_TARGETS"],["literal","AUTHORIZED_TARGETS"]],"researchBudgetRef":"@Id","unresolvedIntent":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "DecisionPolicyRevision": ["object",{"mandateRevisionId":"@Id","methodFamily":"string","nativeImplementationVersion":"string","learnedParametersRef":["union","@Id",["literal",null]],"trainingProtocolRef":"@Id","validationEvidenceRefs":["array","@Id"],"applicableCapabilityIds":["array","@Id"],"validDomain":"@Json","numericalToleranceRef":"@Id","selectionRule":"@SelectionRule","limitations":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "StrategyArchetype": ["union",["literal","PREDICTIVE_ALPHA"],["literal","RELATIVE_VALUE"],["literal","EVENT_DRIVEN"],["literal","MICROSTRUCTURE"],["literal","PORTFOLIO_OVERLAY"],["literal","FACTOR_RESEARCH"]],
  "SupportState": ["union",["literal","SUPPORTED"],["literal","CONDITIONAL"],["literal","UNSUPPORTED"]],
  "CapabilityDescriptor": ["object",{"provider":"string","providerVersion":"string","strategyArchetypes":["array","@StrategyArchetype"],"dataKinds":["array","string"],"predictionUnits":["array","string"],"accountingModels":["array","string"],"executionModels":["array","string"],"supportedOperations":["array","string"],"operationContracts":["array","@OperationContract"],"boundaries":["array",["object",{"operation":"string","state":"@SupportState","requiredEvidence":["array","string"],"reasonCodes":["array","string"]}]],"nativeSchemaIds":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "OperationContract": ["object",{"name":"string","inputSchemaId":"string","outputSchemaId":"string","requiredCapabilityIds":["array","@Id"],"effect":["union",["literal","PURE"],["literal","NATIVE_COMPUTE"],["literal","EXTERNAL_EFFECT"]],"replayPolicy":["union",["literal","DETERMINISTIC"],["literal","REQUEST_BOUND"],["literal","RECONCILE_REQUIRED"]],"incompatibleCapabilityIds":["array","@Id"]}],
  "StrategyDefinition": ["object",{"name":"string","archetype":"@StrategyArchetype","economicMechanism":"string","falsificationConditions":["array","string"],"opportunityUniverseRef":"@Id","requiredCapabilities":["array","string"],"intendedHoldingHorizonRef":"@Id","knownFailureModes":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "StrategyVersion": ["object",{"definitionId":"@Id","mandateRevisionId":"@Id","policyRevisionId":"@Id","modelRefs":["array","@Id"],"featureSchemaRef":"@Id","signalContractRef":["union","@Id",["literal",null]],"stateSchemaRef":["union","@Id",["literal",null]],"entryRuleRef":["union","@Id",["literal",null]],"holdingRuleRef":["union","@Id",["literal",null]],"invalidationRuleRef":["union","@Id",["literal",null]],"portfolioRuleRef":["union","@Id",["literal",null]],"executionRuleRef":["union","@Id",["literal",null]],"exitRuleRef":["union","@Id",["literal",null]],"componentStrategyVersionIds":["array","@Id"],"capabilityBindingIds":["array","@Id"],"fittedEvidenceRefs":["array","@Id"],"pipelineAssessmentId":"@Id","schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "PipelineStage": ["union",["literal","IDEA"],["literal","RESEARCH"],["literal","SELECTION"],["literal","INDEPENDENT_EVALUATION"],["literal","SHADOW"],["literal","PAPER"],["literal","TARGET_OPERATION"],["literal","MONITORING"],["literal","RETIRED"]],
  "PipelineAssessment": ["object",{"strategyVersionId":"@Id","stage":"@PipelineStage","outcome":["union",["literal","READY"],["literal","NOT_READY"],["literal","NEEDS_EVIDENCE"],["literal","SUPERSEDED"],["literal","RETIRED"]],"evaluatedAtNs":"@Ns","requirementsVersion":"string","evidenceRefs":["array","@Id"],"reasonCodes":["array","string"],"nextPermittedActions":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "DatasetRevision": ["object",{"datasetId":"@Id","sourceId":"@Id","sourceRevision":"string","parentRevisionId":["union","@Id",["literal",null]],"contentRef":"@Id","eventInterval":"@Interval","availableThroughNs":"@Ns","admittedAtNs":"@Ns","sourceClockPrecision":"@ClockPrecision","universeSnapshotRef":"@Id","instrumentRulesRef":"@Id","schemaId":"string","qualityEvidenceRefs":["array","@Id"],"qualityStatus":"@Availability","missingCapabilities":["array","string"],"revisionKind":["union",["literal","APPEND"],["literal","CORRECTION"],["literal","RETRACTION"]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "LabelDefinition": ["object",{"id":"@Id","targetKind":["union",["literal","FORWARD_RETURN"],["literal","EVENT_OUTCOME"],["literal","EXECUTION_OUTCOME"]],"horizonKind":["union",["literal","OBSERVATION"],["literal","CLOCK_TIME"],["literal","EVENT"]],"horizonValue":"string","unit":"string","returnBasis":["union",["literal","GROSS_PRICE_RETURN"],["literal","NET_ACCOUNT_RETURN"],["literal","NOT_APPLICABLE"]],"priceBasis":["union",["literal","TRADE"],["literal","BID"],["literal","ASK"],["literal","MID"],["literal","SETTLEMENT"],["literal","NOT_APPLICABLE"]],"priceAndCashflowRuleRef":"@Id","availabilityRuleRef":"@Id"}],
  "PredictionRow": ["object",{"instrumentId":"string","eventAtNs":"@Ns","decisionAtNs":"@Ns","featureAvailableAtNs":"@Ns","factorVersionId":"@Id","prediction":["union","number",["literal",null]],"status":"@Availability","labelDefinitionId":"@Id","labelEndNs":["union","@Ns",["literal",null]],"labelAvailableAtNs":["union","@Ns",["literal",null]],"realizedValue":["union","number",["literal",null]],"foldId":"@Id"}],
  "ResearchBudget": ["object",{"maximumTrials":"number","nativeComputeSeconds":["union","number",["literal",null]],"modelSpend":["union","@Money",["literal",null]],"dataBytes":["union","string",["literal",null]],"reservationForClosureRef":"@Id","usageEvidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "CampaignState": ["union",["literal","PLANNED"],["literal","SEARCHING"],["literal","CLOSING"],["literal","EVALUATING_INDEPENDENT"],["literal","CONCLUDED"],["literal","PAUSED"]],
  "ResearchCampaign": ["object",{"mandateRevisionId":"@Id","strategyDefinitionId":"@Id","state":"@CampaignState","runtime":"@RuntimeRef","datasetRevisionIds":["array","@Id"],"evaluationProtocolRef":"@Id","budgetId":"@Id","hypothesisIds":["array","@Id"],"trialIds":["array","@Id"],"latestComparisonId":["union","@Id",["literal",null]],"selectedStrategyVersionId":["union","@Id",["literal",null]],"conclusionReason":["union","string",["literal",null]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "Hypothesis": ["object",{"campaignId":"@Id","economicFamilyId":"@Id","parentHypothesisId":["union","@Id",["literal",null]],"mechanism":"string","prediction":"string","falsification":"string","requiredDataCapabilities":["array","string"],"incrementalQuestion":"string","state":["union",["literal","PROPOSED"],["literal","TESTING"],["literal","SUPPORTED"],["literal","REFUTED"],["literal","INCONCLUSIVE"],["literal","RETIRED"]],"evidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "TrialState": ["union",["literal","ADMITTED"],["literal","RUNNING"],["literal","COMPUTED"],["literal","FAILED"],["literal","REJECTED"]],
  "ResearchTrial": ["object",{"campaignId":"@Id","hypothesisId":"@Id","parentTrialIds":["array","@Id"],"runtime":"@RuntimeRef","state":"@TrialState","purpose":["union",["literal","FEASIBILITY"],["literal","CALIBRATION"],["literal","ENSEMBLE_FIT"],["literal","JOINT_REPLAY"],["literal","CAPITAL_SCALE"],["literal","ABLATION"],["literal","ROBUSTNESS"]],"proposalRef":"@Id","datasetRevisionIds":["array","@Id"],"protocolRef":"@Id","methodVersion":"string","countedAttempt":"boolean","resourceUsageRef":["union","@Id",["literal",null]],"evidenceRefs":["array","@Id"],"errorCode":["union","string",["literal",null]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "ComparisonScope": ["object",{"id":"@Id","allowedUniverseRef":"@Id","datasetRevisionIds":["array","@Id"],"interval":"@Interval","role":"@EvidenceRole","calendarId":"@Id","labelDefinitionId":["union","@Id",["literal",null]],"accountingPolicyRef":["union","@Id",["literal",null]],"costModelRef":["union","@Id",["literal",null]],"startingStateRef":["union","@Id",["literal",null]],"capital":["union","@Money",["literal",null]]}],
  "CandidateScorecard": ["object",{"trialId":"@Id","comparisonScopeId":["union","@Id",["literal",null]],"status":"@Availability","metrics":["array","@MetricObservation"],"frontierMembership":["union",["literal","MEMBER"],["literal","DOMINATED"],["literal","UNDETERMINED"]],"dominatedByTrialIds":["array","@Id"],"missingEvidence":["array","string"],"baselineComparisonRef":["union","@Id",["literal",null]]}],
  "CandidateComparison": ["object",{"campaignId":"@Id","policyRevisionId":"@Id","scopes":["array","@ComparisonScope"],"candidates":["array","@CandidateScorecard"],"objectiveRankings":["array",["object",{"scopeId":"@Id","metricKey":"string","metricDefinitionId":"@Id","ties":["array",["array","@Id"]]}]],"recommendedTrialId":["union","@Id",["literal",null]],"recommendationStatus":["union",["literal","SELECTED"],["literal","FRONTIER_ONLY"],["literal","NO_VIABLE_CANDIDATE"],["literal","NEEDS_EVIDENCE"]],"reasonCodes":["array","string"],"evidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "ModeledOperatingOutcome": ["object",{"grossExpectedReturn":"number","linearTurnoverCost":"number","netExpectedReturn":"number","variance":"number","objectiveValue":"number"}],
  "ModeledOperatingComparison": ["object",{"method":["union",["literal","MATCHED_LINEAR_RETURN_VARIANCE_V1"],["literal","MATCHED_ROUNDED_LINEAR_RETURN_VARIANCE_V1"]],"basis":["literal","FORWARD_MODEL_ESTIMATE"],"forecastUnit":["literal","RETURN_PER_HORIZON"],"methodPolicyRef":"@Id","horizonRef":"@Id","riskAversion":"@Decimal","nativeEvidenceRef":"@Id","startingStateRef":"@Id","asOfNs":"@Ns","availableAtNs":"@Ns","hold":"@ModeledOperatingOutcome","proposed":["union","@ModeledOperatingOutcome",["literal",null]],"targetBlockReason":["union","string",["literal",null]],"limitations":["array","string"]}],
  "OperatingComparison": ["object",{"cycleId":"@Id","basisRevision":"string","policyRevisionId":"@Id","scope":"@ComparisonScope","incumbentDecisionId":["union","@Id",["literal",null]],"evaluationContext":["union",["literal","HISTORICAL_JOINT_REPLAY"],["literal","FORWARD_MODEL_ESTIMATE"],["literal","OBSERVED_FORWARD"]],"candidates":["array",["object",{"id":"@Id","kind":["union",["literal","HOLD_CURRENT"],["literal","CASH"],["literal","REBALANCE"],["literal","REDUCE"]],"configurationEvidenceRef":"@Id","feasibility":["optional",["object",{"status":["union",["literal","FEASIBLE"],["literal","INFEASIBLE"],["literal","UNKNOWN"]],"evidenceRefs":["array","@Id"],"reasonCodes":["array","string"]}]],"status":"@Availability","transitionCost":["array","@MetricObservation"],"netOutcome":["array","@MetricObservation"],"downside":["array","@MetricObservation"],"capacityEstimateIds":["array","@Id"],"reasonCodes":["array","string"]}]],"pairedDifferenceEvidenceRefs":["array","@Id"],"selectedCandidateId":["union","@Id",["literal",null]],"conclusion":["union",["literal","SELECTED"],["literal","UNRESOLVED_TRADEOFF"],["literal","NEEDS_EVIDENCE"],["literal","NO_FEASIBLE_SOLUTION"]],"reasonCodes":["array","string"],"modeledComparison":["optional","@ModeledOperatingComparison"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "FactorVersion": ["object",{"economicFamilyId":"@Id","modelRef":"@Id","featureSchemaRef":"@Id","labelDefinitionId":"@Id","outputUnit":"string","trainingEvidenceRefs":["array","@Id"],"validationEvidenceRefs":["array","@Id"],"supportedCapabilityIds":["array","@Id"],"status":["union",["literal","RESEARCH"],["literal","ELIGIBLE"],["literal","DEGRADED"],["literal","RETIRED"]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "CalibrationModel": ["object",{"factorVersionId":"@Id","inputUnit":"string","outputUnit":"string","labelDefinitionId":"@Id","trainingInterval":"@Interval","trainingCutoffNs":"@Ns","fitProtocolRef":"@Id","modelRef":"@Id","fitEvidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "FactorWeight": ["object",{"factorVersionId":"@Id","calibrationModelId":["union","@Id",["literal",null]],"weight":"@Decimal","economicFamilyId":"@Id","contributionEvidenceRef":["union","@Id",["literal",null]]}],
  "EnsembleRevision": ["object",{"strategyVersionId":"@Id","policyRevisionId":"@Id","members":["array","@FactorWeight"],"predictionUnit":"string","labelDefinitionId":"@Id","fitProtocolRef":"@Id","fitCutoffNs":"@Ns","trainingEvidenceRefs":["array","@Id"],"jointValidationEvidenceRefs":["array","@Id"],"qualification":["union",["literal","CANDIDATE"],["literal","SELECTED"],["literal","INDEPENDENTLY_EVALUATED"],["literal","RETIRED"]],"rejectionReasonCodes":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "CapacityLevel": ["union",["literal","OBSERVED_LIQUIDITY"],["literal","CONDITIONAL_FEASIBILITY"],["literal","ECONOMIC"]],
  "CapacityPoint": ["object",{"capital":"@Money","status":"@Availability","replayEvidenceRef":["union","@Id",["literal",null]],"strategyVersionId":"@Id","portfolioDecisionRef":["union","@Id",["literal",null]],"netPerformance":["array","@MetricObservation"],"entryDemand":["union","@Money",["literal",null]],"exitDemand":["union","@Money",["literal",null]],"estimatedCost":["union","@Money",["literal",null]],"fillRatio":["union","number",["literal",null]],"bindingConstraints":["array","string"],"reasonCodes":["array","string"]}],
  "CapacityEstimate": ["object",{"strategyVersionId":"@Id","ensembleRevisionId":["union","@Id",["literal",null]],"basisRevision":"string","level":"@CapacityLevel","status":"@Availability","currency":"@Currency","observedInterval":"@Interval","asOfNs":"@Ns","validUntilNs":["union","@Ns",["literal",null]],"minimumTestedCapital":["union","@Money",["literal",null]],"maximumTestedCapital":["union","@Money",["literal",null]],"supportedCapitalUpperBound":["union","@Money",["literal",null]],"feasibleRegions":["array",["object",{"lower":"@Money","upper":"@Money","kind":["union",["literal","TESTED_POINT"],["literal","VALIDATED_INTERVAL"]],"configurationEvidenceRef":"@Id","evidenceRef":"@Id"}]],"curve":["array","@CapacityPoint"],"liquidityModelRef":["union","@Id",["literal",null]],"impactModelRef":["union","@Id",["literal",null]],"executionModelRef":"@Id","exitModelRef":["union","@Id",["literal",null]],"evidenceRefs":["array","@Id"],"assumptions":["array","string"],"missingEvidence":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "AccountBasis": ["object",{"accountBindingId":"@Id","accountSourceId":"string","observationId":"string","sequence":"string","observedAtNs":"@Ns","availableAtNs":"@Ns","controlRevision":"string","currency":"@Currency","equity":["union","@Money",["literal",null]],"valuation":["optional",["object",{"origin":["literal","DERIVED"],"timeBasis":["optional",["union",["literal","SNAPSHOT_TIME"],["literal","DECISION_CUTOFF"]]],"priceEvidenceRef":["optional","@Id"],"reportValue":"@Money","valuationEvidenceRef":"@Id","methodRef":"@Id","asOfNs":"@Ns","availableAtNs":"@Ns"}]],"reserved":["union","@Money",["literal",null]],"positionsEvidenceRef":"@Id","outstandingCommandsRef":"@Id"}],
  "CapitalPlan": ["object",{"equity":"@Money","reserved":"@Money","ownerLiquidityReserve":"@Money","deployableBudget":"@Money","committedCapital":"@Money","uncommittedCash":"@Money","projectedTradingCash":"@Money","capacityEstimateIds":["array","@Id"],"sizingEvidenceRef":"@Id"}],
  "AssetTarget": ["object",{"instrumentId":"string","navWeight":"@Decimal","targetQuantity":["union","@Decimal",["literal",null]],"targetNotional":"@Money","currentNotional":["union","@Money",["literal",null]],"netTradeNotional":["union","@Money",["literal",null]],"executionEstimate":["optional",["object",{"resultingQuantity":"@Decimal","resultingNotional":"@Money","netTradeQuantity":"@Decimal","netTradeNotional":"@Money","evidenceRef":"@Id"}]],"quantityRuleRef":"@Id","riskContribution":["union","@MetricObservation",["literal",null]]}],
  "StrategyAllocation": ["object",{"strategyVersionId":"@Id","ensembleRevisionId":["union","@Id",["literal",null]],"committedCapital":"@Money","capitalFraction":["union","@Decimal",["literal",null]],"allocationEvidenceRef":"@Id","riskContributionEvidenceRef":["union","@Id",["literal",null]],"nettingGroupId":"@Id"}],
  "DecisionAction": ["union",["literal","HOLD"],["literal","REBALANCE"],["literal","REDUCE"],["literal","CASH"],["literal","NEEDS_EVIDENCE"],["literal","NO_FEASIBLE_SOLUTION"]],
  "PortfolioDecision": ["object",{"cycleId":"@Id","mandateRevisionId":"@Id","strategyVersionId":"@Id","ensembleRevisionId":["union","@Id",["literal",null]],"strategyAllocations":["array","@StrategyAllocation"],"policyRevisionId":"@Id","basisRevision":"string","incumbentDecisionId":["union","@Id",["literal",null]],"accountBasis":["union","@AccountBasis",["literal",null]],"researchStartingStateRef":["union","@Id",["literal",null]],"action":"@DecisionAction","status":["union",["literal","CANDIDATE"],["literal","CURRENT"],["literal","SUPERSEDED"],["literal","EXPIRED"],["literal","REVOKED"]],"assetTargets":["array","@AssetTarget"],"capital":["union","@CapitalPlan",["literal",null]],"comparisonRef":"@Id","solverEvidenceRef":["union","@Id",["literal",null]],"jointReplayEvidenceRef":["union","@Id",["literal",null]],"reasonCodes":["array","string"],"explanationFacts":"@Json","validUntilNs":["union","@Ns",["literal",null]],"nextInvalidationDependencies":["array","@Id"],"targetCommandId":["union","@Id",["literal",null]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "CycleState": ["union",["literal","PENDING"],["literal","CAPTURING"],["literal","COMPUTING"],["literal","EVALUATING"],["literal","CONCLUDED"],["literal","SUPERSEDED"],["literal","WAITING_EVIDENCE"],["literal","FAILED"]],
  "DecisionCycle": ["object",{"mandateRevisionId":"@Id","runtime":"@RuntimeRef","state":"@CycleState","triggerEventIds":["array","@Id"],"basisRevision":"string","datasetRevisionIds":["array","@Id"],"accountBasis":["union","@AccountBasis",["literal",null]],"policyRevisionId":"@Id","strategyVersionId":"@Id","asOfNs":"@Ns","availableCutoffNs":"@Ns","dependencyRevisionRefs":["array","@Id"],"outputDecisionId":["union","@Id",["literal",null]],"supersededByCycleId":["union","@Id",["literal",null]],"reasonCodes":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "ObservedOutcome": ["object",{"decisionId":"@Id","targetCommandId":["union","@Id",["literal",null]],"accountBasis":"@AccountBasis","interval":"@Interval","availableAtNs":"@Ns","receiptState":["union",["literal","NOT_OFFERED"],["literal","PENDING"],["literal","ACKNOWLEDGED"],["literal","REJECTED"],["literal","UNCERTAIN"]],"reconciliationStatus":["union",["literal","MATCHED"],["literal","PARTIAL"],["literal","UNMATCHED"]],"realizedMetrics":["array","@MetricObservation"],"attributionEvidenceRef":["union","@Id",["literal",null]],"matureLabelEvidenceRef":["union","@Id",["literal",null]],"reasonCodes":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "DecisionTrigger": ["union",["literal","DATA_REVISION"],["literal","SOURCE_RECOVERED"],["literal","LABEL_MATURED"],["literal","ACCOUNT_CHANGED"],["literal","COST_OR_RULE_CHANGED"],["literal","RESEARCH_EVIDENCE"],["literal","EVIDENCE_INVALIDATED"],["literal","MANDATE_CHANGED"],["literal","FRESHNESS_DEADLINE"]],
  "BusinessEvent": ["object",{"id":"@Id","kind":"@DecisionTrigger","sourceRevisionRef":"@Id","availableAtNs":"@Ns","affectedMandateIds":["array","@Id"],"supersedesEventId":["union","@Id",["literal",null]]}],
  "TargetCommand": ["object",{"decisionId":"@Id","idempotencyKey":"string","accountBindingId":"@Id","basisRevision":"string","controlRevision":"string","payloadEvidenceRef":"@Id","validUntilNs":"@Ns","state":["union",["literal","NOT_OFFERED"],["literal","PENDING"],["literal","ACKNOWLEDGED"],["literal","REJECTED"],["literal","UNCERTAIN"]],"externalReceiptRef":["union","@Id",["literal",null]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "EvidenceGap": ["object",{"affectedEntityId":"@Id","requiredCapability":"string","reasonCode":"string","permittedRemedy":["union",["literal","WAIT_LABEL"],["literal","RECOMPUTE"],["literal","REFRESH_SOURCE"],["literal","COLLECT_AUTHORIZED_DATA"],["literal","CLARIFY_INTENT"],["literal","UNSUPPORTED"]],"dataAuthorizationRef":["union","@Id",["literal",null]],"state":["union",["literal","OPEN"],["literal","IN_PROGRESS"],["literal","RESOLVED"],["literal","UNRESOLVABLE"]],"resolutionEvidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "EvaluationProtocol": ["object",{"campaignId":"@Id","searchFamilyId":"@Id","labelDefinitionId":"@Id","folds":["array",["object",{"id":"@Id","train":"@Interval","select":"@Interval","independent":["union","@Interval",["literal",null]],"purgeRuleRef":"@Id","embargoRuleRef":["union","@Id",["literal",null]]}]],"comparisonScopeIds":["array","@Id"],"methodSelectionRuleRef":"@Id","statisticalDiagnostics":["array","string"],"requiredCapabilities":["array","string"],"stoppingRuleRef":"@Id","intendedUse":["union",["literal","FACTOR_EVIDENCE"],["literal","STRATEGY_SELECTION"],["literal","ADAPTIVE_POLICY_EVALUATION"]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "SearchFamily": ["object",{"economicFamilyId":"@Id","campaignIds":["array","@Id"],"trialIds":["array","@Id"],"methodVariantRefs":["array","@Id"],"countedAttempts":"string","failedAttempts":"string","costEvidenceRefs":["array","@Id"],"selectionExposureRefs":["array","@Id"],"effectiveTrialEstimate":["union","@MetricObservation",["literal",null]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "ExposureRecord": ["object",{"evidenceRef":"@Id","roleAtExposure":"@EvidenceRole","campaignId":["union","@Id",["literal",null]],"policyRevisionId":["union","@Id",["literal",null]],"exposedAtNs":"@Ns","consumer":["union",["literal","RESEARCH_AGENT"],["literal","SELECTION_METHOD"],["literal","INDEPENDENT_EVALUATOR"],["literal","OWNER"]],"allowedUse":["union",["literal","FIT"],["literal","SELECT"],["literal","EVALUATE_ONCE"],["literal","OBSERVE"]],"resultingEvidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "AdaptiveUpdateProtocol": ["object",{"strategyVersionId":"@Id","eligibleTriggers":["array","@DecisionTrigger"],"maturityRuleRef":"@Id","updateMethodRef":"@Id","trainingWindowRuleRef":"@Id","modelSelectionProtocolRef":"@Id","riskAndCostRefreshRuleRef":"@Id","evidenceFreshnessRuleRef":"@Id","resourceBudgetRef":"@Id","wholeProcessValidationRefs":["array","@Id"],"allowedParameterDomain":"@Json","outOfDomainAction":["union",["literal","KEEP_VALID_INCUMBENT"],["literal","REDUCE_WITH_EVIDENCE"],["literal","WAIT_EVIDENCE"]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "RiskModel": ["object",{"method":["union",["literal","SAMPLE_COVARIANCE"],["literal","SCENARIO_CVAR"],["literal","VALIDATED_EXTENSION"]],"labelDefinitionId":"@Id","assetUniverseRef":"@Id","trainingInterval":"@Interval","cutoffNs":"@Ns","matrixOrScenarioRef":"@Id","sampleCoverageRef":"@Id","numericalDiagnosticsRef":"@Id","validationEvidenceRefs":["array","@Id"],"status":"@Availability","reasonCodes":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "CostModel": ["object",{"currency":"@Currency","horizonRef":"@Id","basis":["union",["literal","VENUE_RULES"],["literal","OBSERVED_EXECUTION"],["literal","CONDITIONAL_SIMULATION"]],"feeRuleRef":"@Id","spreadModelRef":["union","@Id",["literal",null]],"impactModelRef":["union","@Id",["literal",null]],"latencyModelRef":["union","@Id",["literal",null]],"exitModelRef":["union","@Id",["literal",null]],"calibrationEvidenceRefs":["array","@Id"],"validationEvidenceRefs":["array","@Id"],"applicableScale":["union",["object",{"minimum":"@Money","maximum":"@Money"}],["literal",null]],"status":"@Availability","missingComponents":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "ResearchWorkAllocation": ["object",{"campaignId":"@Id","budgetId":"@Id","consideredHypothesisIds":["array","@Id"],"selectedHypothesisIds":["array","@Id"],"nextExperimentPurpose":["union",["union",["literal","FEASIBILITY"],["literal","CALIBRATION"],["literal","ENSEMBLE_FIT"],["literal","JOINT_REPLAY"],["literal","CAPITAL_SCALE"],["literal","ABLATION"],["literal","ROBUSTNESS"]],["literal",null]],"expectedInformationEvidenceRef":["union","@Id",["literal",null]],"resourceEstimateRef":"@Id","incumbentComparisonRef":["union","@Id",["literal",null]],"reasonCodes":["array","string"],"action":["union",["literal","EXPERIMENT"],["literal","WAIT_DATA"],["literal","CLOSE_SEARCH"],["literal","RETIRE_HYPOTHESIS"]],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "BudgetMovement": ["object",{"budgetId":"@Id","campaignId":"@Id","hypothesisId":["union","@Id",["literal",null]],"trialId":["union","@Id",["literal",null]],"kind":["union",["literal","RESERVE"],["literal","CONSUME"],["literal","RELEASE"],["literal","TRANSFER"]],"resource":["union",["literal","TRIAL"],["literal","NATIVE_SECONDS"],["literal","MODEL_COST"],["literal","DATA_BYTES"]],"amount":"@Decimal","currency":["union","@Currency",["literal",null]],"fromReservationId":["union","@Id",["literal",null]],"toReservationId":["union","@Id",["literal",null]],"measurementEvidenceRef":["union","@Id",["literal",null]],"reasonCode":"string","schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "FeatureField": ["object",{"name":"string","dataType":["union",["literal","FLOAT64"],["literal","DECIMAL"],["literal","INTEGER_STRING"],["literal","BOOLEAN"],["literal","CATEGORY"]],"unit":"string","sourceFieldRef":"@Id","transformationRef":["union","@Id",["literal",null]],"availabilityRuleRef":"@Id","missingValuePolicy":["union",["literal","UNAVAILABLE"],["literal","EXPLICIT_MASK"]]}],
  "FeatureSchema": ["object",{"fields":["array","@FeatureField"],"instrumentIdentitySchemaId":"string","eventClockField":"string","availableClockField":"string","warmupRuleRef":"@Id","fitParameterEvidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "SignalContract": ["object",{"kind":["union",["literal","RETURN_FORECAST"],["literal","RANK_SCORE"],["literal","EVENT_PROBABILITY"],["literal","RELATIVE_VALUE"],["literal","ORDER_INTENT"],["literal","TARGET_INTENT"]],"unit":"string","labelDefinitionId":["union","@Id",["literal",null]],"valueSchemaId":"string","horizonRef":"@Id","confidenceEvidenceRequired":"boolean","validityRuleRef":"@Id","requiredCapabilities":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "StrategyStateContract": ["object",{"stateSchemaId":"string","initialStateRef":"@Id","eventSchemaIds":["array","string"],"transitionRuleRef":"@Id","entryRuleRef":["union","@Id",["literal",null]],"holdingRuleRef":["union","@Id",["literal",null]],"exitRuleRef":["union","@Id",["literal",null]],"invalidationRuleRef":"@Id","inventoryModelRef":["union","@Id",["literal",null]],"multiLegProtocolRef":["union","@Id",["literal",null]],"requiredCapabilities":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "MarketAdapterContract": ["object",{"venueId":"string","instrumentSchemaId":"string","instrumentRulesRef":"@Id","calendarRef":"@Id","eventRelationsRef":["union","@Id",["literal",null]],"priceAndSettlementRuleRef":"@Id","currencyAndCollateralRuleRef":"@Id","feesRef":"@Id","corporateActionRuleRef":["union","@Id",["literal",null]],"borrowingAndMarginRuleRef":["union","@Id",["literal",null]],"sourceClockRuleRef":"@Id","capabilityDescriptorId":"@Id","schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "OutcomeAttribution": ["object",{"observedOutcomeId":"@Id","decisionId":"@Id","scope":"@ComparisonScope","cashFlowNormalizationRef":"@Id","actualObservationRefs":["array","@Id"],"counterfactualEvidenceRefs":["array","@Id"],"components":["array",["object",{"kind":["union",["literal","FORECAST"],["literal","ALLOCATION"],["literal","CAPITAL"],["literal","FEES"],["literal","EXECUTION"],["literal","DATA"]],"metric":"@MetricObservation","identification":["union",["literal","EXACT_ACCOUNTING"],["literal","MATCHED_COUNTERFACTUAL"],["literal","UNIDENTIFIED"]],"methodRef":["union","@Id",["literal",null]]}]],"aggregation":["union",["literal","ADDITIVE_ACCOUNTING"],["literal","NON_ADDITIVE_COUNTERFACTUAL"]],"residual":["union","@MetricObservation",["literal",null]],"limitations":["array","string"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "RetirementDisposition": ["object",{"strategyVersionId":"@Id","pipelineAssessmentId":"@Id","signalGeneration":["literal","STOPPED"],"positionDisposition":["union",["literal","NONE"],["literal","MONITORING_EXISTING"],["literal","EXIT_TARGET_PENDING"],["literal","EXTERNAL_HANDOFF"],["literal","FLAT_CONFIRMED"]],"residualPositionsEvidenceRef":["union","@Id",["literal",null]],"outstandingTargetCommandIds":["array","@Id"],"exitDecisionId":["union","@Id",["literal",null]],"reconciliationRequired":"boolean","closureEvidenceRefs":["array","@Id"],"schemaVersion":["literal",1],"id":"@Id","revision":"string","supersedesId":["union","@Id",["literal",null]],"createdAtNs":"@Ns","producerVersion":"string"}],
  "PmMandateStart": ["object",{"requestId":"string","goal":"string","datasetId":"string","funding":["union",["object",{"kind":["literal","ACCOUNT"],"accountBindingId":"@Id"}],["object",{"kind":["literal","RESEARCH_SCENARIO"],"capital":"@Money"}]],"model":"@ModelRef","maxTrials":"number","operatingAuthority":["optional",["union",["literal","RESEARCH_ONLY"],["literal","SHADOW_TARGETS"],["literal","AUTHORIZED_TARGETS"]]],"businessPreference":["optional",["union",["literal","NET_RETURN"],["literal","DOWNSIDE_FIRST"],["literal","FRONTIER"]]]}],
  "PmCommand": ["object",{"requestId":"string","expectedRevision":"string"}],
  "PmMandateView": ["object",{"mandate":"@MandateRevision","activePolicy":"@DecisionPolicyRevision","currentDecision":["union","@PortfolioDecision",["literal",null]],"lastCycle":["union","@DecisionCycle",["literal",null]],"campaigns":["array","@ResearchCampaign"],"researchTrials":["array","@ResearchTrial"],"budgetMovements":["array","@BudgetMovement"],"searchFamily":"@SearchFamily","strategies":["array","@StrategyVersion"],"assessments":["array","@PipelineAssessment"],"capacityEstimates":["array","@CapacityEstimate"],"gaps":["array","@EvidenceGap"],"history":["array","@PortfolioDecision"],"cycles":["array","@DecisionCycle"],"datasetRevisions":["array","@DatasetRevision"],"capabilities":["array","@CapabilityDescriptor"],"ensembles":["array","@EnsembleRevision"],"comparisons":["array","@OperatingComparison"],"metricDefinitions":["optional",["array","@MetricDefinition"]],"outcomes":["array","@ObservedOutcome"],"targetCommands":["array","@TargetCommand"],"attributions":["array","@OutcomeAttribution"],"retirements":["array","@RetirementDisposition"]}],
  ModelRef: ['object', { provider: 'string', modelId: 'string' }],
};
