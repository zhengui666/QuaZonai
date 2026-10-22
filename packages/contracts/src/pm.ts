/** PM domain contracts. Existing numeric research/runtime APIs retain their original identities. */
import type { Json, ModelRef } from './index.ts';

export type Id = string;
export type Decimal = string;
export type Ns = string;
export type Currency = string;
export type Direction = 'MAXIMIZE' | 'MINIMIZE' | 'DIAGNOSTIC';
export type ClockPrecision = 'SECOND' | 'MILLISECOND' | 'MICROSECOND' | 'NANOSECOND' | 'UNKNOWN';
export type EvidenceRole = 'TRAINING' | 'INNER_VALIDATION' | 'SELECTION'
  | 'INDEPENDENT' | 'PREQUENTIAL' | 'OBSERVED' | 'CONTROL';
export type Availability = 'AVAILABLE' | 'INSUFFICIENT' | 'UNAVAILABLE' | 'STALE'
  | 'INCOMPARABLE' | 'AMBIGUOUS' | 'INVALID';
export type FactOrigin = 'SOURCE' | 'ACCOUNT' | 'OWNER_INTENT' | 'ESTIMATED'
  | 'DERIVED' | 'METHOD_POLICY';
export interface Interval { startNs: Ns; endExclusiveNs: Ns }
export interface RuntimeRef { conversationId: number; taskId: number | null }
export interface VersionedFact {
  schemaVersion: 1;
  id: Id;
  revision: string;
  supersedesId: Id | null;
  createdAtNs: Ns;
  producerVersion: string;
}
export interface Money { amount: Decimal; currency: Currency }
export interface EvidenceRef {
  id: Id;
  sourceKind: 'NATIVE_RESULT' | 'DATASET' | 'MODEL' | 'ACCOUNT' | 'DERIVED';
  role: EvidenceRole;
  runtime: RuntimeRef | null;
  objectKey: string;
  contentDigest: string | null;
  mediaType: string;
  sourceRevision: string;
  eventInterval: Interval | null;
  availableAtNs: Ns;
  clockPrecision: ClockPrecision;
  schemaId: string;
  visibility: 'RESEARCH_VISIBLE' | 'OWNER_ONLY';
}
export interface EstimateInterval {
  lower: number;
  upper: number;
  coverageProbability: number;
  methodRef: Id;
  effectiveSampleSize: number | null;
}
export interface MetricDefinition {
  id: Id;
  key: string;
  definitionVersion: string;
  label: string;
  unit: 'RETURN_RATIO' | 'LOSS_RATIO' | 'RATIO' | 'COUNT' | 'SECONDS'
    | 'CURRENCY' | 'NATIVE_QUANTITY' | 'DIMENSIONLESS';
  valueEncoding: 'FLOAT64' | 'DECIMAL' | 'INTEGER_STRING';
  direction: Direction;
  aggregation: string;
  calendarId: Id | null;
  annualizationDays: number | null;
  horizonRef: Id | null;
  requiredInputs: string[];
}
export interface MetricObservation {
  definition: MetricDefinition;
  status: Availability;
  value: number | Decimal | null;
  currency: Currency | null;
  reasonCodes: string[];
  role: EvidenceRole;
  interval: Interval;
  asOfNs: Ns;
  availableAtNs: Ns;
  origin: FactOrigin;
  methodRef: Id | null;
  evidenceRefs: Id[];
  sampleCount: string | null;
  expectedCount: string | null;
  uncertainty: EstimateInterval | null;
  raw: { nativeKey: string; value: number | Decimal | null;
    unit: string; conversionVersion: string | null } | null;
}

export interface Objective {
  metricKey: string;
  metricDefinitionId: Id;
  direction: 'MAXIMIZE' | 'MINIMIZE';
  horizonRef: Id;
  scope: 'ACCOUNT' | 'FORECAST' | 'RESEARCH';
}
export interface OwnerBoundary {
  id: Id;
  kind: 'LOSS' | 'LEVERAGE' | 'MARKET_SCOPE' | 'LIQUIDITY_NEED'
    | 'CAPITAL_COMMITMENT' | 'DATA_USE';
  value: Json;
  sourceStatement: string;
  effectiveFromNs: Ns;
}
export interface SelectionRule {
  mode: 'LEXICOGRAPHIC' | 'EXPECTED_UTILITY' | 'FRONTIER_ONLY';
  objectives: Objective[];
  utilityModelRef: Id | null;
  establishedBy: 'OWNER_GOAL' | 'ACCEPTED_PROFILE';
  rationale: string;
}
export interface MandateRevision extends VersionedFact {
  ownerGoal: string;
  interpretation: string;
  baseCurrency: Currency;
  capitalSource: { kind: 'ACCOUNT'; accountBindingId: Id }
    | { kind: 'RESEARCH_SCENARIO'; capital: Money };
  allowedUniverseRef: Id;
  ownerBoundaries: OwnerBoundary[];
  selection: SelectionRule;
  operatingAuthority: 'RESEARCH_ONLY' | 'SHADOW_TARGETS' | 'AUTHORIZED_TARGETS';
  researchBudgetRef: Id;
  unresolvedIntent: string[];
}
export interface DecisionPolicyRevision extends VersionedFact {
  mandateRevisionId: Id;
  methodFamily: string;
  nativeImplementationVersion: string;
  learnedParametersRef: Id | null;
  trainingProtocolRef: Id;
  validationEvidenceRefs: Id[];
  applicableCapabilityIds: Id[];
  validDomain: Json;
  numericalToleranceRef: Id;
  selectionRule: SelectionRule;
  limitations: string[];
}

