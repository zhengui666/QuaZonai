import { App, Button, Form, Input, InputNumber, Modal, Select, Table } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { lazy, Suspense, useState } from 'react';
import type { DatasetView, PortfolioStart, PortfolioView, TargetWeight } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { display, ErrorNotice, MetricGrid, MoreResearch, NoData, number, PageHeading, QueryPanel, RefreshButton, Status, Surface, time, useResearch, useResource } from './ui';
const EquityPlot = lazy(() => import('./equity-plot'));

export function Portfolios() {
  const listing = useResource<PortfolioView[]>('/portfolios', 8000); const [selected, setSelected] = useState<string>(); const [creating, setCreating] = useState(false);
  const id = selected ?? listing.data?.[0]?.id;
  return <div className="page"><PageHeading title="策略组合" count={listing.data?.length} action={<><RefreshButton loading={listing.isFetching} label="刷新组合" onClick={() => { void listing.refetch(); }} /><Button type="primary" icon={<PlusOutlined aria-hidden />} onClick={() => setCreating(true)}>新建组合</Button></>} />
    <QueryPanel query={listing}>{listing.data?.length ? <div className="portfolio-layout"><section className="surface selection-list" aria-label="组合列表">{listing.data.map(item => <button key={item.id} aria-pressed={id === item.id} onClick={() => setSelected(item.id)}>
      <strong>{display(item.name)}</strong><span className="selection-meta"><Status value={item.status} /><span>{item.researchIds.length} 项研究</span></span>
    </button>)}</section>{id && <PortfolioRecord key={id} id={id} />}</div> : <Surface><NoData text="暂无策略组合" /></Surface>}</QueryPanel>
    {creating && <PortfolioForm close={() => setCreating(false)} started={value => { setSelected(value.id); setCreating(false); }} />}
  </div>;
}
function PortfolioRecord({ id }: { id: string }) {
  const detail = useResource<PortfolioView>(`/portfolios/${encodeURIComponent(id)}`, 5000); const datasets = useResource<DatasetView[]>('/datasets'); const value = detail.data;
  return <QueryPanel query={detail}>{value && <div className="detail-stack">
    <Surface><div className="detail-heading"><h2>{display(value.name)}</h2><Status value={value.status} /></div><div className="record-meta" style={{ marginTop: 16 }}>
      <span>资本 {number(value.capital)} {display(value.currency)}</span><span>{datasets.data?.find(item => item.id === value.datasetId)?.name ?? '—'}</span><span>{value.researchIds.map(id => `#${id}`).join(' · ') || '—'}</span><span>{time(value.createdAt)}</span>
    </div><div style={{ marginTop: 24 }}><MetricGrid metrics={value.metrics} /></div><ErrorNotice error={value.error} /></Surface>
    <Surface><Suspense fallback={<div role="status">加载曲线</div>}><EquityPlot points={value.curve} currency={value.currency} title="共享账户回测" /></Suspense></Surface>
    <Surface title="目标权重" extra={<RefreshButton loading={detail.isFetching} label="刷新组合详情" onClick={() => { void detail.refetch(); }} />}><Table<TargetWeight> rowKey="instrumentId" dataSource={value.targets} pagination={false} size="small" locale={{ emptyText: '暂无目标权重' }} columns={[
      { title: '标的', dataIndex: 'instrumentId', render: display }, { title: '权重', dataIndex: 'weight', align: 'right', render: display },
    ]} /></Surface>
  </div>}</QueryPanel>;
}
type Fields = { name: string; researchIds: number[]; datasetId: string; capital: string; weights?: Record<string, string> };
function PortfolioForm({ close, started }: { close: () => void; started: (value: PortfolioView) => void }) {
  const research = useResearch(); const datasets = useResource<DatasetView[]>('/datasets'); const client = useQueryClient(); const { message } = App.useApp();
  const [form] = Form.useForm<Fields>(); const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>(); const [pending, setPending] = useState<{ fields: string; body: PortfolioStart }>();
  const selectedIds = Form.useWatch('researchIds', form) as number[] | undefined;
  const readyDatasets = datasets.data?.filter(item => item.ready) ?? [];
  async function submit(values: Fields) {
    setBusy(true); setError(undefined);
    const { weights, ...selection } = values;
    const fields = JSON.stringify(values); const body = pending?.fields === fields ? pending.body : { requestId: crypto.randomUUID(), ...selection, name: values.name.trim(), weights: values.researchIds.map(researchId => ({ researchId, weight: weights?.[String(researchId)] ?? (values.researchIds.length === 1 ? '1' : '') })) };
    setPending({ fields, body });
    try { const value = await api<PortfolioView>('/portfolios', { method: 'POST', body }); client.setQueryData([`/portfolios/${encodeURIComponent(value.id)}`], value); client.setQueryData<PortfolioView[]>(['/portfolios'], previous => [...(previous?.filter(item => item.id !== value.id) ?? []), value]); await client.invalidateQueries({ queryKey: ['/portfolios'] }); void message.success('已接纳'); started(value); }
    catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  return <Modal title="新建组合" open onCancel={close} footer={null} width={600} closable={!busy} maskClosable={!busy} keyboard={!busy} afterOpenChange={open => { if (open) form.getFieldInstance('name')?.focus(); }}>
    <ErrorNotice error={research.error} retry={() => { void research.refetch(); }} /><ErrorNotice error={datasets.error} retry={() => { void datasets.refetch(); }} />
    <Form form={form} layout="vertical" requiredMark={false} disabled={busy} onFinish={submit}>
      <Form.Item name="name" label="组合名称" rules={[{ required: true, whitespace: true, message: '请输入名称' }]}><Input autoFocus /></Form.Item>
      <Form.Item name="researchIds" label="研究" rules={[{ required: true, type: 'array', min: 1, message: '请选择研究' }]}><Select mode="multiple" showSearch optionFilterProp="label" placeholder="选择研究" loading={research.isPending} notFoundContent={research.isPending ? '加载中' : '暂无研究'} options={research.data?.items.map(item => ({ value: item.id, label: `#${item.id} · ${item.title}` }))} /></Form.Item>
      <MoreResearch query={research} />
      <div className="form-grid">{selectedIds?.map(id => <Form.Item key={id} name={['weights', String(id)]} label={`#${id} 权重`} initialValue={selectedIds.length === 1 ? '1' : undefined} rules={[{ required: true, message: '请输入权重' }]}><InputNumber stringMode min="0.000000000000000001" max="1" /></Form.Item>)}</div>
      <div className="form-grid"><Form.Item name="datasetId" label="数据集" rules={[{ required: true, message: '请选择数据集' }]}><Select showSearch optionFilterProp="label" placeholder="选择数据集" loading={datasets.isPending} notFoundContent={datasets.isPending ? '加载中' : '暂无就绪数据集'} options={readyDatasets.map(item => ({ value: item.id, label: `${item.name} · ${item.currency}` }))} /></Form.Item>
        <Form.Item name="capital" label="初始资本" rules={[{ required: true, message: '请输入资本' }, { validator: (_, value) => !value || Number(value) > 0 ? Promise.resolve() : Promise.reject(new Error('资本须大于 0')) }]}><InputNumber stringMode min="0.01" /></Form.Item></div>
      <ErrorNotice error={error} /><div className="form-actions"><Button disabled={busy} onClick={close}>关闭</Button><Button type="primary" htmlType="submit" loading={busy} disabled={research.isError || datasets.isError || !research.data?.items.length || !readyDatasets.length}>开始回测</Button></div>
    </Form>
  </Modal>;
}
