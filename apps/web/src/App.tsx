import { App as AntApp, Button, ConfigProvider, Drawer, Skeleton } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { AppstoreOutlined, ExperimentOutlined, FundOutlined, LoadingOutlined, MenuOutlined, RadarChartOutlined, SettingOutlined } from '@ant-design/icons';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { lazy, Suspense, useEffect, useState } from 'react';
import type { WorkspaceView } from '../../../packages/contracts/src/index.ts';
import { AuthGate, LogoutButton } from './auth';
import AppErrorBoundary from './AppErrorBoundary';
import { useMedia, useResource } from './ui';

const Research = lazy(() => import('./research').then(module => ({ default: module.Research })));
const Portfolios = lazy(() => import('./portfolio').then(module => ({ default: module.Portfolios })));
const Observations = lazy(() => import('./observations').then(module => ({ default: module.Observations })));
const Settings = lazy(() => import('./settings').then(module => ({ default: module.Settings })));
const Extensions = lazy(() => import('./extensions').then(module => ({ default: module.Extensions })));
const client = new QueryClient({ defaultOptions: {
  queries: { retry: false, staleTime: 10_000, refetchOnWindowFocus: true, networkMode: 'always' },
  mutations: { retry: false },
} });
const navigation = [
  { key: 'research', label: '研究看板', icon: <ExperimentOutlined aria-hidden /> },
  { key: 'portfolios', label: '策略组合', icon: <FundOutlined aria-hidden /> },
  { key: 'observations', label: '实盘/模拟盘观测', icon: <RadarChartOutlined aria-hidden /> },
  { key: 'settings', label: '设置', icon: <SettingOutlined aria-hidden /> },
  { key: 'extensions', label: '扩展', icon: <AppstoreOutlined aria-hidden /> },
];
function currentPage() { const key = location.hash.slice(1); return navigation.some(item => item.key === key) ? key : 'research'; }
function Brand() { return <div className="brand"><span className="brand-mark" aria-hidden>Q</span><span>QuaZonai</span></div>; }
function Console() {
  const [active, setActive] = useState(currentPage); const [menuOpen, setMenuOpen] = useState(false); const mobile = useMedia('(max-width: 959px)');
  const settings = useResource<WorkspaceView>('/workspace');
  useEffect(() => {
    const navigate = () => { setActive(currentPage()); setMenuOpen(false); requestAnimationFrame(() => document.getElementById('main-content')?.focus()); };
    window.addEventListener('hashchange', navigate); return () => window.removeEventListener('hashchange', navigate);
  }, []);
  useEffect(() => { document.title = `${navigation.find(item => item.key === active)?.label} · ${settings.data?.name || 'QuaZonai'}`; }, [active, settings.data?.name]);
  const menu = <nav className="navigation" aria-label="主导航">{navigation.map(item => <a className="nav-item" key={item.key} href={`#${item.key}`} aria-current={active === item.key ? 'page' : undefined} onClick={() => setMenuOpen(false)}>{item.icon}<span>{item.label}</span></a>)}</nav>;
  const content = active === 'portfolios' ? <Portfolios /> : active === 'observations' ? <Observations /> : active === 'settings' ? <Settings /> : active === 'extensions' ? <Extensions /> : <Research />;
  return <><a className="skip-link" href="#main-content" onClick={event => { event.preventDefault(); document.getElementById('main-content')?.focus(); }}>跳至内容</a><div className="app-layout">
    <aside className="sidebar"><Brand /><div className="nav-label">工作空间</div>{menu}<div className="sidebar-foot"><span className="local-badge">本机</span></div></aside>
    <div className="main-shell"><header className="topbar"><div className="topbar-location"><Button className="mobile-menu" icon={<MenuOutlined aria-hidden />} aria-label="打开主导航" onClick={() => setMenuOpen(true)} /><span className="location-root">{settings.data?.name || 'QuaZonai'}</span><span className="location-root" aria-hidden>/</span><strong>{navigation.find(item => item.key === active)?.label}</strong></div><div className="topbar-tools"><span className="local-badge">本机</span><LogoutButton /></div></header>
      <main className="main-content" id="main-content" tabIndex={-1}><AppErrorBoundary key={active}><Suspense fallback={<div role="status" aria-label="加载中"><Skeleton paragraph={{ rows: 4 }} /></div>}>{content}</Suspense></AppErrorBoundary></main>
    </div><Drawer title="主导航" placement="left" open={menuOpen && mobile} onClose={() => setMenuOpen(false)} width={280}><div className="mobile-navigation"><Brand /><div className="nav-label">工作空间</div>{menu}</div></Drawer>
  </div></>;
}
export default function App() {
  const reducedMotion = useMedia('(prefers-reduced-motion: reduce)');
  return <ConfigProvider locale={zhCN} virtual={false} button={{ autoInsertSpace: false, loadingIcon: <LoadingOutlined aria-hidden /> }} theme={{ token: {
    colorPrimary: '#bf674b', colorLink: '#9d5038', colorText: '#49352a', colorTextSecondary: '#756152', colorTextDescription: '#756152', colorTextPlaceholder: '#756152',
    colorBgBase: '#fffcf7', colorBgContainer: '#fffcf7', colorBgLayout: '#f8f3eb', colorBorder: '#dfd1c2', colorBorderSecondary: '#e8ded1',
    colorSuccess: '#486754', colorWarning: '#80582c', colorError: '#a43c32', borderRadius: 12, controlHeight: 44,
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
    motion: !reducedMotion, motionEaseOut: 'cubic-bezier(.2, .85, .2, 1)',
  }, components: {
    Button: { colorPrimary: '#a95439', colorPrimaryHover: '#95452e', colorPrimaryActive: '#843c28', primaryShadow: 'none' },
    Table: { headerBg: '#f5eee4', headerColor: '#756152', rowHoverBg: '#f6efe6', cellPaddingInline: 12 },
    Tabs: { itemSelectedColor: '#9d5038', inkBarColor: '#bf674b' },
    Modal: { borderRadiusLG: 20 }, Drawer: { colorBgElevated: '#f8f3eb' },
  } }}><QueryClientProvider client={client}><AntApp><AppErrorBoundary><AuthGate><Console /></AuthGate></AppErrorBoundary></AntApp></QueryClientProvider></ConfigProvider>;
}
