import type { AccountBasis, AssetTarget, Decimal, Id, Money, Ns } from './pm.ts';
import { addDecimal, compareDecimal, multiplyDecimal, PmValidationError, validatePmEntity } from './pm-validation.ts';

/** Original transport evidence, not a PM-generated permission or readiness flag.
 * Rates are nonnegative, all-in execution bounds per traded report-currency
 * notional. Missing components remain unknown, including unknown impact. */
export interface PmExecutionEvidence {
  schemaVersion: 1;
  id: Id;
  sourceKind: 'PAPER_EXECUTION_MODEL' | 'LIVE_EXECUTION_SOURCE';
  sourceRevision: string;
  accountSourceId: string;
  nativeAccountId: string;
  observationId: string;
  sequence: string;
  controlRevision: string;
  currency: string;
  /** Execution source's own economic time; may differ from account observedAtNs. */
  asOfNs: Ns;
  availableAtNs: Ns;
  validUntilNs: Ns;
  methodRef: Id;
  sourceEvidenceRef: Id;
  instruments: {
    instrumentId: string;
    rulesRef: Id;
    costs: { commissionRate: Decimal | null; slippageRate: Decimal | null; impactRate: Decimal | null; evidenceRef: Id };
    entry: { maximumBuyNotional: Decimal; maximumSellNotional: Decimal; evidenceRef: Id };
    exit: { maximumNotional: Decimal; evidenceRef: Id };
  }[];
}
export interface PmExecutionLiquidity {
  instrumentId: string;
  currency: string;
  eventNs: Ns;
  availableNs: Ns;
  notional: Decimal;
  lotNotional: Decimal;
  minimumNotional: Decimal;
  takerFee: Decimal;
}
export interface PmExecutionQualificationInput {
  proof: unknown | null;
  proofEvidenceRef: Id;
  accountBasis: AccountBasis;
  nativeAccountId: string;
  mode: 'paper' | 'live';
  decisionAtNs: Ns;
  evaluatedAtNs: Ns;
  nativeTargetValidUntilNs: Ns;
  frozenTargetTtlNs: Ns;
  liquidityValidUntilNs: Ns;
  liquidity: readonly PmExecutionLiquidity[];
  /** Complete inventory at the same native cutoff marks. null means unknown.
   * An empty normalized UI position list is not a complete native inventory. */
  positions: readonly { instrumentId: string; currentNotional: Money }[] | null;
  targets: readonly AssetTarget[];
  /** Original build.input.assets rates; do not copy these from the source proof. */
  modeledCostRates: readonly { instrumentId: string; rate: Decimal }[];
}
export interface PmExecutionQualification {
  scope: 'CURRENT_SOURCE_EXECUTION_BOUNDS';
  outcome: 'READY' | 'NEEDS_EVIDENCE' | 'NOT_READY';
  validUntilNs: Ns | null;
  evidenceRefs: Id[];
  reasonCodes: string[];
  instruments: { instrumentId: string; allInCostRate: Decimal | null; estimatedTradingCost: Money | null }[];
}

