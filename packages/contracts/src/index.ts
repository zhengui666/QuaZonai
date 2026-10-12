/** Source DTOs for the compact TypeScript application; native scientific DTOs remain Rust-owned. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };
export interface ModelRef { provider: string; modelId: string }
export interface ModelView extends ModelRef { name: string; reasoning: boolean }
export interface Page<T> { items: T[]; cursor?: JsonObject }
export interface Problem { error: string; message?: string }
export interface SessionView { setupRequired: boolean; authenticated: boolean }
export interface ResearchInput { goal: string; model: ModelRef; data: JsonObject; policy: JsonObject }
export type ResearchStart = { requestId: string; input: ResearchInput } | {
  requestId: string; goal: string; model: ModelRef; datasetId: string; maxTrials: number;
};
export interface ResearchReceipt { conversationId: number; submissionId: number }
export interface Metric { key: string; label: string; value: number | string | null; unit?: string }
export interface ResearchSummary {
  id: number; title: string; status: string; trials: number; model: string;
  createdAt: string | null; updatedAt: string | null;
}
export interface TrialView { id: number; hypothesis: string; status: string; metrics: Metric[]; error: string | null }
export interface TaskView { id: number; name: string; status: string; error: string | null }
export interface ResearchDetail extends ResearchSummary {
  input: ResearchInput; trialRecords: TrialView[]; tasks: TaskView[];
  submission?: { id: number; status: string } | null;
  final: { status: string; trialId: number | null; reason: string | null; metrics: Metric[] } | null;
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
}
export type ObservationMode = 'paper' | 'live';
export interface PositionView { instrumentId: string; quantity: string; value: string | null; pnl: string | null }
export interface TargetReceipt { id: string; status: string; externalId: string | null; expiresAt: string; acknowledgedAt: string | null }
export interface ObservationStart { requestId: string; portfolioId: string; mode: ObservationMode; connectionId: string }
export interface ObservationView {
  id: string; name: string; portfolioId: string; mode: ObservationMode; status: string;
  currency: string; equity: string | null; cash: string | null; updatedAt: string | null;
  positions: PositionView[]; curve: EquityPoint[]; receipt: TargetReceipt | null; error: string | null;
}
export interface ConnectionView { id: string; name: string; mode: ObservationMode; connected: boolean }
export interface SettingsView {
  name: string; defaultModel: ModelRef | null; maxTrials: number;
  taskTimeoutSeconds: number; nativeTimeoutSeconds: number; connections: ConnectionView[];
}
export interface ExtensionView {
  name: string; version: string | null; enabled: boolean; kind: string;
  state: 'loaded' | 'disabled' | 'pending_restart' | 'error'; error: string | null;
}
export interface UsageView { inputTokens: number | null; outputTokens: number | null; cost: string | null; activeTasks: number }

export const API = '/api/v3';
