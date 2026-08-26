import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from 'react';
import { Button, DropdownMenu, Theme } from '@radix-ui/themes';
import { Direction } from 'radix-ui';
import { localeLabels, localeOrder, useI18n, type Locale } from '../i18n';
import '../styles/auth.css';

type AuthState = 'checking' | 'authenticated' | 'anonymous';
type BootstrapError = { kind: 'http'; status: number } | { kind: 'unreachable' };
type LoginError =
  | { kind: 'api'; message: string }
  | { kind: 'fallback'; message: 'auth.authenticationFailed' | 'auth.unreachable' };

export type LogoutFailure =
  | { kind: 'api'; message: string }
  | { kind: 'http'; status: number }
  | { kind: 'unreachable' };

export class LogoutError extends Error {
  readonly failure: LogoutFailure;

  constructor(failure: LogoutFailure) {
    super(failure.kind === 'api' ? failure.message : failure.kind);
    this.name = 'LogoutError';
    this.failure = failure;
  }
}

export function isLogoutError(error: unknown): error is LogoutError {
  return error instanceof LogoutError;
}

interface SessionView {
  authenticated: boolean;
  username: string;
  trusted_browser: boolean;
  auth_enabled: boolean;
}

interface ErrorEnvelope {
  error?: { message?: string };
}

interface OperatorAuthContextValue {
  authEnabled: boolean;
  logout: () => Promise<void>;
}

const OperatorAuthContext = createContext<OperatorAuthContextValue | null>(null);
export const AUTH_SESSION_REVALIDATION_INTERVAL_MS = 30_000;
export const TOTP_CODE_LENGTH = 6;
const TOTP_CODE_PATTERN = `[0-9]{${TOTP_CODE_LENGTH}}`;

export function useOperatorAuth(): OperatorAuthContextValue {
  const value = useContext(OperatorAuthContext);
  if (value === null) throw new Error('useOperatorAuth must be used inside AuthGate');
  return value;
}

async function apiErrorMessage(response: Response): Promise<string | null> {
  try {
    const payload = await response.json() as ErrorEnvelope;
    return payload.error?.message ?? null;
  } catch {
    return null;
  }
}

function normalizeTotpCode(value: string): string {
  return value
    .replace(/[\u0660-\u0669]/g, (digit) => String(digit.charCodeAt(0) - 0x0660))
    .replace(/[\u06f0-\u06f9]/g, (digit) => String(digit.charCodeAt(0) - 0x06f0))
    .replace(/\D/g, '')
    .slice(0, TOTP_CODE_LENGTH);
}

