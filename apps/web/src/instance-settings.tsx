import { Button, Form, Input, Select, Switch } from 'antd';
import { ArrowClockwiseIcon, PlusIcon, TrashIcon } from '@phosphor-icons/react';
import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { InstanceSettingsUpdate, InstanceSettingsView, NautilusConnection } from '../../../packages/contracts/src/index.ts';
import type { RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { ApiError, api } from './api';
import { draftIsPersisted, readDraft, removeDraft, saveDraft } from './commands';
import { useResource } from './data-hooks';
import { DraftNotice, Empty, ErrorNotice, Loading } from './ui';

type Fields = { backtestEnabled: boolean; backtest?: RemoteNativeTarget; connections: NautilusConnection[] };
type Draft = { revision: number; values: Fields; pending?: InstanceSettingsUpdate };
const key = 'instance-settings';
const fields = (value: InstanceSettingsView): Fields => ({ backtestEnabled: !!value.backtest, backtest: value.backtest ?? undefined, connections: value.connections });
const optional = (value?: string) => value?.trim() || undefined;
function content(value: Fields) {
  return {
    backtest: value.backtestEnabled ? { id: value.backtest?.id?.trim() ?? '', url: value.backtest?.url?.trim() ?? '', ...(optional(value.backtest?.tokenEnv) ? { tokenEnv: optional(value.backtest?.tokenEnv) } : {}) } : null,
    connections: (value.connections ?? []).map(connection => ({ id: connection.id.trim(), name: connection.name.trim(), mode: connection.mode, url: connection.url.trim(), clientId: connection.clientId.trim(), accountId: connection.accountId.trim(), ...(optional(connection.tokenEnv) ? { tokenEnv: optional(connection.tokenEnv) } : {}) })),
  };
}
const required = [{ required: true, whitespace: true, message: '请填写此项' }];
const address = [...required, { validator: (_rule: unknown, value: string) => {
  if (!value) return Promise.resolve();
  try { const url = new URL(value.trim()); if (['http:', 'https:'].includes(url.protocol) && !url.username && !url.password && !url.search && !url.hash) return Promise.resolve(); } catch { /* Form explains the accepted address. */ }
  return Promise.reject(new Error('请填写 HTTP 或 HTTPS 地址，不含密码、查询参数或片段'));
} }];
const credential = [{ pattern: /^[A-Za-z_][A-Za-z0-9_]*$/, message: '请填写服务端环境变量名称' }];

export default function InstanceSettings() {
  const query = useResource<InstanceSettingsView>('/settings/instances'); const cache = useQueryClient(); const [form] = Form.useForm<Fields>();
  const current = Form.useWatch([], form) as Fields | undefined;
  const [saved, setSaved] = useState<InstanceSettingsView>(); const [revision, setRevision] = useState<number>();
  const [saving, setSaving] = useState(false); const [pending, setPending] = useState<InstanceSettingsUpdate>();
  const [error, setError] = useState<unknown>(); const [notice, setNotice] = useState(''); const [draftSaved, setDraftSaved] = useState(() => draftIsPersisted(key));
  const initialized = useRef(false); const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const dirty = !!saved && !!current && JSON.stringify(content(current)) !== JSON.stringify({ backtest: saved.backtest, connections: saved.connections });
  useEffect(() => {
    const next = query.data; if (!next) return;
    if (!initialized.current) {
      initialized.current = true; const draft = readDraft<Draft>(key);
      form.setFieldsValue(draft?.values ?? fields(next)); setRevision(draft?.revision ?? next.revision); setPending(draft?.pending);
    } else if (saved && JSON.stringify(content(form.getFieldsValue())) === JSON.stringify({ backtest: saved.backtest, connections: saved.connections }) && !pending) {
      form.resetFields(); form.setFieldsValue(fields(next)); setRevision(next.revision);
    }
    setSaved(next);
  }, [query.data]);
  const changed = () => { setError(undefined); setNotice(''); setDraftSaved(saveDraft(key, { revision, values: form.getFieldsValue() })); };
  async function save() {
    if (saving || revision === undefined) return;
    const value = pending ?? { expectedRevision: revision, ...content(form.getFieldsValue()) };
    setSaving(true); setPending(value); setError(undefined); setNotice(''); setDraftSaved(saveDraft(key, { revision, values: form.getFieldsValue(), pending: value }));
    try {
      const next = await api<InstanceSettingsView>('/settings/instances', { method: 'PUT', body: value });
      await cache.cancelQueries({ queryKey: ['/settings/instances'] });
      if (!mounted.current) { void cache.invalidateQueries({ queryKey: ['/settings/instances'] }); return; }
      const latest = cache.setQueryData<InstanceSettingsView>(['/settings/instances'], known => known && known.revision > next.revision ? known : next)!;
      void cache.invalidateQueries({ queryKey: ['/workspace'] }); void cache.invalidateQueries({ queryKey: ['/datasets'] });
      form.resetFields(); form.setFieldsValue(fields(latest)); setSaved(latest); setRevision(latest.revision); setPending(undefined); removeDraft(key); setDraftSaved(true); setNotice('已保存');
    } catch (failure) {
      if (!mounted.current) return; setError(failure);
      if (failure instanceof ApiError && failure.status >= 400 && failure.status < 500) { setPending(undefined); setDraftSaved(saveDraft(key, { revision, values: form.getFieldsValue() })); void query.refetch(); }
    } finally { if (mounted.current) setSaving(false); }
  }
  const loadLatest = () => {
    if (!query.data) return; form.resetFields(); form.setFieldsValue(fields(query.data)); setSaved(query.data); setRevision(query.data.revision); setPending(undefined); setError(undefined); setNotice(''); removeDraft(key); setDraftSaved(true);
  };
  return <div className="settings-page instance-settings"><header className="page-heading"><h2>实例连接</h2><Button icon={<ArrowClockwiseIcon size={15} />} disabled={saving || !!pending} onClick={() => { void query.refetch(); }}>刷新配置</Button></header>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !saved ? null : <Form form={form} layout="vertical" disabled={saving || !!pending} onValuesChange={changed} onFinish={() => { void save(); }}>
      <section className="settings-group instance-backtest"><header><h2>回测与研究</h2></header><div className="settings-group-body">
        <Form.Item label="启用回测连接" name="backtestEnabled" valuePropName="checked"><Switch aria-label="启用回测连接" /></Form.Item>
        {current?.backtestEnabled && <div className="instance-fields">
          <Form.Item label="实例标识" name={['backtest', 'id']} rules={required}><Input autoComplete="off" /></Form.Item>
          <Form.Item label="服务地址" name={['backtest', 'url']} rules={address}><Input placeholder="https://nautilus.example.com" autoComplete="off" /></Form.Item>
          <Form.Item label="凭据环境变量" name={['backtest', 'tokenEnv']} rules={credential} extra="填写 QuaZonai 服务端已配置的变量名；无需凭据时留空。"><Input placeholder="QZ_BACKTEST_TOKEN" autoComplete="off" /></Form.Item>
        </div>}
      </div></section>
      <section className="settings-group"><header><h2>模拟盘与实盘</h2></header><div className="settings-group-body"><Form.List name="connections">{(items, { add, remove }) => <>
        {!items.length && <Empty title="尚无实例连接" />}
        {items.map(({ key: itemKey, name, ...rest }, index) => <section className="instance-card instance-connection" key={itemKey} aria-label={`账户连接 ${index + 1}`}>
          <header><h3>连接 {index + 1}</h3><Button icon={<TrashIcon size={15} />} aria-label={`移除连接 ${index + 1}`} onClick={() => { remove(name); changed(); }}>移除连接</Button></header>
          <Form.Item {...rest} label="名称" name={[name, 'name']} rules={required}><Input autoComplete="off" /></Form.Item>
          <div className="instance-pair"><Form.Item {...rest} label="模式" name={[name, 'mode']} rules={[{ required: true, message: '请选择模式' }]}><Select options={[{ value: 'paper', label: '模拟盘' }, { value: 'live', label: '实盘' }]} /></Form.Item>
          <Form.Item {...rest} label="实例标识" name={[name, 'id']} rules={required}><Input autoComplete="off" /></Form.Item></div>
          <Form.Item {...rest} label="服务地址" name={[name, 'url']} rules={address}><Input placeholder="https://nautilus.example.com" autoComplete="off" /></Form.Item>
          <div className="instance-pair"><Form.Item {...rest} label="客户端标识" name={[name, 'clientId']} rules={required}><Input autoComplete="off" /></Form.Item>
          <Form.Item {...rest} label="账户标识" name={[name, 'accountId']} rules={required}><Input autoComplete="off" /></Form.Item></div>
          <Form.Item {...rest} label="凭据环境变量" name={[name, 'tokenEnv']} rules={credential}><Input placeholder="QZ_ACCOUNT_TOKEN" autoComplete="off" /></Form.Item>
        </section>)}
        <Button icon={<PlusIcon size={15} />} onClick={() => { add({ id: crypto.randomUUID(), name: '', mode: 'paper', url: '', clientId: '', accountId: '' }); changed(); }}>添加连接</Button>
      </>}</Form.List></div></section>
      <p className="quiet">保存后用于新研究、回测及新观测。已有任务继续使用原连接。</p>
      {saved.revision !== revision && <div className="instance-conflict" role="status">配置已有更新。<Button disabled={saving || !!pending} onClick={loadLatest}>载入最新配置</Button><span className="quiet">将替换当前未保存内容</span></div>}
      <ErrorNotice error={error} /><DraftNotice saved={draftSaved} /><div className="settings-save"><Button type="primary" htmlType={pending ? 'button' : 'submit'} loading={saving} disabled={!dirty && !pending} onClick={pending ? () => { void save(); } : undefined}>{saving ? '保存中' : pending ? '核对保存' : '保存实例连接'}</Button><span role="status" className="quiet">{notice || (pending ? '保存结果待核对' : dirty ? '未保存' : '')}</span></div>
    </Form>}
  </div>;
}
