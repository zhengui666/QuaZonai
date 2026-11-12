import { useEffect, useRef, useState } from 'react';
import { Button, Dropdown, Form, Input, Select, Table, Tabs } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeftIcon, ArrowRightIcon, ArrowUpRightIcon, CaretDownIcon, CaretRightIcon, DatabaseIcon, DotsThreeIcon, FlaskIcon, PlusIcon } from '@phosphor-icons/react';
import type { ResearchListItem } from '../../../packages/contracts/src/lists.ts';
import type { DatasetView, ModelCatalog, ModelRef, ResearchDetail, ResearchReceipt, ServiceTier, SettingsView, TaskView } from '../../../packages/contracts/src/index.ts';
import { ApiError, api } from './api';
import { datasetResearchSettings } from '../../../packages/contracts/src/research.ts';
import { NativeInputFacts } from './native-input';
import { draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResearchDetail, useResource } from './data-hooks';
import { ListTools, listPathFor, useListWorkspace } from './list-workspace';
import { currentRoute, href, navigate, type Route } from './router';
import { Copy, DraftNotice, Empty, ErrorNotice, Facts, Loading, PendingCommand, Status, Timestamp, useConfirm } from './ui';
import { RecordTitle, TextDisclosure } from './disclosure';
import { shortTitle } from './titles';
import { ExecutionFailure, Reason } from './diagnostics';
import { Literature } from './literature';
import { publicLiteratureTopics } from './literature-input';
import { DeleteRecord } from './delete-record';

/** Only transport user intent; do not reconstruct retired scoring/fit specifications. */
export function nativeResearchInput(values: Record<string, string>) {
  const publicQueries = publicLiteratureTopics(values.publicLiteratureTopics);
  return { goal: values.goal, ...(publicQueries ? { publicLiteratureQueries: publicQueries } : {}), model: JSON.parse(values.model!) as ModelRef, datasetId: values.datasetId, ...(values.thinkingLevel ? { thinkingLevel: values.thinkingLevel } : {}), ...(values.serviceTier ? { serviceTier: values.serviceTier } : {}) };
}