export type StrategyArchetype = 'PREDICTIVE_ALPHA' | 'RELATIVE_VALUE' | 'EVENT_DRIVEN'
  | 'MICROSTRUCTURE' | 'PORTFOLIO_OVERLAY' | 'FACTOR_RESEARCH';
export type SupportState = 'SUPPORTED' | 'CONDITIONAL' | 'UNSUPPORTED';
export interface CapabilityDescriptor extends VersionedFact {
  provider: string;
  providerVersion: string;
  strategyArchetypes: StrategyArchetype[];
  dataKinds: string[];
  predictionUnits: string[];
  accountingModels: string[];
  executionModels: string[];
  supportedOperations: string[];
  operationContracts: OperationContract[];
  boundaries: { operation: string; state: SupportState;
    requiredEvidence: string[]; reasonCodes: string[] }[];
  nativeSchemaIds: string[];
}
export interface OperationContract {
  name: string;
  inputSchemaId: string;
  outputSchemaId: string;
  requiredCapabilityIds: Id[];
  effect: 'PURE' | 'NATIVE_COMPUTE' | 'EXTERNAL_EFFECT';
  replayPolicy: 'DETERMINISTIC' | 'REQUEST_BOUND' | 'RECONCILE_REQUIRED';
  incompatibleCapabilityIds: Id[];
}
export interface StrategyDefinition extends VersionedFact {
  name: string;
  archetype: StrategyArchetype;
  economicMechanism: string;
  falsificationConditions: string[];
  opportunityUniverseRef: Id;
  requiredCapabilities: string[];
  intendedHoldingHorizonRef: Id;
  knownFailureModes: string[];
}
export interface StrategyVersion extends VersionedFact {
  definitionId: Id;
  mandateRevisionId: Id;
  policyRevisionId: Id;
  modelRefs: Id[];
  featureSchemaRef: Id;
  signalContractRef: Id | null;
  stateSchemaRef: Id | null;
  entryRuleRef: Id | null;
  holdingRuleRef: Id | null;
  invalidationRuleRef: Id | null;
  portfolioRuleRef: Id | null;
  executionRuleRef: Id | null;
  exitRuleRef: Id | null;
  componentStrategyVersionIds: Id[];
  capabilityBindingIds: Id[];
  fittedEvidenceRefs: Id[];
  pipelineAssessmentId: Id;
}
export type PipelineStage = 'IDEA' | 'RESEARCH' | 'SELECTION' | 'INDEPENDENT_EVALUATION'
  | 'SHADOW' | 'PAPER' | 'TARGET_OPERATION' | 'MONITORING' | 'RETIRED';
export interface PipelineAssessment extends VersionedFact {
  strategyVersionId: Id;
  stage: PipelineStage;
  outcome: 'READY' | 'NOT_READY' | 'NEEDS_EVIDENCE' | 'SUPERSEDED' | 'RETIRED';
  evaluatedAtNs: Ns;
  requirementsVersion: string;
  evidenceRefs: Id[];
  reasonCodes: string[];
  nextPermittedActions: string[];
}

export interface DatasetRevision extends VersionedFact {
  datasetId: Id;
  sourceId: Id;
  sourceRevision: string;
  parentRevisionId: Id | null;
  contentRef: Id;
  eventInterval: Interval;
  availableThroughNs: Ns;
  admittedAtNs: Ns;
  sourceClockPrecision: ClockPrecision;
  universeSnapshotRef: Id;
  instrumentRulesRef: Id;
  schemaId: string;
  qualityEvidenceRefs: Id[];
  qualityStatus: Availability;
  missingCapabilities: string[];
  revisionKind: 'APPEND' | 'CORRECTION' | 'RETRACTION';
}
export interface LabelDefinition {
  id: Id;
  targetKind: 'FORWARD_RETURN' | 'EVENT_OUTCOME' | 'EXECUTION_OUTCOME';
  horizonKind: 'OBSERVATION' | 'CLOCK_TIME' | 'EVENT';
  horizonValue: string;
  unit: string;
  returnBasis: 'GROSS_PRICE_RETURN' | 'NET_ACCOUNT_RETURN' | 'NOT_APPLICABLE';
  priceBasis: 'TRADE' | 'BID' | 'ASK' | 'MID' | 'SETTLEMENT' | 'NOT_APPLICABLE';
  priceAndCashflowRuleRef: Id;
  availabilityRuleRef: Id;
}
export interface PredictionRow {
  instrumentId: string;
  eventAtNs: Ns;
  decisionAtNs: Ns;
  featureAvailableAtNs: Ns;
  factorVersionId: Id;
  prediction: number | null;
  status: Availability;
  labelDefinitionId: Id;
  labelEndNs: Ns | null;
  labelAvailableAtNs: Ns | null;
  realizedValue: number | null;
  foldId: Id;
}
export interface ResearchBudget extends VersionedFact {
  maximumTrials: number;
  nativeComputeSeconds: number | null;
  modelSpend: Money | null;
  dataBytes: string | null;
  reservationForClosureRef: Id;
  usageEvidenceRefs: Id[];
}
export type CampaignState = 'PLANNED' | 'SEARCHING' | 'CLOSING'
  | 'EVALUATING_INDEPENDENT' | 'CONCLUDED' | 'PAUSED';
