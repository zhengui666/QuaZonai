import type { ResearchRuntime } from '../src/research-runtime.ts';
import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { realpath } from 'node:fs/promises';
import { object } from '../src/trials.ts';
import { NativeEvidencePart, readNativeEvidenceParts } from './native-evidence.ts';
import { isTickResearchInput, nativeResearchRequest } from '../../../packages/contracts/src/tick-input.ts';

// Scientific exposure facts use the upstream session transaction, not a new
// task/session database. A fork or a new conversation cannot rewind this doc.
export const Exposures = defineDoc<{ windows: JsonObject[] }>({
  kind: 'qz.native-exposure', version: 1, scope: 'session', initial: () => ({ windows: [] }),
});

export function partitionBoundaries(request: JsonObject, catalog = false): void {
  const selection = catalog ? request : request.input !== undefined ? object(request.input, 'native_tick_input') : object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  const boundaries = [selection.event_start_ns, request.train_end_ns, request.validation_end_ns, selection.event_end_ns].map((value) => {
    if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('native_partition_boundaries');
    return BigInt(value);
  });
  const [start, train, validation, end] = boundaries as [bigint, bigint, bigint, bigint];
  if (!(start < train && train < validation && validation <= end)) throw new Error('native_partition_boundaries');
}

export async function footprint(data: JsonObject): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const ensemble = data.catalog_ensemble_request !== undefined;
  const catalog = ensemble || data.catalog_scan_request !== undefined;
  if ([data.catalog_ensemble_request, data.catalog_scan_request, data.request].filter(value => value !== undefined).length > 1) throw new Error('ambiguous_factor_request');
  const request = nativeResearchRequest(data);
  const tick = isTickResearchInput(data);
  partitionBoundaries(ensemble ? { ...request, train_end_ns: data.train_end_ns!, validation_end_ns: data.validation_end_ns! } : request, catalog);
  const selection = catalog ? request : tick ? object(request.input, 'native_tick_input') : object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  if (!tick && !catalog && (!Array.isArray(selection.bar_types) || !selection.bar_types.every((id) => typeof id === 'string'))) throw new Error('bar_types_required');
  // Paths, source revisions and modalities do not create a fresh historical sample.
  // Compare conservative instrument/time lineage in the same durable ledger.
  return { catalog: await realpath(data.catalog),
    ...(tick ? { modality: 'TRADE_TICK', instrumentIds: [...selection.instrument_ids as string[]].sort() }
      : catalog ? { barTypesRef: object(data.catalog_scope, 'catalog_scope') } : { barTypes: [...selection.bar_types as string[]].sort() }),
    start: ensemble ? data.validation_end_ns! : request.validation_end_ns!, end: selection.event_end_ns! };
}

