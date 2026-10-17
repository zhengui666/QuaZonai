import { useEffect, useRef, useState } from 'react';
import { Button, Dropdown, Form, Input, Select, Table, Tabs } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeftIcon, ArrowRightIcon, ArrowUpRightIcon, CaretDownIcon, CaretRightIcon, ChartLineUpIcon, CheckIcon, DatabaseIcon, DotsThreeIcon, FlaskIcon, MagnifyingGlassIcon, PlugsIcon, PlusIcon, SlidersHorizontalIcon } from '@phosphor-icons/react';
import type { DatasetView, ModelCatalog, ModelRef, ResearchDetail, ResearchReceipt, SettingsView, TaskView, TrialView } from '../../../packages/contracts/src/index.ts';
import { api } from './api';
import { draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource, useStudies } from './data-hooks';
import { currentRoute, href, navigate, type Route } from './router';
import { Copy, DraftNotice, Empty, ErrorNotice, EvidenceMetrics, Facts, Loading, Metrics, PendingCommand, Status, Timestamp, useConfirm } from './ui';

const modelKey = (model: ModelRef) => JSON.stringify({ provider: model.provider, modelId: model.modelId });
export default function Research({ route }: { route: Route }) { return route.creating ? <NewStudy route={route} /> : <ResearchWorkspace route={route} />; }
function ResearchWorkspace({ route }: { route: Route }) {
  const studies = useStudies(); const datasets = useResource<DatasetView[]>('/datasets');
  const query = route.params.get('q') ?? ''; const filter = route.params.get('state') ?? '全部状态';
  const listPath = () => { const params = new URLSearchParams(); if (query) params.set('q', query); if (filter !== '全部状态') params.set('state', filter); return `/research${params.size ? `?${params}` : ''}`; };
  const detailPath = (id: number) => listPath().replace('/research', `/research/${id}`);
  const updateFilter = (q: string, state: string) => { const params = new URLSearchParams(route.params); if (q) params.set('q', q); else params.delete('q'); if (state !== '全部状态') params.set('state', state); else params.delete('state'); navigate(`/research${route.id ? `/${encodeURIComponent(route.id)}` : ''}${params.size ? `?${params}` : ''}`, true); };
  const stateMatches = (status: string) => filter === '全部状态' || (filter === '进行中' ? ['RUNNING', 'STARTED', 'QUEUED'].includes(status) : filter === '已计算' ? status === 'COMPUTED' : filter === '失败' ? status === 'FAILED' : filter === '已停止' ? status === 'STOPPED' : status === 'IDLE');
  const visible = studies.items.filter(item => stateMatches(item.status) && `${item.title} ${item.model}`.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
  return <div className={`research-layout ${route.id ? 'has-selection' : ''}`}>
    <section className="collection" aria-label="研究列表"><header className="collection-heading"><div>{route.id ? <h2>研究</h2> : <h1>研究</h1>}<span>{studies.items.length}</span></div><Button type="primary" className="new-study-button" icon={<PlusIcon size={15} />} onClick={() => navigate('/research/new')}>新建研究</Button></header>
      <div className="collection-tools"><label className="sr-only" htmlFor="research-search">搜索已加载研究</label><Input id="research-search" prefix={<MagnifyingGlassIcon size={16} />} value={query} allowClear placeholder="搜索研究" onChange={event => updateFilter(event.target.value, filter)} /><Select aria-label="研究状态" value={filter} onChange={value => updateFilter(query, value)} suffixIcon={<CaretDownIcon size={12} />} options={['全部状态', '进行中', '已计算', '失败', '已停止', '待处理'].map(value => ({ value }))} /></div>
      <div className="collection-scope"><span>已加载 {studies.items.length} 条{query || filter !== '全部状态' ? ` / 匹配 ${visible.length} 条` : ''}</span>{visible.length > 0 && (query || filter !== '全部状态') && <button className="text-link" onClick={() => updateFilter('', '全部状态')}>清除筛选</button>}</div>
      {studies.isPending ? <Loading /> : <><ErrorNotice error={studies.error} retry={() => { void studies.refetch(); }} /><div className="study-list">{visible.map(item => <a className={`study-row ${String(item.id) === route.id ? 'active' : ''}`} key={item.id} data-study-id={item.id} href={href(detailPath(item.id))} aria-current={String(item.id) === route.id ? 'true' : undefined}><div className="study-row-top"><code>#{item.id}</code><Status plain value={item.status} /></div><h3>{item.title}</h3><div className="study-meta"><span>{item.trials} 次试验</span><span className="model-name">{item.model}</span></div></a>)}</div>
      {!visible.length && !studies.error && <Empty title={query || filter !== '全部状态' ? '没有匹配的研究' : '尚无研究'} action={query || filter !== '全部状态' ? <Button type="text" onClick={() => updateFilter('', '全部状态')}>清除筛选</Button> : undefined} />}
      {studies.hasNextPage && <Button className="load-more" block loading={studies.isFetchingNextPage} onClick={() => { void studies.fetchNextPage(); }}>加载更多</Button>}</>}
      {datasets.data && <div className="collection-bottom"><DatabaseIcon size={15} /><span>{datasets.data.length} 个已登记数据集</span><a aria-label="查看数据" href={href('/data')}><ArrowUpRightIcon size={15} /></a></div>}
    </section>
    {route.id ? <StudyDocument route={route} back={listPath()} /> : <div className="study-document"><Empty title={studies.items.length ? '选择一项研究' : '从一个研究问题开始'} /></div>}
  </div>;
}
function StudyDocument({ route, back }: { route: Route; back: string }) {
  const cache = useQueryClient(); const query = useResource<ResearchDetail>(`/research/${encodeURIComponent(route.id!)}`, { poll: 2000 });
  const [stopError, setStopError] = useState<unknown>(); const [stopping, setStopping] = useState(false);
  const confirm = useConfirm(); const detail = query.data;
  const tab = route.params.get('tab') ?? 'evidence'; const openTrialId = Number(route.params.get('trial')) || undefined;
  const selectTab = (value: string, trialId?: number) => { const params = new URLSearchParams(route.params); params.set('tab', value); if (trialId !== undefined) params.set('trial', String(trialId)); else params.delete('trial'); navigate(`/research/${route.id}?${params}`, true); };
  if (!detail) return <article className="study-document"><header className="study-heading"><h1>研究</h1></header>{query.isPending ? <Loading /> : <ErrorNotice error={query.error ?? new Error('未找到研究')} retry={() => { void query.refetch(); }} />}<a className="text-link" href={href(back)}>返回研究列表</a></article>;
  const budget = detail.input.policy.max_trials; const active = detail.tasks.some(task => task.status !== 'terminal') || ['queued', 'placed'].includes(detail.submission?.status ?? '');
  const stop = async () => { setStopping(true); setStopError(undefined); try { const value = await api<ResearchDetail>(`/research/${detail.id}/stop`, { method: 'POST' }); cache.setQueryData([`/research/${detail.id}`], value); await cache.invalidateQueries({ queryKey: ['/research'] }); } catch (error) { setStopError(error); } finally { setStopping(false); } };
  const stoppedLabel = detail.status === 'STOPPED' && active ? '停止中' : detail.status === 'STARTED' ? '独立评估中' : undefined;
  const records = [...detail.trialRecords].reverse();
  return <article className="study-document"><header className="study-heading"><div className="document-toolbar"><div className="document-identity"><a className="mobile-back text-link" href={href(back)}><ArrowLeftIcon size={14} />研究列表</a><span className="desktop-breadcrumb">研究 <CaretRightIcon size={11} /><code>{detail.id}</code></span><Status value={detail.status} label={stoppedLabel} /></div><Dropdown trigger={['click']} menu={{ items: [{ key: 'inputs', label: '查看原始身份', onClick: () => selectTab('inputs') }, ...(active && (!detail.final || detail.final.status === 'STARTED') ? [{ key: 'stop', label: stopping ? '停止中' : '停止研究', danger: true, disabled: stopping, onClick: () => confirm('停止研究？', '停止此研究的原始执行。', () => { void stop(); }) }] : [])] }}><Button className="icon-button" type="text" aria-label="研究操作" icon={<DotsThreeIcon size={21} />} /></Dropdown></div>
      <GoalTitle value={detail.title} />
      <div className="study-context"><span><DatabaseIcon size={14} />{detail.dataset?.name ?? detail.datasetId ?? '数据集名称未记录'}</span><span><PlugsIcon size={14} />{detail.model}</span><span><FlaskIcon size={14} /><code>{detail.trials}{typeof budget === 'number' ? ` / ${budget}` : ''}</code> 次试验</span></div></header>
      <ErrorNotice error={stopError} /><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
      <Tabs className="document-tabs" activeKey={tab} onChange={value => selectTab(value)} items={[
        { key: 'evidence', label: '证据', children: <div className="evidence-content">
          {!detail.final ? <Empty title={active ? '尚无独立结果' : '未产生独立结果'} action={<Button onClick={() => selectTab(active ? 'trials' : 'inputs')}>{active ? '查看试验' : '查看执行'}</Button>} /> : <section className="independent-result"><div className="section-heading"><h3>独立评估</h3>{detail.final.trialId !== null && <button className="selection-link" onClick={() => selectTab('trials', detail.final!.trialId!)}>所选试验 <code>#{detail.final.trialId}</code><ArrowUpRightIcon size={14} /></button>}</div>
            {detail.final.status === 'COMPUTED' ? <EvidenceMetrics metrics={detail.final.metrics} /> : <Status value={detail.final.status} label={detail.final.status === 'STARTED' ? '独立评估中' : detail.final.status === 'FAILED' ? '独立评估失败' : undefined} />}
            {detail.final.error && <div className="recorded-error"><span>独立评估失败</span><details><summary>技术详情</summary><code>{detail.final.error}</code></details></div>}
            {detail.final.reason && detail.final.reason !== detail.final.error && <details className="reason-disclosure"><summary>研究结论</summary><p>{detail.final.reason}</p></details>}
          </section>}
          <section className="trial-preview"><div className="section-heading"><h3>试验记录 <span className="heading-count">{records.length}</span></h3>{records.length > 0 && <button className="text-link" onClick={() => selectTab('trials')}>全部试验 <ArrowUpRightIcon size={14} /></button>}</div>{!records.length ? <p className="quiet">尚无试验</p> : <div className="preview-rows">{records.slice(0, 3).map(row => <button key={row.id} className="preview-row" onClick={() => selectTab('trials', row.id)}><code>{row.id}</code><span className="preview-hypothesis">{row.hypothesis || '未记录假设'}</span>{row.id === detail.final?.trialId && <span className="selected-mark">所选</span>}<Status plain value={row.status} /><CaretRightIcon size={14} /></button>)}</div>}</section>
          {detail.final?.status === 'COMPUTED' && <footer className="consumer-footer"><div className="consumer-label"><ChartLineUpIcon size={19} /><h3>组合回测</h3><span>可选</span></div><Button onClick={() => navigate(`/portfolios/new?research=${detail.id}`)}>创建组合 <ArrowRightIcon size={14} /></Button></footer>}
        </div> },
        { key: 'trials', label: <span>试验 <span className="tab-count">{records.length}</span></span>, children: <TrialLedger records={records} selected={detail.final?.trialId ?? null} openId={openTrialId} onOpen={id => selectTab('trials', id)} /> },
        { key: 'inputs', label: '输入与执行', children: <div className="input-content"><section><h3>研究输入</h3><Facts items={[["数据集", detail.dataset?.name ?? detail.datasetId ?? '未记录'], ["来源", detail.dataset?.source ?? '未记录'], ["模型", detail.model], ["推理强度", detail.input.thinkingLevel ?? '未记录'], ["试验预算", typeof budget === 'number' ? `${budget} 次` : '未记录'], ["覆盖区间", detail.dataset ? <><Timestamp value={detail.dataset.start} /><span> 至 </span><Timestamp value={detail.dataset.end} /></> : '未记录'], ["创建时间", <Timestamp value={detail.createdAt} />]]} /></section><section><h3>原始身份</h3><Facts items={[["研究", <><code>{detail.id}</code><Copy value={String(detail.id)} label="复制研究身份" /></>], ["请求", detail.submission?.requestId ? <><code>{detail.submission.requestId}</code><Copy value={detail.submission.requestId} label="复制请求身份" /></> : '未记录'], ["提交", detail.submission ? <code>{detail.submission.id}</code> : '未记录'], ["提交状态", detail.submission ? <Status value={detail.submission.status} /> : '未记录'], ["终态原因", detail.submission?.reason ?? '未提供']]} /></section><section><h3>最近执行记录</h3><Table<TaskView> rowKey="id" size="small" dataSource={detail.tasks} pagination={{ pageSize: 10, hideOnSinglePage: true }} scroll={{ x: 500 }} columns={[{ title: '任务', render: (_value, task) => <div className="trial-name">{task.name}<code>#{task.id}</code></div> }, { title: '状态', render: (_value, task) => <Status value={task.outcome ?? task.status} /> }, { title: '错误', render: (_value, task) => task.error ? <code>{task.error}</code> : <span className="quiet">未提供</span> }]} /></section></div> },
      ]} />
  </article>;
}
function GoalTitle({ value }: { value: string }) {
  const heading = useRef<HTMLHeadingElement>(null); const [expanded, setExpanded] = useState(false); const [clipped, setClipped] = useState(false);
  useEffect(() => {
    const node = heading.current; if (!node) return;
    const measure = () => setClipped(node.scrollHeight > Number.parseFloat(getComputedStyle(node).lineHeight) * 2 + 1);
    const observer = new ResizeObserver(measure); observer.observe(node); void document.fonts.ready.then(measure);
    return () => observer.disconnect();
  }, [value]);
  return <><h1 ref={heading} className={expanded ? '' : 'clamp-goal'}>{value}</h1>{clipped && <Button type="text" size="small" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{expanded ? '收起' : '展开'}</Button>}</>;
}
function TrialLedger({ records, selected, openId, onOpen }: { records: TrialView[]; selected: number | null; openId?: number; onOpen: (id?: number) => void }) {
  const [expanded, setExpanded] = useState<number | null>(openId ?? null); const [page, setPage] = useState(1);
  const ready = records.length > 0;
  useEffect(() => { if (openId !== undefined) { setExpanded(openId); const index = records.findIndex(item => item.id === openId); if (index >= 0) setPage(Math.floor(index / 10) + 1); } }, [openId, ready]);
  if (!records.length) return <Empty title="尚无试验" />;
  return <section className="trial-section"><div className="section-heading"><h3>试验记录</h3><span className="quiet">{records.length} 次尝试</span></div>{openId !== undefined && !records.some(item => item.id === openId) && <p role="status">未找到所选试验</p>}<Table<TrialView> rowKey="id" size="small" dataSource={records} scroll={{ x: 590 }} pagination={{ current: page, pageSize: 10, hideOnSinglePage: true, onChange: setPage }} expandable={{ columnWidth: 44, expandedRowKeys: expanded === null ? [] : [expanded], onExpand: (open, row) => { setExpanded(open ? row.id : null); onOpen(open ? row.id : undefined); }, expandIcon: ({ expanded: open, onExpand, record }) => <Button type="text" className="trial-expander" aria-label={`${open ? '收起' : '展开'}试验 ${record.id}`} aria-expanded={open} icon={<CaretDownIcon className={open ? 'rotate' : ''} size={16} />} onClick={event => onExpand(record, event)} />, expandedRowRender: row => <div className="trial-detail"><div className="detail-overline"><code>#{row.id}</code><span>原始试验</span></div><h4>{row.hypothesis || '未记录假设'}</h4>{row.error && <div className="recorded-error"><code>{row.error}</code></div>}<Metrics metrics={row.metrics} /></div> }} columns={[
    { title: '研究假设', width: 260, render: (_value, row) => <div className="trial-name">{row.hypothesis || '未记录假设'}<code>#{row.id}</code></div> },
    { title: '验证指标', align: 'right', width: 110, render: (_value, row) => { const metric = row.metrics.find(item => (item.partition === 'VALIDATION' || item.key.startsWith('VALIDATION.')) && /(?:pearson_ic|^ic)$/.test(item.key)); return <span className="table-metric">{metric ? metric.value ?? '不可用' : '未提供'}</span>; } },
    { title: '状态', width: 100, render: (_value, row) => <Status plain value={row.status} /> },
    { title: '选择', width: 80, render: (_value, row) => row.id === selected ? <span className="selected-trial"><CheckIcon size={13} />所选</span> : <span className="quiet">未选择</span> }, Table.EXPAND_COLUMN,
  ]} /></section>;
}
function NewStudy({ route }: { route: Route }) {
  const cache = useQueryClient(); const catalog = useResource<ModelCatalog>('/models'); const datasets = useResource<DatasetView[]>('/datasets'); const settings = useResource<SettingsView>('/settings');
  const [form] = Form.useForm(); const [draft] = useState(() => readDraft<Record<string, unknown>>('research')); const [draftSaved, setDraftSaved] = useState(() => draftIsPersisted('research')); const initialized = useRef(false);
  const command = useCommand<ResearchReceipt>('research', result => { removeDraft('research'); void cache.invalidateQueries({ queryKey: ['/research'] }); if (currentRoute().page === 'research' && currentRoute().creating) navigate(`/research/${result.conversationId}`); });
  const origin = route.params.get('from') === 'data' ? { path: '/data', label: '数据' } : { path: '/research', label: '研究' };
  const [thinkingChanged, setThinkingChanged] = useState(false);
  const thinkingLevel = Form.useWatch('thinkingLevel', form) as string | undefined;
  const selectedModelKey = Form.useWatch('model', form) as string | undefined;
  const selectedModel = catalog.data?.models.find(model => modelKey(model) === selectedModelKey);
  const modelOptions = catalog.data?.models.map(model => ({ value: modelKey(model), label: `${model.name} / ${model.provider}` })) ?? [];
  if (selectedModelKey && !modelOptions.some(option => option.value === selectedModelKey)) { const recorded = JSON.parse(selectedModelKey) as ModelRef; modelOptions.push({ value: selectedModelKey, label: `${recorded.provider}/${recorded.modelId} / 当前不可用` }); }
  const selectedDatasetId = Form.useWatch('datasetId', form) as string | undefined;
  const selectedDataset = datasets.data?.find(dataset => dataset.id === selectedDatasetId);
  useEffect(() => {
    if (initialized.current || !catalog.data || !datasets.data || !settings.data) return;
    initialized.current = true;
    if (command.values) { form.setFieldsValue({ ...command.values, model: modelKey(command.values.model as ModelRef), maxTrials: String(command.values.maxTrials) }); return; }
    const preferred = settings.data.defaultModel && catalog.data.models.find(model => modelKey(model) === modelKey(settings.data!.defaultModel!));
    form.setFieldsValue({ ...(preferred ? { model: modelKey(preferred), thinkingLevel: settings.data.thinkingLevel ?? preferred.effectiveThinkingLevel } : {}), ...draft, ...(route.params.get('dataset') ? { datasetId: route.params.get('dataset') } : {}) });
  }, [catalog.data, datasets.data, settings.data]);
  useEffect(() => {
    if (!command.pending && selectedModel && thinkingLevel && !selectedModel.thinkingLevels.some(level => level === thinkingLevel)) { form.setFieldValue('thinkingLevel', undefined); setThinkingChanged(true); setDraftSaved(saveDraft('research', form.getFieldsValue())); }
  }, [selectedModel, thinkingLevel, command.pending]);
  return <div className="authoring-page"><a className="text-link back-link" href={href(origin.path)}><ArrowLeftIcon size={14} />{origin.label}</a><div className="authoring-heading"><div className="authoring-symbol"><FlaskIcon weight="duotone" size={24} /></div><h1>新建研究</h1></div>
    <ErrorNotice error={catalog.error ?? datasets.error ?? settings.error} retry={() => { void catalog.refetch(); void datasets.refetch(); void settings.refetch(); }} />
    {catalog.isPending || datasets.isPending || settings.isPending ? <Loading /> : !catalog.data || !datasets.data || !settings.data ? null : <>
    {catalog.data && !catalog.data.models.length && <div className="prerequisite">没有可用模型 <a href={href('/settings')}>连接模型 <ArrowUpRightIcon size={13} /></a></div>}
    {datasets.data && !datasets.data.some(item => item.ready) && <div className="prerequisite">没有就绪的数据集 <a href={href('/data')}>查看数据 <ArrowUpRightIcon size={13} /></a></div>}
    <PendingCommand command={command} />
    <Form form={form} layout="vertical" validateTrigger="onBlur" scrollToFirstError={{ focus: true }} onValuesChange={(_changed, values: Record<string, unknown>) => { if (!command.pending) setDraftSaved(saveDraft('research', values)); }} onFinish={(values: { goal: string; model: string; datasetId: string; thinkingLevel?: string; maxTrials: string }) => { void command.submit({ goal: values.goal, model: JSON.parse(values.model) as ModelRef, datasetId: values.datasetId, ...(values.thinkingLevel ? { thinkingLevel: values.thinkingLevel } : {}), maxTrials: Number(values.maxTrials) }); }} disabled={!!command.pending || command.busy}>
      <Form.Item label="研究问题" name="goal" rules={[{ required: true, whitespace: true, message: '请输入研究问题' }]}><Input.TextArea autoFocus autoSize={{ minRows: 4, maxRows: 10 }} placeholder="例如：成交量变化能否解释短期收益？" /></Form.Item>
      <Form.Item label="数据集" name="datasetId" rules={[{ required: true, message: '请选择数据集' }, { validator: (_rule, value: string) => !value || datasets.data?.some(item => item.id === value && item.ready) ? Promise.resolve() : Promise.reject(new Error('数据集尚未就绪')) }]}><Select showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} placeholder="选择已登记数据集" options={datasets.data?.map(item => ({ value: item.id, label: `${item.name} / ${item.currency}`, disabled: !item.ready }))} /></Form.Item>
      {selectedDataset && <div className="dataset-context"><span><DatabaseIcon size={13} />{selectedDataset.source}</span><code>{selectedDataset.id}</code><code>{selectedDataset.currency}</code>{(selectedDataset.start || selectedDataset.end) && <span><Timestamp value={selectedDataset.start} /> 至 <Timestamp value={selectedDataset.end} /></span>}<span>{selectedDataset.ready ? '已就绪' : '未就绪'}</span></div>}
      <div className="form-section-heading"><SlidersHorizontalIcon size={16} /><h2>运行设置</h2></div>
      <Form.Item label="模型" name="model" rules={[{ required: true, message: '请选择模型' }, { validator: (_rule, value: string) => !value || catalog.data?.models.some(model => modelKey(model) === value) ? Promise.resolve() : Promise.reject(new Error('所选模型不可用，请重新选择')) }]}><Select showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} placeholder="选择可用模型" options={modelOptions} onChange={value => { const model = catalog.data?.models.find(item => modelKey(item) === value); setThinkingChanged(false); form.setFieldValue('thinkingLevel', model?.effectiveThinkingLevel); setDraftSaved(saveDraft('research', form.getFieldsValue())); }} /></Form.Item>
      {selectedModel && <p className="model-context"><code>{selectedModel.provider}/{selectedModel.modelId}</code></p>}
      <div className="form-columns"><Form.Item label="推理强度" name="thinkingLevel" extra={thinkingChanged ? '原推理强度不可用，请重新选择' : undefined} rules={[{ required: !!selectedModel, message: '请选择推理强度' }]}><Select onChange={() => setThinkingChanged(false)} suffixIcon={<CaretDownIcon size={14} />} disabled={!selectedModel || !!command.pending} options={selectedModel?.thinkingLevels.map(value => ({ value, label: ({ off: '关闭', minimal: '最低', low: '低', medium: '中', high: '高', xhigh: '更高', max: '最高' } as Record<string, string>)[value] }))} /></Form.Item><Form.Item label="试验预算" name="maxTrials" extra="失败试验计入预算" rules={[{ required: true, message: '请输入试验预算' }, { validator: (_rule, value: string) => !value || /^\d+$/.test(String(value)) && Number.isSafeInteger(Number(value)) ? Promise.resolve() : Promise.reject(new Error('请输入非负安全整数')) }]}><Input inputMode="numeric" suffix="次" placeholder="输入次数" /></Form.Item></div>
      <ErrorNotice error={command.error} /><DraftNotice saved={draftSaved || !!command.pending} /><div className="form-actions"><Button disabled={false} onClick={() => navigate(origin.path)}>取消</Button>{command.pending ? <Button type="primary" disabled={command.busy} aria-busy={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>{command.busy ? '提交中' : '核对提交'}</Button> : <Button type="primary" htmlType="submit" loading={command.busy} disabled={!catalog.data?.models.length || !datasets.data?.some(item => item.ready)}>开始研究 <ArrowRightIcon size={15} /></Button>}</div>
    </Form></>}
  </div>;
}