export interface ResearchCampaign extends VersionedFact {
  mandateRevisionId: Id;
  strategyDefinitionId: Id;
  state: CampaignState;
  runtime: RuntimeRef;
  datasetRevisionIds: Id[];
  evaluationProtocolRef: Id;
  budgetId: Id;
  hypothesisIds: Id[];
  trialIds: Id[];
  latestComparisonId: Id | null;
  selectedStrategyVersionId: Id | null;
  conclusionReason: string | null;
}
export interface Hypothesis extends VersionedFact {
  campaignId: Id;
  economicFamilyId: Id;
  parentHypothesisId: Id | null;
  mechanism: string;
  prediction: string;
  falsification: string;
  requiredDataCapabilities: string[];
  incrementalQuestion: string;
  state: 'PROPOSED' | 'TESTING' | 'SUPPORTED' | 'REFUTED' | 'INCONCLUSIVE' | 'RETIRED';
  evidenceRefs: Id[];
}
export type TrialState = 'ADMITTED' | 'RUNNING' | 'COMPUTED' | 'FAILED' | 'REJECTED';
export interface ResearchTrial extends VersionedFact {
  campaignId: Id;
  hypothesisId: Id;
  parentTrialIds: Id[];
  runtime: RuntimeRef;
  state: TrialState;
  purpose: 'FEASIBILITY' | 'CALIBRATION' | 'ENSEMBLE_FIT' | 'JOINT_REPLAY'
    | 'CAPITAL_SCALE' | 'ABLATION' | 'ROBUSTNESS';
  proposalRef: Id;
  datasetRevisionIds: Id[];
  protocolRef: Id;
  methodVersion: string;
  countedAttempt: boolean;
  resourceUsageRef: Id | null;
  evidenceRefs: Id[];
  errorCode: string | null;
}
export interface ComparisonScope {
  id: Id;
  allowedUniverseRef: Id;
  datasetRevisionIds: Id[];
  interval: Interval;
  role: EvidenceRole;
  calendarId: Id;
  labelDefinitionId: Id | null;
  accountingPolicyRef: Id | null;
  costModelRef: Id | null;
  startingStateRef: Id | null;
  capital: Money | null;
}
export interface CandidateScorecard {
  trialId: Id;
  comparisonScopeId: Id | null;
  status: Availability;
  metrics: MetricObservation[];
  frontierMembership: 'MEMBER' | 'DOMINATED' | 'UNDETERMINED';
  dominatedByTrialIds: Id[];
  missingEvidence: string[];
  baselineComparisonRef: Id | null;
}
export interface CandidateComparison extends VersionedFact {
  campaignId: Id;
  policyRevisionId: Id;
  scopes: ComparisonScope[];
  candidates: CandidateScorecard[];
  objectiveRankings: { scopeId: Id; metricKey: string; metricDefinitionId: Id; ties: Id[][] }[];
  recommendedTrialId: Id | null;
  recommendationStatus: 'SELECTED' | 'FRONTIER_ONLY' | 'NO_VIABLE_CANDIDATE' | 'NEEDS_EVIDENCE';
  reasonCodes: string[];
  evidenceRefs: Id[];
}
/** Predicted return and variance over the original label horizon, never daily
 * account performance. Objective is lambda * variance + linear cost - gross return.
 */
