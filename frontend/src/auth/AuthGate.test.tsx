import { type ReactNode, useState } from 'react';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AUTH_SESSION_REVALIDATION_INTERVAL_MS,
  AuthGate,
  useOperatorAuth,
} from './AuthGate';
import { I18nProvider, localeLabels, localeOrder, useI18n, type Locale } from '../i18n';

function renderAuthGate(children: ReactNode, locale: Locale = 'en') {
  return render(
    <I18nProvider initialLocale={locale}>
      <AuthGate>{children}</AuthGate>
    </I18nProvider>,
  );
}

function jsonResponse(value: unknown, status = 200): Promise<Response> {
  return Promise.resolve(new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  }));
}

function emptyResponse(status = 204): Promise<Response> {
  return Promise.resolve(new Response(null, { status }));
}

function deferredResponse(): { promise: Promise<Response>; resolve: (response: Response) => void } {
  let resolve: (response: Response) => void;
  const promise = new Promise<Response>((complete) => { resolve = complete; });
  return { promise, resolve: (response) => resolve(response) };
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

function AuthModeProbe() {
  const { authEnabled } = useOperatorAuth();
  return <div>{authEnabled ? 'Authentication enabled' : 'Direct access enabled'}</div>;
}

function LocaleChangeProbe() {
  const { setLocale } = useI18n();
  return <button onClick={() => setLocale('ar')}>Change locale</button>;
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
      username: 'local-operator',
      trusted_browser: false,
      auth_enabled: true,
    })));

    renderAuthGate(<div>Workbench ready</div>);

    expect(await screen.findByText('Workbench ready')).toBeInTheDocument();
  });

  it('keeps an authenticated session stable when changing locale', async () => {
    const unexpectedRebootstrap = deferredResponse();
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }))
      .mockImplementation(() => unexpectedRebootstrap.promise);
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<><div>Workbench ready</div><LocaleChangeProbe /></>);

    expect(await screen.findByText('Workbench ready')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Change locale' }));
    await waitFor(() => expect(document.documentElement).toHaveAttribute('dir', 'rtl'));
    await act(async () => { await Promise.resolve(); });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Workbench ready')).toBeInTheDocument();
  });

  it('preserves direct access when operator authentication is disabled', async () => {
    const fetchMock = vi.fn(() => jsonResponse({
      authenticated: true,
      username: 'local-operator',
      trusted_browser: false,
      auth_enabled: false,
    }));
    vi.stubGlobal('fetch', fetchMock);

    renderAuthGate(<div>Direct workbench</div>);

    expect(await screen.findByText('Direct workbench')).toBeInTheDocument();
    act(() => window.dispatchEvent(new Event('quazonai:auth-required')));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('Direct workbench')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Verify your identity' })).not.toBeInTheDocument();
  });

  it('rechecks a stale direct-access session after an authentication-required signal', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: false,
      }))
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401));
    vi.stubGlobal('fetch', fetchMock);

    renderAuthGate(<div>Direct workbench</div>);

    expect(await screen.findByText('Direct workbench')).toBeInTheDocument();
    act(() => window.dispatchEvent(new Event('quazonai:auth-required')));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();
  });

  it('ignores an out-of-order direct-access rebootstrap response', async () => {
    const staleDirectSession = deferredResponse();
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: false,
      }))
      .mockImplementationOnce(() => staleDirectSession.promise)
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401));
    vi.stubGlobal('fetch', fetchMock);

    renderAuthGate(<div>Direct workbench</div>);

    expect(await screen.findByText('Direct workbench')).toBeInTheDocument();
    act(() => {
      window.dispatchEvent(new Event('quazonai:auth-required'));
      window.dispatchEvent(new Event('quazonai:auth-required'));
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(3));
    expect(await screen.findByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();

    await act(async () => {
      staleDirectSession.resolve(await jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: false,
      }));
      await Promise.resolve();
    });

    expect(screen.getByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();
    expect(screen.queryByText('Direct workbench')).not.toBeInTheDocument();
  });

  it('cancels an unfinished bootstrap request when unmounted', async () => {
    const bootstrap = deferredResponse();
    const fetchMock = vi.fn<typeof fetch>(() => bootstrap.promise);
    vi.stubGlobal('fetch', fetchMock);

    const view = renderAuthGate(<div>Workbench ready</div>);

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    const options = fetchMock.mock.calls[0]?.[1] as RequestInit;
    const signal = options.signal as AbortSignal;
    expect(signal.aborted).toBe(false);

    view.unmount();
    expect(signal.aborted).toBe(true);

    await act(async () => {
      bootstrap.resolve(await jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }));
      await Promise.resolve();
    });
  });

  it('shows only authenticator code and trusted-browser option when anonymous', async () => {
    vi.stubGlobal('fetch', vi.fn(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401)));

    renderAuthGate(<div>Workbench ready</div>);

    expect(await screen.findByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();
    expect(screen.queryByLabelText('Username')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Password')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Authenticator code')).toHaveFocus();
    expect(screen.getByText('Trust this browser')).toBeInTheDocument();
  });

  it('localizes the login chrome and preserves entry directions in Arabic', async () => {
    vi.stubGlobal('fetch', vi.fn(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401)));

    renderAuthGate(<div>Workbench ready</div>, 'ar');

    expect(await screen.findByRole('heading', { name: 'تحقق من هويتك' })).toBeInTheDocument();
    await waitFor(() => expect(document.documentElement).toHaveAttribute('dir', 'rtl'));
    expect(screen.getByLabelText('رمز المصادقة')).toHaveAttribute('dir', 'ltr');
  });

  it('lets an anonymous operator switch the login language before authentication', async () => {
    vi.stubGlobal('fetch', vi.fn(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401)));
    const user = userEvent.setup();

    renderAuthGate(<div>Workbench ready</div>);

    await screen.findByRole('heading', { name: 'Verify your identity' });
    await user.click(screen.getByRole('button', { name: 'Change language: English' }));
    for (const code of localeOrder) {
      expect(screen.getByText(localeLabels[code].native, { selector: `span[lang="${code}"]:not(.qz-section-meta)` })).toHaveAttribute('dir', localeLabels[code].dir);
      const englishLabel = screen.getByText(localeLabels[code].english, { selector: 'span.qz-section-meta' });
      expect(englishLabel).toHaveAttribute('lang', 'en');
      expect(englishLabel).toHaveAttribute('dir', 'ltr');
    }
    await user.click(screen.getByRole('menuitemradio', { name: /العربية/ }));

    await waitFor(() => {
      expect(document.documentElement).toHaveAttribute('lang', 'ar');
      expect(document.documentElement).toHaveAttribute('dir', 'rtl');
      expect(screen.getByRole('heading', { name: 'تحقق من هويتك' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'تغيير اللغة: العربية' })).toBeInTheDocument();
    });
  });

  it('re-renders fallback login errors in the selected locale while preserving API messages', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401))
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'INVALID_CREDENTIALS' } }, 401))
      .mockImplementationOnce(() => jsonResponse({ error: { message: 'Operator locked.' } }, 403));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<div>Workbench ready</div>);
    await user.type(await screen.findByLabelText('Authenticator code'), '123456');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Authentication failed.');
    await user.click(screen.getByRole('button', { name: 'Change language: English' }));
    await user.click(screen.getByRole('menuitemradio', { name: /العربية/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('فشلت المصادقة.');

    await user.click(screen.getByRole('button', { name: 'تسجيل الدخول' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Operator locked.');
  });

  it('normalizes Arabic TOTP digits before login', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401))
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<div>Workbench ready</div>, 'ar');
    const totp = await screen.findByLabelText('رمز المصادقة');
    await user.type(totp, '١٢٣٤٥٦');
    expect(totp).toHaveValue('123456');
    await user.clear(totp);
    await user.type(totp, '۱۲۳۴۵۶');
    expect(totp).toHaveValue('123456');

    await user.click(screen.getByRole('button', { name: 'تسجيل الدخول' }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    const loginOptions = fetchMock.mock.calls[1]?.[1] as RequestInit;
    expect(JSON.parse(String(loginOptions.body))).toMatchObject({ totp_code: '123456' });
  });

  it('submits trusted-browser intent and reveals the workbench after successful login', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401))
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: true,
        auth_enabled: true,
      }));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<div>Workbench ready</div>);
    await user.type(await screen.findByLabelText('Authenticator code'), '123456');
    await user.click(screen.getByText('Trust this browser'));
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    const loginOptions = fetchMock.mock.calls[1]?.[1] as RequestInit;
    expect(JSON.parse(String(loginOptions.body))).toEqual({
      totp_code: '123456',
      trust_browser: true,
    });
    expect(await screen.findByText('Workbench ready')).toBeInTheDocument();
  });

  it('requires a complete six-digit authenticator code before submitting', async () => {
    const fetchMock = vi.fn(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<div>Workbench ready</div>);

    const totp = await screen.findByLabelText('Authenticator code');
    const signIn = screen.getByRole('button', { name: 'Sign in' });
    expect(signIn).toBeDisabled();
    await user.type(totp, '12345');
    expect(signIn).toBeDisabled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await user.type(totp, '6');
    expect(signIn).toBeEnabled();
  });

  it('disables the login controls while one submission is pending', async () => {
    const login = deferredResponse();
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401))
      .mockImplementationOnce(() => login.promise);
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<div>Workbench ready</div>);

    const totp = await screen.findByLabelText('Authenticator code');
    await user.type(totp, '123456');
    const signIn = screen.getByRole('button', { name: 'Sign in' });
    await user.click(signIn);

    expect(signIn).toBeDisabled();
    expect(totp).toBeDisabled();
    expect(fetchMock).toHaveBeenCalledTimes(2);

    await act(async () => {
      login.resolve(await jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }));
      await Promise.resolve();
    });
  });

  it('enters the login gate only after logout succeeds', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: true,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => emptyResponse());
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<LogoutProbe />);

    await user.click(await screen.findByRole('button', { name: 'Sign out probe' }));

    expect(await screen.findByRole('heading', { name: 'Verify your identity' })).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('keeps the authenticated workbench when logout is rejected', async () => {
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: true,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => jsonResponse({
        error: { message: 'The request origin is not allowed.' },
      }, 403));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<LogoutProbe />);

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
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401));
    vi.stubGlobal('fetch', fetchMock);

    renderAuthGate(<div>Alpha library cache</div>);

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

  it('periodically revalidates a direct-access session when authentication becomes enabled', async () => {
    const setIntervalSpy = vi.spyOn(window, 'setInterval');
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: false,
      }))
      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401));
    vi.stubGlobal('fetch', fetchMock);

    renderAuthGate(<div>Alpha library cache</div>);

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

  it('adopts direct-access mode after a successful periodic revalidation', async () => {
    const setIntervalSpy = vi.spyOn(window, 'setInterval');
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: false,
      }));
    vi.stubGlobal('fetch', fetchMock);

    renderAuthGate(<AuthModeProbe />);

    expect(await screen.findByText('Authentication enabled')).toBeInTheDocument();
    const revalidate = setIntervalSpy.mock.calls.find(
      ([, delay]) => delay === AUTH_SESSION_REVALIDATION_INTERVAL_MS,
    )?.[0];
    expect(typeof revalidate).toBe('function');
    await act(async () => {
      if (typeof revalidate === 'function') revalidate();
      await Promise.resolve();
    });

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('Direct access enabled')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Verify your identity' })).not.toBeInTheDocument();
  });

  it('ignores an older successful periodic revalidation response', async () => {
    const setIntervalSpy = vi.spyOn(window, 'setInterval');
    const staleRevalidation = deferredResponse();
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => staleRevalidation.promise)
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }));
    vi.stubGlobal('fetch', fetchMock);

    renderAuthGate(<AuthModeProbe />);

    expect(await screen.findByText('Authentication enabled')).toBeInTheDocument();
    const revalidate = setIntervalSpy.mock.calls.find(
      ([, delay]) => delay === AUTH_SESSION_REVALIDATION_INTERVAL_MS,
    )?.[0];
    expect(typeof revalidate).toBe('function');
    await act(async () => {
      if (typeof revalidate === 'function') {
        revalidate();
        revalidate();
      }
      await Promise.resolve();
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(3));

    await act(async () => {
      staleRevalidation.resolve(await jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: false,
      }));
      await Promise.resolve();
    });

    expect(screen.getByText('Authentication enabled')).toBeInTheDocument();
    expect(screen.queryByText('Direct access enabled')).not.toBeInTheDocument();
  });

  it('ignores a stale revalidation failure after logout and re-login', async () => {
    const setIntervalSpy = vi.spyOn(window, 'setInterval');
    const staleRevalidation = deferredResponse();
    const fetchMock = vi.fn()
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }))
      .mockImplementationOnce(() => staleRevalidation.promise)
      .mockImplementationOnce(() => emptyResponse())
      .mockImplementationOnce(() => jsonResponse({
        authenticated: true,
        username: 'local-operator',
        trusted_browser: false,
        auth_enabled: true,
      }));
    vi.stubGlobal('fetch', fetchMock);
    const user = userEvent.setup();

    renderAuthGate(<LogoutProbe />);

    await screen.findByText('Workbench ready');
    const revalidate = setIntervalSpy.mock.calls.find(
      ([, delay]) => delay === AUTH_SESSION_REVALIDATION_INTERVAL_MS,
    )?.[0];
    expect(typeof revalidate).toBe('function');
    act(() => { if (typeof revalidate === 'function') revalidate(); });
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));

    await user.click(screen.getByRole('button', { name: 'Sign out probe' }));
    await user.type(await screen.findByLabelText('Authenticator code'), '123456');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));
    expect(await screen.findByText('Workbench ready')).toBeInTheDocument();

    await act(async () => {
      staleRevalidation.resolve(new Response(null, { status: 401 }));
      await Promise.resolve();
    });

    expect(screen.getByText('Workbench ready')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Verify your identity' })).not.toBeInTheDocument();
  });
});
