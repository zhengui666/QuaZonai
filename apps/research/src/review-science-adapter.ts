import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { NativeJson, NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { ReviewMetric } from '../../../packages/contracts/src/review.ts';
import { canonicalJsonDigest } from './native-json.ts';
import { executeNativeValue } from './native-execution.ts';

const METHOD = 'qz.review-science.v1';
const DEFINITIONS: Record<string, [string, string]> = {
  candidate_net_return_mean: ['account.net_return_mean', 'RETURN_RATIO'], candidate_net_return_lower: ['account.net_return_lower', 'RETURN_RATIO'],
  return_coverage: ['data.coverage', 'RATIO'],
  coverage: ['data.coverage', 'RATIO'], rank_ic: ['factor.rank_ic', 'CORRELATION'],
  rank_ic_lower: ['factor.rank_ic_lower', 'CORRELATION'], rank_ic_upper: ['factor.rank_ic_upper', 'CORRELATION'],
  quantile_spread: ['factor.quantile_spread', 'RETURN_RATIO'], quantile_monotonicity: ['factor.quantile_monotonicity', 'RATIO'],
  bootstrap_blocks: ['factor.bootstrap_blocks', 'COUNT'],
  positive_slice_fraction: ['robustness.positive_fraction', 'RATIO'], dsr: ['selection.dsr', 'PROBABILITY'], pbo: ['selection.pbo', 'PROBABILITY'],
  net_return_delta: ['incremental.net_return_delta', 'RETURN_RATIO'], net_return_delta_lower: ['incremental.net_return_delta_lower', 'RETURN_RATIO'],
  net_return_delta_upper: ['incremental.net_return_delta_upper', 'RETURN_RATIO'], return_correlation: ['incremental.return_correlation', 'CORRELATION'],
  drawdown_reduction: ['incremental.drawdown_reduction', 'RETURN_RATIO'], sharpe_delta: ['incremental.sharpe_delta', 'RATIO'],
};
function record(value: unknown): Record<string, unknown> | null { return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null; }

/** Parses only the known scientific producer's structured outputs. A model cannot supply these facts. */
export function projectReviewScienceMetrics(value: unknown, evidenceRef: string): ReviewMetric[] {
  const result = record(value);
  if (!result || result.method_version !== METHOD || !Number.isSafeInteger(result.observations) || Number(result.observations) < 0 || !Array.isArray(result.metrics)) return [];
  if (!['POOLED_TIME_SERIES', 'CROSS_SECTIONAL', 'SELECTION_COHORT', 'SHARED_ACCOUNT'].includes(String(result.scope))) return [];
  const seen = new Set<string>();
  return result.metrics.map(item => {
    const metric = record(item); const code = String(metric?.code); const definition = DEFINITIONS[code];
    if (!metric || !definition || seen.has(code)) throw new Error('review_native_metric_identity_invalid');
    seen.add(code);
    const status: ReviewMetric['status'] = metric.status === 'AVAILABLE' ? 'AVAILABLE' : metric.status === 'INSUFFICIENT_EVIDENCE' ? 'INSUFFICIENT' : metric.status === 'INVALID_INPUT' ? 'INVALID' : metric.status === 'UNSUPPORTED' ? 'UNSUPPORTED' : 'INVALID';
    if (status === 'AVAILABLE' && (typeof metric.value !== 'number' || !Number.isFinite(metric.value)) || status !== 'AVAILABLE' && metric.value !== null) throw new Error('review_native_metric_value_invalid');
    const [key, unit] = definition;
    return { key, unit, scope: code === 'coverage' || code === 'return_coverage' ? 'DATA' : String(result.scope), value: status === 'AVAILABLE' ? metric.value as number : null, status, observationCount: Number(result.observations), method: METHOD,
      frequency: 'REGISTERED', evidenceRef, ...(status !== 'AVAILABLE' ? { reason: typeof metric.reason === 'string' ? metric.reason : 'native_metric_unavailable' } : {}) };
  });
}

/** A summary retains original partition diagnostics; absent historical/tick/scan rows stay absent. */
export function nativeFactorReviewMetrics(native: unknown, evidenceRef: string, partition: 'VALIDATION' | 'HELDOUT' = 'VALIDATION'): ReviewMetric[] {
  const original = record(native); if (!original || !Array.isArray(original.partitions)) return [];
  const rows = original.partitions.map(record).filter(row => row?.partition === partition);
  if (rows.length !== 1) return [];
  return projectReviewScienceMetrics(rows[0]?.diagnostics, `${evidenceRef}#${partition.toLowerCase()}`);
}

/** The caller supplies immutable stored native evidence and its frozen target, never agent arguments. */
export async function runReviewScience(input: { source: JsonObject; analysis: JsonObject; evidenceRef: string; target: NativeTarget; context: Context }): Promise<{ native: JsonObject; metrics: ReviewMetric[] }> {
  const sourceDigest = `sha256:${canonicalJsonDigest(input.source as NativeJson)}`;
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-review-'));
  try {
    const native = await executeNativeValue(input.target, 'review-science', { schema_version: 1, source_digest: sourceDigest, analysis: input.analysis }, [], directory, input.context);
    const result = record(native);
    if (!result || result.schema_version !== 1 || result.source_digest !== sourceDigest || result.method_version !== METHOD) throw new Error('review_native_source_mismatch');
    return { native: native as JsonObject, metrics: projectReviewScienceMetrics(result, input.evidenceRef) };
  } finally { await rm(directory, { recursive: true, force: true }); }
}
