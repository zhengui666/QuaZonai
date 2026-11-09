import { Button, Form, Input, InputNumber, Select, Switch } from 'antd';
import { ArrowClockwiseIcon } from '@phosphor-icons/react';
import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { DataSourceRefreshView, DataSourceUpdate, DataSourceView } from '../../../packages/contracts/src/data-sources.ts';
import { api } from './api';
import { useResource } from './data-hooks';
import { Empty, ErrorNotice, Loading, Status, Timestamp } from './ui';

const formValues = (source: DataSourceView) => ({ enabled: source.enabled, config: { ...source.config, ...Object.fromEntries(source.fields.map(field => [field.key, source.config[field.key]])) } });

function DataPlugin({ source }: { source: DataSourceView }) {
  const cache = useQueryClient(); const [form] = Form.useForm<DataSourceUpdate>();
  const values = Form.useWatch([], form) as DataSourceUpdate | undefined;
  const [busy, setBusy] = useState<'save' | 'refresh'>(); const [error, setError] = useState<unknown>(); const [notice, setNotice] = useState('');
  const saved = formValues(source);
  const dirty = !!values && JSON.stringify(values) !== JSON.stringify(saved);
  const previous = useRef(saved);
  const observer = useRef<EventSource | null>(null);
  useEffect(() => () => { observer.current?.close(); }, []);
  useEffect(() => {
    if (JSON.stringify(form.getFieldsValue(true)) === JSON.stringify(previous.current)) { form.resetFields(); form.setFieldsValue(saved); }
    previous.current = saved;
  }, [source]);
  async function update(operation: 'save' | 'refresh') {
    if (busy) return;
    setBusy(operation); setError(undefined); setNotice('');
    try {
      const path = `/data-sources/${encodeURIComponent(source.name)}`;
      if (operation === 'refresh') {
        const receipt = await api<{ taskId: number }>(`${path}/refresh`, { method: 'POST' });
        if (!Number.isSafeInteger(receipt.taskId) || receipt.taskId <= 0) throw new Error('数据加载回执不可用');
        setNotice('已开始加载数据'); observer.current?.close();
        const events = new EventSource(`/api/v3${path}/events?taskId=${receipt.taskId}`);
        observer.current = events;
        events.addEventListener('data-source', (message: MessageEvent<string>) => {
          let state: DataSourceRefreshView;
          try {
            state = JSON.parse(message.data) as DataSourceRefreshView;
            if (state.source?.name !== source.name || typeof state.status !== 'string') throw new Error('invalid_source_event');
          }
          catch { events.close(); setBusy(undefined); setError(new Error('数据加载状态不可用，请刷新目录')); return; }
          if (state.taskId !== receipt.taskId) return;
          cache.setQueryData<DataSourceView[]>(['/data-sources'], known => known?.map(item => item.name === state.source.name ? state.source : item));
          if (state.status !== 'terminal') return;
          events.close(); observer.current = null; setBusy(undefined);
          if (state.outcome === 'completed') setNotice('已加载数据');
          else { setNotice(''); setError(new Error(state.source.error ?? '数据加载未完成')); }
          void Promise.all(['/datasets', '/extensions', '/data-sources'].map(path => cache.invalidateQueries({ queryKey: [path] })));
        });
        events.onerror = () => {
          if (events.readyState === EventSource.CLOSED) {
            setBusy(undefined); setNotice(''); setError(new Error('加载进度暂时不可用，请刷新目录核对原任务'));
          } else setNotice('数据仍在加载，正在重新连接进度');
        };
        return;
      }
      const next = await api<DataSourceView>(path, { method: 'PUT', body: form.getFieldsValue(true) });
      cache.setQueryData<DataSourceView[]>(['/data-sources'], known => known?.map(item => item.name === next.name ? next : item));
      form.resetFields(); form.setFieldsValue(formValues(next));
      setNotice(next.enabled ? '已保存配置，可加载数据' : '已保存配置');
      await Promise.all(['/datasets', '/extensions', '/data-sources'].map(path => cache.invalidateQueries({ queryKey: [path] })));
    } catch (failure) { setError(failure); setBusy(undefined); }
    finally { if (operation === 'save') setBusy(undefined); }
  }
  return <section className="settings-group" aria-label={source.name}>
    <header><h2>{source.name}</h2><code>{source.version ?? '未提供版本'}</code><Status plain value={source.state} /></header>
    <div className="settings-group-body"><Form form={form} layout="vertical" initialValues={saved} disabled={!!busy} onValuesChange={() => { setNotice(''); setError(undefined); }} onFinish={() => { void update('save'); }}>
      <Form.Item label="启用插件" name="enabled" valuePropName="checked"><Switch aria-label={`启用 ${source.name}`} /></Form.Item>
      {source.fields.map(field => <Form.Item key={field.key} label={field.label} name={['config', field.key]} extra={field.description} valuePropName={field.type === 'boolean' ? 'checked' : 'value'} rules={field.required && values?.enabled ? [{ required: true, message: `请填写${field.label}` }] : undefined}>
        {field.choices ? <Select allowClear options={field.choices} placeholder="选择输入类型" /> : field.type === 'password' ? <Input.Password autoComplete="new-password" /> : field.type === 'number' ? <InputNumber style={{ width: '100%' }} /> : field.type === 'boolean' ? <Switch aria-label={field.label} /> : <Input autoComplete="off" />}
      </Form.Item>)}
      <p className="quiet">{source.datasetIds.length} 个数据集 · 上次加载 <Timestamp value={source.lastLoadedAt} /></p>
      {source.acquisition && <div aria-label="最近完成的原始采集">
        <p>原始来源已保存。完成目录准备与验证后可用于研究。</p>
        <p>采集清单：<code style={{ overflowWrap: 'anywhere' }}>{source.acquisition.manifestPath}</code></p>
        <details><summary>来源计划</summary><code style={{ overflowWrap: 'anywhere' }}>{source.acquisition.planId}</code></details>
      </div>}
      {source.error && <p className="error-notice" role="alert">{source.error}</p>}<ErrorNotice error={error} />
      <div className="settings-save"><Button type="primary" htmlType="submit" loading={busy === 'save'} disabled={!dirty || !!busy}>保存配置</Button><Button loading={busy === 'refresh'} disabled={dirty || !source.enabled || !!busy} onClick={() => { void update('refresh'); }}>加载数据</Button></div><p className="quiet" role="status">{dirty ? '请先保存配置，再加载数据' : notice}</p>
    </Form></div>
  </section>;
}
export default function DataPlugins() {
  const query = useResource<DataSourceView[]>('/data-sources');
  return <div className="settings-page"><header className="page-heading"><h2>数据插件</h2><Button icon={<ArrowClockwiseIcon size={15} />} onClick={() => { void query.refetch(); }}>刷新目录</Button></header>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title="没有已发现的数据插件" /> : query.data.map(source => <DataPlugin key={source.name} source={source} />)}
  </div>;
}