function LoginPage({ onAuthenticated }: { onAuthenticated: (session: SessionView) => void }) {
  const { locale, setLocale, t } = useI18n();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [totpCode, setTotpCode] = useState('');
  const [trustBrowser, setTrustBrowser] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<LoginError | null>(null);
  const changeLocale = (value: string) => {
    if ((localeOrder as readonly string[]).includes(value)) setLocale(value as Locale);
  };
  const errorMessage = error === null
    ? null
    : error.kind === 'api'
      ? error.message
      : t(error.message);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      const response = await fetch('/api/v1/auth/login', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          username,
          password,
          totp_code: totpCode,
          trust_browser: trustBrowser,
        }),
      });
      if (!response.ok) {
        const message = await apiErrorMessage(response);
        setError(message === null
          ? { kind: 'fallback', message: 'auth.authenticationFailed' }
          : { kind: 'api', message });
        return;
      }
      onAuthenticated(await response.json() as SessionView);
    } catch {
      setError({ kind: 'fallback', message: 'auth.unreachable' });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
      <Direction.Provider dir={localeLabels[locale].dir}>
        <main className="qz-auth-page">
          <section className="qz-auth-card" aria-labelledby="qz-auth-title">
            <div className="qz-auth-language">
              <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                  <Button aria-label={`${t('language.change')}: ${localeLabels[locale].native}`} className="qz-auth-language-button" size="1" variant="soft">{localeLabels[locale].short}</Button>
                </DropdownMenu.Trigger>
                <DropdownMenu.Content align="end">
                  <DropdownMenu.RadioGroup value={locale} onValueChange={changeLocale}>
                    {localeOrder.map((code) => (
                      <DropdownMenu.RadioItem key={code} value={code}>
                        <span lang={code} dir={localeLabels[code].dir}>{localeLabels[code].native}</span>
                        <span className="qz-section-meta" lang="en" dir="ltr">{localeLabels[code].english}</span>
                      </DropdownMenu.RadioItem>
                    ))}
                  </DropdownMenu.RadioGroup>
                </DropdownMenu.Content>
              </DropdownMenu.Root>
            </div>
            <div className="qz-auth-mark" aria-hidden="true">QZ</div>
          <div className="qz-auth-heading">
            <p className="qz-auth-eyebrow">{t('auth.operatorAccess')}</p>
            <h1 id="qz-auth-title">{t('auth.verifyIdentity')}</h1>
            <p>{t('auth.loginDescription', { digits: TOTP_CODE_LENGTH })}</p>
          </div>
          <form className="qz-auth-form" onSubmit={submit}>
            <label>
              <span>{t('auth.username')}</span>
              <input
                autoComplete="username"
                dir="auto"
                autoFocus
                disabled={submitting}
                onChange={(event) => setUsername(event.target.value)}
                required
                value={username}
              />
            </label>
            <label>
              <span>{t('auth.password')}</span>
              <input
                autoComplete="current-password"
                dir="ltr"
                disabled={submitting}
                onChange={(event) => setPassword(event.target.value)}
                required
                type="password"
                value={password}
              />
            </label>
            <label>
              <span>{t('auth.authenticatorCode')}</span>
              <input
                autoComplete="one-time-code"
                dir="ltr"
                disabled={submitting}
                inputMode="numeric"
                maxLength={TOTP_CODE_LENGTH}
                onChange={(event) => setTotpCode(normalizeTotpCode(event.target.value))}
                pattern={TOTP_CODE_PATTERN}
                placeholder={'0'.repeat(TOTP_CODE_LENGTH)}
                required
                value={totpCode}
              />
            </label>
            <label className="qz-auth-trust">
              <input
                checked={trustBrowser}
                disabled={submitting}
                onChange={(event) => setTrustBrowser(event.target.checked)}
                type="checkbox"
              />
              <span>
                <strong>{t('auth.trustBrowser')}</strong>
                <small>{t('auth.trustBrowserDescription')}</small>
              </span>
            </label>
            {errorMessage !== null ? <div className="qz-auth-error" dir="auto" role="alert">{errorMessage}</div> : null}
            <button className="qz-auth-submit" disabled={submitting || totpCode.length !== TOTP_CODE_LENGTH} type="submit">
              {submitting ? t('auth.verifying') : t('auth.signIn')}
            </button>
          </form>
            <p className="qz-auth-footnote">{t('auth.logoutFootnote')}</p>
          </section>
        </main>
      </Direction.Provider>
    </Theme>
  );
}

