# 规范字段合同：业务实体、状态、数据语义与 API

> 本文件是设计包的字段规范，不是已发布 API。所有新增合同为拟议。专题中的候选类型、函数名与本文件不一致时，以这里的语义与名称为准，并在实现前修订专题。以下 TypeScript 是自洽的领域 DTO 草案，不是要求立即添加到生产源码。

## 1. 通用规则

1. 新业务 ID 使用不可解释的字符串；官方 Pi `conversationId`、`taskId` 仍使用其 branded 安全整数，单独存放，不能通过转换数字生成新的业务身份。来源市场/事件/资产 ID 保留命名空间，不与 factor ID 混用。
2. 金额、数量、权重及精确费率使用十进制字符串。禁止经过 JS `Number` 计算金额再转回字符串。浮点统计仅接受有限值；`NaN`、`Infinity` 不进入 JSON。
3. 时钟使用非负整数 `Ns` 字符串，另记实际精度。把毫秒乘一百万不代表获得纳秒精度。所有业务区间采用 `[startNs, endExclusiveNs)`；有可用时钟才可证明 as-of 合法。
4. 不可用指标必须 `value:null`，同时给出状态和原因；未知不等于零、风险小、不合格或现金策略成功。`AVAILABLE` 与“有投资价值”独立。
5. 原生统计保留 `nativeKey`、原始值/单位和转换版本。规范最大回撤为非负损失幅度；当前 Nautilus 负数回撤经显式取负转换，不直接改变原证据。
6. 不可变对象产生新 ID/版本，不原位覆盖。活动指针、待处理修订和官方 task 引用是运行控制信息；不复制官方运行时任务状态。
7. 事实来源、方法推导、业主意愿、产品方法默认值分开记录。系统推导值也有假设、方法版本与有效域，不能包装成天然真值。
8. 大样本面板、模型、原生日志、回放与净值序列使用 `EvidenceRef` 引用；API 返回摘要和分页，不把它们内联到所有周期。

## 2. 可校验的核心类型

