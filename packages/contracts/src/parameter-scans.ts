import type { EquityPoint, JsonObject, ListPage, Metric } from './index.ts';
import { addDecimal, compareDecimal, multiplyDecimal, subtractDecimal } from './pm-validation.ts';

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

export class ParameterScanError extends Error {
  readonly code: string;
  readonly path: string;
  constructor(code: string, path: string) { super(`${code}: ${path}`); this.code = code; this.path = path; }
}
function fail(code: string, path: string): never { throw new ParameterScanError(code, path); }
function record(value: unknown, path: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('parameter_scan_object_required', path);
  return value as Record<string, unknown>;
}
function fields(value: Record<string, unknown>, allowed: readonly string[], path: string) {
  for (const key of Object.keys(value)) if (!allowed.includes(key)) fail('parameter_scan_field_unsupported', `${path}.${key}`);
}
function text(value: unknown, path: string): string {
  if (typeof value !== 'string' || !value.trim()) fail('parameter_scan_text_required', path);
  return value;
}
function expandedNumber(value: string): string {
  if (!/[eE]/.test(value)) return value;
  const match = /^([+-]?)(\d*)(?:\.(\d*))?[eE]([+-]?\d+)$/.exec(value);
  if (!match || !(match[2] || match[3])) throw new Error('invalid_number');
  const exponent = Number(match[4]); if (!Number.isSafeInteger(exponent)) throw new Error('invalid_number');
  const whole = match[2]!, fraction = match[3] ?? '', digits = whole + fraction;
  const point = whole.length + exponent;
  return (match[1] === '-' ? '-' : '') + (point <= 0 ? `0.${'0'.repeat(-point)}${digits}` : point >= digits.length ? digits + '0'.repeat(point - digits.length) : `${digits.slice(0, point)}.${digits.slice(point)}`);
}
function scalar(_key: ParameterScanKey, value: unknown, path: string): string {
  try { return addDecimal(expandedNumber(text(value, path)), '0'); } catch { return fail('parameter_scan_decimal_required', path); }
}
function decimalParts(value: string): { coefficient: bigint; scale: number } {
  const negative = value.startsWith('-'); const [whole, fraction = ''] = value.replace(/^[+-]/, '').split('.');
  return { coefficient: BigInt((whole || '0') + fraction) * (negative ? -1n : 1n), scale: fraction.length };
}
function rangeLength(start: string, end: string, step: string): bigint {
  const difference = decimalParts(subtractDecimal(end, start)); const increment = decimalParts(step);
  return difference.coefficient * 10n ** BigInt(increment.scale) / (increment.coefficient * 10n ** BigInt(difference.scale)) + 1n;
}
function normalizeAxis(value: unknown, _mode: ParameterScanMode, index: number): NormalizedParameterAxis {
  const path = `axes.${index}`; const axis = record(value, path); fields(axis, ['key', 'domain'], path);
  const key = text(axis.key, `${path}.key`);
  if (key.split('.').some(part => !part || ['__proto__', 'prototype', 'constructor'].includes(part))) fail('parameter_scan_axis_unsupported', `${path}.key`); const domain = record(axis.domain, `${path}.domain`);
  if (domain.kind === 'values') {
    fields(domain, ['kind', 'values'], `${path}.domain`);
    if (!Array.isArray(domain.values) || domain.values.length === 0) fail('parameter_scan_values_required', `${path}.domain.values`);
    const values: string[] = []; const seen = new Set<string>();
    for (const [i, item] of domain.values.entries()) {
      const normalized = scalar(key, item, `${path}.domain.values.${i}`);
      const identity = normalized;
      if (!seen.has(identity)) { seen.add(identity); values.push(normalized); }
    }
    return { key, domain: { kind: 'values', values }, count: String(values.length), duplicates: String(domain.values.length - values.length) };
  }
  if (domain.kind !== 'range') fail('parameter_scan_domain_unsupported', `${path}.domain.kind`);
  fields(domain, ['kind', 'start', 'end', 'step'], `${path}.domain`);
  const start = scalar(key, domain.start, `${path}.domain.start`); const end = scalar(key, domain.end, `${path}.domain.end`);
  let step: string;
  try { const value = text(domain.step, `${path}.domain.step`); step = addDecimal(expandedNumber(value), '0'); } catch { return fail('parameter_scan_step_invalid', `${path}.domain.step`); }
  if (compareDecimal(step, '0') <= 0 || compareDecimal(start, end) > 0) fail('parameter_scan_range_invalid', `${path}.domain`);
  const count = rangeLength(start, end, step);
  const normalized: NormalizedParameterAxis = { key, domain: { kind: 'range', start, end, step }, count: count.toString(), duplicates: '0' };
  return normalized;
}
function parsePlan(value: unknown, requireRequest: boolean): ParameterScanStart {
  const input = record(value, 'scan'); fields(input, ['requestId', 'name', 'source', 'mode', 'axes', 'objective', 'execution'], 'scan');
  const source = record(input.source, 'source'); let parsedSource: ParameterScanSource;
  if (source.kind !== 'research') return fail('parameter_scan_source_unsupported', 'source.kind');
  fields(source, ['kind', 'researchId'], 'source');
  if (!Number.isSafeInteger(source.researchId) || Number(source.researchId) < 1) fail('parameter_scan_source_invalid', 'source.researchId');
  parsedSource = { kind: 'research', researchId: source.researchId as number };
  if (input.mode !== 'STRATEGY') fail('parameter_scan_mode_unsupported', 'mode');
  const mode = input.mode;
  if (!Array.isArray(input.axes) || input.axes.length === 0) fail('parameter_scan_axes_required', 'axes');
  const axes = input.axes.map((axis, i) => normalizeAxis(axis, mode, i));
  if (new Set(axes.map(axis => axis.key)).size !== axes.length) fail('parameter_scan_duplicate_axis', 'axes');
  const objective = record(input.objective, 'objective'); fields(objective, ['metricId', 'partition', 'direction'], 'objective');
  const selected = parameterScanObjective(text(objective.metricId, 'objective.metricId'));
  if (!selected || objective.partition !== 'FULL_SELECTION' || !['maximize', 'minimize'].includes(String(objective.direction))) fail('parameter_scan_objective_unsupported', 'objective');
  const execution = input.execution === undefined ? { concurrency: 1 } : record(input.execution, 'execution'); fields(execution, ['concurrency'], 'execution');
  if (!Number.isSafeInteger(execution.concurrency) || Number(execution.concurrency) < 1) fail('parameter_scan_concurrency_invalid', 'execution.concurrency');
  return { requestId: requireRequest ? text(input.requestId, 'requestId') : typeof input.requestId === 'string' ? input.requestId : '',
    name: text(input.name, 'name'), source: parsedSource, mode,
    axes: input.axes as ParameterAxis[], objective: { metricId: selected.metricId, partition: 'FULL_SELECTION', direction: objective.direction as 'maximize' | 'minimize' }, execution: { concurrency: execution.concurrency as number } };
}
export function parameterScanStart(value: unknown): ParameterScanStart { return parsePlan(value, true); }
export function parameterScanPlan(value: unknown): ParameterScanPlan { const { requestId: _requestId, ...plan } = parsePlan(value, false); return plan; }

