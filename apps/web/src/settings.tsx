import { Grid, Select, Space, Tabs, Typography } from 'antd';
import { useState } from 'react';
import { DataManagement } from './data';
import { IntegrationManagement } from './integrations';
import { CodexSettings } from './codex';
import { MigrationManagement } from './migrations';
import { AuthenticationSettings } from './auth-settings';

const categories = [
  { key: 'codex', label: 'Codex' },
  { key: 'auth', label: '鉴权管理' },
  { key: 'integrations', label: '集成' },
  { key: 'data', label: '数据' },
  { key: 'migrations', label: '迁移' },
];

export function Settings() {
  const [tab, setTab] = useState('codex');
  const screens = Grid.useBreakpoint();
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={1}>设置</Typography.Title>
    {screens.md ? <Tabs activeKey={tab} onChange={setTab} items={categories} />
      : <Select aria-label="设置类别" className="full-width" virtual={false} value={tab} onChange={setTab}
        options={categories.map(({ key, label }) => ({ value: key, label }))} />}
    {tab === 'codex' ? <CodexSettings /> : tab === 'auth' ? <AuthenticationSettings /> : tab === 'integrations' ? <IntegrationManagement /> : tab === 'migrations' ? <MigrationManagement /> : <DataManagement />}
  </Space>;
}
