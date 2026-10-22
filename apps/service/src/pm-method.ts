import type { JsonObject } from '@earendil-works/pi-durable';

/** Registered before experiments as a candidate-generation method. This does
 * not set the owner's utility, mandate loss limit or a probability guarantee.
 * A new parameter variant requires its own frozen policy and counted trial.
 */
export function pmAllocationMethod(): JsonObject {
  return { id: 'qz.pm.owner-nav-constrained-mean-variance/1', purpose: 'CANDIDATE_GENERATION',
    objective: 'MAX_UTILITY', riskMeasure: 'VARIANCE', parameterOrigin: 'REGISTERED_METHOD',
    capitalBasis: 'INITIAL_NAV_EXPOSURE_RATIO', reinvestmentRule: 'CONSTANT_EXPOSURE_FRACTION_OF_CURRENT_NAV',
    inputFreshnessRule: 'MAXIMUM_AGE_EQUALS_ORIGINAL_REGISTERED_TARGET_TTL',
    clockRule: 'FIRST_SELECTION_TARGET_PLUS_INTEGER_REGISTERED_TTL_INTERVALS',
    optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1',
      parameters: { schema_version: 1, risk_aversion: '1', cvar_confidence: null, risk_budgeting: null,
        max_iterations: 200, solver_tolerance: '0.0000000001', accept_inaccurate: false } },
    limitations: ['REGISTERED_METHOD_NOT_LEARNED_PARAMETERS', 'OWNER_UTILITY_AND_LOSS_LIMITS_ARE_SEPARATE', 'HISTORICAL_SELECTION_REQUIRES_INDEPENDENT_EVALUATION'] };
}
