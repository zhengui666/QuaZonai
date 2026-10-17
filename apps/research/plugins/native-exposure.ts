import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject, ToolExecutionApi } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { realpath } from 'node:fs/promises';
import { object } from '../src/trials.ts';

// Scientific exposure facts use the upstream session transaction, not a new
// task/session database. A fork or a new conversation cannot rewind this doc.
export const Exposures = defineDoc<{ windows: JsonObject[] }>({
  kind: 'qz.native-exposure', version: 1, scope: 'session', initial: () => ({ windows: [] }),
});

export function partitionBoundaries(request: JsonObject): void {
  const selection = object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  const boundaries = [selection.event_start_ns, request.train_end_ns, request.validation_end_ns, selection.event_end_ns].map((value) => {
    if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('native_partition_boundaries');
    return BigInt(value);
  });
  const [start, train, validation, end] = boundaries as [bigint, bigint, bigint, bigint];
  if (!(start < train && train < validation && validation <= end)) throw new Error('native_partition_boundaries');
}

export async function footprint(data: JsonObject): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const request = object(data.request, 'native_request');
  partitionBoundaries(request);
  const selection = object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  if (!Array.isArray(selection.bar_types) || !selection.bar_types.every((id) => typeof id === 'string')) throw new Error('bar_types_required');
  // Paths and owner labels are provenance, not freshness. The same native BAR
  // identity and event interval stay exposed across labels, copies and revisions.
  return { catalog: await realpath(data.catalog), barTypes: [...selection.bar_types].sort(), start: request.validation_end_ns!, end: selection.event_end_ns! };
}

function overlaps(left: JsonObject, right: JsonObject): boolean {
  const assets = left.barTypes as string[];
  return (right.barTypes as string[]).some((asset) => assets.includes(asset))
    && BigInt(left.start as string) < BigInt(right.end as string) && BigInt(right.start as string) < BigInt(left.end as string);
}

export async function checkExposure(api: ToolExecutionApi, context: Context, data: JsonObject, reserve: boolean, finalData?: JsonObject, evidenceMode?: 'HISTORICAL_REASSESSMENT'): Promise<void> {
  const window = await footprint(data);
  const historical = evidenceMode === 'HISTORICAL_REASSESSMENT';
  const kind = historical ? (reserve ? 'HISTORICAL_FINAL' : 'HISTORICAL_ITERATION') : (reserve ? 'FINAL' : 'ITERATION');
  const selection = object(object(object(data.request, 'request').forecast, 'forecast').selection, 'selection');
  if (!reserve) {
    window.start = selection.event_start_ns!;
    window.end = object(data.request, 'request').validation_end_ns!;
  }
  // Iteration reads only the clipped prefix. Keep the original-range check
  // against final exposures so a new root cannot reset an exposed study.
  const end = finalData ? (await footprint(finalData)).end! : selection.event_end_ns!;
  const checked = reserve ? window : { ...window, end };
  await api.commit(async (tx) => {
    const ledger = await tx.doc(Exposures);
    const previous = ledger.windows.find((item) => (historical || reserve || item.kind !== 'ITERATION')
      && (!historical || item.conversationId !== api.conversationId)
      && !(item.taskId === api.taskId && item.kind === kind) && overlaps(item, checked));
    if (previous && !historical) throw new Error('heldout_already_exposed');
    if (historical && !previous) throw new Error('historical_exposure_required');
    if (!ledger.windows.some((item) => item.taskId === api.taskId && item.kind === kind)) {
      // Owner-bound reassessment adds permanent, explicitly non-independent facts; it never rewrites prior exposure.
      ledger.windows.push({ ...window, kind, taskId: api.taskId, conversationId: api.conversationId,
        ...(historical ? { evidenceMode, priorExposureTaskIds: previous ? [previous.taskId!] : [] } : {}) });
    }
  }, context);
}
