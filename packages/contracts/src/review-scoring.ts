import type { ReviewAllocationMember, ReviewAllocationPolicy, ReviewAllocationRequest, ReviewDimensionId, ReviewMetric, ReviewCheck, ReviewPolicy, ReviewReport, ReviewScoringInput, ReviewTargetKind } from './review.ts';

const DIMENSIONS: ReviewDimensionId[] = ['DATA', 'STATISTICS', 'ROBUSTNESS', 'EXECUTION', 'INCREMENTAL', 'MECHANISM'];
const LABELS = { DATA: '数据与实现', STATISTICS: '样本外统计证据', ROBUSTNESS: '稳健性', EXECUTION: '净成本与容量', INCREMENTAL: '增量与分散性', MECHANISM: '机制与简洁性' };
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
const clip = (value: number) => Math.max(0, Math.min(1, value));

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

/** Pure, replayable gate over trusted tool results. Incomplete evidence never reallocates score weights. */
export function scoreReview(input: ReviewScoringInput): ReviewReport {
  const policy = validateReviewPolicy(input.policy); const { target } = input;
  if (!identifier(input.id) || !identifier(target?.id) || !identifier(target.version) || !ns(target.evaluationStartNs) || target.kind !== policy.targetKind || !Array.isArray(target.sourceEvidenceRefs) || !target.sourceEvidenceRefs.length || !target.sourceEvidenceRefs.every(identifier) || !Number.isFinite(Date.parse(input.createdAt))) invalid('review_target_invalid');
  const missing: string[] = []; const failures: string[] = [];
  // frozenAtNs is a wall-clock admission fact, not the historical market interval.
  // The host binds it before starting the native trial; compare calibration market time separately.
  if (!policy.calibration) missing.push('reference_calibration_missing');
  else if (BigInt(policy.calibration.endNs) >= BigInt(target.evaluationStartNs)) missing.push('reference_calibration_not_prior');
  if (!unique(input.checks.map(c => c.id)) || !unique(input.metrics.map(m => m.key))) invalid('review_duplicate_evidence');
  for (const check of input.checks) if (!identifier(check.id) || !['PASS', 'FAIL', 'UNKNOWN'].includes(check.status) || !Array.isArray(check.evidenceRefs) || !check.evidenceRefs.every(identifier)) invalid('review_check_invalid');
  for (const id of policy.requiredChecks) {
    const check = input.checks.find(c => c.id === id);
    if (check?.status === 'FAIL' && policy.rejectChecks.includes(id) && check.evidenceRefs.length) failures.push(`${id}:${check.reason}`);
    else if (!check || check.status !== 'PASS' || !check.evidenceRefs.length) missing.push(`${id}:${check?.reason ?? 'missing_check'}`);
  }
  // Final independent evidence is an additional veto when present; a pre-test
  // portfolio review may omit it, while every downstream consumer requires it.
  if (!policy.requiredChecks.includes('independent_outcome')) {
    const outcome = input.checks.find(check => check.id === 'independent_outcome');
    if (outcome?.status === 'FAIL' && outcome.evidenceRefs.length) failures.push(`independent_outcome:${outcome.reason}`);
    else if (outcome && (outcome.status !== 'PASS' || !outcome.evidenceRefs.length)) missing.push(`independent_outcome:${outcome.reason}`);
  }
  const scored = new Map<string, number>();
  for (const rule of policy.metricRules) {
    const m = input.metrics.find(m => m.key === rule.key);
    let reason: string | null = null;
    if (!m) reason = 'missing';
    else if (m.status !== 'AVAILABLE') reason = m.reason ?? m.status.toLowerCase();
    else if (!finite(m.value) || !Number.isSafeInteger(m.observationCount) || m.observationCount < rule.minimumObservations) reason = 'insufficient_or_invalid';
    else if (!identifier(m.evidenceRef) || !target.sourceEvidenceRefs.includes(m.evidenceRef) || !rule.methods.includes(m.method) || m.scope !== rule.scope || m.unit !== rule.unit || m.frequency !== rule.frequency) reason = 'method_or_scope_mismatch';
    if (reason) { if (rule.required) missing.push(`${rule.key}:${reason}`); continue; }
    if (rule.minimumTarget && (rule.minimumTarget.inclusive ? m!.value! < rule.minimumTarget.value : m!.value! <= rule.minimumTarget.value)) {
      failures.push(`${rule.key}:below_absolute_admission_minimum`);
    }
    const ratio = clip((m!.value! - rule.low) / (rule.high - rule.low));
    scored.set(rule.key, 100 * (rule.direction === 'HIGHER' ? ratio : 1 - ratio));
  }
  const dimensions = DIMENSIONS.map(id => {
    const rules = policy.metricRules.filter(r => r.dimension === id);
    const incomplete = rules.some(r => r.required && !scored.has(r.key));
    const values = rules.filter(r => scored.has(r.key)).map(r => scored.get(r.key)!);
    return { id, label: LABELS[id], score: incomplete || !values.length ? null : values.reduce((a, b) => a + b, 0) / values.length,
      weight: policy.dimensionWeights[id], status: incomplete || !values.length ? 'INCOMPLETE' as const : 'COMPLETE' as const,
      reasons: missing.filter(reason => rules.some(r => reason.startsWith(`${r.key}:`))) };
  });
  const total = dimensions.some(d => d.score === null) ? null : dimensions.reduce((sum, d) => sum + d.score! * d.weight / 100, 0);
  const floor = dimensions.filter(d => ['STATISTICS', 'ROBUSTNESS', 'EXECUTION'].includes(d.id)).every(d => d.score !== null && d.score >= policy.dimensionFloor);
  const decision = failures.length ? 'REJECT' : missing.length || total === null ? 'INCONCLUSIVE' : total < policy.observeScore ? 'REJECT' : total >= policy.passScore && floor ? 'PASS' : 'INCONCLUSIVE';
  const reasons = [...failures, ...missing];
  if (!failures.length && !missing.length && total !== null && decision !== 'PASS') reasons.push(total < policy.observeScore ? 'score_below_reject_threshold' : !floor ? 'required_dimension_below_floor' : 'observation_band');
  return { schemaVersion: 1, id: input.id, target: clone(target), policyId: policy.id, policy, decision,
    totalScore: total, dimensions, qualityCoefficient: decision === 'PASS' ? clip((total! - 60) / 40) : 0,
    reasonCodes: reasons, hardFailures: failures, missingEvidence: missing, metrics: clone(input.metrics), checks: clone(input.checks),
    analyses: clone(input.analyses ?? []), weights: [], narrative: input.narrative ?? null, createdAt: input.createdAt };
}

