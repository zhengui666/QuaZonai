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
import { Theme } from '@radix-ui/themes';
import '../styles/auth.css';

type AuthState = 'checking' | 'authenticated' | 'anonymous';

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

export function useOperatorAuth(): OperatorAuthContextValue {
  const value = useContext(OperatorAuthContext);
  if (value === null) throw new Error('useOperatorAuth must be used inside AuthGate');
  return value;
}

async function responseErrorMessage(response: Response, fallback: string): Promise<string> {
  try {
    const payload = await response.json() as ErrorEnvelope;
    return payload.error?.message ?? fallback;
  } catch {
    return fallback;
  }
}

function LoginPage({ onAuthenticated }: { onAuthenticated: (session: SessionView) => void }) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [totpCode, setTotpCode] = useState('');
  const [trustBrowser, setTrustBrowser] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
        setError(await responseErrorMessage(response, 'Authentication failed.'));
        return;
      }
      onAuthenticated(await response.json() as SessionView);
    } catch {
      setError('Unable to reach QuaZonai.');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
      <main className="qz-auth-page">
        <section className="qz-auth-card" aria-labelledby="qz-auth-title">
          <div className="qz-auth-mark" aria-hidden="true">QZ</div>
          <div className="qz-auth-heading">
            <p className="qz-auth-eyebrow">QuaZonai operator access</p>
            <h1 id="qz-auth-title">Verify your identity</h1>
            <p>Password and a Google Authenticator-compatible 6-digit code are required.</p>
          </div>
          <form className="qz-auth-form" onSubmit={submit}>
            <label>
              <span>Username</span>
              <input
                autoComplete="username"
                autoFocus
                disabled={submitting}
                onChange={(event) => setUsername(event.target.value)}
                required
                value={username}
              />
            </label>
            <label>
              <span>Password</span>
              <input
                autoComplete="current-password"
                disabled={submitting}
                onChange={(event) => setPassword(event.target.value)}
                required
                type="password"
                value={password}
              />
            </label>
            <label>
              <span>Authenticator code</span>
              <input
                autoComplete="one-time-code"
                disabled={submitting}
                inputMode="numeric"
                maxLength={6}
                onChange={(event) => setTotpCode(event.target.value.replace(/\D/g, '').slice(0, 6))}
                pattern="[0-9]{6}"
                placeholder="000000"
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
                <strong>Trust this browser</strong>
                <small>Future visits can sign in without password or authenticator code until this device trust expires.</small>
              </span>
            </label>
            {error ? <div className="qz-auth-error" role="alert">{error}</div> : null}
            <button className="qz-auth-submit" disabled={submitting || totpCode.length !== 6} type="submit">
              {submitting ? 'Verifying…' : 'Sign in'}
            </button>
          </form>
          <p className="qz-auth-footnote">Only trust a browser profile you control. Logging out forgets this browser.</p>
        </section>
      </main>
    </Theme>
  );
}

export function AuthGate({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>('checking');
  const [session, setSession] = useState<SessionView | null>(null);
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);
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
      setBootstrapError(`Authentication service returned HTTP ${response.status}.`);
      setSession(null);
      setState('anonymous');
    } catch {
      if (!isCurrent()) return;
      setBootstrapError('Unable to reach the authentication service.');
      setSession(null);
      setState('anonymous');
    } finally {
      if (isCurrent()) sessionCheckAbortController.current = null;
    }
  }, [acceptSession]);

  const logout = useCallback(async () => {
    if (!session?.auth_enabled) return;
    const response = await fetch('/api/v1/auth/logout', {
      method: 'POST',
      credentials: 'same-origin',
    });
    if (!response.ok) {
      throw new Error(await responseErrorMessage(response, `Sign out failed with HTTP ${response.status}.`));
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
    if (state !== 'authenticated' || session?.auth_enabled !== true) return;
    let active = true;
    const revalidateSession = async () => {
      try {
        const response = await fetch('/api/v1/auth/session', { credentials: 'same-origin' });
        if (!active) return;
        if (response.ok) {
          // The API can change from enabled authentication back to direct access
          // while this tab remains open. A successful bootstrap response is the
          // current source of truth for both the credential and auth mode.
          const nextSession = await response.json() as SessionView;
          if (active) acceptSession(nextSession);
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

  if (state === 'checking') {
    return (
      <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
        <main className="qz-auth-page"><div className="qz-auth-loading">Checking operator session…</div></main>
      </Theme>
    );
  }
  if (state === 'anonymous') {
    return (
      <>
        <LoginPage onAuthenticated={acceptSession} />
        {bootstrapError ? <div className="qz-auth-bootstrap-error" role="status">{bootstrapError}</div> : null}
      </>
    );
  }
  return (
    <OperatorAuthContext.Provider value={contextValue}>
      {children}
    </OperatorAuthContext.Provider>
  );
}
