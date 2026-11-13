import { useEffect, useState } from 'react';
import { Button, Input, Select, Table, Tabs, Tag } from 'antd';
import { ArrowLeftIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { ParameterScanTrial, ParameterScanTrials, ParameterScanView, ParameterValues } from '../../../packages/contracts/src/parameter-scans.ts';
import { api } from './api';
import { useParameterScanDetail, useResource } from './data-hooks';
import { ExecutionFailure } from './diagnostics';
import { RecordTitle } from './disclosure';
import { ListTools, useListWorkspace } from './list-workspace';
import { scanNativeMetrics } from './parameter-scan-values';
import { href, navigate, type Route } from './router';
import { Copy, Empty, ErrorNotice, Exact, Facts, Loading, Metrics, Status, Timestamp } from './ui';
import './parameter-scans.css';

const collection = '/parameter-scans';
const active = (value: ParameterScanView) => ['QUEUED', 'RUNNING', 'RECONCILING', 'STOPPING'].includes(value.status);
const labels: Record<string, string> = { STRATEGY: '策略账户回测', COMPLETED: '计算完成', STOPPING: '停止中', RECONCILING: '远端回执待核对', SKIPPED: '约束跳过', PENDING: '尚未执行' };
const parameterLabel = (key: string) => key;
function ScanStatus({ status }: { status: string }) { return <Status value={status} label={labels[status]} />; }
function ParameterFacts({ values, copy = false }: { values: ParameterValues; copy?: boolean }) { return <div className="scan-parameter-list">{Object.entries(values).map(([key, value]) => <span key={key}>{parameterLabel(key)} <Exact value={value} /></span>)}{copy && <Copy value={JSON.stringify(values, null, 2)} label="复制参数" />}</div>; }
function StaleNotice({ stale, at }: { stale: boolean; at: number }) { return stale && at ? <p className="quiet" role="status">连接未更新，下方保留上次读取记录：<Timestamp value={new Date(at).toISOString()} /></p> : null; }

export default function ParameterScans({ route }: { route: Route }) {
  if (route.creating) return <Empty title="自动参数扫描已退役" action={<a href={href(collection)}>查看历史记录</a>} />;
  return route.id ? <ScanDetail key={route.id} route={route} id={route.id} /> : <ScanList route={route} />;
}

function ScanList({ route }: { route: Route }) {
  const list = useListWorkspace<ParameterScanView>(route, collection, 'createdAt');
  return <section className="parameter-scans list-workspace" aria-label="参数扫描列表">
    <header className="list-heading"><h1>历史参数扫描</h1></header>
    <p className="quiet">仅查看原始历史记录及停止未决执行，不再创建参数网格或计算排名。</p>
    <ListTools list={list} searchLabel="搜索参数扫描" placeholder="搜索扫描名称、来源或数据集" filters={[{ key: 'status', label: '执行状态' }, { key: 'mode', label: '扫描类型', options: [{ value: 'STRATEGY', label: '策略账户回测' }] }, { key: 'datasetId', label: '数据集' }]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} /><StaleNotice stale={list.query.isError} at={list.query.dataUpdatedAt} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<ParameterScanView> {...list.tableProps} rowKey="id" size="small" scroll={{ x: 1150 }} locale={{ emptyText: <Empty title={list.hasFilters ? '没有匹配的参数扫描' : '尚无参数扫描'} /> }} columns={[
      { title: '扫描名称', key: 'name', width: 220, fixed: 'left', ...list.sortColumn('name'), render: (_cell, row) => <a className="list-record-name" {...list.linkProps(row.id)}>{row.name}</a> },
      { title: '类型', width: 135, render: (_cell, row) => labels[row.mode] },
      { title: '执行状态', width: 150, render: (_cell, row) => <ScanStatus status={row.status} /> },
      { title: '数据集', width: 180, render: (_cell, row) => row.sourceView.datasetName ?? row.sourceView.datasetId ?? '未记录' },
      { title: '已完成 / 可执行', width: 160, render: (_cell, row) => <><Exact value={row.counts.completed} /> / <Exact value={row.counts.eligible} /></> },
      { title: '失败 / 跳过', width: 125, render: (_cell, row) => <><Exact value={row.counts.failed} /> / <Exact value={row.counts.skipped} /></> },
      { title: '创建时间', key: 'createdAt', width: 200, ...list.sortColumn('createdAt'), render: (_cell, row) => <Timestamp value={row.createdAt} /> },
    ]} />}
  </section>;
}

