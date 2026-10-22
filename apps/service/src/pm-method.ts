import type { JsonObject } from '@earendil-works/pi-durable';
import { pmCanonical } from '../../../packages/contracts/src/pm-validation.ts';

/** Registered before experiments as a candidate-generation method. This does
 * not set the owner's utility, mandate loss limit or a probability guarantee.
 * A new parameter variant requires its own frozen policy and counted trial.
 */
export function pmAllocationMethod(): JsonObject {
  return { id: 'qz.pm.owner-nav-constrained-mean-variance/3', purpose: 'CANDIDATE_GENERATION',
    objective: 'MAX_UTILITY', riskMeasure: 'VARIANCE', parameterOrigin: 'REGISTERED_METHOD',
    capitalBasis: 'INITIAL_NAV_EXPOSURE_RATIO', reinvestmentRule: 'CONSTANT_EXPOSURE_FRACTION_OF_CURRENT_NAV',
    inputFreshnessRule: 'MAXIMUM_AGE_EQUALS_ORIGINAL_REGISTERED_TARGET_TTL',
    clockRule: 'FIRST_SELECTION_TARGET_PLUS_INTEGER_REGISTERED_TTL_INTERVALS',
    cashFunding: { applicableDomain: 'OBSERVED_SPOT_CASH_LONG_ONLY_MAX_UTILITY_VARIANCE',
      nativeConstraint: 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1',
      method: 'ONE_QP_ORIGINAL_LINEAR_TURNOVER_COST_WITHIN_DEPLOYMENT_BUDGET',
      otherDomains: 'UNCHANGED_ORIGINAL_REGISTERED_ACCOUNTING_CONSTRAINTS',
      qualification: 'REGISTER_BEFORE_RESEARCH_AND_INDEPENDENT_EVALUATION_NO_RETROACTIVE_INHERITANCE' },
    numericalOutput: { protocol: 'VALIDATED_FEASIBLE_SEGMENT_V1',
      nativeParameter: 'optimizer.parameters.cash_numerical_output',
      applicableDomain: 'OBSERVED_SPOT_CASH_LONG_ONLY_MAX_UTILITY_VARIANCE_WITH_REGISTERED_CASH_FUNDING',
      rawPoint: 'PRESERVE_ORIGINAL_SINGLE_QP_STATUS_SOLUTION_OBJECTIVE_AND_RESIDUALS',
      trigger: 'RAW_DECIMAL_OUTPUT_FAILS_COMPLETE_ORIGINAL_CONSTRAINT_VALIDATION',
      anchors: ['ORIGINAL_CURRENT_WEIGHTS', 'ALL_CASH'],
      anchorAdmission: 'COMPLETE_ORIGINAL_CONSTRAINT_VALIDATION_AND_POSITIVE_FUNDING_MARGIN',
      correction: 'BOUNDED_DETERMINISTIC_EXACT_DECIMAL_SEGMENT_NO_ADDITIONAL_QP',
      objectiveReference: 'ORIGINAL_18_DECIMAL_RAW_TARGETS',
      objectiveChangeLimit: 'ABSOLUTE_CHANGE_AT_MOST_ORIGINAL_OPTIMIZER_SOLVER_TOLERANCE',
      acceptance: 'COMPLETE_ORIGINAL_CONSTRAINT_REVALIDATION_WITH_STRICT_NONNEGATIVE_POST_FEE_CASH',
      failure: 'FAILED_WITH_ORIGINAL_NATIVE_TRACE_NO_CONSTRAINT_RELAXATION',
      otherDomains: 'UNCHANGED_ORIGINAL_STRICT_NUMERICAL_OUTPUT',
      qualification: 'REGISTER_BEFORE_RESEARCH_AND_INDEPENDENT_EVALUATION_NO_RETROACTIVE_INHERITANCE' },
    optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1',
      parameters: { schema_version: 1, risk_aversion: '1', cvar_confidence: null, risk_budgeting: null,
        max_iterations: 200, solver_tolerance: '0.0000000001', accept_inaccurate: false } },
    limitations: ['REGISTERED_METHOD_NOT_LEARNED_PARAMETERS', 'OWNER_UTILITY_AND_LOSS_LIMITS_ARE_SEPARATE', 'HISTORICAL_SELECTION_REQUIRES_INDEPENDENT_EVALUATION'] };
}

export function validatePmAllocationMethod(value: unknown): JsonObject {
  const serialized = pmCanonical(value);
  if (serialized !== pmCanonical(pmAllocationMethod())) throw new Error('pm_unregistered_allocation_method');
  return JSON.parse(serialized) as JsonObject;
}

/** The protocol applies only to the registered observed CASH domain. Other
 * account types keep the original optimizer and strict decimal validation. */
export function pmNativeOptimizer(method: JsonObject, observedCash: boolean): JsonObject {
  const registered = validatePmAllocationMethod(method);
  const optimizer = registered.optimizer as JsonObject;
  if (observedCash) (optimizer.parameters as JsonObject).cash_numerical_output = (registered.numericalOutput as JsonObject).protocol!;
  return optimizer;
}
