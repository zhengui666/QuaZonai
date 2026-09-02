import {
  AtomIcon,
  ArrowClockwiseIcon,
  BellIcon,
  ChartLineUpIcon,
  CirclesFourIcon,
  FlaskIcon,
  GaugeIcon,
  GearIcon,
  HouseIcon,
  DownloadSimpleIcon,
  ListIcon,
  MoonIcon,
  PaperPlaneTiltIcon,
  SignOutIcon,
  SunIcon,
  TargetIcon,
} from '@phosphor-icons/react';
import { Button, Dialog, DropdownMenu, Theme } from '@radix-ui/themes';
import { Direction } from 'radix-ui';
import { Suspense, useEffect, useMemo, useState, type ReactNode } from 'react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { isLogoutError, useOperatorAuth, type LogoutFailure } from '../../auth/AuthGate';
import { localeLabels, localeOrder, useI18n, type Locale, type MessageKey } from '../../i18n';
import { usePwa } from '../../pwa/PwaProvider';
import { PageSkeleton } from '../ui/Skeleton';

const nav: Array<{ to: string; labelKey: MessageKey; mobileKey?: MessageKey; icon: typeof HouseIcon; end?: boolean }> = [
  { to: '/', labelKey: 'nav.dashboard', icon: HouseIcon, end: true },
  { to: '/ideas', labelKey: 'nav.ideas', mobileKey: 'nav.mobile.ideas', icon: FlaskIcon },
  { to: '/research', labelKey: 'nav.research', mobileKey: 'nav.mobile.research', icon: AtomIcon },
  { to: '/alpha', labelKey: 'nav.alpha', mobileKey: 'nav.mobile.alpha', icon: ChartLineUpIcon },
  { to: '/portfolio', labelKey: 'nav.portfolio', mobileKey: 'nav.mobile.portfolio', icon: CirclesFourIcon },
  { to: '/approval', labelKey: 'nav.approval', icon: TargetIcon },
  { to: '/handoff', labelKey: 'nav.handoff', icon: PaperPlaneTiltIcon },
  { to: '/admin', labelKey: 'nav.admin', icon: GearIcon },
];

function useThemeMode() {
  const [mode, setMode] = useState<'dark' | 'light'>(() => (localStorage.getItem('qz-theme') as 'dark' | 'light') || 'dark');
  useEffect(() => {
    document.documentElement.dataset.theme = mode;
    localStorage.setItem('qz-theme', mode);
    document.querySelector('meta[name="theme-color"]')?.setAttribute('content', mode === 'dark' ? '#0a0f0e' : '#f5f8f7');
  }, [mode]);
  return [mode, setMode] as const;
}

export function LocaleDirectionProvider({ children }: { children: ReactNode }) {
  const { locale } = useI18n();
  return <Direction.Provider dir={localeLabels[locale].dir}>{children}</Direction.Provider>;
}