const thinkingLabels: Record<string, string> = { off: '关闭', minimal: '最低', low: '低', medium: '中', high: '高', xhigh: '更高', max: '最高' };
const modelKey = (model: ModelRef) => JSON.stringify({ provider: model.provider, modelId: model.modelId });
export default function Research({ route }: { route: Route }) {
  if (route.creating) return <NewStudy route={route} />;
  return route.id ? <StudyDocument route={route} back={listPathFor(route)} /> : <ResearchWorkspace route={route} />;
}
function ResearchWorkspace({ route }: { route: Route }) {
  const list = useListWorkspace<ResearchListItem>(route, '/research');
  return <section className="list-workspace" aria-label="研究列表">
    <header className="list-heading"><h1>研究</h1><Button type="primary" className="new-study-button" icon={<PlusIcon size={15} />} onClick={() => navigate(list.detailPath('new'))}>新建研究</Button></header>
    <ListTools list={list} searchLabel="搜索研究" placeholder="搜索研究名称、问题、依据或数据集" filters={[
      { key: 'status', label: '研究状态' }, { key: 'datasetId', label: '研究数据集' },
      { key: 'modality', label: '输入类型', advanced: true }, { key: 'model', label: '研究模型', advanced: true },
    ]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<ResearchListItem> {...list.tableProps} rowKey="id" size="small" scroll={{ x: 1120 }} locale={{ emptyText: <Empty title={list.hasFilters ? '没有匹配的研究' : '尚无研究'} /> }} columns={[
      { title: '研究名称', key: 'title', width: 240, ellipsis: { showTitle: false }, ...list.sortColumn('title'), render: (_value, row) => <a className="list-record-name" {...list.linkProps(row.id)}>{shortTitle(row.title, undefined, '未命名研究')}</a> },
      { title: '数据集', key: 'datasetName', width: 190, ellipsis: { showTitle: false }, ...list.sortColumn('datasetName'), render: (_value, row) => <span className="list-cell-text">{row.datasetName ? shortTitle(row.datasetName) : '未记录'}</span> },
      { title: '执行状态', key: 'status', width: 130, render: (_value, row) => <Status plain value={row.status} label={row.status === 'STARTED' ? '原生执行中' : undefined} /> },
    ]} />}
  </section>;
}
export function hasActiveResearchExecution(detail: Pick<ResearchDetail, 'tasks' | 'submission'>): boolean {
  return detail.tasks.some(task => task.status !== 'terminal') || ['queued', 'placed'].includes(detail.submission?.status ?? '');
}
function StudyDocument({ route, back }: { route: Route; back: string }) {
  const cache = useQueryClient(); const query = useResearchDetail(route.id!);
  const [stopError, setStopError] = useState<unknown>(); const [stopping, setStopping] = useState(false);
  const confirm = useConfirm(); const detail = query.data;
  const tab = route.params.get('tab') ?? 'evidence';
  const selectTab = (value: string) => { const params = new URLSearchParams(route.params); params.set('tab', value); navigate(`/research/${route.id}?${params}`, true); };
  if (!detail) return <article className="study-document"><header className="study-heading"><h1>研究</h1></header>{query.isPending ? <Loading /> : <ErrorNotice error={query.error ?? new Error('未找到研究')} retry={() => { void query.refetch(); }} />}<a className="text-link" href={href(back)}>返回研究列表</a></article>;
  const active = hasActiveResearchExecution(detail);
  const stop = async () => { setStopping(true); setStopError(undefined); try { const value = await api<ResearchDetail>(`/research/${detail.id}/stop`, { method: 'POST' }); cache.setQueryData([`/research/${detail.id}`], value); await cache.invalidateQueries({ queryKey: ['/research'] }); } catch (error) { setStopError(error); } finally { setStopping(false); } };
  const stoppedLabel = detail.status === 'STOPPED' && active ? '停止中' : detail.status === 'STARTED' ? '原生执行中' : undefined;
  const originalSubmission = detail.originalSubmission ?? detail.submission;
  const nativeSettings = datasetResearchSettings(detail.input.data);
  return <article className="study-document"><header className="study-heading"><div className="document-toolbar"><div className="document-identity"><a className="list-back text-link" href={href(back)}><ArrowLeftIcon size={14} />研究列表</a><span className="desktop-breadcrumb">研究 <CaretRightIcon size={11} /><code>{detail.id}</code></span><Status value={detail.status} label={stoppedLabel} /></div><Dropdown trigger={['click']} menu={{ items: [{ key: 'native-scan', label: '扫描原生请求参数', onClick: () => navigate(`/parameter-scans/new?sourceKind=research&researchId=${detail.id}`) }, { key: 'inputs', label: '查看原始身份', onClick: () => selectTab('inputs') }, ...(active ? [{ key: 'stop', label: stopping ? '停止中' : '停止研究', danger: true, disabled: stopping, onClick: () => confirm('停止研究？', '停止此研究的原始执行。', () => { void stop(); }) }] : [])] }}><Button className="icon-button" type="text" aria-label="研究操作" icon={<DotsThreeIcon size={21} />} /></Dropdown><DeleteRecord key={detail.id} kind="research" id={String(detail.id)} back={back} /></div>
      <RecordTitle text={detail.title} fallback="研究" />
      <div className="study-context"><span><DatabaseIcon size={14} />{detail.dataset?.name ?? detail.datasetId ?? '数据集名称未记录'}</span></div></header>
      <ErrorNotice error={stopError} /><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
      <ResearchConversation messages={detail.messages} />
      <ResearchAssessments assessments={detail.assessments} />
      {detail.toolPolicy === 'NAUTILUS_ONLY' && <NativeResearchContinuation key={detail.id} detail={detail} active={active} changed={() => { void query.refetch(); void cache.invalidateQueries({ queryKey: ['/research'] }); }} />}
      <Tabs className="document-tabs" activeKey={tab} onChange={value => selectTab(value)} items={[
        { key: 'evidence', label: '证据', children: <div className="evidence-content">
          {nativeSettings.modality === 'TRADE_TICK' && <section className="result-section"><h3>逐笔输入与证据范围</h3><NativeInputFacts settings={nativeSettings} preparation={detail.dataset?.preparation} coverage={detail.final?.inputCoverage ?? detail.dataset?.inputCoverage} /></section>}
          {!detail.final ? <Empty title={active ? '尚无原生结果' : '未产生原生结果'} action={<Button onClick={() => selectTab('inputs')}>查看执行</Button>} /> : <section className="independent-result"><div className="section-heading"><h3>{detail.final.nativeStrategyResult ? 'Nautilus 官方策略历史回测' : '执行结果'}</h3></div>
            {detail.final.nativeStrategyResult && <NativeStrategyResultView value={detail.final.nativeStrategyResult} />}
            {detail.final.status !== 'COMPUTED' && <Status value={detail.final.status} label={detail.final.status === 'STARTED' ? '原生执行中' : detail.final.status === 'FAILED' ? '原生执行失败' : undefined} />}
            <ExecutionFailure error={detail.final.error} detail={detail.final.errorDetail} identity={`研究 #${detail.id}${detail.final.trialId !== null ? ` / 试验 #${detail.final.trialId}` : ''}`} title="原生执行失败" />
            {detail.final.reason && detail.final.reason !== detail.final.error && <details className="reason-disclosure"><summary>研究结论</summary><p>{detail.final.reason}</p></details>}
          </section>}
        </div> },
        { key: 'literature', label: '文献', children: <Literature value={detail.literature} publicQueries={detail.input.publicLiteratureQueries} /> },
        { key: 'inputs', label: '输入与执行', children: <div className="input-content"><section><h3>研究输入</h3><TextDisclosure label="原始研究目标" value={detail.input.goal} collapsed /><Facts items={[["数据集", detail.dataset?.name ?? detail.datasetId ?? '未记录'], ["来源", detail.dataset?.source ?? '未记录'], ["模型", detail.model], ["推理强度", detail.input.thinkingLevel ?? '未记录'], ["推理速度", detail.input.serviceTier === 'priority' ? 'Fast' : detail.input.serviceTier === 'default' ? '标准' : '默认速度'], ["覆盖区间", detail.dataset ? <><Timestamp value={detail.dataset.start} /><span> 至 </span><Timestamp value={detail.dataset.end} /></> : '未记录'], ["创建时间", <Timestamp value={detail.createdAt} />]]} /></section><section><h3>原始身份</h3><Facts items={[["研究", <><code>{detail.id}</code><Copy value={String(detail.id)} label="复制研究身份" /></>], ["请求", originalSubmission?.requestId ? <><code>{originalSubmission.requestId}</code><Copy value={originalSubmission.requestId} label="复制请求身份" /></> : '未记录'], ["提交", originalSubmission ? <code>{originalSubmission.id}</code> : '未记录'], ["提交状态", originalSubmission ? <Status value={originalSubmission.status} /> : '未记录'], ["终态原因", originalSubmission?.reason ? <Reason value={originalSubmission.reason} /> : '未提供']]} /></section>{detail.submission && detail.submission.id !== originalSubmission?.id && <section><h3>当前执行</h3><Facts items={[["请求", <code>{detail.submission.requestId ?? '未记录'}</code>], ["提交", <code>{detail.submission.id}</code>], ["提交状态", <Status value={detail.submission.status} />], ["终态原因", detail.submission.reason ? <Reason value={detail.submission.reason} /> : '未提供']]} /></section>}
          {!!detail.previousStops?.length && <section><h3>历史停止记录</h3>{detail.previousStops.map(stop => <Facts key={stop.previousSubmissionId} items={[["原提交", <code>{stop.previousSubmissionId}</code>], ["停止任务", <code>{String(stop.final.taskId ?? '未记录')}</code>], ["状态", <Status value={String(stop.final.status)} />], ["停止原因", typeof stop.final.reason === 'string' ? stop.final.reason : '未记录']]} />)}</section>}
          {!!detail.previousNativeResults?.length && <section><h3>历史原生研究结果</h3>{detail.previousNativeResults.map(run => <div key={run.previousSubmissionId}><p>原提交 #{run.previousSubmissionId} · 任务 #{String(run.final.taskId ?? '未记录')}</p><TextDisclosure label="原始结果" value={JSON.stringify(run.final, null, 2)} collapsed /></div>)}</section>}
          <section><h3>最近执行记录</h3><Table<TaskView> rowKey="id" size="small" dataSource={detail.tasks} pagination={{ pageSize: 10, hideOnSinglePage: true }} scroll={{ x: 500 }} columns={[{ title: '任务', render: (_value, task) => <div className="trial-name">{task.name}<code>#{task.id}</code></div> }, { title: '状态', render: (_value, task) => <Status value={task.outcome ?? task.status} /> }, { title: '错误', render: (_value, task) => task.error ? <Reason value={task.error} /> : <span className="quiet">未提供</span> }]} /></section></div> },
      ]} />
  </article>;
}
export function NativeStrategyResultView({ value }: { value: NonNullable<ResearchDetail['final']>['nativeStrategyResult'] }) {
  if (!value) return null;
  return <section aria-label="Nautilus 原始策略结果">
    {value.instrumentDefinition && typeof value.instrumentDefinition === 'object' && !Array.isArray(value.instrumentDefinition) && value.instrumentDefinition.mode === 'STATIC_SCENARIO' && <p role="note">静态规格情景：假设所选目录合约规格在整个窗口不变，未验证当时真实规格；这不是实际历史可交易性或实盘证据。</p>}
    <p>这是 Nautilus 官方策略的回测执行报告，不代表独立样本验证通过、综合星级或模拟盘与实盘准入。原始配置、数据选择、费用假设和报告保持原生含义；不在浏览器重算绩效。</p>
    <Facts items={[["Nautilus 版本", typeof value.nativeVersion === 'string' ? value.nativeVersion : '未记录'], ["证据范围", typeof value.scientificClaim === 'string' ? value.scientificClaim : '未记录']]} />
    <details><summary>原始策略配置与数据选择</summary><Copy value={JSON.stringify(value.request ?? null, null, 2)} label="复制原始请求" /><pre>{JSON.stringify(value.request ?? null, null, 2)}</pre></details>
    <details><summary>上游原始执行报告</summary><p className="quiet">保留官方统计键和原始编码；未把持仓收益统计转换成日权益收益。</p><Copy value={JSON.stringify(value.canonicalResult ?? null, null, 2)} label="复制原生报告" /><pre>{JSON.stringify(value.canonicalResult ?? null, null, 2)}</pre></details>
    <details><summary>输入记录数与运行说明</summary><pre>{JSON.stringify({ dataRowsByType: value.dataRowsByType ?? null, notes: value.notes ?? null, instrumentDefinition: value.instrumentDefinition ?? null }, null, 2)}</pre></details>
  </section>;
}

export function ResearchAssessments({ assessments }: { assessments: ResearchDetail['assessments'] }) {
  if (!assessments?.length) return null;
  const record = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
  const stars = (value: unknown) => typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= 5 ? `${value} / 5 星` : '未评级';
  return <section aria-label="双路研究原始评级"><h2>双路研究评级</h2><p>来自原始 Pi 任务，不从 Agent 对话重新推断。评级仅评价已记录方法和证据，不代表收益承诺或交易许可。</p>
    {assessments.map(row => {
      const result = row.result ?? {}; const judgment = record(result.judgment); const data = record(result.dataJudgment);
      const evidence = record(result.evidence); const logic = record(record(record(evidence.logic).result).judgment);
      return <article key={row.taskId} id={`assessment-${row.taskId}`}>
        <h3>{row.mode === 'LOGIC' ? '事前逻辑审阅' : '综合研究报告'} #{row.taskId}</h3>
        <p>{row.configuration === 'CURRENT' ? '当前配置' : '历史配置，不适用于当前输入'} · {row.status}</p>
        <Facts items={row.mode === 'LOGIC' ? [['逻辑评级', stars(judgment.stars)]] : [['逻辑评级', stars(logic.stars)], ['数据评级', stars(data.stars)], ['综合评级', stars(judgment.stars)], ['评级状态', typeof result.ratingStatus === 'string' ? result.ratingStatus : '未完成']]} />
        {record(result.instrumentDefinitionAssumption).mode === 'STATIC_SCENARIO' && <p role="note">条件性情景评级：历史合约规格未核实，不是实际历史验证或交易许可。</p>}
        {typeof judgment.rationale === 'string' && <p>{judgment.rationale}</p>}
        {row.error && <p>{row.error}</p>}
        <TextDisclosure label="原始报告、证据范围与未完成事项" value={JSON.stringify(row.result, null, 2)} collapsed />
        <nav aria-label={`报告 ${row.taskId} 原始证据`}>{row.evidenceLinks.map(link => <a key={link.taskId} href={link.href} target="_blank" rel="noreferrer">原任务 #{link.taskId} </a>)}</nav>
      </article>;
    })}
  </section>;
}

export function ResearchConversation({ messages }: { messages: ResearchDetail['messages'] }) {
  if (!messages?.length) return null;
  return <section className="research-conversation" aria-label="Agent 对话"><h2>Agent 对话</h2>
    {messages.map(message => <div key={message.entryId} className="research-message"><span className="quiet">Agent</span><p style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{message.text}</p></div>)}
  </section>;
}

export function canContinueResearch(detail: ResearchDetail, active: boolean): boolean {
  if (detail.toolPolicy !== 'NAUTILUS_ONLY' || active || !detail.submission) return false;
  const stopped = detail.final?.status === 'STOPPED' && detail.final.trialId === null;
  const ownerStopped = detail.submission.status === 'unanswered' && detail.submission.reason === 'aborted'
    && (stopped && detail.final?.reason === 'owner_stop' || !!detail.final?.nativeStrategyResult);
  const modelFailedAfterResult = detail.submission.status === 'unanswered' && detail.submission.reason === 'model_error' && !!detail.final?.nativeStrategyResult;
  return !!ownerStopped || modelFailedAfterResult || detail.submission.status === 'done'
    && (!detail.final || !!detail.final.nativeStrategyResult || stopped);
}

/** Reuses the existing resume endpoint and original submission identity, including uncertain retries. */
export function NativeResearchContinuation({ detail, active, changed }: { detail: ResearchDetail; active: boolean; changed: () => void }) {
  const key = `native-research-resume:${detail.id}`;
  type Intent = { submissionId: number; instruction: string };
  const [pending, setPending] = useState<Intent | undefined>(() => readDraft<Intent>(key));
  const [instruction, setInstruction] = useState(pending?.instruction ?? '');
  const [busy, setBusy] = useState(false); const inFlight = useRef(false); const [error, setError] = useState<unknown>();
  const available = canContinueResearch(detail, active);
  const send = async () => {
    if (inFlight.current || !pending && (!available || !instruction.trim())) return;
    const intent = pending ?? { submissionId: detail.submission!.id, instruction: instruction.trim() };
    saveDraft(key, intent); setPending(intent); inFlight.current = true; setBusy(true); setError(undefined);
    try {
      await api(`/research/${detail.id}/resume`, { method: 'POST', body: intent });
      removeDraft(key); setPending(undefined); setInstruction(''); changed();
    } catch (failure) {
      setError(failure);
      if (failure instanceof ApiError && failure.status >= 400 && failure.status < 500 && failure.status !== 401) { removeDraft(key); setPending(undefined); }
    } finally { inFlight.current = false; setBusy(false); }
  };
  if (!available && !pending && !error) return null;
  return <section aria-label="继续原生研究"><p>在同一会话补充要求或调整参数。保留原任务与结果，远端执行未决时不会重复提交。</p>
    <Input.TextArea aria-label="后续研究要求" placeholder="回答 Agent 的问题，或补充后续研究要求" value={instruction} disabled={!!pending || busy} onChange={event => setInstruction(event.target.value)} />
    <Button onClick={() => { void send(); }} loading={busy} disabled={busy || !pending && (!available || !instruction.trim())}>{pending ? '核对原继续请求' : '继续研究'}</Button>
    {pending && <DraftNotice saved={draftIsPersisted(key)} />}<ErrorNotice error={error} />
  </section>;
}
function NewStudy({ route }: { route: Route }) {
  const cache = useQueryClient(); const catalog = useResource<ModelCatalog>('/models'); const datasets = useResource<DatasetView[]>('/datasets'); const settings = useResource<SettingsView>('/settings');
  const [form] = Form.useForm(); const [draft] = useState(() => readDraft<Record<string, unknown>>('research')); const [draftSaved, setDraftSaved] = useState(() => draftIsPersisted('research')); const initialized = useRef(false);
  const listParams = new URLSearchParams(route.params); listParams.delete('from'); listParams.delete('dataset');
  const returnPath = listPathFor({ ...route, params: listParams });
  const command = useCommand<ResearchReceipt>('research', result => { removeDraft('research'); void cache.invalidateQueries({ queryKey: ['/research'] }); if (currentRoute().page === 'research' && currentRoute().creating) navigate(returnPath.replace('/research', `/research/${result.conversationId}`)); });
  const origin = route.params.get('from') === 'data' ? { path: '/settings?section=data', label: '数据' } : { path: returnPath, label: '研究' };
  const [thinkingChanged, setThinkingChanged] = useState(false);
  const thinkingLevel = Form.useWatch('thinkingLevel', form) as string | undefined;
  const serviceTier = Form.useWatch('serviceTier', form) as ServiceTier | undefined;
  const selectedModelKey = Form.useWatch('model', form) as string | undefined;
  const selectedModel = catalog.data?.models.find(model => modelKey(model) === selectedModelKey);
  const modelOptions = catalog.data?.models.map(model => ({ value: modelKey(model), label: `${model.name} / ${model.provider}` })) ?? [];
  if (selectedModelKey && !modelOptions.some(option => option.value === selectedModelKey)) { const recorded = JSON.parse(selectedModelKey) as ModelRef; modelOptions.push({ value: selectedModelKey, label: `${recorded.provider}/${recorded.modelId} / 当前不可用` }); }
  const selectedDatasetId = Form.useWatch('datasetId', form) as string | undefined;
  const selectedDataset = datasets.data?.find(dataset => dataset.id === selectedDatasetId);
  useEffect(() => {
    if (initialized.current || !catalog.data || !datasets.data || !settings.data) return;
    initialized.current = true;
    if (command.values) { form.setFieldsValue({ ...command.values, publicLiteratureTopics: Array.isArray(command.values.publicLiteratureQueries) ? command.values.publicLiteratureQueries.join('\n') : '', model: modelKey(command.values.model as ModelRef) }); return; }
    const preferred = settings.data.defaultModel && catalog.data.models.find(model => modelKey(model) === modelKey(settings.data!.defaultModel!));
    form.setFieldsValue({ ...(preferred ? { model: modelKey(preferred), thinkingLevel: settings.data.thinkingLevel ?? preferred.effectiveThinkingLevel, serviceTier: !draft?.model || draft.model === modelKey(preferred) ? settings.data.serviceTier ?? undefined : undefined } : {}), ...draft, ...(route.params.get('dataset') ? { datasetId: route.params.get('dataset') } : {}) });
  }, [catalog.data, datasets.data, settings.data]);
  useEffect(() => {
    if (!command.pending && selectedModel && thinkingLevel && !selectedModel.thinkingLevels.some(level => level === thinkingLevel)) { form.setFieldValue('thinkingLevel', undefined); setThinkingChanged(true); setDraftSaved(saveDraft('research', form.getFieldsValue())); }
  }, [selectedModel, thinkingLevel, command.pending]);
  useEffect(() => {
    if (!command.pending && selectedModel && serviceTier && !selectedModel.serviceTiers.includes(serviceTier)) { form.setFieldValue('serviceTier', undefined); setDraftSaved(saveDraft('research', form.getFieldsValue())); }
  }, [selectedModel, serviceTier, command.pending]);
  const preview = Form.useWatch([], form) as Record<string, string> | undefined;
  const [specError, setSpecError] = useState<unknown>();
  return <div className="authoring-page"><a className="text-link back-link" href={href(origin.path)}><ArrowLeftIcon size={14} />{origin.label}</a><div className="authoring-heading"><div className="authoring-symbol"><FlaskIcon weight="duotone" size={24} /></div><h1>新建研究</h1></div>
    <ErrorNotice error={catalog.error ?? datasets.error ?? settings.error} retry={() => { void catalog.refetch(); void datasets.refetch(); void settings.refetch(); }} />
    {catalog.isPending || datasets.isPending || settings.isPending ? <Loading /> : !catalog.data || !datasets.data || !settings.data ? null : <>
    {catalog.data && !catalog.data.models.length && <div className="prerequisite">没有可用模型 <a href={href('/settings')}>连接模型 <ArrowUpRightIcon size={13} /></a></div>}
    {datasets.data && !datasets.data.some(item => item.ready) && <div className="prerequisite">没有就绪的数据集 <a href={href('/settings?section=data')}>查看数据 <ArrowUpRightIcon size={13} /></a></div>}
    <PendingCommand command={command} />
    <Form form={form} layout="vertical" validateTrigger="onBlur" scrollToFirstError={{ focus: true }} onValuesChange={(_changed, values: Record<string, unknown>) => { if (!command.pending) setDraftSaved(saveDraft('research', values)); }} onFinish={(values: Record<string, string>) => { try { const input = nativeResearchInput(values); setSpecError(undefined); void command.submit(input); } catch (error) { setSpecError(error); } }}  disabled={!!command.pending || command.busy}>
      <div className="form-section-heading"><FlaskIcon size={16} /><h2>研究问题</h2></div><p className="quiet">描述策略、数据范围与参数要求。Agent 使用 Nautilus 原生配置与计算，不再生成 Wasm 因子或自定义评分。</p>
      <Form.Item label="研究问题" name="goal" rules={[{ required: true, whitespace: true, message: '请输入研究问题' }]}><Input.TextArea autoSize={{ minRows: 2, maxRows: 6 }} placeholder="例如：使用这个数据集运行 Nautilus 官方策略，使用原配置并展示原生回测报告。" /></Form.Item>
      <Form.Item label="公开论文检索主题（可选）" name="publicLiteratureTopics" extra="仅这些主题会发送到论文检索服务，Agent 会自动检索整理。每行一个，最多 3 个，每个最多 500 字。" rules={[{ validator: (_rule, value?: string) => { try { publicLiteratureTopics(value); return Promise.resolve(); } catch { return Promise.reject(new Error('请输入最多 3 个公开主题，每行 1 个、最多 500 字，不含控制字符')); } } }]}><Input.TextArea autoSize={{ minRows: 2, maxRows: 5 }} placeholder="market microstructure&#10;momentum transaction costs" /></Form.Item>
      <div className="form-section-heading"><DatabaseIcon size={16} /><h2>市场与数据</h2></div>
      <Form.Item label="数据集" name="datasetId" rules={[{ required: true, message: '请选择数据集' }, { validator: (_rule, value: string) => !value || datasets.data?.some(item => item.id === value && item.ready) ? Promise.resolve() : Promise.reject(new Error('数据集尚未就绪')) }]}><Select showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} placeholder="选择已登记数据集" options={datasets.data?.map(item => ({ value: item.id, label: `${item.nativeModality === 'TRADE_TICK' ? 'TradeTick / ' : ''}${item.name} / ${item.currency}`, disabled: !item.ready }))} /></Form.Item>
      {selectedDataset && <div className="dataset-context"><span><DatabaseIcon size={13} />{selectedDataset.source}</span><code>{selectedDataset.id}</code><code>{selectedDataset.currency}</code>{(selectedDataset.start || selectedDataset.end) && <span><Timestamp value={selectedDataset.start} /> 至 <Timestamp value={selectedDataset.end} /></span>}<span>{selectedDataset.ready ? '已就绪' : '未就绪'}</span></div>}
      {selectedDataset?.researchSettings?.modality === 'TRADE_TICK' && <NativeInputFacts settings={selectedDataset.researchSettings} preparation={selectedDataset.preparation} coverage={selectedDataset.inputCoverage} />}
      <Form.Item label="模型" name="model" rules={[{ required: true, message: '请选择模型' }, { validator: (_rule, value: string) => !value || catalog.data?.models.some(model => modelKey(model) === value) ? Promise.resolve() : Promise.reject(new Error('所选模型不可用，请重新选择')) }]}><Select showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} placeholder="选择可用模型" options={modelOptions} onChange={value => { const model = catalog.data?.models.find(item => modelKey(item) === value); setThinkingChanged(false); form.setFieldValue('thinkingLevel', settings.data?.defaultModel && modelKey(settings.data.defaultModel) === value ? settings.data.thinkingLevel ?? model?.effectiveThinkingLevel : model?.effectiveThinkingLevel); form.setFieldValue('serviceTier', settings.data?.defaultModel && modelKey(settings.data.defaultModel) === value ? settings.data.serviceTier ?? undefined : undefined); setDraftSaved(saveDraft('research', form.getFieldsValue())); }} /></Form.Item>
      {selectedModel && <p className="model-context"><code>{selectedModel.provider}/{selectedModel.modelId}</code></p>}
      <Form.Item label="推理速度" name="serviceTier" extra={selectedModel && !selectedModel.serviceTiers.length ? '当前模型服务不支持速度配置' : undefined}><Select allowClear disabled={!selectedModel?.serviceTiers.length || !!command.pending || command.busy} placeholder={!selectedModel ? '先选择模型' : selectedModel.serviceTiers.length ? '使用默认速度' : '由模型服务决定'} suffixIcon={<CaretDownIcon size={14} />} options={selectedModel?.serviceTiers.map(value => ({ value, label: value === 'priority' ? 'Fast' : '标准' }))} /></Form.Item>
      <div className="form-columns"><Form.Item label="推理强度" name="thinkingLevel" extra={thinkingChanged ? '原推理强度不可用，请重新选择' : undefined} rules={[{ required: !!selectedModel, message: '请选择推理强度' }]}><Select onChange={() => setThinkingChanged(false)} suffixIcon={<CaretDownIcon size={14} />} disabled={!selectedModel || !!command.pending || command.busy} options={selectedModel?.thinkingLevels.map(value => ({ value, label: thinkingLabels[value] }))} /></Form.Item></div>
      <details className="submission-preview" open><summary>提交预览</summary><Facts items={[["研究问题", preview?.goal || '待填写'], ["市场与数据", selectedDataset ? `${selectedDataset.name} / ${selectedDataset.currency}` : '待选择'], ["模型与推理", selectedModel ? `${selectedModel.provider}/${selectedModel.modelId} / ${thinkingLabels[thinkingLevel ?? ''] ?? '未选择'}` : '待选择'], ["推理速度", serviceTier === 'priority' ? 'Fast' : serviceTier === 'default' ? '标准' : '默认速度']]} /></details>
      <ErrorNotice error={specError ?? command.error} /><DraftNotice saved={draftSaved || !!command.pending} /><div className="form-actions"><Button disabled={false} onClick={() => navigate(origin.path)}>取消</Button>{command.pending ? <Button type="primary" disabled={command.busy} aria-busy={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>{command.busy ? '提交中' : '核对提交'}</Button> : <Button type="primary" htmlType="submit" loading={command.busy} disabled={!catalog.data?.models.length || !datasets.data?.some(item => item.ready)}>开始研究 <ArrowRightIcon size={15} /></Button>}</div>
    </Form></>}
  </div>;
}
