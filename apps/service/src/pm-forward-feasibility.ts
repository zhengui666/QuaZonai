import type { JsonObject } from '@earendil-works/pi-durable';
import { addDecimal, compareDecimal, multiplyDecimal, subtractDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { canonical } from '../../research/src/documents.ts';
import { object } from './facts.ts';

type Feasibility = { status: 'FEASIBLE' | 'INFEASIBLE' | 'UNKNOWN'; reasonCodes: string[] };
export interface PmForwardFeasibility {
  method: 'ORIGINAL_LONG_ONLY_BOUNDS_AND_MODELED_FEE_CASH_V1';
  exposureTolerance: string | null;
  hold: Feasibility;
  proposed: Feasibility;
  amounts: JsonObject | null;
}
/** The original objective only compares feasible alternatives. It cannot make
 * a drifting, constraint-violating incumbent an admissible operating choice. */
export function selectPmForwardProposal(feasibility: PmForwardFeasibility, holdObjective: number,
  proposedObjective: number | null, numericalTie: number): 'HOLD' | 'REBALANCE' | 'NEEDS_EVIDENCE' {
  if (!Number.isFinite(holdObjective) || proposedObjective === null || !Number.isFinite(proposedObjective)
    || !Number.isFinite(numericalTie) || numericalTie < 0 || feasibility.hold.status === 'UNKNOWN'
    || feasibility.proposed.status !== 'FEASIBLE') return 'NEEDS_EVIDENCE';
  if (feasibility.hold.status === 'INFEASIBLE') return 'REBALANCE';
  return holdObjective - proposedObjective > numericalTie ? 'REBALANCE' : 'HOLD';
}
const abs = (value: string) => compareDecimal(value, '0') < 0 ? multiplyDecimal(value, '-1') : value;
const sum = (values: string[]) => values.reduce((total, value) => addDecimal(total, value), '0');
const rows = (value: unknown): JsonObject[] => {
  if (!Array.isArray(value)) throw new Error('native_constraint_rows_missing');
  return value.map(value => object(value));
};

/** A measurement of the original native proposal, not another allocator.
 * Compare exact notionals against original weight bounds by multiplication,
 * without rounding a ratio. Source execution limits are assessed separately.
 */
export function assessPmForwardFeasibility(native: JsonObject): PmForwardFeasibility {
  const unknown = (reason: string): PmForwardFeasibility => ({ method: 'ORIGINAL_LONG_ONLY_BOUNDS_AND_MODELED_FEE_CASH_V1', exposureTolerance: null,
    hold: { status: 'UNKNOWN', reasonCodes: [reason] }, proposed: { status: 'UNKNOWN', reasonCodes: [reason] }, amounts: null });
  try {
    const input = object(object(native.build).input), mandate = object(object(native.request).mandate);
    const constraints = object(input.constraints), tolerance = String(input.exposure_tolerance);
    if (canonical(constraints) !== canonical(mandate.constraints!) || tolerance !== mandate.exposure_tolerance) return unknown('native_original_constraint_identity_mismatch');
    if (constraints.long_only !== true || rows(constraints.group_bounds).length || rows(constraints.asset_overrides).length
      || constraints.max_participation !== null || constraints.max_ex_ante_risk !== null || constraints.liquidity_ref !== null) {
      return unknown('current_constraint_measurement_not_supported');
    }
    if (constraints.cash_funding != null && constraints.cash_funding !== 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1') return unknown('current_cash_funding_measurement_not_supported');
    if (compareDecimal(tolerance, '0') <= 0 || compareDecimal(tolerance, '0.001') > 0) return unknown('native_exposure_tolerance_invalid');
    const valuation = object(native.valuation), nav = String(valuation.total), legs = rows(valuation.legs), assets = rows(input.assets);
    const trades = rows(native.trade_estimates);
    if (compareDecimal(nav, '0') <= 0 || compareDecimal(nav, String(input.capital_assumption)) !== 0
      || assets.length !== trades.length || new Set(trades.map(row => row.instrument_id)).size !== assets.length) return unknown('native_complete_current_inventory_missing');
    const quote = legs.filter(leg => leg.instrument_id === null && leg.currency === valuation.report_currency);
    if (quote.length !== 1 || legs.some(leg => leg.instrument_id !== null && !assets.some(asset => asset.instrument_id === leg.instrument_id))) return unknown('native_complete_current_inventory_missing');
    const holdAmounts: string[] = [], proposedAmounts: string[] = [], changes: string[] = [], costs: string[] = [];
    for (const asset of assets) {
      const trade = trades.find(row => row.instrument_id === asset.instrument_id);
      if (!trade) return unknown('native_complete_rounded_trade_evidence_missing');
      const current = String(trade.current_notional), proposed = String(trade.post_rounding_notional), change = String(trade.rounded_trade_notional), cost = String(trade.modeled_linear_cost);
      const leg = legs.find(row => row.instrument_id === asset.instrument_id);
      // Missing nonquote balances can only mean zero when the original native
      // wrapper emitted a complete explicit zero trade for that allowed asset.
      if (compareDecimal(current, String(leg?.report_value ?? '0')) !== 0
        || compareDecimal(addDecimal(current, change), proposed) !== 0
        || compareDecimal(cost, multiplyDecimal(abs(change), String(asset.transaction_cost_rate))) !== 0
        || compareDecimal(current, '0') < 0 || compareDecimal(proposed, '0') < 0 || compareDecimal(cost, '0') < 0) return unknown('native_rounded_inventory_or_cost_identity_invalid');
      holdAmounts.push(current); proposedAmounts.push(proposed); changes.push(abs(change)); costs.push(cost);
    }
    const originalCash = String(quote[0]!.report_value);
    if (compareDecimal(addDecimal(originalCash, sum(holdAmounts)), nav) !== 0) return unknown('native_current_cash_conservation_invalid');
    const totalCost = sum(costs), postFeeNav = subtractDecimal(nav, totalCost);
    const postFeeCash = subtractDecimal(postFeeNav, sum(proposedAmounts));
    const check = (amounts: string[], cash: string, denominator: string, turnover: string, context: string): string[] => {
      const reasons: string[] = [];
      if (compareDecimal(denominator, '0') <= 0) return [`${context}:nonpositive_nav`];
      const within = (amount: string, minimum: string, maximum: string, label: string) => {
        if (compareDecimal(amount, multiplyDecimal(subtractDecimal(minimum, tolerance), denominator)) < 0) reasons.push(`${context}:${label}_below_minimum`);
        if (compareDecimal(amount, multiplyDecimal(addDecimal(maximum, tolerance), denominator)) > 0) reasons.push(`${context}:${label}_above_maximum`);
      };
      amounts.forEach((amount, index) => within(amount, String(constraints.min_asset_weight), String(constraints.max_asset_weight), `asset:${assets[index]!.instrument_id}`));
      // A cash-account repair may never use the numerical weight tolerance to
      // authorize actual borrowing or conceal an unmodeled fee reserve.
      if (compareDecimal(cash, '0') < 0) reasons.push(`${context}:cash_funding_shortfall`);
      within(cash, String(constraints.min_cash_weight), String(constraints.max_cash_weight), 'cash');
      within(sum(amounts), String(constraints.min_net_exposure), String(constraints.max_net_exposure), 'net_exposure');
      if (compareDecimal(sum(amounts.map(abs)), multiplyDecimal(addDecimal(String(constraints.max_gross_exposure), tolerance), denominator)) > 0) reasons.push(`${context}:gross_exposure_above_maximum`);
      if (compareDecimal(turnover, multiplyDecimal(addDecimal(String(constraints.max_turnover_per_rebalance), tolerance), denominator)) > 0) reasons.push(`${context}:turnover_above_maximum`);
      return reasons;
    };
    const holdReasons = check(holdAmounts, originalCash, nav, '0', 'HOLD');
    // Preserve the optimizer's original cutoff-NAV exposure units, then also
    // measure the actual rounded cash/inventory after its stated linear fees.
    // The second check is a conservative delivery bound, not a new QP solve.
    const proposedReasons = [...check(proposedAmounts, subtractDecimal(nav, sum(proposedAmounts)), nav, sum(changes), 'ROUNDED_CUTOFF_NAV'),
      ...check(proposedAmounts, postFeeCash, postFeeNav, sum(changes), 'ROUNDED_POST_FEE_NAV')];
    if (constraints.cash_funding === 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1'
      && compareDecimal(postFeeCash, multiplyDecimal(subtractDecimal(String(constraints.min_cash_weight), tolerance), nav)) < 0) {
      proposedReasons.push('ROUNDED_CUTOFF_NAV:linear_funding_below_original_cash_floor');
    }
    if (!native.target || !native.rounded_modeled_comparison) proposedReasons.push('ROUNDED:original_native_target_unavailable');
    return { method: 'ORIGINAL_LONG_ONLY_BOUNDS_AND_MODELED_FEE_CASH_V1', exposureTolerance: tolerance,
      hold: { status: holdReasons.length ? 'INFEASIBLE' : 'FEASIBLE', reasonCodes: holdReasons },
      proposed: { status: proposedReasons.length ? 'INFEASIBLE' : 'FEASIBLE', reasonCodes: proposedReasons },
      amounts: { cutoffNav: nav, originalCash, totalModeledLinearCost: totalCost, postFeeNav, postFeeCash,
        grossRoundedTurnover: sum(changes), originalInventory: holdAmounts, roundedInventory: proposedAmounts } };
  } catch {
    return unknown('native_current_constraint_evidence_invalid');
  }
}