```ts
type Id = string;
type Decimal = string;
type Ns = string;
type Currency = string;
type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type Direction = 'MAXIMIZE' | 'MINIMIZE' | 'DIAGNOSTIC';
type ClockPrecision = 'SECOND' | 'MILLISECOND' | 'MICROSECOND' | 'NANOSECOND' | 'UNKNOWN';
type EvidenceRole = 'TRAINING' | 'INNER_VALIDATION' | 'SELECTION'
  | 'INDEPENDENT' | 'PREQUENTIAL' | 'OBSERVED' | 'CONTROL';
type Availability = 'AVAILABLE' | 'INSUFFICIENT' | 'UNAVAILABLE' | 'STALE'
  | 'INCOMPARABLE' | 'AMBIGUOUS' | 'INVALID';
type FactOrigin = 'SOURCE' | 'ACCOUNT' | 'OWNER_INTENT' | 'ESTIMATED'
  | 'DERIVED' | 'METHOD_POLICY';
interface Interval { startNs: Ns; endExclusiveNs: Ns }
interface RuntimeRef { conversationId: number; taskId: number | null }
interface VersionedFact {
  schemaVersion: 1;
  id: Id;
  revision: string;
  supersedesId: Id | null;
  createdAtNs: Ns;
  producerVersion: string;
}
interface Money { amount: Decimal; currency: Currency }
interface EvidenceRef {
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
interface EstimateInterval {
  lower: number;
  upper: number;
  coverageProbability: number;
  methodRef: Id;
  effectiveSampleSize: number | null;
}
interface MetricDefinition {
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
interface MetricObservation {
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

interface Objective {
  metricKey: string;
  metricDefinitionId: Id;
  direction: 'MAXIMIZE' | 'MINIMIZE';
  horizonRef: Id;
  scope: 'ACCOUNT' | 'FORECAST' | 'RESEARCH';
}
interface OwnerBoundary {
  id: Id;
  kind: 'LOSS' | 'LEVERAGE' | 'MARKET_SCOPE' | 'LIQUIDITY_NEED'
    | 'CAPITAL_COMMITMENT' | 'DATA_USE';
  value: Json;
  sourceStatement: string;
  effectiveFromNs: Ns;
}
interface SelectionRule {
  mode: 'LEXICOGRAPHIC' | 'EXPECTED_UTILITY' | 'FRONTIER_ONLY';
  objectives: Objective[];
  utilityModelRef: Id | null;
  establishedBy: 'OWNER_GOAL' | 'ACCEPTED_PROFILE';
  rationale: string;
}
interface MandateRevision extends VersionedFact {
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
interface DecisionPolicyRevision extends VersionedFact {
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

type StrategyArchetype = 'PREDICTIVE_ALPHA' | 'RELATIVE_VALUE' | 'EVENT_DRIVEN'
  | 'MICROSTRUCTURE' | 'PORTFOLIO_OVERLAY' | 'FACTOR_RESEARCH';
type SupportState = 'SUPPORTED' | 'CONDITIONAL' | 'UNSUPPORTED';
interface CapabilityDescriptor extends VersionedFact {
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
interface OperationContract {
  name: string;
  inputSchemaId: string;
  outputSchemaId: string;
  requiredCapabilityIds: Id[];
  effect: 'PURE' | 'NATIVE_COMPUTE' | 'EXTERNAL_EFFECT';
  replayPolicy: 'DETERMINISTIC' | 'REQUEST_BOUND' | 'RECONCILE_REQUIRED';
  incompatibleCapabilityIds: Id[];
}
interface StrategyDefinition extends VersionedFact {
  name: string;
  archetype: StrategyArchetype;
  economicMechanism: string;
  falsificationConditions: string[];
  opportunityUniverseRef: Id;
  requiredCapabilities: string[];
  intendedHoldingHorizonRef: Id;
  knownFailureModes: string[];
}
interface StrategyVersion extends VersionedFact {
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
type PipelineStage = 'IDEA' | 'RESEARCH' | 'SELECTION' | 'INDEPENDENT_EVALUATION'
  | 'SHADOW' | 'PAPER' | 'TARGET_OPERATION' | 'MONITORING' | 'RETIRED';
interface PipelineAssessment extends VersionedFact {
  strategyVersionId: Id;
  stage: PipelineStage;
  outcome: 'READY' | 'NOT_READY' | 'NEEDS_EVIDENCE' | 'SUPERSEDED' | 'RETIRED';
  evaluatedAtNs: Ns;
  requirementsVersion: string;
  evidenceRefs: Id[];
  reasonCodes: string[];
  nextPermittedActions: string[];
}

interface DatasetRevision extends VersionedFact {
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
interface LabelDefinition {
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
interface PredictionRow {
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
interface ResearchBudget extends VersionedFact {
  maximumTrials: number;
  nativeComputeSeconds: number | null;
  modelSpend: Money | null;
  dataBytes: string | null;
  reservationForClosureRef: Id;
  usageEvidenceRefs: Id[];
}
type CampaignState = 'PLANNED' | 'SEARCHING' | 'CLOSING'
  | 'EVALUATING_INDEPENDENT' | 'CONCLUDED' | 'PAUSED';
interface ResearchCampaign extends VersionedFact {
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
interface Hypothesis extends VersionedFact {
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
type TrialState = 'ADMITTED' | 'RUNNING' | 'COMPUTED' | 'FAILED' | 'REJECTED';
interface ResearchTrial extends VersionedFact {
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
interface ComparisonScope {
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
interface CandidateScorecard {
  trialId: Id;
  comparisonScopeId: Id | null;
  status: Availability;
  metrics: MetricObservation[];
  frontierMembership: 'MEMBER' | 'DOMINATED' | 'UNDETERMINED';
  dominatedByTrialIds: Id[];
  missingEvidence: string[];
  baselineComparisonRef: Id | null;
}
interface CandidateComparison extends VersionedFact {
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
interface OperatingComparison extends VersionedFact {
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
}

interface FactorVersion extends VersionedFact {
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
interface CalibrationModel extends VersionedFact {
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
interface FactorWeight {
  factorVersionId: Id;
  calibrationModelId: Id | null;
  weight: Decimal;
  economicFamilyId: Id;
  contributionEvidenceRef: Id | null;
}
interface EnsembleRevision extends VersionedFact {
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

type CapacityLevel = 'OBSERVED_LIQUIDITY' | 'CONDITIONAL_FEASIBILITY' | 'ECONOMIC';
interface CapacityPoint {
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
interface CapacityEstimate extends VersionedFact {
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
interface AccountBasis {
  accountBindingId: Id;
  accountSourceId: string;
  observationId: string;
  sequence: string;
  observedAtNs: Ns;
  availableAtNs: Ns;
  controlRevision: string;
  currency: Currency;
  equity: Money | null;
  reserved: Money | null;
  positionsEvidenceRef: Id;
  outstandingCommandsRef: Id;
}
interface CapitalPlan {
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
interface AssetTarget {
  instrumentId: string;
  navWeight: Decimal;
  targetQuantity: Decimal | null;
  targetNotional: Money;
  currentNotional: Money;
  netTradeNotional: Money;
  quantityRuleRef: Id;
  riskContribution: MetricObservation | null;
}
interface StrategyAllocation {
  strategyVersionId: Id;
  ensembleRevisionId: Id | null;
  committedCapital: Money;
  capitalFraction: Decimal | null;
  allocationEvidenceRef: Id;
  riskContributionEvidenceRef: Id | null;
  nettingGroupId: Id;
}
type DecisionAction = 'HOLD' | 'REBALANCE' | 'REDUCE' | 'CASH'
  | 'NEEDS_EVIDENCE' | 'NO_FEASIBLE_SOLUTION';
interface PortfolioDecision extends VersionedFact {
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
type CycleState = 'PENDING' | 'CAPTURING' | 'COMPUTING' | 'EVALUATING'
  | 'CONCLUDED' | 'SUPERSEDED' | 'WAITING_EVIDENCE' | 'FAILED';
interface DecisionCycle extends VersionedFact {
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
interface ObservedOutcome extends VersionedFact {
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
type DecisionTrigger = 'DATA_REVISION' | 'LABEL_MATURED' | 'ACCOUNT_CHANGED'
  | 'COST_OR_RULE_CHANGED' | 'RESEARCH_EVIDENCE' | 'EVIDENCE_INVALIDATED'
  | 'MANDATE_CHANGED' | 'FRESHNESS_DEADLINE';
interface BusinessEvent {
  id: Id;
  kind: DecisionTrigger;
  sourceRevisionRef: Id;
  availableAtNs: Ns;
  affectedMandateIds: Id[];
  supersedesEventId: Id | null;
}
interface TargetCommand extends VersionedFact {
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
interface EvidenceGap extends VersionedFact {
  affectedEntityId: Id;
  requiredCapability: string;
  reasonCode: string;
  permittedRemedy: 'WAIT_LABEL' | 'RECOMPUTE' | 'REFRESH_SOURCE'
    | 'COLLECT_AUTHORIZED_DATA' | 'CLARIFY_INTENT' | 'UNSUPPORTED';
  dataAuthorizationRef: Id | null;
  state: 'OPEN' | 'IN_PROGRESS' | 'RESOLVED' | 'UNRESOLVABLE';
  resolutionEvidenceRefs: Id[];
}
interface EvaluationProtocol extends VersionedFact {
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
interface SearchFamily extends VersionedFact {
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
interface ExposureRecord extends VersionedFact {
  evidenceRef: Id;
  roleAtExposure: EvidenceRole;
  campaignId: Id | null;
  policyRevisionId: Id | null;
  exposedAtNs: Ns;
  consumer: 'RESEARCH_AGENT' | 'SELECTION_METHOD' | 'INDEPENDENT_EVALUATOR' | 'OWNER';
  allowedUse: 'FIT' | 'SELECT' | 'EVALUATE_ONCE' | 'OBSERVE';
  resultingEvidenceRefs: Id[];
}
interface AdaptiveUpdateProtocol extends VersionedFact {
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
interface RiskModel extends VersionedFact {
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
interface CostModel extends VersionedFact {
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
interface ResearchWorkAllocation extends VersionedFact {
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
interface BudgetMovement extends VersionedFact {
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
interface FeatureField {
  name: string;
  dataType: 'FLOAT64' | 'DECIMAL' | 'INTEGER_STRING' | 'BOOLEAN' | 'CATEGORY';
  unit: string;
  sourceFieldRef: Id;
  transformationRef: Id | null;
  availabilityRuleRef: Id;
  missingValuePolicy: 'UNAVAILABLE' | 'EXPLICIT_MASK';
}
interface FeatureSchema extends VersionedFact {
  fields: FeatureField[];
  instrumentIdentitySchemaId: string;
  eventClockField: string;
  availableClockField: string;
  warmupRuleRef: Id;
  fitParameterEvidenceRefs: Id[];
}
interface SignalContract extends VersionedFact {
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
interface StrategyStateContract extends VersionedFact {
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
interface MarketAdapterContract extends VersionedFact {
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
interface OutcomeAttribution extends VersionedFact {
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
interface RetirementDisposition extends VersionedFact {
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
```

