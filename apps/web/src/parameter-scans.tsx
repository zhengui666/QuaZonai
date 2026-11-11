import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Button, Checkbox, Form, Input, Select, Table, Tabs, Tag } from 'antd';
import { ArrowLeftIcon, ArrowRightIcon, PlusIcon, SlidersHorizontalIcon } from '@phosphor-icons/react';
import { useQueries, useQueryClient } from '@tanstack/react-query';
import type { FactorView, ResearchDetail } from '../../../packages/contracts/src/index.ts';
import type {
  ParameterAxis, ParameterScanKey, ParameterScanMetricId, ParameterScanMode, ParameterScanOptions,
  ParameterScanPlan, ParameterScanPreview, ParameterScanSource, ParameterScanSurface,
  ParameterScanTrial, ParameterScanTrials, ParameterScanView, ParameterValues,
} from '../../../packages/contracts/src/parameter-scans.ts';
import { parameterScanAxisValue, parameterScanParameters, parameterScanPlan } from '../../../packages/contracts/src/parameter-scans.ts';
import { api } from './api';
import { draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useParameterScanDetail, useResource, useStudies } from './data-hooks';
import { ExecutionFailure } from './diagnostics';
import { RecordTitle } from './disclosure';
import { ListTools, useListWorkspace } from './list-workspace';
import { scanComparisonOrdinals, scanFixedParameters, scanValidationMetrics } from './parameter-scan-values';
import { currentRoute, href, navigate, type Route } from './router';
import { Copy, DraftNotice, Empty, ErrorNotice, Exact, Facts, Loading, Metrics, PendingCommand, Status, Timestamp } from './ui';
import './parameter-scans.css';

const Plots = lazy(() => import('./parameter-scan-plots').then(module => ({ default: module.ParameterHeatmap })));
const Curves = lazy(() => import('./parameter-scan-plots').then(module => ({ default: module.ParameterCurveComparison })));
const Trend = lazy(() => import('./parameter-scan-plots').then(module => ({ default: module.ParameterTrend })));
const collection = '/parameter-scans';
const active = (value: ParameterScanView) => ['QUEUED', 'RUNNING', 'RECONCILING', 'STOPPING'].includes(value.status);
const labels: Record<string, string> = { FACTOR: '因子评估', STRATEGY: '策略账户回测', COMPLETED: '计算完成', STOPPING: '停止中', RECONCILING: '远端回执待核对', SKIPPED: '约束跳过', PENDING: '尚未执行' };
const parameterLabel = (key: string) => parameterScanParameters.find(item => item.key === key)?.label ?? key;
function ScanStatus({ status }: { status: string }) { return <Status value={status} label={labels[status]} />; }
function ParameterFacts({ values, copy = false }: { values: ParameterValues; copy?: boolean }) { return <div className="scan-parameter-list">{Object.entries(values).map(([key, value]) => <span key={key}>{parameterLabel(key)} <Exact value={value} /></span>)}{copy && <Copy value={JSON.stringify(values, null, 2)} label="复制参数" />}</div>; }
function StaleNotice({ stale, at }: { stale: boolean; at: number }) { return stale && at ? <p className="quiet" role="status">连接未更新，下方保留上次读取记录：<Timestamp value={new Date(at).toISOString()} /></p> : null; }

export default function ParameterScans({ route }: { route: Route }) {
  if (route.creating) return <ScanBuilder route={route} />;
  return route.id ? <ScanDetail key={route.id} route={route} id={route.id} /> : <ScanList route={route} />;
}

function ScanList({ route }: { route: Route }) {
  const list = useListWorkspace<ParameterScanView>(route, collection, 'createdAt');
  return <section className="parameter-scans list-workspace" aria-label="参数扫描列表">
    <header className="list-heading"><h1>参数扫描</h1><Button type="primary" icon={<PlusIcon size={15} />} onClick={() => navigate(`${collection}/new`)}>新建扫描</Button></header>
    <p className="quiet">对同一原始模型的明确参数运行原生评估，比较验证分区结果；无需先创建组合或配置研究模型。</p>
    <ListTools list={list} searchLabel="搜索参数扫描" placeholder="搜索扫描名称、来源或数据集" filters={[{ key: 'status', label: '执行状态' }, { key: 'mode', label: '扫描类型', options: [{ value: 'FACTOR', label: '因子评估' }, { value: 'STRATEGY', label: '策略账户回测' }] }, { key: 'datasetId', label: '数据集' }]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} /><StaleNotice stale={list.query.isError} at={list.query.dataUpdatedAt} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<ParameterScanView> {...list.tableProps} rowKey="id" size="small" scroll={{ x: 1150 }} locale={{ emptyText: <Empty title={list.hasFilters ? '没有匹配的参数扫描' : '尚无参数扫描'} /> }} columns={[
      { title: '扫描名称', key: 'name', width: 220, fixed: 'left', ...list.sortColumn('name'), render: (_cell, row) => <a className="list-record-name" {...list.linkProps(row.id)}>{row.name}</a> },
      { title: '类型', width: 135, render: (_cell, row) => labels[row.mode] },
      { title: '执行状态', width: 150, render: (_cell, row) => <ScanStatus status={row.status} /> },
      { title: '数据集', width: 180, render: (_cell, row) => row.sourceView.datasetName ?? row.sourceView.datasetId ?? '未记录' },
      { title: '已完成 / 可执行', width: 160, render: (_cell, row) => <><Exact value={row.counts.completed} /> / <Exact value={row.counts.eligible} /></> },
      { title: '失败 / 跳过', width: 125, render: (_cell, row) => <><Exact value={row.counts.failed} /> / <Exact value={row.counts.skipped} /></> },
      { title: '最佳验证值', width: 130, render: (_cell, row) => <Exact value={row.best?.metric.value} missing="暂无可比结果" /> },
      { title: '创建时间', key: 'createdAt', width: 200, ...list.sortColumn('createdAt'), render: (_cell, row) => <Timestamp value={row.createdAt} /> },
    ]} />}
  </section>;
}

