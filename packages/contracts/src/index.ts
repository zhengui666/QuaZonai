export * from './capital-exit.ts';
import type { CapitalExitStatus } from './capital-exit.ts';
export type * from './pm.ts';
export type * from './maintenance.ts';
export type * from './data-sources.ts';
export * from './tick-input.ts';
export type * from './pm-intent.ts';
export * from './decision.ts';
export * from './pm-validation.ts';
export * from './pm-forward.ts';
export type * from './review.ts';
export type * from './review-source.ts';
export type * from './reviewed-factors.ts';
export type * from './portfolio-performance.ts';
export * from './parameter-scans.ts';
export * from './review-scoring.ts';
/** Source DTOs for the compact TypeScript application; native scientific DTOs remain Rust-owned. */
import type { ExecutionErrorDetail } from './execution.ts';
import type { LiteratureView } from './literature.ts';
import type { RemoteNativeInstance } from './nautilus.ts';
import type { ReviewReport } from './review.ts';
export type { LiteratureSource, LiteraturePaper, LiteratureSearchResult, LiteratureSearch, LiteratureNote, LiteratureView } from './literature.ts';
export type { ExecutionErrorCategory, ExecutionErrorDetail } from './execution.ts';
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };
export type ModelRef = import('@earendil-works/pi-durable').ModelRef;
export type ThinkingLevel = import('@earendil-works/pi-ai').ModelThinkingLevel;
export type ServiceTier = 'default' | 'priority';
export interface ModelView extends ModelRef { name: string; reasoning: boolean; thinkingLevels: ThinkingLevel[]; effectiveThinkingLevel: ThinkingLevel; serviceTiers: ServiceTier[] }
export type ModelAuthType = 'api_key' | 'oauth';
export interface ProviderView { id: string; name: string; configured: boolean; authTypes: ModelAuthType[]; login: { id: string; type: ModelAuthType } | null; error: string | null }
export interface ModelCatalog { providers: ProviderView[]; models: ModelView[] }
export type ModelAuthPrompt = { id: number; message: string } & (
  { type: 'text' | 'secret' | 'manual_code'; placeholder?: string } |
  { type: 'select'; options: readonly { id: string; label: string; description?: string }[] }
);
export type ModelAuthEvent = { type: 'auth_url'; url: string; instructions?: string } |
  { type: 'device_code'; userCode: string; verificationUri: string } |
  { type: 'info' | 'progress'; message: string };
