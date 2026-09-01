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
import { QRCodeSVG } from 'qrcode.react';
import { localeLabels, localeOrder, useI18n, type Locale } from '../i18n';
import '../styles/auth.css';

type AuthState = 'checking' | 'setup' | 'authenticated' | 'anonymous';
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
  error?: { code?: string; message?: string };
}

interface AuthBootstrapView {
  auth_enabled: boolean;
  setup_required: boolean;
}

interface SetupStartView {
  issuer: string;
  account_name: string;
  otpauth_uri: string;
  manual_key: string;
  expires_in_seconds: number;
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

async function apiError(response: Response): Promise<{ code?: string; message?: string }> {
  try {
    const payload = await response.json() as ErrorEnvelope;
    return payload.error ?? {};
  } catch {
    return {};
  }
}

function LanguageMenu() {
  const { locale, setLocale, t } = useI18n();
  const changeLocale = (value: string) => {
    if ((localeOrder as readonly string[]).includes(value)) setLocale(value as Locale);
  };
  return (
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
  );
}

function normalizeTotpCode(value: string): string {
  return value
    .replace(/[\u0660-\u0669]/g, (digit) => String(digit.charCodeAt(0) - 0x0660))
    .replace(/[\u06f0-\u06f9]/g, (digit) => String(digit.charCodeAt(0) - 0x06f0))
    .replace(/\D/g, '')
    .slice(0, TOTP_CODE_LENGTH);
}

function LoginPage({
  onAuthenticated,
  onSetupRequired,
}: {
  onAuthenticated: (session: SessionView) => void;
  onSetupRequired: () => void;
}) {
  const { locale, t } = useI18n();
  const [totpCode, setTotpCode] = useState('');
  const [trustBrowser, setTrustBrowser] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<LoginError | null>(null);
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
          totp_code: totpCode,
          trust_browser: trustBrowser,
        }),
      });
      if (!response.ok) {
        const failure = await apiError(response);
        if (failure.code === 'AUTH_SETUP_REQUIRED') {
          onSetupRequired();
          return;
        }
        setError(failure.message === undefined
          ? { kind: 'fallback', message: 'auth.authenticationFailed' }
          : { kind: 'api', message: failure.message });
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
            <LanguageMenu />
            <div className="qz-auth-mark" aria-hidden="true">QZ</div>
          <div className="qz-auth-heading">
            <p className="qz-auth-eyebrow">{t('auth.operatorAccess')}</p>
            <h1 id="qz-auth-title">{t('auth.verifyIdentity')}</h1>
            <p>{t('auth.loginDescription', { digits: TOTP_CODE_LENGTH })}</p>
          </div>
          <form className="qz-auth-form" onSubmit={submit}>
            <label>
              <span>{t('auth.authenticatorCode')}</span>
              <input
                autoComplete="one-time-code"
                autoFocus
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

function SetupPage({
  onAuthenticated,
  onAlreadyCompleted,
}: {
  onAuthenticated: (session: SessionView) => void;
  onAlreadyCompleted: () => void;
}) {
  const { locale, t } = useI18n();
  const [candidate, setCandidate] = useState<SetupStartView | null>(null);
  const [expiresAt, setExpiresAt] = useState<number | null>(null);
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const [totpCode, setTotpCode] = useState('');
  const [trustBrowser, setTrustBrowser] = useState(false);
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [expired, setExpired] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const started = useRef(false);

  const start = useCallback(async () => {
    setLoading(true);
    setError(null);
    setExpired(false);
    setCopied(false);
    setCandidate(null);
    setExpiresAt(null);
    setTotpCode('');
    try {
      const response = await fetch('/api/v1/auth/setup/start', {
        method: 'POST',
        credentials: 'same-origin',
      });
      const failure = response.ok ? {} : await apiError(response);
      if (!response.ok) {
        if (failure.code === 'AUTH_SETUP_ALREADY_COMPLETED') {
          onAlreadyCompleted();
          return;
        }
        setError(failure.message ?? t('auth.setupUnavailable'));
        return;
      }
      const nextCandidate = await response.json() as SetupStartView;
      setCandidate(nextCandidate);
      setExpiresAt(Date.now() + nextCandidate.expires_in_seconds * 1000);
    } catch {
      setError(t('auth.unreachable'));
    } finally {
      setLoading(false);
    }
  }, [onAlreadyCompleted, t]);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void start();
  }, [start]);

  useEffect(() => {
    if (expiresAt === null) return undefined;
    const update = () => {
      const next = Math.max(0, Math.ceil((expiresAt - Date.now()) / 1000));
      setRemainingSeconds(next);
      if (next === 0) {
        setCandidate(null);
        setExpiresAt(null);
        setExpired(true);
        setTotpCode('');
      }
    };
    update();
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [expiresAt]);

  const copyManualKey = async () => {
    if (candidate === null || !navigator.clipboard?.writeText) return;
    try {
      await navigator.clipboard.writeText(candidate.manual_key);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (candidate === null || expired) return;
    setSubmitting(true);
    setError(null);
    try {
      const response = await fetch('/api/v1/auth/setup/confirm', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ totp_code: totpCode, trust_browser: trustBrowser }),
      });
      if (!response.ok) {
        const failure = await apiError(response);
        if (failure.code === 'AUTH_SETUP_ALREADY_COMPLETED') {
          setCandidate(null);
          setExpiresAt(null);
          onAlreadyCompleted();
          return;
        }
        if (failure.code === 'AUTH_SETUP_EXPIRED') {
          setCandidate(null);
          setExpiresAt(null);
          setExpired(true);
          setTotpCode('');
        }
        setError(failure.message ?? t('auth.authenticationFailed'));
        return;
      }
      onAuthenticated(await response.json() as SessionView);
    } catch {
      setError(t('auth.unreachable'));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Theme appearance="dark" accentColor="jade" grayColor="sage" radius="small" scaling="90%">
      <Direction.Provider dir={localeLabels[locale].dir}>
        <main className="qz-auth-page">
          <section className="qz-auth-card qz-auth-setup-card" aria-labelledby="qz-auth-title">
            <LanguageMenu />
            <div className="qz-auth-mark" aria-hidden="true">QZ</div>
            <div className="qz-auth-heading">
              <p className="qz-auth-eyebrow">{t('auth.operatorAccess')}</p>
              <h1 id="qz-auth-title">{t('auth.setupTitle')}</h1>
              <p>{t('auth.setupDescription')}</p>
            </div>
            {loading ? <p className="qz-auth-setup-status" role="status">{t('auth.setupPreparing')}</p> : null}
            {candidate !== null ? (
              <>
                <div className="qz-auth-qr">
                  <QRCodeSVG aria-label={t('auth.setupQrLabel')} role="img" value={candidate.otpauth_uri} size={192} level="M" includeMargin />
                </div>
                <div className="qz-auth-setup-meta">
                  <span>{candidate.issuer} · {candidate.account_name}</span>
                  <span>{t('auth.setupExpires', { seconds: remainingSeconds ?? candidate.expires_in_seconds })}</span>
                </div>
                <div className="qz-auth-manual-key">
                  <span>{t('auth.setupManualKey')}</span>
                  <code dir="ltr">{candidate.manual_key}</code>
                  <button className="qz-auth-copy" onClick={() => void copyManualKey()} type="button">
                    {copied ? t('auth.setupCopied') : t('auth.setupCopy')}
                  </button>
                </div>
                <p className="qz-auth-setup-help">{t('auth.setupInstructions')}</p>
                <form className="qz-auth-form" onSubmit={submit}>
                  <label>
                    <span>{t('auth.authenticatorCode')}</span>
                    <input
                      autoComplete="one-time-code"
                      autoFocus
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
                    <input checked={trustBrowser} disabled={submitting} onChange={(event) => setTrustBrowser(event.target.checked)} type="checkbox" />
                    <span><strong>{t('auth.trustBrowser')}</strong><small>{t('auth.trustBrowserDescription')}</small></span>
                  </label>
                  {error !== null ? <div className="qz-auth-error" dir="auto" role="alert">{error}</div> : null}
                  <button className="qz-auth-submit" disabled={submitting || totpCode.length !== TOTP_CODE_LENGTH} type="submit">
                    {submitting ? t('auth.setupConfirming') : t('auth.setupConfirm')}
                  </button>
                </form>
              </>
            ) : null}
            {expired ? (
              <div className="qz-auth-error" dir="auto" role="alert">
                <p>{t('auth.setupExpired')}</p>
                <button className="qz-auth-submit" disabled={loading} onClick={() => void start()} type="button">{t('auth.setupRegenerate')}</button>
              </div>
            ) : null}
            {candidate === null && !expired && !loading && error !== null ? <div className="qz-auth-error" dir="auto" role="alert">{error}</div> : null}
            <p className="qz-auth-footnote">{t('auth.setupSecurityNote')}</p>
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
  const sessionRef = useRef<SessionView | null>(null);
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
    sessionRef.current = nextSession;
    setSession(nextSession);
    setState(nextSession.authenticated ? 'authenticated' : 'anonymous');
  }, [invalidateSessionChecks]);

  const checkSession = useCallback(async (
    { preserveAuthenticatedOnTransientFailure = false }: {
      preserveAuthenticatedOnTransientFailure?: boolean;
    } = {},
  ) => {
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
        sessionRef.current = null;
        setSession(null);
        setState('anonymous');
        return;
      }
      setBootstrapError({ kind: 'http', status: response.status });
      sessionRef.current = null;
      setSession(null);
      setState('anonymous');
    } catch {
      if (!isCurrent()) return;
      setBootstrapError({ kind: 'unreachable' });
      if (preserveAuthenticatedOnTransientFailure && sessionRef.current?.authenticated) return;
      setSession(null);
      setState('anonymous');
    } finally {
      if (isCurrent()) sessionCheckAbortController.current = null;
    }
  }, [acceptSession]);

  const checkBootstrap = useCallback(async (
    { preserveAuthenticatedOnTransientFailure = false }: {
      preserveAuthenticatedOnTransientFailure?: boolean;
    } = {},
  ) => {
    const generation = sessionCheckGeneration.current + 1;
    sessionCheckGeneration.current = generation;
    sessionCheckAbortController.current?.abort();
    const controller = new AbortController();
    sessionCheckAbortController.current = controller;
    const isCurrent = () => sessionCheckGeneration.current === generation;
    setBootstrapError(null);
    try {
      const response = await fetch('/api/v1/auth/bootstrap', {
        credentials: 'same-origin',
        signal: controller.signal,
      });
      if (!isCurrent()) return;
      if (!response.ok) {
        if (response.status === 401) {
          sessionRef.current = null;
          setSession(null);
          setState('anonymous');
          return;
        }
        setBootstrapError({ kind: 'http', status: response.status });
        sessionRef.current = null;
        setSession(null);
        setState('anonymous');
        return;
      }
      const payload = await response.json() as Partial<AuthBootstrapView>;
      if (typeof payload.auth_enabled !== 'boolean' || typeof payload.setup_required !== 'boolean') {
        setBootstrapError({ kind: 'http', status: 502 });
        sessionRef.current = null;
        setSession(null);
        setState('anonymous');
        return;
      }
      if (payload.auth_enabled && payload.setup_required) {
        setSession(null);
        setState('setup');
        return;
      }
      await checkSession({ preserveAuthenticatedOnTransientFailure });
    } catch {
      if (!isCurrent()) return;
      setBootstrapError({ kind: 'unreachable' });
      if (preserveAuthenticatedOnTransientFailure && sessionRef.current?.authenticated) return;
      setSession(null);
      setState('anonymous');
    } finally {
      if (isCurrent()) sessionCheckAbortController.current = null;
    }
  }, [checkSession]);

  const logout = useCallback(async () => {
    if (!session?.auth_enabled) return;
    try {
      const response = await fetch('/api/v1/auth/logout', {
        method: 'POST',
        credentials: 'same-origin',
      });
      if (!response.ok) {
        const failure = await apiError(response);
        throw new LogoutError(failure.message === undefined
          ? { kind: 'http', status: response.status }
          : { kind: 'api', message: failure.message });
      }
    } catch (error) {
      if (isLogoutError(error)) throw error;
      throw new LogoutError({ kind: 'unreachable' });
    }
    invalidateSessionChecks();
    sessionRef.current = null;
    setSession(null);
    setState('anonymous');
  }, [invalidateSessionChecks, session?.auth_enabled]);

  useEffect(() => {
    void checkBootstrap();
    return invalidateSessionChecks;
  }, [checkBootstrap, invalidateSessionChecks]);
  useEffect(() => {
    // Direct-access tabs need this too: the backend can be restarted with
    // Operator Authentication enabled while a static page has no SSE or
    // other request that would surface the new login requirement.
    if (state !== 'authenticated') return;
    const revalidateSession = async () => {
      try {
        await checkBootstrap({ preserveAuthenticatedOnTransientFailure: true });
      } catch {
        // A transient network failure is not evidence that the browser credential expired.
      }
    };
    const interval = window.setInterval(() => { void revalidateSession(); }, AUTH_SESSION_REVALIDATION_INTERVAL_MS);
    return () => {
      window.clearInterval(interval);
    };
  }, [checkBootstrap, session?.auth_enabled, state]);
  useEffect(() => {
    const requireAuth = () => {
      if (sessionRef.current?.auth_enabled === false) {
        // An open direct-access tab may outlive an API restart that enables
        // Operator Authentication. Re-bootstrap instead of trusting its stale
        // session mode forever.
        setState('checking');
        void checkBootstrap();
        return;
      }
      invalidateSessionChecks();
      setSession(null);
      setState('anonymous');
    };
    window.addEventListener('quazonai:auth-required', requireAuth);
    return () => window.removeEventListener('quazonai:auth-required', requireAuth);
  }, [checkBootstrap, invalidateSessionChecks, session?.auth_enabled]);

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
        <LoginPage onAuthenticated={acceptSession} onSetupRequired={() => { setState('checking'); void checkBootstrap(); }} />
        {bootstrapErrorMessage ? <div className="qz-auth-bootstrap-error" dir="auto" role="status">{bootstrapErrorMessage}</div> : null}
      </>
    );
  }
  if (state === 'setup') {
    return <SetupPage onAlreadyCompleted={() => { setState('checking'); void checkBootstrap(); }} onAuthenticated={acceptSession} />;
  }
  return (
    <OperatorAuthContext.Provider value={contextValue}>
      {children}
    </OperatorAuthContext.Provider>
  );
}
