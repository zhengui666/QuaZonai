import { App, Grid, Select, Space, Tabs, Typography } from 'antd';
import { useContext, useState } from 'react';
import { GuardContext } from './ui';
import { DataWorkbench } from './data-workbench';
import { IntegrationManagement } from './integrations';
import { CodexSettings } from './codex';
import { MigrationManagement } from './migrations';
import { AuthenticationSettings } from './auth-settings';
import { CapitalExitAccounts } from './capital-exits';

const categories = [
  { key: 'codex', label: 'Codex' },
  { key: 'auth', label: '鉴权管理' },
  { key: 'accounts', label: '账户' },
  { key: 'integrations', label: '集成' },
  { key: 'data', label: '数据' },
  { key: 'migrations', label: '迁移' },
];

export function Settings() {
  const [tab, setTab] = useState('codex');
  const screens = Grid.useBreakpoint();
  const { blocked } = useContext(GuardContext);
  const { message } = App.useApp();
  function changeTab(next: string) {
    if (next === tab) return;
    if (blocked) { void message.info('请先保存或取消更改'); return; }
    setTab(next);
  }
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={1}>设置</Typography.Title>
    {screens.md ? <Tabs activeKey={tab} onChange={changeTab} items={categories} />
      : <Select aria-label="设置类别" className="full-width" virtual={false} value={tab} onChange={changeTab}
        options={categories.map(({ key, label }) => ({ value: key, label }))} />}
    {tab === 'codex' ? <CodexSettings /> : tab === 'auth' ? <AuthenticationSettings /> : tab === 'accounts' ? <CapitalExitAccounts /> : tab === 'integrations' ? <IntegrationManagement /> : tab === 'migrations' ? <MigrationManagement /> : <DataWorkbench />}
  </Space>;
}
