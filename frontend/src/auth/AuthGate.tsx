import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react';
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

function LoginPage({ onAuthenticated }: { onAuthenticated: () => void }) {
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
        let message = 'Authentication failed.';
        try {
          const payload = await response.json() as ErrorEnvelope;
          message = payload.error?.message ?? message;
        } catch {
          // Keep the intentionally generic authentication message.
        }
        setError(message);
        return;
      }
      onAuthenticated();
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
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);

  const checkSession = useCallback(async () => {
    setBootstrapError(null);
    try {
      const response = await fetch('/api/v1/auth/session', { credentials: 'same-origin' });
      if (response.ok) {
        const session = await response.json() as SessionView;
        setState(session.authenticated ? 'authenticated' : 'anonymous');
        return;
      }
      if (response.status === 401) {
        setState('anonymous');
        return;
      }
      setBootstrapError(`Authentication service returned HTTP ${response.status}.`);
      setState('anonymous');
    } catch {
      setBootstrapError('Unable to reach the authentication service.');
      setState('anonymous');
    }
  }, []);

  useEffect(() => { void checkSession(); }, [checkSession]);
  useEffect(() => {
    const requireAuth = () => setState('anonymous');
    window.addEventListener('quazonai:auth-required', requireAuth);
    return () => window.removeEventListener('quazonai:auth-required', requireAuth);
  }, []);

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
        <LoginPage onAuthenticated={() => setState('authenticated')} />
        {bootstrapError ? <div className="qz-auth-bootstrap-error" role="status">{bootstrapError}</div> : null}
      </>
    );
  }
  return <>{children}</>;
}