export function parameterScanAxisValue(axis: NormalizedParameterAxis, index: bigint): string {
  if (index < 0n || index >= BigInt(axis.count)) return fail('parameter_scan_axis_index_invalid', axis.key);
  return axis.domain.kind === 'values' ? axis.domain.values[Number(index)]! : addDecimal(axis.domain.start, multiplyDecimal(axis.domain.step, index.toString()));
}
export function normalizeParameterScan(plan: ParameterScanPlan, defaults: ParameterValues): ParameterScanGrid {
  const axes = plan.axes.map((axis, i) => normalizeAxis(axis, plan.mode, i));
  const total = axes.reduce((product, axis) => product * BigInt(axis.count), 1n).toString();
  return { axes, fixed: { ...defaults }, total, eligible: total, skipped: '0' };
}
export function parameterScanPoint(grid: ParameterScanGrid, ordinal: string): ParameterScanPoint {
  if (!/^(0|[1-9]\d*)$/.test(ordinal) || BigInt(ordinal) >= BigInt(grid.total)) return fail('parameter_scan_ordinal_invalid', 'ordinal');
  let index = BigInt(ordinal); const parameters = { ...grid.fixed };
  for (let i = grid.axes.length - 1; i >= 0; i--) {
    const axis = grid.axes[i]!; const count = BigInt(axis.count); parameters[axis.key] = parameterScanAxisValue(axis, index % count); index /= count;
  }
  return { ordinal, parameters, skipReason: null };
}

/** Validate JSON-number transport, not strategy policy. Exact string leaves never use this. */
export function assertDistinctNativeNumbers(axis: NormalizedParameterAxis): void {
  const reject = () => fail('parameter_scan_native_number_indistinguishable', axis.key);
  const finite = (text: string) => { const value = Number(text); if (!Number.isFinite(value)) fail('parameter_scan_native_number_not_representable', axis.key); return value; };
  if (axis.domain.kind === 'values') {
    const seen = new Set<number>();
    for (const text of axis.domain.values) { const value = finite(text); if (seen.has(value)) reject(); seen.add(value); }
    return;
  }
  const view = new DataView(new ArrayBuffer(8));
  const bits = (value: number) => { view.setFloat64(0, value === 0 ? 0 : value); return view.getBigUint64(0); };
  const sign = 1n << 63n, mask = (1n << 64n) - 1n;
  const rank = (value: number) => { const raw = bits(value); return raw & sign ? ((~raw) & mask) + 1n : raw + sign; };
  const bucket = (value: number) => bits(value) >> 52n;
  const at = (index: bigint) => finite(parameterScanAxisValue(axis, index));
  const step = decimalParts(axis.domain.step);
  const compareUlp = (value: number) => {
    const exponentBits = Number((bits(value) >> 52n) & 2047n);
    const exponent = exponentBits === 0 ? -1074 : exponentBits - 1023 - 52;
    const left = step.coefficient * (exponent < 0 ? 1n << BigInt(-exponent) : 1n);
    const right = 10n ** BigInt(step.scale) * (exponent > 0 ? 1n << BigInt(exponent) : 1n);
    return left < right ? -1 : left > right ? 1 : 0;
  };
  const check = (lo: bigint, hi: bigint): void => {
    if (lo >= hi) { at(lo); return; }
    const first = at(lo), last = at(hi);
    if (first === last) reject();
    if (hi - lo === 1n) return;
    if (bucket(first) === bucket(last)) {
      // Within one IEEE-754 binade, ULP is constant. For steps below ULP,
      // each adjacent output advances at most one rank: endpoint rank count
      // therefore detects any duplicate without enumerating a huge grid.
      const comparison = compareUlp(first);
      if (comparison < 0) { if (rank(last) - rank(first) < hi - lo) reject(); return; }
      if (comparison > 0) return;
      // At exactly one ULP, ties-to-even can alternate duplicate / double steps.
      if (first === at(lo + 1n) || at(lo + 1n) === at(lo + 2n)) reject();
      return;
    }
    const middle = (lo + hi) / 2n;
    check(lo, middle); check(middle, hi);
  };
  check(0n, BigInt(axis.count) - 1n);
}
