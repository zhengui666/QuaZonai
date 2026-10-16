import { App, Button, Form, Input, Select, Tag } from 'antd';
import { KeyOutlined, SearchOutlined } from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useState } from 'react';
import type { ModelAuthType, ModelCatalog, ProviderView, SettingsView, ThinkingLevel } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { ErrorNotice, modelKey, NoData, PageHeading, QueryPanel, RefreshButton, Surface, useResource } from './ui';
import { ModelLogin } from './model-login';

export const thinkingLabels: Record<ThinkingLevel, string> = { off: '关闭', minimal: '最低', low: '低', medium: '中', high: '高', xhigh: '更高', max: '最高' };
type Login = { id: string; provider: ProviderView; type: ModelAuthType; resume: boolean };
export function Settings() {
  const settings = useResource<SettingsView>('/settings'); const catalog = useResource<ModelCatalog>('/models'); const client = useQueryClient();
  const { modal } = App.useApp(); const [login, setLogin] = useState<Login>(); const [query, setQuery] = useState(''); const [filter, setFilter] = useState('all');
  const [busyProvider, setBusyProvider] = useState<string>(); const [error, setError] = useState<unknown>();
  const refresh = useCallback(async () => { await Promise.all(['/models', '/research/models', '/settings'].map(path => client.invalidateQueries({ queryKey: [path] }))); }, [client]);
  function connect(provider: ProviderView, type: ModelAuthType) {
    setError(undefined); setLogin({ id: provider.login?.id ?? crypto.randomUUID(), provider, type: provider.login?.type ?? type, resume: !!provider.login });
  }
  async function logout(provider: ProviderView) {
    setBusyProvider(provider.id); setError(undefined);
    try { await api(`/models/providers/${encodeURIComponent(provider.id)}`, { method: 'DELETE' }); await refresh(); }
    catch (failure) { setError(failure); } finally { setBusyProvider(undefined); }
  }
  const search = query.trim().toLowerCase();
  const providers = catalog.data?.providers.filter(provider => `${provider.name} ${provider.id}`.toLowerCase().includes(search) && (filter === 'all' || provider.configured === (filter === 'connected')))
    .sort((a, b) => Number(b.configured) - Number(a.configured) || a.name.localeCompare(b.name)) ?? [];
  return <div className="page model-settings"><PageHeading title="设置" action={<RefreshButton loading={settings.isFetching || catalog.isFetching} label="刷新模型配置" onClick={() => { void refresh(); }} />} />
    <QueryPanel query={settings}>{settings.data && <ModelPreferences value={settings.data} catalog={catalog.data} unavailable={settings.isError || catalog.isError} />}</QueryPanel>
    <Surface title="模型服务" extra={<span className="muted">{catalog.data ? `${catalog.data.providers.filter(provider => provider.configured).length} 个已连接` : '—'}</span>}>
      <div className="toolbar provider-toolbar"><Input prefix={<SearchOutlined aria-hidden />} aria-label="搜索模型服务" placeholder="搜索名称或标识" allowClear value={query} onChange={event => setQuery(event.target.value)} />
        <Select aria-label="模型服务状态" value={filter} onChange={setFilter} options={[{ value: 'all', label: '全部服务' }, { value: 'connected', label: '已连接' }, { value: 'disconnected', label: '未连接' }]} /></div>
      <ErrorNotice error={error} /><QueryPanel query={catalog}>{providers.length ? <div className="provider-directory">{providers.map(provider => <article className="provider-row" key={provider.id}>
        <div className="provider-identity"><span className="provider-icon" aria-hidden><KeyOutlined /></span><div><h3>{provider.name}</h3><span className="muted">{provider.id}</span></div></div>
        <Tag className={`status status-${provider.configured ? 'good' : 'quiet'}`}>{provider.error ? '状态不可用' : provider.configured ? '已连接' : '未连接'}</Tag>
        <div className="provider-actions">{provider.login ? <Button disabled={!!busyProvider || catalog.isError} onClick={() => connect(provider, provider.login!.type)}>继续连接</Button>
          : provider.authTypes.map(type => <Button key={type} disabled={!!busyProvider || catalog.isError} aria-label={`${type === 'oauth' ? '登录' : '配置 API Key'} ${provider.name}`} onClick={() => connect(provider, type)}>{type === 'oauth' ? '登录' : 'API Key'}</Button>)}
          {provider.configured && <Button disabled={!!busyProvider || catalog.isError} loading={busyProvider === provider.id} aria-label={`退出 ${provider.name}`} onClick={() => modal.confirm({ title: `退出 ${provider.name}？`, content: '将移除本应用保存的凭据。环境提供的认证不受影响。', okText: '退出', cancelText: '取消', onOk: () => logout(provider) })}>退出</Button>}
          {!provider.authTypes.length && !provider.configured && <span className="muted">由宿主环境配置</span>}</div>
      </article>)}</div> : <NoData text={query || filter !== 'all' ? '无匹配模型服务' : '暂无模型服务'} />}</QueryPanel>
    </Surface>
    {login && <ModelLogin key={login.id} request={login} changed={refresh} close={() => setLogin(undefined)} />}
  </div>;
}