## 3. 字段语义与跨字段不变量

### 3.1 身份、版本与时间

| 字段/组合 | 精确定义 | 校验与失败结果 |
| --- | --- | --- |
| `schemaVersion` | DTO 的结构版本，不是方法、数据或任务版本 | 结构未知拒绝，不按相似字段猜 |
| `producerVersion` | 产生该业务事实的部署/计算适配版本 | 与 native 版本同时保留，不能只存当前 Git HEAD |
| `id` / `supersedesId` | 新事实身份及被取代事实；旧事实仍可查询 | 不形成环；已消费输出不得原位修改 |
| `revision` | 非负整数修订字符串，供expectedRevision比较 | 数据/模型/策略/证据版本不可变；Campaign、Trial、Hypothesis、Cycle、命令、EvidenceGap等工作聚合使用稳定ID+递增revision，保存历史转换，不为进度更新制造新业务身份 |
| `EvidenceRef.objectKey` | 服务内可解析的原存储引用 | 不向模型暴露私有文件路径或凭据；不存在则证据不可用 |
| `contentDigest` | 大型不可变科学对象内容校验；不是第二个任务/幂等账本 | 若源合同不提供可留 null，但不能声称已校验不可变内容 |
| `availableAtNs` | 本系统或该协议实际可使用该证据的最早时间 | 必须不晚于当前决策可用截止；晚到修订不得回写过去 |
| `asOfNs` | 该估计描述的决策时点 | 不等于生成时间；数据可用条件另校验 |
| `validUntilNs` | 有依据推导的最晚可用时间 | null 表示未建立有效期，不是永久有效；运行交付不接受未知时效 |
| `basisRevision` | 数据、账户、方法、策略、授权的精确版本集合身份 | 发布前 compare-and-check；变化则 supersede，不能“尽力提交” |
| `availableThroughNs` | 数据源可用 watermark | 不是最大 event timestamp；不自动证明缺失之前都完整 |

上述类型同时包含事实内容与 API 生命周期投影。`FactorVersion.status`、`PortfolioDecision.status`、`StrategyVersion.pipelineAssessmentId` 是通过带版本生命周期事实/索引解析的当前投影；状态变化不能原位修改对应模型、成员、权重、计算输入或原始独立评估。查询历史 revision 返回当时投影。官方 Task 的执行状态保持原 owner，不在这些 DTO 内重新实现。

