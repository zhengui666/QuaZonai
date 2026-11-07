import { lazy, Suspense, useState } from 'react';
import { Button, Progress, Spin, Table } from 'antd';
import { useQuery } from '@tanstack/react-query';
import type { TickEconomicReplayDetail, TickEconomicReplaySummary } from '../../../packages/contracts/src/economic-tick.ts';
import { api } from './api';
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
  return replay.members ? <ul className="tick-replay-members">{replay.members.map(member => <li key={member.factorId}><code>{member.factorId}</code> · 权重 <Exact value={member.weight} /></li>)}</ul> : <code>{replay.factorId ?? '未记录'}</code>;
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

/** A page collection, independent of standard portfolio ownership and commands. */
export function TickEconomicReplays() {
  const [selected, setSelected] = useState<string>();
  const query = useQuery({ queryKey: [collection], queryFn: ({ signal }) => api<TickEconomicReplaySummary[]>(collection, { signal }),
    refetchInterval: current => current.state.data?.some(active) ? 5000 : false, refetchIntervalInBackground: false, refetchOnMount: 'always' });
  return <section className="tick-economic-replays" aria-label="Tick 经济回放">
    <div className="section-heading"><h2>Tick 经济回放</h2><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新回放列表</Button></div>
    <p className="quiet">独立的探索性经济回放。活动记录约每 5 秒更新；新回放可手动刷新查找。</p>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} /><LastRead stale={query.isError} at={query.dataUpdatedAt} />
    {query.isPending ? <Loading /> : query.data?.length ? <Table<TickEconomicReplaySummary> rowKey="id" size="small" dataSource={query.data} pagination={{ pageSize: 10, hideOnSinglePage: true }} scroll={{ x: 1200 }} columns={[
      { title: '回放编号', width: 220, render: (_cell, row) => <Button type="link" className="tick-replay-select" aria-pressed={selected === row.id} onClick={() => setSelected(row.id)}>{row.id}</Button> },
      { title: '数据集', dataIndex: 'datasetId', width: 130 },
      { title: '因子与权重', width: 220, render: (_cell, row) => <Members replay={row} /> },
      { title: '执行模型', width: 190, render: (_cell, row) => <code>{row.executionModel ?? '未记录'}</code> },
      { title: '状态与进度', width: 220, render: (_cell, row) => <><Status value={row.status} label={row.status === 'RUNNING' ? '运行中' : undefined} /><TickReplayProgress replay={row} /></> },
      { title: '科学资格', width: 220, render: (_cell, row) => <Qualification value={row.scientificQualification} /> },
      { title: '创建时间', width: 180, render: (_cell, row) => <Timestamp value={row.createdAt} /> },
    ]} /> : !query.error && <Empty title="尚无 Tick 经济回放" />}
    {selected && <TickReplayDetail key={selected} id={selected} />}
  </section>;
}
function TickReplayDetail({ id }: { id: string }) {
  const query = useQuery({ queryKey: [collection, id], queryFn: async ({ signal }) => {
    const replay = await api<TickEconomicReplayDetail>(`${collection}/${encodeURIComponent(id)}`, { signal });
    if (replay.id !== id) throw new Error('回放结果与所选编号不一致');
    return replay;
  }, refetchInterval: current => current.state.data && active(current.state.data) ? 5000 : false, refetchIntervalInBackground: false, refetchOnMount: 'always' });
  const value = query.data; const result = value?.status === 'COMPUTED' ? value.result : null;
  return <article className="tick-replay-detail" aria-label={`经济回放 ${id}`}>
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