export interface ModeledOperatingOutcome {
  grossExpectedReturn: number;
  linearTurnoverCost: number;
  netExpectedReturn: number;
  variance: number;
  objectiveValue: number;
}
export interface ModeledOperatingComparison {
  method: 'MATCHED_LINEAR_RETURN_VARIANCE_V1' | 'MATCHED_ROUNDED_LINEAR_RETURN_VARIANCE_V1';
  basis: 'FORWARD_MODEL_ESTIMATE';
  forecastUnit: 'RETURN_PER_HORIZON';
  methodPolicyRef: Id;
  horizonRef: Id;
  /** Registered numerical method parameter; it is not an inferred owner preference. */
  riskAversion: Decimal;
  nativeEvidenceRef: Id;
  startingStateRef: Id;
  /** Model decision cutoff, not the economic clock of the original snapshot. */
  asOfNs: Ns;
  /** Maximum actual availability of consumed source inputs; not computation time. */
  availableAtNs: Ns;
  hold: ModeledOperatingOutcome;
  proposed: ModeledOperatingOutcome | null;
  targetBlockReason: string | null;
  limitations: string[];
}
export interface OperatingComparison extends VersionedFact {
  cycleId: Id;
  basisRevision: string;
  policyRevisionId: Id;
  scope: ComparisonScope;
  incumbentDecisionId: Id | null;
  evaluationContext: 'HISTORICAL_JOINT_REPLAY' | 'FORWARD_MODEL_ESTIMATE' | 'OBSERVED_FORWARD';
  candidates: {
    id: Id;
    kind: 'HOLD_CURRENT' | 'CASH' | 'REBALANCE' | 'REDUCE';
    configurationEvidenceRef: Id;
    /** An explicit native constraint proof; execution failure never establishes infeasibility. */
    feasibility?: { status: 'FEASIBLE' | 'INFEASIBLE' | 'UNKNOWN'; evidenceRefs: Id[]; reasonCodes: string[] };
    status: Availability;
    transitionCost: MetricObservation[];
    netOutcome: MetricObservation[];
    downside: MetricObservation[];
    capacityEstimateIds: Id[];
    reasonCodes: string[];
  }[];
  pairedDifferenceEvidenceRefs: Id[];
  selectedCandidateId: Id | null;
  conclusion: 'SELECTED' | 'UNRESOLVED_TRADEOFF' | 'NEEDS_EVIDENCE' | 'NO_FEASIBLE_SOLUTION';
  reasonCodes: string[];
  modeledComparison?: ModeledOperatingComparison;
}

export interface FactorVersion extends VersionedFact {
  economicFamilyId: Id;
  modelRef: Id;
  featureSchemaRef: Id;
  labelDefinitionId: Id;
  outputUnit: string;
  trainingEvidenceRefs: Id[];
  validationEvidenceRefs: Id[];
  supportedCapabilityIds: Id[];
  status: 'RESEARCH' | 'ELIGIBLE' | 'DEGRADED' | 'RETIRED';
}
export interface CalibrationModel extends VersionedFact {
  factorVersionId: Id;
  inputUnit: string;
  outputUnit: string;
  labelDefinitionId: Id;
  trainingInterval: Interval;
  trainingCutoffNs: Ns;
  fitProtocolRef: Id;
  modelRef: Id;
  fitEvidenceRefs: Id[];
}
export interface FactorWeight {
  factorVersionId: Id;
  calibrationModelId: Id | null;
  weight: Decimal;
  economicFamilyId: Id;
  contributionEvidenceRef: Id | null;
}
export interface EnsembleRevision extends VersionedFact {
  strategyVersionId: Id;
  policyRevisionId: Id;
  members: FactorWeight[];
  predictionUnit: string;
  labelDefinitionId: Id;
  fitProtocolRef: Id;
  fitCutoffNs: Ns;
  trainingEvidenceRefs: Id[];
  jointValidationEvidenceRefs: Id[];
  qualification: 'CANDIDATE' | 'SELECTED' | 'INDEPENDENTLY_EVALUATED' | 'RETIRED';
  rejectionReasonCodes: string[];
}

