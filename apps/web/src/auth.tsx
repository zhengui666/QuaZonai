import { Alert, Button, Card, Checkbox, Form, Input, Modal, QRCode, Space, Typography } from 'antd';
import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { api, ApiFailure, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { ErrorNotice, useClock, useGuard, useOnline } from './ui';

export const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const codeRules = [{ required: true, pattern: /^[0-9]{6}$/, message: '请输入验证器当前的六位数字，不含空格。' }];
export function CodeField() {
  return <Form.Item name="code" label="动态验证码" rules={codeRules}>
    <Input inputMode="numeric" autoComplete="one-time-code" maxLength={6} placeholder="六位验证码" />
  </Form.Item>;
}
function TrustFields() {
  return <>
    <Form.Item name="trust_device" valuePropName="checked"><Checkbox>信任这台私人设备（最长 30 天）</Checkbox></Form.Item>
    <Form.Item name="device_label" label="设备名称" rules={[{ max: 120 }]}><Input maxLength={120} autoComplete="off" placeholder="例如：我的笔记本" /></Form.Item>
  </>;
}
function waitFor(error: unknown, now: number) {
  return error instanceof ApiFailure && error.retryAt > now;
}
export function LoginPanel({ bootstrap, done }: { bootstrap: Schema['BootstrapStatus']; done: () => void }) {
  const [enrollment, setEnrollment] = useState<Schema['BootstrapEnrollment']>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const [form] = Form.useForm<Schema['LoginRequest']>();
  const [setup] = Form.useForm<Schema['BootstrapStart']>();
  const now = useClock();
  const online = useOnline();
  const expired = enrollment !== undefined && Date.parse(enrollment.expires_at) <= now;
  useGuard(busy || (enrollment !== undefined && !expired));
  const disabled = busy || !online || waitFor(error, now);
  async function start(values: Schema['BootstrapStart']) {
    if (disabled) return;
    setBusy(true); setError(undefined);
    try {
      const result = dataOf(await api.POST('/api/v2/bootstrap/start', { body: {
        schema_version: 1, capability_id: values.capability_id, capability: values.capability,
      } }));
      setEnrollment(result);
    } catch (failure) { setError(failure); }
    finally { setup.resetFields(); setBusy(false); }
  }
  async function login(values: Schema['LoginRequest']) {
    if (disabled || expired) return;
    setBusy(true); setError(undefined);
    // An untouched checkbox may be absent after the enrollment form switches.
    // Absence means false, never implicit device trust or an omitted wire field.
    const fields = { schema_version: 1 as const, code: values.code, trust_device: values.trust_device === true, device_label: values.device_label || null };
    try {
      if (enrollment) {
        dataOf(await api.POST('/api/v2/bootstrap/confirm', { body: { ...fields, enrollment_id: enrollment.enrollment_id } }));
      } else {
        dataOf(await api.POST('/api/v2/auth/login', { body: fields }));
      }
      setEnrollment(undefined);
      done();
    } catch (failure) { setError(failure); }
    finally { form.resetFields(['code']); setBusy(false); }
  }
  return <main className="auth-page">
    <Card className="auth-card">
      <Typography.Text className="eyebrow">QUAZONAI · 研究工作台</Typography.Text>
      <Typography.Title level={1}>{bootstrap.initialized ? '使用验证器登录' : '绑定你的验证器'}</Typography.Title>
      <Typography.Paragraph type="secondary">只有动态验证码，没有用户名和密码。验证器密钥不会保存到浏览器缓存。</Typography.Paragraph>
      {!online && <Alert showIcon type="warning" title="当前离线，无法验证或提交。" />}
      <ErrorNotice error={error} />
      {!bootstrap.initialized && !enrollment ? <>
        <Alert type="info" showIcon title="先使用服务器本机签发的一次性初始化凭据。"
          description="这不是允许任何访客绑定系统的公开入口。二维码只返回一次；响应丢失或到期后，需要在本机重新签发凭据。" />
        <Form key="bootstrap-capability" form={setup} layout="vertical" onFinish={start} autoComplete="off" disabled={disabled}>
          <Form.Item name="capability_id" label="初始化凭据编号" rules={[{ required: true, pattern: uuidPattern, message: '请输入完整的 UUIDv7 凭据编号。' }]}><Input autoComplete="off" /></Form.Item>
          <Form.Item name="capability" label="一次性初始化凭据" rules={[{ required: true, pattern: /^[A-Za-z0-9_-]{43}$/, message: '请输入本机签发的完整 43 位凭据。' }]}><Input.Password autoComplete="off" /></Form.Item>
          <Button htmlType="submit" type="primary" block aria-label="显示绑定二维码" aria-busy={busy} loading={busy} disabled={disabled || !bootstrap.setup_allowed}>显示绑定二维码</Button>
        </Form>
      </> : <>
        {enrollment && <section className="enrollment" aria-label="验证器绑定二维码">
          {expired ? <Alert showIcon type="warning" title="绑定已过期。请在本机重新签发初始化凭据。" />
            : <><QRCode value={enrollment.provisioning_uri} type="svg" size={224} />
              <Typography.Paragraph>用验证器扫描二维码，再输入验证码完成绑定。有效期至 {displayTime(enrollment.expires_at)}。</Typography.Paragraph></>}
        </section>}
        <Form key={enrollment ? 'bootstrap-confirm' : 'totp-login'} form={form} layout="vertical" onFinish={login} initialValues={{ trust_device: false, device_label: '' }} disabled={disabled || expired}>
          <CodeField /><TrustFields />
          <Button htmlType="submit" type="primary" block aria-label={enrollment ? '确认绑定并登录' : '登录'} aria-busy={busy} loading={busy} disabled={disabled || expired}>{enrollment ? '确认绑定并登录' : '登录'}</Button>
        </Form>
      </>}
    </Card>
  </main>;
}
export function AuthBoundary({ children }: { children: (session: Schema['BrowserSession']) => ReactNode }) {
  const client = useQueryClient();
  const bootstrap = useQuery({ queryKey: ['bootstrap'], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/bootstrap/status', { signal })), retry: false });
  const session = useQuery({ queryKey: ['session'], enabled: bootstrap.data?.initialized === true,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/auth/session', { signal })),
    refetchInterval: 60_000, retry: false, gcTime: 0,
  });
  const reload = () => { void bootstrap.refetch(); if (bootstrap.data?.initialized) void session.refetch(); };
  const finish = () => { client.clear(); reload(); };
  if (bootstrap.isPending || (bootstrap.data?.initialized && session.isPending)) {
    return <main className="auth-page" role="status"><Typography.Text>正在核验系统与登录状态…</Typography.Text></main>;
  }
  if (bootstrap.error) return <main className="auth-page"><ErrorNotice error={bootstrap.error} retry={reload} /></main>;
  if (session.error && !(session.error instanceof ApiFailure && session.error.status === 401)) {
    return <main className="auth-page"><ErrorNotice error={session.error} retry={reload} /></main>;
  }
  if (session.data && !session.error) return children(session.data);
  if (!bootstrap.data) return <main className="auth-page"><ErrorNotice error={new ApiFailure('HTTP_CONTRACT_ERROR', '系统状态响应缺失。')} retry={reload} /></main>;
  return <LoginPanel bootstrap={bootstrap.data} done={finish} />;
}
export function VerifyDialog({ open, close }: { open: boolean; close: () => void }) {
  const [form] = Form.useForm<Schema['VerifyRequest']>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const client = useQueryClient();
  const online = useOnline(); const now = useClock();
  useGuard(open);
  const disabled = busy || !online || waitFor(error, now);
  async function submit(value: Schema['VerifyRequest']) {
    if (disabled) return;
    setBusy(true); setError(undefined);
    try {
      const result = dataOf(await api.POST('/api/v2/auth/verify', { body: { schema_version: 1, code: value.code } }));
      client.setQueryData(['session'], result);
      close();
    } catch (failure) { setError(failure); }
    finally { form.resetFields(); setBusy(false); }
  }
  // Reauthentication must stay above action modals nested in a detail Drawer.
  return <Modal title="重新验证敏感操作" zIndex={2000} open={open} onCancel={() => { if (!busy) { form.resetFields(); setError(undefined); close(); } }} footer={null} maskClosable={!busy} closable={!busy} destroyOnHidden>
    <Space orientation="vertical" className="full-width">
      <Alert showIcon type="info" title="验证完成后，请返回原表单检查内容并手动重新提交。" description="不会自动重放审批、取消或其他敏感操作。" />
      <ErrorNotice error={error} />
      <Form form={form} layout="vertical" onFinish={submit} disabled={disabled}>
        <CodeField /><Button type="primary" htmlType="submit" aria-label="确认验证" aria-busy={busy} loading={busy} disabled={disabled}>确认验证</Button>
      </Form>
    </Space>
  </Modal>;
}