### 3.2 意愿、策略与能力

| 对象/字段 | 来源与约束 |
| --- | --- |
| `MandateRevision.ownerGoal` | 用户原话保留；`interpretation` 是系统解释，不覆盖原话 |
| `OwnerBoundary.value` | 受 kind 对应 schema 校验，不能任意 JSON 绕过；经济损失用金额/比例+时间口径，授权使用明确资源范围 |
| `SelectionRule.objectives` | 不用未定义的加权总分；lexicographic 顺序是业务偏好，不能每次看结果后重排 |
| `utilityModelRef` | EXPECTED_UTILITY 必填，其他模式必须 null；包含风险取舍与参数来源 |
| `unresolvedIntent` | 不阻止合法研究；涉及实际目标输出的关键歧义必须解决或仅返回前沿 |
| `CapabilityDescriptor.boundaries` | 每项具体能力的支持、条件与缺口；“支持 catalog”不代表支持盘口冲击/真实套利执行 |
| `StrategyVersion` | 经济定义、模型、组合、执行及退出规则的冻结集合；模型文件单独更新即产生新版本 |
| `stateSchemaRef/entryRuleRef/holdingRuleRef/invalidationRuleRef` | 状态型策略的机器可校验状态、入场、持有与失效规则；纯因子研究可null，需状态/执行的策略不能缺少其所需规则 |
| `componentStrategyVersionIds` | 多策略组合的原成员身份；与因子成员分离。空数组表示非复合策略，不代表因子为空；初始合同禁止嵌套复合成员，先去重/扁平化叶策略后登记，避免递归重复计资 |
| `pipelineAssessmentId` | 当前该版本对应评估；评估 history 可追加新事实，不修改旧晋级记录 |
| `PipelineAssessment` | READY 仅针对当前阶段及适用能力，不是永久合格；因子研究不强制进入交易阶段 |

`OwnerBoundary.value` 按 kind 使用注册子 schema：LOSS 至少有 `metricKey,metricDefinitionId,limit,unit,horizonRef`；LEVERAGE 有 `maximumGrossNavRatio,borrowingAllowed`；MARKET_SCOPE 有 `universeRef,excludedCapabilityIds`；LIQUIDITY_NEED 有 `amount,currency,neededByNs`；CAPITAL_COMMITMENT 有 `maximumAmount,currency,validUntilNs`；DATA_USE 有 `sourceIds,permittedPurposes,validUntilNs`。其值来自意愿或授权事实，不由波动率替代。不存在的边界不能自动填一套行业默认值。

`OperationContract` 是绑定已安装官方插件/科学适配的业务调用合同，不是新插件运行时。input/output schema、effect及replayPolicy必须与实际注册工具的能力一致，不能因为声明DETERMINISTIC就将有副作用调用标成官方safe。平台在运行前核对依赖与不兼容集合，拒绝无法恢复/对账的外部作用。

`FeatureSchema`、`SignalContract`、`StrategyStateContract`、`MarketAdapterContract` 给通用化提供真实扩展点。规则引用必须解析到有schema与版本的规则/原生合同，不接受任意自然语言直接执行；状态型微观结构或多腿策略需要inventory/multiLeg能力，不能仅凭信号字段存在就自动接入当前threshold路径。

### 3.3 科学数据与比较

| 对象/字段 | 单位/来源/空值 | 不变量 |
| --- | --- | --- |
| `PredictionRow.prediction` | 原生预测，明确 unit；缺失 null | 与 label、horizon 一致；缺失不能填零进入 covariance |
| `LabelDefinition.returnBasis/priceBasis` | 预测收益是原价格收益还是账户净收益，及trade/bid/ask/mid/settlement基准 | 资产配置μ不能把已扣账户成本的收益当毛收益再扣一次；事件标签使用NOT_APPLICABLE并绑定定义 |
| `labelAvailableAtNs` | 结算/完整持有周期真正可得时点 | 超出训练 cutoff 的标签不能参与拟合，即使其 eventAt 在训练期 |
| `MetricObservation.value` | 与 valueEncoding 一致 | AVAILABLE 必须有合法有限值；其他状态必须 null；raw 可保留原异常证据 |
| `sampleCount/expectedCount` | 十进制整数，完整日或定义的样本单位 | 分子不超过分母；有效独立样本数不得用原 tick 数替代 |
| `uncertainty` | 有模型依据的区间 | lower≤upper，coverageProbability∈(0,1)，方法/相关性处理可追溯；无法估计为 null |
| `CandidateComparison.scopes` | 同一允许 universe、数据、资本/现仓、成本、窗口与日历 | 候选持仓、因子数量/权重属于搜索变量，不据此拆成不能比较的孤立组 |
| `frontierMembership` | 同一comparison scope与登记维度上的经验点估计Pareto关系，不是统计显著性或投资资格 | 全亏损候选仍可在前沿；需现金/原组合基准和业务规则选择。置信支配若启用，另由方法版本与配对证据报告，不覆盖此字段含义 |
| `objectiveRankings.ties` | 从好到坏的并列集合 | 仅可比较候选；数值误差/tie 规则来自方法，不按 trial ID 伪造收益差异 |
| `recommendedTrialId` | 科学事实+冻结偏好导出的推荐 | SELECTED 时必填且属于可选集合；其他状态 null；不默认拿排序第一个 |