export type CapacityLevel = 'OBSERVED_LIQUIDITY' | 'CONDITIONAL_FEASIBILITY' | 'ECONOMIC';
export interface CapacityPoint {
  capital: Money;
  status: Availability;
  replayEvidenceRef: Id | null;
  strategyVersionId: Id;
  portfolioDecisionRef: Id | null;
  netPerformance: MetricObservation[];
  entryDemand: Money | null;
  exitDemand: Money | null;
  estimatedCost: Money | null;
  fillRatio: number | null;
  bindingConstraints: string[];
  reasonCodes: string[];
}
export interface CapacityEstimate extends VersionedFact {
  strategyVersionId: Id;
  ensembleRevisionId: Id | null;
  basisRevision: string;
  level: CapacityLevel;
  status: Availability;
  currency: Currency;
  observedInterval: Interval;
  asOfNs: Ns;
  validUntilNs: Ns | null;
  minimumTestedCapital: Money | null;
  maximumTestedCapital: Money | null;
  supportedCapitalUpperBound: Money | null;
  feasibleRegions: { lower: Money; upper: Money;
    kind: 'TESTED_POINT' | 'VALIDATED_INTERVAL';
    configurationEvidenceRef: Id; evidenceRef: Id }[];
  curve: CapacityPoint[];
  liquidityModelRef: Id | null;
  impactModelRef: Id | null;
  executionModelRef: Id;
  exitModelRef: Id | null;
  evidenceRefs: Id[];
  assumptions: string[];
  missingEvidence: string[];
}
export interface AccountBasis {
  accountBindingId: Id;
  accountSourceId: string;
  observationId: string;
  sequence: string;
  observedAtNs: Ns;
  availableAtNs: Ns;
  controlRevision: string;
  currency: Currency;
  /** Original source equity; a derived inventory value never overwrites it. */
  equity: Money | null;
  valuation?: {
    origin: 'DERIVED';
    /** Omission retains the original snapshot-time contract. Forward valuation
     * revalues the original quantities without rewriting their source clock. */
    timeBasis?: 'SNAPSHOT_TIME' | 'DECISION_CUTOFF';
    priceEvidenceRef?: Id;
    reportValue: Money;
    valuationEvidenceRef: Id;
    methodRef: Id;
    asOfNs: Ns;
    /** Maximum input availability. For DECISION_CUTOFF this is <= asOfNs;
     * persisted evidence createdAtNs records later report production separately. */
    availableAtNs: Ns;
  };
  reserved: Money | null;
  positionsEvidenceRef: Id;
  outstandingCommandsRef: Id;
}
export interface CapitalPlan {
  equity: Money;
  reserved: Money;
  ownerLiquidityReserve: Money;
  deployableBudget: Money;
  committedCapital: Money;
  uncommittedCash: Money;
  projectedTradingCash: Money;
  capacityEstimateIds: Id[];
  sizingEvidenceRef: Id;
}
export interface AssetTarget {
  instrumentId: string;
  navWeight: Decimal;
  targetQuantity: Decimal | null;
  targetNotional: Money;
  currentNotional: Money | null;
  netTradeNotional: Money | null;
  /** Original native lot-rounded estimate at retained marks, never a fill or
   * realized position. The continuous model target above remains unchanged. */
  executionEstimate?: {
    resultingQuantity: Decimal;
    resultingNotional: Money;
    netTradeQuantity: Decimal;
    netTradeNotional: Money;
    evidenceRef: Id;
  };
  quantityRuleRef: Id;
  riskContribution: MetricObservation | null;
}
export interface StrategyAllocation {
  strategyVersionId: Id;
  ensembleRevisionId: Id | null;
  committedCapital: Money;
  /** Exact decimal convenience only. Null also permits a nonterminating ratio; amounts remain authoritative. */
  capitalFraction: Decimal | null;
  allocationEvidenceRef: Id;
  riskContributionEvidenceRef: Id | null;
  nettingGroupId: Id;
}
export type DecisionAction = 'HOLD' | 'REBALANCE' | 'REDUCE' | 'CASH'
  | 'NEEDS_EVIDENCE' | 'NO_FEASIBLE_SOLUTION';
export interface PortfolioDecision extends VersionedFact {
  cycleId: Id;
  mandateRevisionId: Id;
  strategyVersionId: Id;
  ensembleRevisionId: Id | null;
  strategyAllocations: StrategyAllocation[];
  policyRevisionId: Id;
  basisRevision: string;
  incumbentDecisionId: Id | null;
  accountBasis: AccountBasis | null;
  researchStartingStateRef: Id | null;
  action: DecisionAction;
  status: 'CANDIDATE' | 'CURRENT' | 'SUPERSEDED' | 'EXPIRED' | 'REVOKED';
  assetTargets: AssetTarget[];
  capital: CapitalPlan | null;
  comparisonRef: Id;
  solverEvidenceRef: Id | null;
  jointReplayEvidenceRef: Id | null;
  reasonCodes: string[];
  explanationFacts: Json;
  validUntilNs: Ns | null;
  nextInvalidationDependencies: Id[];
  targetCommandId: Id | null;
}
export type CycleState = 'PENDING' | 'CAPTURING' | 'COMPUTING' | 'EVALUATING'
  | 'CONCLUDED' | 'SUPERSEDED' | 'WAITING_EVIDENCE' | 'FAILED';
export interface DecisionCycle extends VersionedFact {
  mandateRevisionId: Id;
  runtime: RuntimeRef;
  state: CycleState;
  triggerEventIds: Id[];
  basisRevision: string;
  datasetRevisionIds: Id[];
  accountBasis: AccountBasis | null;
  policyRevisionId: Id;
  strategyVersionId: Id;
  asOfNs: Ns;
  availableCutoffNs: Ns;
  dependencyRevisionRefs: Id[];
  outputDecisionId: Id | null;
  supersededByCycleId: Id | null;
  reasonCodes: string[];
}
export interface ObservedOutcome extends VersionedFact {
  decisionId: Id;
  targetCommandId: Id | null;
  accountBasis: AccountBasis;
  interval: Interval;
  availableAtNs: Ns;
  receiptState: 'NOT_OFFERED' | 'PENDING' | 'ACKNOWLEDGED' | 'REJECTED' | 'UNCERTAIN';
  reconciliationStatus: 'MATCHED' | 'PARTIAL' | 'UNMATCHED';
  realizedMetrics: MetricObservation[];
  attributionEvidenceRef: Id | null;
  matureLabelEvidenceRef: Id | null;
  reasonCodes: string[];
}
export type DecisionTrigger = 'DATA_REVISION' | 'LABEL_MATURED' | 'ACCOUNT_CHANGED'
  | 'COST_OR_RULE_CHANGED' | 'RESEARCH_EVIDENCE' | 'EVIDENCE_INVALIDATED'
  | 'MANDATE_CHANGED' | 'FRESHNESS_DEADLINE';
