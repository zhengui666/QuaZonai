import { setTimeout as delay } from 'node:timers/promises';
import type { PortfolioView, ResearchDetail } from '../../../packages/contracts/src/index.ts';
import type { Client } from './client.ts';

// Only the original native submission proves settlement. An explicit observation
// window detaches the client; it never cancels admitted research.
export type ObservedResearch = ResearchDetail & { submission?: { id: number; status: string } | null };
export async function observe(client: Client, id: number, seconds?: number, emit?: (value: unknown) => void, signal?: AbortSignal) {
  const window = seconds === undefined ? undefined : AbortSignal.timeout(seconds * 1000);
  const observation = signal && window ? AbortSignal.any([signal, window]) : signal ?? window;
  let last: ObservedResearch | null = null;
  try {
    while (!observation?.aborted) {
      last = await client.request<ObservedResearch>('GET', `/research/${id}`, undefined, observation);
      emit?.(last);
      if (last.submission?.status === 'done' || last.submission?.status === 'unanswered') return { conversationId: id, submissionId: last.submission.id, submissionStatus: last.submission.status, settled: true, research: last };
      await delay(500, undefined, { signal: observation });
    }
  } catch (error) { if (!observation?.aborted) throw error; }
  return { conversationId: id, settled: false, watchEnded: signal?.aborted ? 'detached' : 'timeout', nativeSubmissionAvailable: !!last?.submission, cancellationRequested: false, research: last };
}

// Reading a running portfolio observes the same admitted work; it never submits it again.
export async function observePortfolio(client: Client, id: string, seconds?: number, emit?: (value: unknown) => void, signal?: AbortSignal) {
  const window = seconds === undefined ? undefined : AbortSignal.timeout(seconds * 1000);
  const observation = signal && window ? AbortSignal.any([signal, window]) : signal ?? window;
  let last: PortfolioView | null = null;
  try {
    while (!observation?.aborted) {
      last = await client.request<PortfolioView>('GET', `/portfolios/${encodeURIComponent(id)}`, undefined, observation);
      emit?.(last);
      if (['COMPUTED', 'FAILED', 'STOPPED'].includes(last.status)) return { portfolioId: id, settled: true, portfolio: last };
      await delay(1000, undefined, { signal: observation });
    }
  } catch (error) { if (!observation?.aborted) throw error; }
  return { portfolioId: id, settled: false, watchEnded: signal?.aborted ? 'detached' : 'timeout', cancellationRequested: false, portfolio: last };
}
