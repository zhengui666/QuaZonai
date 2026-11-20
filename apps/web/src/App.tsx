import { tr } from './i18n';
import { createContext, lazy, Suspense, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import zhCN from 'antd/locale/zh_CN';
import enUS from 'antd/locale/en_US';
import { useTranslation } from 'react-i18next';
import './i18n';
import { App as AntApp, Button, ConfigProvider, Drawer, Dropdown, theme } from 'antd';
import { QueryClient, QueryClientProvider, useQueryClient } from '@tanstack/react-query';
import { ChartLineUpIcon, CircleHalfIcon, CircleNotchIcon, FlaskIcon, IconContext, ListIcon, MoonIcon, GearIcon, SignOutIcon, SunIcon, ArrowsLeftRightIcon } from '@phosphor-icons/react';
import type { ModelAuthType, ProviderView, SessionView, SettingsView } from '../../../packages/contracts/src/index.ts';
import { api, authenticationEvents } from './api';
import { useResource } from './data-hooks';
import { currentRoute, href, rememberListNavigation, restoreListNavigation, useRoute, type Workspace } from './router';
import { Auth } from './auth';
import { Empty, ErrorNotice, Loading } from './ui';
import AppErrorBoundary from './AppErrorBoundary';
import { Brand } from './brand';
import { DeletionRecovery } from './delete-record';
import { isPlainNavigationClick } from './list-values';
const Research = lazy(() => import('./research'));
const Portfolios = lazy(() => import('./portfolio'));
const Observations = lazy(() => import('./observations'));
const Settings = lazy(() => import('./settings'));
const ModelLogin = lazy(() => import('./model-login').then(module => ({ default: module.ModelLogin })));
const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 1500, refetchOnWindowFocus: true }, mutations: { retry: false } } });
type Appearance = 'system' | 'light' | 'dark';
const destinations: { page: Workspace; label: string; icon: ReactNode; secondary?: boolean }[] = [
  { page: 'research', label: "Research", icon: <FlaskIcon /> },
  { page: 'portfolios', label: "Portfolio backtests", icon: <ChartLineUpIcon /> }, { page: 'observations', label: "Targets and observations", icon: <ArrowsLeftRightIcon /> },
  { page: 'settings', label: "Settings", icon: <GearIcon />, secondary: true },
];
function WorkspaceApp() {
  const { i18n } = useTranslation();
  const route = useRoute(); const cache = useQueryClient(); const session = useResource<SessionView>('/session');
  const languageSettings = useResource<SettingsView>('/settings', { enabled: session.data?.authenticated === true, poll: 30000 });
  useEffect(() => {
    const language = session.data?.authenticated ? languageSettings.data?.language : 'en';
    if (language && i18n.resolvedLanguage !== language) void i18n.changeLanguage(language);
  }, [session.data?.authenticated, languageSettings.data?.language, i18n]);
  const [drawer, setDrawerOpen] = useState(false); const [logoutError, setLogoutError] = useState<unknown>();
  const drawerLocation = useRef<string | undefined>(undefined);
  const setDrawer = (open: boolean) => { drawerLocation.current = open ? location.hash : undefined; setDrawerOpen(open); };
  const [authRequest, setAuthRequest] = useState<{ id: string; provider: ProviderView; type: ModelAuthType; resume: boolean }>();
  const changeSession = (value: SessionView | undefined) => { cache.removeQueries({ predicate: query => query.queryKey[0] !== '/session' }); cache.setQueryData(['/session'], value ?? { setupRequired: false, authenticated: false }); };
  const refreshModels = useCallback(async () => { await cache.invalidateQueries({ queryKey: ['/models'] }); }, [cache]);
  useEffect(() => { const expired = () => { setAuthRequest(old => old ? { ...old, resume: true } : undefined); cache.setQueryData<SessionView>(['/session'], { setupRequired: false, authenticated: false }); }; authenticationEvents.addEventListener('required', expired); return () => authenticationEvents.removeEventListener('required', expired); }, [cache]);
  useEffect(() => {
    const remember = (event: MouseEvent) => {
      if (!isPlainNavigationClick(event)) return;
      const anchor = event.target instanceof Element ? event.target.closest('a[href]') : null;
      const target = anchor?.getAttribute('href'); const current = currentRoute();
      if (target && /^#\/(?:research|factors|portfolios)\/(?!new(?:\?|$))[^/?]+/.test(target) && !current.id && !current.creating && ['research', 'factors', 'portfolios'].includes(current.page)) rememberListNavigation(target);
    };
    document.addEventListener('click', remember, true);
    return () => document.removeEventListener('click', remember, true);
  }, []);
  useLayoutEffect(() => {
    // A menu may open for the new URL before useRoute receives its queued hashchange.
    if (session.data?.authenticated && drawerLocation.current === location.hash) return;
    setDrawer(false);
    if (!route.creating) {
      if (!restoreListNavigation(route)) document.getElementById('main-content')?.focus({ preventScroll: true });
    }
  }, [route.page, route.id, route.creating, route.params.get('section'), session.data?.authenticated]);
  if (session.isPending) return <main><Loading /></main>;
  if (session.error || !session.data) return <main><ErrorNotice error={session.error ?? new Error(tr("Session unavailable"))} retry={() => { void session.refetch(); }} /></main>;
  if (!session.data.authenticated) return <Auth session={session.data} changed={changeSession} />;
  const activePage = route.page === 'data' || route.page === 'extensions' ? 'settings' : route.page;
  const links = (secondary: boolean) => destinations.filter(item => Boolean(item.secondary) === secondary).map(item => <a key={item.page} className={`nav-link ${activePage === item.page ? 'selected' : ''}`} aria-current={activePage === item.page ? 'page' : undefined} href={href(`/${item.page}`)} onClick={() => setDrawer(false)}>{item.icon}<span>{tr(item.label)}</span></a>);
  return <><a href="#main-content" className="skip-link" onClick={event => { event.preventDefault(); document.getElementById('main-content')?.focus(); }}>{tr("Skip to content")}</a>
    <header className="app-topbar"><div className="topbar-inner"><a className="wordmark" href={href('/research')}><Brand /></a><nav className="primary-nav" aria-label={tr("Main workspace")}>{links(false)}</nav><div className="topbar-right"><nav className="secondary-nav" aria-label={tr("Workspace settings")}>{links(true)}</nav><div className="topbar-divider" /><AppearanceControl /><Button className="icon-button logout-button" type="text" aria-label={tr("Sign out")} icon={<SignOutIcon size={19} />} onClick={() => { setLogoutError(undefined); void api<SessionView>('/session', { method: 'DELETE' }).then(changeSession, setLogoutError); }} /></div><Button className="mobile-menu icon-button" type="text" icon={<ListIcon size={21} />} aria-label={tr("Open navigation")} onClick={() => setDrawer(true)} /></div></header>
    <Drawer title={<Brand />} placement="left" getContainer={false} open={drawer} onClose={() => setDrawer(false)} width={288}><nav className="drawer-nav" aria-label={tr("Workspace")}>{links(false)}<div className="drawer-divider" />{links(true)}</nav><div className="drawer-appearance"><AppearanceControl /><span>{tr("Appearance")}</span><Button type="text" onClick={() => { void api<SessionView>('/session', { method: 'DELETE' }).then(value => { setDrawer(false); changeSession(value); }, setLogoutError); }}>{tr("Sign out")}</Button></div></Drawer>
    <main id="main-content" tabIndex={-1}><ErrorNotice error={logoutError} /><DeletionRecovery /><AppErrorBoundary key={`${route.page}/${route.creating}/${route.id ?? ''}/${route.creating ? route.params.toString() : ''}/${route.page === 'settings' ? route.params.get('section') ?? '' : ''}`}><Suspense fallback={<Loading />}>
      {route.unknown ? <Empty title={tr("Page not found")} action={<a className="text-link" href={href('/research')}>{tr("Back to research")}</a>} /> : route.page === 'research' ? <Research route={route} /> : route.page === 'parameter-scans' ? <Empty title={tr("The standalone parameter scans page has been retired")} action={<a className="text-link" href={href('/research')}>{tr("Back to research")}</a>} /> : route.page === 'portfolios' ? <Portfolios route={route} /> : route.page === 'observations' ? <Observations route={route} /> : <Settings connect={setAuthRequest} route={route} />}
    </Suspense></AppErrorBoundary></main>
    {authRequest && <Suspense fallback={<Loading />}><ModelLogin key={authRequest.id} request={authRequest} changed={refreshModels} close={() => setAuthRequest(undefined)} /></Suspense>}
  </>;
}
const AppearanceContext = createContext<{ mode: Appearance; setMode: (mode: Appearance) => void }>({ mode: 'system', setMode: () => undefined });
export function useAppearance() { return useContext(AppearanceContext); }
function AppearanceControl() {
  useTranslation('core');
  const { mode, setMode } = useAppearance();
  return <Dropdown trigger={['click']} menu={{ selectedKeys: [mode], onClick: event => setMode(event.key as Appearance), items: [{ key: 'system', label: tr("System"), icon: <CircleHalfIcon /> }, { key: 'light', label: tr("Light"), icon: <SunIcon /> }, { key: 'dark', label: tr("Dark"), icon: <MoonIcon /> }] }}><Button className="icon-button" type="text" aria-label={tr("Appearance")} icon={<CircleHalfIcon size={19} />} /></Dropdown>;
}
export default function App() {
  const { i18n } = useTranslation();
  useEffect(() => { document.documentElement.lang = i18n.resolvedLanguage === 'zh-CN' ? 'zh-CN' : 'en'; }, [i18n.resolvedLanguage]);
  const [mode, setAppearance] = useState<Appearance>(() => { try { const value = localStorage.getItem('quazonai.appearance.v3'); if (['light', 'dark', 'system'].includes(value ?? '')) return value as Appearance; } catch { /* Follow system when storage is unavailable. */ } return 'system'; });
  const [systemDark, setSystemDark] = useState(matchMedia('(prefers-color-scheme: dark)').matches);
  useEffect(() => { const media = matchMedia('(prefers-color-scheme: dark)'); const update = () => setSystemDark(media.matches); media.addEventListener('change', update); return () => media.removeEventListener('change', update); }, []);
  const dark = mode === 'dark' || mode === 'system' && systemDark;
  const setMode = (next: Appearance) => { setAppearance(next); try { localStorage.setItem('quazonai.appearance.v3', next); } catch { /* The current selection still applies. */ } };
  const color = dark ? { canvas: '#1E201E', surface: '#272A26', text: '#E9E6DE', secondary: '#B6B3A7', accent: '#A4C2AB', primary: '#CCDCCD', primaryText: '#233128', border: '#82877B', line: '#41453C', inset: '#30342E', selected: '#34443A', error: '#ECACA1' } : { canvas: '#F7F4EF', surface: '#FFFDFA', text: '#302F2B', secondary: '#6C665C', accent: '#446D5D', primary: '#405F52', primaryText: '#FFFDFA', border: '#9A9182', line: '#E5DED3', inset: '#F2EEE7', selected: '#E9EEE6', error: '#A4473D' };
  useEffect(() => { document.documentElement.style.colorScheme = dark ? 'dark' : 'light'; document.documentElement.style.backgroundColor = color.canvas; }, [dark, color.canvas]);
  return <QueryClientProvider client={client}><AppearanceContext.Provider value={{ mode, setMode }}><ConfigProvider locale={i18n.resolvedLanguage === 'zh-CN' ? zhCN : enUS} virtual={false} wave={{ disabled: true }} button={{ autoInsertSpace: false, loadingIcon: <CircleNotchIcon size={16} aria-hidden className="busy-glyph" /> }} theme={{ algorithm: dark ? theme.darkAlgorithm : theme.defaultAlgorithm, token: { colorPrimary: color.primary, colorTextLightSolid: color.primaryText, colorBgBase: color.surface, colorBgContainer: color.surface, colorBgLayout: color.canvas, colorText: color.text, colorTextSecondary: color.secondary, colorTextPlaceholder: color.secondary, colorBorder: color.border, colorBorderSecondary: color.line, colorFillAlter: color.inset, colorLink: color.accent, colorError: color.error, colorErrorText: color.error, colorErrorBorder: color.error, borderRadius: 10, controlHeight: 40, fontSize: 14, fontWeightStrong: 600, fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", system-ui, sans-serif', motion: !matchMedia('(prefers-reduced-motion: reduce)').matches }, components: { Button: { primaryShadow: 'none', defaultShadow: 'none', dangerShadow: 'none', fontWeight: 500 }, Table: { headerBg: color.surface, headerColor: color.secondary, headerSplitColor: 'transparent', borderColor: color.line, cellPaddingBlock: 15, rowHoverBg: color.inset, rowExpandedBg: color.inset }, Tabs: { horizontalItemGutter: 28, inkBarColor: color.accent, itemSelectedColor: color.text, itemActiveColor: color.accent, titleFontSize: 13 }, Select: { optionSelectedBg: color.selected, optionSelectedColor: color.text } } }}><AntApp><IconContext.Provider value={{ size: 17, weight: 'regular', 'aria-hidden': true }}><div className="workbench" data-theme={dark ? 'dark' : 'light'}><WorkspaceApp /></div></IconContext.Provider></AntApp></ConfigProvider></AppearanceContext.Provider></QueryClientProvider>;
}