export interface BusinessEvent {
  id: Id;
  kind: DecisionTrigger;
  sourceRevisionRef: Id;
  availableAtNs: Ns;
  affectedMandateIds: Id[];
  supersedesEventId: Id | null;
}
export interface TargetCommand extends VersionedFact {
  decisionId: Id;
  idempotencyKey: string;
  accountBindingId: Id;
  basisRevision: string;
  controlRevision: string;
  payloadEvidenceRef: Id;
  validUntilNs: Ns;
  state: 'NOT_OFFERED' | 'PENDING' | 'ACKNOWLEDGED' | 'REJECTED' | 'UNCERTAIN';
  externalReceiptRef: Id | null;
}
export interface EvidenceGap extends VersionedFact {
  affectedEntityId: Id;
  requiredCapability: string;
  reasonCode: string;
  permittedRemedy: 'WAIT_LABEL' | 'RECOMPUTE' | 'REFRESH_SOURCE'
    | 'COLLECT_AUTHORIZED_DATA' | 'CLARIFY_INTENT' | 'UNSUPPORTED';
  dataAuthorizationRef: Id | null;
  state: 'OPEN' | 'IN_PROGRESS' | 'RESOLVED' | 'UNRESOLVABLE';
  resolutionEvidenceRefs: Id[];
}
export interface EvaluationProtocol extends VersionedFact {
  campaignId: Id;
  searchFamilyId: Id;
  labelDefinitionId: Id;
  folds: { id: Id; train: Interval; select: Interval;
    independent: Interval | null; purgeRuleRef: Id; embargoRuleRef: Id | null }[];
  comparisonScopeIds: Id[];
  methodSelectionRuleRef: Id;
  statisticalDiagnostics: string[];
  requiredCapabilities: string[];
  stoppingRuleRef: Id;
  intendedUse: 'FACTOR_EVIDENCE' | 'STRATEGY_SELECTION' | 'ADAPTIVE_POLICY_EVALUATION';
}
export interface SearchFamily extends VersionedFact {
  economicFamilyId: Id;
  campaignIds: Id[];
  trialIds: Id[];
  methodVariantRefs: Id[];
  countedAttempts: string;
  failedAttempts: string;
  costEvidenceRefs: Id[];
  selectionExposureRefs: Id[];
  effectiveTrialEstimate: MetricObservation | null;
}
export interface ExposureRecord extends VersionedFact {
  evidenceRef: Id;
  roleAtExposure: EvidenceRole;
  campaignId: Id | null;
  policyRevisionId: Id | null;
  exposedAtNs: Ns;
  consumer: 'RESEARCH_AGENT' | 'SELECTION_METHOD' | 'INDEPENDENT_EVALUATOR' | 'OWNER';
  allowedUse: 'FIT' | 'SELECT' | 'EVALUATE_ONCE' | 'OBSERVE';
  resultingEvidenceRefs: Id[];
}
export interface AdaptiveUpdateProtocol extends VersionedFact {
  strategyVersionId: Id;
  eligibleTriggers: DecisionTrigger[];
  maturityRuleRef: Id;
  updateMethodRef: Id;
  trainingWindowRuleRef: Id;
  modelSelectionProtocolRef: Id;
  riskAndCostRefreshRuleRef: Id;
  evidenceFreshnessRuleRef: Id;
  resourceBudgetRef: Id;
  wholeProcessValidationRefs: Id[];
  allowedParameterDomain: Json;
  outOfDomainAction: 'KEEP_VALID_INCUMBENT' | 'REDUCE_WITH_EVIDENCE' | 'WAIT_EVIDENCE';
}
export interface RiskModel extends VersionedFact {
  method: 'SAMPLE_COVARIANCE' | 'SCENARIO_CVAR' | 'VALIDATED_EXTENSION';
  labelDefinitionId: Id;
  assetUniverseRef: Id;
  trainingInterval: Interval;
  cutoffNs: Ns;
  matrixOrScenarioRef: Id;
  sampleCoverageRef: Id;
  numericalDiagnosticsRef: Id;
  validationEvidenceRefs: Id[];
  status: Availability;
  reasonCodes: string[];
}
export interface CostModel extends VersionedFact {
  currency: Currency;
  horizonRef: Id;
  basis: 'VENUE_RULES' | 'OBSERVED_EXECUTION' | 'CONDITIONAL_SIMULATION';
  feeRuleRef: Id;
  spreadModelRef: Id | null;
  impactModelRef: Id | null;
  latencyModelRef: Id | null;
  exitModelRef: Id | null;
  calibrationEvidenceRefs: Id[];
  validationEvidenceRefs: Id[];
  applicableScale: { minimum: Money; maximum: Money } | null;
  status: Availability;
  missingComponents: string[];
}
export interface ResearchWorkAllocation extends VersionedFact {
  campaignId: Id;
  budgetId: Id;
  consideredHypothesisIds: Id[];
  selectedHypothesisIds: Id[];
  nextExperimentPurpose: ResearchTrial['purpose'] | null;
  expectedInformationEvidenceRef: Id | null;
  resourceEstimateRef: Id;
  incumbentComparisonRef: Id | null;
  reasonCodes: string[];
  action: 'EXPERIMENT' | 'WAIT_DATA' | 'CLOSE_SEARCH' | 'RETIRE_HYPOTHESIS';
}
export interface BudgetMovement extends VersionedFact {
  budgetId: Id;
  campaignId: Id;
  hypothesisId: Id | null;
  trialId: Id | null;
  kind: 'RESERVE' | 'CONSUME' | 'RELEASE' | 'TRANSFER';
  resource: 'TRIAL' | 'NATIVE_SECONDS' | 'MODEL_COST' | 'DATA_BYTES';
  amount: Decimal;
  currency: Currency | null;
  fromReservationId: Id | null;
  toReservationId: Id | null;
  measurementEvidenceRef: Id | null;
  reasonCode: string;
}
export interface FeatureField {
  name: string;
  dataType: 'FLOAT64' | 'DECIMAL' | 'INTEGER_STRING' | 'BOOLEAN' | 'CATEGORY';
  unit: string;
  sourceFieldRef: Id;
  transformationRef: Id | null;
  availabilityRuleRef: Id;
  missingValuePolicy: 'UNAVAILABLE' | 'EXPLICIT_MASK';
}
export interface FeatureSchema extends VersionedFact {
  fields: FeatureField[];
  instrumentIdentitySchemaId: string;
  eventClockField: string;
  availableClockField: string;
  warmupRuleRef: Id;
  fitParameterEvidenceRefs: Id[];
}
export interface SignalContract extends VersionedFact {
  kind: 'RETURN_FORECAST' | 'RANK_SCORE' | 'EVENT_PROBABILITY' | 'RELATIVE_VALUE'
    | 'ORDER_INTENT' | 'TARGET_INTENT';
  unit: string;
  labelDefinitionId: Id | null;
  valueSchemaId: string;
  horizonRef: Id;
  confidenceEvidenceRequired: boolean;
  validityRuleRef: Id;
  requiredCapabilities: string[];
}
export interface StrategyStateContract extends VersionedFact {
  stateSchemaId: string;
  initialStateRef: Id;
  eventSchemaIds: string[];
  transitionRuleRef: Id;
  entryRuleRef: Id | null;
  holdingRuleRef: Id | null;
  exitRuleRef: Id | null;
  invalidationRuleRef: Id;
  inventoryModelRef: Id | null;
  multiLegProtocolRef: Id | null;
  requiredCapabilities: string[];
}
export interface MarketAdapterContract extends VersionedFact {
  venueId: string;
  instrumentSchemaId: string;
  instrumentRulesRef: Id;
  calendarRef: Id;
  eventRelationsRef: Id | null;
  priceAndSettlementRuleRef: Id;
  currencyAndCollateralRuleRef: Id;
  feesRef: Id;
  corporateActionRuleRef: Id | null;
  borrowingAndMarginRuleRef: Id | null;
  sourceClockRuleRef: Id;
  capabilityDescriptorId: Id;
}
export interface OutcomeAttribution extends VersionedFact {
  observedOutcomeId: Id;
  decisionId: Id;
  scope: ComparisonScope;
  cashFlowNormalizationRef: Id;
  actualObservationRefs: Id[];
  counterfactualEvidenceRefs: Id[];
  components: {
    kind: 'FORECAST' | 'ALLOCATION' | 'CAPITAL' | 'FEES' | 'EXECUTION' | 'DATA';
    metric: MetricObservation;
    identification: 'EXACT_ACCOUNTING' | 'MATCHED_COUNTERFACTUAL' | 'UNIDENTIFIED';
    methodRef: Id | null;
  }[];
  aggregation: 'ADDITIVE_ACCOUNTING' | 'NON_ADDITIVE_COUNTERFACTUAL';
  residual: MetricObservation | null;
  limitations: string[];
}
export interface RetirementDisposition extends VersionedFact {
  strategyVersionId: Id;
  pipelineAssessmentId: Id;
  signalGeneration: 'STOPPED';
  positionDisposition: 'NONE' | 'MONITORING_EXISTING' | 'EXIT_TARGET_PENDING'
    | 'EXTERNAL_HANDOFF' | 'FLAT_CONFIRMED';
  residualPositionsEvidenceRef: Id | null;
  outstandingTargetCommandIds: Id[];
  exitDecisionId: Id | null;
  reconciliationRequired: boolean;
  closureEvidenceRefs: Id[];
}

