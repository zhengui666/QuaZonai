import { act, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useEventStream } from '../lib/useEventStream';

class FakeEventSource {
  static instance: FakeEventSource | null = null;

  readonly url: string;
  onopen: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  addEventListener = vi.fn();
  removeEventListener = vi.fn();
  close = vi.fn();

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instance = this;
  }
}

function Probe() {
  const { connected } = useEventStream();
  return <div>{connected ? 'connected' : 'disconnected'}</div>;
}

afterEach(() => {
  FakeEventSource.instance = null;
  vi.unstubAllGlobals();
});

describe('useEventStream authentication recovery', () => {
  it('dispatches auth-required when a failed SSE stream has no valid session', async () => {
    vi.stubGlobal('EventSource', FakeEventSource);
    const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 401 })));
    vi.stubGlobal('fetch', fetchMock);
    const authRequired = vi.fn();
    window.addEventListener('quazonai:auth-required', authRequired);

    render(<Probe />);
    expect(screen.getByText('disconnected')).toBeInTheDocument();

    act(() => {
      FakeEventSource.instance?.onerror?.(new Event('error'));
    });

    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      '/api/v1/auth/session',
      { credentials: 'same-origin' },
    ));
    await waitFor(() => expect(authRequired).toHaveBeenCalledTimes(1));
    window.removeEventListener('quazonai:auth-required', authRequired);
  });

  it('keeps the workbench authenticated when session recheck succeeds', async () => {
    vi.stubGlobal('EventSource', FakeEventSource);
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response(null, { status: 200 }))));
    const authRequired = vi.fn();
    window.addEventListener('quazonai:auth-required', authRequired);

    render(<Probe />);
    act(() => {
      FakeEventSource.instance?.onerror?.(new Event('error'));
    });

    await waitFor(() => expect(FakeEventSource.instance).not.toBeNull());
    await Promise.resolve();
    expect(authRequired).not.toHaveBeenCalled();
    window.removeEventListener('quazonai:auth-required', authRequired);
  });
});