所有消费指标的规则（Objective、LOSS边界、objectiveRankings）均绑定不可变`MetricDefinition.id`，并验证metricKey、horizon、单位和日历一致。`account.sharpe`的252天与365天是不同定义身份，key相同也不互换。MetricObservation内嵌定义须与注册原定义一致；同一比较scope/定义/主体存在多条矛盾值时判AMBIGUOUS，不选第一条。旧spec的具体native key通过版本化迁移映射到精确定义；原来无法确定的日历/期限不能自动补默认值。

`CandidateComparison` 仅比较研究试验；`OperatingComparison` 比较运行周期中的保持/现金/重配候选，不能为了满足trialId伪造实时ResearchTrial。`PortfolioDecision.comparisonRef` 必须解析为同cycleId/basisRevision的OperatingComparison；该运行比较可以引用研究证据，但不继承其独立资格。comparison scope定义共同现仓/资金/成本/时点，configurationEvidenceRef定义候选本身。实时预测估计、历史回放和实际前瞻结果用evaluationContext区分，不混成“已实现增益”；差值的不确定性/配对方法由pairedDifferenceEvidenceRefs承载。选择候选必须属于当前比较、状态支持其用途；保持和现金也要计入其真实退出/持有成本。

`EvaluationProtocol` 冻结方法选择、时间 folds 与统计诊断的用途；`SearchFamily` 将相关研究、失败和多次方法尝试归并；`ExposureRecord` 记录数据何时被哪个角色用于何种目的；`AdaptiveUpdateProtocol` 是整个持续更新过程的被验证对象。它们不能通过改ID或重新开campaign消除已经发生的选择偏差。

`BudgetMovement` 的 TRIAL/DATA_BYTES 数量为整数；NATIVE_SECONDS 为非负时长；MODEL_COST 必有币种。RESERVE不等于已消耗，RELEASE只释放未消耗预留，TRANSFER不增加总预算。资源不可互换单位；结项预留引用可解析到这组记录。未知实际成本保持未计量状态，不能以0消费假装剩余额度无限。

### 3.4 因子权重、资产权重和资金守恒

| 字段 | 定义 | 约束 |
| --- | --- | --- |
| `FactorWeight.weight` | 同目标预测的组合系数，无量纲 | 初始非负凸组合；含零的拟合结果保留，正系数归一化和精确舍入由 native 负责；禁止给无效因子补 epsilon |
| `EnsembleRevision` | 一次拟合结果与证据的不可变对象 | 因子成员或权重变化创建新 revision；后续在线拟合规则属于 policy，不能原位更新旧 weights |
| `StrategyAllocation` | 多策略预算分配；capitalFraction=该策略committedCapital/组合committedCapital | 组合投入为0时fraction=null；策略预算和因子预测权重不同；共享资产先按nettingGroup净额化，风险贡献不按预算比例假算 |
| `PortfolioDecision.strategyVersionId` | 单策略版本，或PORTFOLIO_OVERLAY复合策略版本 | capital可用时，非复合决策strategyAllocations恰有一项指根strategyVersionId；复合时成员集才与componentStrategyVersionIds一致。所有策略committedCapital之和等于组合committedCapital；ensembleRevisionId仅适用当前有单一ensemble时，否则null |
| `AssetTarget.navWeight` | 有符号目标市值 / 同一基准 NAV | 不是可投入资本的比例；与 targetNotional 及 NAV 一致；当前实际支持卖空/杠杆依 capability |
| `netTradeNotional` | targetNotional−currentNotional，按结算币种 | 与未完成命令/订单做净额协调；禁止把多因子需求当不同账户叠加 |
| `targetQuantity` | 原交易规则步长后的数量；仅估计金额时可 null | 需要精确数量的执行能力不接受 null |
| `deployableBudget` | 账户可分配资金减已预留与业主流动性安排 | 避免预留与冻结余额重复扣减；确切项目依原权威账户 schema 解释 |
| `committedCapital` | 本轮实际承诺策略的资本，不是总名义敞口 | ≥0 且不超过可用预算；允许杠杆时另由风险/保证金规则控制 gross exposure |
| `uncommittedCash` | 可用预算−committedCapital | 与策略内部 projectedTradingCash 区分，不能双重计入可提现现金 |
| `projectedTradingCash` | 策略内部预计留在交易账户的现金 | 来自 native 模拟/配置，不等于无风险可立即提现金额 |
| `PortfolioDecision.accountBasis` | 实际账户路径的权威一致快照 | 有账户则 researchStartingStateRef=null；研究模式相反，不能混用 |
| `capital:null` | 无法推导有效资金计划 | 不是零投入；不能输出可执行 target |

在已验证AdaptiveUpdateProtocol适用域内生成的新权重仍是新EnsembleRevision，可标为SELECTED并通过PipelineAssessment说明来自已验证更新方法；不能因此标记为该新权重本身已做过一次独立HELDOUT。超出更新协议的模型/方法变化，必须作为新研究候选重新评价。

