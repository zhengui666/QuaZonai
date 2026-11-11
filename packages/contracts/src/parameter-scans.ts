import type { EquityPoint, JsonObject, ListPage, Metric } from './index.ts';
import { addDecimal, compareDecimal, multiplyDecimal, subtractDecimal } from './pm-validation.ts';

export type ParameterScanMode = 'FACTOR' | 'STRATEGY';
export type ParameterScanKey = 'fast_period' | 'slow_period' | 'threshold' | 'investedWeight' | 'targetTtlNs' | 'maximum_asset_weight' | 'participation_fraction';
export type ParameterValues = Partial<Record<ParameterScanKey, string>>;
export type ParameterScanSource = { kind: 'factor'; factorId: string } | { kind: 'research-trial'; researchId: number; trialId: number };
export type ParameterDomain = { kind: 'values'; values: string[] } | { kind: 'range'; start: string; end: string; step: string };
export interface ParameterAxis { key: ParameterScanKey; domain: ParameterDomain }
export type ParameterScanMetricId = 'factor.pearson_ic' | 'account.sharpe252' | 'account.cagr' | 'account.maxDrawdown' | 'account.sharpe365' | 'account.cagr365';
export interface ParameterScanObjective { metricId: ParameterScanMetricId; partition: 'VALIDATION'; direction: 'maximize' }
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
  key: ParameterScanKey; label: string; kind: 'integer' | 'number' | 'decimal' | 'nanoseconds'; modes: ParameterScanMode[]; modalities?: ('BAR' | 'TRADE_TICK')[];
}
export interface ParameterScanObjectiveDefinition {
  metricId: ParameterScanMetricId; label: string; mode: ParameterScanMode; partition: 'VALIDATION'; direction: 'maximize'; nativeKey: string; unit: string; modalities?: ('BAR' | 'TRADE_TICK')[];
}
export const parameterScanParameters: readonly ParameterScanParameterDefinition[] = [
  { key: 'fast_period', label: '快 EMA 周期', kind: 'integer', modes: ['FACTOR', 'STRATEGY'] },
  { key: 'slow_period', label: '慢 EMA 周期', kind: 'integer', modes: ['FACTOR', 'STRATEGY'] },
  { key: 'threshold', modalities: ['BAR'], label: '信号阈值', kind: 'number', modes: ['STRATEGY'] },
  { key: 'investedWeight', label: '投入比例', kind: 'decimal', modes: ['STRATEGY'] },
  { key: 'targetTtlNs', modalities: ['BAR'], label: '目标有效期（纳秒）', kind: 'nanoseconds', modes: ['STRATEGY'] },
  { key: 'maximum_asset_weight', label: '单资产最大权重', kind: 'decimal', modes: ['STRATEGY'], modalities: ['TRADE_TICK'] },
  { key: 'participation_fraction', label: '成交参与比例', kind: 'decimal', modes: ['STRATEGY'], modalities: ['TRADE_TICK'] },
];
export const parameterScanObjectives: readonly ParameterScanObjectiveDefinition[] = [
  { metricId: 'factor.pearson_ic', label: '验证集 Pearson IC', mode: 'FACTOR', partition: 'VALIDATION', direction: 'maximize', nativeKey: 'pearson_ic', unit: 'RATIO' },
  { metricId: 'account.sharpe252', modalities: ['BAR'], label: '原生夏普比率（252 天）', mode: 'STRATEGY', partition: 'VALIDATION', direction: 'maximize', nativeKey: 'Sharpe Ratio (252 days)', unit: 'RATIO' },
  { metricId: 'account.cagr', modalities: ['BAR'], label: '原生 CAGR（252 天）', mode: 'STRATEGY', partition: 'VALIDATION', direction: 'maximize', nativeKey: 'CAGR (252 days)', unit: 'RATIO' },
  { metricId: 'account.maxDrawdown', label: '原生最大回撤（负值，越接近零越好）', mode: 'STRATEGY', partition: 'VALIDATION', direction: 'maximize', nativeKey: 'Max Drawdown', unit: 'RATIO' },
  { metricId: 'account.sharpe365', label: '原生夏普比率（365 天）', mode: 'STRATEGY', partition: 'VALIDATION', direction: 'maximize', nativeKey: 'sharpe_365', unit: 'RATIO', modalities: ['TRADE_TICK'] },
  { metricId: 'account.cagr365', label: '原生 CAGR（365 天）', mode: 'STRATEGY', partition: 'VALIDATION', direction: 'maximize', nativeKey: 'annualized_return_365', unit: 'RATIO', modalities: ['TRADE_TICK'] },
];