function ScanDetail({ route, id }: { route: Route; id: string }) {
  const cache = useQueryClient(); const query = useParameterScanDetail(id); const value = query.data;
  const [stopping, setStopping] = useState(false); const [error, setError] = useState<unknown>();
  const requestedTab = route.params.get('tab'); const tab = requestedTab && ['overview', 'trials', 'inputs'].includes(requestedTab) ? requestedTab : 'overview';
  const update = (patch: Record<string, string | undefined>) => { const params = new URLSearchParams(route.params); for (const [key, item] of Object.entries(patch)) { if (item === undefined) params.delete(key); else params.set(key, item); } navigate(`${collection}/${encodeURIComponent(id)}?${params}`, true); };
  const stop = async () => { setStopping(true); setError(undefined); try { const result = await api<ParameterScanView>(`${collection}/${encodeURIComponent(id)}/stop`, { method: 'POST' }); cache.setQueryData([`${collection}/${encodeURIComponent(id)}`], result); void cache.invalidateQueries({ queryKey: [collection] }); } catch (failure) { setError(failure); } finally { setStopping(false); } };
  return <div className="parameter-scans"><a className="text-link back-link" href={href(collection)}><ArrowLeftIcon size={14} />参数扫描列表</a><article className="study-document"><header className="study-heading"><div className="document-toolbar">{value && <><ScanStatus status={value.status} /><div className="scan-command-actions"><Button onClick={() => { void query.refetch(); }}>刷新</Button>{active(value) && <Button danger disabled={stopping || value.status === 'STOPPING'} loading={stopping} onClick={() => { void stop(); }}>{value.status === 'STOPPING' ? '停止中' : '停止扫描'}</Button>}</div></>}</div><RecordTitle name={value?.name} fallback="参数扫描" />{value && <div className="study-context"><span>{labels[value.mode]}</span><span>{value.sourceView.datasetName ?? value.sourceView.datasetId}</span><Tag>探索性结果，未获科学资格</Tag></div>}</header>
    <ErrorNotice error={query.error ?? error} retry={() => { void query.refetch(); }} /><StaleNotice stale={query.isError} at={query.dataUpdatedAt} />
    {query.isPending ? <Loading /> : value && <Tabs className="document-tabs" destroyOnHidden activeKey={tab} onChange={next => update({ tab: next })} items={[
      { key: 'overview', label: '概览', children: <div className="scan-detail-content"><ScanOverview value={value} /><Button onClick={() => update({ tab: 'trials' })}>查看全部参数结果</Button></div> },
      { key: 'trials', label: '结果表', children: <div className="scan-detail-content"><ScanTrials scan={value} route={route} update={update} /></div> },
      { key: 'inputs', label: '输入与执行', children: <div className="scan-detail-content"><Facts items={[["扫描", value.id], ["原请求", value.requestId], ["主任务", value.taskId], ["来源", value.sourceView.label], ["来源研究", value.sourceView.researchId], ["创建时间", <Timestamp value={value.createdAt} />], ["更新时间", <Timestamp value={value.updatedAt} />], ["并发任务", value.execution.concurrency]]} />{value.sourceView.assumptions.length > 0 && <section className="result-section"><h3>原始假设</h3><ul>{value.sourceView.assumptions.map((item, index) => <li key={index}>{item}</li>)}</ul></section>}<details><summary>原始参数计划</summary><Copy value={JSON.stringify(value.input, null, 2)} label="复制扫描计划" /><pre className="scan-code">{JSON.stringify(value.input, null, 2)}</pre></details><details><summary>参数组合与固定值</summary><pre className="scan-code">{JSON.stringify(value.grid, null, 2)}</pre></details></div> },
    ]} />}
  </article></div>;
}

function ScanOverview({ value }: { value: ParameterScanView }) {
  const rows: [string, string][] = [['全部组合', value.counts.total], ['可执行', value.counts.eligible], ['已完成', value.counts.completed], ['可比较', value.counts.comparable], ['运行中', value.counts.running], ['尚未执行', value.counts.pending], ['失败', value.counts.failed], ['约束跳过', value.counts.skipped], ['已停止', value.counts.stopped], ['远端待核对', value.counts.unknown]];
  return <><dl className="scan-summary">{rows.map(([label, count]) => <div key={label}><dt>{label}</dt><dd><Exact value={count} /></dd></div>)}</dl>
    {active(value) && <div className="scan-notice" role="status"><p>{value.status === 'STOPPING' ? '停止请求已记录；等待当前原生执行结束及远端回执核对。' : value.status === 'RECONCILING' ? '原执行结果尚不确定，正在核对同一远端身份。' : '进度来自已记录的参数任务；关闭页面不会取消扫描。'}</p><p className="quiet">更新于 <Timestamp value={value.updatedAt} />。本地任务停止不代表远端已停止。</p></div>}
    <ExecutionFailure error={value.error} identity={`参数扫描 ${value.id} / 主任务 #${value.taskId}`} title="扫描未完整完成" />
    <p className="quiet">历史探索记录，不重新计算排名，不代表独立证据通过。</p>
  </>;
}

