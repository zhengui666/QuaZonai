import { lazy, Suspense } from 'react';
import { Button, Progress, Spin, Table } from 'antd';
import { ArrowLeftIcon } from '@phosphor-icons/react';
import { useQuery } from '@tanstack/react-query';
import type { TickEconomicReplayDetail, TickEconomicReplaySummary } from '../../../packages/contracts/src/economic-tick.ts';
import { api } from './api';
import type { TickReplayListItem } from '../../../packages/contracts/src/lists.ts';
import { href, type Route } from './router';
import { ListTools, listPathFor, useListWorkspace } from './list-workspace';
import { shortTitle } from './titles';
import { FactorIdentity } from './factors';
import { Empty, ErrorNotice, Exact, Facts, Loading, Metrics, Status, TableSurface, Timestamp } from './ui';
import { partitionLabel } from './metric-glossary';
import { replayCurve, replayDayTime, replayMetrics, replayPhasePercentage } from './tick-economic-values';

const EquityPlot = lazy(() => import('./equity-plot'));
const collection = '/tick-economic-replays';
const active = (value: TickEconomicReplaySummary) => value.status === 'QUEUED' || value.status === 'RUNNING';
const phases: Record<string, string> = { QUEUED: '等待执行', compute: '经济回放计算', PREPARING_MODELS: '准备原始模型', REPLAYING: '原生经济回放', RECONCILING: '核对原执行回执' };
const regimes = { BULL: '上涨市场', BEAR: '下跌市场', RANGE: '区间市场', UNKNOWN: '市场状态未知' };

function Qualification({ value }: { value: string }) {
  return <span className="tick-replay-qualification">{value === 'UNQUALIFIED_EXPLORATORY' && <span>探索性结果，未获科学资格</span>}<code>{value}</code></span>;
}
function Members({ replay }: { replay: TickEconomicReplaySummary }) {
  return replay.members ? <ul className="tick-replay-members">{replay.members.map(member => <li key={member.factorId}><FactorIdentity id={member.factorId} /><span>权重 <Exact value={member.weight} /></span></li>)}</ul> : replay.factorId ? <FactorIdentity id={replay.factorId} /> : <span>未记录</span>;
}
export function TickReplayProgress({ replay }: { replay: TickEconomicReplaySummary }) {
  if (!active(replay)) return null;
  const progress = replay.progress; const percent = replayPhasePercentage(progress);
  return <div className="tick-replay-progress">
    <p role="status">{progress ? phases[progress.phase] ?? progress.phase : '任务进度暂不可用'}</p>
    {percent === undefined ? <Spin size="small" aria-label="等待任务进度，无完成百分比" /> : <>
      <Progress percent={percent} showInfo={false} aria-label="当前阶段进度" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} />
      <p className="quiet">当前阶段 {percent.toLocaleString('zh-CN', { maximumFractionDigits: 1 })}%（{progress!.completed} / {progress!.total}）</p>
    </>}
    <p className="quiet">任务更新于 <Timestamp value={progress?.updatedAt} /></p>
  </div>;
}
function LastRead({ stale, at }: { stale: boolean; at: number }) {
  return stale && at > 0 ? <p className="quiet" role="status">数据未更新，下方为上次读取的记录：<Timestamp value={new Date(at).toISOString()} /></p> : null;
}