/** Once-only independent outcome over the same registered native contracts.
 * Raw independent observations stay in owner-only evidence, outside the Agent context. */
export function reviewIndependentOutcomeCheck(policy: ReviewPolicy, metrics: readonly ReviewMetric[], evidenceRef: string): ReviewCheck {
  const frozen = validateReviewPolicy(policy);
  const lowerRule = frozen.metricRules.find(rule => rule.key === 'account.net_return_lower');
  const coverageRule = frozen.metricRules.find(rule => rule.key === 'data.coverage');
  const valid = (key: string, rule: typeof lowerRule) => {
    const matches = metrics.filter(metric => metric.key === key); const metric = matches[0];
    return rule && matches.length === 1 && metric?.status === 'AVAILABLE' && finite(metric.value)
      && Number.isSafeInteger(metric.observationCount) && metric.observationCount >= rule.minimumObservations
      && metric.evidenceRef === evidenceRef && rule.methods.includes(metric.method) && metric.scope === rule.scope && metric.unit === rule.unit && metric.frequency === rule.frequency ? metric : null;
  };
  const lower = valid('account.net_return_lower', lowerRule); const coverage = valid('data.coverage', coverageRule);
  const floor = lowerRule?.minimumTarget;
  const available = !!lower && !!coverage && coverage.value === 1 && !!floor;
  const passes = available && (floor!.inclusive ? lower!.value! >= floor!.value : lower!.value! > floor!.value);
  return { id: 'independent_outcome', status: !available ? 'UNKNOWN' : passes ? 'PASS' : 'FAIL', reason: !available ? 'fresh_independent_statistical_evidence_missing'
    : passes ? 'frozen_configuration_passed_predeclared_independent_net_return_floor' : 'frozen_configuration_failed_predeclared_independent_net_return_floor', evidenceRefs: [evidenceRef] };
}

