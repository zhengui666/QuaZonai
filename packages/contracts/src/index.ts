/** Source DTOs for the compact TypeScript application; native scientific DTOs remain Rust-owned. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };
export interface ModelRef { provider: string; modelId: string }
export type ThinkingLevel = 'off' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh' | 'max';
export interface ModelView extends ModelRef { name: string; reasoning: boolean; thinkingLevels: ThinkingLevel[]; effectiveThinkingLevel: ThinkingLevel }
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
export interface Problem { error: string; message?: string }
export interface SessionView { setupRequired: boolean; authenticated: boolean }
export interface ResearchInput { goal: string; model: ModelRef; thinkingLevel?: ThinkingLevel; data: JsonObject; policy: JsonObject }
export type ResearchStart = { requestId: string; input: ResearchInput } | {
  requestId: string; goal: string; model: ModelRef; thinkingLevel?: ThinkingLevel; datasetId: string; maxTrials: number;
};
export interface ResearchReceipt { conversationId: number; submissionId: number }
export interface ResearchResume { submissionId: number }
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
export interface TrialView { id: number; hypothesis: string; status: string; metrics: Metric[]; error: string | null }
export interface TaskView { id: number; name: string; status: string; error: string | null; outcome?: string }
export interface RecordedDataset { id: string; name: string; assetClass: string; currency: string; source: string; start: string | null; end: string | null }
export interface ResearchDetail extends ResearchSummary {
  input: ResearchInput; trialRecords: TrialView[]; tasks: TaskView[];
  dataset?: RecordedDataset | null; datasetId?: string | null;
  submission?: ResearchSubmissionView | null; originalSubmission?: ResearchSubmissionView | null;
  final: { status: string; trialId: number | null; reason: string | null; error?: string | null; metrics: Metric[] } | null;
}
export interface DatasetView {
  id: string; name: string; assetClass: string; currency: string; source: string;
  start: string | null; end: string | null; ready: boolean;
}
export interface EquityPoint { at: string; value: string | null }
export interface TargetWeight { instrumentId: string; weight: string }
export interface PortfolioStart {
  requestId: string; name: string; researchIds: number[]; datasetId: string; capital: string;
  weights?: { researchId: number; weight: string }[];
}
export interface PortfolioView {
  id: string; name: string; researchIds: number[]; datasetId: string; status: string;
  capital: string; currency: string; createdAt: string; taskId: number | null;
  metrics: Metric[]; curve: EquityPoint[]; targets: TargetWeight[]; error: string | null;
  weights?: { researchId: number; weight: string }[]; targetExpiresAt?: string | null; targetExpired?: boolean;
}
export type ObservationMode = 'paper' | 'live';
export interface PositionView { instrumentId: string; quantity: string; value: string | null; pnl: string | null }
export interface TargetReceipt { id: string; status: string; externalId: string | null; expiresAt: string; acknowledgedAt: string | null }
export interface ObservationStart { requestId: string; portfolioId: string; mode: ObservationMode; connectionId: string }
export interface ObservationView {
  id: string; name: string; portfolioId: string; mode: ObservationMode; status: string;
  currency: string; equity: string | null; cash: string | null; updatedAt: string | null;
  positions: PositionView[]; curve: EquityPoint[]; receipt: TargetReceipt | null; error: string | null;
  connectionId?: string; connectionName?: string;
  provenance?: { clientId: string; accountId: string; sequence: string; nativeClockNs: string } | null;
}
export interface ConnectionView { id: string; name: string; mode: ObservationMode; connected: boolean }
export interface SettingsView { defaultModel: ModelRef | null; thinkingLevel: ThinkingLevel | null }
export interface WorkspaceView { name: string; connections: ConnectionView[] }
export interface ExtensionView {
  name: string; version: string | null; enabled: boolean; kind: string; required?: boolean;
  state: 'loaded' | 'disabled' | 'pending_restart' | 'error'; error: string | null;
}
export interface UsageView { inputTokens: number | null; outputTokens: number | null; cost: string | null; activeTasks: number }

export const API = '/api/v3';
