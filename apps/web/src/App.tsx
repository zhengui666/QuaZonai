import { App as AntApp, Alert, Button, ConfigProvider, Drawer, Grid, Layout, Menu, Space, Typography, theme } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { ApartmentOutlined, ExperimentOutlined, ExportOutlined, FundOutlined, MenuOutlined, MoonOutlined, PlayCircleOutlined, SettingOutlined, SunOutlined } from '@ant-design/icons';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useContext, useLayoutEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { Projects } from './projects';
import { Alphas } from './alphas';
import { Portfolios } from './portfolio';
import { Delivery } from './delivery';
import { Runs } from './runs';
import { Settings } from './settings';
import { PwaUpdate } from './pwa';
import { GuardContext, GuardProvider, useOnline, useReducedMotion } from './ui';
import { useColorTheme } from './theme';
import type { ColorTheme } from './theme';

const queries = new QueryClient({ defaultOptions: {
  queries: { retry: false, staleTime: 15_000, gcTime: 60_000, networkMode: 'always', refetchOnWindowFocus: true },
  mutations: { retry: false, gcTime: 0, networkMode: 'always' },
} });
const navigation = [
  { key: 'research', label: '研究', icon: <ExperimentOutlined aria-hidden /> },
  { key: 'alpha', label: 'Alpha', icon: <FundOutlined aria-hidden /> },
  { key: 'portfolio', label: '组合', icon: <ApartmentOutlined aria-hidden /> },
  { key: 'delivery', label: '交付', icon: <ExportOutlined aria-hidden /> },
  { key: 'runs', label: '运行', icon: <PlayCircleOutlined aria-hidden /> },
  { key: 'settings', label: '设置', icon: <SettingOutlined aria-hidden /> },
];
function Console({ colorTheme, toggleTheme }: { colorTheme: ColorTheme; toggleTheme: () => void }) {
  const [active, setActive] = useState('research');
  const [menuOpen, setMenuOpen] = useState(false);
  const online = useOnline(); const screens = Grid.useBreakpoint(); const { blocked } = useContext(GuardContext);
  const { modal } = AntApp.useApp();
  function navigate(key: string) {
    const change = () => { setActive(key); setMenuOpen(false); };
    if (blocked && key !== active) modal.confirm({ title: '放弃未保存的更改？', okText: '放弃更改', cancelText: '继续编辑', onOk: change });
    else change();
  }
  const menu = <Menu aria-label="主导航" theme={colorTheme} mode="inline" selectedKeys={[active]} items={navigation} onClick={({ key }) => navigate(key)} />;
  let content: ReactNode;
  switch (active) {
    case 'alpha': content = <Alphas />; break;
    case 'portfolio': content = <Portfolios />; break;
    case 'delivery': content = <Delivery />; break;
    case 'runs': content = <Runs />; break;
    case 'settings': content = <Settings />; break;
    default: content = <Projects />;
  }
  const themeLabel = colorTheme === 'light' ? '切换为深色主题' : '切换为浅色主题';
  return <>
    <section className="update-bar" aria-label="应用版本"><PwaUpdate /></section>
    <Layout className="console-layout">
      {screens.lg && <Layout.Sider width={216} theme={colorTheme} className="console-sidebar"><Typography.Title level={3} className="brand">QuaZonai</Typography.Title>{menu}</Layout.Sider>}
      <Layout>
        <Layout.Header className="console-header">
          <Space>{!screens.lg && <Button icon={<MenuOutlined aria-hidden />} aria-label="打开主导航" onClick={() => setMenuOpen(true)} />}<Typography.Text strong>QuaZonai</Typography.Text></Space>
          <Button icon={colorTheme === 'light' ? <MoonOutlined aria-hidden /> : <SunOutlined aria-hidden />} aria-label={themeLabel} title={themeLabel} onClick={toggleTheme} />
        </Layout.Header>
        <Layout.Content className="console-content" id="main-content" tabIndex={-1}>
          <a className="skip-link" href="#main-content">跳至主要内容</a>
          {!online && <Alert className="global-notice" showIcon type="warning" title="离线，无法提交操作" />}
          {content}
        </Layout.Content>
      </Layout>
      <Drawer title="主导航" placement="left" open={menuOpen && !screens.lg} onClose={() => setMenuOpen(false)} width={280}>{menu}</Drawer>
    </Layout>
  </>;
}
export default function App() {
  const reducedMotion = useReducedMotion();
  const [colorTheme, toggleTheme] = useColorTheme();
  // Establish Ant Design's MotionProvider before the first paint. A preference
  // change must not remount a form or discard unsaved work.
  const [motionProviderReady, setMotionProviderReady] = useState(false);
  useLayoutEffect(() => { setMotionProviderReady(true); }, []);
  const dark = colorTheme === 'dark';
  return <ConfigProvider locale={zhCN} button={{ autoInsertSpace: false }} theme={{
    algorithm: dark ? theme.darkAlgorithm : theme.defaultAlgorithm,
    cssVar: { key: 'quazonai' },
    components: {
      Tabs: {
        itemSelectedColor: dark ? '#83b2ff' : '#2857b4',
        itemHoverColor: dark ? '#b0ccff' : '#1f4796',
        itemActiveColor: dark ? '#83b2ff' : '#183b80',
        inkBarColor: dark ? '#83b2ff' : '#2857b4',
      },
    },
    token: {
      colorPrimary: '#2857b4',
      colorLink: dark ? '#83b2ff' : '#2857b4',
      colorLinkHover: dark ? '#b0ccff' : '#1f4796',
      colorLinkActive: dark ? '#5f9cff' : '#183b80',
      colorError: dark ? '#ff8b83' : '#b42318',
      colorErrorHover: dark ? '#ffb1a9' : '#8f1c13',
      colorErrorActive: dark ? '#f47068' : '#72160f',
      colorTextSecondary: dark ? '#c1c7d0' : '#596273',
      colorTextTertiary: dark ? '#c1c7d0' : '#596273',
      colorTextDescription: dark ? '#c1c7d0' : '#596273',
      colorTextPlaceholder: dark ? '#c1c7d0' : '#596273',
      borderRadius: 8, controlHeight: 44, fontSize: 15, motion: motionProviderReady && !reducedMotion,
    },
  }}>
    <AntApp><QueryClientProvider client={queries}><GuardProvider><Console colorTheme={colorTheme} toggleTheme={toggleTheme} /></GuardProvider></QueryClientProvider></AntApp>
  </ConfigProvider>;
}
