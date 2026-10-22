import { createHash } from 'node:crypto';
import type { JsonObject } from '@earendil-works/pi-durable';
import { pmCanonical } from '../../../packages/contracts/src/pm-validation.ts';
import { pmAllocationMethod } from './pm-method.ts';

export const PM_OPERATING_RULE_ID = 'qz.pm.frozen-current-spot-operating/2';

/** Must be registered in the original policy before its research/independent
 * evidence is produced. This permits applying a fixed learned method to fresh
 * facts; it is not permission to fit, choose a new method, or send real orders.
 * All numeric tolerances below resolve representation ties only. They are not
 * owner utility, minimum economic return, confidence bounds or significance.
 */
export function pmOperatingRule(): JsonObject {
  const method = pmAllocationMethod();
  const tolerance = ((method.optimizer as JsonObject).parameters as JsonObject).solver_tolerance;
  if (typeof tolerance !== 'string') throw new Error('pm_registered_solver_tolerance_required');
  return {
    schemaVersion: 1,
    id: PM_OPERATING_RULE_ID,
    purpose: 'MODEL_METHOD_APPLICATION',
    allocationMethodRef: method.id!,
    registration: 'ORIGINAL_POLICY_BEFORE_RESEARCH_AND_INDEPENDENT_EVIDENCE',
    fixedInputs: ['FACTOR_MEMBERS_AND_MODEL_BYTES', 'FROZEN_CALIBRATION_COEFFICIENTS', 'LEARNED_ENSEMBLE_WEIGHTS',
      'ORIGINAL_OPTIMIZER_PARAMETERS_INCLUDING_RISK_AVERSION', 'SELECTED_CAPITAL_TO_NAV_RATIO', 'EXECUTION_AND_COST_POLICY',
      'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1',
      'ORIGINAL_SCHEDULE_ANCHOR_AND_TARGET_TTL'],
    cashFunding: { protocol: 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1',
      domain: 'OBSERVED_CASH_LONG_ONLY_MAX_UTILITY_VARIANCE',
      solver: 'SAME_ORIGINAL_SINGLE_QP_WITH_LINEAR_TURNOVER_CASH_CONSTRAINT',
      requirement: 'HISTORICAL_SELECTION_INDEPENDENT_AND_CURRENT_APPLICATION_USE_IDENTICAL_FUNDING_RULE',
      legacyEvidence: 'PRE_FUNDING_METHOD_EVIDENCE_CANNOT_QUALIFY_THIS_RULE' },
    variableInputs: ['CURRENT_ORIGINAL_OBSERVED_BALANCES', 'CURRENT_MATCHED_CAPITAL_CONTROL', 'CUTOFF_AVAILABLE_MARKET_FACTS'],
    clockRule: {
      cadence: 'ORIGINAL_SELECTION_ANCHOR_PLUS_INTEGER_TARGET_TTL',
      admission: 'AT_MOST_ONE_LOGICAL_SOLVE_PER_ORIGINAL_GRID_SLOT',
      actualCutoff: 'CYCLE_ASOF_AFTER_REQUIRED_SOURCE_RECEIPTS_ARE_AVAILABLE',
      delayedSource: 'WAIT_FOR_TRUSTWORTHY_SOURCE_NEVER_BACKDATE_RECEIVER_AVAILABILITY_TO_GRID',
      maximumInputAge: 'ORIGINAL_TARGET_TTL',
      originalTaskRecovery: 'RESUME_ORIGINAL_LOGICAL_SOLVE_AND_CHECKPOINTS',
      sameBasis: 'ORIGINAL_POLICY_STRATEGY_DATA_ACCOUNT_AND_GRID_SLOT',
    },
    decisionRule: {
      baseline: 'HOLD_ORIGINAL_OBSERVED_BALANCES',
      candidate: 'ONE_NATIVE_BUILD_OBSERVED_SPOT_WITH_FIXED_METHOD',
      direction: 'MINIMIZE',
      nativeObjective: 'LAMBDA_TIMES_VARIANCE_PLUS_LINEAR_TURNOVER_COST_MINUS_GROSS_EXPECTED_RETURN',
      feasibilityPriority: {
        applicableConstraints: ['LONG_ONLY', 'GROSS_EXPOSURE', 'NET_EXPOSURE', 'ASSET_WEIGHT', 'CASH_WEIGHT', 'TURNOVER'],
        baseline: 'ACTUAL_ORIGINAL_INVENTORY_AT_CURRENT_CUTOFF',
        proposal: 'ACTUAL_NATIVE_ROUNDED_INVENTORY_CASH_AND_FEES',
        denominators: ['ORIGINAL_CUTOFF_NAV', 'CONSERVATIVE_MODELED_LINEAR_FEE_POST_NAV'],
        fees: 'ORIGINAL_NATIVE_LINEAR_COST_SCENARIO_NOT_REALIZED_FEES',
        infeasibleHold: 'OBJECTIVE_CANNOT_SELECT_HOLD_OVER_A_PROVEN_FEASIBLE_PROPOSAL',
        feasibleRepair: 'REDUCE_OR_REBALANCE_USING_ORIGINAL_FROZEN_METHOD',
        unprovenRepair: 'NEEDS_EVIDENCE',
        unsupportedConstraints: 'NEEDS_EVIDENCE_NEVER_IGNORE_GROUP_OVERRIDE_PARTICIPATION_OR_RISK_REQUIREMENTS',
        candidateScope: 'ONLY_THE_SINGLE_ORIGINAL_NATIVE_PROPOSAL_NO_SEARCH_OR_PARAMETER_CHANGE',
        authority: 'FEASIBILITY_REPAIR_DOES_NOT_BYPASS_CURRENT_TARGET_QUALIFICATION_OR_OWNER_AUTHORITY',
      },
      compare: 'HOLD_OBJECTIVE_MINUS_PROPOSED_OBJECTIVE_GREATER_THAN_NUMERICAL_TIE_THRESHOLD',
      numericalTie: { version: 'qz.pm.modeled-objective-numerical-tie/1',
        absoluteTolerance: tolerance, relativeTolerance: tolerance,
        parameterSource: 'ORIGINAL_REGISTERED_OPTIMIZER_SOLVER_TOLERANCE',
        formula: 'ABSOLUTE_PLUS_RELATIVE_TIMES_MAX_ABSOLUTE_HOLD_OR_PROPOSED_OBJECTIVE',
        purpose: 'REPRESENTATION_TIE_NOT_ECONOMIC_PREFERENCE_OR_STATISTICAL_EVIDENCE' },
      onTieOrNoImprovement: 'HOLD',
      onMissingOrInvalidNativeEvidence: 'NEEDS_EVIDENCE',
      forecastUnit: 'RETURN_PER_HORIZON',
      riskUnit: 'SQUARED_RETURN_PER_SAME_HORIZON',
    },
    researchBoundary: {
      operatingCompute: 'METER_ORIGINAL_OPERATING_WORK_WITHOUT_NEW_SEARCH_TRIAL',
      changedFixedInput: 'RETURN_TO_RESEARCH_AND_QUALIFICATION_WITH_ORIGINAL_BUDGET_AND_EXPOSURES',
      oldPolicyWithoutRule: 'INELIGIBLE_NO_RETROACTIVE_QUALIFICATION',
      independentEvidence: 'ORIGINAL_METHOD_EVIDENCE_NEVER_REOPEN_SEALED_FOR_OPERATING_UPDATE',
    },
    qualificationBoundary: {
      modeledComparison: 'CONDITIONAL_MODEL_ESTIMATES_NOT_REALIZED_PNL_OR_PREDICTED_DAILY_RETURN_OR_DRAWDOWN',
      originalIndependentEvidence: 'METHOD_EVIDENCE_DOES_NOT_PROVE_ACTUAL_OPERATING_DELAY_PNL',
      paperExecution: 'SHADOW_OBSERVATION_ONLY_WITH_PAPER_SOURCE_AND_CURRENT_EXECUTION_BOUNDS',
      liveExecution: 'ORIGINAL_TARGET_OPERATION_INDEPENDENT_PROTOCOL_AND_AUTHORIZATION_AND_VERIFIED_PIT_REQUIRED',
      separateRequirements: ['CURRENT_ACCOUNT_AND_NO_OPEN_ORDERS', 'NATIVE_COST_AND_LOT_FEASIBILITY',
        'CURRENT_LIQUIDITY_AND_EXIT_EVIDENCE', 'OWNER_BOUNDARIES', 'CURRENT_EXECUTION_AUTHORITY'],
      authorization: 'MODELED_IMPROVEMENT_ALONE_NEVER_AUTHORIZES_TARGET_DELIVERY',
    },
  };
}

/** Reject missing/modified rules; callers must pass the rule retained by the
 * original policy, never substitute pmOperatingRule() for a missing value. */
export function validatePmOperatingRule(value: unknown): JsonObject {
  const canonical = pmCanonical(value);
  if (canonical !== pmCanonical(pmOperatingRule())) throw new Error('pm_original_operating_rule_missing_or_changed');
  // Official transaction documents expose JSON through proxies. Normalize the
  // already-validated JSON rather than structuredClone() of the proxy itself.
  return JSON.parse(canonical) as JsonObject;
}
export function pmOperatingRuleIdentity(value: unknown): string {
  return `sha256:${createHash('sha256').update(pmCanonical(validatePmOperatingRule(value))).digest('hex')}`;
}
