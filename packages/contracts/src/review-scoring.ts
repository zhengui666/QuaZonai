import type { ReviewDimensionId, ReviewPolicy, ReviewTargetKind } from './review.ts';

const DIMENSIONS: ReviewDimensionId[] = ['DATA', 'STATISTICS', 'ROBUSTNESS', 'EXECUTION', 'INCREMENTAL', 'MECHANISM'];
const REQUIREMENTS = ['provenance', 'implementation', 'trial_ledger', 'reproducibility', 'no_leakage', 'independent_window', 'execution_constraints'];
const REJECT = ['provenance', 'implementation', 'reproducibility', 'no_leakage', 'execution_constraints'];
/** Registered bounded scales: their attainable endpoints are independent of the reference population. */
export const REVIEW_FIXED_SCORE_BOUNDS_V1: Readonly<Record<string, { low: number; high: number }>> = {
  'data.coverage': { low: 0.9, high: 1 },
  'robustness.positive_fraction': { low: 0.5, high: 1 },
  'mechanism.evidence_fraction': { low: 0, high: 1 },
  'selection.dsr': { low: 0.5, high: 0.99 },
  'selection.pbo': { low: 0, high: 1 },
};
const MINIMUM_TARGETS: Readonly<Record<string, { value: number; inclusive: boolean }>> = {
  'factor.rank_ic_lower': { value: 0, inclusive: false },
  'account.net_return_lower': { value: 0, inclusive: false },
  'execution.stressed_net_return': { value: 0, inclusive: false },
  'incremental.net_return_delta_lower': { value: 0, inclusive: false },
  'selection.dsr': { value: 0.95, inclusive: true },
};
const requiredMinimumKeys = (kind: ReviewTargetKind) => [kind === 'FACTOR' ? 'factor.rank_ic_lower' : 'account.net_return_lower', 'execution.stressed_net_return', 'incremental.net_return_delta_lower', 'selection.dsr'];
const invalid = (message: string): never => { throw new Error(message); };
const ns = (value: unknown): value is string => typeof value === 'string' && /^(0|[1-9]\d*)$/.test(value);
const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
const identifier = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0 && value.length <= 512;
const unique = (values: readonly string[]) => new Set(values).size === values.length;
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

/** Cold-start policies emit reports but cannot auto-release without reference evidence. */
export function createReviewPolicy(kind: ReviewTargetKind, id: string, frozenAtNs: string): ReviewPolicy {
  const definitions: [string, ReviewDimensionId, 'HIGHER' | 'LOWER', number, number, string][] = [
    ['data.coverage', 'DATA', 'HIGHER', 0.9, 1, 'RATIO'],
    [kind === 'FACTOR' ? 'factor.rank_ic_lower' : 'account.net_return_lower', 'STATISTICS', 'HIGHER', 0, 0.05, kind === 'FACTOR' ? 'CORRELATION' : 'RETURN_RATIO'],
    ['selection.dsr', 'STATISTICS', 'HIGHER', 0.5, 0.99, 'PROBABILITY'],
    ['robustness.positive_fraction', 'ROBUSTNESS', 'HIGHER', 0.5, 1, 'RATIO'],
    ['execution.stressed_net_return', 'EXECUTION', 'HIGHER', 0, 0.01, 'RETURN_RATIO'],
    ['incremental.net_return_delta_lower', 'INCREMENTAL', 'HIGHER', 0, 0.01, 'RETURN_RATIO'],
    ['mechanism.evidence_fraction', 'MECHANISM', 'HIGHER', 0, 1, 'RATIO'],
  ];
  return validateReviewPolicy({ schemaVersion: 1, id, targetKind: kind, frozenAtNs, calibration: null,
    requiredChecks: [...REQUIREMENTS, ...(kind === 'ALLOCATION' ? ['independent_outcome'] : [])], rejectChecks: [...REJECT, ...(kind === 'ALLOCATION' ? ['independent_outcome'] : [])],
    metricRules: definitions.map(([key, dimension, direction, low, high, unit]) => ({ key, dimension, direction, low, high, normalization: REVIEW_FIXED_SCORE_BOUNDS_V1[key] ? 'FIXED_BOUNDS_V1' : 'REFERENCE_P10_P90_V1', ...(MINIMUM_TARGETS[key] ? { minimumTarget: { ...MINIMUM_TARGETS[key] } } : {}),
      minimumObservations: dimension === 'MECHANISM' ? 5 : 30, required: true, scope: dimension === 'DATA' ? 'DATA' : dimension === 'MECHANISM' ? 'MECHANISM' : key.startsWith('selection.') ? 'SELECTION_COHORT' : dimension === 'STATISTICS' || dimension === 'ROBUSTNESS' ? kind === 'FACTOR' ? 'POOLED_TIME_SERIES' : 'SHARED_ACCOUNT' : 'SHARED_ACCOUNT', methods: [dimension === 'MECHANISM' ? 'qz.review-mechanism/1' : 'qz.review-science.v1'], unit, frequency: 'REGISTERED' })),
    dimensionWeights: kind === 'FACTOR' ? { DATA: 20, STATISTICS: 25, ROBUSTNESS: 20, EXECUTION: 15, INCREMENTAL: 15, MECHANISM: 5 }
      : { DATA: 15, STATISTICS: 20, ROBUSTNESS: 15, EXECUTION: 20, INCREMENTAL: 25, MECHANISM: 5 },
    passScore: 80, observeScore: 65, dimensionFloor: 60 });
}