初始实现先覆盖现有真实账户模型支持的 long-only、现金与相应预测市场语义。通用类型为其他模型留能力边界，不意味着所有模型已经支持。杠杆、衍生品、保证金、借券等必须按资产/账户适配扩展现金与风险合同，不能套同一个资金守恒公式。

### 3.5 容量、决策与反馈

| 字段 | 语义与边界 |
| --- | --- |
| `CapacityEstimate.level` | 观测流动性、条件可执行、经济容量三种不同结论；level 高低不是可自动晋级的整数 |
| `maximumTestedCapital` | 最大实际测试资金；可能仍未达到瓶颈，不等于容量上限 |
| `supportedCapitalUpperBound` | 证据支持的边界；未知 null，不能用 maximumTestedCapital 替代；还需检查是否为有限范围内的条件值 |
| `feasibleRegions` | 同basis/config下真实验证的点或有证据支持的区间；TESTED_POINT要求lower=upper，区间下界≤上界且币种一致 | 容量可非单调，capital≤上界不等于可行。不得跨不连续区间插值；本次资本无匹配点/有效区间时自动补回放并计预算，不能将摘要上界用作运行通行证 |
| `CapacityPoint.portfolioDecisionRef` | 同一资金规模下重新求解的配置；未重求解时明确保持同一策略/权重条件 |
| `fillRatio` | 在定义执行窗口内的原生/权威成交满足率，不是 BAR 参与率 |
| `PortfolioDecision.action` | 选择的经济动作；NEEDS_EVIDENCE 与 CASH 不等价 |
| `PortfolioDecision.status` | 该版本能否仍被消费；和经济动作、计算是否成功分开 |
| `explanationFacts` | 受版本 schema 约束的结构化原因，不存模型编造的解释事实 |
| `CycleState` | 业务工作阶段，不复制官方 task phase 的完整机器状态 |
| `ObservedOutcome.receiptState` | 目标接收状态；ACKNOWLEDGED 不证明成交或收益 |
| `reconciliationStatus` | 目标、实际持仓/成交与现金流是否匹配；部分匹配只发布其支持的归因 |
| `matureLabelEvidenceRef` | 满足可用时点后释放给未来学习的数据 |
| `EvidenceGap.permittedRemedy` | 缺证据后的合法下一动作；不能凭“智能补齐”扩大数据授权 |

`ObservedOutcome.attributionEvidenceRef`解析为`OutcomeAttribution`或其原生证据manifest：费用/资金流等确定记账差异，与经过匹配的反事实影响分开。只有ADDITIVE_ACCOUNTING且单位、范围相同的项允许求和，并保留无法解释的residual；消融或反事实影响非加性，不编造完全分摊的因子PnL。UNIDENTIFIED项的metric不得携带伪造数值。

退休处置使用`RetirementDisposition`：停止生成新信号不意味着已撤回外部目标或已平仓；剩余持仓和未完成命令继续对账。NONE仅适用于纯研究或可证明从未存在目标/仓位责任的情况；已关联账户但仓位未知不能使用NONE，必须保留监测/对账及EvidenceGap。FLAT_CONFIRMED只能由权威持仓/现金流证据建立，EXTERNAL_HANDOFF记录真实承接回执，不把本地task终止作为完成退出的依据。

## 4. 规范指标目录（首批，按能力逐项接入）

| 规范 key | 单位/方向 | 原始来源或拟议算法 | 支持条件 |
| --- | --- | --- | --- |
| `factor.pearson_ic` | DIMENSIONLESS / MAXIMIZE | 原因子VALIDATION Pearson IC | 纯因子证据；不代替共享账户净收益 |
| `data.expected_days` / `data.observed_days` | COUNT / DIAGNOSTIC | Calendar原完整日计数 | INTEGER_STRING、同一个日历/窗口 |
| `account.negative_days` / `account.zero_days` | COUNT / DIAGNOSTIC | Calendar原负/零收益日计数 | 不跨不同分母直接排名 |
| `account.daily_geometric_net_return` | RETURN_RATIO / MAXIMIZE | 原生 Calendar Geometric Daily Return | 完整日、原费用已经进入权益；当前并非全部路径已接通 |
| `account.worst_daily_net_return` | RETURN_RATIO / MAXIMIZE | Calendar Minimum Daily Return | 同上；不能绝对值化负收益 |
| `account.negative_day_fraction` | RATIO / MINIMIZE | negativeDays/expectedDays | 完整覆盖、分母>0；保留分子分母 |
| `account.longest_losing_streak` | COUNT / MINIMIZE | 拟议原生日序列路径统计 | 当前无该完整业务合同；缺日不能跳过继续拼串 |
| `account.maximum_drawdown` | LOSS_RATIO / MINIMIZE | 原生 Max Drawdown 经显式负→正转换 | 有效净权益路径；与原 nativeKey 对照 |
| `account.daily_maximum_deviation` | RETURN_RATIO / MINIMIZE | Calendar Maximum Absolute Deviation | 不是标准差或置信界 |
| `account.daily_cash_pnl` | CURRENCY / DIAGNOSTIC | 权威权益与现金流调节 | 不以收益率×当前本金回填 |
| `account.sharpe` | DIMENSIONLESS / MAXIMIZE | 原生统计，annualizationDays 必须明确 | 同日历/窗口；252与365不混排 |
| `data.complete_day_coverage` | RATIO / DIAGNOSTIC | 原生完整日计数 | coverage=1 也不能覆盖未定价状态 |
| `cost.recorded_commission` | CURRENCY / DIAGNOSTIC | 原 recorded_fee_totals | 精确 Decimal；净收益不再减一次 |
| `execution.gross_fill_notional` / `execution.fill_count` | CURRENCY或COUNT / DIAGNOSTIC | 原 recorded_fee_totals | 不是独立交易样本数，不据此推导经济容量 |
| `cost.implementation_shortfall` | RETURN_RATIO或CURRENCY / MINIMIZE | 拟议目标—实现归因 | 同一原目标、价格基准、时间与成交证据 |
| `factor.incremental_net_value` | RETURN_RATIO / MAXIMIZE | 拟议消融+重拟合+联合回放 | 非加性，不直接求和变成账户 PnL |
| `risk.volatility_contribution` | RATIO / DIAGNOSTIC | w_i(Σw)_i/σ | σ>0、PSD、单位/horizon一致；允许负贡献 |
| `forecast.calibration_error` | 由目标单位定义 / MINIMIZE | 时间 OOF 预测及成熟标签 | 训练损失不是投资资格 |
| `capacity.economic_upper_bound` | CURRENCY / DIAGNOSTIC | 拟议规模回放+冲击/退出模型 | 无经济证据不输出；不得替换成历史 BAR 上界 |