/** Reports and aligned prior data are resolved by the service, never uploaded metric summaries. */
export function buildReviewAllocationRequest(members: ReviewAllocationMember[], policy: ReviewAllocationPolicy): ReviewAllocationRequest {
  if (!identifier(policy.id) || !ns(policy.frozenAtNs) || !['FACTOR', 'PORTFOLIO'].includes(policy.kind) || members.length < 2 || !unique(members.map(m => m.id))) invalid('review_allocation_policy_invalid');
  const times = policy.observationTimesNs;
  if (times.length < 30 || !times.every(ns) || times.some((t, i) => i > 0 && BigInt(t) <= BigInt(times[i - 1]!)) || BigInt(times.at(-1)!) >= BigInt(policy.frozenAtNs) || policy.observationEvidenceRefs.length !== members.length || !policy.observationEvidenceRefs.every(identifier)) invalid('review_allocation_observations_invalid');
  const resolved = members.map(m => {
    const repeated = scoreReview({ id: m.report.id, target: m.report.target, policy: m.report.policy, metrics: m.report.metrics, checks: m.report.checks, createdAt: m.report.createdAt });
    if (m.report.decision !== 'PASS' || repeated.decision !== 'PASS' || repeated.totalScore !== m.report.totalScore || m.report.target.id !== m.id || m.report.target.kind !== policy.kind || m.observations.length !== times.length || !m.observations.every(finite)) invalid('review_allocation_member_not_approved');
    return { id: m.id, source_report_id: m.report.id, score: repeated.totalScore!, observations: [...m.observations] };
  });
  return { schema_version: 1, kind: policy.kind, members: resolved,
    member_caps: policy.memberCaps.map(c => ({ id: c.id, maximum_weight: c.maximumWeight })),
    cluster_caps: policy.clusterCaps.map(c => ({ id: c.id, members: [...c.members], maximum_weight: c.maximumWeight })),
    current_weights: policy.currentWeights.map(c => ({ id: c.id, weight: c.weight })), maximum_turnover: policy.maximumTurnover };
}

/** Derive q10/q90 anchors from trusted, deduplicated earlier root evidence.
 * The service resolves these records; this function does not confer provenance on uploads. */
export function deriveReviewPolicyFromReference(policy: ReviewPolicy, records: import('./review.ts').ReviewReferenceObservation[], evaluationStartNs: string): ReviewPolicy {
  const result = validateReviewPolicy(policy);
  if (!ns(evaluationStartNs) || records.length < 30 || !unique(records.map(r => r.rootId))) invalid('review_reference_roots_required');
  for (const r of records) if (!identifier(r.rootId) || !ns(r.endNs) || BigInt(r.endNs) >= BigInt(evaluationStartNs) || BigInt(r.endNs) > BigInt(policy.frozenAtNs) || !r.evidenceRefs.length || !r.evidenceRefs.every(identifier) || !unique(r.metrics.map(m => m.key))) invalid('review_reference_scope_invalid');
  for (const rule of result.metricRules) {
    const values = records.flatMap(r => {
      const m = r.metrics.find(m => m.key === rule.key);
      return m?.status === 'AVAILABLE' && finite(m.value) && Number.isSafeInteger(m.observationCount) && m.observationCount >= rule.minimumObservations && rule.methods.includes(m.method) && m.scope === rule.scope && m.unit === rule.unit && m.frequency === rule.frequency && identifier(m.evidenceRef) ? [m.value] : [];
    }).sort((a, b) => a - b);
    if (values.length < 30) invalid('review_reference_metric_insufficient');
    const fixed = REVIEW_FIXED_SCORE_BOUNDS_V1[rule.key];
    if (fixed) {
      if (values.some(value => value < 0 || value > 1)) invalid('review_reference_bounded_metric_invalid');
      rule.low = fixed.low; rule.high = fixed.high; rule.normalization = 'FIXED_BOUNDS_V1';
    } else {
      const low = values[Math.floor((values.length - 1) * 0.1)]!;
      const high = values[Math.floor((values.length - 1) * 0.9)]!;
      if (low >= high) invalid('review_reference_metric_degenerate');
      rule.low = low; rule.high = high; rule.normalization = 'REFERENCE_P10_P90_V1';
    }
  }
  result.calibration = { rootCount: records.length, endNs: records.reduce((latest, r) => BigInt(r.endNs) > BigInt(latest) ? r.endNs : latest, '0'),
    evidenceRefs: [...new Set(records.flatMap(r => r.evidenceRefs))] };
  return validateReviewPolicy(result);
}