type AxisFields = { enabled: boolean; kind: 'values' | 'range'; valuesText: string; start: string; end: string; step: string };
type ScanFields = {
  name: string; sourceKind: ParameterScanSource['kind']; factorId: string; researchId: string; trialId: string;
  mode: ParameterScanMode; metricId: ParameterScanMetricId; concurrency: string;
  parameters: Partial<Record<ParameterScanKey, AxisFields>>;
};
function fieldsFromPlan(plan: ParameterScanPlan): Partial<ScanFields> {
  return { name: plan.name, sourceKind: plan.source.kind, ...(plan.source.kind === 'factor' ? { factorId: plan.source.factorId } : { researchId: String(plan.source.researchId), trialId: String(plan.source.trialId) }), mode: plan.mode, metricId: plan.objective.metricId, concurrency: String(plan.execution.concurrency),
    parameters: Object.fromEntries(plan.axes.map(axis => [axis.key, axis.domain.kind === 'values' ? { enabled: true, kind: 'values', valuesText: axis.domain.values.join(', ') } : { enabled: true, ...axis.domain }])) };
}
function ResearchSourceFields({ researchId, form }: { researchId?: string; form: ReturnType<typeof Form.useForm<ScanFields>>[0] }) {
  const studies = useStudies(); const detail = useResource<ResearchDetail>(`/research/${researchId}`, { enabled: !!researchId });
  return <><Form.Item label="来源研究" name="researchId" rules={[{ required: true, message: '请选择来源研究' }]}><Select showSearch optionFilterProp="label" options={studies.items.map(item => ({ value: String(item.id), label: `${item.title} / #${item.id}` }))} onChange={() => form.setFieldValue('trialId', undefined)} /></Form.Item>
    {studies.hasNextPage && <Button size="small" loading={studies.isFetchingNextPage} onClick={() => { void studies.fetchNextPage(); }}>载入更多研究</Button>}
    <ErrorNotice error={studies.error ?? detail.error} />
    <Form.Item label="来源试验" name="trialId" rules={[{ required: true, message: '请选择已计算的来源试验' }]}><Select showSearch optionFilterProp="label" loading={detail.isFetching} options={detail.data?.trialRecords.map(item => ({ value: String(item.id), label: `试验 #${item.id} / ${item.hypothesis}`, disabled: item.status !== 'COMPUTED' }))} /></Form.Item></>;
}

