import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonValue } from '@earendil-works/chord';
import type { JsonObject, ConversationId } from '@earendil-works/pi-durable';

export type ResearchInput = JsonObject & {
  goal: string;
  specification?: import('../../../packages/contracts/src/index.ts').ResearchSpecification;
  model: { provider: string; modelId: string };
  thinkingLevel?: import('@earendil-works/pi-ai').ModelThinkingLevel;
  serviceTier?: import('../../../packages/contracts/src/index.ts').ServiceTier;
  data: JsonObject;
  policy: JsonObject;
}

// These are research facts, not a second copy of pi.agent/pi.live/pi.inbox/pi.usage.
export const Study = defineDoc<{
  input: ResearchInput | null;
  trials: JsonObject[];
  final: JsonObject | null;
  previousStops?: { previousSubmissionId: number; final: JsonObject }[];
}>({
  kind: 'qz.study', version: 1, scope: 'conversation', history: 'rewindable',
  fork: 'current', initial: () => ({ input: null, trials: [], final: null }),
});

export const Requests = defineDoc<{ requests: Record<string, {
  input: JsonValue; conversationId: ConversationId;
  recovery?: { previousSubmissionId: number; mode: 'model_error' | 'stopped' };
}> }>({
  kind: 'qz.research-requests', version: 1, scope: 'session',
  initial: () => ({ requests: {} }),
});

// Structural equality for command replay, with no content-digest ledger.
export function canonical(value: JsonValue): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key]!)}`).join(',')}}`;
  }
  return JSON.stringify(value);
}
