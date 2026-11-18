import { useTranslation } from 'react-i18next';
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
 const { t, i18n } = useTranslation('settings');

  const cache = useQueryClient(); const [form] = Form.useForm<DataSourceUpdate>();
  const values = Form.useWatch([], form) as DataSourceUpdate | undefined;
  const [busy, setBusy] = useState<'save' | 'refresh'>(); const [error, setError] = useState<unknown>(); const [notice, setNotice] = useState('');
  const saved = formValues(source);
  // Built-in field labels are interface text; third-party plugin labels are left intact.
  const builtInLabels: Record<string, Record<string, string>> = {
    'qz.data.local-config': { path: 'Dataset configuration file' },
    'qz.data.cryptohftdata': { archiveDirectory: 'CryptoHFTData archive directory', preparedConfig: 'Prepared dataset configuration' },
  };
  const knownLabels = Object.hasOwn(builtInLabels, source.name) ? builtInLabels[source.name] : undefined;
  const fields = source.fields.map(field => ({ ...field, label: knownLabels && Object.hasOwn(knownLabels, field.key) ? t(knownLabels[field.key]!) : field.label }));
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
        if (!Number.isSafeInteger(receipt.taskId) || receipt.taskId <= 0) throw new Error(t("Data-loading receipt is unavailable"));
        setNotice(t("Data loading started")); observer.current?.close();
        const events = new EventSource(`/api/v3${path}/events?taskId=${receipt.taskId}`);
        observer.current = events;
        events.addEventListener('data-source', (message: MessageEvent<string>) => {
          let state: DataSourceRefreshView;
          try {
            state = JSON.parse(message.data) as DataSourceRefreshView;
            if (state.source?.name !== source.name || typeof state.status !== 'string') throw new Error('invalid_source_event');
          }
          catch { events.close(); setBusy(undefined); setError(new Error(t("Data-loading status is unavailable; refresh the catalog"))); return; }
          if (state.taskId !== receipt.taskId) return;
          cache.setQueryData<DataSourceView[]>(['/data-sources'], known => known?.map(item => item.name === state.source.name ? state.source : item));
          if (state.status !== 'terminal') return;
          events.close(); observer.current = null; setBusy(undefined);
          if (state.outcome === 'completed') setNotice(t("Data loaded"));
          else { setNotice(''); setError(new Error(state.source.error ?? t("Data loading is incomplete"))); }
          void Promise.all(['/datasets', '/extensions', '/data-sources'].map(path => cache.invalidateQueries({ predicate: query => query.queryKey[0] === path || path === '/datasets' && typeof query.queryKey[0] === 'string' && (query.queryKey[0].startsWith('/datasets?') || query.queryKey[0].startsWith('/datasets/')) })));
        });
        events.onerror = () => {
          if (events.readyState === EventSource.CLOSED) {
            setBusy(undefined); setNotice(''); setError(new Error(t("Loading progress is temporarily unavailable. Refresh the catalog to check the original task")));
          } else setNotice(t("Data is still loading; reconnecting to progress"));
        };
        return;
      }
      const next = await api<DataSourceView>(path, { method: 'PUT', body: form.getFieldsValue(true) });
      cache.setQueryData<DataSourceView[]>(['/data-sources'], known => known?.map(item => item.name === next.name ? next : item));
      form.resetFields(); form.setFieldsValue(formValues(next));
      setNotice(next.enabled ? t("Configuration saved; data can now be loaded") : t("Configuration saved"));
      await Promise.all(['/datasets', '/extensions', '/data-sources'].map(path => cache.invalidateQueries({ predicate: query => query.queryKey[0] === path || path === '/datasets' && typeof query.queryKey[0] === 'string' && (query.queryKey[0].startsWith('/datasets?') || query.queryKey[0].startsWith('/datasets/')) })));
    } catch (failure) { setError(failure); setBusy(undefined); }
    finally { if (operation === 'save') setBusy(undefined); }
  }
  return <section className="settings-group" aria-label={source.name}>
    <header><h2>{source.name}</h2><code>{source.version ?? t("No version provided")}</code><Status plain value={source.state} /></header>
    <div className="settings-group-body"><Form form={form} layout="vertical" initialValues={saved} disabled={!!busy} onValuesChange={() => { setNotice(''); setError(undefined); }} onFinish={() => { void update('save'); }}>
      <Form.Item label={t("Enable plugin")} name="enabled" valuePropName="checked"><Switch aria-label={t("Enable {{v0}}", { v0: source.name })} /></Form.Item>
      {fields.map(field => <Form.Item key={field.key} label={field.label} name={['config', field.key]} valuePropName={field.type === 'boolean' ? 'checked' : 'value'} rules={field.required && values?.enabled ? [{ required: true, message: t("Enter {{v0}}", { v0: field.label }) }] : undefined}>
        {field.choices ? <Select allowClear options={field.choices} placeholder={t("Choose input type")} /> : field.type === 'password' ? <Input.Password autoComplete="new-password" /> : field.type === 'number' ? <InputNumber style={{ width: '100%' }} /> : field.type === 'boolean' ? <Switch aria-label={field.label} /> : <Input autoComplete="off" />}
      </Form.Item>)}
      <p className="quiet">{t("Datasets: {{countText}} · Last loaded", { countText: new Intl.NumberFormat(i18n.language).format(source.datasetIds.length) })}<Timestamp value={source.lastLoadedAt} /></p>
      {source.acquisition && <div aria-label={t("Latest completed original acquisition")}>

        <p>{t("Acquisition manifest:")}<code style={{ overflowWrap: 'anywhere' }}>{source.acquisition.manifestPath}</code></p>
        <details><summary>{t("Source plan")}</summary><code style={{ overflowWrap: 'anywhere' }}>{source.acquisition.planId}</code></details>
      </div>}
      {source.error && <p className="error-notice" role="alert">{source.error}</p>}<ErrorNotice error={error} />
      <div className="settings-save"><Button type="primary" htmlType="submit" loading={busy === 'save'} disabled={!dirty || !!busy}>{t("Save configuration")}</Button><Button loading={busy === 'refresh'} disabled={dirty || !source.enabled || !!busy} onClick={() => { void update('refresh'); }}>{t("Load data")}</Button></div><p className="quiet" role="status">{dirty ? t("Save configuration before loading data") : notice}</p>
    </Form></div>
  </section>;
}
export default function DataPlugins() {
 const { t } = useTranslation('settings');

  const query = useResource<DataSourceView[]>('/data-sources');
  return <div className="settings-page"><header className="page-heading"><h2>{t("Data plugins")}</h2><Button icon={<ArrowClockwiseIcon size={15} />} onClick={() => { void query.refetch(); }}>{t("Refresh catalog")}</Button></header>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title={t("No data plugins discovered")} /> : query.data.map(source => <DataPlugin key={source.name} source={source} />)}
  </div>;
}
