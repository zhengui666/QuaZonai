import type { ResearchStart } from '../../../packages/contracts/src/index.ts';

export type CompactResearchStart = Extract<ResearchStart, { goal: string }>;
export type ResearchIntent = { raw: string; value: CompactResearchStart };
export const researchIntentKey = 'quazonai.research.pending.v3';
export function readResearchIntent(storage: Pick<Storage, 'getItem'> = localStorage): ResearchIntent | undefined {
  try {
    const raw = storage.getItem(researchIntentKey);
    if (!raw) return;
    const value = JSON.parse(raw) as CompactResearchStart;
    if (value.requestId && value.goal && value.model && value.datasetId && Number.isInteger(value.maxTrials)) return { raw, value };
  } catch { /* A damaged local draft is not a submitted research request. */ }
}
export function captureResearchIntent(input: Omit<CompactResearchStart, 'requestId'>, storage: Pick<Storage, 'setItem'> = localStorage): ResearchIntent {
  const value: CompactResearchStart = { requestId: crypto.randomUUID(), ...input };
  const raw = JSON.stringify(value);
  storage.setItem(researchIntentKey, raw);
  return { raw, value };
}
export function clearResearchIntent(storage: Pick<Storage, 'removeItem'> = localStorage) { storage.removeItem(researchIntentKey); }
