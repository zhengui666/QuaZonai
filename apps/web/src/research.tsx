import { App, Button, Drawer, Form, Input, InputNumber, Modal, Select, Table } from 'antd';
import { ArrowRightOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import type { DatasetView, ModelView, ResearchDetail, ResearchReceipt, SettingsView, TaskView, UsageView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { captureResearchIntent, clearResearchIntent, readResearchIntent, type ResearchIntent } from './request-intent';
import { display, ErrorNotice, isActive, MetricGrid, modelKey, MoreResearch, NoData, number, PageHeading, QueryPanel, RefreshButton, Status, Surface, time, useResearch, useResource } from './ui';

export function Research() {
  const listing = useResearch(8000);
  const usage = useResource<UsageView>('/usage', 8000);
  const [creating, setCreating] = useState(false); const [selected, setSelected] = useState<number>();
  const [search, setSearch] = useState(''); const [filter, setFilter] = useState('all');
  const items = listing.data?.items.filter(item => item.title.toLowerCase().includes(search.toLowerCase()) && (filter === 'all' || (filter === 'active' ? isActive(item.status) : !isActive(item.status)))) ?? [];
  return <div className="page">
    <PageHeading title="研究看板" count={listing.data?.items.length} action={<><RefreshButton loading={listing.isFetching || usage.isFetching} label="刷新研究" onClick={() => { void listing.refetch(); void usage.refetch(); }} /><Button type="primary" icon={<PlusOutlined aria-hidden />} onClick={() => setCreating(true)}>新建研究</Button></>} />
    <QueryPanel query={usage}><dl className="usage-grid surface" aria-label="用量">
      <div><dt>运行任务</dt><dd>{number(usage.data?.activeTasks)}</dd></div>
      <div><dt>输入 Token</dt><dd>{number(usage.data?.inputTokens)}</dd></div>
      <div><dt>输出 Token</dt><dd>{number(usage.data?.outputTokens)}</dd></div>
      <div><dt>费用</dt><dd>{number(usage.data?.cost)}</dd></div>
    </dl></QueryPanel>
    <div className="toolbar"><Input prefix={<SearchOutlined aria-hidden />} aria-label="搜索研究" placeholder="搜索研究" allowClear value={search} onChange={event => setSearch(event.target.value)} />
      <Select aria-label="研究状态" value={filter} onChange={setFilter} options={[{ value: 'all', label: '全部状态' }, { value: 'active', label: '进行中' }, { value: 'finished', label: '已结束' }]} />
      <span className="muted result-count">{listing.data ? `${items.length} 项` : '—'}</span>
    </div>
    <QueryPanel query={listing}>{items.length ? <div className="study-grid">{items.map(item => <article className="study-card" key={item.id}>
      <div className="card-top"><span className="study-number">#{item.id.toString().padStart(3, '0')}</span><Status value={item.status} /></div>
      <h2><button className="title-button" onClick={() => setSelected(item.id)}>{display(item.title)}</button></h2>
      <div className="study-meta"><span>{display(item.model)}</span><span>{number(item.trials)} 次试验</span></div>
      <div className="card-bottom"><time dateTime={item.updatedAt ?? undefined}>{time(item.updatedAt)}</time><Button type="text" aria-label={`查看 ${item.title}`} icon={<ArrowRightOutlined aria-hidden />} onClick={() => setSelected(item.id)}>查看</Button></div>
    </article>)}</div> : <Surface><NoData text={search || filter !== 'all' ? '无匹配研究' : '暂无研究'} /></Surface>}</QueryPanel>
    <MoreResearch query={listing} />
    {creating && <ResearchForm close={() => setCreating(false)} onStarted={id => { setCreating(false); setSelected(id); }} />}
    {selected !== undefined && <ResearchRecord id={selected} close={() => setSelected(undefined)} />}
  </div>;
}
type Fields = { goal: string; model: string; datasetId: string; maxTrials: number };
function ResearchForm({ close, onStarted }: { close: () => void; onStarted: (id: number) => void }) {
  const models = useResource<ModelView[]>('/research/models'); const datasets = useResource<DatasetView[]>('/datasets'); const settings = useResource<SettingsView>('/settings');
  const client = useQueryClient(); const { message } = App.useApp();
  const [form] = Form.useForm<Fields>(); const [intent, setIntent] = useState<ResearchIntent | undefined>(() => readResearchIntent());
  const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  useEffect(() => {
    if (intent || !settings.data) return;
    if (!form.isFieldTouched('model')) form.setFieldValue('model', modelKey(settings.data.defaultModel));
    if (!form.isFieldTouched('maxTrials')) form.setFieldValue('maxTrials', settings.data.maxTrials);
  }, [settings.data, intent, form]);
  const readyDatasets = datasets.data?.filter(item => item.ready) ?? [];
  const unavailable = models.isError || datasets.isError || !models.data?.length || !readyDatasets.length;
  async function submit(values: Fields) {
    setBusy(true); setError(undefined);
    try {
      let pending = intent;
      if (!pending) {
        const model = models.data?.find(item => modelKey(item) === values.model);
        if (!model) throw new Error('请选择模型');
        pending = captureResearchIntent({ goal: values.goal.trim(), model: { provider: model.provider, modelId: model.modelId }, datasetId: values.datasetId, maxTrials: values.maxTrials });
        setIntent(pending);
      }
      const receipt = await api<ResearchReceipt>('/research', { method: 'POST', rawBody: pending.raw });
      clearResearchIntent(); await client.invalidateQueries({ queryKey: ['/research'] });
      void client.invalidateQueries({ queryKey: ['/usage'] }); void message.success('已接纳'); onStarted(receipt.conversationId);
    } catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  const saved = intent?.value;
  return <Modal title="新建研究" open onCancel={close} footer={null} width={600} maskClosable={!busy} closable={!busy} keyboard={!busy} afterOpenChange={open => { if (open) form.getFieldInstance('goal')?.focus(); }}>
    <ErrorNotice error={models.error} retry={() => { void models.refetch(); }} /><ErrorNotice error={datasets.error} retry={() => { void datasets.refetch(); }} />
    {intent && <div className="intent-status"><span>待重试</span><Button size="small" disabled={busy} onClick={() => { clearResearchIntent(); setIntent(undefined); setError(undefined); form.resetFields(); }}>新请求</Button></div>}
    <Form form={form} layout="vertical" requiredMark={false} disabled={busy || !!intent} initialValues={saved ? { ...saved, model: modelKey(saved.model) } : { maxTrials: settings.data?.maxTrials, model: modelKey(settings.data?.defaultModel) }} onFinish={submit}>
      <Form.Item name="goal" label="研究目标" rules={[{ required: true, whitespace: true, message: '请输入研究目标' }]}><Input.TextArea rows={4} autoFocus /></Form.Item>
      <div className="form-grid"><Form.Item name="datasetId" label="数据集" rules={[{ required: true, message: '请选择数据集' }]}>
        <Select showSearch optionFilterProp="label" loading={datasets.isPending} placeholder="选择数据集" notFoundContent={datasets.isPending ? '加载中' : '暂无就绪数据集'} options={readyDatasets.map(item => ({ value: item.id, label: `${item.name} · ${item.currency}` }))} />
      </Form.Item><Form.Item name="model" label="模型" rules={[{ required: true, message: '请选择模型' }]}>
        <Select showSearch optionFilterProp="label" loading={models.isPending} placeholder="选择模型" notFoundContent={models.isPending ? '加载中' : '暂无模型'} options={models.data?.map(item => ({ value: modelKey(item), label: `${item.name} · ${item.provider}` }))} />
      </Form.Item><Form.Item name="maxTrials" label="最多试验" rules={[{ required: true, message: '请输入试验次数' }]}><InputNumber min={1} precision={0} placeholder="—" /></Form.Item></div>
      <ErrorNotice error={error} /><div className="form-actions"><Button disabled={busy} onClick={close}>关闭</Button>{intent
        ? <Button type="primary" loading={busy} disabled={false} onClick={() => { void submit(form.getFieldsValue()); }}>重试提交</Button>
        : <Button type="primary" htmlType="submit" loading={busy} disabled={unavailable}>开始研究</Button>}</div>
    </Form>
  </Modal>;
}
function ResearchRecord({ id, close }: { id: number; close: () => void }) {
  const detail = useResource<ResearchDetail>(`/research/${id}`, 5000); const client = useQueryClient();
  const [busy, setBusy] = useState(false); const [error, setError] = useState<unknown>();
  async function stop() {
    setBusy(true); setError(undefined);
    try { const value = await api<ResearchDetail>(`/research/${id}/stop`, { method: 'POST' }); client.setQueryData([`/research/${id}`], value); void client.invalidateQueries({ queryKey: ['/research'] }); }
    catch (failure) { setError(failure); }
    finally { setBusy(false); }
  }
  const value = detail.data;
  return <Drawer title={`研究 #${id}`} open onClose={close} width={760}>
    <QueryPanel query={detail}>{value && <div className="detail-stack">
      <div className="detail-heading"><h2>{display(value.title)}</h2><Status value={value.status} /></div>
      <div className="record-meta"><span>{display(value.model)}</span><span>{number(value.trials)} 次试验</span><span>更新 {time(value.updatedAt)}</span></div>
      <div className="toolbar"><RefreshButton loading={detail.isFetching} label="刷新研究详情" onClick={() => { void detail.refetch(); }} /><Button disabled={!isActive(value.status) || busy || detail.isError} loading={busy} onClick={stop}>停止研究</Button></div>
      <ErrorNotice error={error} />
      <Surface title="研究结果" extra={<Status value={value.final?.status} />}><div className="muted" style={{ marginBottom: 16 }}>选定试验 {value.final?.trialId == null ? '—' : `#${value.final.trialId}`}</div><MetricGrid metrics={value.final?.metrics ?? []} /></Surface>
      <section><div className="section-heading"><h2>试验</h2><span className="muted">{value.trialRecords.length} 项</span></div>
        {value.trialRecords.length ? <div className="trial-list">{value.trialRecords.map(trial => <article className="trial-row" key={trial.id}>
          <div className="trial-top"><span>#{trial.id}</span><h3>{display(trial.hypothesis)}</h3><Status value={trial.status} /></div><MetricGrid metrics={trial.metrics} /><ErrorNotice error={trial.error} />
        </article>)}</div> : <NoData text="暂无试验" />}
      </section><Surface title="任务"><Table<TaskView> rowKey="id" dataSource={value.tasks} pagination={false} size="small" locale={{ emptyText: '暂无任务' }} columns={[
        { title: '任务', dataIndex: 'name', render: display }, { title: '状态', dataIndex: 'status', width: 100, render: status => <Status value={status} /> },
        { title: '错误', dataIndex: 'error', render: error => error ? <ErrorNotice error={error} /> : '—' },
      ]} /></Surface>
    </div>}</QueryPanel>
  </Drawer>;
}