新增 key 必须注册定义、单位、原始来源、计算/转换版本、业务用途与可用条件。指标计算的增量/在线版本必须与同输入离线重算在登记的数值容差内一致。

## 5. API 规范（拟议增量；沿用现有 API 前缀与认证）

请求使用 `requestId` 和 `expectedRevision` 处理幂等与并发；服务端生成事实身份。模型接口不允许指定未经登记的私有证据路径。路径中的 ID 均为 opaque string；已有数字研究 API 保持兼容。

| 方法与相对路径 | 请求字段 | 返回 | 业务效果 |
| --- | --- | --- | --- |
| `GET /capabilities` | 无 | `CapabilityDescriptor[]` | 确认支持范围，不猜测原生能力 |
| `POST /mandates` | requestId, goal, capitalSource, allowedUniverseRef, operatingAuthority | 原话、结构化解释、MandateRevision、歧义 | 从意图建合同；不要求技术配置 |
| `POST /mandates/:id/revisions` | requestId, expectedRevision, goal/已明确意愿变化 | 新 MandateRevision | 不覆盖旧授权/目标 |
| `POST /datasets/:id/revisions` | requestId, sourceRevision, contentRef, 时间/源证明 | DatasetRevision+验证任务回执 | 可用时点/质量核实后事件生效 |
| `POST /campaigns` | requestId, mandateRevisionId, strategyDefinitionId, datasetRevisionIds | ResearchCampaign+runtime | 开始预算内研究 |
| `GET /campaigns/:id` | 无 | ResearchCampaign及预算/原回执引用 | 读取当前进度和原输入 |
| `GET /campaigns/:id/trials` | cursor, status可选 | ResearchTrial页 | 包含失败和拒绝，不只返回赢家 |
| `GET /campaigns/:id/decision` | cursor可选 | CandidateComparison | 自动最新比较与完整证据范围 |
| `POST /campaigns/:id/close` | requestId, expectedRevision, reason | 收尾状态与结论回执 | 关闭新实验，保留比较/终审或不选择 |
| `POST /campaigns/:id/pause` | requestId, expectedRevision, reason | PAUSED回执 | 暂停后继工作，保留预算与数据暴露 |
| `POST /campaigns/:id/resume` | requestId, expectedRevision, predecessorReceiptId | 原Campaign的合法后继回执 | 从原检查点续接；不重置预算，不重算已结算失败 |
| `GET /strategies/:id/versions` | cursor | StrategyVersion[] | 版本/方法/证据生命周期 |
| `GET /strategies/:id/assessments` | cursor | PipelineAssessment[] | 各阶段何时及为何具备/失去资格 |
| `GET /mandates/:id/decision` | 无 | 当前PortfolioDecision及时效 | 直接回答现阶段的投资判断 |
| `GET /mandates/:id/decisions` | cursor | PortfolioDecision历史页 | 查询原时点判断，不被最新投影抹掉 |
| `POST /mandates/:id/recompute` | requestId, expectedRevision, reason | DecisionCycle回执 | 运维/研究重算入口；正常事件无需人工调用 |
| `GET /cycles/:id` | 无 | DecisionCycle+耗时/原因 | 观察依赖、状态、失败，不暴露运行时私密信息 |
| `GET /capacity-estimates/:id` | 无 | CapacityEstimate | 分层曲线、测试范围、瓶颈与未知 |
| `GET /decisions/:id/diff` | againstDecisionId | weights/capital/cost/原因差异 | 变化由证据解释，不只给新数字 |
| `GET /decisions/:id/evidence` | cursor | 所有者可见 EvidenceRef页 | 遵守角色隔离，不给研究模型返回Sealed |
| `GET /decisions/:id/outcomes` | cursor | ObservedOutcome页 | 原目标→后续实际账户归因 |
| `POST /strategies/:id/pause` | requestId, expectedRevision, reason | 停止新增目标的业务回执 | 本地暂停不代表撤回外部已接受目标 |
| `POST /strategies/:id/resume` | requestId, expectedRevision, predecessorReceiptId | 新鲜依据下的运行准备评估 | 重新核对时效、账户与能力，不直接重发旧target |
| `POST /strategies/:id/retire` | requestId, expectedRevision, reason | RETIRED评估及持仓/目标处置状态 | 不直接假定已平仓；继续对账 |