function ScanBuilder({ route }: { route: Route }) {
  const cache = useQueryClient(); const [form] = Form.useForm<ScanFields>(); const initialized = useRef(false);
  const factors = useResource<FactorView[]>('/factors');
  const [draft] = useState(() => readDraft<ScanFields>('parameter-scan')); const [saved, setSaved] = useState(() => draftIsPersisted('parameter-scan'));
  const [preview, setPreview] = useState<ParameterScanPreview>(); const [previewing, setPreviewing] = useState(false); const [error, setError] = useState<unknown>();
  const [modeChoice, setModeChoice] = useState<{ source: string; mode: ParameterScanMode }>();
  const command = useCommand<ParameterScanView>('parameter-scans', value => { removeDraft('parameter-scan'); void cache.invalidateQueries({ queryKey: [collection] }); if (currentRoute().page === 'parameter-scans' && currentRoute().creating) navigate(`${collection}/${encodeURIComponent(value.id)}`); });
  const sourceKind = Form.useWatch('sourceKind', form) ?? 'factor'; const factorId = Form.useWatch('factorId', form); const researchId = Form.useWatch('researchId', form); const trialId = Form.useWatch('trialId', form);
  const mode: ParameterScanMode = Form.useWatch('mode', form) ?? 'FACTOR';
  const sourceKey = sourceKind === 'factor' ? `factor:${factorId ?? ''}` : `research-trial:${researchId ?? ''}:${trialId ?? ''}`;
  // Discover a new source before forcing a mode: a multi-member trial can be
  // strategy-only, and must not fail its first capability read as a factor.
  const optionParams = new URLSearchParams({ sourceKind });
  if (modeChoice?.source === sourceKey) optionParams.set('mode', modeChoice.mode);
  if (sourceKind === 'factor' && factorId) optionParams.set('sourceId', factorId);
  if (sourceKind === 'research-trial' && researchId && trialId) { optionParams.set('researchId', researchId); optionParams.set('trialId', trialId); }
  const selectedSource = sourceKind === 'factor' ? !!factorId : !!researchId && !!trialId;
  const options = useResource<ParameterScanOptions>(`${collection}/options?${optionParams}`, { enabled: selectedSource });
  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    const pending = command.values ? fieldsFromPlan(command.values as unknown as ParameterScanPlan) : undefined;
    const initial: Partial<ScanFields> = { sourceKind: 'factor', mode: 'FACTOR', concurrency: '1', ...draft };
    form.setFieldsValue({ ...initial,
      ...(route.params.get('sourceKind') === 'research-trial' ? { sourceKind: 'research-trial', researchId: route.params.get('researchId') ?? '', trialId: route.params.get('trialId') ?? '' } : route.params.get('sourceId') ? { sourceKind: 'factor', factorId: route.params.get('sourceId')! } : {}), ...pending });
  }, [command.values, draft, form, route.params]);
  useEffect(() => {
    if (!options.data?.source || command.pending) return;
    if (!options.data.modes.includes(mode)) {
      const supported = options.data.modes[0];
      if (supported) { form.setFieldValue('mode', supported); setPreview(undefined); }
      return;
    }
    if (options.data.objectives[0]?.mode !== mode) {
      setModeChoice({ source: sourceKey, mode });
      return;
    }
    for (const definition of options.data.parameters.filter(item => item.modes.includes(mode))) {
      if (form.getFieldValue(['parameters', definition.key])) continue;
      const value = options.data.defaults[definition.key] ?? '';
      form.setFieldValue(['parameters', definition.key], { enabled: definition.key === 'fast_period' || definition.key === 'slow_period', kind: 'values', valuesText: value, start: value, end: value, step: '1' });
    }
    if (!options.data.objectives.some(item => item.metricId === form.getFieldValue('metricId') && item.mode === mode)) form.setFieldValue('metricId', options.data.objectives.find(item => item.mode === mode)?.metricId);
  }, [options.data, mode, sourceKey, form, command.pending]);
  const inspect = async (values: ScanFields) => {
    setError(undefined); setPreview(undefined);
    if (!options.data?.source) return;
    try {
      const axes: ParameterAxis[] = options.data.parameters.filter(item => item.modes.includes(values.mode) && values.parameters?.[item.key]?.enabled).map(item => {
        const field = values.parameters[item.key]!;
        return { key: item.key, domain: field.kind === 'range' ? { kind: 'range', start: field.start, end: field.end, step: field.step } : { kind: 'values', values: (field.valuesText ?? '').split(/[,，\s]+/).filter(Boolean) } };
      });
      const input = parameterScanPlan({ name: values.name, source: options.data.source.source, mode: values.mode, axes,
        objective: { metricId: values.metricId, partition: 'VALIDATION', direction: 'maximize' }, execution: { concurrency: Number(values.concurrency) } });
      setPreviewing(true); const result = await api<ParameterScanPreview>(`${collection}/preview`, { method: 'POST', body: input }); setPreview(result);
    } catch (failure) { setError(failure); } finally { setPreviewing(false); }
  };
  return <div className="parameter-scans authoring-page"><a className="text-link back-link" href={href(collection)}><ArrowLeftIcon size={14} />参数扫描</a>
    <div className="authoring-heading"><div className="authoring-symbol"><SlidersHorizontalIcon size={24} /></div><h1>新建参数扫描</h1></div>
    <p className="quiet">复用所选原始模型与已登记数据，仅改变明确列出的参数。默认单任务执行，不需要模型调用。</p>
    <ErrorNotice error={factors.error} retry={() => { void factors.refetch(); }} />
    <Form form={form} layout="vertical" scrollToFirstError={{ focus: true }} disabled={!!command.pending || command.busy || previewing} onValuesChange={(_change, values) => { setPreview(undefined); setError(undefined); if (!command.pending) setSaved(saveDraft('parameter-scan', values)); }} onFinish={values => { void inspect(values); }}>
      <PendingCommand command={command} />
      <Form.Item label="扫描名称" name="name" rules={[{ required: true, whitespace: true, message: '请输入扫描名称' }]}><Input autoFocus /></Form.Item>
      <Form.Item label="来源类型" name="sourceKind" rules={[{ required: true }]}><Select options={[{ value: 'factor', label: '原子因子' }, { value: 'research-trial', label: '研究中的已计算试验' }]} /></Form.Item>
      {sourceKind === 'factor' ? <Form.Item label="来源因子" name="factorId" rules={[{ required: true, message: '请选择来源因子' }]}><Select showSearch optionFilterProp="label" loading={factors.isPending} options={factors.data?.map(item => ({ value: item.id, label: `${item.name || item.hypothesis} / ${item.id}`, disabled: item.nativeModality === 'TRADE_TICK' }))} /></Form.Item> : <ResearchSourceFields researchId={researchId} form={form} />}
      <Form.Item label="评估类型" name="mode" rules={[{ required: true }]}><Select onChange={(next: ParameterScanMode) => setModeChoice({ source: sourceKey, mode: next })} options={[{ value: 'FACTOR', label: '因子参数 / 验证分区 IC', disabled: !!options.data && !options.data.modes.includes('FACTOR') }, { value: 'STRATEGY', label: '策略参数 / 原生共享账户回测', disabled: !!options.data && !options.data.modes.includes('STRATEGY') }]} /></Form.Item>
      <ErrorNotice error={options.error} retry={() => { void options.refetch(); }} />{options.isFetching && <Loading rows={1} />}
      {options.data?.reason && <p className="scan-subtle-error" role="status">当前来源不支持此扫描：{options.data.reason}</p>}
      {options.data?.source && <>
        <Facts items={[["数据集", options.data.source.datasetName ?? options.data.source.datasetId], ["输入", 'BAR'], ["验证开始（ns）", <Exact value={options.data.source.interval.startNs} />], ["验证结束（不含，ns）", <Exact value={options.data.source.interval.endExclusiveNs} />], ["来源", options.data.source.label]]} />
        <div className="form-section-heading"><SlidersHorizontalIcon size={16} /><h2>参数范围</h2></div>
        <p className="quiet">勾选需要扫描的参数。未勾选的项沿用所选来源值；每个组合保留真实原生结果。</p>
        <div className="scan-parameters">{options.data.parameters.filter(item => item.modes.includes(mode)).map(item => <AxisEditor key={item.key} definition={item} form={form} current={options.data!.defaults[item.key]} />)}</div>
        <Form.Item label="比较指标（验证分区，最大化）" name="metricId" rules={[{ required: true, message: '请选择比较指标' }]}><Select options={options.data.objectives.filter(item => item.mode === mode).map(item => ({ value: item.metricId, label: item.label }))} /></Form.Item>
        <Form.Item label="同时执行数量" name="concurrency" extra="默认 1；并发是执行设置，不改变参数组合或科学口径。" rules={[{ required: true }, { validator: (_rule, value: string) => /^[1-9]\d*$/.test(value ?? '') && Number.isSafeInteger(Number(value)) ? Promise.resolve() : Promise.reject(new Error('请输入正整数')) }]}><Input inputMode="numeric" /></Form.Item>
      </>}
      {preview && <section className="scan-notice" aria-label="扫描预览"><h3>组合预览</h3><dl className="scan-summary"><div><dt>全部组合</dt><dd><Exact value={preview.grid.total} /></dd></div><div><dt>可执行</dt><dd><Exact value={preview.grid.eligible} /></dd></div><div><dt>约束跳过</dt><dd><Exact value={preview.grid.skipped} /></dd></div><div><dt>并发</dt><dd>{preview.input.execution.concurrency}</dd></div></dl>
        <p>比较：{preview.objective.label} / VALIDATION；fast_period ≥ slow_period 的组合不执行。</p>
        {preview.warnings.map((warning, index) => <p className="quiet" key={index}>{warning}</p>)}
        <details><summary>前几个组合与固定值</summary><ParameterFacts values={preview.grid.fixed} /><Table rowKey="ordinal" size="small" pagination={false} dataSource={preview.examples} scroll={{ x: 550 }} columns={[{ title: '序号', dataIndex: 'ordinal' }, { title: '参数', render: (_cell, row) => <ParameterFacts values={row.parameters} /> }, { title: '约束', render: (_cell, row) => row.skipReason ?? '可执行' }]} /></details>
      </section>}
      <ErrorNotice error={error ?? command.error} /><DraftNotice saved={saved || !!command.pending} />
      <div className="form-actions"><Button disabled={false} onClick={() => navigate(collection)}>返回列表</Button>{command.pending ? <Button type="primary" disabled={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>核对原提交</Button> : <><Button htmlType="submit" loading={previewing} disabled={!options.data?.source || !!options.data.reason}>预览组合</Button>{preview && <Button type="primary" loading={command.busy} disabled={preview.grid.eligible === '0'} onClick={() => { void command.submit(preview.input); }}>开始扫描 <ArrowRightIcon size={14} /></Button>}</>}</div>
    </Form>
  </div>;
}