function requireFact(ok: unknown, code: string, path: string): asserts ok {
  if (!ok) throw new PmValidationError(code, path);
}
function object(value: unknown, keys: readonly string[], path: string): Record<string, unknown> {
  requireFact(value !== null && typeof value === 'object' && !Array.isArray(value), 'pm_execution_object_required', path);
  const result = value as Record<string, unknown>;
  requireFact(keys.every(key => Object.hasOwn(result, key)) && Object.keys(result).every(key => keys.includes(key)), 'pm_execution_shape_invalid', path);
  return result;
}
function identity(value: unknown, path: string): asserts value is string {
  requireFact(typeof value === 'string' && value.trim().length > 0 && value.length <= 1024, 'pm_execution_identity_invalid', path);
}
function clock(value: unknown, path: string): asserts value is Ns {
  requireFact(typeof value === 'string' && /^(0|[1-9]\d*)$/.test(value) && value.length <= 32, 'pm_execution_clock_invalid', path);
}
function nonnegative(value: unknown, path: string): asserts value is Decimal {
  requireFact(typeof value === 'string' && compareDecimal(value, '0') >= 0, 'pm_execution_amount_invalid', path);
}
function unique(values: readonly string[], path: string): void {
  requireFact(new Set(values).size === values.length, 'pm_execution_duplicate_instrument', path);
}
export function validatePmExecutionEvidence(value: unknown): PmExecutionEvidence {
  const p = object(value, ['schemaVersion', 'id', 'sourceKind', 'sourceRevision', 'accountSourceId', 'nativeAccountId', 'observationId', 'sequence', 'controlRevision', 'currency', 'asOfNs', 'availableAtNs', 'validUntilNs', 'methodRef', 'sourceEvidenceRef', 'instruments'], 'pmEvidence');
  requireFact(p.schemaVersion === 1 && ['PAPER_EXECUTION_MODEL', 'LIVE_EXECUTION_SOURCE'].includes(String(p.sourceKind)), 'pm_execution_source_kind_invalid', 'pmEvidence');
  for (const key of ['id', 'sourceRevision', 'accountSourceId', 'nativeAccountId', 'observationId', 'controlRevision', 'currency', 'methodRef', 'sourceEvidenceRef']) identity(p[key], `pmEvidence.${key}`);
  for (const key of ['sequence', 'asOfNs', 'availableAtNs', 'validUntilNs']) clock(p[key], `pmEvidence.${key}`);
  requireFact(BigInt(p.asOfNs as string) <= BigInt(p.availableAtNs as string) && BigInt(p.availableAtNs as string) < BigInt(p.validUntilNs as string), 'pm_execution_source_clock_order_invalid', 'pmEvidence');
  requireFact(Array.isArray(p.instruments), 'pm_execution_instruments_required', 'pmEvidence');
  for (const value of p.instruments) {
    const row = object(value, ['instrumentId', 'rulesRef', 'costs', 'entry', 'exit'], 'pmEvidence.instruments');
    identity(row.instrumentId, 'instrumentId'); identity(row.rulesRef, 'rulesRef');
    const costs = object(row.costs, ['commissionRate', 'slippageRate', 'impactRate', 'evidenceRef'], 'costs'); identity(costs.evidenceRef, 'costs.evidenceRef');
    for (const key of ['commissionRate', 'slippageRate', 'impactRate']) if (costs[key] !== null) nonnegative(costs[key], `costs.${key}`);
    const entry = object(row.entry, ['maximumBuyNotional', 'maximumSellNotional', 'evidenceRef'], 'entry');
    identity(entry.evidenceRef, 'entry.evidenceRef'); nonnegative(entry.maximumBuyNotional, 'entry.maximumBuyNotional'); nonnegative(entry.maximumSellNotional, 'entry.maximumSellNotional');
    const exit = object(row.exit, ['maximumNotional', 'evidenceRef'], 'exit'); identity(exit.evidenceRef, 'exit.evidenceRef'); nonnegative(exit.maximumNotional, 'exit.maximumNotional');
  }
  const proof = structuredClone(value) as PmExecutionEvidence;
  unique(proof.instruments.map(row => row.instrumentId), 'pmEvidence.instruments');
  return proof;
}

/** Assesses only source-backed execution bounds. READY is not investment or
 * target authorization: consumers separately establish PIT data, independent
 * method evidence, original native lot/price rules, no outstanding orders,
 * reserves, authority and the atomic revision-bound downstream fence.
 * This pure helper performs no native rounding and never infers fills. */
