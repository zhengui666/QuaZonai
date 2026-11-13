import type { AccountBasis, AssetTarget, Decimal, Id, Money, Ns } from './pm.ts';

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
