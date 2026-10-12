import { setTimeout as delay } from 'node:timers/promises';
import type { ResearchDetail } from '../../../packages/contracts/src/index.ts';
import type { Client } from './client.ts';

// A service may project the ORIGINAL native submission with the detail. No
// fallback to final/status/empty tasks: those are not native settlement facts.
export type ObservedResearch = ResearchDetail & { submission?: { id: number; status: string } | null };
export async function observe(client: Client, id: number, seconds: number, emit?: (value: unknown) => void, signal?: AbortSignal) {
  const deadline = Date.now() + seconds * 1000;
  let last: ObservedResearch | null = null;
  try {
    do {
      const remaining = deadline - Date.now();
      if (remaining <= 0 || signal?.aborted) break;
      const timeout = AbortSignal.timeout(remaining);
      last = await client.request<ObservedResearch>('GET', `/research/${id}`, undefined, signal ? AbortSignal.any([signal, timeout]) : timeout);
      emit?.(last);
      if (last.submission?.status === 'done' || last.submission?.status === 'unanswered') return { conversationId: id, submissionId: last.submission.id, submissionStatus: last.submission.status, settled: true, research: last };
      await delay(Math.min(500, Math.max(0, deadline - Date.now())), undefined, { signal });
    } while (Date.now() < deadline);
  } catch (error) {
    if (!signal?.aborted && Date.now() < deadline) throw error;
  }
  return { conversationId: id, settled: false, watchEnded: signal?.aborted ? 'detached' : 'timeout', nativeSubmissionAvailable: !!last?.submission, cancellationRequested: false, research: last };
}
