import { useEffect, useState } from 'react';
import type { ActivityEvent } from './api/types';

const SSE_EVENT_NAME = 'qz-event';

async function recheckOperatorSession(): Promise<void> {
  try {
    const response = await fetch('/api/v1/auth/session', { credentials: 'same-origin' });
    if (response.status === 401) {
      window.dispatchEvent(new Event('quazonai:auth-required'));
    }
  } catch {
    // A transient network failure is not proof that the credential expired.
  }
}

export function useEventStream(limit = 30) {
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const source = new EventSource('/api/v1/events/stream');
    source.onopen = () => setConnected(true);
    source.onerror = () => {
      setConnected(false);
      void recheckOperatorSession();
    };

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