export interface PmMandateStart {
  requestId: string;
  goal: string;
  datasetId: string;
  funding: MandateRevision['capitalSource'];
  model: ModelRef;
  maxTrials: number;
  operatingAuthority?: MandateRevision['operatingAuthority'];
  businessPreference?: 'NET_RETURN' | 'DOWNSIDE_FIRST' | 'FRONTIER';
}
export interface PmCommand { requestId: string; expectedRevision: string }
export interface PmMandateView {
  mandate: MandateRevision;
  activePolicy: DecisionPolicyRevision;
  currentDecision: PortfolioDecision | null;
  lastCycle: DecisionCycle | null;
  campaigns: ResearchCampaign[];
  researchTrials: ResearchTrial[];
  budgetMovements: BudgetMovement[];
  searchFamily: SearchFamily;
  strategies: StrategyVersion[];
  assessments: PipelineAssessment[];
  capacityEstimates: CapacityEstimate[];
  gaps: EvidenceGap[];
  history: PortfolioDecision[];
  cycles: DecisionCycle[];
  datasetRevisions: DatasetRevision[];
  capabilities: CapabilityDescriptor[];
  ensembles: EnsembleRevision[];
  comparisons: OperatingComparison[];
  metricDefinitions?: MetricDefinition[];
  outcomes: ObservedOutcome[];
  targetCommands: TargetCommand[];
  attributions: OutcomeAttribution[];
  retirements: RetirementDisposition[];
}