type Fields = { provider?: string; modelId?: string; thinkingLevel?: ThinkingLevel };
function ModelPreferences({ value, catalog, unavailable }: { value: SettingsView; catalog?: ModelCatalog; unavailable: boolean }) {
  const client = useQueryClient(); const [form] = Form.useForm<Fields>(); const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  const [dirty, setDirty] = useState(false); const [saved, setSaved] = useState(false);
  const provider = Form.useWatch('provider', form) as string | undefined; const modelId = Form.useWatch('modelId', form) as string | undefined;
  const model = catalog?.models.find(item => item.provider === provider && item.modelId === modelId);
  const original = { provider: value.defaultModel?.provider, modelId: value.defaultModel?.modelId, thinkingLevel: value.thinkingLevel ?? undefined };
  useEffect(() => { if (!dirty) { form.resetFields(); form.setFieldsValue({ provider: value.defaultModel?.provider, modelId: value.defaultModel?.modelId, thinkingLevel: value.thinkingLevel ?? undefined }); } }, [value, dirty, form]);
  useEffect(() => {
    if (!dirty) return;
    const guard = (event: BeforeUnloadEvent) => { event.preventDefault(); };
    window.addEventListener('beforeunload', guard); return () => window.removeEventListener('beforeunload', guard);
  }, [dirty]);
  function restore() { form.resetFields(); form.setFieldsValue(original); setDirty(false); setSaved(false); setError(undefined); }
  async function submit(fields: Fields) {
    if (unavailable || (fields.modelId && !model)) return;
    setBusy(true); setError(undefined); setSaved(false);
    try {
      const updated = await api<SettingsView>('/settings', { method: 'PATCH', body: { defaultModel: fields.modelId ? { provider: fields.provider, modelId: fields.modelId } : null, thinkingLevel: fields.modelId ? fields.thinkingLevel ?? null : null } });
      client.setQueryData(['/settings'], updated); setDirty(false); setSaved(true);
    } catch (failure) { setError(failure); } finally { setBusy(false); }
  }
  const choices = catalog?.models.filter(item => item.provider === provider).map(item => ({ value: item.modelId, label: item.name, disabled: false })) ?? [];
  if (modelId && !choices.some(item => item.value === modelId)) choices.push({ value: modelId, label: `${modelId}（不可用）`, disabled: true });
  const providers = catalog?.providers.map(item => ({ value: item.id, label: item.name, disabled: !item.configured })) ?? [];
  if (provider && !providers.some(item => item.value === provider)) providers.push({ value: provider, label: `${provider}（不可用）`, disabled: true });
  const selected = catalog?.models.find(item => modelKey(item) === modelKey(value.defaultModel));
  return <Surface title="默认模型" className="model-preferences"><div className="current-model"><span className="current-model-name">{selected?.name ?? value.defaultModel?.modelId ?? '尚未选择模型'}</span>
    <span className="muted">{value.defaultModel ? `${catalog?.providers.find(item => item.id === value.defaultModel?.provider)?.name ?? value.defaultModel.provider}${value.thinkingLevel ? ` / ${thinkingLabels[value.thinkingLevel]}` : ''}` : '连接模型服务后选择'}</span></div>
    <Form form={form} layout="vertical" requiredMark={false} disabled={busy || unavailable} initialValues={original} onFinish={submit} onValuesChange={() => { setDirty(true); setSaved(false); }}>
      <div className="model-picker-grid"><Form.Item name="provider" label="模型服务"><Select showSearch optionFilterProp="label" placeholder="选择已连接的服务" options={providers} loading={!catalog} onChange={() => { form.setFieldValue('modelId', undefined); form.setFieldValue('thinkingLevel', undefined); }} /></Form.Item>
        <Form.Item name="modelId" label="模型"><Select allowClear showSearch optionFilterProp="label" placeholder="选择模型" options={choices} disabled={busy || unavailable || !provider} onChange={() => form.setFieldValue('thinkingLevel', undefined)} /></Form.Item>
        <Form.Item name="thinkingLevel" label="推理强度"><Select allowClear placeholder="模型默认" disabled={busy || unavailable || !model || !model.reasoning} options={model?.thinkingLevels.map(level => ({ value: level, label: thinkingLabels[level] }))} /></Form.Item></div>
      <ErrorNotice error={error} /><div className="form-actions">{saved && <span className="saved-status" role="status">已保存</span>}<Button disabled={!dirty || busy} onClick={restore}>还原</Button><Button type="primary" htmlType="submit" loading={busy} disabled={!dirty || unavailable || (!!modelId && !model)}>保存模型</Button></div>
    </Form>
  </Surface>;
}