export interface ParameterScanSourceView {
  source: ParameterScanSource; label: string; researchId: number; trialId: number; memberIndexes: number[];
  datasetId: string | null; datasetName: string | null; modality: 'BAR' | 'TRADE_TICK'; currency: string | null;
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
export interface ParameterScanMetric extends Metric { value: number | null; reason: string | null }
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
  if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)[eE][+-]?\d+$/.test(value) || !Number.isFinite(Number(value))) throw new Error('invalid_number');
  const rendered = String(Number(value));
  if (!/[eE]/.test(rendered)) return rendered;
  const negative = rendered.startsWith('-');
  const [coefficient, exponent] = rendered.replace(/^-/, '').split('e');
  const [whole, fraction = ''] = coefficient!.split('.'); const digits = whole! + fraction;
  const point = whole!.length + Number(exponent);
  const decimal = point <= 0 ? `0.${'0'.repeat(-point)}${digits}` : point >= digits.length ? digits + '0'.repeat(point - digits.length) : `${digits.slice(0, point)}.${digits.slice(point)}`;
  return (negative ? '-' : '') + decimal;
}
function scalar(key: ParameterScanKey, value: unknown, path: string): string {
  const input = text(value, path);
  if (key === 'fast_period' || key === 'slow_period' || key === 'targetTtlNs') {
    if (!/^\d+$/.test(input)) fail('parameter_scan_integer_required', path);
    const n = BigInt(input); const minimum = key === 'slow_period' ? 2n : 1n;
    if (n < minimum || n > (key === 'targetTtlNs' ? 9223372036854775807n : 4294967295n)) fail('parameter_scan_value_invalid', path);
    return n.toString();
  }
  let canonical: string;
  try { canonical = addDecimal(key === 'threshold' ? expandedNumber(input) : input, '0'); } catch { return fail('parameter_scan_decimal_required', path); }
  if (key === 'threshold') {
    if (!Number.isFinite(Number(canonical))) fail('parameter_scan_value_invalid', path);
  } else if (compareDecimal(canonical, '0') <= 0 || compareDecimal(canonical, '1') > 0 || (canonical.split('.')[1]?.length ?? 0) > 18) {
    fail('parameter_scan_value_invalid', path);
  }
  return canonical;
}
function decimalParts(value: string): { coefficient: bigint; scale: number } {
  const negative = value.startsWith('-'); const [whole, fraction = ''] = value.replace(/^[+-]/, '').split('.');
  return { coefficient: BigInt((whole || '0') + fraction) * (negative ? -1n : 1n), scale: fraction.length };
}
function rangeLength(start: string, end: string, step: string): bigint {
  const difference = decimalParts(subtractDecimal(end, start)); const increment = decimalParts(step);
  return difference.coefficient * 10n ** BigInt(increment.scale) / (increment.coefficient * 10n ** BigInt(difference.scale)) + 1n;
}
function normalizeAxis(value: unknown, mode: ParameterScanMode, index: number): NormalizedParameterAxis {
  const path = `axes.${index}`; const axis = record(value, path); fields(axis, ['key', 'domain'], path);
  const definition = parameterScanParameters.find(item => item.key === axis.key && item.modes.includes(mode));
  if (!definition) fail('parameter_scan_axis_unsupported', `${path}.key`);
  const key = definition.key; const domain = record(axis.domain, `${path}.domain`);
  if (domain.kind === 'values') {
    fields(domain, ['kind', 'values'], `${path}.domain`);
    if (!Array.isArray(domain.values) || domain.values.length === 0) fail('parameter_scan_values_required', `${path}.domain.values`);
    const values: string[] = []; const seen = new Set<string>();
    for (const [i, item] of domain.values.entries()) {
      const normalized = scalar(key, item, `${path}.domain.values.${i}`);
      const identity = key === 'threshold' ? String(Number(normalized)) : normalized;
      if (!seen.has(identity)) { seen.add(identity); values.push(normalized); }
    }
    return { key, domain: { kind: 'values', values }, count: String(values.length), duplicates: String(domain.values.length - values.length) };
  }
  if (domain.kind !== 'range') fail('parameter_scan_domain_unsupported', `${path}.domain.kind`);
  fields(domain, ['kind', 'start', 'end', 'step'], `${path}.domain`);
  const start = scalar(key, domain.start, `${path}.domain.start`); const end = scalar(key, domain.end, `${path}.domain.end`);
  let step: string;
  try { const value = text(domain.step, `${path}.domain.step`); step = addDecimal(key === 'threshold' ? expandedNumber(value) : value, '0'); } catch { return fail('parameter_scan_step_invalid', `${path}.domain.step`); }
  if (compareDecimal(step, '0') <= 0 || compareDecimal(start, end) > 0
    || ['integer', 'nanoseconds'].includes(definition.kind) && step.includes('.')) fail('parameter_scan_range_invalid', `${path}.domain`);
  const count = rangeLength(start, end, step);
  const normalized: NormalizedParameterAxis = { key, domain: { kind: 'range', start, end, step }, count: count.toString(), duplicates: '0' };
  if (definition.kind === 'decimal' && (step.split('.')[1]?.length ?? 0) > 18) fail('parameter_scan_value_invalid', `${path}.domain.step`);
  if (key === 'threshold' && count > 1n) {
    // A range may cross a binary exponent boundary between its endpoints.
    // Require the exact decimal step to cover the widest f64 spacing anywhere
    // in the interval, rather than only checking its first/last adjacent pair.
    const bits = new DataView(new ArrayBuffer(8));
    bits.setFloat64(0, Math.max(Math.abs(Number(start)), Math.abs(Number(end))));
    const exponentBits = Number((bits.getBigUint64(0) >> 52n) & 2047n);
    const exponent = exponentBits === 0 ? -1074 : exponentBits - 1023 - 52;
    const increment = decimalParts(step); const denominator = 10n ** BigInt(increment.scale);
    // Strictly wider also covers ties-to-even when decimal values fall halfway
    // between adjacent floats. Exact equality can still repeat a rounded value.
    const distinct = exponent >= 0 ? increment.coefficient > (1n << BigInt(exponent)) * denominator
      : increment.coefficient * (1n << BigInt(-exponent)) > denominator;
    if (!distinct) fail('parameter_scan_float_step_indistinguishable', `${path}.domain.step`);
  }
  return normalized;
}
function parsePlan(value: unknown, requireRequest: boolean): ParameterScanStart {
  const input = record(value, 'scan'); fields(input, ['requestId', 'name', 'source', 'mode', 'axes', 'objective', 'execution'], 'scan');
  const source = record(input.source, 'source'); let parsedSource: ParameterScanSource;
  if (source.kind === 'factor') {
    fields(source, ['kind', 'factorId'], 'source'); parsedSource = { kind: 'factor', factorId: text(source.factorId, 'source.factorId') };
  } else if (source.kind === 'research-trial') {
    fields(source, ['kind', 'researchId', 'trialId'], 'source');
    for (const key of ['researchId', 'trialId']) if (!Number.isSafeInteger(source[key]) || Number(source[key]) < 1) fail('parameter_scan_source_invalid', `source.${key}`);
    parsedSource = { kind: 'research-trial', researchId: source.researchId as number, trialId: source.trialId as number };
  } else return fail('parameter_scan_source_unsupported', 'source.kind');
  if (input.mode !== 'FACTOR' && input.mode !== 'STRATEGY') fail('parameter_scan_mode_unsupported', 'mode');
  const mode = input.mode;
  if (!Array.isArray(input.axes) || input.axes.length === 0) fail('parameter_scan_axes_required', 'axes');
  const axes = input.axes.map((axis, i) => normalizeAxis(axis, mode, i));
  if (new Set(axes.map(axis => axis.key)).size !== axes.length) fail('parameter_scan_duplicate_axis', 'axes');
  const objective = record(input.objective, 'objective'); fields(objective, ['metricId', 'partition', 'direction'], 'objective');
  const selected = parameterScanObjectives.find(item => item.metricId === objective.metricId && item.mode === mode);
  if (!selected || objective.partition !== 'VALIDATION' || objective.direction !== 'maximize') fail('parameter_scan_objective_unsupported', 'objective');
  const execution = input.execution === undefined ? { concurrency: 1 } : record(input.execution, 'execution'); fields(execution, ['concurrency'], 'execution');
  if (!Number.isSafeInteger(execution.concurrency) || Number(execution.concurrency) < 1) fail('parameter_scan_concurrency_invalid', 'execution.concurrency');
  return { requestId: requireRequest ? text(input.requestId, 'requestId') : typeof input.requestId === 'string' ? input.requestId : '',
    name: text(input.name, 'name'), source: parsedSource, mode,
    axes: input.axes as ParameterAxis[], objective: { metricId: selected.metricId, partition: 'VALIDATION', direction: 'maximize' }, execution: { concurrency: execution.concurrency as number } };
}
export function parameterScanStart(value: unknown): ParameterScanStart { return parsePlan(value, true); }
export function parameterScanPlan(value: unknown): ParameterScanPlan { const { requestId: _requestId, ...plan } = parsePlan(value, false); return plan; }