/** Trust boundary for owner-frozen policy; model answers do not supply this object. */
export function validateReviewPolicy(value: unknown): ReviewPolicy {
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid('review_policy_required');
  const p = value as ReviewPolicy;
  if (p.schemaVersion !== 1 || !identifier(p.id) || !['FACTOR', 'PORTFOLIO', 'ALLOCATION'].includes(p.targetKind) || !ns(p.frozenAtNs)) invalid('review_policy_invalid');
  if (!finite(p.passScore) || !finite(p.observeScore) || !finite(p.dimensionFloor) || p.passScore < 80 || p.passScore > 100 || p.observeScore < 65 || p.observeScore >= p.passScore || p.dimensionFloor < 60 || p.dimensionFloor > 100) invalid('review_policy_threshold_invalid');
  for (const field of ['requiredChecks', 'rejectChecks'] as const) if (!Array.isArray(p[field]) || !p[field].every(identifier) || !unique(p[field])) invalid('review_policy_checks_invalid');
  if (p.targetKind === 'ALLOCATION' && (!p.requiredChecks.includes('independent_outcome') || !p.rejectChecks.includes('independent_outcome'))) invalid('review_policy_independent_outcome_required');
  if (REQUIREMENTS.some(id => !p.requiredChecks.includes(id)) || REJECT.some(id => !p.rejectChecks.includes(id)) || p.rejectChecks.some(id => !p.requiredChecks.includes(id))) invalid('review_policy_checks_incomplete');
  if (!p.dimensionWeights || DIMENSIONS.some(id => !finite(p.dimensionWeights[id]) || p.dimensionWeights[id] <= 0) || Object.values(p.dimensionWeights).reduce((a, b) => a + b, 0) !== 100) invalid('review_policy_dimension_weights_invalid');
  if (!Array.isArray(p.metricRules) || !p.metricRules.length || p.metricRules.length > 128 || !unique(p.metricRules.map(r => r.key))) invalid('review_policy_metrics_invalid');
  for (const r of p.metricRules) if (!identifier(r.key) || !DIMENSIONS.includes(r.dimension) || !['HIGHER', 'LOWER'].includes(r.direction) || !finite(r.low) || !finite(r.high) || r.low >= r.high || !Number.isSafeInteger(r.minimumObservations) || r.minimumObservations < 1 || typeof r.required !== 'boolean' || !Array.isArray(r.methods) || !r.methods.length || !r.methods.every(identifier) || !identifier(r.scope) || !identifier(r.unit) || !identifier(r.frequency)) invalid('review_policy_metric_invalid');
  for (const rule of p.metricRules) if (rule.normalization !== undefined && !['FIXED_BOUNDS_V1', 'REFERENCE_P10_P90_V1'].includes(rule.normalization)) invalid('review_policy_normalization_invalid');
  for (const rule of p.metricRules) if (rule.minimumTarget !== undefined && (!finite(rule.minimumTarget.value) || typeof rule.minimumTarget.inclusive !== 'boolean' || rule.direction !== 'HIGHER')) invalid('review_policy_minimum_target_invalid');
  for (const key of requiredMinimumKeys(p.targetKind)) {
    const rule = p.metricRules.find(rule => rule.key === key); const floor = MINIMUM_TARGETS[key]!;
    if (!rule?.required || !rule.minimumTarget || rule.minimumTarget.value < floor.value
      || rule.minimumTarget.value === floor.value && !floor.inclusive && rule.minimumTarget.inclusive) invalid('review_policy_minimum_target_required');
  }
  if (DIMENSIONS.some(id => !p.metricRules.some(r => r.dimension === id && r.required))) invalid('review_policy_dimension_missing');
  if (p.calibration !== null && (!p.calibration || !Number.isSafeInteger(p.calibration.rootCount) || p.calibration.rootCount < 30 || !ns(p.calibration.endNs) || BigInt(p.calibration.endNs) > BigInt(p.frozenAtNs) || !Array.isArray(p.calibration.evidenceRefs) || !p.calibration.evidenceRefs.length || !p.calibration.evidenceRefs.every(identifier))) invalid('review_policy_calibration_invalid');
  return clone(p);
}
