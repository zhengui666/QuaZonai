import type { Context, JsonValue } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { Requests, Study } from './documents.ts';
import type { ResearchRuntime } from './research-runtime.ts';

export async function studyInput(api: ResearchRuntime, context: Context) {
  const study = await api.snapshot(Study, api.conversationId, context);
  if (!study?.input) throw new Error('research_input_missing');
  return study.input;
}

export function object(value: JsonValue | undefined, name: string): JsonObject {
  if (value === null || !value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${name}_required`);
  return value;
}

/** Recover the registered identity from the original intake, not a catalog path guess. */
export async function studyDatasetId(api: ResearchRuntime, context: Context): Promise<string | null> {
  const requests = await api.snapshot(Requests, context);
  for (const request of Object.values(requests?.requests ?? {})) {
    if (request.conversationId !== api.conversationId || request.recovery) continue;
    const dataset = request.metadata?.dataset;
    if (dataset && typeof dataset === 'object' && !Array.isArray(dataset) && typeof dataset.id === 'string') return dataset.id;
    const original = request.metadata?.original;
    if (original && typeof original === 'object' && !Array.isArray(original) && typeof original.datasetId === 'string') return original.datasetId;
  }
  return null;
}
