import type { Context, JsonValue } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { Study } from './documents.ts';
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