export function AuthGate({ children }: { children: ReactNode }) {
  const { t } = useI18n();
  const [state, setState] = useState<AuthState>('checking');
  const [session, setSession] = useState<SessionView | null>(null);
  const [bootstrapError, setBootstrapError] = useState<BootstrapError | null>(null);
  const sessionCheckGeneration = useRef(0);
  const sessionCheckAbortController = useRef<AbortController | null>(null);

  const invalidateSessionChecks = useCallback(() => {
    sessionCheckGeneration.current += 1;
    sessionCheckAbortController.current?.abort();
    sessionCheckAbortController.current = null;
  }, []);

  const acceptSession = useCallback((nextSession: SessionView) => {
    invalidateSessionChecks();
    setSession(nextSession);
    setState(nextSession.authenticated ? 'authenticated' : 'anonymous');
  }, [invalidateSessionChecks]);

  const checkSession = useCallback(async () => {
    const generation = sessionCheckGeneration.current + 1;
    sessionCheckGeneration.current = generation;
    sessionCheckAbortController.current?.abort();
    const controller = new AbortController();
    sessionCheckAbortController.current = controller;
    const isCurrent = () => sessionCheckGeneration.current === generation;
    setBootstrapError(null);
    try {
      const response = await fetch('/api/v1/auth/session', {
        credentials: 'same-origin',
        signal: controller.signal,
      });
      if (!isCurrent()) return;
      if (response.ok) {
        const nextSession = await response.json() as SessionView;
        if (isCurrent()) acceptSession(nextSession);
        return;
      }
      if (response.status === 401) {
        setSession(null);
        setState('anonymous');
        return;
      }
      setBootstrapError({ kind: 'http', status: response.status });
      setSession(null);
      setState('anonymous');
    } catch {
      if (!isCurrent()) return;
      setBootstrapError({ kind: 'unreachable' });
      setSession(null);
      setState('anonymous');
    } finally {
      if (isCurrent()) sessionCheckAbortController.current = null;
    }
  }, [acceptSession]);

  const logout = useCallback(async () => {
    if (!session?.auth_enabled) return;
    try {
      const response = await fetch('/api/v1/auth/logout', {
        method: 'POST',
        credentials: 'same-origin',
      });
      if (!response.ok) {
        const message = await apiErrorMessage(response);
        throw new LogoutError(message === null
          ? { kind: 'http', status: response.status }
          : { kind: 'api', message });
      }
    } catch (error) {
      if (isLogoutError(error)) throw error;
      throw new LogoutError({ kind: 'unreachable' });
    }
    invalidateSessionChecks();
    setSession(null);
    setState('anonymous');
  }, [invalidateSessionChecks, session?.auth_enabled]);

  useEffect(() => {
    void checkSession();
    return invalidateSessionChecks;
  }, [checkSession, invalidateSessionChecks]);
  useEffect(() => {
    // Direct-access tabs need this too: the backend can be restarted with
    // Operator Authentication enabled while a static page has no SSE or
    // other request that would surface the new login requirement.
    if (state !== 'authenticated') return;
    let active = true;
    const revalidateSession = async () => {
      // A slow earlier probe must never overwrite a newer revalidation, logout,
      // or bootstrap result. Share the same monotonic generation as bootstrap
      // checks so every session-derived UI update has last-result-wins semantics.
      const generation = sessionCheckGeneration.current + 1;
      sessionCheckGeneration.current = generation;
      const isCurrent = () => active && sessionCheckGeneration.current === generation;
      try {
        const response = await fetch('/api/v1/auth/session', { credentials: 'same-origin' });
        if (!isCurrent()) return;
        if (response.ok) {
          // The API can change from enabled authentication back to direct access
          // while this tab remains open. A successful bootstrap response is the
          // current source of truth for both the credential and auth mode.
          const nextSession = await response.json() as SessionView;
          if (isCurrent()) acceptSession(nextSession);
          return;
        }
        if (response.status === 401) {
          setSession(null);
          setState('anonymous');
        }
      } catch {
        // A transient network failure is not evidence that the browser credential expired.
      }
    };
    const interval = window.setInterval(() => { void revalidateSession(); }, AUTH_SESSION_REVALIDATION_INTERVAL_MS);
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, [acceptSession, session?.auth_enabled, state]);
  useEffect(() => {
    const requireAuth = () => {
      if (session?.auth_enabled === false) {
        // An open direct-access tab may outlive an API restart that enables
        // Operator Authentication. Re-bootstrap instead of trusting its stale
        // session mode forever.
        setState('checking');
        void checkSession();
        return;
      }
      invalidateSessionChecks();
      setSession(null);
      setState('anonymous');
    };
    window.addEventListener('quazonai:auth-required', requireAuth);
    return () => window.removeEventListener('quazonai:auth-required', requireAuth);
  }, [checkSession, invalidateSessionChecks, session?.auth_enabled]);

  const contextValue = useMemo<OperatorAuthContextValue>(
    () => ({ authEnabled: session?.auth_enabled ?? false, logout }),
    [logout, session?.auth_enabled],
  );
  const bootstrapErrorMessage = bootstrapError?.kind === 'http'
    ? t('auth.serviceHttpError', { status: bootstrapError.status })
    : bootstrapError?.kind === 'unreachable'
      ? t('auth.serviceUnreachable')
      : null;

  if (state === 'checking') {
    return (
      <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
        <main className="qz-auth-page"><div className="qz-auth-loading">{t('auth.checkingSession')}</div></main>
      </Theme>
    );
  }
  if (state === 'anonymous') {
    return (
      <>
        <LoginPage onAuthenticated={acceptSession} />
        {bootstrapErrorMessage ? <div className="qz-auth-bootstrap-error" dir="auto" role="status">{bootstrapErrorMessage}</div> : null}
      </>
    );
  }
  return (
    <OperatorAuthContext.Provider value={contextValue}>
      {children}
    </OperatorAuthContext.Provider>
  );
}