function ScanTrials({ scan, route, update }: { scan: ParameterScanView; route: Route; update: (patch: Record<string, string | undefined>) => void }) {
  const page = Math.max(1, Number(route.params.get('trialPage')) || 1); const pageSize = 20; const status = route.params.get('trialStatus') ?? '';
  const params = new URLSearchParams({ page: String(page), pageSize: String(pageSize), sort: 'ordinal', order: 'asc' }); if (status) params.set('status', status);
  const path = `${collection}/${encodeURIComponent(scan.id)}/trials?${params}`;
  const query = useResource<ParameterScanTrials>(path, { poll: active(scan) ? 3000 : undefined });
  const ordinal = route.params.get('trial'); const detail = useResource<ParameterScanTrial>(`${collection}/${encodeURIComponent(scan.id)}/trials/${encodeURIComponent(ordinal ?? '')}`, { enabled: ordinal !== null, poll: active(scan) && ordinal !== null ? 3000 : undefined });
  const [lookup, setLookup] = useState(ordinal ?? ''); const [lookupError, setLookupError] = useState(false);
  useEffect(() => { setLookup(ordinal ?? ''); setLookupError(false); }, [ordinal]);
  return <><div className="scan-toolbar"><label>执行状态<Select aria-label="筛选试验状态" allowClear value={status || undefined} onChange={value => update({ trialStatus: value, trialPage: undefined })} options={['PENDING', 'RUNNING', 'RECONCILING', 'COMPLETED', 'FAILED', 'STOPPED', 'SKIPPED'].map(value => ({ value, label: labels[value] ?? ({ RUNNING: '运行中', FAILED: '失败', STOPPED: '已停止' })[value as 'RUNNING'] ?? value }))} /></label></div>
    <p className="quiet">列表展示已创建计算任务的参数点。仅可读取已保存的原序号，不重新展开参数组合；读取不启动计算。</p>
    <div className="scan-toolbar"><Input.Search aria-label="按精确参数序号查看" placeholder="参数序号（从 0 开始）" value={lookup} onChange={event => { setLookup(event.target.value); setLookupError(false); }} enterButton="查看参数点" onSearch={value => { if (!/^(0|[1-9]\d*)$/.test(value)) { setLookupError(true); return; } update({ trial: value }); }} /></div>
    {lookupError && <p className="scan-subtle-error" role="alert">请输入从 0 开始的完整非负整数序号。</p>}
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} /><StaleNotice stale={query.isError} at={query.dataUpdatedAt} />
    <Table<ParameterScanTrial> rowKey="ordinal" size="small" loading={query.isPending} dataSource={query.data?.items ?? []} scroll={{ x: 1100 }} pagination={{ current: query.data?.page ?? page, pageSize, total: query.data?.total ?? 0, showSizeChanger: false, onChange: next => update({ trialPage: String(next) }) }} columns={[
      { title: '试验', width: 90, fixed: 'left', render: (_cell, row) => <button className="text-link" onClick={() => update({ trial: row.ordinal })}>#{row.ordinal}</button> },
      { title: '参数', width: 360, render: (_cell, row) => <ParameterFacts values={row.parameters} /> },
      { title: '执行', width: 140, render: (_cell, row) => <ScanStatus status={row.status} /> },
      { title: '目标原生统计值', width: 150, render: (_cell, row) => <Exact value={row.metric?.value} unit={row.metric?.unit} missing="不可用" /> },
      { title: '指标原因 / 执行错误', width: 230, render: (_cell, row) => row.metric?.reason ?? row.error ?? row.skipReason ?? (row.status === 'COMPLETED' && row.metric?.value == null ? '目标指标未提供' : '—') },
      { title: '远端回执', width: 140, render: (_cell, row) => row.remoteStatus ?? '未记录' },
    ]} locale={{ emptyText: <Empty title="暂无匹配的参数结果" /> }} />
    {ordinal !== null && <section className="result-section"><div className="section-heading"><h3>试验 #{ordinal}</h3><Button size="small" onClick={() => update({ trial: undefined })}>收起</Button></div><ErrorNotice error={detail.error} retry={() => { void detail.refetch(); }} />{detail.isPending ? <Loading /> : detail.data && <><ParameterFacts values={detail.data.parameters} copy /><Facts items={[["执行状态", <ScanStatus status={detail.data.status} />], ["科学任务", detail.data.taskId], ["远端回执", detail.data.remoteStatus], ["约束", detail.data.skipReason]]} /><ExecutionFailure error={detail.data.error} identity={`参数扫描 ${scan.id} / 试验 #${detail.data.ordinal}`} title="试验未完成" /><Metrics metrics={scanNativeMetrics(detail.data.metrics)} />{detail.data.details?.equityPerformance && <details><summary>官方日权益收益与年化口径</summary><p>252/365是年化频率，不是观测天数；短样本年化是外推，非实现的一年收益。不可用时不替换为持仓收益。</p><pre className="scan-code">{JSON.stringify(detail.data.details.equityPerformance, null, 2)}</pre></details>}{detail.data.details?.instrumentDefinition && <details><summary>原始合约规格依据</summary><pre className="scan-code">{JSON.stringify(detail.data.details.instrumentDefinition, null, 2)}</pre></details>}{detail.data.details?.curveReason === 'native_equity_pages_not_loaded' && <p role="note">权益快照已按页保存，当前曲线未加载，并非缺少权益数据。可由研究工具读取原任务的分页证据；此页面不展开整份大表。</p>}{detail.data.sampleCounts && <details><summary>样本与预热计数</summary><pre className="scan-code">{JSON.stringify(detail.data.sampleCounts, null, 2)}</pre></details>}</>}</section>}
  </>;
}