export async function checkExposure(api: ResearchRuntime, context: Context, data: JsonObject, reserve: boolean, finalData?: JsonObject, evidenceMode?: 'HISTORICAL_REASSESSMENT'): Promise<void> {
  const window = await footprint(data);
  const historical = evidenceMode === 'HISTORICAL_REASSESSMENT';
  const kind = historical ? (reserve ? 'HISTORICAL_FINAL' : 'HISTORICAL_ITERATION') : (reserve ? 'FINAL' : 'ITERATION');
  const ensemble = data.catalog_ensemble_request !== undefined;
  const request = nativeResearchRequest(data);
  const selection = ensemble || data.catalog_scan_request !== undefined ? request : isTickResearchInput(data) ? object(request.input, 'native_tick_input') : object(object(request.forecast, 'forecast').selection, 'selection');
  if (!reserve) {
    window.start = selection.event_start_ns!;
    window.end = ensemble ? data.validation_end_ns! : request.validation_end_ns!;
  }
  // Iteration reads only the clipped prefix. Keep the original-range check
  // against final exposures so a new root cannot reset an exposed study.
  const end = finalData?.catalog_ensemble_request !== undefined ? object(finalData.catalog_ensemble_request, 'native_request').event_end_ns!
    : finalData ? (await footprint(finalData)).end! : selection.event_end_ns!;
  const checked = reserve ? window : { ...window, end };
  await api.commit(async (tx) => {
    const ledger = await tx.doc(Exposures);
    const scopes = new Map<string, Set<string>>();
    async function assets(item: JsonObject): Promise<Set<string>> {
      if (Array.isArray(item.instrumentIds) && item.instrumentIds.every(id => typeof id === 'string')) return new Set(item.instrumentIds as string[]);
      if (Array.isArray(item.barTypes)) return new Set(item.barTypes as string[]);
      const reference = object(item.barTypesRef, 'bar_types_reference');
      const key = `${reference.conversationId}:${reference.taskId}`;
      let scope = scopes.get(key);
      if (!scope) {
        const record = await tx.task(reference.taskId as TaskId<JsonObject>);
        if (record?.state.status !== 'terminal' || record.state.outcome.status !== 'completed') throw new Error('native_catalog_scope_missing');
        const original = await readNativeEvidenceParts(object(object(record.state.outcome.result, 'scope_result').native, 'scope_native'), async (index) =>
          (await tx.doc(NativeEvidencePart, record.conversationId, `${record.id}:${index}`, null)).data);
        if (!Array.isArray(original.barTypes) || !original.barTypes.every((type) => typeof type === 'string')) throw new Error('native_catalog_scope_invalid');
        scope = new Set(original.barTypes as string[]); scopes.set(key, scope);
      }
      return scope;
    }
    const overlaps = (start: bigint, end: bigint, otherStart: bigint, otherEnd: bigint) => start < otherEnd && otherStart < end;
    const tickWindow = (item: JsonObject) => item.modality === 'TRADE_TICK' || Array.isArray(item.instrumentIds);
    function barSourceStarts(item: JsonObject, types: Set<string>): Map<string, bigint> {
      // Completed BAR labels can reveal earlier underlying trades. These bounds
      // are conservative cross-modality comparisons, never rewritten ledger facts.
      const units: Record<string, bigint> = { MILLISECOND: 1_000_000n, SECOND: 1_000_000_000n,
        MINUTE: 60_000_000_000n, HOUR: 3_600_000_000_000n, DAY: 86_400_000_000_000n,
        WEEK: 604_800_000_000_000n, MONTH: 31n * 86_400_000_000_000n, YEAR: 366n * 86_400_000_000_000n };
      const starts = new Map<string, bigint>();
      for (const type of types) {
        const parsed = /^(.+)-([1-9]\d*)-([A-Z_]+)-[A-Z_]+-(?:EXTERNAL|INTERNAL)$/.exec(type);
        if (!parsed) throw new Error('native_exposure_bar_type_invalid');
        const unit = units[parsed[3]!];
        const lower = unit === undefined ? 0n : BigInt(item.start as string) - BigInt(parsed[2]!) * unit;
        const start = lower < 0n ? 0n : lower;
        const previous = starts.get(parsed[1]!);
        if (previous === undefined || start < previous) starts.set(parsed[1]!, start);
      }
      return starts;
    }
    async function exposureOverlaps(item: JsonObject): Promise<boolean> {
      const leftTick = tickWindow(item); const rightTick = tickWindow(checked);
      const start = BigInt(item.start as string); const end = BigInt(item.end as string);
      const otherStart = BigInt(checked.start as string); const otherEnd = BigInt(checked.end as string);
      if (leftTick === rightTick) {
        // Preserve exact BAR identities and their original label-time admission.
        if (!overlaps(start, end, otherStart, otherEnd)) return false;
        const left = await assets(item); const right = await assets(checked);
        for (const asset of right) if (left.has(asset)) return true;
        return false;
      }
      const bars = leftTick ? checked : item;
      const ticks = leftTick ? item : checked;
      const starts = barSourceStarts(bars, await assets(bars));
      const barEnd = BigInt(bars.end as string);
      for (const id of await assets(ticks)) {
        const barStart = starts.get(id);
        if (barStart !== undefined && overlaps(barStart, barEnd, BigInt(ticks.start as string), BigInt(ticks.end as string))) return true;
      }
      return false;
    }
    let previous: JsonObject | undefined;
    for (const item of ledger.windows) {
      if (!(historical || reserve || item.kind !== 'ITERATION')
        || historical && item.conversationId === api.conversationId
        || item.taskId === api.taskId && item.kind === kind) continue;
      if (await exposureOverlaps(item)) { previous = item; break; }
    }
    if (previous && !historical) throw new Error('heldout_already_exposed');
    if (historical && !previous) throw new Error('historical_exposure_required');
    if (!ledger.windows.some((item) => item.taskId === api.taskId && item.kind === kind)) {
      // Owner-bound reassessment adds permanent, explicitly non-independent facts; it never rewrites prior exposure.
      ledger.windows.push({ ...window, kind, taskId: api.taskId, conversationId: api.conversationId,
        ...(historical ? { evidenceMode, priorExposureTaskIds: previous ? [previous.taskId!] : [] } : {}) });
    }
  }, context);
}
