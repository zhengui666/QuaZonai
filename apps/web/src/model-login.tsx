import { Button, Form, Input, Modal, Select, Skeleton } from 'antd';
import { useEffect, useRef, useState } from 'react';
import type { ModelAuthType, ModelLoginView, ProviderView } from '../../../packages/contracts/src/index.ts';
import { ApiError, api } from './api';
import { Copy, ErrorNotice, Status } from './ui';
function safeLink(value: string) { try { const url = new URL(value); return !url.username && !url.password && (url.protocol === 'https:' || url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)); } catch { return false; } }
export interface ModelLoginRequest { id: string; provider: ProviderView; type: ModelAuthType; resume: boolean }
export function ModelLogin({ request, changed, close }: { request: ModelLoginRequest; changed: () => Promise<void>; close: () => void }) {
  const [operation, setOperation] = useState<ModelLoginView>(); const [error, setError] = useState<unknown>(); const [actionError, setActionError] = useState<unknown>();
  const [busy, setBusy] = useState(false); const [cancelling, setCancelling] = useState(false); const [retry, setRetry] = useState(0); const [value, setValue] = useState('');
  const responseObserver = useRef<AbortController | undefined>(undefined); const started = useRef(request.resume); const finished = useRef(false);
  const response = useRef<{ promptId: number; value: string } | undefined>(undefined); const [unknownResponse, setUnknownResponse] = useState(false);
  const path = `/models/login/${encodeURIComponent(request.id)}`;
  useEffect(() => {
    const observer = new AbortController(); let timer: ReturnType<typeof setTimeout>;
    async function observe() {
      try {
        const current = started.current ? await api<ModelLoginView>(path, { signal: observer.signal }) : await api<ModelLoginView>('/models/login', { method: 'POST', body: { requestId: request.id, providerId: request.provider.id, type: request.type }, signal: observer.signal });
        if (observer.signal.aborted) return;
        started.current = true; setOperation(current); setError(undefined);
        if (current.status === 'running') timer = setTimeout(() => { void observe(); }, 700);
        else if (!finished.current) { finished.current = true; setValue(''); response.current = undefined; setUnknownResponse(false); await changed(); }
      } catch (failure) { if (!observer.signal.aborted) setError(failure); }
    }
    void observe(); return () => { observer.abort(); clearTimeout(timer); };
  }, [path, request.id, request.provider.id, request.type, retry, changed]);
  useEffect(() => { setValue(''); if (response.current && operation?.prompt?.id !== response.current.promptId) { response.current = undefined; setUnknownResponse(false); } }, [operation?.prompt?.id]);
  useEffect(() => () => { responseObserver.current?.abort(); response.current = undefined; }, []);
  async function cancel() {
    if (cancelling) return; setCancelling(true); setActionError(undefined); responseObserver.current?.abort();
    try { await api(path, { method: 'DELETE' }); await changed(); close(); } catch (failure) { setActionError(failure); } finally { setCancelling(false); }
  }
  async function submit() {
    if (!operation?.prompt || busy || cancelling) return;
    response.current ??= { promptId: operation.prompt.id, value };
    const observer = new AbortController(); responseObserver.current = observer; setBusy(true); setActionError(undefined);
    try {
      const current = await api<ModelLoginView>(`${path}/respond`, { method: 'POST', body: response.current, signal: observer.signal });
      if (observer.signal.aborted) return;
      response.current = undefined; setUnknownResponse(false); setValue(''); setOperation(current); setRetry(count => count + 1);
    } catch (failure) { if (responseObserver.current === observer) { if (!observer.signal.aborted) setActionError(failure); setUnknownResponse(true); } }
    finally { if (responseObserver.current === observer) { responseObserver.current = undefined; setBusy(false); } }
  }
  const ended = error instanceof ApiError && error.code === 'model_login_not_found';
  const terminal = ended || !!operation && operation.status !== 'running'; const prompt = terminal ? undefined : operation?.prompt;
  useEffect(() => { if (ended) { setValue(''); response.current = undefined; setUnknownResponse(false); } }, [ended]);
  const events = operation?.events.filter(event => event.type === 'auth_url' || event.type === 'device_code').slice(-2) ?? [];
  const progress = operation?.events.filter(event => event.type === 'info' || event.type === 'progress').at(-1);
  return <Modal open getContainer={false} title={request.provider.name} width={520} footer={null} maskClosable={false} closable={!cancelling} keyboard={!cancelling} onCancel={() => { if (terminal) close(); else void cancel(); }}>
    <div className="model-auth-body"><div className="model-auth-heading"><span className="quiet">{request.type === 'oauth' ? '账号授权' : 'API Key'}</span><Status value={ended ? 'unavailable' : operation?.status ?? 'pending'} label={ended ? '登录已结束' : operation?.status === 'done' ? '已配置' : operation?.status === 'running' ? '授权中' : operation ? undefined : '连接中'} /></div>
      <ErrorNotice error={error} retry={ended ? undefined : () => setRetry(count => count + 1)} /><ErrorNotice error={actionError} />
      {operation?.status === 'done' && <p role="status">模型配置已保存</p>}{operation?.status === 'failed' && <p role="alert">模型服务连接失败</p>}{operation?.status === 'cancelled' && <p role="status">连接已取消</p>}
      {events.map((event, index) => <div className="model-auth-event" key={index}>{event.type === 'auth_url' ? <>{event.instructions && <details><summary>授权说明</summary><p>{event.instructions}</p></details>}{safeLink(event.url) ? <Button href={event.url} target="_blank" rel="noopener noreferrer">打开授权页面</Button> : <p role="alert">授权链接不可用</p>}</> : event.type === 'device_code' ? <><code className="device-code">{event.userCode}</code><Copy value={event.userCode} label="复制授权码" />{safeLink(event.verificationUri) && <Button href={event.verificationUri} target="_blank" rel="noopener noreferrer">打开授权页面</Button>}</> : null}</div>)}
      {progress && 'message' in progress && <p className="quiet">{progress.message}</p>}
      {prompt && <Form key={prompt.id} layout="vertical" requiredMark={false} onFinish={() => { void submit(); }}><Form.Item label={prompt.message} htmlFor="model-auth-value">
        {prompt.type === 'select' ? <Select id="model-auth-value" autoFocus value={value || undefined} disabled={busy || cancelling || unknownResponse} onChange={setValue} options={prompt.options.map(option => ({ value: option.id, label: option.label, description: option.description }))} optionRender={option => <div>{option.label}{option.data.description && <small className="option-description">{option.data.description}</small>}</div>} /> : prompt.type === 'secret' ? <Input.Password id="model-auth-value" autoFocus autoComplete="new-password" value={value} disabled={busy || cancelling || unknownResponse} onChange={event => setValue(event.target.value)} placeholder={prompt.placeholder} /> : <Input id="model-auth-value" autoFocus autoComplete="off" value={value} disabled={busy || cancelling || unknownResponse} onChange={event => setValue(event.target.value)} placeholder={prompt.placeholder} />}
      </Form.Item><div className="form-actions"><Button type="primary" htmlType="submit" loading={busy} disabled={cancelling || (((prompt.type === 'select' && !value) || (prompt.type === 'secret' && !value.trim())) && !unknownResponse)}>{unknownResponse ? '重试当前响应' : '确认连接'}</Button></div></Form>}
      {!prompt && !terminal && !error && <div role="status" aria-label="等待授权"><Skeleton active={false} paragraph={{ rows: 1 }} /></div>}
      <div className="form-actions"><Button loading={cancelling} onClick={() => { if (terminal) close(); else void cancel(); }}>{terminal ? '关闭' : '取消连接'}</Button></div>
    </div>
  </Modal>;
}
