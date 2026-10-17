import { Button, Checkbox, Form, Input } from 'antd';
import { useState } from 'react';
import type { SessionView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { ErrorNotice } from './ui';
export function Auth({ session, changed }: { session: SessionView; changed: (value: SessionView) => void }) {
  const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  const setup = session.setupRequired;
  return <main className="auth-page"><section className="auth-document"><a className="wordmark auth-wordmark" href="#/research">QuaZonai</a><h1>{setup ? '设置工作台密码' : '登录工作台'}</h1><Form layout="vertical" requiredMark={false} validateTrigger="onBlur" scrollToFirstError={{ focus: true }} onFinish={async (values: { password: string; remember?: boolean }) => {
    if (busy) return; setBusy(true); setError(undefined);
    try { changed(await api<SessionView>(setup ? '/session/setup' : '/session/login', { method: 'POST', body: { password: values.password, ...(setup ? {} : { remember: values.remember ?? false }) } })); }
    catch (failure) { setError(failure); } finally { setBusy(false); }
  }}>
    <Form.Item label="密码" name="password" rules={[{ required: true, message: '请输入密码' }]}><Input.Password autoFocus autoComplete={setup ? 'new-password' : 'current-password'} disabled={busy} /></Form.Item>
    {setup ? <Form.Item label="确认密码" name="confirmation" dependencies={['password']} rules={[{ required: true, message: '请确认密码' }, ({ getFieldValue }) => ({ validator: (_rule, value: string) => !value || value === getFieldValue('password') ? Promise.resolve() : Promise.reject(new Error('两次密码不一致')) })]}><Input.Password autoComplete="new-password" disabled={busy} /></Form.Item> : <Form.Item name="remember" valuePropName="checked"><Checkbox>记住登录</Checkbox></Form.Item>}
    <ErrorNotice error={error} /><Button type="primary" htmlType="submit" block loading={busy}>{setup ? '创建密码' : '登录'}</Button>
  </Form></section></main>;
}
