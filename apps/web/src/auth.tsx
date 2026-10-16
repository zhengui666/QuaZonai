import { App, Button, Checkbox, Form, Input } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import type { SessionView } from '../../../packages/contracts/src/index.ts';
import { api, authenticationEvents } from './api';
import { ErrorNotice, QueryPanel, useResource } from './ui';

const AuthContext = createContext({ end: () => {} });
export function AuthGate({ children }: { children: ReactNode }) {
  const client = useQueryClient();
  const session = useResource<SessionView>('/session');
  const [expired, setExpired] = useState(false);
  function end() {
    client.clear(); setExpired(true);
    client.setQueryData<SessionView>(['/session'], { authenticated: false, setupRequired: false });
  }
  useEffect(() => {
    const required = () => { client.clear(); setExpired(true); client.setQueryData<SessionView>(['/session'], { authenticated: false, setupRequired: false }); };
    authenticationEvents.addEventListener('required', required);
    return () => authenticationEvents.removeEventListener('required', required);
  }, [client]);
  if (session.data?.authenticated) return <AuthContext.Provider value={{ end }}>{children}</AuthContext.Provider>;
  return <main className="auth-page"><section className="auth-card surface"><div className="brand auth-brand"><span className="brand-mark" aria-hidden>Q</span><span>QuaZonai</span></div>
    <QueryPanel query={session}><h1>{session.data?.setupRequired ? '设置密码' : '欢迎回来'}</h1>
      {expired && <div className="auth-status" role="status">请重新登录</div>}
      <LoginForm key={String(session.data?.setupRequired)} setup={session.data?.setupRequired ?? false} onSuccess={async () => {
        setExpired(false); client.clear(); await session.refetch();
      }} />
    </QueryPanel>
  </section></main>;
}
function LoginForm({ setup, onSuccess }: { setup: boolean; onSuccess: () => Promise<unknown> }) {
  const [form] = Form.useForm(); const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  async function submit(values: { password: string; remember?: boolean }) {
    setBusy(true); setError(undefined);
    try {
      await api('/session/' + (setup ? 'setup' : 'login'), { method: 'POST', body: setup ? { password: values.password } : { password: values.password, remember: values.remember ?? false } });
      form.resetFields(); await onSuccess();
    } catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  return <Form form={form} layout="vertical" disabled={busy} onFinish={submit} initialValues={{ remember: false }} requiredMark={false}>
    <Form.Item name="password" label="密码" rules={[{ required: true, message: '请输入密码' }, ...(setup ? [{ min: 8, message: '至少 8 个字符' }] : [])]}>
      <Input.Password autoComplete={setup ? 'new-password' : 'current-password'} autoFocus />
    </Form.Item>
    {setup && <Form.Item name="confirm" label="确认密码" dependencies={['password']} rules={[{ required: true, message: '请确认密码' }, ({ getFieldValue }) => ({ validator: (_, value) => !value || value === getFieldValue('password') ? Promise.resolve() : Promise.reject(new Error('密码不一致')) })]}>
      <Input.Password autoComplete="new-password" />
    </Form.Item>}
    {!setup && <Form.Item name="remember" valuePropName="checked"><Checkbox>记住登录</Checkbox></Form.Item>}
    <ErrorNotice error={error} /><Button type="primary" htmlType="submit" block loading={busy}>{setup ? '设置并登录' : '登录'}</Button>
  </Form>;
}
export function LogoutButton() {
  const { end } = useContext(AuthContext); const { message } = App.useApp(); const [busy, setBusy] = useState(false);
  async function logout() {
    setBusy(true);
    try { await api('/session', { method: 'DELETE' }); end(); }
    catch { void message.error('退出失败'); }
    finally { setBusy(false); }
  }
  return <Button className="logout-button" loading={busy} onClick={logout}>退出登录</Button>;
}
