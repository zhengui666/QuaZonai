import { Button, Form, Input, InputNumber } from 'antd';
import { useEffect, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { DatasetRegistryView, DatasetRegistrationRequest as Request, DatasetRegistrationReceipt as Receipt } from '../../../packages/contracts/src/data-sources.ts';
import { api } from './api';
import { useResource } from './data-hooks';
import { ErrorNotice } from './ui';

export default function DatasetRegistration() {
  const { t } = useTranslation('settings'); const cache = useQueryClient();
  const registry = useResource<DatasetRegistryView>('/datasets/registry');
  const [form] = Form.useForm<Request>(); const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(); const [receipt, setReceipt] = useState<Receipt>();
  useEffect(() => { if (registry.data && form.getFieldValue('expectedRevision') === undefined) form.setFieldValue('expectedRevision', registry.data.revision); }, [registry.data, form]);
  async function register(value: Request) {
    setBusy(true); setError(undefined); setReceipt(undefined);
    try {
      const saved = await api<Receipt>('/datasets/register', { method: 'POST', body: value }); setReceipt(saved);
      await cache.invalidateQueries({ predicate: query => typeof query.queryKey[0] === 'string' && query.queryKey[0].startsWith('/datasets') });
    } catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  return <section className="settings-group"><header><h2>{t('Register a native catalog')}</h2></header><div className="settings-group-body">
    <p className="quiet">{t('Register an existing server-side DatasetConfig JSON descriptor that points to a Nautilus catalog. This does not download, copy, convert, or replay data.')}</p>
    <p className="quiet">{t('Data plugins are retired. Existing data and research bindings are preserved; old provider configuration files are no longer reloaded.')}</p>
    <ErrorNotice error={registry.error} retry={() => { void registry.refetch(); }} />
    <Form<Request> form={form} layout="vertical" disabled={busy} initialValues={{ requestId: crypto.randomUUID() }} onFinish={value => { void register(value); }}>
      <Form.Item name="path" label={t('Server descriptor path')} rules={[{ required: true }]}><Input autoComplete="off" /></Form.Item>
      <Form.Item name="requestId" label={t('Registration request ID')} rules={[{ required: true }]}><Input autoComplete="off" /></Form.Item>
      <Form.Item name="expectedRevision" label={t('Expected registry revision')} rules={[{ required: true, type: 'integer', min: 1 }]}><InputNumber min={1} precision={0} /></Form.Item>
      <p className="quiet">{t('Keep the same request ID, revision, and path when retrying an uncertain result. A successful replay returns the original receipt without rereading the file.')}</p>
      <ErrorNotice error={error} /><Button type="primary" htmlType="submit" loading={busy}>{t('Register dataset')}</Button>
      {receipt && <p role="status">{t('Registered dataset IDs')}: <code>{receipt.datasetIds.join(', ')}</code> · {t('Registry revision')}: {receipt.revision} · <code>{receipt.requestId}</code></p>}
    </Form>
  </div></section>;
}
