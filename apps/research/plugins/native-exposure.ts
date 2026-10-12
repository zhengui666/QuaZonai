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

export async function footprint(data: JsonObject): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const request = object(data.request, 'native_request');
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

export async function checkExposure(api: ToolExecutionApi, context: Context, data: JsonObject, reserve: boolean): Promise<void> {
  const window = await footprint(data);
  const kind = reserve ? 'FINAL' : 'ITERATION';
  const selection = object(object(object(data.request, 'request').forecast, 'forecast').selection, 'selection');
  if (!reserve) {
    window.start = selection.event_start_ns!;
    window.end = object(data.request, 'request').validation_end_ns!;
  }
  // Iteration reads only the clipped prefix. Keep the original-range check
  // against final exposures so a new root cannot reset an exposed study.
  const checked = reserve ? window : { ...window, end: selection.event_end_ns! };
  await api.commit(async (tx) => {
    const ledger = await tx.doc(Exposures);
    const previous = ledger.windows.find((item) => (reserve || item.kind !== 'ITERATION')
      && !(item.taskId === api.taskId && item.kind === kind) && overlaps(item, checked));
    if (previous) throw new Error('heldout_already_exposed');
    if (!ledger.windows.some((item) => item.taskId === api.taskId && item.kind === kind)) {
      ledger.windows.push({ ...window, kind, taskId: api.taskId, conversationId: api.conversationId });
    }
  }, context);
}