/** Runtime validation uses these entity names; this is not a second persistence layer. */
export interface PmEntities {
  PmCommand: PmCommand;
  PmMandateStart: PmMandateStart;
  PmMandateView: PmMandateView;
  Interval: Interval;
  RuntimeRef: RuntimeRef;
  VersionedFact: VersionedFact;
  Money: Money;
  EvidenceRef: EvidenceRef;
  EstimateInterval: EstimateInterval;
  MetricDefinition: MetricDefinition;
  MetricObservation: MetricObservation;
  Objective: Objective;
  OwnerBoundary: OwnerBoundary;
  SelectionRule: SelectionRule;
  MandateRevision: MandateRevision;
  DecisionPolicyRevision: DecisionPolicyRevision;
  CapabilityDescriptor: CapabilityDescriptor;
  OperationContract: OperationContract;
  StrategyDefinition: StrategyDefinition;
  StrategyVersion: StrategyVersion;
  PipelineAssessment: PipelineAssessment;
  DatasetRevision: DatasetRevision;
  LabelDefinition: LabelDefinition;
  PredictionRow: PredictionRow;
  ResearchBudget: ResearchBudget;
  ResearchCampaign: ResearchCampaign;
  Hypothesis: Hypothesis;
  ResearchTrial: ResearchTrial;
  ComparisonScope: ComparisonScope;
  CandidateScorecard: CandidateScorecard;
  CandidateComparison: CandidateComparison;
  ModeledOperatingOutcome: ModeledOperatingOutcome;
  ModeledOperatingComparison: ModeledOperatingComparison;
  OperatingComparison: OperatingComparison;
  FactorVersion: FactorVersion;
  CalibrationModel: CalibrationModel;
  FactorWeight: FactorWeight;
  EnsembleRevision: EnsembleRevision;
  CapacityPoint: CapacityPoint;
  CapacityEstimate: CapacityEstimate;
  AccountBasis: AccountBasis;
  CapitalPlan: CapitalPlan;
  AssetTarget: AssetTarget;
  StrategyAllocation: StrategyAllocation;
  PortfolioDecision: PortfolioDecision;
  DecisionCycle: DecisionCycle;
  ObservedOutcome: ObservedOutcome;
  BusinessEvent: BusinessEvent;
  TargetCommand: TargetCommand;
  EvidenceGap: EvidenceGap;
  EvaluationProtocol: EvaluationProtocol;
  SearchFamily: SearchFamily;
  ExposureRecord: ExposureRecord;
  AdaptiveUpdateProtocol: AdaptiveUpdateProtocol;
  RiskModel: RiskModel;
  CostModel: CostModel;
  ResearchWorkAllocation: ResearchWorkAllocation;
  BudgetMovement: BudgetMovement;
  FeatureField: FeatureField;
  FeatureSchema: FeatureSchema;
  SignalContract: SignalContract;
  StrategyStateContract: StrategyStateContract;
  MarketAdapterContract: MarketAdapterContract;
  OutcomeAttribution: OutcomeAttribution;
  RetirementDisposition: RetirementDisposition;
}
export type PmEntityName = keyof PmEntities;
