import { Button, Table } from 'antd';
import { ArrowLeftIcon, ArrowRightIcon } from '@phosphor-icons/react';
import type { CapacityReport, DatasetView, FactorView } from '../../../packages/contracts/src/index.ts';
import type { FactorListItem } from '../../../packages/contracts/src/lists.ts';
import { useResource } from './data-hooks';
import { href, navigate, type Route } from './router';
import { Empty, ErrorNotice, Facts, Loading, MetricLabel, Metrics, Status, stateLabel } from './ui';
import { FactorCapacity } from './capacity';
import { Disclosure, RecordTitle, TextDisclosure } from './disclosure';
import { shortTitle } from './titles';
import { Reason } from './diagnostics';
import { DeleteRecord } from './delete-record';
import { ReviewReportPanel } from './review-report';
import { ListTools, listPathFor, useListWorkspace, type ListFilter } from './list-workspace';
export function FactorIdentity({ id }: { id: string }) {
  const query = useResource<FactorView>(`/factors/${encodeURIComponent(id)}`);
  return <div className="trial-name"><a href={href(`/factors/${encodeURIComponent(id)}`)}>{shortTitle(query.data?.name, query.data?.hypothesis, '因子名称待取得')}</a>{query.data?.name && shortTitle(query.data.name) !== query.data.name && <TextDisclosure label="完整因子名称" value={query.data.name} collapsed />}<TextDisclosure label="研究假设" value={query.data?.hypothesis} />{query.data && <a href={href(`/research/${query.data.researchId}?tab=trials&trial=${query.data.trialId}`)}>原研究 #{query.data.researchId} / 试验 #{query.data.trialId}</a>}<code>{id}</code></div>;
}
const evidence: Record<FactorView['evidence'], string> = { independent: '原单因子独立评估', historical: '历史重新评估', component_only: '仅组合组件证据', validation_only: '仅验证分区', unavailable: '无可用证据' };
const evidenceLabel = (factor: FactorView) => `${evidence[factor.evidence]}${factor.nativeModality === 'TRADE_TICK' ? ' / 逐笔探索，PIT 未验证' : ''}`;
export default function Factors({ route }: { route: Route }) {
  return route.id ? <FactorDetail route={route} /> : <FactorLibrary route={route} />;
}
function FactorDetail({ route }: { route: Route }) {
  const datasets = useResource<DatasetView[]>('/datasets');
  const datasetName = (id: string | null) => datasets.data?.find(dataset => dataset.id === id)?.name ?? (datasets.isPending && id ? '数据集名称读取中…' : '数据集名称未记录');
  const detail = useResource<FactorView>(`/factors/${encodeURIComponent(route.id ?? '')}`, { enabled: !!route.id });
  const capacity = useResource<CapacityReport>(`/factors/${encodeURIComponent(route.id ?? '')}/capacity`, { enabled: !!route.id });
  const back = listPathFor(route);
  return <div className="portfolio-result"><a className="text-link back-link" href={href(back)}><ArrowLeftIcon size={14} />因子库</a><article className="study-document"><header className="study-heading"><div className="document-toolbar">{detail.data && <DeleteRecord key={detail.data.id} kind="factor" id={detail.data.id} back={back} />}</div><RecordTitle name={detail.data?.name} text={detail.data?.hypothesis} fallback="原子因子" /></header><ErrorNotice error={detail.error ?? datasets.error} retry={() => { void detail.refetch(); void datasets.refetch(); }} />{detail.isPending ? <Loading /> : detail.data && <div className="result-content"><Facts items={[["证据来源", evidenceLabel(detail.data)], ["可组合状态", detail.data.combinable ? '可使用原独立证据' : <Reason value={detail.data.reason} fallback="当前没有可用的原单因子独立证据。" />]]} /><TextDisclosure label="完整研究假设" value={detail.data.hypothesis} />
    <Disclosure title="原始身份与数据来源"><Facts items={[["原子身份", detail.data.id], ["原研究 / 试验 / 成员", <a href={href(`/research/${detail.data.researchId}?tab=trials&trial=${detail.data.trialId}`)}>#{detail.data.researchId} / #{detail.data.trialId} / {detail.data.memberIndex}</a>], ["数据集", datasetName(detail.data.datasetId)]]} /></Disclosure>
    <ReviewReportPanel report={detail.data.review} />
    <ErrorNotice error={capacity.error} retry={() => { void capacity.refetch(); }} />{capacity.isPending ? <Loading /> : <FactorCapacity key={detail.data.id} factor={detail.data} report={capacity.data} />}<section className="result-section"><h3>原始指标与分区</h3><Metrics metrics={detail.data.metrics} /></section>
    <details className="provenance-disclosure"><summary>原始模型来源</summary><code className="model-source">{detail.data.modelSource}</code></details><div className="form-actions"><Button type="primary" disabled={!detail.data.combinable} onClick={() => navigate(`/portfolios/new?factor=${encodeURIComponent(detail.data!.id)}`)}>加入组合 <ArrowRightIcon size={14} /></Button></div></div>}</article></div>;
}

const shortEvidence: Record<FactorView['evidence'], string> = { independent: '独立评估', historical: '历史重评', component_only: '组合组件', validation_only: '仅验证分区', unavailable: '无可用证据' };
const reviewLabels: Record<string, string> = { PASS: '通过评审', REJECT: '不予放行', INCONCLUSIVE: '证据不足', unknown: '未评审' };
function FactorLibrary({ route }: { route: Route }) {
  const list = useListWorkspace<FactorListItem>(route, '/factors');
  const filters: ListFilter[] = [
    { key: 'combinable', label: '可组合状态', options: [{ value: 'true', label: '可组合' }, { value: 'false', label: '不可组合' }] },
    { key: 'evidence', label: '证据类型', options: Object.entries(evidence).map(([value, label]) => ({ value, label })) },
    { key: 'datasetId', label: '数据集' },
    { key: 'research', label: '来源研究', advanced: true },
    { key: 'reviewDecision', label: '评审结论', advanced: true, options: Object.entries(reviewLabels).map(([value, label]) => ({ value, label })) },
    { key: 'status', label: '试验状态', advanced: true, options: list.data?.facets.status?.map(option => ({ ...option, label: stateLabel(option.value) })) },
    { key: 'modality', label: '数据形态', advanced: true, options: [{ value: 'BAR', label: 'K 线' }, { value: 'TRADE_TICK', label: '逐笔探索' }, { value: 'unknown', label: '未记录' }] },
    { key: 'icMin', label: '独立 IC 下限', kind: 'number', min: -1, max: 1, advanced: true },
    { key: 'icMax', label: '独立 IC 上限', kind: 'number', min: -1, max: 1, advanced: true },
  ];
  return <div className="index-page"><header className="page-heading"><h1>因子库</h1><Button onClick={() => navigate('/portfolios/new')}>创建组合</Button></header>
    <ListTools list={list} searchLabel="搜索因子" placeholder="搜索因子名称、研究假设、来源研究或数据集" filters={filters} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} />
    {list.query.isPending ? <Loading /> : !list.items.length ? !list.query.error && <Empty title={list.hasFilters ? '尚无匹配原子因子' : '尚无原子因子'} action={list.hasFilters ? <Button onClick={list.clear}>清除筛选</Button> : <a className="text-link" href={href('/research')}>查看研究</a>} /> :
      <Table<FactorListItem> {...list.tableProps} rowKey="id" dataSource={list.items} scroll={{ x: 1100 }} columns={[
        { title: '因子名称', key: 'name', width: 330, ...list.sortColumn('name'), render: (_value, row) => <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}><a {...list.linkProps(row.id)} style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', minWidth: 0 }}>{shortTitle(row.name, null, '原子因子')}</a><span className="quiet" style={{ flexShrink: 0, fontSize: 12 }}>试验 {row.trialId} / 分量 {row.memberIndex + 1}</span></div> },
        { title: '数据集', key: 'datasetName', width: 180, ellipsis: true, ...list.sortColumn('datasetName'), render: (_value, row) => row.datasetName ?? '数据集名称未记录' },
        { title: '证据类型', key: 'evidence', width: 150, render: (_value, row) => <span>{shortEvidence[row.evidence]}{row.nativeModality === 'TRADE_TICK' && <span className="quiet"> · 逐笔探索</span>}</span> },
        { title: <MetricLabel metricKey="HELDOUT.pearson_ic" label="独立样本 IC" />, key: 'independentIc', width: 150, align: 'right', ...list.sortColumn('independentIc'), render: (_value, row) => row.independentIc == null ? <span className="quiet" title="未提供可用的原单因子独立样本 IC">—</span> : <span className="exact-value" title={`原始值：${row.independentIc}`}>{Number(row.independentIc.toPrecision(4)).toString()}</span> },
        { title: '可组合状态', key: 'combinable', width: 110, render: (_value, row) => <Status plain value={row.combinable ? 'READY' : 'NOT_READY'} label={row.combinable ? '可组合' : '不可组合'} /> },
        { title: '评审结论', key: 'reviewDecision', width: 120, render: (_value, row) => <Status plain value={row.reviewDecision === 'REJECT' ? 'REJECTED' : row.reviewDecision === 'INCONCLUSIVE' ? 'UNCERTAIN' : row.reviewDecision === 'PASS' ? 'READY' : 'PENDING'} label={reviewLabels[row.reviewDecision ?? 'unknown'] ?? '未知结论'} /> },
        { title: '操作', key: 'actions', width: 110, render: (_value, row) => <Button type="text" disabled={!row.combinable} onClick={() => navigate(`/portfolios/new?factor=${encodeURIComponent(row.id)}`)}>加入组合</Button> },
      ]} />}
  </div>;
}
