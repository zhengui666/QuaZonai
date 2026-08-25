import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AuthGate } from './AuthGate';

function jsonResponse(value: unknown, status = 200): Promise<Response> {
  return Promise.resolve(new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  }));
}

afterEach(() => {
  cleanup();
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
});