function AxisEditor({ definition, form, current }: { definition: ParameterScanOptions['parameters'][number]; form: ReturnType<typeof Form.useForm<ScanFields>>[0]; current?: string }) {
  const kind = Form.useWatch(['parameters', definition.key, 'kind'], form) ?? 'values'; const enabled = Form.useWatch(['parameters', definition.key, 'enabled'], form);
  const field = (name: keyof AxisFields) => ['parameters', definition.key, name];
  return <section className="scan-parameter"><div className="scan-parameter-header"><h3>{definition.label}</h3><Form.Item name={field('enabled')} valuePropName="checked" noStyle><Checkbox aria-label={`扫描${definition.label}`}>扫描此项</Checkbox></Form.Item></div>
    <p className="quiet">来源值 <Exact value={current} /> · {definition.kind === 'nanoseconds' ? '纳秒，按精确整数字符串传递' : definition.kind === 'integer' ? '正整数观测周期' : '按精确十进制文本传递'}</p>
    {enabled && <><Form.Item label="取值方式" name={field('kind')}><Select options={[{ value: 'values', label: '明确取值列表' }, { value: 'range', label: '起点 / 终点 / 步长' }]} /></Form.Item>
      {kind === 'range' ? <div className="scan-range">{(['start', 'end', 'step'] as const).map((name, index) => <Form.Item key={name} label={['起点（包含）', '终点（包含可达值）', '步长'][index]} name={field(name)} rules={[{ required: true, message: '请输入精确数值' }]}><Input inputMode="decimal" /></Form.Item>)}</div> : <Form.Item label="参数值（逗号或空格分隔）" name={field('valuesText')} rules={[{ required: true, whitespace: true, message: '请至少输入一个数值' }]}><Input.TextArea autoSize={{ minRows: 1, maxRows: 4 }} /></Form.Item>}
    </>}
  </section>;
}

