import { useEffect } from 'react';
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query';
import type { Page, PortfolioView, ResearchDetail, ResearchSummary } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
export function useResource<T>(path: string, options: { enabled?: boolean; poll?: number } = {}) {
  return useQuery({ queryKey: [path], queryFn: ({ signal }) => api<T>(path, { signal }), enabled: options.enabled ?? true, refetchInterval: options.poll, refetchIntervalInBackground: false });
}
function useLiveResource<T>(path: string, event: string) {
  const cache = useQueryClient();
  const query = useQuery({ queryKey: [path], queryFn: ({ signal }) => api<T>(path, { signal }),
    staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false });
  useEffect(() => {
    // Attach after the initial read settles. The stream sends a fresh value at
    // attachment, so an older in-flight GET cannot overwrite a newer event.
    if (!query.isSuccess) return;
    const events = new EventSource(`/api/v3${path}/events`);
    const changed = (event: MessageEvent<string>) => {
      try { cache.setQueryData([path], JSON.parse(event.data) as T); }
      catch { events.close(); void cache.invalidateQueries({ queryKey: [path] }); }
    };
    events.addEventListener(event, changed as EventListener);
    return () => { events.removeEventListener(event, changed as EventListener); events.close(); };
  }, [cache, path, event, query.isSuccess]);
  return query;
}
export function useResearchDetail(id: string) { return useLiveResource<ResearchDetail>(`/research/${encodeURIComponent(id)}`, 'research'); }
export function usePortfolioDetail(id: string) { return useLiveResource<PortfolioView>(`/portfolios/${encodeURIComponent(id)}`, 'portfolio'); }
export function useStudies() {
  const query = useInfiniteQuery({ queryKey: ['/research'], initialPageParam: undefined as Record<string, unknown> | undefined,
    queryFn: ({ pageParam, signal }) => api<Page<ResearchSummary>>(`/research${pageParam ? `?cursor=${encodeURIComponent(JSON.stringify(pageParam))}` : ''}`, { signal }),
    getNextPageParam: page => page.cursor, refetchInterval: 7000, refetchIntervalInBackground: false });
  const records = new Map<number, ResearchSummary>();
  for (const page of query.data?.pages ?? []) for (const item of page.items) if (!records.has(item.id)) records.set(item.id, item);
  return { ...query, items: [...records.values()] };
}
