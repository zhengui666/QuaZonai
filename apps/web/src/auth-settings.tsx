import { App, Button, Card, Form, Input, Space, Table, Typography } from 'antd';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, dataOf, displayTime } from './api';
import { passwordRules, useAuth } from './auth';
import { ErrorNotice, QueryPanel, useGuard, useOnline } from './ui';

export function AuthenticationSettings() {
  const [form] = Form.useForm();
  const [pending, setPending] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [error, setError] = useState<unknown>();
  const { endSession } = useAuth();
  const { modal } = App.useApp();
  const online = useOnline();
  useGuard(dirty);
  const devices = useQuery({ queryKey: ['auth', 'cli-devices'], queryFn: async () => dataOf(await api.GET('/api/v2/auth/cli/devices')) });
  const revoke = useMutation({ mutationFn: async (id: string) => {
    await api.DELETE('/api/v2/auth/cli/devices/{id}', { params: { path: { id } } });
  }, onSuccess: async () => { await devices.refetch(); } });
  async function changePassword(values: { current_password: string; new_password: string }) {
    setPending(true); setError(undefined);
    try {
      await api.POST('/api/v2/auth/password', { body: { schema_version: 1, current_password: values.current_password, new_password: values.new_password } });
      form.resetFields(); setDirty(false); endSession();
    } catch (failure) { form.resetFields(); setDirty(false); setError(failure); }
    finally { setPending(false); }
  }
  return <Space orientation="vertical" className="full-width" size="large">
    <Typography.Title level={2}>鉴权管理</Typography.Title>
    <Card title="修改密码">
      <Typography.Paragraph>修改后所有浏览器需要重新登录；CLI 机器保持连接，直到你主动删除。</Typography.Paragraph>
      <Form form={form} layout="vertical" className="password-form" disabled={pending} onValuesChange={() => setDirty(true)} onFinish={values => { void changePassword(values); }}>
        <Form.Item name="current_password" label="当前密码" rules={[{ required: true, message: '请输入当前密码' }]}><Input.Password autoComplete="current-password" maxLength={1024} /></Form.Item>
        <Form.Item name="new_password" label="新密码" rules={passwordRules}><Input.Password autoComplete="new-password" maxLength={1024} /></Form.Item>
        <Form.Item name="confirm_password" label="确认新密码" dependencies={['new_password']} rules={[
          { required: true, message: '请再次输入新密码' },
          ({ getFieldValue }) => ({ validator: (_, value) => !value || getFieldValue('new_password') === value
            ? Promise.resolve() : Promise.reject(new Error('两次输入的密码不一致')) }),
        ]}><Input.Password autoComplete="new-password" maxLength={1024} /></Form.Item>
        <Space orientation="vertical" className="full-width"><ErrorNotice error={error} />
          <Button type="primary" htmlType="submit" loading={pending} disabled={!online}>修改密码并重新登录</Button>
        </Space>
      </Form>
    </Card>
    <Card title="已连接的 CLI 机器" extra={<Button loading={devices.isFetching} onClick={() => { void devices.refetch(); }}>刷新机器</Button>}>
      <Typography.Paragraph>机器通过实例地址和密码登录后会持续保持连接。删除后，该机器必须重新登录才能使用。</Typography.Paragraph>
      <ErrorNotice error={revoke.error} />
      <QueryPanel pending={devices.isPending} error={devices.error} stale={!!devices.data} reload={() => { void devices.refetch(); }}>
        <Table rowKey="id" dataSource={devices.data} pagination={false} scroll={{ x: 640 }} onHeaderRow={() => ({ tabIndex: 0 })} locale={{ emptyText: <Typography.Text type="secondary">尚无已连接的 CLI 机器</Typography.Text> }} columns={[
          { title: '机器名称', dataIndex: 'name' },
          { title: '首次连接', dataIndex: 'created_at', render: displayTime },
          { title: '最近使用', dataIndex: 'last_used_at', render: displayTime },
          { title: '操作', key: 'actions', render: (_, device) => <Button danger disabled={!online || revoke.isPending} onClick={() => modal.confirm({
            title: `删除机器“${device.name}”？`, content: '该机器保存的凭据将立即失效。', okText: '删除机器', cancelText: '取消', okButtonProps: { danger: true },
            onOk: () => { revoke.mutate(device.id); },
          })}>删除机器</Button> },
        ]} />
      </QueryPanel>
    </Card>
  </Space>;
}
