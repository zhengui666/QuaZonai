import { useEffect, useState } from 'react';
import type { ActivityEvent } from './api/types';

const SSE_EVENT_NAME = 'qz-event';

export function useEventStream(limit = 30) {
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const source = new EventSource('/api/v1/events/stream');
    source.onopen = () => setConnected(true);
    source.onerror = () => setConnected(false);

    const receive = (event: MessageEvent<string>) => {
      try {
        const payload = JSON.parse(event.data) as Partial<ActivityEvent>;
        const next: ActivityEvent = {
          id: event.lastEventId || payload.id || crypto.randomUUID(),
          kind: payload.kind ?? 'EVENT',
          created_at: payload.created_at ?? new Date().toISOString(),
          ...payload,
        } as ActivityEvent;
        setEvents((current) => [next, ...current.filter((item) => item.id !== next.id)].slice(0, limit));
      } catch {
        // Ignore malformed SSE frames; the stream remains connected for later valid events.
      }
    };

    source.addEventListener(SSE_EVENT_NAME, receive as EventListener);
    return () => {
      source.removeEventListener(SSE_EVENT_NAME, receive as EventListener);
      source.close();
    };
  }, [limit]);

  return { events, connected };
}
