import { Alert, Button, Card, Descriptions, Modal, Space, Table, Tabs, Typography } from 'antd';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { ErrorNotice, NoData, Pager, QueryPanel, useGuard, useOnline } from './ui';

import { DataManagement } from './data';
import { IntegrationManagement } from './integrations';

type SettingsProps = { session: Schema['BrowserSession']; verify: () => void };

export function Settings(props: SettingsProps) {
  const [tab, setTab] = useState('security');
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={1}>设置</Typography.Title>
    <Tabs activeKey={tab} onChange={setTab} items={[
      { key: 'security', label: '浏览器安全' },
      { key: 'integrations', label: '原生集成' },
      { key: 'data', label: '数据与许可' },
    ]} />
    {tab === 'security' ? <SecuritySettings {...props} /> : tab === 'integrations' ? <IntegrationManagement /> : <DataManagement />}
  </Space>;
}

function SecuritySettings({ session, verify }: SettingsProps) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [target, setTarget] = useState<Schema['TrustedDevice']>();
  const client = useQueryClient(); const online = useOnline(); const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['devices', cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/auth/devices', { params: { query: { cursor } }, signal })) });
  const revoke = useMutation({ mutationFn: async (id: string) => { await api.DELETE('/api/v2/auth/devices/{id}', { params: { path: { id } } }); },
    onSuccess: async () => { setTarget(undefined); await client.invalidateQueries({ queryKey: ['devices'] }); await client.invalidateQueries({ queryKey: ['session'] }); },
  });
  useGuard(target !== undefined || revoke.isPending);
  return <Space orientation="vertical" className="full-width" size="large">
    <Card title="登录安全" extra={<Button disabled={!online} onClick={verify}>重新验证</Button>}>
      <Descriptions column={1} items={[
        { key: 'time', label: '最近验证', children: displayTime(session.authenticated_at) },
        { key: 'expiry', label: '登录有效期至', children: displayTime(session.expires_at) },
        { key: 'recent', label: '敏感操作', children: session.recent_authentication_required ? '需要重新输入动态验证码' : '近期验证有效；每次操作仍由服务器授权' },
        { key: 'device', label: '当前信任设备', children: session.trusted_device_id ?? '本次登录未启用信任设备' },
      ]} />
    </Card>
    <Card title="信任设备">
      <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
        <Alert type="info" showIcon title="撤销立即影响对应登录。撤销本机设备也会让当前登录失效。" />
        <Table<Schema['TrustedDevice']> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 680 }} locale={{ emptyText: <NoData text="没有信任设备记录。" /> }} columns={[
          { title: '设备', dataIndex: 'label' }, { title: '最后使用', key: 'used', render: (_, device) => displayTime(device.last_used_at) },
          { title: '到期', key: 'expiry', render: (_, device) => displayTime(device.expires_at) },
          { title: '状态', key: 'status', render: (_, device) => device.revoked_at ? '已撤销' : '未撤销' },
          { title: '操作', key: 'revoke', render: (_, device) => <Button danger disabled={!online || query.isError || revoke.isPending || !!device.revoked_at} onClick={() => { revoke.reset(); setTarget(device); }}>撤销设备</Button> },
        ]} />
        <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
      </QueryPanel>
    </Card>
    <Alert type="warning" showIcon title="模型、推理强度与连接设置尚未提供浏览器写入接口。" description="不会读取本机凭据、把 API Key 填到网页或显示没有运行时能力依据的模型滑块。" />
    <Modal open={!!target} title="撤销这台设备的信任？" okText="确认撤销" cancelText="返回" confirmLoading={revoke.isPending}
      okButtonProps={{ danger: true, disabled: !online }} closable={!revoke.isPending} maskClosable={!revoke.isPending}
      onCancel={() => { if (!revoke.isPending) setTarget(undefined); }} onOk={() => { if (target && online && !revoke.isPending) revoke.mutate(target.id); }}>
      <Typography.Paragraph>{target?.label}</Typography.Paragraph>
      <Typography.Paragraph className="break-word">设备编号：{target?.id}</Typography.Paragraph>
      <ErrorNotice error={revoke.error} />
    </Modal>
  </Space>;
}
