import { createContext, lazy, Suspense, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import zhCN from 'antd/locale/zh_CN';
import { App as AntApp, Button, ConfigProvider, Drawer, Dropdown, theme } from 'antd';
import { QueryClient, QueryClientProvider, useQueryClient } from '@tanstack/react-query';
import { ChartLineUpIcon, CircleHalfIcon, CircleNotchIcon, DatabaseIcon, FlaskIcon, IconContext, ListIcon, MoonIcon, PlugsIcon, SignOutIcon, SquaresFourIcon, SunIcon, ArrowsLeftRightIcon } from '@phosphor-icons/react';
import type { ModelAuthType, ProviderView, SessionView } from '../../../packages/contracts/src/index.ts';
import { api, authenticationEvents } from './api';
import { useResource } from './data-hooks';
import { href, useRoute, type Route, type Workspace } from './router';
import { Auth } from './auth';
import { Empty, ErrorNotice, Loading } from './ui';
import AppErrorBoundary from './AppErrorBoundary';
const Research = lazy(() => import('./research'));
const Data = lazy(() => import('./data'));
const Portfolios = lazy(() => import('./portfolio'));
const Factors = lazy(() => import('./factors'));
const Observations = lazy(() => import('./observations'));
const Settings = lazy(() => import('./settings'));
const Extensions = lazy(() => import('./extensions'));
const ModelLogin = lazy(() => import('./model-login').then(module => ({ default: module.ModelLogin })));
const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 1500, refetchOnWindowFocus: true }, mutations: { retry: false } } });
type Appearance = 'system' | 'light' | 'dark';
const destinations: { page: Workspace; label: string; icon: ReactNode; secondary?: boolean }[] = [
  { page: 'research', label: '研究', icon: <FlaskIcon /> }, { page: 'data', label: '数据', icon: <DatabaseIcon /> },
  { page: 'factors', label: '因子库', icon: <SquaresFourIcon /> },
  { page: 'portfolios', label: '组合回测', icon: <ChartLineUpIcon /> }, { page: 'observations', label: '目标与观测', icon: <ArrowsLeftRightIcon /> },
  { page: 'settings', label: '模型设置', icon: <PlugsIcon />, secondary: true }, { page: 'extensions', label: '扩展', icon: <SquaresFourIcon />, secondary: true },
];
function WorkspaceApp() {
  const route = useRoute(); const previousRoute = useRef<Route | undefined>(undefined); const cache = useQueryClient(); const session = useResource<SessionView>('/session');
  const [drawer, setDrawer] = useState(false); const [logoutError, setLogoutError] = useState<unknown>();
  const [authRequest, setAuthRequest] = useState<{ id: string; provider: ProviderView; type: ModelAuthType; resume: boolean }>();
  const changeSession = (value: SessionView | undefined) => { cache.removeQueries({ predicate: query => query.queryKey[0] !== '/session' }); cache.setQueryData(['/session'], value ?? { setupRequired: false, authenticated: false }); };
  const refreshModels = useCallback(async () => { await cache.invalidateQueries({ queryKey: ['/models'] }); }, [cache]);
  useEffect(() => { const expired = () => { setAuthRequest(old => old ? { ...old, resume: true } : undefined); cache.setQueryData<SessionView>(['/session'], { setupRequired: false, authenticated: false }); }; authenticationEvents.addEventListener('required', expired); return () => authenticationEvents.removeEventListener('required', expired); }, [cache]);
  useEffect(() => {
    const previous = previousRoute.current; previousRoute.current = route; setDrawer(false);
    if (!route.creating) {
      document.getElementById('main-content')?.focus({ preventScroll: true });
      if (route.page === 'research' && !route.id && previous?.page === 'research' && previous.id) requestAnimationFrame(() => document.querySelector<HTMLAnchorElement>(`a[data-study-id="${CSS.escape(previous.id!)}"]`)?.focus({ preventScroll: true }));
    }
  }, [route.page, route.id, route.creating, session.data?.authenticated]);
  if (session.isPending) return <main><Loading /></main>;
  if (session.error || !session.data) return <main><ErrorNotice error={session.error ?? new Error('会话不可用')} retry={() => { void session.refetch(); }} /></main>;
  if (!session.data.authenticated) return <Auth session={session.data} changed={changeSession} />;
  const links = (secondary: boolean) => destinations.filter(item => Boolean(item.secondary) === secondary).map(item => <a key={item.page} className={`nav-link ${route.page === item.page ? 'selected' : ''}`} aria-current={route.page === item.page ? 'page' : undefined} href={href(`/${item.page}`)} onClick={() => setDrawer(false)}>{item.icon}<span>{item.label}</span></a>);
  return <><a href="#main-content" className="skip-link" onClick={event => { event.preventDefault(); document.getElementById('main-content')?.focus(); }}>跳到内容</a>
    <header className="app-topbar"><div className="topbar-inner"><a className="wordmark" href={href('/research')}>QuaZonai</a><nav className="primary-nav" aria-label="主要工作区">{links(false)}</nav><div className="topbar-right"><nav className="secondary-nav" aria-label="工作台设置">{links(true)}</nav><div className="topbar-divider" /><AppearanceControl /><Button className="icon-button logout-button" type="text" aria-label="退出登录" icon={<SignOutIcon size={19} />} onClick={() => { setLogoutError(undefined); void api<SessionView>('/session', { method: 'DELETE' }).then(changeSession, setLogoutError); }} /></div><Button className="mobile-menu icon-button" type="text" icon={<ListIcon size={21} />} aria-label="打开导航" onClick={() => setDrawer(true)} /></div></header>
    <Drawer title="QuaZonai" placement="left" getContainer={false} open={drawer} onClose={() => setDrawer(false)} width={288}><nav className="drawer-nav" aria-label="工作区">{links(false)}<div className="drawer-divider" />{links(true)}</nav><div className="drawer-appearance"><AppearanceControl /><span>外观设置</span><Button type="text" onClick={() => { void api<SessionView>('/session', { method: 'DELETE' }).then(value => { setDrawer(false); changeSession(value); }, setLogoutError); }}>退出登录</Button></div></Drawer>
    <main id="main-content" tabIndex={-1} className={route.page === 'research' && !route.creating ? 'research-main' : ''}><ErrorNotice error={logoutError} /><AppErrorBoundary key={`${route.page}/${route.creating}/${route.id ?? ''}/${route.creating ? route.params.toString() : ''}`}><Suspense fallback={<Loading />}>
      {route.unknown ? <Empty title="未找到页面" action={<a className="text-link" href={href('/research')}>返回研究</a>} /> : route.page === 'research' ? <Research route={route} /> : route.page === 'data' ? <Data /> : route.page === 'factors' ? <Factors route={route} /> : route.page === 'portfolios' ? <Portfolios route={route} /> : route.page === 'observations' ? <Observations route={route} /> : route.page === 'settings' ? <Settings connect={setAuthRequest} /> : <Extensions />}
    </Suspense></AppErrorBoundary></main>
    {authRequest && <Suspense fallback={<Loading />}><ModelLogin key={authRequest.id} request={authRequest} changed={refreshModels} close={() => setAuthRequest(undefined)} /></Suspense>}
  </>;
}
const AppearanceContext = createContext<{ mode: Appearance; setMode: (mode: Appearance) => void }>({ mode: 'system', setMode: () => undefined });
export function useAppearance() { return useContext(AppearanceContext); }
function AppearanceControl() {
  const { mode, setMode } = useAppearance();
  return <Dropdown trigger={['click']} menu={{ selectedKeys: [mode], onClick: event => setMode(event.key as Appearance), items: [{ key: 'system', label: '跟随系统', icon: <CircleHalfIcon /> }, { key: 'light', label: '浅色', icon: <SunIcon /> }, { key: 'dark', label: '深色', icon: <MoonIcon /> }] }}><Button className="icon-button" type="text" aria-label="外观设置" icon={<CircleHalfIcon size={19} />} /></Dropdown>;
}
export default function App() {
  const [mode, setAppearance] = useState<Appearance>(() => { try { const value = localStorage.getItem('quazonai.appearance.v3'); if (['light', 'dark', 'system'].includes(value ?? '')) return value as Appearance; } catch { /* Follow system when storage is unavailable. */ } return 'system'; });
  const [systemDark, setSystemDark] = useState(matchMedia('(prefers-color-scheme: dark)').matches);
  useEffect(() => { const media = matchMedia('(prefers-color-scheme: dark)'); const update = () => setSystemDark(media.matches); media.addEventListener('change', update); return () => media.removeEventListener('change', update); }, []);
  const dark = mode === 'dark' || mode === 'system' && systemDark;
  const setMode = (next: Appearance) => { setAppearance(next); try { localStorage.setItem('quazonai.appearance.v3', next); } catch { /* The current selection still applies. */ } };
  const color = dark ? { canvas: '#171B20', surface: '#20252B', text: '#E7ECF1', secondary: '#A7B1BC', accent: '#94C7BC', primary: '#E7ECF1', primaryText: '#202830', border: '#70808E', line: '#38414A', inset: '#292F36', selected: '#2B3B3A', error: '#F0A29A' } : { canvas: '#F5F6F8', surface: '#FDFEFE', text: '#26313A', secondary: '#626F7B', accent: '#2A6B62', primary: '#293740', primaryText: '#FCFDFE', border: '#8795A1', line: '#E0E5EA', inset: '#F1F4F6', selected: '#EDF3F1', error: '#AF433E' };
  useEffect(() => { document.documentElement.style.colorScheme = dark ? 'dark' : 'light'; document.documentElement.style.backgroundColor = color.canvas; }, [dark, color.canvas]);
  return <QueryClientProvider client={client}><AppearanceContext.Provider value={{ mode, setMode }}><ConfigProvider locale={zhCN} virtual={false} wave={{ disabled: true }} button={{ autoInsertSpace: false, loadingIcon: <CircleNotchIcon size={16} aria-hidden className="busy-glyph" /> }} theme={{ algorithm: dark ? theme.darkAlgorithm : theme.defaultAlgorithm, token: { colorPrimary: color.primary, colorTextLightSolid: color.primaryText, colorBgBase: color.surface, colorBgContainer: color.surface, colorBgLayout: color.canvas, colorText: color.text, colorTextSecondary: color.secondary, colorTextPlaceholder: color.secondary, colorBorder: color.border, colorBorderSecondary: color.line, colorFillAlter: color.inset, colorLink: color.accent, colorError: color.error, colorErrorText: color.error, colorErrorBorder: color.error, borderRadius: 7, controlHeight: 38, fontSize: 14, fontWeightStrong: 600, fontFamily: '"Geist Variable", "Noto Sans CJK SC", "Microsoft YaHei", system-ui, sans-serif', motion: !matchMedia('(prefers-reduced-motion: reduce)').matches }, components: { Button: { primaryShadow: 'none', defaultShadow: 'none', dangerShadow: 'none', fontWeight: 500 }, Table: { headerBg: color.surface, headerColor: color.secondary, headerSplitColor: 'transparent', borderColor: color.line, cellPaddingBlock: 15, rowHoverBg: color.inset, rowExpandedBg: color.inset }, Tabs: { horizontalItemGutter: 28, inkBarColor: color.accent, itemSelectedColor: color.text, itemActiveColor: color.accent, titleFontSize: 13 }, Select: { optionSelectedBg: color.selected, optionSelectedColor: color.text } } }}><AntApp><IconContext.Provider value={{ size: 17, weight: 'regular', 'aria-hidden': true }}><div className="workbench" data-theme={dark ? 'dark' : 'light'}><WorkspaceApp /></div></IconContext.Provider></AntApp></ConfigProvider></AppearanceContext.Provider></QueryClientProvider>;
}
