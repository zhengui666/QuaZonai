import { useState } from 'react';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AUTH_SESSION_REVALIDATION_INTERVAL_MS,
  AuthGate,
  useOperatorAuth,
} from './AuthGate';

function jsonResponse(value: unknown, status = 200): Promise<Response> {
  return Promise.resolve(new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  }));
}

function emptyResponse(status = 204): Promise<Response> {
  return Promise.resolve(new Response(null, { status }));
}

function LogoutProbe() {
  const { logout } = useOperatorAuth();
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <div>Workbench ready</div>
      <button
        onClick={() => {
          void logout().catch((reason: unknown) => {
            setError(reason instanceof Error ? reason.message : 'Sign out failed.');
          });
        }}
        type="button"
      >
        Sign out probe
      </button>
      {error ? <div role="alert">{error}</div> : null}
    </>
  );
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('AuthGate', () => {
  it('renders the workbench immediately when a browser session is valid', async () => {
    vi.stubGlobal('fetch', vi.fn(() => jsonResponse({
      authenticated: true,
      username: 'operator',
      trusted_browser: false,
      auth_enabled: true,
    })));

    render(<AuthGate><div>Workbench ready</div></AuthGate>);

    expect(await screen.findByText('Workbench ready')).toBeInTheDocument();
  });

  it('preserves direct access when operator authentication is disabled', async () => {
    vi.stubGlobal('fetch', vi.fn(() => jsonResponse({
      authenticated: true,
      username: 'local-operator',
      trusted_browser: false,
      auth_enabled: false,
    })));

    render(<AuthGate><div>Direct workbench</div></AuthGate>);

    expect(await screen.findByText('Direct workbench')).toBeInTheDocument();
    act(() => window.dispatchEvent(new Event('quazonai:auth-required')));
    expect(screen.getByText('Direct workbench')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Verify your identity' })).not.toBeInTheDocument();
  });

  it('shows password, authenticator code, and trusted-browser option when anonymous', async () => {
    vi.stubGlobal('fetch', vi.fn(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401)));

    render(<AuthGate><div>Workbench ready</div></AuthGate>);

    expect(await screen.findByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();
    expect(screen.getByLabelText('Username')).toBeInTheDocument();
    expect(screen.getByLabelText('Password')).toBeInTheDocument();
    expect(screen.getByLabelText('Authenticator code')).toBeInTheDocument();
    expect(screen.getByText('Trust this browser')).toBeInTheDocument();
  });

  it('submits trusted-browser intent and reveals the workbench after successful login', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401))
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'operator',
        trusted_browser: true,
        auth_enabled: true,
      }));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    render(<AuthGate><div>Workbench ready</div></AuthGate>);

    await user.type(await screen.findByLabelText('Username'), 'operator');
    await user.type(screen.getByLabelText('Password'), 'correct horse battery staple');
    await user.type(screen.getByLabelText('Authenticator code'), '123456');
    await user.click(screen.getByText('Trust this browser'));
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    const loginOptions = fetchMock.mock.calls[1]?.[1] as RequestInit;
    expect(JSON.parse(String(loginOptions.body))).toEqual({
      username: 'operator',
      password: 'correct horse battery staple',
      totp_code: '123456',
      trust_browser: true,
    });
    expect(await screen.findByText('Workbench ready')).toBeInTheDocument();
  });

  it('enters the login gate only after logout succeeds', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'operator',
        trusted_browser: true,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => emptyResponse());
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    render(<AuthGate><LogoutProbe /></AuthGate>);

    await user.click(await screen.findByRole('button', { name: 'Sign out probe' }));

    expect(await screen.findByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('keeps the authenticated workbench when logout is rejected', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'operator',
        trusted_browser: true,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => jsonResponse({
        error: { message: 'The request origin is not allowed.' },
      }, 403));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    render(<AuthGate><LogoutProbe /></AuthGate>);

    await user.click(await screen.findByRole('button', { name: 'Sign out probe' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('The request origin is not allowed.');
    expect(screen.getByText('Workbench ready')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Verify your identity' })).not.toBeInTheDocument();
  });

  it('periodically revalidates an enabled session outside the dashboard route', async () => {
    const setIntervalSpy = vi.spyOn(window, 'setInterval');
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'operator',
        trusted_browser: false,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401));
    vi.stubGlobal('fetch', fetchMock);

    render(<AuthGate><div>Alpha library cache</div></AuthGate>);

    expect(await screen.findByText('Alpha library cache')).toBeInTheDocument();
    const revalidate = setIntervalSpy.mock.calls.find(
      ([, delay]) => delay === AUTH_SESSION_REVALIDATION_INTERVAL_MS,
    )?.[0];
    expect(typeof revalidate).toBe('function');
    await act(async () => {
      if (typeof revalidate === 'function') revalidate();
      await Promise.resolve();
    });

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();
  });
});
