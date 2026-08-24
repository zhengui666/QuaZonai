import {
  AtomIcon,
  BellIcon,
  ChartLineUpIcon,
  CirclesFourIcon,
  FlaskIcon,
  GaugeIcon,
  GearIcon,
  HouseIcon,
  ListIcon,
  MoonIcon,
  PaperPlaneTiltIcon,
  SunIcon,
  TargetIcon,
} from '@phosphor-icons/react';
import { Button, DropdownMenu, Theme } from '@radix-ui/themes';
import { Suspense, useEffect, useMemo, useState } from 'react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { PageSkeleton } from '../ui/Skeleton';

const nav = [
  { to: '/', label: 'Dashboard', icon: HouseIcon, end: true },
  { to: '/ideas', label: 'Idea Composer', icon: FlaskIcon },
  { to: '/research', label: 'Research Observatory', icon: AtomIcon },
  { to: '/alpha', label: 'Alpha Library', icon: ChartLineUpIcon },
  { to: '/portfolio', label: 'Portfolio Lab', icon: CirclesFourIcon },
  { to: '/approval', label: 'Candidate Approval', icon: TargetIcon },
  { to: '/handoff', label: 'Handoff Center', icon: PaperPlaneTiltIcon },
  { to: '/admin', label: 'Administration', icon: GearIcon },
];

function useThemeMode() {
  const [mode, setMode] = useState<'dark' | 'light'>(() => (localStorage.getItem('qz-theme') as 'dark' | 'light') || 'dark');
  useEffect(() => {
    document.documentElement.dataset.theme = mode;
    localStorage.setItem('qz-theme', mode);
  }, [mode]);
  return [mode, setMode] as const;
}

export function AppShell() {
  const [mode, setMode] = useThemeMode();
  const location = useLocation();
  const navigate = useNavigate();
  const current = useMemo(
    () => nav.find((item) => (item.end ? location.pathname === item.to : location.pathname.startsWith(item.to)))?.label ?? 'QuaZonai',
    [location.pathname],
  );

  return (
    <Theme appearance={mode} accentColor="jade" grayColor="sage" radius="small" scaling="90%">
      <div className="qz-app">
        <aside className="qz-sidebar" aria-label="Primary navigation">
          <div className="qz-brand">
            <div className="qz-brand-mark" aria-hidden="true"><GaugeIcon size={18} weight="duotone" /></div>
            <div><div className="qz-brand-title">QuaZonai</div><div className="qz-brand-subtitle">Quant Research Cockpit</div></div>
          </div>
          <nav className="qz-nav">
            {nav.map(({ to, label, icon: Icon, end }) => (
              <NavLink key={to} to={to} end={end} className="qz-nav-link">
                {({ isActive }) => <><Icon size={17} weight={isActive ? 'duotone' : 'regular'} /><span>{label}</span></>}
              </NavLink>
            ))}
          </nav>
          <div className="qz-sidebar-bottom">
            <NavLink to="/admin" className="qz-nav-link"><BellIcon size={17} /><span>System status</span></NavLink>
          </div>
        </aside>
        <main className="qz-main">
          <header className="qz-topbar">
            <div className="qz-topbar-title">{current}</div>
            <div className="qz-topbar-actions">
              <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                  <Button className="qz-mobile-nav-button" aria-label="Open navigation" size="1" variant="soft"><ListIcon size={16} /></Button>
                </DropdownMenu.Trigger>
                <DropdownMenu.Content align="end">
                  {nav.map(({ to, label, icon: Icon }) => (
                    <DropdownMenu.Item key={to} onSelect={() => navigate(to)}><Icon size={14} />{label}</DropdownMenu.Item>
                  ))}
                </DropdownMenu.Content>
              </DropdownMenu.Root>
              <Button aria-label={`Switch to ${mode === 'dark' ? 'light' : 'dark'} theme`} size="1" variant="soft" onClick={() => setMode(mode === 'dark' ? 'light' : 'dark')}>
                {mode === 'dark' ? <SunIcon size={15} /> : <MoonIcon size={15} />}
              </Button>
            </div>
          </header>
          <div className="qz-content"><Suspense fallback={<PageSkeleton />}><Outlet /></Suspense></div>
        </main>
        <nav className="qz-mobile-nav" aria-label="Mobile primary navigation">
          {nav.slice(0, 5).map(({ to, label, icon: Icon, end }) => (
            <NavLink key={to} to={to} end={end}>
              {({ isActive }) => <><Icon size={19} weight={isActive ? 'duotone' : 'regular'} /><span>{label.replace('Idea Composer', 'Ideas').replace('Research Observatory', 'Research').replace('Alpha Library', 'Alpha').replace('Portfolio Lab', 'Portfolio')}</span></>}
            </NavLink>
          ))}
        </nav>
      </div>
    </Theme>
  );
}
