import { Alert, App, Button, Card, Checkbox, Form, Input, Modal, Space, Typography } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { createContext, useCallback, useContext, useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { api, ApiFailure, authenticationEvents, dataOf } from './api';
import type { Schema } from './api';
import { ErrorNotice, GuardContext, QueryPanel, useOnline } from './ui';

const AuthContext = createContext({ endSession: () => {} });
export const useAuth = () => useContext(AuthContext);
export const passwordRules = [{ required: true, message: '请输入至少 8 个字符的密码' }, {
  validator: (_: unknown, value: string | undefined) => !value || (Array.from(value).length >= 8 && new TextEncoder().encode(value).length <= 1024)
    ? Promise.resolve() : Promise.reject(new Error('密码至少 8 个字符，最多 1024 字节')),
}];

export function AuthGate({ children }: { children: ReactNode }) {
  const queries = useQueryClient();
  const { message, notification } = App.useApp();
  const [session, setSession] = useState<Schema['BrowserSession'] | null>(null);
  const [setup, setSetup] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>();
  const [notice, setNotice] = useState(false);
  const endSession = useCallback(() => {
    setSession(null); setSetup(false); setNotice(true); queries.clear();
    Modal.destroyAll(); message.destroy(); notification.destroy();
  }, [queries, message, notification]);
  const load = useCallback(async () => {
    setLoading(true); setError(undefined);
    try {
      const status = dataOf(await api.GET('/api/v2/auth/status'));
      setSetup(status.setup_required);
      if (!status.setup_required) {
        try { setSession(dataOf(await api.GET('/api/v2/auth/session'))); }
        catch (failure) { if (!(failure instanceof ApiFailure && failure.status === 401)) throw failure; }
      }
    } catch (failure) { setError(failure); }
    finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    if (!session) return;
    // A 30-day session exceeds a browser timer's signed 32-bit delay limit.
    let timer: number;
    const checkExpiry = () => {
      const remaining = Date.parse(session.expires_at) - Date.now();
      if (remaining <= 0) endSession();
      else timer = window.setTimeout(checkExpiry, Math.min(remaining, 2_147_483_647));
    };
    checkExpiry();
    authenticationEvents.addEventListener('required', endSession);
    return () => { clearTimeout(timer); authenticationEvents.removeEventListener('required', endSession); };
  }, [session, endSession]);
  if (session) return <AuthContext.Provider value={{ endSession }}>{children}</AuthContext.Provider>;
  return <main className="auth-page"><Card className="auth-card">
    <Typography.Title level={1}>QuaZonai</Typography.Title>
    <QueryPanel pending={loading} error={error} reload={() => { void load(); }}>
      <Typography.Title level={2}>{setup ? '设置登录密码' : '登录 QuaZonai'}</Typography.Title>
      {notice && <Alert showIcon type="info" title="请重新登录" />}
      <LoginForm setup={setup} onConfigured={() => { setSetup(false); }} onLogin={value => { queries.clear(); setSession(value); setNotice(false); }} />
    </QueryPanel>
  </Card></main>;
}

function LoginForm({ setup, onConfigured, onLogin }: {
  setup: boolean; onConfigured: () => void; onLogin: (session: Schema['BrowserSession']) => void;
}) {
  const [form] = Form.useForm();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>();
  const online = useOnline();
  async function submit(values: { password: string; remember_device: boolean }) {
    setPending(true); setError(undefined);
    try {
      const result = await api.POST(setup ? '/api/v2/auth/setup' : '/api/v2/auth/login', {
        body: { schema_version: 1, password: values.password, remember_device: values.remember_device },
      });
      form.resetFields(); onLogin(dataOf(result));
    } catch (failure) {
      form.resetFields(['password', 'confirm_password']);
      if (setup && failure instanceof ApiFailure && failure.status === 409) onConfigured();
      setError(failure);
    } finally { setPending(false); }
  }
  return <Form form={form} layout="vertical" onFinish={values => { void submit(values); }} initialValues={{ remember_device: false }} disabled={pending}>
    <Form.Item name="password" label={setup ? '设置密码' : '登录密码'} rules={setup ? passwordRules : [{ required: true, message: '请输入登录密码' }]}>
      <Input.Password autoComplete={setup ? 'new-password' : 'current-password'} autoFocus maxLength={1024} />
    </Form.Item>
    {setup && <Form.Item name="confirm_password" label="确认密码" dependencies={['password']} rules={[
      { required: true, message: '请再次输入密码' },
      ({ getFieldValue }) => ({ validator: (_, value) => !value || getFieldValue('password') === value
        ? Promise.resolve() : Promise.reject(new Error('两次输入的密码不一致')) }),
    ]}><Input.Password autoComplete="new-password" maxLength={1024} /></Form.Item>}
    <Form.Item name="remember_device" valuePropName="checked"><Checkbox>记住本设备 30 天</Checkbox></Form.Item>
    <Space orientation="vertical" className="full-width">
      {!online && <Alert showIcon type="warning" title="离线，无法登录" />}
      <ErrorNotice error={error} />
      <Button type="primary" htmlType="submit" block loading={pending} disabled={!online}>{setup ? '设置密码并登录' : '登录'}</Button>
    </Space>
  </Form>;
}

export function LogoutButton() {
  const { endSession } = useAuth();
  const { blocked } = useContext(GuardContext);
  const { message, modal } = App.useApp();
  const [pending, setPending] = useState(false);
  const online = useOnline();
  async function logout() {
    setPending(true);
    try { await api.POST('/api/v2/auth/logout'); endSession(); }
    catch (error) { void message.error(error instanceof ApiFailure ? error.message : '退出失败，请重试'); }
    finally { setPending(false); }
  }
  return <Button loading={pending} disabled={!online} onClick={() => {
    if (blocked) modal.confirm({ title: '放弃未保存的更改并退出？', okText: '退出登录', cancelText: '继续编辑', onOk: logout });
    else void logout();
  }}>退出登录</Button>;
}