export function parameterScanAxisValue(axis: NormalizedParameterAxis, index: bigint): string {
  if (index < 0n || index >= BigInt(axis.count)) return fail('parameter_scan_axis_index_invalid', axis.key);
  return axis.domain.kind === 'values' ? axis.domain.values[Number(index)]! : addDecimal(axis.domain.start, multiplyDecimal(axis.domain.step, index.toString()));
}
function singleton(key: ParameterScanKey, value: string): NormalizedParameterAxis { return { key, domain: { kind: 'values', values: [value] }, count: '1', duplicates: '0' }; }
function ceilDiv(a: bigint, b: bigint): bigint { return a <= 0n ? a / b : (a + b - 1n) / b; }
function clamp(value: bigint, maximum: bigint): bigint { return value < 0n ? 0n : value > maximum ? maximum : value; }
// Sum floor((a*i+b)/m) for nonnegative integers, without enumerating a range.
function floorSum(n: bigint, m: bigint, a: bigint, b: bigint): bigint {
  let sum = 0n;
  for (;;) {
    if (a >= m) { sum += (n - 1n) * n * (a / m) / 2n; a %= m; }
    if (b >= m) { sum += n * (b / m); b %= m; }
    const top = a * n + b; if (top < m) return sum;
    n = top / m; b = top % m; [m, a] = [a, m];
  }
}
function validPeriodPairs(fast: NormalizedParameterAxis, slow: NormalizedParameterAxis): bigint {
  if (fast.domain.kind === 'range' && slow.domain.kind === 'range') {
    const a = BigInt(fast.domain.start), p = BigInt(fast.domain.step), n = BigInt(fast.count);
    const b = BigInt(slow.domain.start), q = BigInt(slow.domain.step), m = BigInt(slow.count);
    const first = clamp(ceilDiv(a + 1n - b, q), m); const full = clamp(ceilDiv(a + p * (n - 1n) + 1n - b, q), m);
    const middle = full - first;
    return (m - full) * n + (middle ? floorSum(middle, p, q, b + q * first - 1n - a) + middle : 0n);
  }
  if (fast.domain.kind === 'values' && slow.domain.kind === 'range') {
    const b = BigInt(slow.domain.start), q = BigInt(slow.domain.step), m = BigInt(slow.count);
    return fast.domain.values.reduce((sum, value) => sum + m - clamp(ceilDiv(BigInt(value) + 1n - b, q), m), 0n);
  }
  if (slow.domain.kind === 'values' && fast.domain.kind === 'range') {
    const a = BigInt(fast.domain.start), p = BigInt(fast.domain.step), n = BigInt(fast.count);
    return slow.domain.values.reduce((sum, value) => sum + clamp(ceilDiv(BigInt(value) - a, p), n), 0n);
  }
  const sorted = (fast.domain as { values: string[] }).values.map(BigInt).sort((a, b) => a < b ? -1 : a > b ? 1 : 0);
  return (slow.domain as { values: string[] }).values.reduce((sum, value) => {
    const limit = BigInt(value); let left = 0, right = sorted.length;
    while (left < right) { const middle = Math.floor((left + right) / 2); if (sorted[middle]! < limit) left = middle + 1; else right = middle; }
    return sum + BigInt(left);
  }, 0n);
}
export function normalizeParameterScan(plan: ParameterScanPlan, defaults: ParameterValues): ParameterScanGrid {
  const axes = plan.axes.map((axis, i) => normalizeAxis(axis, plan.mode, i)); const fixed: ParameterValues = {};
  for (const definition of parameterScanParameters) {
    const value = defaults[definition.key]; if (value !== undefined && definition.modes.includes(plan.mode)) fixed[definition.key] = scalar(definition.key, value, `defaults.${definition.key}`);
  }
  const fast = axes.find(axis => axis.key === 'fast_period') ?? singleton('fast_period', text(fixed.fast_period, 'defaults.fast_period'));
  const slow = axes.find(axis => axis.key === 'slow_period') ?? singleton('slow_period', text(fixed.slow_period, 'defaults.slow_period'));
  const total = axes.reduce((product, axis) => product * BigInt(axis.count), 1n);
  const periodTotal = BigInt(fast.count) * BigInt(slow.count); const eligible = total / periodTotal * validPeriodPairs(fast, slow);
  return { axes, fixed, total: total.toString(), eligible: eligible.toString(), skipped: (total - eligible).toString() };
}
export function parameterScanPoint(grid: ParameterScanGrid, ordinal: string): ParameterScanPoint {
  if (!/^(0|[1-9]\d*)$/.test(ordinal) || BigInt(ordinal) >= BigInt(grid.total)) return fail('parameter_scan_ordinal_invalid', 'ordinal');
  let index = BigInt(ordinal); const parameters = { ...grid.fixed };
  for (let i = grid.axes.length - 1; i >= 0; i--) {
    const axis = grid.axes[i]!; const count = BigInt(axis.count); parameters[axis.key] = parameterScanAxisValue(axis, index % count); index /= count;
  }
  return { ordinal, parameters, skipReason: BigInt(parameters.fast_period!) >= BigInt(parameters.slow_period!) ? 'fast_period_must_be_less_than_slow_period' : null };
}
