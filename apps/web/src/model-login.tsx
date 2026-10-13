import { Alert, Button, Form, Input, Modal, Select, Typography } from 'antd';
import { useEffect, useRef, useState } from 'react';
import type { ModelAuthType, ModelLoginView, ProviderView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { ErrorNotice, Status } from './ui';

function safeLink(value: string) {
  try { const url = new URL(value); return !url.username && !url.password && (url.protocol === 'https:' || (url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))); }
  catch { return false; }
}
export function ModelLogin({ request, changed, close }: {
  request: { id: string; provider: ProviderView; type: ModelAuthType; resume: boolean }; changed: () => Promise<void>; close: () => void;
}) {
  const [operation, setOperation] = useState<ModelLoginView>(); const [error, setError] = useState<unknown>(); const [actionError, setActionError] = useState<unknown>();
  const [busy, setBusy] = useState(false); const [retry, setRetry] = useState(0); const [value, setValue] = useState('');
  const started = useRef(request.resume); const finished = useRef(false); const response = useRef<{ promptId: number; value: string } | undefined>(undefined);
  const [unknownResponse, setUnknownResponse] = useState(false);
  const path = `/models/login/${encodeURIComponent(request.id)}`;
  useEffect(() => {
    const observer = new AbortController(); let timer: ReturnType<typeof setTimeout>;
    async function observe() {
      try {
        const current = started.current ? await api<ModelLoginView>(path, { signal: observer.signal }) : await api<ModelLoginView>('/models/login', {
          method: 'POST', body: { requestId: request.id, providerId: request.provider.id, type: request.type }, signal: observer.signal,
        });
        if (observer.signal.aborted) return;
        started.current = true; setOperation(current); setError(undefined);
        if (current.status === 'running') timer = setTimeout(() => { void observe(); }, 700);
        else if (!finished.current) { finished.current = true; setValue(''); response.current = undefined; setUnknownResponse(false); await changed(); }
      } catch (failure) { if (!observer.signal.aborted) setError(failure); }
    }
    void observe();
    return () => { observer.abort(); clearTimeout(timer); };
  }, [path, request.id, request.provider.id, request.type, retry, changed]);
  useEffect(() => {
    setValue('');
    if (response.current && operation?.prompt?.id !== response.current.promptId) { response.current = undefined; setUnknownResponse(false); }
  }, [operation?.prompt?.id]);
  async function cancel() {
    setBusy(true); setActionError(undefined);
    try { await api(path, { method: 'DELETE' }); await changed(); close(); }
    catch (failure) { setActionError(failure); } finally { setBusy(false); }
  }
  async function submit() {
    if (!operation?.prompt || busy) return;
    response.current ??= { promptId: operation.prompt.id, value };
    setBusy(true); setActionError(undefined);
    try {
      const current = await api<ModelLoginView>(`${path}/respond`, { method: 'POST', body: response.current });
      response.current = undefined; setUnknownResponse(false); setValue(''); setOperation(current); setRetry(count => count + 1);
    } catch (failure) { setActionError(failure); setUnknownResponse(true); }
    finally { setBusy(false); }
  }
  const terminal = operation && operation.status !== 'running'; const prompt = operation?.prompt;
  return <Modal open title={`${request.provider.name} · 连接模型服务`} width={560} footer={null} maskClosable={false} closable={!busy} keyboard={!busy} onCancel={() => { if (terminal) close(); else void cancel(); }}>
    <div className="model-auth-body"><div className="model-auth-heading"><span className="muted">{request.type === 'oauth' ? '账号授权' : 'API Key'}</span><Status value={operation?.status ?? 'pending'} /></div>
      <ErrorNotice error={error} retry={() => setRetry(count => count + 1)} /><ErrorNotice error={actionError} />
      {operation?.error && <Alert type="error" showIcon title="连接未完成，请关闭后重新连接" />}
      {operation?.status === 'done' && <Alert type="success" showIcon title="模型服务已连接" />}
      {operation?.status === 'cancelled' && <Alert type="info" showIcon title="连接已取消" />}
      {operation?.events.map((event, index) => <div className="model-auth-event" key={index}>{event.type === 'auth_url' ? <>
        {event.instructions && <p>{event.instructions}</p>}{safeLink(event.url) && <Button href={event.url} target="_blank" rel="noopener noreferrer">打开授权页面</Button>}
      </> : event.type === 'device_code' ? <><Typography.Text code copyable={{ text: event.userCode }}>{event.userCode}</Typography.Text>{safeLink(event.verificationUri) && <Button href={event.verificationUri} target="_blank" rel="noopener noreferrer">打开授权页面</Button>}</>
        : <p>{event.message}</p>}</div>)}
      {prompt && <Form key={prompt.id} layout="vertical" requiredMark={false} onFinish={() => { void submit(); }}>
        <Form.Item label={prompt.message} htmlFor="model-auth-value">{prompt.type === 'select' ? <Select id="model-auth-value" autoFocus value={value || undefined} disabled={busy || unknownResponse} onChange={setValue} options={prompt.options.map(option => ({ value: option.id, label: option.label }))} />
          : prompt.type === 'secret' ? <Input.Password id="model-auth-value" autoFocus autoComplete="new-password" value={value} disabled={busy || unknownResponse} onChange={event => setValue(event.target.value)} placeholder={prompt.placeholder} />
            : <Input id="model-auth-value" autoFocus autoComplete="off" value={value} disabled={busy || unknownResponse} onChange={event => setValue(event.target.value)} placeholder={prompt.placeholder} />}</Form.Item>
        <div className="form-actions"><Button type="primary" htmlType="submit" loading={busy} disabled={!value.trim() && !unknownResponse}>{unknownResponse ? '重试当前响应' : '确认连接'}</Button></div>
      </Form>}
      {!prompt && !terminal && !error && <span className="muted" role="status">等待模型服务授权…</span>}
      <div className="form-actions"><Button loading={busy} onClick={() => { if (terminal) close(); else void cancel(); }}>{terminal ? '关闭' : '取消连接'}</Button></div>
    </div>
  </Modal>;
}
