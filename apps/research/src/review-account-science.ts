import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { ReviewMetric } from '../../../packages/contracts/src/review.ts';
import { canonical } from './documents.ts';
import { runReviewScience } from './review-science-adapter.ts';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { evaluatePortfolio, executeNative } from '../plugins/native-portfolio.ts';
import type { StrategyMember, StrategyPolicy } from './strategy.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
const json = (value: unknown): JsonObject => JSON.parse(JSON.stringify(value)) as JsonObject;
function object(value: unknown, label = 'native_object'): JsonObject { if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(label); return value as JsonObject; }
function counter(value: unknown, label: string): string { if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value)) throw new Error(label); return value; }

/** Fixed registered stress: one whole observed BAR delay plus native adverse slippage. */
export function adverseReviewRequest(request: JsonObject): JsonObject {
  const changed = structuredClone(request); const selection = object(changed.selection); const settings = object(changed.settings);
  const units: Record<string, bigint> = { MILLISECOND: 1_000_000n, SECOND: 1_000_000_000n, MINUTE: 60_000_000_000n, HOUR: 3_600_000_000_000n, DAY: 86_400_000_000_000n, WEEK: 604_800_000_000_000n };
  if (!Array.isArray(selection.bar_types) || !selection.bar_types.length) throw new Error('review_stress_bar_clock_required');
  const delays = selection.bar_types.map(raw => { const match = /-([1-9]\d*)-([A-Z_]+)-[A-Z_]+-(?:EXTERNAL|INTERNAL)$/.exec(String(raw)); if (!match || !units[match[2]!]) throw new Error('review_stress_bar_clock_unsupported'); return BigInt(match[1]!) * units[match[2]!]!; });
  const delay = delays.reduce((maximum, value) => value > maximum ? value : maximum, 0n);
  const latency = object(settings.latency_model); const fill = object(settings.fill_model);
  if (latency.adapter_kind !== 'NAUTILUS_STATIC_LATENCY' || fill.adapter_kind !== 'NAUTILUS_DEFAULT_FILL') throw new Error('review_stress_native_model_unsupported');
  object(latency.parameters).base_latency_ns = (BigInt(counter(object(latency.parameters).base_latency_ns, 'base_latency_ns')) + delay).toString();
  object(fill.parameters).prob_slippage = '1';
  return changed;
}

/** Explicit strategy consumer for factor Review; never normalizes a raw factor into a position. */
export async function replayReviewAccount(input: { data: JsonObject; members: StrategyMember[]; policy: StrategyPolicy; binary: NativeTarget; evidenceRef: string }, context: Context): Promise<{ metrics: ReviewMetric[]; evidence: JsonObject }> {
  if (input.data.catalog_scan_request !== undefined || input.data.catalog_ensemble_request !== undefined) throw new Error('review_explicit_bar_strategy_required');
  const { sealed: _sealed, ...original } = input.data;
  const data = { ...original, request: iterationRequest(input.data, true) };
  const candidate = await evaluatePortfolio({ data, members: input.members, policy: input.policy, binary: input.binary, partition: 'VALIDATION' }, context);
  return replayStoredReviewAccount({ data: input.data, source: candidate.evidence, binary: input.binary, evidenceRef: input.evidenceRef }, context);
}