export interface ModelLoginView {
  id: string; providerId: string; type: ModelAuthType; status: 'running' | 'done' | 'failed' | 'cancelled';
  prompt: ModelAuthPrompt | null; events: ModelAuthEvent[]; error: string | null;
}
export interface Page<T> { items: T[]; cursor?: JsonObject }
/** Persistent owner-visible deletion; original evidence and receipts remain retained. */
export interface DeletionReceipt { kind: 'mandate' | 'research' | 'factor' | 'portfolio'; id: string; deletedAt: string }
export interface Problem { error: string; message?: string }
export interface SessionView { setupRequired: boolean; authenticated: boolean }
export interface CapitalAssumptions { scenarios: string[]; participation: string; maximumAgeSeconds: number }
export interface ResearchSpecification {
  version: 1; name: string; question: string; rationale: string; exclusions?: string;
  evaluation: { metric: 'pearson_ic' | 'Sharpe Ratio (252 days)' | 'Sharpe Ratio (365 days)'; direction: 'maximize'; target?: number; scheme: 'registered-partitions' };
  search: { scope: 'single-factor' | 'ensemble'; maxMembers: number; stop: 'selection' | 'budget-or-selection' };
  capital?: CapitalAssumptions;
}
export interface ResearchInput { goal: string; specification?: ResearchSpecification; publicLiteratureQueries?: string[]; model: ModelRef; thinkingLevel?: ThinkingLevel; serviceTier?: ServiceTier; data: JsonObject; policy: JsonObject }
export type ResearchStart = { requestId: string; input: ResearchInput; reviewedFactorIds?: string[] } | {
  requestId: string; reviewedFactorIds?: string[]; goal: string; specification?: ResearchSpecification; publicLiteratureQueries?: string[]; model: ModelRef; thinkingLevel?: ThinkingLevel; serviceTier?: ServiceTier; datasetId: string;
};
export interface CriterionView { status: 'computed' | 'met' | 'not_met' | 'insufficient' | 'failed'; metric: string; value: number | null; target: number | null; partition: string; reason: string | null }
export interface ResearchReceipt { conversationId: number; submissionId: number }
export interface ResearchResume { submissionId: number; instruction?: string }
export interface ResearchResumeReceipt extends ResearchReceipt { previousSubmissionId: number }
export interface ResearchSubmissionView { id: number; status: string; requestId?: string; reason?: string | null }
export interface ResearchDiagnostics {
  conversationId: number; originalSubmission: ResearchSubmissionView | null; submission: ResearchSubmissionView | null;
  generation: { taskId: number; status: string; outcome: string | null; error: string | null } | null;
}
export interface Metric { key: string; label: string; value: number | string | null; unit?: string; status?: string; partition?: string; component?: number }
export interface ResearchSummary {
  id: number; title: string; status: string; trials: number; model: string; datasetId?: string | null;
  createdAt: string | null; updatedAt: string | null;
}
export interface TrialView { id: number; hypothesis: string; status: string; metrics: Metric[]; account?: JsonObject; assumptions?: string[]; inputCoverage?: JsonObject; economic?: JsonObject; error: string | null; errorDetail?: ExecutionErrorDetail | null }
export interface DailyReturn { timestamp_ns: string; value: number | null; reason_code: string | null }
export interface TaskView { id: number; name: string; status: string; error: string | null; outcome?: string }
export interface DatasetPreparation {
  archiveLoaded: boolean;
  nativePrepared: boolean;
  /** Operational readiness is separate from scientific evidence. */
  scientificQualification: 'UNVERIFIED' | 'NOT_ASSESSED';
}
export interface RecordedDataset { id: string; name: string; assetClass: string; currency: string; source: string; start: string | null; end: string | null; nativeModality?: 'BAR' | 'TRADE_TICK'; preparation?: DatasetPreparation; inputCoverage?: JsonObject }
export interface ResearchMessage { entryId: number; text: string }
export interface ResearchDetail extends ResearchSummary {
  /** Visible assistant text from the original Pi conversation, not model reasoning or tool payloads. */
  messages?: ResearchMessage[];
  reviews?: ReviewReport[];
  toolPolicy?: 'NAUTILUS_ONLY';
  previousNativeResults?: { previousSubmissionId: number; final: JsonObject }[];
  input: ResearchInput; trialRecords: TrialView[]; tasks: TaskView[];
  literature?: LiteratureView;
  dataset?: RecordedDataset | null; datasetId?: string | null;
  submission?: ResearchSubmissionView | null; originalSubmission?: ResearchSubmissionView | null;
  previousStops?: { previousSubmissionId: number; final: JsonObject }[];
  criterion?: CriterionView; factors?: string[];
  final: { nativeIndicatorResult?: JsonObject; status: string; trialId: number | null; reason: string | null; error?: string | null; errorDetail?: ExecutionErrorDetail | null; metrics: Metric[]; account?: JsonObject; assumptions?: string[]; inputCoverage?: JsonObject; dailyReturns?: DailyReturn[]; executionRequest?: JsonObject; registeredRequest?: JsonObject } | null;
}
export interface DatasetView {
  id: string; name: string; assetClass: string; currency: string; source: string;
  start: string | null; end: string | null; ready: boolean;
  nativeModality?: 'BAR' | 'TRADE_TICK';
  preparation?: DatasetPreparation;
  inputCoverage?: JsonObject;
  researchSettings?: { instruments: string[]; inputFamily: string; horizon: number | null; horizonNs?: string; horizonUnit?: 'BAR_OBSERVATIONS' | 'TRADE_COHORTS' | 'EVENT_TIME_NS' | 'RECEIVED_TIME_NS'; modality?: 'BAR' | 'TRADE_TICK'; sampling?: string; sourceRevision?: string; availability?: string; partitions: JsonObject | null; strategy: JsonObject | null };
}
export interface EquityPoint { at: string; value: string | null }
export interface TargetWeight { instrumentId: string; weight: string }
export interface FactorReference { researchId: number; trialId: number; memberIndex: number }
export interface FactorView extends FactorReference {
  review?: ReviewReport | null;
  id: string; name: string; hypothesis: string; modelSource: string; datasetId: string | null; currency: string | null; market: string[];
  status: string; evidence: 'independent' | 'historical' | 'component_only' | 'validation_only' | 'unavailable';
  metrics: Metric[]; criterion: CriterionView | null; combinable: boolean; reason: string | null; capacity?: CapacityReport; capacityAssumptions?: CapitalAssumptions;
  nativeModality?: 'BAR' | 'TRADE_TICK'; availability?: 'EVENT_TIME_PROXY_UNVERIFIED' | 'RECEIVED_TIME_OBSERVED'; scientificQualification?: 'UNQUALIFIED_EXPLORATORY';
}
export interface FactorAllocation { factorId: string; weight: string }
export interface PortfolioStart {
  independentDatasetId?: string;
  requestId: string; name: string; researchIds?: number[]; factors?: FactorAllocation[]; datasetId: string; capital: string;
  weights?: { researchId: number; weight: string }[]; capacityPolicy?: NonNullable<ResearchSpecification['capital']>;
}
export interface CapacityScenario {
  capital: string; requiredTurnover: string | null; maximumParticipation: string | null; usableCapital: string | null;
  unallocatedCash: string | null; status: string; reason: string | null;
  entryRebalanceTurnover?: string | null; exitTurnover?: string | null;
  entryRebalanceParticipation?: string | null; exitParticipation?: string | null;
}
export interface CapacityConstraint {
  kind: 'ENTRY_REBALANCE' | 'FULL_EXIT'; maximumCapital: string; instrumentId: string;
  decisionNs: string; barEventNs: string; barAvailableNs: string; observedNotional: string; requiredWeight: string;
}
export interface CapacityReport {
  status: string; reason: string | null; scope: string; currency: string; basis: string;
  errorDetail?: ExecutionErrorDetail;
  /** Original explicit policy; scenarios also contains native-generated diagnostic rows. */
  requestedAssumptions?: CapitalAssumptions;
  maximumCapital: string | null; minimumCapital: string | null; bindingInstrument: string | null;
  observationStartNs: string | null; observationEndNs: string | null; decisionClockNs: string | null; validUntilNs: string | null;
  assumptions: { participation: string | null; maximumAgeSeconds: number | null; executionWindow: string; exit: string; fees: string; depth: string };
  scenarios: CapacityScenario[];
  methodology?: { id: string; formula: string; notes: string[] };
  constraints?: CapacityConstraint[];
  coverage?: { decisionCount: number; assessedDecisionCount: number; requiredObservations: number; coveredObservations: number; uniqueBars: number; reusedBarObservations: number };
  volumeStress?: { volumeMultiplier: string; maximumCapital: string | null; status: string; reason: string | null; scenarios: CapacityScenario[] }[];
  economicCapacity?: { status: 'UNAVAILABLE'; maximumCapital: null; reason: string; missingInputs: string[] };
}
export interface PortfolioProgress { phase: string; updatedAt: string; completed?: number; total?: number }
export interface PortfolioView {
  performance?: import('./portfolio-performance.ts').PortfolioPerformanceSummary;
  review?: ReviewReport | null;
  progress?: PortfolioProgress;
  id: string; name: string; researchIds: number[]; datasetId: string; status: string;
  capital: string; currency: string; createdAt: string; taskId: number | null;
  metrics: Metric[]; curve: EquityPoint[]; targets: TargetWeight[]; error: string | null; errorDetail?: ExecutionErrorDetail | null;
  weights?: { researchId: number; weight: string }[]; factors?: FactorAllocation[]; capacity?: CapacityReport; targetExpiresAt?: string | null; targetExpired?: boolean;
}
export type ObservationMode = 'paper' | 'live';
export interface PositionView { instrumentId: string; quantity: string; value: string | null; pnl: string | null }
export interface TargetReceipt { id: string; status: string; externalId: string | null; expiresAt: string; acknowledgedAt: string | null; nativeProtection?: JsonObject }
export interface AutomaticPolicy { enabled: boolean; cashBuffer: string; maximumSnapshotAgeSeconds: number }
export interface AutomaticAllocation {
  status: string; reason: string | null; updatedAt: string | null; sequence: string | null; nativeClockNs: string | null;
  equity: string | null; reserved: string | null; cashBuffer: string; budget: string | null; capital: string | null; capacity: string | null; residualCash: string | null;
  controlRevision: string | null; members: { factorId: string; capital: string; scale: string }[];
  targets: TargetWeight[]; receipt: TargetReceipt | null; epoch: string;
}
export interface AccountObservationStart { requestId: string; purpose: 'ACCOUNT_ONLY'; mode: ObservationMode; connectionId: string; portfolioId?: never; automatic?: never }
export interface ObservationStart { requestId: string; portfolioId: string; mode: ObservationMode; connectionId: string; automatic?: AutomaticPolicy; nativeBrackets?: import('./native-order-preview.ts').NativeTargetBrackets }
export interface ObservationView {
  purpose?: 'ACCOUNT_ONLY';
  id: string; name: string; portfolioId: string | null; mode: ObservationMode; status: string;
  currency: string; equity: string | null; cash: string | null; updatedAt: string | null;
  positions: PositionView[]; curve: EquityPoint[]; receipt: TargetReceipt | null; error: string | null;
  automaticPolicy?: AutomaticPolicy; allocation?: AutomaticAllocation; capitalExit?: CapitalExitStatus;
  connectionId?: string; connectionName?: string;
  provenance?: { clientId: string; accountId: string; sequence: string; nativeClockNs: string } | null;
}
export interface ConnectionView { id: string; name: string; mode: ObservationMode; connected: boolean }
export interface NautilusConnection {
  id: string; name: string; mode: ObservationMode;
  url: string; clientId: string; accountId: string; token?: string; tokenEnv?: string;
}
export interface InstanceSettingsView {
  revision: number; backtest: RemoteNativeInstance | null; connections: NautilusConnection[];
}
export interface InstanceSettingsUpdate {
  expectedRevision: number; backtest: RemoteNativeInstance | null; connections: NautilusConnection[];
}
export interface SettingsView { defaultModel: ModelRef | null; thinkingLevel: ThinkingLevel | null; serviceTier: ServiceTier | null; tickBookCores?: number }
export interface WorkspaceView { name: string; connections: ConnectionView[] }
export interface ExtensionView {
  name: string; version: string | null; enabled: boolean; kind: string; required?: boolean;
  state: 'loaded' | 'disabled' | 'pending_restart' | 'error'; error: string | null;
}
export interface UsageView { inputTokens: number | null; outputTokens: number | null; cost: string | null; activeTasks: number }

export const API = '/api/v3';
export type { ListPage, ListCriterion, ResearchListItem, FactorListItem, ListPerformance, PortfolioListItem, TickReplayListItem, AllocationListItem } from './lists.ts';
