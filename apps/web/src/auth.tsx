import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import { Button, Checkbox, Form, Input } from 'antd';
import { useState } from 'react';
import type { SessionView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { ErrorNotice } from './ui';
import { Brand } from './brand';
export function Auth({ session, changed }: { session: SessionView; changed: (value: SessionView) => void }) {
  useTranslation('core');
  const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  const setup = session.setupRequired;
  return <main className="auth-page"><section className="auth-document"><a className="wordmark auth-wordmark" href="#/research"><Brand /></a><h1>{setup ? tr("Set your workspace password") : tr("Sign in to your workspace")}</h1><Form layout="vertical" requiredMark={false} validateTrigger="onBlur" scrollToFirstError={{ focus: true }} onFinish={async (values: { password: string; remember?: boolean }) => {
    if (busy) return; setBusy(true); setError(undefined);
    try { changed(await api<SessionView>(setup ? '/session/setup' : '/session/login', { method: 'POST', body: { password: values.password, ...(setup ? {} : { remember: values.remember ?? false }) } })); }
    catch (failure) { setError(failure); } finally { setBusy(false); }
  }}>
    <Form.Item label={tr("Password")} name="password" rules={[{ required: true, message: tr("Enter your password") }]}><Input.Password autoFocus autoComplete={setup ? 'new-password' : 'current-password'} disabled={busy} /></Form.Item>
    {setup ? <Form.Item label={tr("Confirm password")} name="confirmation" dependencies={['password']} rules={[{ required: true, message: tr("Confirm your password") }, ({ getFieldValue }) => ({ validator: (_rule, value: string) => !value || value === getFieldValue('password') ? Promise.resolve() : Promise.reject(new Error(tr("Passwords do not match"))) })]}><Input.Password autoComplete="new-password" disabled={busy} /></Form.Item> : <Form.Item name="remember" valuePropName="checked"><Checkbox>{tr("Remember me")}</Checkbox></Form.Item>}
    <ErrorNotice error={error} /><Button type="primary" htmlType="submit" block loading={busy}>{setup ? tr("Create password") : tr("Sign in")}</Button>
  </Form></section></main>;
}