/** Review an already executed shared-account candidate using its exact frozen target sequence and settings. */
export async function replayStoredReviewAccount(input: { data: JsonObject; source: JsonObject; binary: NativeTarget; evidenceRef: string }, context: Context): Promise<{ metrics: ReviewMetric[]; evidence: JsonObject }> {
  const candidate = object(input.source.simulation, 'review_original_simulation_required');
  const request = structuredClone(object(input.source.simulationRequest, 'review_original_simulation_request_required'));
  const selection = object(request.selection);
  const original = iterationRequest(input.data, true);
  const end = counter(original.validation_end_ns, 'review_original_validation_end');
  if (BigInt(counter(selection.event_end_ns, 'review_original_event_end')) > BigInt(end)
    || BigInt(counter(selection.event_start_ns, 'review_original_event_start')) < BigInt(counter(original.train_end_ns, 'review_original_train_end'))
    || BigInt(counter(selection.decision_cutoff_ns, 'review_original_cutoff')) > BigInt(end)) throw new Error('review_original_account_outside_validation');
  if (input.source.catalog !== undefined && input.source.catalog !== input.data.catalog) throw new Error('review_original_account_catalog_mismatch');
  if (input.source.request && canonical(input.source.request) !== canonical(original)) throw new Error('review_original_account_request_mismatch');
  const cash = structuredClone(request);
  cash.target_points = (cash.target_points as JsonObject[]).map(point => ({ ...point, cash_weight: '1', targets: (point.targets as JsonObject[]).map(target => ({ ...target, weight: '0' })) }));
  const stressRequest = adverseReviewRequest(request);
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-review-account-'));
  try {
    const bridge = object(input.source.datasetBridge ?? {});
    const extra = object(request.settings).multi_currency_spot_cash != null ? ['--dataset-revision-id', String(bridge.id ?? '')] : [];
    if (extra.length && !bridge.id) throw new Error('review_original_account_dataset_bridge_missing');
    const args = ['--catalog', String(input.data.catalog), ...extra];
    const baseline = await executeNative(input.binary, 'simulate', cash, args, directory, context);
    const stress = await executeNative(input.binary, 'simulate', stressRequest, args, directory, context);
    const compared = await accountReviewMetrics({ candidate, baseline, stress, binary: input.binary, evidenceRef: input.evidenceRef, context });
    return { metrics: compared.metrics, evidence: { ...compared.evidence, candidate: input.source, baseline, stress, stressRequest,
      limitations: ['The candidate is its original native shared-account result and exact frozen targets; no models, calibrations or member weights are refitted.',
        'Venue fees remain their original source rates; no unsupported fee multiplier is fabricated.', 'BAR delay/slippage is a registered scenario, not order-book or capacity evidence.'] } };
  } finally { await rm(directory, { recursive: true, force: true }); }
}

/** Only original native daily equity returns; no curve arithmetic or zero-filled days. */
export function nativeReviewReturnPanel(simulation: JsonObject): { times: string[]; returns: number[] } {
  if (simulation.returns_kind !== 'PORTFOLIO_DAILY' || simulation.returns_status !== 'OK' || !Array.isArray(simulation.returns) || simulation.returns.length < 30) throw new Error('review_native_daily_returns_required');
  const rows = simulation.returns.map(row => object(row, 'review_native_return'));
  const times = rows.map(row => counter(row.timestamp_ns, 'review_native_return_time'));
  if (times.some((time, index) => index > 0 && BigInt(time) <= BigInt(times[index-1]!)) || rows.some(row => typeof row.value !== 'number' || !Number.isFinite(row.value) || row.reason_code != null)) throw new Error('review_native_daily_returns_incomplete');
  return { times, returns: rows.map(row => row.value as number) };
}

/** Caller binds the common original account, interval and source settings before invoking. */
export async function accountReviewMetrics(input: { candidate: JsonObject; baseline: JsonObject; stress?: JsonObject; binary: NativeTarget; evidenceRef: string; context: Context }): Promise<{ metrics: ReviewMetric[]; evidence: JsonObject }> {
  const candidate = nativeReviewReturnPanel(input.candidate); const baseline = nativeReviewReturnPanel(input.baseline);
  if (canonical(candidate.times) !== canonical(baseline.times)) throw new Error('review_native_return_alignment_mismatch');
  const analysis = { kind: 'PORTFOLIO', candidate: { id: 'candidate', returns: candidate.returns }, baseline: { id: 'baseline', returns: baseline.returns }, timestamps_ns: candidate.times, frequency: 'DAY', block_length: 5 };
  const compared = await runReviewScience({ source: json({ candidate: input.candidate, baseline: input.baseline }), analysis, evidenceRef: input.evidenceRef, target: input.binary, context: input.context });
  const metrics = [...compared.metrics];
  let stressed: JsonObject | null = null;
  if (input.stress) {
    const stress = nativeReviewReturnPanel(input.stress);
    if (canonical(stress.times) !== canonical(candidate.times)) throw new Error('review_native_stress_alignment_mismatch');
    const result = await runReviewScience({ source: json({ stress: input.stress, baseline: input.baseline }), analysis: { ...analysis, candidate: { id: 'stress', returns: stress.returns } }, evidenceRef: input.evidenceRef, target: input.binary, context: input.context });
    stressed = result.native;
    const mean = result.metrics.find(metric => metric.key === 'account.net_return_mean');
    if (mean) metrics.push({ ...mean, key: 'execution.stressed_net_return' });
  }
  return { metrics, evidence: { paired: compared.native, stress: stressed, method: 'QZ_REVIEW_NATIVE_DAILY_PAIRED_BLOCK5_V1', assumptions: ['Five-day moving blocks; at least 30 complete blocks for intervals.', 'Original shared-account net equity returns only; cash baseline has its own actual native replay.'] } };
}
