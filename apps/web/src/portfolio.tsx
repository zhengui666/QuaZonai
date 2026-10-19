import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Button, Form, Input, Select, Table } from 'antd';
import { ArrowLeftIcon, ArrowRightIcon, ArrowUpRightIcon, CaretDownIcon, ChartLineUpIcon, FlaskIcon, PlusIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { CapitalAssumptions, DatasetView, PortfolioView, ResearchDetail } from '../../../packages/contracts/src/index.ts';
import { decimalUnits, draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource, useStudies } from './data-hooks';
import { currentRoute, href, navigate, type Route } from './router';
import { DraftNotice, Empty, ErrorNotice, Exact, Facts, Loading, MetricLabel, Metrics, PendingCommand, Status, Timestamp } from './ui';
import { RecordTitle, TextDisclosure } from './disclosure';
import { shortTitle } from './titles';
import { ExecutionFailure } from './diagnostics';
import { PortfolioCapacity, CapitalFields, capitalFields, capitalInput, type CapitalFormFields } from './capacity';
import { AtomicPortfolioBuilder } from './portfolio-builder';
import { FactorIdentity } from './factors';
const EquityPlot = lazy(() => import('./equity-plot'));
export default function Portfolios({ route }: { route: Route }) { return route.creating ? (route.params.has('research') ? <PortfolioBuilder route={route} /> : <AtomicPortfolioBuilder route={route} />) : route.id ? <PortfolioDetail id={route.id} /> : <PortfolioList />; }
function PortfolioList() {
  const query = useResource<PortfolioView[]>('/portfolios', { poll: 5000 });
  return <div className="index-page"><header className="page-heading"><h1>组合回测</h1><Button type="primary" icon={<PlusIcon size={15} />} onClick={() => navigate('/portfolios/new')}>创建组合</Button></header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : !query.data?.length ? !query.error && <Empty title="尚无策略组合" action={<a className="text-link" href={href('/research')}>查看研究 <ArrowUpRightIcon size={14} /></a>} /> : <Table<PortfolioView> rowKey="id" dataSource={query.data} pagination={{ pageSize: 20, hideOnSinglePage: true }} scroll={{ x: 700 }} columns={[
    { title: '组合名称', render: (_value, row) => <><a className="record-link" href={href(`/portfolios/${encodeURIComponent(row.id)}`)}>{shortTitle(row.name, undefined, '组合')}</a>{shortTitle(row.name) !== row.name && <TextDisclosure label="完整组合名称" value={row.name} collapsed />}</> }, { title: '状态', render: (_value, row) => <Status plain value={row.status} /> },
    { title: '数据集', dataIndex: 'datasetId' }, { title: <MetricLabel metricKey="members" />, align: 'right', render: (_value, row) => `${row.factors?.length ?? row.researchIds.length}${(row.factors?.length ?? row.researchIds.length) === 1 ? '（单因子配置）' : ''}` }, { title: <MetricLabel metricKey="capital" />, align: 'right', render: (_value, row) => <Exact value={row.capital} unit={row.currency} /> }, { title: '创建时间', render: (_value, row) => <Timestamp value={row.createdAt} /> },
  ]} />}</div>;
}
function PortfolioDetail({ id }: { id: string }) {
  const query = useResource<PortfolioView>(`/portfolios/${encodeURIComponent(id)}`, { poll: 3000 }); const value = query.data;
  return <div className="portfolio-result"><a className="text-link back-link" href={href('/portfolios')}><ArrowLeftIcon size={14} />组合回测</a><article className="study-document"><header className="study-heading"><div className="document-toolbar">{value && <Status value={value.status} />}</div><RecordTitle name={value?.name} fallback="组合回测" /></header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : value && <div className="result-content">
    <ExecutionFailure error={value.error} detail={value.errorDetail} identity={`组合 ${value.id}${value.taskId !== null ? ` / 计算任务 #${value.taskId}` : ''}`} title="回测未完成" />
    <Facts items={[["数据集", value.datasetId], ["本金", <Exact value={value.capital} unit={value.currency} />], ["创建时间", <Timestamp value={value.createdAt} />], ["计算任务", value.taskId ?? '未记录']]} />
    {value.factors && <section className="result-section"><h3>原子因子与权重</h3><Table rowKey="factorId" size="small" pagination={false} dataSource={value.factors} columns={[{ title: '因子来源', render: (_cell, row) => <FactorIdentity id={row.factorId} /> }, { title: <MetricLabel metricKey="weight" />, render: (_cell, row) => <Exact value={row.weight} /> }]} /></section>}
    {!value.factors && <section className="result-section"><h3>研究与权重（历史方式）</h3><Table rowKey="researchId" size="small" pagination={false} dataSource={value.researchIds.map(researchId => ({ researchId, weight: value.weights?.find(item => item.researchId === researchId)?.weight }))} columns={[{ title: '原始研究', render: (_cell, row) => <a href={href(`/research/${row.researchId}`)}>研究 #{row.researchId}</a> }, { title: <MetricLabel metricKey="weight" />, align: 'right', render: (_cell, row) => <Exact value={row.weight} missing="未记录" /> }]} /></section>}
    <PortfolioCapacity key={value.id} portfolio={value} />
    <section className="result-section"><Suspense fallback={<Loading />}><EquityPlot points={value.curve} currency={value.currency} initialCapital={value.capital} /></Suspense></section>
    <section className="result-section"><h3>回测指标</h3><Metrics metrics={value.metrics} /></section>
    <section className="result-section"><div className="section-heading"><h3>目标权重</h3>{value.status === 'COMPUTED' && value.targets.length > 0 && (value.targetExpired ? <span className="quiet">目标已过期</span> : <Button onClick={() => navigate(`/observations/new?portfolio=${encodeURIComponent(value.id)}`)}>交付目标 <ArrowUpRightIcon size={14} /></Button>)}</div>{value.targetExpiresAt && <p className="quiet expiry-fact">到期时间 <Timestamp value={value.targetExpiresAt} /></p>}{value.targets.length ? <Table rowKey="instrumentId" pagination={false} size="small" dataSource={value.targets} columns={[{ title: '标的', dataIndex: 'instrumentId' }, { title: <MetricLabel metricKey="weight" label="目标权重" />, align: 'right', render: (_cell, row) => <Exact value={row.weight} /> }]} /> : <p className="quiet">尚无目标</p>}</section>
  </div>}</article></div>;
}
type PortfolioFields = { name: string; datasetId: string; researchIds: number[]; capital: string; weights: Record<string, string> } & CapitalFormFields;
function PortfolioBuilder({ route }: { route: Route }) {
  const cache = useQueryClient(); const studies = useStudies(); const datasets = useResource<DatasetView[]>('/datasets');
  const initialId = Number(route.params.get('research')); const hasInitial = route.params.has('research') && Number.isSafeInteger(initialId) && initialId >= 0;
  const origin = hasInitial ? { path: `/research/${initialId}`, label: '原始研究' } : { path: '/portfolios', label: '组合回测' };
  const initialStudy = useResource<ResearchDetail>(`/research/${initialId}`, { enabled: hasInitial });
  const [form] = Form.useForm<PortfolioFields>(); const initialized = useRef(false); const [allocationError, setAllocationError] = useState<string>();
  const [draft] = useState(() => readDraft<PortfolioFields>('portfolio')); const [draftSaved, setDraftSaved] = useState(() => draftIsPersisted('portfolio'));
  const command = useCommand<PortfolioView>('portfolios', value => { removeDraft('portfolio'); void cache.invalidateQueries({ queryKey: ['/portfolios'] }); if (currentRoute().page === 'portfolios' && currentRoute().creating) navigate(`/portfolios/${encodeURIComponent(value.id)}`); });
  const ids = Form.useWatch('researchIds', form) as number[] | undefined;
  const weights = Form.useWatch('weights', form) as Record<string, string> | undefined;
  const datasetId = Form.useWatch('datasetId', form) as string | undefined;
  const rows = new Map<number, { id: number; title: string; status: string; datasetId?: string | null }>(studies.items.map(item => [item.id, item])); if (initialStudy.data) rows.set(initialStudy.data.id, initialStudy.data);
  for (const id of ids ?? []) if (!rows.has(id)) rows.set(id, { id, title: `研究 #${id}（未加载）`, status: 'UNKNOWN' });
  useEffect(() => {
    if (initialized.current || !datasets.data || studies.isPending || hasInitial && !initialStudy.data) return;
    initialized.current = true;
    if (command.values) { const pairs = command.values.weights as { researchId: number; weight: string }[] | undefined; form.setFieldsValue({ ...command.values, ...capitalFields(command.values.capacityPolicy as CapitalAssumptions | undefined), weights: Object.fromEntries(pairs?.map(pair => [String(pair.researchId), pair.weight]) ?? []) } as PortfolioFields); }
    else {
      const researchIds = [...(draft?.researchIds ?? [])]; const allocations = { ...draft?.weights };
      if (hasInitial && !researchIds.includes(initialId)) { researchIds.push(initialId); allocations[String(initialId)] = researchIds.length === 1 ? '1' : ''; }
      form.setFieldsValue({ ...capitalFields(initialStudy.data?.input.specification?.capital), ...(!draft?.datasetId && initialStudy.data?.datasetId ? { datasetId: initialStudy.data.datasetId } : {}), ...draft, researchIds, weights: allocations });
    }
  }, [datasets.data, studies.isPending, initialStudy.data, initialStudy.isPending]);
  const values = (ids ?? []).map(id => decimalUnits(weights?.[String(id)] ?? ''));
  const sum = values.length && values.every((value): value is bigint => value !== undefined && value > 0n) ? values.reduce((a, b) => a + b, 0n) : undefined;
  const ready = datasets.data?.find(item => item.id === datasetId);
  return <div className="authoring-page"><a className="text-link back-link" href={href(origin.path)}><ArrowLeftIcon size={14} />{origin.label}</a><div className="authoring-heading"><div className="authoring-symbol"><ChartLineUpIcon size={24} weight="duotone" /></div><h1>创建组合</h1></div>
    <ErrorNotice error={datasets.error ?? studies.error ?? initialStudy.error} retry={() => { void datasets.refetch(); void studies.refetch(); if (hasInitial) void initialStudy.refetch(); }} />
    {datasets.isPending || studies.isPending || hasInitial && initialStudy.isPending ? <Loading /> : datasets.data && (!hasInitial || initialStudy.data) && <Form form={form} layout="vertical" validateTrigger="onBlur" scrollToFirstError={{ focus: true }} disabled={!!command.pending || command.busy} onValuesChange={(_changed, value) => { setAllocationError(undefined); if (!command.pending) setDraftSaved(saveDraft('portfolio', value)); }} onFinish={value => {
      if (sum !== 1_000_000_000_000_000_000n) { setAllocationError('权重合计须为 1'); if (value.researchIds[0] !== undefined) form.scrollToField(['weights', String(value.researchIds[0])], { focus: true }); return; }
      try { const capacityPolicy = value.participation || value.maximumAgeSeconds || value.scenarios ? capitalInput(value, value.capital) : undefined;
        void command.submit({ name: value.name, datasetId: value.datasetId, researchIds: value.researchIds, capital: value.capital, weights: value.researchIds.map(researchId => ({ researchId, weight: value.weights[String(researchId)] })), ...(capacityPolicy ? { capacityPolicy } : {}) });
      } catch { setAllocationError('请填写精确资本情景、成交量参与率与正整数数据时差'); }
    }}>
      <PendingCommand command={command} />
      <Form.Item label="组合名称" name="name" rules={[{ required: true, whitespace: true, message: '请输入组合名称' }]}><Input autoFocus /></Form.Item>
      <Form.Item label="数据集" name="datasetId" rules={[{ required: true, message: '请选择数据集' }]}><Select suffixIcon={<CaretDownIcon size={14} />} options={datasets.data?.map(item => ({ value: item.id, label: `${item.name} / ${item.currency}`, disabled: !item.ready }))} /></Form.Item>
      <div className="form-section-heading"><FlaskIcon size={16} /><h2>研究与权重</h2></div>
      <Form.Item label="原始研究" name="researchIds" rules={[{ required: true, type: 'array', min: 1, message: '请选择有独立结果的研究' }]}><Select mode="multiple" showSearch optionFilterProp="label" suffixIcon={<CaretDownIcon size={14} />} options={[...rows.values()].map(item => ({ value: item.id, label: `#${item.id} / ${item.title}`, title: item.title, disabled: item.status !== 'COMPUTED' }))} onChange={(selected: number[]) => { if (selected.length === 1 && !form.getFieldValue(['weights', String(selected[0])])) form.setFieldValue(['weights', String(selected[0])], '1'); setDraftSaved(saveDraft('portfolio', { ...form.getFieldsValue(), weights: form.getFieldValue('weights') })); }} /></Form.Item>
      {studies.hasNextPage && <Button type="text" disabled={false} loading={studies.isFetchingNextPage} onClick={() => { void studies.fetchNextPage(); }}>加载更多研究</Button>}
      {(ids ?? []).map(id => <div className="member-row" key={id}><div><span className="selected-mark">所选研究</span><h3>{shortTitle(rows.get(id)?.title, undefined, `研究 #${id}`)}</h3>{rows.get(id)?.title && shortTitle(rows.get(id)?.title) !== rows.get(id)?.title && <TextDisclosure label="完整研究名称" value={rows.get(id)?.title} collapsed />}<div className="member-origin"><code>#{id}</code><Status plain value={rows.get(id)?.status ?? 'UNKNOWN'} /><span>{rows.get(id)?.datasetId ?? '数据身份未记录'}</span></div></div><Form.Item label={`研究 ${id} 权重`} name={['weights', String(id)]} rules={[{ required: true, message: '请输入权重' }, { validator: (_rule, value: string) => !value || (decimalUnits(value) ?? 0n) > 0n ? Promise.resolve() : Promise.reject(new Error('请输入正数，最多 18 位小数')) }]}><Input inputMode="decimal" /></Form.Item></div>)}
      {(ids?.length ?? 0) > 0 && <p className="allocation-total" role={allocationError ? 'alert' : 'status'}>合计 <code>{sum === undefined ? '未完整填写' : `${sum / 1_000_000_000_000_000_000n}.${(sum % 1_000_000_000_000_000_000n).toString().padStart(18, '0')}`.replace(/\.?0+$/, '')}</code>{allocationError && <span className="error-text">{allocationError}</span>}</p>}
      <Form.Item label="本金" name="capital" rules={[{ required: true, message: '请输入本金' }, { validator: (_rule, value: string) => !value || (decimalUnits(value) ?? 0n) > 0n ? Promise.resolve() : Promise.reject(new Error('请输入正数，最多 18 位小数')) }]}><Input inputMode="decimal" suffix={<span className="input-unit">{ready?.currency ?? ''}</span>} /></Form.Item>
      <details className="provenance-disclosure"><summary>条件容量假设（可选）</summary><CapitalFields /><p className="quiet">本次明确假设适用于整个原始组合；均留空时仅继承一致的原研究假设。</p></details>
      <ErrorNotice error={command.error} /><DraftNotice saved={draftSaved || !!command.pending} /><div className="form-actions"><Button disabled={false} onClick={() => navigate(origin.path)}>取消</Button>{command.pending ? <Button type="primary" disabled={command.busy} aria-busy={command.busy} loading={command.busy} onClick={() => { void command.submit(); }}>{command.busy ? '提交中' : '核对提交'}</Button> : <Button type="primary" htmlType="submit">运行回测 <ArrowRightIcon size={15} /></Button>}</div>
    </Form>}
  </div>;
}
