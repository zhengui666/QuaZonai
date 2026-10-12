import { Button, Form, Input, InputNumber, Select } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import type { ModelView, SettingsView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { ErrorNotice, modelKey, PageHeading, QueryPanel, RefreshButton, Surface, useResource } from './ui';

type Fields = { name: string; defaultModel?: string; maxTrials: number; taskTimeoutSeconds: number; nativeTimeoutSeconds: number };
export function Settings() {
  const settings = useResource<SettingsView>('/settings');
  return <div className="page"><PageHeading title="设置" action={<RefreshButton loading={settings.isFetching} label="刷新设置" onClick={() => { void settings.refetch(); }} />} />
    <QueryPanel query={settings}>{settings.data && <SettingsForm value={settings.data} />}</QueryPanel>
  </div>;
}
function SettingsForm({ value }: { value: SettingsView }) {
  const models = useResource<ModelView[]>('/research/models'); const client = useQueryClient(); const [form] = Form.useForm<Fields>();
  const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>(); const [saved, setSaved] = useState(false); const [dirty, setDirty] = useState(false);
  useEffect(() => { if (!dirty) form.setFieldsValue({ ...value, defaultModel: modelKey(value.defaultModel) }); }, [value, form, dirty]);
  function restore() { form.setFieldsValue({ ...value, defaultModel: modelKey(value.defaultModel) }); setDirty(false); setError(undefined); setSaved(false); }
  async function submit(fields: Fields) {
    const model = models.data?.find(item => modelKey(item) === fields.defaultModel);
    if (fields.defaultModel && !model && fields.defaultModel !== modelKey(value.defaultModel)) { setError(new Error('请选择可用模型')); return; }
    setBusy(true); setError(undefined); setSaved(false);
    try {
      const updated = await api<SettingsView>('/settings', { method: 'PATCH', body: { name: fields.name.trim(), defaultModel: model ? { provider: model.provider, modelId: model.modelId } : fields.defaultModel ? value.defaultModel : null, maxTrials: fields.maxTrials, taskTimeoutSeconds: fields.taskTimeoutSeconds, nativeTimeoutSeconds: fields.nativeTimeoutSeconds } });
      client.setQueryData(['/settings'], updated); setDirty(false); setSaved(true);
    } catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  const options = models.data?.map(item => ({ value: modelKey(item), label: `${item.name} · ${item.provider}` })) ?? [];
  if (value.defaultModel && !options.some(item => item.value === modelKey(value.defaultModel))) options.push({ value: modelKey(value.defaultModel), label: `${value.defaultModel.modelId} · ${value.defaultModel.provider}` });
  return <Surface title="应用" className="settings-surface"><ErrorNotice error={models.error} retry={() => { void models.refetch(); }} />
    <Form form={form} layout="vertical" requiredMark={false} disabled={busy} initialValues={{ ...value, defaultModel: modelKey(value.defaultModel) }} onFinish={submit} onValuesChange={() => { setDirty(true); setSaved(false); }}>
      <Form.Item name="name" label="应用名称" rules={[{ required: true, whitespace: true, message: '请输入名称' }]}><Input /></Form.Item>
      <Form.Item name="defaultModel" label="默认模型"><Select allowClear showSearch optionFilterProp="label" placeholder="—" loading={models.isPending} options={options} notFoundContent={models.isPending ? '加载中' : '暂无模型'} /></Form.Item>
      <div className="form-grid"><Form.Item name="maxTrials" label="最多试验" rules={[{ required: true, message: '请输入试验次数' }]}><InputNumber min={1} precision={0} /></Form.Item>
        <Form.Item name="taskTimeoutSeconds" label="任务超时 / 秒" rules={[{ required: true, message: '请输入超时' }]}><InputNumber min={1} precision={0} /></Form.Item>
        <Form.Item name="nativeTimeoutSeconds" label="原生计算超时 / 秒" rules={[{ required: true, message: '请输入超时' }]}><InputNumber min={1} precision={0} /></Form.Item></div>
      <ErrorNotice error={error} /><div className="form-actions">{saved && <span className="saved-status" role="status">已保存</span>}<Button disabled={!dirty || busy} onClick={restore}>还原</Button><Button type="primary" htmlType="submit" loading={busy} disabled={!dirty}>保存设置</Button></div>
    </Form>
  </Surface>;
}
