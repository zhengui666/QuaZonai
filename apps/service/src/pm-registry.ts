import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { fact, identity } from './pm-facts.ts';
import { nowNs } from './facts.ts';

export const PM_CAPABILITY_ID = 'qz.native.pm/1';
export function pmCapabilities(): Pm.CapabilityDescriptor[] {
  const operations = ['simulate', 'value-observed-spot', 'study-observed-spot', 'compare-observed-spot'];
  return [{ ...fact(PM_CAPABILITY_ID, '0'), provider: 'Nautilus account adapter', providerVersion: '1',
    strategyArchetypes: ['PREDICTIVE_ALPHA', 'FACTOR_RESEARCH', 'PORTFOLIO_OVERLAY'], dataKinds: ['BAR'],
    predictionUnits: ['RETURN_PER_HORIZON'], accountingModels: ['SINGLE_CURRENCY_SHARED_ACCOUNT', 'MULTI_CURRENCY_SPOT_CASH'],
    executionModels: ['NAUTILUS_BAR_CONDITIONAL'], supportedOperations: operations,
    operationContracts: operations.map(name => ({ name,
      inputSchemaId: `qz.native.${name}.request/1`, outputSchemaId: `qz.native.${name}.result/1`,
      requiredCapabilityIds: [], effect: 'NATIVE_COMPUTE' as const, replayPolicy: 'REQUEST_BOUND' as const, incompatibleCapabilityIds: [] })),
    boundaries: [
      { operation: 'simulate', state: 'CONDITIONAL', requiredEvidence: ['original_catalog', 'registered_cost_rules', 'matched_account_starting_state'], reasonCodes: ['fresh_historical_account_is_not_current_holdings'] },
      { operation: 'study-observed-spot', state: 'CONDITIONAL', requiredEvidence: ['lossless_original_cash_account_snapshot', 'same_account_known_zero_open_orders', 'no_locked_or_borrowed_balances', 'point_in_time_price_valuation', 'explicit_spot_execution_policy'], reasonCodes: ['historical_paired_hold_rebalance_is_not_realized_pnl', 'margin_and_betting_accounts_unsupported'] },
      { operation: 'value-observed-spot', state: 'CONDITIONAL', requiredEvidence: ['lossless_original_cash_account_snapshot', 'original_cutoff_available_price_marks', 'explicit_spot_reporting_currency'], reasonCodes: ['derived_inventory_value_does_not_replace_raw_source_equity'] },
      { operation: 'compare-observed-spot', state: 'CONDITIONAL', requiredEvidence: ['same_original_spot_balances', 'known_zero_open_orders', 'matched_selection_and_cost_policy'], reasonCodes: ['historical_hold_rebalance_counterfactual_not_realized_outcome'] },
      { operation: 'TARGET_OPERATION', state: 'CONDITIONAL', requiredEvidence: ['independent_strategy_evidence', 'authoritative_account_basis', 'live_execution_capacity', 'authorized_target_transport'], reasonCodes: ['historical_replay_does_not_authorize_live_target'] },
      { operation: 'MICROSTRUCTURE', state: 'UNSUPPORTED', requiredEvidence: ['historical_order_book', 'validated_queue_and_impact_model'], reasonCodes: ['bar_data_cannot_establish_microstructure_execution'] },
      { operation: 'EVENT_DRIVEN', state: 'UNSUPPORTED', requiredEvidence: ['point_in_time_event_relations', 'settlement_and_cashflow_rules'], reasonCodes: ['event_settlement_semantics_not_integrated'] },
      { operation: 'RELATIVE_VALUE', state: 'UNSUPPORTED', requiredEvidence: ['multi_leg_execution', 'financing_and_borrowing_rules'], reasonCodes: ['multi_leg_strategy_state_not_integrated'] },
    ], nativeSchemaIds: operations.map(operation => `qz.native.${operation}.request/1`) }];
}

export function assessStage(strategy: Pm.StrategyVersion, stage: Pm.PipelineStage, evidence: Pm.EvidenceRef[], requirements: string[] = []): Pm.PipelineAssessment {
  const independent = evidence.filter(item => item.role === 'INDEPENDENT');
  const missing = [...requirements];
  if (['INDEPENDENT_EVALUATION', 'SHADOW', 'PAPER', 'TARGET_OPERATION', 'MONITORING'].includes(stage) && independent.length === 0) missing.push('independent_strategy_evidence_missing');
  if (stage === 'TARGET_OPERATION') missing.push('live_execution_capacity_requires_current_account_evidence');
  return { ...fact(identity('assessment', [strategy.id, stage, evidence.map(item => item.id), missing])), strategyVersionId: strategy.id, stage,
    outcome: missing.length ? 'NEEDS_EVIDENCE' : 'READY', evaluatedAtNs: nowNs(), requirementsVersion: 'qz.pipeline/1',
    evidenceRefs: evidence.map(item => item.id), reasonCodes: missing, nextPermittedActions: missing.length ? ['COLLECT_EVIDENCE'] : ['ADVANCE'] };
}
