import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId, ToolExecutionApi } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { realpath } from 'node:fs/promises';
import { object } from '../src/trials.ts';
import { NativeEvidencePart, readNativeEvidenceParts } from './native-evidence.ts';

// Scientific exposure facts use the upstream session transaction, not a new
// task/session database. A fork or a new conversation cannot rewind this doc.
export const Exposures = defineDoc<{ windows: JsonObject[] }>({
  kind: 'qz.native-exposure', version: 1, scope: 'session', initial: () => ({ windows: [] }),
});

export function partitionBoundaries(request: JsonObject, catalog = false): void {
  const selection = catalog ? request : object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  const boundaries = [selection.event_start_ns, request.train_end_ns, request.validation_end_ns, selection.event_end_ns].map((value) => {
    if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('native_partition_boundaries');
    return BigInt(value);
  });
  const [start, train, validation, end] = boundaries as [bigint, bigint, bigint, bigint];
  if (!(start < train && train < validation && validation <= end)) throw new Error('native_partition_boundaries');
}

export async function footprint(data: JsonObject): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const catalog = data.catalog_scan_request !== undefined;
  if (catalog && data.request !== undefined) throw new Error('ambiguous_factor_request');
  const request = object(catalog ? data.catalog_scan_request : data.request, 'native_request');
  partitionBoundaries(request, catalog);
  const selection = catalog ? request : object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  if (!catalog && (!Array.isArray(selection.bar_types) || !selection.bar_types.every((id) => typeof id === 'string'))) throw new Error('bar_types_required');
  // Paths and owner labels are provenance, not freshness. The same native BAR
  // identity and event interval stay exposed across labels, copies and revisions.
  return { catalog: await realpath(data.catalog),
    ...(catalog ? { barTypesRef: object(data.catalog_scope, 'catalog_scope') } : { barTypes: [...selection.bar_types as string[]].sort() }),
    start: request.validation_end_ns!, end: selection.event_end_ns! };
}

export async function checkExposure(api: ToolExecutionApi, context: Context, data: JsonObject, reserve: boolean, finalData?: JsonObject, evidenceMode?: 'HISTORICAL_REASSESSMENT'): Promise<void> {
  const window = await footprint(data);
  const historical = evidenceMode === 'HISTORICAL_REASSESSMENT';
  const kind = historical ? (reserve ? 'HISTORICAL_FINAL' : 'HISTORICAL_ITERATION') : (reserve ? 'FINAL' : 'ITERATION');
  const request = object(data.catalog_scan_request ?? data.request, 'request');
  const selection = data.catalog_scan_request !== undefined ? request : object(object(request.forecast, 'forecast').selection, 'selection');
  if (!reserve) {
    window.start = selection.event_start_ns!;
    window.end = request.validation_end_ns!;
  }
  // Iteration reads only the clipped prefix. Keep the original-range check
  // against final exposures so a new root cannot reset an exposed study.
  const end = finalData ? (await footprint(finalData)).end! : selection.event_end_ns!;
  const checked = reserve ? window : { ...window, end };
  await api.commit(async (tx) => {
    const ledger = await tx.doc(Exposures);
    const scopes = new Map<string, Set<string>>();
    async function assets(item: JsonObject): Promise<Set<string>> {
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
    let previous: JsonObject | undefined;
    for (const item of ledger.windows) {
      if (!(historical || reserve || item.kind !== 'ITERATION')
        || historical && item.conversationId === api.conversationId
        || item.taskId === api.taskId && item.kind === kind
        || BigInt(item.start as string) >= BigInt(checked.end as string)
        || BigInt(checked.start as string) >= BigInt(item.end as string)) continue;
      const left = await assets(item); const right = await assets(checked);
      for (const asset of right) if (left.has(asset)) { previous = item; break; }
      if (previous) break;
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