异步写接口返回 `202`，回执至少 `{requestId, entityId, runtime, state, viewPath}`。只读成功 `200`；语义不支持 `422`，版本/请求内容冲突 `409`，资源不存在 `404`，依赖暂不可用 `503`。现有 API 仍保持原错误合同，新 API 不通过静默重试掩盖冲突。

错误体：`{code, category, entityId, reasonCodes, retryable, requiredEvidence, correlationId}`。category 为 `INTENT | DATA | SCIENCE | CAPABILITY | CONFLICT | EXECUTION`。`retryable` 只描述相同原请求是否适合重试；科学不支持不会通过无限重试变成功。外部目标回执不确定时关联原命令进行 reconcile。

## 6. 原生适配合同与职责

| 拟议操作 | 核心输入 | 核心输出 | 复用与新增 |
| --- | --- | --- | --- |
| `fit-ensemble` | PredictionPanelRef、校准/label单位、时间fold协议、方法版本 | EnsembleRevision所需成员系数、拟合证据、原生solver结果 | 新 typed QP adapter 调用官方 Clarabel；不是新优化器 |
| `derive-risk` | 原收益/暴露面板、as-of、方法版本 | covariance/scenario模型、覆盖、单位与有效域 | 先复用已支持sample covariance；其他估计器单独验收 |
| `study-portfolio` | 原生已支持的市场、预测、执行/风险/流动性合同 | 滚动资产解与共享账户回放 | 复用现有；同步BAR限制不能隐藏 |
| `evaluate-capital-curve` | 资金意图、市场最小单位、已冻结策略、现仓/成本模型、预算 | 实际测试CapacityPoint、瓶颈与边界 | 新编排+原生回放；资本点自适应生成且计账 |
| `derive-portfolio-decision` | 三层方法输入、账户basis、有效证据引用 | PortfolioDecision所需金额、仓位、保持/调整比较 | 可组合已有科学工具，名称不等于一次万能求解器 |

科学请求与结果都需要 schemaVersion、methodVersion、inputEvidenceRefs、cutoff、units、status/reason、diagnostics、原预算消耗。TS 负责冻结/调用/持久化与策略决策，Rust负责数值计算和必要纯校验。具体 native CLI 新操作是否合并为同一个命令属于实现细节；其语义合同不能因此合并三种权重。

## 7. 事件、存储与恢复映射

| 业务事实 | 拟议存储方式 | 注意点 |
| --- | --- | --- |
| Mandate/Policy/Strategy/Capability版本 | 官方Doc/DocFamily中不可变版本记录，Application保留活动引用 | 逻辑不可变通过业务写入规则实现，不宣称Doc本身不可变 |
| Campaign/Trial/Hypothesis与比较 | 复用Study所属conversation与原native任务，新增版本合同/引用 | 保留原试验身份、总尝试及失败，不新建独立scheduler |
| DecisionCycle及待处理事件 | 官方Task+业务Doc，revision frontier同事务更新 | 业务事件不等于新增Kafka/自研消息总线 |
| 大型预测/收益/模型/回放 | 现有原生证据分块/数据存储，EvidenceRef定位 | 与原conversation/task绑定；不复制全部行情进Application |
| PortfolioDecision/TargetCommand | 正式版本事实+当前活动引用，沿原回执/对账结构演进 | 原事务冻结命令身份后交付；不确定只对账原命令 |
| Outcome/归因与成熟标签 | 权威账户原件+派生引用 | 不能用目标权重或ACK来伪造实际收益 |

官方 `Doc.migrate(value, fromVersion)` 与 `TaskDefinition.migrate(input, checkpoint, fromVersion)` 只在经过兼容测试的情况下使用。旧 v1 冻结研究继续原语义；不能运行一次 migrate 就补出原本没做过的独立验证、校准或容量证明。

## 8. 合同级验收

字段校验至少覆盖：金额与币种一致、时间区间/可用性、指标状态与值互斥、目标与标签horizon、成本净额、三层权重分母、资金守恒、原生能力支持、前沿与资格独立、容量测试区间与真正上界分离、研究样本角色隔离、旧版本兼容以及过时basis不能交付。

这份合同用于让业务设计可实现和可核对；不是要求一次性把所有实体变成重型服务。实际模块可以采用纯函数、官方Doc与少量Task组合，前提是所有业务事实与责任仍然清晰。
