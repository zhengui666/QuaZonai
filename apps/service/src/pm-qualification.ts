import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { compareDecimal, pmCanonical, validateMetricObservation, validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';
import { json } from './facts.ts';

/** Economic falsification of one already selected configuration. This is not
 * a significance test, a new selection round or live execution qualification. */
export function pmQualificationProtocol(selection: Pm.SelectionRule): JsonObject {
  return json({ id: 'qz.pm.frozen-paired-economic-falsification/1', version: 1,
    sequence: ['TRAIN_PURGED_OOF', 'BUDGETED_VALIDATION_SELECTION', 'FREEZE_FIT_METHOD_CAPITAL', 'ONE_UNEXPOSED_PAIRED_INDEPENDENT_EVALUATION'],
    selection, independentRoles: ['FROZEN_CONFIGURATION', 'MATCHED_BASELINE'], baseline: 'CASH_OR_SAME_OBSERVED_SPOT_BALANCES_HOLD',
    requiredEvidence: ['EXACT_FROZEN_CONFIGURATION', 'SAME_SCOPE_AND_STARTING_STATE', 'COMPLETE_REQUIRED_NATIVE_METRICS', 'REGISTERED_COST_POLICY'],
    economicRule: 'STRICTLY_PREFERRED_TO_MATCHED_BASELINE_UNDER_ORIGINAL_OWNER_OBJECTIVES',
    maximumPromotion: 'SHADOW', statisticalSignificanceClaim: false, liveTargetAuthorization: false,
    onContradiction: 'NOT_READY_PRESERVE_EVIDENCE_NO_RESELECTION', onMissingEvidence: 'NEEDS_EVIDENCE_NO_RESELECTION' });
}

export function evaluatePmQualification(protocol: JsonObject, selection: Pm.SelectionRule,
  candidate: Pm.MetricObservation[], baseline: Pm.MetricObservation[], atNs: string): {
    outcome: Pm.PipelineAssessment['outcome']; reasonCodes: string[]; improved: string[]; worsened: string[];
  } {
  const result = { outcome: 'NEEDS_EVIDENCE' as Pm.PipelineAssessment['outcome'], reasonCodes: [] as string[], improved: [] as string[], worsened: [] as string[] };
  if (pmCanonical(protocol) !== pmCanonical(pmQualificationProtocol(selection))) { result.reasonCodes.push('registered_qualification_protocol_mismatch'); return result; }
  validatePmEntity('SelectionRule', selection);
  for (const objective of selection.objectives) {
    const a = candidate.filter(metric => metric.definition.id === objective.metricDefinitionId);
    const b = baseline.filter(metric => metric.definition.id === objective.metricDefinitionId);
    if (a.length !== 1 || b.length !== 1) { result.reasonCodes.push('independent_objective_missing_or_ambiguous'); return result; }
    const left = validateMetricObservation(a[0]); const right = validateMetricObservation(b[0]);
    if (left.status !== 'AVAILABLE' || right.status !== 'AVAILABLE' || left.role !== 'INDEPENDENT' || right.role !== 'INDEPENDENT'
      || pmCanonical(left.definition) !== pmCanonical(right.definition) || pmCanonical(left.interval) !== pmCanonical(right.interval)
      || left.currency !== right.currency || left.definition.key !== objective.metricKey || left.definition.direction !== objective.direction
      || left.definition.horizonRef !== objective.horizonRef || [left, right].some(metric => BigInt(metric.availableAtNs) > BigInt(atNs) || BigInt(metric.asOfNs) > BigInt(atNs))) {
      result.reasonCodes.push('independent_paired_objective_incomparable'); return result;
    }
    const difference = typeof left.value === 'number' && typeof right.value === 'number' ? left.value < right.value ? -1 : left.value > right.value ? 1 : 0
      : typeof left.value === 'string' && typeof right.value === 'string' ? compareDecimal(left.value, right.value) : null;
    if (difference === null) { result.reasonCodes.push('independent_objective_encoding_mismatch'); return result; }
    const preference = difference * (objective.direction === 'MAXIMIZE' ? 1 : -1);
    if (preference > 0) result.improved.push(objective.metricDefinitionId);
    if (preference < 0) result.worsened.push(objective.metricDefinitionId);
  }
  if (selection.mode === 'EXPECTED_UTILITY') { result.reasonCodes.push('registered_utility_evaluation_unavailable'); return result; }
  const firstDifference = selection.objectives.find(objective => result.improved.includes(objective.metricDefinitionId) || result.worsened.includes(objective.metricDefinitionId));
  const preferred = selection.mode === 'LEXICOGRAPHIC' ? !!firstDifference && result.improved.includes(firstDifference.metricDefinitionId)
    : result.improved.length > 0 && result.worsened.length === 0;
  if (selection.mode === 'FRONTIER_ONLY' && result.improved.length && result.worsened.length) { result.reasonCodes.push('independent_owner_tradeoff_unresolved'); return result; }
  result.outcome = preferred ? 'READY' : 'NOT_READY';
  result.reasonCodes.push(preferred ? 'frozen_configuration_survived_registered_economic_falsification' : 'frozen_configuration_not_preferred_to_matched_baseline',
    'shadow_research_promotion_only', 'no_statistical_significance_or_live_execution_claim');
  return result;
}