function ScanDetail({ route, id }: { route: Route; id: string }) {
  const cache = useQueryClient(); const query = useParameterScanDetail(id); const value = query.data;
  const [stopping, setStopping] = useState(false); const [error, setError] = useState<unknown>();
  const tab = route.params.get('tab') ?? 'overview';
  const update = (patch: Record<string, string | undefined>) => { const params = new URLSearchParams(route.params); for (const [key, item] of Object.entries(patch)) { if (item === undefined) params.delete(key); else params.set(key, item); } navigate(`${collection}/${encodeURIComponent(id)}?${params}`, true); };
  const stop = async () => { setStopping(true); setError(undefined); try { const result = await api<ParameterScanView>(`${collection}/${encodeURIComponent(id)}/stop`, { method: 'POST' }); cache.setQueryData([`${collection}/${encodeURIComponent(id)}`], result); void cache.invalidateQueries({ queryKey: [collection] }); } catch (failure) { setError(failure); } finally { setStopping(false); } };
  return <div className="parameter-scans"><a className="text-link back-link" href={href(collection)}><ArrowLeftIcon size={14} />参数扫描列表</a><article className="study-document"><header className="study-heading"><div className="document-toolbar">{value && <><ScanStatus status={value.status} /><div className="scan-command-actions"><Button onClick={() => { void query.refetch(); }}>刷新</Button>{active(value) && <Button danger disabled={stopping || value.status === 'STOPPING'} loading={stopping} onClick={() => { void stop(); }}>{value.status === 'STOPPING' ? '停止中' : '停止扫描'}</Button>}</div></>}</div><RecordTitle name={value?.name} fallback="参数扫描" />{value && <div className="study-context"><span>{labels[value.mode]}</span><span>{value.sourceView.datasetName ?? value.sourceView.datasetId}</span><Tag>探索性结果，未获科学资格</Tag></div>}</header>
    <ErrorNotice error={query.error ?? error} retry={() => { void query.refetch(); }} /><StaleNotice stale={query.isError} at={query.dataUpdatedAt} />
    {query.isPending ? <Loading /> : value && <Tabs className="document-tabs" destroyOnHidden activeKey={tab} onChange={next => update({ tab: next })} items={[
      { key: 'overview', label: '概览', children: <div className="scan-detail-content"><ScanOverview value={value} /><Button onClick={() => update({ tab: 'trials' })}>查看全部参数结果</Button></div> },
      { key: 'trials', label: '结果表', children: <div className="scan-detail-content"><ScanTrials scan={value} route={route} update={update} /></div> },
      { key: 'surface', label: '参数分析', children: <div className="scan-detail-content"><ScanSurface scan={value} route={route} update={update} /></div> },
      { key: 'compare', label: '曲线对比', children: <div className="scan-detail-content"><ScanComparison scan={value} route={route} update={update} /></div> },
      { key: 'inputs', label: '输入与执行', children: <div className="scan-detail-content"><Facts items={[["扫描", value.id], ["原请求", value.requestId], ["主任务", value.taskId], ["来源", value.sourceView.label], ["原研究 / 试验", `${value.sourceView.researchId} / ${value.sourceView.trialId}`], ["创建时间", <Timestamp value={value.createdAt} />], ["更新时间", <Timestamp value={value.updatedAt} />], ["并发", value.execution.concurrency]]} />{value.sourceView.assumptions.length > 0 && <section className="result-section"><h3>原始假设</h3><ul>{value.sourceView.assumptions.map((item, index) => <li key={index}>{item}</li>)}</ul></section>}<details><summary>原始参数计划</summary><Copy value={JSON.stringify(value.input, null, 2)} label="复制扫描计划" /><pre className="scan-code">{JSON.stringify(value.input, null, 2)}</pre></details><details><summary>参数组合与固定值</summary><pre className="scan-code">{JSON.stringify(value.grid, null, 2)}</pre></details></div> },
    ]} />}
  </article></div>;
}

