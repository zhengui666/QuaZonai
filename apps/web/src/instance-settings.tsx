import { Button, Form, Input, Select, Switch } from 'antd';
import { ArrowClockwiseIcon, PlusIcon, TrashIcon } from '@phosphor-icons/react';
import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { InstanceSettingsUpdate, InstanceSettingsView, NautilusConnection } from '../../../packages/contracts/src/index.ts';
import type { RemoteNativeInstance } from '../../../packages/contracts/src/nautilus.ts';
import { ApiError, api } from './api';
import { draftIsPersisted, readDraft, removeDraft, saveDraft } from './commands';
import { useResource } from './data-hooks';
import { DraftNotice, Empty, ErrorNotice, Loading } from './ui';

type Fields = { backtestEnabled: boolean; backtest?: RemoteNativeInstance; connections: NautilusConnection[] };
type Draft = { revision: number; values: Fields; pending?: InstanceSettingsUpdate };
const key = 'instance-settings';
const fields = (value: InstanceSettingsView): Fields => ({ backtestEnabled: !!value.backtest, backtest: value.backtest ?? undefined, connections: value.connections });
let memoryDraft: Draft | undefined;
const optional = (value?: string) => value || undefined;
function storeDraft(value: Draft) {
  memoryDraft = value;
  const { token: _backtestToken, ...backtest } = value.values.backtest ?? {};
  const values = { ...value.values, backtest: value.values.backtest ? backtest as RemoteNativeInstance : undefined, connections: (value.values.connections ?? []).map(({ token: _token, ...connection }) => connection) };
  const hasToken = !!value.pending?.backtest?.token || value.pending?.connections.some(connection => !!connection.token);
  return saveDraft(key, { revision: value.revision, values, ...(!hasToken && value.pending ? { pending: value.pending } : {}) });
}
function clearDraft() { memoryDraft = undefined; removeDraft(key); }
function content(value: Fields) {
  return {
    backtest: value.backtestEnabled ? { id: value.backtest?.id?.trim() ?? '', url: value.backtest?.url?.trim() ?? '', ...(optional(value.backtest?.token) ? { token: optional(value.backtest?.token) } : {}) } : null,
    connections: (value.connections ?? []).map(connection => ({ id: connection.id.trim(), name: connection.name.trim(), mode: connection.mode, url: connection.url.trim(), clientId: connection.clientId.trim(), accountId: connection.accountId.trim(), ...(optional(connection.token) ? { token: optional(connection.token) } : {}) })),
  };
}
const required = [{ required: true, whitespace: true, message: '请填写此项' }];
const address = [...required, { validator: (_rule: unknown, value: string) => {
  if (!value) return Promise.resolve();
  try { const url = new URL(value.trim()); if (['http:', 'https:'].includes(url.protocol) && !url.username && !url.password && !url.search && !url.hash) return Promise.resolve(); } catch { /* Form explains the accepted address. */ }
  return Promise.reject(new Error('请填写 HTTP 或 HTTPS 地址，不含密码、查询参数或片段'));
} }];

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
      initialized.current = true; const draft = memoryDraft ?? readDraft<Draft>(key);
      const values = draft?.values;
      if (values && !memoryDraft) {
        if (values.backtest && values.backtest.id === next.backtest?.id && values.backtest.url === next.backtest.url) values.backtest.token = next.backtest.token;
        values.connections = (values.connections ?? []).map(connection => ({ ...connection, token: next.connections.find(item => item.id === connection.id && item.url === connection.url)?.token }));
      }
      form.setFieldsValue(values ?? fields(next)); setRevision(draft?.revision ?? next.revision); setPending(draft?.pending);
    } else if (saved && JSON.stringify(content(form.getFieldsValue())) === JSON.stringify({ backtest: saved.backtest, connections: saved.connections }) && !pending) {
      form.resetFields(); form.setFieldsValue(fields(next)); setRevision(next.revision);
    }
    setSaved(next);
  }, [query.data]);
  const changed = () => { setError(undefined); setNotice(''); setDraftSaved(storeDraft({ revision: revision!, values: form.getFieldsValue() })); };
  async function save() {
    if (saving || revision === undefined) return;
    const value = pending ?? { expectedRevision: revision, ...content(form.getFieldsValue()) };
    setSaving(true); setPending(value); setError(undefined); setNotice(''); setDraftSaved(storeDraft({ revision: revision!, values: form.getFieldsValue(), pending: value }));
    try {
      const next = await api<InstanceSettingsView>('/settings/instances', { method: 'PUT', body: value });
      await cache.cancelQueries({ queryKey: ['/settings/instances'] });
      if (!mounted.current) { void cache.invalidateQueries({ queryKey: ['/settings/instances'] }); return; }
      const latest = cache.setQueryData<InstanceSettingsView>(['/settings/instances'], known => known && known.revision > next.revision ? known : next)!;
      void cache.invalidateQueries({ queryKey: ['/workspace'] }); void cache.invalidateQueries({ queryKey: ['/datasets'] });
      form.resetFields(); form.setFieldsValue(fields(latest)); setSaved(latest); setRevision(latest.revision); setPending(undefined); clearDraft(); setDraftSaved(true); setNotice('已保存');
    } catch (failure) {
      if (!mounted.current) return; setError(failure);
      if (failure instanceof ApiError && failure.status >= 400 && failure.status < 500) { setPending(undefined); setDraftSaved(storeDraft({ revision: revision!, values: form.getFieldsValue() })); void query.refetch(); }
    } finally { if (mounted.current) setSaving(false); }
  }
  const loadLatest = () => {
    if (!query.data) return; form.resetFields(); form.setFieldsValue(fields(query.data)); setSaved(query.data); setRevision(query.data.revision); setPending(undefined); setError(undefined); setNotice(''); clearDraft(); setDraftSaved(true);
  };
  return <div className="settings-page instance-settings"><header className="page-heading"><h2>自管 Nautilus 运行环境与连接</h2><Button icon={<ArrowClockwiseIcon size={15} />} disabled={saving || !!pending} onClick={() => { void query.refetch(); }}>刷新配置</Button></header>
    <p>选择并保存你已有的 Nautilus 运行环境连接。QuaZonai 部署不附带或自动启动 Nautilus；连接配置不是安装步骤，也不修改你的原生策略。</p>
    <p>当前表单仅支持已经接通的兼容 HTTP 接口。Nautilus 的进程内 API、官方消息总线和远程 HTTP 不是同一种接口；本机或远程原生环境的直接接入仍待验证，不能只填任意地址就宣称已连接。已有 Rust 安装用于运行新节点，与接入正在运行的节点是两种能力；本页尚未提供对应原生探测入口，保存不会探测版本或证明运行能力。</p>
    <p>官方 Redis 消息总线仅可读取节点已经配置并发布的流，不代表完整历史，也不自动提供启停或交易控制；本页尚未接入该只读来源。可执行文件路径也不是通用 Nautilus 接口，现有 runner 需要对应 JSON 协议，不能将任意 Rust 二进制视为可直接连接。</p>
    <p>保存连接只保存地址和访问配置，不安装或启动服务，不连接交易账户，不启动策略。账户配置保存、恢复连接和交易启动均为独立明确操作。</p>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending ? <Loading /> : !saved ? null : <Form form={form} layout="vertical" disabled={saving || !!pending} onValuesChange={changed} onFinish={() => { void save(); }}>
      <section className="settings-group instance-backtest"><header><h2>回测与研究</h2></header><div className="settings-group-body">
        <Form.Item label="启用回测连接" name="backtestEnabled" valuePropName="checked"><Switch aria-label="启用回测连接" /></Form.Item>
        {current?.backtestEnabled && <div className="instance-fields">
          <Form.Item label="实例标识" name={['backtest', 'id']} rules={required}><Input autoComplete="off" /></Form.Item>
          <Form.Item label="兼容适配器根地址" name={['backtest', 'url']} rules={address}><Input placeholder="https://adapter.example.com" autoComplete="off" /></Form.Item>
          <Form.Item label="访问令牌" name={['backtest', 'token']} extra="直接填写访问令牌；无需凭据时留空。"><Input.Password autoComplete="new-password" /></Form.Item>
        </div>}
      </div></section>
      <section className="settings-group"><header><h2>模拟盘与实盘</h2></header><div className="settings-group-body"><Form.List name="connections">{(items, { add, remove }) => <>
        {!items.length && <Empty title="尚无实例连接" />}
        {items.map(({ key: itemKey, name, ...rest }, index) => <section className="instance-card instance-connection" key={itemKey} aria-label={`账户连接 ${index + 1}`}>
          <header><h3>连接 {index + 1}</h3><Button icon={<TrashIcon size={15} />} aria-label={`移除连接 ${index + 1}`} onClick={() => { remove(name); changed(); }}>移除连接</Button></header>
          <Form.Item {...rest} label="名称" name={[name, 'name']} rules={required}><Input autoComplete="off" /></Form.Item>
          <div className="instance-pair"><Form.Item {...rest} label="模式" name={[name, 'mode']} rules={[{ required: true, message: '请选择模式' }]}><Select options={[{ value: 'paper', label: '模拟盘' }, { value: 'live', label: '实盘' }]} /></Form.Item>
          <Form.Item {...rest} label="实例标识" name={[name, 'id']} rules={required}><Input autoComplete="off" /></Form.Item></div>
          <Form.Item {...rest} label="兼容适配器账户地址" name={[name, 'url']} rules={address}><Input placeholder="https://adapter.example.com/accounts/your-account" autoComplete="off" /></Form.Item>
          <div className="instance-pair"><Form.Item {...rest} label="客户端标识" name={[name, 'clientId']} rules={required}><Input autoComplete="off" /></Form.Item>
          <Form.Item {...rest} label="账户标识" name={[name, 'accountId']} rules={required}><Input autoComplete="off" /></Form.Item></div>
          <Form.Item {...rest} label="访问令牌" name={[name, 'token']}><Input.Password autoComplete="new-password" /></Form.Item>
        </section>)}
        <Button icon={<PlusIcon size={15} />} onClick={() => { add({ id: crypto.randomUUID(), name: '', mode: 'paper', url: '', clientId: '', accountId: '' }); changed(); }}>添加连接</Button>
      </>}</Form.List></div></section>
      <p className="quiet">保存后用于新研究、回测及新观测。已有任务继续使用原连接。未保存的访问令牌仅保留在当前页面会话，刷新页面后需重新填写。</p>
      {saved.revision !== revision && <div className="instance-conflict" role="status">配置已有更新。<Button disabled={saving || !!pending} onClick={loadLatest}>载入最新配置</Button><span className="quiet">将替换当前未保存内容</span></div>}
      <ErrorNotice error={error} /><DraftNotice saved={draftSaved} /><div className="settings-save"><Button type="primary" htmlType={pending ? 'button' : 'submit'} loading={saving} disabled={!dirty && !pending} onClick={pending ? () => { void save(); } : undefined}>{saving ? '保存中' : pending ? '核对保存' : '保存实例连接'}</Button><span role="status" className="quiet">{notice || (pending ? '保存结果待核对' : dirty ? '未保存' : '')}</span></div>
    </Form>}
  </div>;
}
