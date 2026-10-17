import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import type { Page, ResearchSummary } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
export function useResource<T>(path: string, options: { enabled?: boolean; poll?: number } = {}) {
  return useQuery({ queryKey: [path], queryFn: ({ signal }) => api<T>(path, { signal }), enabled: options.enabled ?? true, refetchInterval: options.poll, refetchIntervalInBackground: false });
}
export function useStudies() {
  const query = useInfiniteQuery({ queryKey: ['/research'], initialPageParam: undefined as Record<string, unknown> | undefined,
    queryFn: ({ pageParam, signal }) => api<Page<ResearchSummary>>(`/research${pageParam ? `?cursor=${encodeURIComponent(JSON.stringify(pageParam))}` : ''}`, { signal }),
    getNextPageParam: page => page.cursor, refetchInterval: 7000, refetchIntervalInBackground: false });
  const records = new Map<number, ResearchSummary>();
  for (const page of query.data?.pages ?? []) for (const item of page.items) if (!records.has(item.id)) records.set(item.id, item);
  return { ...query, items: [...records.values()] };
}