function ScanOverview({ value }: { value: ParameterScanView }) {
  const rows: [string, string][] = [['全部组合', value.counts.total], ['可执行', value.counts.eligible], ['已完成', value.counts.completed], ['可比较', value.counts.comparable], ['运行中', value.counts.running], ['尚未执行', value.counts.pending], ['失败', value.counts.failed], ['约束跳过', value.counts.skipped], ['已停止', value.counts.stopped], ['远端待核对', value.counts.unknown]];
  return <><dl className="scan-summary">{rows.map(([label, count]) => <div key={label}><dt>{label}</dt><dd><Exact value={count} /></dd></div>)}</dl>
    {active(value) && <div className="scan-notice" role="status"><p>{value.status === 'STOPPING' ? '停止请求已记录；等待当前原生执行结束及远端回执核对。' : value.status === 'RECONCILING' ? '原执行结果尚不确定，正在核对同一远端身份。' : '进度来自已记录的参数任务；关闭页面不会取消扫描。'}</p><p className="quiet">更新于 <Timestamp value={value.updatedAt} />。本地任务停止不代表远端已停止。</p></div>}
    <ExecutionFailure error={value.error} identity={`参数扫描 ${value.id} / 主任务 #${value.taskId}`} title="扫描未完整完成" />
    <section className="result-section"><h3>{active(value) ? '当前最佳可比验证结果（扫描未结束）' : '最佳可比验证结果'}</h3>{value.best ? <><p>试验 #{value.best.ordinal} · <Exact value={value.best.metric.value} unit={value.best.metric.unit} /></p><ParameterFacts values={value.best.parameters} copy /></> : <p className="quiet">尚无目标指标可用、口径一致的结果。</p>}</section>
    <div className="scan-notice"><p>本次扫描按 VALIDATION 分区最大化指定指标；计算完成、指标可比较与独立证据通过分别记录。</p><p className="quiet">扫描结果用于调参，不创建独立评估、组合资格或目标交付。费用、输入分区和市场口径继承来源；不在浏览器重算收益。</p></div>
  </>;
}

function ScanTrials({ scan, route, update }: { scan: ParameterScanView; route: Route; update: (patch: Record<string, string | undefined>) => void }) {
  const page = Math.max(1, Number(route.params.get('trialPage')) || 1); const pageSize = 20; const status = route.params.get('trialStatus') ?? ''; const order = route.params.get('trialOrder') ?? 'ordinal';
  const params = new URLSearchParams({ page: String(page), pageSize: String(pageSize), sort: order, order: order === 'metric' ? 'desc' : 'asc' }); if (status) params.set('status', status);
  const path = `${collection}/${encodeURIComponent(scan.id)}/trials?${params}`;
  const query = useResource<ParameterScanTrials>(path, { poll: active(scan) ? 3000 : undefined });
  const ordinal = route.params.get('trial'); const detail = useResource<ParameterScanTrial>(`${collection}/${encodeURIComponent(scan.id)}/trials/${encodeURIComponent(ordinal ?? '')}`, { enabled: ordinal !== null, poll: active(scan) && ordinal !== null ? 3000 : undefined });
  const [lookup, setLookup] = useState(ordinal ?? ''); const [lookupError, setLookupError] = useState(false);
  useEffect(() => { setLookup(ordinal ?? ''); setLookupError(false); }, [ordinal]);
  const compared = scanComparisonOrdinals(route.params.get('compare'));
  return <><div className="scan-toolbar"><label>执行状态<Select aria-label="筛选试验状态" allowClear value={status || undefined} onChange={value => update({ trialStatus: value, trialPage: undefined })} options={['PENDING', 'RUNNING', 'RECONCILING', 'COMPLETED', 'FAILED', 'STOPPED', 'SKIPPED'].map(value => ({ value, label: labels[value] ?? ({ RUNNING: '运行中', FAILED: '失败', STOPPED: '已停止' })[value as 'RUNNING'] ?? value }))} /></label><label>排序<Select aria-label="结果排序" value={order} onChange={value => update({ trialOrder: value, trialPage: undefined })} options={[{ value: 'ordinal', label: '参数序号' }, { value: 'metric', label: '目标验证值，从高到低' }]} /></label>{scan.mode === 'STRATEGY' && <Button onClick={() => update({ tab: 'compare' })}>对比已选曲线（{compared.length}/5）</Button>}</div>
    <p className="quiet">列表展示已创建计算任务的参数点。尚未执行或因约束跳过的点，可在参数分析中查看，或输入原序号读取；读取不启动计算。</p>
    <div className="scan-toolbar"><Input.Search aria-label="按精确参数序号查看" placeholder="参数序号（从 0 开始）" value={lookup} onChange={event => { setLookup(event.target.value); setLookupError(false); }} enterButton="查看参数点" onSearch={value => { if (!/^(0|[1-9]\d*)$/.test(value)) { setLookupError(true); return; } update({ trial: value }); }} /></div>
    {lookupError && <p className="scan-subtle-error" role="alert">请输入从 0 开始的完整非负整数序号。</p>}
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} /><StaleNotice stale={query.isError} at={query.dataUpdatedAt} />
    {scan.grid.axes.length === 1 && query.data && <Suspense fallback={<Loading />}><Trend parameter={parameterLabel(scan.grid.axes[0]!.key)} metric={scan.best?.metric.label ?? scan.objective.metricId} points={[...query.data.items].sort((a, b) => BigInt(a.ordinal) < BigInt(b.ordinal) ? -1 : 1).map(row => ({ ordinal: row.ordinal, x: row.parameters[scan.grid.axes[0]!.key] ?? '参数未记录', value: row.status === 'COMPLETED' && row.metric?.status === 'AVAILABLE' ? row.metric.value : null, reason: row.metric?.reason ?? row.error ?? row.skipReason }))} /></Suspense>}
    <Table<ParameterScanTrial> rowKey="ordinal" size="small" loading={query.isPending} dataSource={query.data?.items ?? []} scroll={{ x: 1100 }} pagination={{ current: query.data?.page ?? page, pageSize, total: query.data?.total ?? 0, showSizeChanger: false, onChange: next => update({ trialPage: String(next) }) }} rowSelection={scan.mode === 'STRATEGY' ? { selectedRowKeys: compared, preserveSelectedRowKeys: true, getCheckboxProps: row => ({ disabled: row.status !== 'COMPLETED' || !compared.includes(row.ordinal) && compared.length >= 5 }), onChange: keys => update({ compare: keys.map(String).join(',') || undefined }) } : undefined} columns={[
      { title: '试验', width: 90, fixed: 'left', render: (_cell, row) => <button className="text-link" onClick={() => update({ trial: row.ordinal })}>#{row.ordinal}</button> },
      { title: '参数', width: 360, render: (_cell, row) => <ParameterFacts values={row.parameters} /> },
      { title: '执行', width: 140, render: (_cell, row) => <ScanStatus status={row.status} /> },
      { title: '目标验证值', width: 150, render: (_cell, row) => <Exact value={row.metric?.value} unit={row.metric?.unit} missing="不可用" /> },
      { title: '指标原因 / 执行错误', width: 230, render: (_cell, row) => row.metric?.reason ?? row.error ?? row.skipReason ?? (row.status === 'COMPLETED' && row.metric?.value == null ? '目标指标未提供' : '—') },
      { title: '远端回执', width: 140, render: (_cell, row) => row.remoteStatus ?? '未记录' },
    ]} locale={{ emptyText: <Empty title="暂无匹配的参数结果" /> }} />
    {ordinal !== null && <section className="result-section"><div className="section-heading"><h3>试验 #{ordinal}</h3><Button size="small" onClick={() => update({ trial: undefined })}>收起</Button></div><ErrorNotice error={detail.error} retry={() => { void detail.refetch(); }} />{detail.isPending ? <Loading /> : detail.data && <><ParameterFacts values={detail.data.parameters} copy /><Facts items={[["执行状态", <ScanStatus status={detail.data.status} />], ["科学任务", detail.data.taskId], ["远端回执", detail.data.remoteStatus], ["约束", detail.data.skipReason]]} /><ExecutionFailure error={detail.data.error} identity={`参数扫描 ${scan.id} / 试验 #${detail.data.ordinal}`} title="试验未完成" /><Metrics metrics={scanValidationMetrics(detail.data.metrics)} />{detail.data.sampleCounts && <details><summary>样本与预热计数</summary><pre className="scan-code">{JSON.stringify(detail.data.sampleCounts, null, 2)}</pre></details>}</>}</section>}
  </>;
}

function ScanSurface({ scan, route, update }: { scan: ParameterScanView; route: Route; update: (patch: Record<string, string | undefined>) => void }) {
  const axes = scan.grid.axes; const x = axes.find(axis => axis.key === route.params.get('x')) ?? axes[0]; const y = axes.find(axis => axis.key === route.params.get('y') && axis.key !== x?.key) ?? axes.find(axis => axis.key !== x?.key);
  const parsedFixed = scanFixedParameters(route.params.get('fixed')); const fixed: ParameterValues = {};
  for (const axis of axes) if (axis.key !== x?.key && axis.key !== y?.key) fixed[axis.key] = parsedFixed?.[axis.key] ?? parameterScanAxisValue(axis, 0n);
  const xOffset = /^(0|[1-9]\d*)$/.test(route.params.get('xOffset') ?? '') ? route.params.get('xOffset')! : '0'; const yOffset = /^(0|[1-9]\d*)$/.test(route.params.get('yOffset') ?? '') ? route.params.get('yOffset')! : '0';
  const params = new URLSearchParams({ x: x?.key ?? '', y: y?.key ?? '', fixed: JSON.stringify(fixed), metricId: scan.objective.metricId, xOffset, yOffset, limit: '30' });
  const query = useResource<ParameterScanSurface>(`${collection}/${encodeURIComponent(scan.id)}/surface?${params}`, { enabled: !!x && !!y && parsedFixed !== undefined, poll: active(scan) ? 4000 : undefined });
  const axisOptions = axes.map(axis => ({ value: axis.key, label: `${parameterLabel(axis.key)} (${axis.count})` }));
  if (!x || !y) return <Empty title="二维参数分析需要至少两个扫描参数" />;
  return <><p className="quiet">展示两个参数的原始格点。其他参数固定为明确取值，不取最佳值、不插值、不聚合。</p><div className="scan-toolbar"><label>横轴<Select value={x.key} options={axisOptions} onChange={value => update({ x: value, y: value === y.key ? x.key : y.key, fixed: undefined, xOffset: undefined, yOffset: undefined })} /></label><label>纵轴<Select value={y.key} options={axisOptions.filter(item => item.value !== x.key)} onChange={value => update({ y: value, fixed: undefined, xOffset: undefined, yOffset: undefined })} /></label></div>
    <div className="scan-toolbar">{axes.filter(axis => axis.key !== x.key && axis.key !== y.key).map(axis => <label key={axis.key}>固定{parameterLabel(axis.key)}<Input aria-label={`固定${parameterLabel(axis.key)}`} key={`${axis.key}:${fixed[axis.key]}`} defaultValue={fixed[axis.key]} onBlur={event => { if (event.target.value !== fixed[axis.key]) update({ fixed: JSON.stringify({ ...fixed, [axis.key]: event.target.value }) }); }} /></label>)}</div>
    {parsedFixed === undefined && <p className="scan-subtle-error" role="status">链接中的固定参数无效。<Button size="small" onClick={() => update({ fixed: undefined })}>恢复原始切片</Button></p>}
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />
    {query.isPending && parsedFixed !== undefined ? <Loading /> : query.data && <><Suspense fallback={<Loading />}><Plots x={{ label: parameterLabel(query.data.x.key), values: query.data.x.values }} y={{ label: parameterLabel(query.data.y.key), values: query.data.y.values }} metric={scan.best?.metric.label ?? scan.objective.metricId} cells={query.data.cells.map(cell => ({ ...cell, value: cell.metric?.value ?? null, reason: cell.reason ?? cell.metric?.reason ?? null }))} onSelect={ordinal => update({ tab: 'trials', trial: ordinal })} /></Suspense>
      <div className="scan-toolbar">{(['x', 'y'] as const).map(direction => { const axis = query.data![direction]; const offset = BigInt(axis.offset); const next = offset + BigInt(axis.values.length); return <div className="scan-command-actions" key={direction}><span>{direction === 'x' ? '横轴' : '纵轴'}：{axis.offset} 起 / 共 {axis.total} 项</span><Button size="small" disabled={offset === 0n} onClick={() => update({ [`${direction}Offset`]: (offset > 30n ? offset - 30n : 0n).toString() })}>上一段</Button><Button size="small" disabled={next >= BigInt(axis.total) || !axis.values.length} onClick={() => update({ [`${direction}Offset`]: next.toString() })}>下一段</Button></div>; })}</div>
      <details><summary>原始切片数据</summary><Table rowKey="ordinal" size="small" pagination={{ pageSize: 20 }} dataSource={query.data.cells} scroll={{ x: 550 }} columns={[{ title: '试验', render: (_cell, row) => <button className="text-link" onClick={() => update({ tab: 'trials', trial: row.ordinal })}>#{row.ordinal}</button> }, { title: parameterLabel(x.key), dataIndex: 'x' }, { title: parameterLabel(y.key), dataIndex: 'y' }, { title: '状态', render: (_cell, row) => <ScanStatus status={row.status} /> }, { title: '目标验证值', render: (_cell, row) => <Exact value={row.metric?.value} missing="不可用" /> }, { title: '原因', render: (_cell, row) => row.reason ?? row.metric?.reason ?? '—' }]} /></details>
    </>}
  </>;
}

function ScanComparison({ scan, route, update }: { scan: ParameterScanView; route: Route; update: (patch: Record<string, string | undefined>) => void }) {
  const ordinals = scanComparisonOrdinals(route.params.get('compare'));
  const results = useQueries({ queries: ordinals.map(ordinal => ({ queryKey: [collection, scan.id, 'trial', ordinal], queryFn: ({ signal }: { signal: AbortSignal }) => api<ParameterScanTrial>(`${collection}/${encodeURIComponent(scan.id)}/trials/${encodeURIComponent(ordinal)}`, { signal }), staleTime: 5000 })) });
  if (scan.mode !== 'STRATEGY') return <Empty title="因子评估没有账户权益曲线" />;
  if (!ordinals.length) return <Empty title="在结果表选择至多五个已完成的策略试验" action={<Button onClick={() => update({ tab: 'trials' })}>选择对比结果</Button>} />;
  const records = results.flatMap(result => result.data ? [result.data] : []); const curves = records.filter(record => record.status === 'COMPLETED').map(record => ({ ordinal: record.ordinal, points: record.curve ?? [] }));
  return <><div className="scan-toolbar">{ordinals.map(ordinal => <Tag key={ordinal} closable onClose={() => update({ compare: ordinals.filter(item => item !== ordinal).join(',') || undefined })}>试验 #{ordinal}</Tag>)}<Button onClick={() => update({ tab: 'trials' })}>更改选择</Button></div>
    <p className="quiet">同一扫描的币种、区间、本金和费用口径保持一致。只读取所选试验已有的原生曲线。</p>
    {results.map((result, index) => <ErrorNotice key={ordinals[index]} error={result.error} retry={() => { void result.refetch(); }} />)}
    {results.some(result => result.isPending) ? <Loading /> : <Suspense fallback={<Loading />}><Curves curves={curves} currency={scan.sourceView.currency ?? '原生报告币种未记录'} /></Suspense>}
    {records.some(record => record.status !== 'COMPLETED') && <p className="quiet">未完成的试验不加入曲线对比。</p>}
    <Table rowKey="ordinal" size="small" pagination={false} dataSource={records} scroll={{ x: 700 }} columns={[{ title: '试验', dataIndex: 'ordinal' }, { title: '参数', render: (_cell, row) => <ParameterFacts values={row.parameters} /> }, { title: '目标验证值', render: (_cell, row) => <Exact value={row.metric?.value} missing="不可用" /> }, { title: '原生曲线记录数', render: (_cell, row) => row.curve?.length ?? '未提供' }]} />
  </>;
}
