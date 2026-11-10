import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { ReviewReport } from '../../../packages/contracts/src/review.ts';

/** Original review receipts live on the same official Durable transaction line. */
export const ReviewLedger = defineDoc<{ records: Record<string, JsonObject> }>({
  kind: 'qz.review-ledger', checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, version: 1, scope: 'conversation', history: 'latest', fork: 'initial', initial: () => ({ records: {} }),
});
/** This document belongs exclusively to the isolated reviewer conversation. */
export const ReviewSession = defineDoc<{
  binding: JsonObject | null; evidence: JsonObject | null; reads: string[]; calls: number; finished: boolean;
  narrative: string | null; findings: JsonObject[];
  nativeResults: JsonObject[];
}>({
  kind: 'qz.review-session', checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  initial: () => ({ binding: null, evidence: null, reads: [], calls: 0, finished: false, narrative: null, findings: [], nativeResults: [] }),
});

export function latestTrialReview(ledger: { records: Record<string, JsonObject> } | null | undefined, trialId: number): ReviewReport | null {
  const record = Object.values(ledger?.records ?? {}).find(item => item.trialId === trialId);
  return record?.report && typeof record.report === 'object' && !Array.isArray(record.report) ? record.report as unknown as ReviewReport : null;
}