export function assessPmExecutionQualification(input: PmExecutionQualificationInput): PmExecutionQualification {
  const basis = validatePmEntity('AccountBasis', input.accountBasis);
  identity(input.proofEvidenceRef, 'proofEvidenceRef'); identity(input.nativeAccountId, 'nativeAccountId');
  requireFact(input.mode === 'paper' || input.mode === 'live', 'pm_execution_mode_invalid', 'mode');
  for (const key of ['decisionAtNs', 'evaluatedAtNs', 'nativeTargetValidUntilNs', 'frozenTargetTtlNs', 'liquidityValidUntilNs'] as const) clock(input[key], key);
  requireFact(BigInt(input.frozenTargetTtlNs) > 0n && BigInt(input.evaluatedAtNs) >= BigInt(input.decisionAtNs), 'pm_execution_decision_clock_invalid', 'decisionAtNs');
  const result: PmExecutionQualification = { scope: 'CURRENT_SOURCE_EXECUTION_BOUNDS', outcome: 'READY', validUntilNs: null,
    evidenceRefs: [input.proofEvidenceRef, basis.positionsEvidenceRef], reasonCodes: [], instruments: [] };
  const issue = (code: string, unknown = false) => {
    result.reasonCodes.push(code);
    if (!unknown || result.outcome === 'READY') result.outcome = unknown ? 'NEEDS_EVIDENCE' : 'NOT_READY';
  };
  if (BigInt(basis.availableAtNs) > BigInt(input.decisionAtNs)) issue('execution_account_not_available_at_cutoff');
  if (BigInt(input.nativeTargetValidUntilNs) !== BigInt(input.decisionAtNs) + BigInt(input.frozenTargetTtlNs)) issue('execution_native_ttl_not_original_cutoff');
  if (input.proof === null || input.proof === undefined) { issue('execution_source_proof_missing', true); return result; }
  const proof = validatePmExecutionEvidence(input.proof);
  result.evidenceRefs.push(proof.id, proof.sourceEvidenceRef, proof.methodRef);
  if (proof.sourceKind !== (input.mode === 'live' ? 'LIVE_EXECUTION_SOURCE' : 'PAPER_EXECUTION_MODEL')) issue('execution_source_environment_mismatch');
  if (proof.nativeAccountId !== input.nativeAccountId || ['accountSourceId', 'observationId', 'sequence', 'controlRevision', 'currency'].some(key => proof[key as keyof PmExecutionEvidence] !== basis[key as keyof AccountBasis])) issue('execution_source_account_basis_mismatch');
  if (BigInt(proof.availableAtNs) > BigInt(basis.availableAtNs)) issue('execution_source_not_original_capture');
  result.validUntilNs = [input.nativeTargetValidUntilNs, proof.validUntilNs, input.liquidityValidUntilNs].reduce((a, b) => BigInt(a) < BigInt(b) ? a : b);
  if (BigInt(result.validUntilNs) <= BigInt(input.evaluatedAtNs)) issue('execution_original_deadline_expired');
  unique(input.targets.map(row => row.instrumentId), 'targets'); unique(input.liquidity.map(row => row.instrumentId), 'liquidity');
  unique(input.modeledCostRates.map(row => row.instrumentId), 'modeledCostRates');
  for (const row of input.modeledCostRates) { identity(row.instrumentId, 'modeledCostRates.instrumentId'); nonnegative(row.rate, 'modeledCostRates.rate'); }
  if (input.positions === null) issue('execution_complete_current_inventory_missing', true);
  else {
    unique(input.positions.map(row => row.instrumentId), 'positions');
    for (const row of input.positions) {
      identity(row.instrumentId, 'positions.instrumentId'); nonnegative(row.currentNotional.amount, 'positions.currentNotional');
      if (row.currentNotional.currency !== basis.currency) issue(`execution_inventory_currency_mismatch:${row.instrumentId}`);
      if (compareDecimal(row.currentNotional.amount, '0') !== 0 && !input.targets.some(target => target.instrumentId === row.instrumentId)) issue(`execution_inventory_target_omitted:${row.instrumentId}`);
    }
  }
  for (const row of input.liquidity) {
    identity(row.instrumentId, 'liquidity.instrumentId'); identity(row.currency, 'liquidity.currency'); clock(row.eventNs, 'liquidity.eventNs'); clock(row.availableNs, 'liquidity.availableNs');
    for (const key of ['notional', 'lotNotional', 'minimumNotional', 'takerFee'] as const) nonnegative(row[key], `liquidity.${key}`);
    if (BigInt(row.eventNs) > BigInt(row.availableNs) || BigInt(row.availableNs) > BigInt(basis.availableAtNs) || row.currency !== basis.currency) issue(`execution_liquidity_basis_mismatch:${row.instrumentId}`);
  }
  for (const rawTarget of input.targets) {
    const target = validatePmEntity('AssetTarget', rawTarget); const id = target.instrumentId;
    const evidence = proof.instruments.find(row => row.instrumentId === id);
    const liquidity = input.liquidity.find(row => row.instrumentId === id);
    const model = input.modeledCostRates.find(row => row.instrumentId === id);
    nonnegative(target.targetNotional.amount, `targets.${id}`);
    const item: PmExecutionQualification['instruments'][number] = { instrumentId: id, allInCostRate: null, estimatedTradingCost: null }; result.instruments.push(item);
    if (target.targetNotional.currency !== basis.currency) issue(`execution_target_currency_mismatch:${id}`);
    if (target.currentNotional === null || target.netTradeNotional === null) { issue(`execution_current_trade_missing:${id}`, true); continue; }
    if (input.positions !== null) {
      const position = input.positions.find(row => row.instrumentId === id);
      if (compareDecimal(target.currentNotional.amount, position?.currentNotional.amount ?? '0') !== 0) issue(`execution_current_inventory_mismatch:${id}`);
    }
    if (!evidence || !liquidity || !model) { issue(`execution_instrument_evidence_missing:${id}`, true); continue; }
    result.evidenceRefs.push(evidence.rulesRef, evidence.costs.evidenceRef, evidence.entry.evidenceRef, evidence.exit.evidenceRef);
    const costs = [evidence.costs.commissionRate, evidence.costs.slippageRate, evidence.costs.impactRate];
    const demand = target.netTradeNotional.amount;
    const absoluteDemand = compareDecimal(demand, '0') < 0 ? multiplyDecimal(demand, '-1') : demand;
    if (costs.some(value => value === null)) issue(`execution_cost_component_unknown:${id}`, true);
    else {
      item.allInCostRate = (costs as string[]).reduce(addDecimal, '0');
      item.estimatedTradingCost = { amount: multiplyDecimal(absoluteDemand, item.allInCostRate), currency: basis.currency };
      if (compareDecimal(model.rate, item.allInCostRate) !== 0) issue(`execution_native_cost_model_mismatch:${id}`);
      if (compareDecimal(evidence.costs.commissionRate!, liquidity.takerFee) < 0) issue(`execution_commission_bound_below_source_fee:${id}`);
    }
    const limit = compareDecimal(demand, '0') < 0 ? evidence.entry.maximumSellNotional : evidence.entry.maximumBuyNotional;
    if (compareDecimal(absoluteDemand, limit) > 0) issue(`execution_entry_limit_exceeded:${id}`);
    if (compareDecimal(target.targetNotional.amount, evidence.exit.maximumNotional) > 0) issue(`execution_exit_limit_exceeded:${id}`);
  }
  result.evidenceRefs = [...new Set(result.evidenceRefs)]; result.reasonCodes = [...new Set(result.reasonCodes)];
  if (result.outcome === 'READY') result.reasonCodes.push('original_execution_source_bounds_cover_current_target');
  return result;
}
