import type { EquityPoint, JsonObject, ListPage, Metric } from './index.ts';

export type ParameterScanMode = 'STRATEGY';
/** Existing numeric leaf path in the bound native request. */
export type ParameterScanKey = string;
export type ParameterValues = Partial<Record<ParameterScanKey, string>>;
export type ParameterScanSource = { kind: 'research'; researchId: number };
export type ParameterDomain = { kind: 'values'; values: string[] } | { kind: 'range'; start: string; end: string; step: string };
export interface ParameterAxis { key: ParameterScanKey; domain: ParameterDomain }
/** Original upstream statistic key, not a QuaZonai score. */
export type ParameterScanMetricId = string;
export interface ParameterScanObjective { metricId: ParameterScanMetricId; partition: 'FULL_SELECTION'; direction: 'maximize' | 'minimize' }
export interface ParameterScanPlan {
  name: string; source: ParameterScanSource; mode: ParameterScanMode; axes: ParameterAxis[];
  objective: ParameterScanObjective; execution: { concurrency: number };
}
export interface ParameterScanStart extends ParameterScanPlan { requestId: string }
export interface NormalizedParameterAxis extends ParameterAxis { count: string; duplicates: string }
export interface ParameterScanGrid {
  axes: NormalizedParameterAxis[]; fixed: ParameterValues; total: string; eligible: string; skipped: string;
}
export interface ParameterScanPoint { ordinal: string; parameters: ParameterValues; skipReason: string | null }
export interface ParameterScanParameterDefinition {
  key: ParameterScanKey; label: string; kind: 'integer' | 'number' | 'decimal' | 'nanoseconds'; modes: ParameterScanMode[]; modalities?: ('BAR' | 'TRADE_TICK' | 'NATIVE')[];
}
export interface ParameterScanObjectiveDefinition {
  metricId: ParameterScanMetricId; label: string; mode: ParameterScanMode; partition: 'FULL_SELECTION'; direction: 'maximize' | 'minimize'; nativeKey: string; unit: string; modalities?: ('BAR' | 'TRADE_TICK' | 'NATIVE')[];
}
/** Source-specific paths are discovered from the unchanged native request. */
export const parameterScanParameters: readonly ParameterScanParameterDefinition[] = [];
export function parameterScanObjective(metricId: string): ParameterScanObjectiveDefinition {
  const equity = /^equity-performance:(252|365):(.+)$/.exec(metricId);
  return { metricId, label: equity ? `官方日权益 · ${equity[2]}` : metricId, mode: 'STRATEGY', partition: 'FULL_SELECTION', direction: 'maximize', nativeKey: equity?.[2] ?? metricId, unit: 'NATIVE' };
}
/** Suggestions only. Any statistic returned by the native analyzer can be selected. */
export const parameterScanObjectives = [...[252, 365].flatMap(period => ['CAGR', 'Sharpe Ratio'].map(name => `equity-performance:${period}:${name} (${period} days)`)), 'Sharpe Ratio (252 days)', 'Sortino Ratio (252 days)', 'CAGR (252 days)', 'Max Drawdown', 'Calmar Ratio (252 days)', 'Value at Risk (confidence 0.95)', 'Expected Shortfall (confidence 0.95)', 'Ulcer Index'].map(parameterScanObjective);

export interface ParameterScanSourceView {
  source: ParameterScanSource; label: string; researchId: number;
  datasetId: string | null; datasetName: string | null; modality: 'BAR' | 'TRADE_TICK' | 'NATIVE'; currency: string | null;
  interval: { startNs: string; endExclusiveNs: string }; assumptions: string[];
}
export interface ParameterScanOptions {
  source: ParameterScanSourceView | null; modes: ParameterScanMode[]; parameters: ParameterScanParameterDefinition[];
  objectives: ParameterScanObjectiveDefinition[]; defaults: ParameterValues; reason: string | null;
}
export interface ParameterScanPreview {
  input: ParameterScanPlan; source: ParameterScanSourceView; grid: ParameterScanGrid;
  objective: ParameterScanObjectiveDefinition; examples: ParameterScanPoint[]; warnings: string[];
}
export type ParameterScanStatus = 'QUEUED' | 'RUNNING' | 'RECONCILING' | 'STOPPING' | 'STOPPED' | 'COMPLETED' | 'FAILED';
export interface ParameterScanCounts {
  total: string; eligible: string; skipped: string; completed: string; failed: string; stopped: string;
  running: string; pending: string; unknown: string; comparable: string;
}
export interface ParameterScanMetric extends Metric { value: number | null; reason: string | null; nativeGroup?: string; currency?: string }
export interface ParameterScanBest { ordinal: string; parameters: ParameterValues; metric: ParameterScanMetric }
export interface ParameterScanView {
  id: string; requestId: string; name: string; source: ParameterScanSource; sourceView: ParameterScanSourceView;
  mode: ParameterScanMode; input: ParameterScanPlan; grid: ParameterScanGrid; taskId: number; createdAt: string; updatedAt: string;
  status: ParameterScanStatus; execution: { concurrency: number }; objective: ParameterScanObjective; counts: ParameterScanCounts;
  best: ParameterScanBest | null; error: string | null; scientificQualification: 'UNQUALIFIED_EXPLORATORY';
}
export type ParameterScanTrialStatus = 'PENDING' | 'RUNNING' | 'RECONCILING' | 'COMPLETED' | 'FAILED' | 'STOPPED' | 'SKIPPED';
export interface ParameterScanTrial extends ParameterScanPoint {
  taskId: number | null; status: ParameterScanTrialStatus; metric: ParameterScanMetric | null; metrics: Metric[];
  sampleCounts: Record<string, string> | null; error: string | null; remoteStatus: string | null;
  curve?: EquityPoint[]; details?: JsonObject;
}
export type ParameterScanList = ListPage<ParameterScanView>;
export type ParameterScanTrials = ListPage<ParameterScanTrial>;
export interface ParameterScanSurface {
  x: { key: ParameterScanKey; values: string[]; offset: string; total: string };
  y: { key: ParameterScanKey; values: string[]; offset: string; total: string };
  fixed: ParameterValues; metricId: ParameterScanMetricId;
  cells: { ordinal: string; x: string; y: string; status: ParameterScanTrialStatus; metric: ParameterScanMetric | null; reason: string | null }[];
}
