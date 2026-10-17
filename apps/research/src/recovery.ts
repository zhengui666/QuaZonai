import type { SubmissionRecord } from '@earendil-works/pi-durable';

/** Public diagnostic categories only; provider-originated text is never returned. */
export function modelErrorCode(message: string): string {
  if (/insufficient_quota|quota.*exhaust|billing|credit.*(?:balance|exhaust)/i.test(message)) return 'model_quota_exhausted';
  if (/\b401\b|\b403\b|unauthorized|invalid.?api.?key|authentication/i.test(message)) return 'model_authentication_failed';
  if (/context[_ ](?:length|window)|maximum context|too many tokens|prompt.*too long/i.test(message)) return 'model_context_overflow';
  if (/\b429\b|rate.?limit|too many requests/i.test(message)) return 'model_rate_limited';
  if (/\b(?:400|413|422)\b|invalid.request|unsupported/i.test(message)) return 'model_request_rejected';
  if (/\b(?:500|502|503|504)\b|timeout|timed out|ECONNRESET|connection|fetch failed/i.test(message)) return 'model_transport_error';
  return 'model_error';
}

export function submissionView(value: SubmissionRecord | null) {
  return value ? { id: value.id as number, status: value.status,
    ...(value.requestId ? { requestId: value.requestId } : {}),
    ...(value.status === 'unanswered' ? { reason: value.reason } : {}) } : null;
}

export const recoveryRequestId = (id: number, previous: number) => `qz.research.resume:${id}:${previous}`;
export const recoveryInput = (previous: number) => `The owner requests continuation of this unfinished original study after model submission ${previous} failed. Keep the original model, thinking level, objective, complete data binding, exposure facts, scientific criteria and cumulative trial ledger/budget. Read research_brief and existing research_trial results, then continue genuine registered trials; never recompute merely to read, reset history or use final held-out results for tuning. The old failed submission and task remain historical facts, not successful or retried receipts.`;
