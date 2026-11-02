import { createHash } from 'node:crypto';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { ReviewPolicy } from '../../../packages/contracts/src/review.ts';
import { canonical } from './documents.ts';

/** One version contract shared by the review producer and historical consumers. */
export function reviewTargetVersion(input: JsonObject, trial: JsonObject, trials: readonly JsonObject[], policy: ReviewPolicy): string {
  const original = JSON.parse(JSON.stringify({ data: input.data, specification: input.specification ?? null,
    proposal: trial.proposal, result: trial.result, policy, trialLedger: trials })) as JsonObject;
  return createHash('sha256').update(canonical(original)).digest('hex');
}