export function AppShell() {
  const [mode, setMode] = useThemeMode();
  const [signingOut, setSigningOut] = useState(false);
  const [signOutError, setSignOutError] = useState<LogoutFailure | null>(null);
  const { locale, setLocale, t } = useI18n();
  const { authEnabled, logout } = useOperatorAuth();
  const { applyUpdate, canInstall, install, isStandalone, needRefresh, updatePhase } = usePwa();
  const location = useLocation();
  const navigate = useNavigate();
  const current = useMemo(() => {
    const item = nav.find((entry) => (entry.end ? location.pathname === entry.to : location.pathname.startsWith(entry.to)));
    return item ? t(item.labelKey) : 'QuaZonai';
  }, [location.pathname, t]);
  const changeLocale = (value: string) => {
    if ((localeOrder as readonly string[]).includes(value)) setLocale(value as Locale);
  };

  async function signOut() {
    if (signingOut) return;
    setSigningOut(true);
    setSignOutError(null);
    try {
      await logout();
      navigate('/');
    } catch (error) {
      setSignOutError(isLogoutError(error) ? error.failure : { kind: 'unreachable' });
    } finally {
      setSigningOut(false);
    }
  }

  const signOutErrorMessage = signOutError?.kind === 'api'
    ? signOutError.message
    : signOutError?.kind === 'http'
      ? t('auth.signOutHttpError', { status: signOutError.status })
        : signOutError === null
        ? null
        : t('auth.signOutFailed');

  const primaryMobileNav = [
    { to: '/', labelKey: 'nav.dashboard' as MessageKey, icon: HouseIcon, end: true },
    { to: '/research', labelKey: 'nav.mobile.research' as MessageKey, icon: AtomIcon },
    { to: '/approval', labelKey: 'nav.approval' as MessageKey, icon: TargetIcon },
    { to: '/portfolio', labelKey: 'nav.mobile.portfolio' as MessageKey, icon: CirclesFourIcon },
  ];

  return (
    <Theme appearance={mode} accentColor="jade" grayColor="sage" radius="small" scaling="90%">
      <LocaleDirectionProvider>
        <div className="qz-app">
          <aside className="qz-sidebar" aria-label={t('a11y.primaryNavigation')}>
            <div className="qz-brand">
              <div className="qz-brand-mark" aria-hidden="true"><GaugeIcon size={18} weight="duotone" /></div>
              <div><div className="qz-brand-title">QuaZonai</div><div className="qz-brand-subtitle">{t('brand.subtitle')}</div></div>
            </div>
            <nav className="qz-nav">
              {nav.map(({ to, labelKey, icon: Icon, end }) => (
                <NavLink key={to} to={to} end={end} className="qz-nav-link">
                  {({ isActive }) => <><Icon size={17} weight={isActive ? 'duotone' : 'regular'} /><span>{t(labelKey)}</span></>}
                </NavLink>
              ))}
            </nav>
            <div className="qz-sidebar-bottom">
              <NavLink to="/admin" className="qz-nav-link"><BellIcon size={17} /><span>{t('nav.status')}</span></NavLink>
            </div>
          </aside>
          <main className="qz-main">
            <header className="qz-topbar">
              <div className="qz-topbar-title">{current}</div>
              <div className="qz-topbar-actions">
                {signOutErrorMessage ? <span className="qz-signout-error" dir="auto" role="alert">{signOutErrorMessage}</span> : null}
                {needRefresh ? <Button className="qz-pwa-desktop-update" size="1" variant="soft" disabled={updatePhase === 'applying'} onClick={() => { void applyUpdate().catch(() => undefined); }}><ArrowClockwiseIcon size={15} />{updatePhase === 'applying' ? t('pwa.updating') : t('pwa.updateNow')}</Button> : null}
                <DropdownMenu.Root>
                  <DropdownMenu.Trigger>
                    <Button className="qz-mobile-nav-button" aria-label={t('a11y.openNavigation')} size="1" variant="soft"><ListIcon size={16} /></Button>
                  </DropdownMenu.Trigger>
                  <DropdownMenu.Content align="end">
                    {nav.map(({ to, labelKey, icon: Icon }) => (
                      <DropdownMenu.Item key={to} onSelect={() => navigate(to)}><Icon size={14} />{t(labelKey)}</DropdownMenu.Item>
                    ))}
                    {authEnabled ? (
                      <>
                        <DropdownMenu.Separator />
                        <DropdownMenu.Item disabled={signingOut} color="red" onSelect={() => { void signOut(); }}>
                          <SignOutIcon size={14} />{t('auth.signOut')}
                        </DropdownMenu.Item>
                      </>
                    ) : null}
                  </DropdownMenu.Content>
                </DropdownMenu.Root>
                <DropdownMenu.Root>
                  <DropdownMenu.Trigger>
                    <Button aria-label={`${t('language.change')}: ${localeLabels[locale].native}`} size="1" variant="soft" className="qz-locale-button">{localeLabels[locale].short}</Button>
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
                <Button aria-label={mode === 'dark' ? t('theme.light') : t('theme.dark')} size="1" variant="soft" onClick={() => setMode(mode === 'dark' ? 'light' : 'dark')}>
                  {mode === 'dark' ? <SunIcon size={15} /> : <MoonIcon size={15} />}
                </Button>
                {authEnabled ? (
                  <Button aria-label={t('auth.signOutAndForgetBrowser')} disabled={signingOut} size="1" variant="soft" color="red" onClick={() => { void signOut(); }}>
                    <SignOutIcon size={15} />
                  </Button>
                ) : null}
              </div>
            </header>
            <div className="qz-content"><Suspense fallback={<PageSkeleton />}><Outlet /></Suspense></div>
          </main>
          <nav className="qz-mobile-nav" aria-label={t('a11y.mobileNavigation')}>
            {primaryMobileNav.map(({ to, labelKey, icon: Icon, end }) => (
              <NavLink key={to} to={to} end={end}>
                {({ isActive }) => <><Icon size={19} weight={isActive ? 'duotone' : 'regular'} /><span>{t(labelKey)}</span></>}
              </NavLink>
            ))}
            <Dialog.Root>
              <Dialog.Trigger>
                <Button className="qz-mobile-more-trigger" size="1" variant="soft"><ListIcon size={19} /><span>{t('mobile.more')}</span></Button>
              </Dialog.Trigger>
              <Dialog.Content className="qz-mobile-more-sheet" aria-describedby="qz-mobile-more-description">
                <Dialog.Title>{t('mobile.more')}</Dialog.Title>
                <Dialog.Description id="qz-mobile-more-description">{t('mobile.moreDescription')}</Dialog.Description>
                <div className="qz-mobile-more-links">
                  {nav.filter(({ to }) => !primaryMobileNav.some((item) => item.to === to)).map(({ to, labelKey, icon: Icon }) => (
                    <Dialog.Close key={to}>
                      <NavLink to={to} className="qz-mobile-more-link">
                        {({ isActive }) => <><Icon size={18} weight={isActive ? 'duotone' : 'regular'} /><span>{t(labelKey)}</span></>}
                      </NavLink>
                    </Dialog.Close>
                  ))}
                </div>
                <div className="qz-mobile-more-settings">
                  <div className="qz-mobile-more-setting-row">
                    <span>{t('language.change')}</span>
                    <DropdownMenu.Root>
                      <DropdownMenu.Trigger><Button size="1" variant="soft">{localeLabels[locale].short}</Button></DropdownMenu.Trigger>
                      <DropdownMenu.Content align="end">
                        <DropdownMenu.RadioGroup value={locale} onValueChange={changeLocale}>
                          {localeOrder.map((code) => <DropdownMenu.RadioItem key={code} value={code}><span lang={code} dir={localeLabels[code].dir}>{localeLabels[code].native}</span></DropdownMenu.RadioItem>)}
                        </DropdownMenu.RadioGroup>
                      </DropdownMenu.Content>
                    </DropdownMenu.Root>
                  </div>
                  <Button className="qz-touch-button" size="2" variant="soft" onClick={() => setMode(mode === 'dark' ? 'light' : 'dark')}>
                    {mode === 'dark' ? <SunIcon size={16} /> : <MoonIcon size={16} />}{mode === 'dark' ? t('theme.light') : t('theme.dark')}
                  </Button>
                  {!isStandalone && canInstall ? <Button className="qz-touch-button" size="2" variant="soft" onClick={() => { void install(); }}><DownloadSimpleIcon size={16} />{t('pwa.install')}</Button> : null}
                  {!isStandalone && !canInstall ? <p className="qz-mobile-more-help">{t('pwa.installHelp')}</p> : null}
                  {needRefresh ? <Button className="qz-touch-button" size="2" variant="soft" disabled={updatePhase === 'applying'} onClick={() => { void applyUpdate().catch(() => undefined); }}><ArrowClockwiseIcon size={16} />{updatePhase === 'applying' ? t('pwa.updating') : t('pwa.updateNow')}</Button> : null}
                  {authEnabled ? <Button className="qz-touch-button" size="2" variant="soft" color="red" disabled={signingOut} onClick={() => { void signOut(); }}><SignOutIcon size={16} />{t('auth.signOut')}</Button> : null}
                </div>
              </Dialog.Content>
            </Dialog.Root>
          </nav>
        </div>
      </LocaleDirectionProvider>
    </Theme>
  );
}
