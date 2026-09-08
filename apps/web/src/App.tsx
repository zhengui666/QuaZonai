import { App as AntApp, Alert, Button, ConfigProvider, Drawer, Grid, Layout, Menu, Result, Space, Typography } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { ApartmentOutlined, ExperimentOutlined, ExportOutlined, FundOutlined, MenuOutlined, PlayCircleOutlined, SettingOutlined } from '@ant-design/icons';
import { QueryClient, QueryClientProvider, useQueryClient } from '@tanstack/react-query';
import { Component, useContext, useEffect, useRef, useState } from 'react';
import type { ErrorInfo, ReactNode } from 'react';
import { api, AUTH_CHANGED, REAUTH_REQUIRED } from './api';
import type { Schema } from './api';
import { AuthBoundary, VerifyDialog } from './auth';
import { Projects } from './projects';
import { Runs } from './runs';
import { Settings } from './settings';
import { PwaUpdate } from './pwa';
import { ErrorNotice, GuardContext, GuardProvider, useGuard, useOnline } from './ui';

const queries = new QueryClient({ defaultOptions: {
  queries: { retry: false, staleTime: 15_000, gcTime: 60_000, networkMode: 'always', refetchOnWindowFocus: true },
  mutations: { retry: false, gcTime: 0, networkMode: 'always' },
} });
class RenderBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(_error: Error, _info: ErrorInfo) { /* Never log authentication or business payloads. */ }
  render() {
    return this.state.failed ? <main className="auth-page"><Result status="error" title="页面未能正常显示"
      subTitle="这不代表服务器上的操作已失败或被取消。请重新载入，核对服务器记录后再操作。"
      extra={<Button onClick={() => window.location.reload()}>重新载入页面</Button>} /></main> : this.props.children;
  }
}
function AuthenticationRoot() {
  const client = useQueryClient();
  const [epoch, setEpoch] = useState(0);
  const channel = useRef<BroadcastChannel | undefined>(undefined);
  useEffect(() => {
    function reset() { client.clear(); setEpoch(value => value + 1); }
    window.addEventListener(AUTH_CHANGED, reset);
    if ('BroadcastChannel' in window) {
      const broadcast = new BroadcastChannel('quazonai-session');
      channel.current = broadcast;
      broadcast.onmessage = event => { if (event.data === 'SIGNED_OUT') reset(); };
    }
    return () => { window.removeEventListener(AUTH_CHANGED, reset); channel.current?.close(); channel.current = undefined; };
  }, [client]);
  function signedOut() {
    channel.current?.postMessage('SIGNED_OUT');
    client.clear(); setEpoch(value => value + 1);
  }
  return <>
    <div className="update-bar" aria-label="应用版本"><PwaUpdate /></div>
    <AuthBoundary key={epoch}>{session => <Console session={session} signedOut={signedOut} />}</AuthBoundary>
  </>;
}
const navigation = [
  { key: 'research', label: '研究', icon: <ExperimentOutlined aria-hidden /> },
  { key: 'alpha', label: 'Alpha', icon: <FundOutlined aria-hidden /> },
  { key: 'portfolio', label: '组合', icon: <ApartmentOutlined aria-hidden /> },
  { key: 'delivery', label: '交付', icon: <ExportOutlined aria-hidden /> },
  { key: 'runs', label: '运行', icon: <PlayCircleOutlined aria-hidden /> },
  { key: 'settings', label: '设置', icon: <SettingOutlined aria-hidden /> },
];
function PendingDomain({ title, description }: { title: string; description: string }) {
  return <><Typography.Title level={1}>{title}</Typography.Title><Result status="warning" title="尚未接通可验收的产品接口" subTitle={description} />
    <Alert showIcon type="info" title="这不是空数据，也不是已完成的功能。" description="不会用示例收益、虚构资格、假审批或假交付填充页面。研究草稿和运行记录可从主导航访问。" /></>;
}
function Console({ session, signedOut }: { session: Schema['BrowserSession']; signedOut: () => void }) {
  const [active, setActive] = useState('research');
  const [menuOpen, setMenuOpen] = useState(false);
  const [verify, setVerify] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);
  const [logoutError, setLogoutError] = useState<unknown>();
  const online = useOnline(); const screens = Grid.useBreakpoint(); const { blocked } = useContext(GuardContext);
  const { modal } = AntApp.useApp();
  useGuard(loggingOut);
  useEffect(() => {
    const requireVerification = () => setVerify(true);
    window.addEventListener(REAUTH_REQUIRED, requireVerification);
    return () => window.removeEventListener(REAUTH_REQUIRED, requireVerification);
  }, []);
  function navigate(key: string) {
    if (loggingOut) return;
    const change = () => { setActive(key); setMenuOpen(false); };
    if (blocked && key !== active) modal.confirm({ title: '离开尚未完成的操作？', content: '未保存内容会丢失，已发送的请求不会被撤销。', okText: '确认离开', cancelText: '继续操作', onOk: change });
    else change();
  }
  async function logout() {
    if (!online || loggingOut) return;
    setLoggingOut(true); setLogoutError(undefined);
    try { await api.POST('/api/v2/auth/logout'); signedOut(); }
    catch (error) { setLogoutError(error); }
    finally { setLoggingOut(false); }
  }
  function requestLogout() {
    modal.confirm({ title: '退出当前登录？', content: blocked ? '未保存内容会丢失。服务器运行不会因为退出而取消。' : '服务器运行不会因为退出而取消。', okText: '确认退出', cancelText: '返回', onOk: logout });
  }
  const menu = <Menu aria-label="主导航" mode="inline" selectedKeys={[active]} items={navigation} onClick={({ key }) => navigate(key)} />;
  let content: ReactNode;
  switch (active) {
    case 'alpha': content = <PendingDomain title="Alpha" description="合格判定、样本外证据及限制尚未形成可用的浏览器合同。运行成功不构成 Alpha 合格。" />; break;
    case 'portfolio': content = <PendingDomain title="组合" description="组合构建、约束取舍和真实回测结果尚未接通。不会用零值代替缺失的风险或成本指标。" />; break;
    case 'delivery': content = <PendingDomain title="交付" description="准确版本的审批、Paper / Live 分离和下游确认尚未形成可用界面。当前没有批准或执行订单按钮。" />; break;
    case 'runs': content = <Runs />; break;
    case 'settings': content = <Settings session={session} verify={() => setVerify(true)} />; break;
    default: content = <Projects />;
  }
  return <Layout className="console-layout">
    {screens.lg && <Layout.Sider width={216} theme="light" className="console-sidebar"><Typography.Title level={3} className="brand">QuaZonai</Typography.Title>{menu}</Layout.Sider>}
    <Layout>
      <Layout.Header className="console-header">
        <Space>{!screens.lg && <Button icon={<MenuOutlined aria-hidden />} aria-label="打开主导航" onClick={() => setMenuOpen(true)} />}<Typography.Text strong>有证据的研究，受约束的运行</Typography.Text></Space>
        <Button aria-label="退出登录" aria-busy={loggingOut} disabled={!online || loggingOut} loading={loggingOut} onClick={requestLogout}>退出登录</Button>
      </Layout.Header>
      <Layout.Content className="console-content" id="main-content" tabIndex={-1}>
        <a className="skip-link" href="#main-content">跳至主要内容</a>
        {!online && <Alert className="global-notice" showIcon type="warning" title="当前离线：显示的数据可能过期，禁止提交操作。" description="恢复连接不会自动补交任何操作。" />}
        <ErrorNotice error={logoutError} />
        {content}
      </Layout.Content>
      <Layout.Footer className="console-footer">QuaZonai · 当前为研究控制面，不是券商订单执行器。完整 Issue #62 产品验收尚未完成。</Layout.Footer>
    </Layout>
    <Drawer title="主导航" placement="left" open={menuOpen && !screens.lg} onClose={() => setMenuOpen(false)} width={280}>{menu}</Drawer>
    <VerifyDialog open={verify} close={() => setVerify(false)} />
  </Layout>;
}
export default function App() {
  return <RenderBoundary><ConfigProvider locale={zhCN} button={{ autoInsertSpace: false }} theme={{ token: {
    colorPrimary: '#2857b4', colorLink: '#2857b4', colorLinkHover: '#1f4796', colorLinkActive: '#183b80',
    colorTextSecondary: '#596273', colorTextTertiary: '#596273', colorTextDescription: '#596273',
    borderRadius: 8, controlHeight: 44, fontSize: 15,
  } }}>
    <AntApp><QueryClientProvider client={queries}><GuardProvider><AuthenticationRoot /></GuardProvider></QueryClientProvider></AntApp>
  </ConfigProvider></RenderBoundary>;
}
