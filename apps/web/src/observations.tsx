import { App, Button, Form, Modal, Select, Table, Tabs } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { lazy, Suspense, useState } from 'react';
import type { ObservationMode, ObservationStart, ObservationView, PortfolioView, PositionView, SettingsView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { display, ErrorNotice, isActive, isReady, NoData, number, PageHeading, QueryPanel, RefreshButton, Status, Surface, time, useResource } from './ui';
const EquityPlot = lazy(() => import('./equity-plot'));

export function Observations() {
  const listing = useResource<ObservationView[]>('/observations', 8000); const [mode, setMode] = useState<ObservationMode>('paper');
  const [selected, setSelected] = useState<string>(); const [creating, setCreating] = useState(false);
  const items = listing.data?.filter(item => item.mode === mode) ?? []; const value = items.find(item => item.id === selected) ?? items[0];
  return <div className="page"><PageHeading title="盘面观测" count={listing.data ? items.length : undefined} action={<><RefreshButton loading={listing.isFetching} label="刷新观测列表" onClick={() => { void listing.refetch(); }} /><Button type="primary" icon={<PlusOutlined aria-hidden />} onClick={() => setCreating(true)}>新建观测</Button></>} />
    <Tabs activeKey={mode} onChange={value => { setMode(value as ObservationMode); setSelected(undefined); }} items={[{ key: 'paper', label: '模拟盘' }, { key: 'live', label: '实盘' }]} />
    <QueryPanel query={listing}>{value ? <div className="observation-layout"><section className="surface selection-list" aria-label="观测列表">{items.map(item => <button key={item.id} aria-pressed={value.id === item.id} onClick={() => setSelected(item.id)}><strong>{display(item.name)}</strong><span className="selection-meta"><Status value={item.status} /><span>{display(item.currency)}</span></span></button>)}</section>
      <ObservationRecord key={value.id} value={value} stale={listing.isError} />
    </div> : <Surface><NoData text={mode === 'paper' ? '暂无模拟盘观测' : '暂无实盘观测'} /></Surface>}</QueryPanel>
    {creating && <ObservationForm mode={mode} close={() => setCreating(false)} started={value => { setSelected(value.id); setCreating(false); }} />}
  </div>;
}
function ObservationRecord({ value, stale }: { value: ObservationView; stale: boolean }) {
  const client = useQueryClient(); const { message } = App.useApp(); const [busy, setBusy] = useState<'refresh' | 'stop'>(); const [error, setError] = useState<unknown>();
  async function act(action: 'refresh' | 'stop') {
    setBusy(action); setError(undefined);
    try {
      const updated = await api<ObservationView>(`/observations/${encodeURIComponent(value.id)}/${action}`, { method: 'POST' });
      client.setQueryData<ObservationView[]>(['/observations'], previous => previous?.map(item => item.id === updated.id ? updated : item));
      void message.success(action === 'refresh' ? '已刷新' : '已接纳');
    } catch (failure) { setError(failure); }
    finally { setBusy(undefined); }
  }
  const receipt = value.receipt;
  return <div className="detail-stack"><Surface><div className="detail-heading"><h2>{display(value.name)}</h2><Status value={value.status} /></div>
    <div className="toolbar" style={{ marginBlock: '16px 24px' }}><Button onClick={() => { void act('refresh'); }} loading={busy === 'refresh'} disabled={!!busy || stale}>刷新账户</Button><Button onClick={() => { void act('stop'); }} loading={busy === 'stop'} disabled={!!busy || stale || !isActive(value.status)}>停止观测</Button><span className="muted">{time(value.updatedAt)}</span></div>
    <ErrorNotice error={error} /><ErrorNotice error={value.error} /><dl className="account-grid"><div><dt>账户净值</dt><dd>{number(value.equity)}<small>{display(value.currency)}</small></dd></div><div><dt>可用现金</dt><dd>{number(value.cash)}<small>{display(value.currency)}</small></dd></div></dl>
  </Surface><Surface><Suspense fallback={<div role="status">加载曲线</div>}><EquityPlot points={value.curve} currency={value.currency} title="账户曲线" /></Suspense></Surface>
    <Surface title="持仓"><Table<PositionView> rowKey="instrumentId" dataSource={value.positions} pagination={false} size="small" locale={{ emptyText: '暂无持仓' }} columns={[
      { title: '标的', dataIndex: 'instrumentId', render: display }, { title: '数量', dataIndex: 'quantity', align: 'right', render: display },
      { title: '市值', dataIndex: 'value', align: 'right', render: display }, { title: '盈亏', dataIndex: 'pnl', align: 'right', render: display },
    ]} /></Surface><Surface title="目标回执" extra={<Status value={receipt?.status} />}>{receipt ? <dl className="receipt-grid">
      <div><dt>回执</dt><dd>{display(receipt.id)}</dd></div><div><dt>外部编号</dt><dd>{display(receipt.externalId)}</dd></div>
      <div><dt>到期</dt><dd>{time(receipt.expiresAt)}</dd></div><div><dt>确认</dt><dd>{time(receipt.acknowledgedAt)}</dd></div>
    </dl> : <NoData text="暂无回执" />}</Surface>
  </div>;
}
function ObservationForm({ mode, close, started }: { mode: ObservationMode; close: () => void; started: (value: ObservationView) => void }) {
  const portfolios = useResource<PortfolioView[]>('/portfolios'); const settings = useResource<SettingsView>('/settings'); const client = useQueryClient(); const { message } = App.useApp();
  const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>(); const [pending, setPending] = useState<{ fields: string; body: ObservationStart }>();
  const ready = portfolios.data?.filter(item => isReady(item.status)) ?? []; const connections = settings.data?.connections.filter(item => item.mode === mode) ?? [];
  async function submit(values: { portfolioId: string; connectionId: string }) {
    setBusy(true); setError(undefined); const fields = JSON.stringify(values); const body = pending?.fields === fields ? pending.body : { requestId: crypto.randomUUID(), mode, ...values }; setPending({ fields, body });
    try {
      const value = await api<ObservationView>('/observations', { method: 'POST', body });
      client.setQueryData<ObservationView[]>(['/observations'], previous => previous ? [...previous.filter(item => item.id !== value.id), value] : [value]);
      void client.invalidateQueries({ queryKey: ['/observations'] }); void message.success('已接纳'); started(value);
    } catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  return <Modal title={mode === 'paper' ? '新建模拟盘观测' : '新建实盘观测'} open onCancel={close} footer={null} width={520} closable={!busy} maskClosable={!busy} keyboard={!busy}>
    <ErrorNotice error={portfolios.error} retry={() => { void portfolios.refetch(); }} /><ErrorNotice error={settings.error} retry={() => { void settings.refetch(); }} />
    <Form layout="vertical" requiredMark={false} disabled={busy} onFinish={submit}>
      <Form.Item name="portfolioId" label="策略组合" rules={[{ required: true, message: '请选择组合' }]}><Select showSearch optionFilterProp="label" placeholder="选择就绪组合" loading={portfolios.isPending} notFoundContent={portfolios.isPending ? '加载中' : '暂无就绪组合'} options={ready.map(item => ({ value: item.id, label: item.name }))} /></Form.Item>
      <Form.Item name="connectionId" label="连接" rules={[{ required: true, message: '请选择连接' }]}><Select placeholder="选择连接" loading={settings.isPending} notFoundContent={settings.isPending ? '加载中' : '暂无可用连接'} options={connections.map(item => ({ value: item.id, label: item.name }))} /></Form.Item>
      <ErrorNotice error={error} /><div className="form-actions"><Button disabled={busy} onClick={close}>关闭</Button><Button type="primary" htmlType="submit" loading={busy} disabled={portfolios.isError || settings.isError || !ready.length || !connections.length}>开始观测</Button></div>
    </Form>
  </Modal>;
}
