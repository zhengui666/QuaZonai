import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { fact, identity } from './pm-facts.ts';
import { nowNs } from './facts.ts';

export const PM_CAPABILITY_ID = 'qz.native.pm/1';
export function pmCapabilities(): Pm.CapabilityDescriptor[] {
  const operations = ['fit-ensemble', 'freeze-ensemble-calibrations', 'capital-curve', 'study-portfolio', 'simulate', 'value-observed-spot', 'study-observed-spot', 'compare-observed-spot'];
  return [{ ...fact(PM_CAPABILITY_ID, '0'), provider: 'QuaZonai native scientific adapter', providerVersion: '1',
    strategyArchetypes: ['PREDICTIVE_ALPHA', 'FACTOR_RESEARCH', 'PORTFOLIO_OVERLAY'], dataKinds: ['BAR'],
    predictionUnits: ['RETURN_PER_HORIZON'], accountingModels: ['SINGLE_CURRENCY_SHARED_ACCOUNT', 'MULTI_CURRENCY_SPOT_CASH'],
    executionModels: ['NAUTILUS_BAR_CONDITIONAL'], supportedOperations: operations,
    operationContracts: operations.map(name => ({ name,
      inputSchemaId: `qz.native.${name}.request/1`, outputSchemaId: `qz.native.${name}.result/1`,
      requiredCapabilityIds: [], effect: 'NATIVE_COMPUTE' as const, replayPolicy: 'REQUEST_BOUND' as const, incompatibleCapabilityIds: [] })),
    boundaries: [
      { operation: 'fit-ensemble', state: 'CONDITIONAL', requiredEvidence: ['aligned_training_predictions', 'mature_labels', 'frozen_label_definition'], reasonCodes: ['training_only_fitted_coefficients'] },
      { operation: 'capital-curve', state: 'CONDITIONAL', requiredEvidence: ['native_simulation_request', 'original_catalog', 'registered_cost_rules'], reasonCodes: ['historical_replay_is_not_economic_capacity'] },
      { operation: 'simulate', state: 'CONDITIONAL', requiredEvidence: ['original_catalog', 'registered_cost_rules', 'matched_account_starting_state'], reasonCodes: ['fresh_historical_account_is_not_current_holdings'] },
      { operation: 'study-observed-spot', state: 'CONDITIONAL', requiredEvidence: ['lossless_original_cash_account_snapshot', 'same_account_known_zero_open_orders', 'no_locked_or_borrowed_balances', 'point_in_time_price_valuation', 'explicit_spot_execution_policy'], reasonCodes: ['historical_paired_hold_rebalance_is_not_realized_pnl', 'margin_and_betting_accounts_unsupported'] },
      { operation: 'TARGET_OPERATION', state: 'CONDITIONAL', requiredEvidence: ['independent_strategy_evidence', 'authoritative_account_basis', 'live_execution_capacity', 'authorized_target_transport'], reasonCodes: ['historical_replay_does_not_authorize_live_target'] },
      { operation: 'MICROSTRUCTURE', state: 'UNSUPPORTED', requiredEvidence: ['historical_order_book', 'validated_queue_and_impact_model'], reasonCodes: ['bar_data_cannot_establish_microstructure_execution'] },
      { operation: 'EVENT_DRIVEN', state: 'UNSUPPORTED', requiredEvidence: ['point_in_time_event_relations', 'settlement_and_cashflow_rules'], reasonCodes: ['event_settlement_semantics_not_integrated'] },
      { operation: 'RELATIVE_VALUE', state: 'UNSUPPORTED', requiredEvidence: ['multi_leg_execution', 'financing_and_borrowing_rules'], reasonCodes: ['multi_leg_strategy_state_not_integrated'] },
    ], nativeSchemaIds: operations.map(operation => `qz.native.${operation}.request/1`) }];
}

export function pmMetricDefinitions(): Pm.MetricDefinition[] {
  return [
    { id: 'qz.metric.daily_geometric_net_return/1', key: 'account.daily_geometric_net_return', label: 'Calendar geometric daily net return', direction: 'MAXIMIZE', unit: 'RETURN_RATIO', aggregation: 'GEOMETRIC_COMPLETE_UTC_DAYS' },
    { id: 'qz.metric.maximum_drawdown/1', key: 'account.maximum_drawdown', label: 'Maximum drawdown loss', direction: 'MINIMIZE', unit: 'LOSS_RATIO', aggregation: 'ACCOUNT_HIGH_WATER_MARK_LOSS' },
    { id: 'qz.metric.negative_day_fraction/1', key: 'account.negative_day_fraction', label: 'Negative calendar day fraction', direction: 'MINIMIZE', unit: 'RATIO', aggregation: 'NEGATIVE_DAYS_DIVIDED_BY_COMPLETE_DAYS' },
  ].map(row => ({ ...row, definitionVersion: '1', valueEncoding: 'FLOAT64', calendarId: 'UTC_COMPLETE_DAYS', annualizationDays: null,
    horizonRef: row.key === 'account.maximum_drawdown' ? 'EVALUATION_INTERVAL' : 'UTC_CALENDAR_DAY', requiredInputs: ['fully_priced_shared_account', 'matched_net_cost_basis'] })) as Pm.MetricDefinition[];
}
/** Display diagnostics never add an owner objective or a hidden admission threshold. */
export function pmDiagnosticDefinitions(): Pm.MetricDefinition[] {
  return [
    { id: 'qz.metric.recorded_commission/1', key: 'cost.recorded_commission', label: 'Recorded commissions', unit: 'CURRENCY', valueEncoding: 'DECIMAL' },
    { id: 'qz.metric.gross_fill_notional/1', key: 'execution.gross_fill_notional', label: 'Gross fill notional', unit: 'CURRENCY', valueEncoding: 'DECIMAL' },
    { id: 'qz.metric.fill_count/1', key: 'execution.fill_count', label: 'Filled executions', unit: 'COUNT', valueEncoding: 'INTEGER_STRING' },
  ].map(row => ({ ...row, definitionVersion: '1', direction: 'DIAGNOSTIC', aggregation: 'NATIVE_ACCOUNT_INTERVAL_TOTAL', calendarId: 'UTC_COMPLETE_DAYS',
    annualizationDays: null, horizonRef: 'EVALUATION_INTERVAL', requiredInputs: ['native_recorded_fee_totals'] })) as Pm.MetricDefinition[];
}
export function selectionRule(preference: 'NET_RETURN' | 'DOWNSIDE_FIRST' | 'FRONTIER' = 'FRONTIER'): Pm.SelectionRule {
  const definitions = pmMetricDefinitions();
  const order = preference === 'DOWNSIDE_FIRST' ? [1, 2, 0] : [0, 1, 2];
  return { mode: preference === 'FRONTIER' ? 'FRONTIER_ONLY' : 'LEXICOGRAPHIC', objectives: order.map(index => {
    const d = definitions[index]!; return { metricKey: d.key, metricDefinitionId: d.id, direction: d.direction as 'MAXIMIZE' | 'MINIMIZE', horizonRef: d.horizonRef!, scope: 'ACCOUNT' as const };
  }), utilityModelRef: null, establishedBy: 'ACCEPTED_PROFILE', rationale: preference === 'FRONTIER' ? 'Compare net return, drawdown and losing days without inventing an investment tradeoff.' : `${preference}: frozen lexicographic ordering of daily net return, drawdown and losing days; no fitted utility preference.` };
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