/** Exploratory replays retain their own list and immutable detail identity. */
export function TickEconomicReplays({ route }: { route: Route }) {
  const list = useListWorkspace<TickReplayListItem>(route, collection, 'createdAt');
  return <section className="tick-economic-replays" aria-label="Tick 经济回放">
    <div className="section-heading"><h2>Tick 经济回放</h2><Button loading={list.query.isFetching} onClick={() => { void list.query.refetch(); }}>刷新回放列表</Button></div>
    <ListTools list={list} searchLabel="搜索经济回放" placeholder="搜索回放、数据集或来源因子" filters={[
      { key: 'status', label: '执行状态' }, { key: 'datasetId', label: '数据集' },
      { key: 'memberType', label: '因子数量', options: [{ value: 'single-factor', label: '单因子' }, { value: 'ensemble', label: '多因子' }] },
      { key: 'createdFrom', label: '创建日期起', kind: 'date', advanced: true }, { key: 'createdTo', label: '创建日期止', kind: 'date', advanced: true },
      { key: 'source', label: '来源研究', advanced: true },
    ]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} /><LastRead stale={list.query.isError} at={list.query.dataUpdatedAt} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<TickReplayListItem> {...list.tableProps} rowKey="id" size="small" dataSource={list.items} scroll={{ x: 960 }} locale={{ emptyText: <Empty title={list.hasFilters ? '没有匹配的 Tick 经济回放' : '尚无 Tick 经济回放'} /> }} columns={[
      { title: '回放名称', key: 'name', width: 220, fixed: 'left', ellipsis: true, ...list.sortColumn('name'), render: (_cell, row) => <a className="record-link" {...list.linkProps(row.id, { view: 'tick' })}>{shortTitle(row.name, undefined, '经济回放')}</a> },
      { title: '执行状态', width: 170, render: (_cell, row) => <div><Status plain value={row.status} label={row.status === 'RUNNING' ? '运行中' : undefined} />{(row.status === 'QUEUED' || row.status === 'RUNNING') && row.progress && <span className="quiet">{phases[row.progress.phase] ?? '回放进行中'}{typeof row.progress.completed === 'number' && typeof row.progress.total === 'number' && row.progress.total > 0 ? ` ${Math.min(100, Math.max(0, row.progress.completed / row.progress.total * 100)).toLocaleString('zh-CN', { maximumFractionDigits: 1 })}%` : ''}</span>}</div> },
      { title: '数据集', key: 'datasetName', width: 170, ellipsis: true, ...list.sortColumn('datasetName'), render: (_cell, row) => row.datasetName ?? '名称未记录' },
      { title: '因子数', key: 'memberCount', width: 95, align: 'right', ...list.sortColumn('memberCount'), dataIndex: 'memberCount' },
      { title: '结果性质', width: 120, render: () => '探索性' },
      { title: '创建时间', key: 'createdAt', width: 205, ...list.sortColumn('createdAt'), render: (_cell, row) => <Timestamp value={row.createdAt} /> },
    ]} />}
  </section>;
}
export function TickReplayDetail({ id, route }: { id: string; route: Route }) {
  const query = useQuery({ queryKey: [collection, id], queryFn: async ({ signal }) => {
    const replay = await api<TickEconomicReplayDetail>(`${collection}/${encodeURIComponent(id)}`, { signal });
    if (replay.id !== id) throw new Error('回放结果与所选编号不一致');
    return replay;
  }, refetchInterval: current => current.state.data && active(current.state.data) ? 5000 : false, refetchIntervalInBackground: false, refetchOnMount: 'always' });
  const value = query.data; const result = value?.status === 'COMPUTED' ? value.result : null;
  return <article className="tick-replay-detail" aria-label={`经济回放 ${id}`}><a className="text-link back-link" href={href(listPathFor(route))}><ArrowLeftIcon size={14} />Tick 经济回放列表</a>
    <div className="section-heading"><h3>回放详情</h3><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新所选回放</Button></div>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} /><LastRead stale={query.isError} at={query.dataUpdatedAt} />
    {query.isPending ? <Loading /> : value && <>
      <Facts items={[["回放编号", value.id], ["数据集", value.datasetId], ["执行状态", <Status value={value.status} label={value.status === 'RUNNING' ? '运行中' : undefined} />], ["科学资格", <Qualification value={value.scientificQualification} />], ["创建时间", <Timestamp value={value.createdAt} />], ["执行模型", value.executionModel ?? '未记录'], ["因子与权重", <Members replay={value} />], ["计算任务", value.taskId]]} />
      <TickReplayProgress replay={value} />
      {(value.status === 'FAILED' || value.status === 'STOPPED') && <div className="recorded-error" role="status"><strong>{value.status === 'FAILED' ? '回放失败' : '回放已停止'}</strong>{value.error && <p className="tick-replay-error">{value.error}</p>}{value.error === 'tick_economic_local_stop_remote_receipt_unsettled' && <p>本地任务已停止，远端执行回执尚未确认终态。</p>}</div>}
      {value.status === 'COMPUTED' && (result ? <>
        <section className="result-section"><Suspense fallback={<Loading />}><EquityPlot points={replayCurve(result.days)} currency="USDT" initialCapital={result.request.scenario.initial_usdt} /></Suspense>
          {result.days.some(day => day.unknown_reason !== null) && <details className="tick-replay-gaps"><summary>权益缺失与原始原因</summary><Table components={{ table: TableSurface }} rowKey="day_start_ns" size="small" pagination={{ pageSize: 10, hideOnSinglePage: true }} dataSource={result.days.filter(day => day.unknown_reason !== null)} scroll={{ x: 500 }} columns={[
            { title: '时间', render: (_cell, day) => <Timestamp value={replayDayTime(day.day_start_ns)} /> }, { title: '权益 / USDT', render: (_cell, day) => <Exact value={day.equity_usdt} /> }, { title: '原始原因', dataIndex: 'unknown_reason' },
          ]} /></details>}
        </section>
        <section className="result-section"><h3>原生回放指标</h3>{result.metrics.length ? result.metrics.map((row, index) => <section className="tick-replay-metric-group" key={`${row.partition}/${row.regime}/${index}`} aria-label={`${row.partition} / ${row.regime}`}>
          <h4>{partitionLabel(row.partition)}（{row.partition}） / {regimes[row.regime]}（{row.regime}）</h4>
          <p className="quiet">区间天数 <Exact value={row.days} /> · 有效收益天数 <Exact value={row.valid_return_days} /></p><Metrics metrics={replayMetrics(row)} />
        </section>) : <p className="quiet">未提供指标</p>}</section>
        {result.limitations.length > 0 && <section className="result-section"><h3>原始结果限制</h3><ul>{result.limitations.map((limitation, index) => <li key={index}>{limitation}</li>)}</ul></section>}
      </> : <p className="quiet" role="status">原始回放结果不可用</p>)}
    </>}
  </article>;
}
