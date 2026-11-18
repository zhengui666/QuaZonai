import { useTranslation } from 'react-i18next';
import i18n, { locale } from './i18n';
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

const t = (key: string) => i18n.t(key, { ns: 'shared' });
const EquityPlot = lazy(() => import('./equity-plot'));
const collection = '/tick-economic-replays';
const active = (value: TickEconomicReplaySummary) => value.status === 'QUEUED' || value.status === 'RUNNING';
const phases: Record<string, string> = { QUEUED: "Queued", compute: "Computing economic replay", PREPARING_MODELS: "Preparing original models", REPLAYING: "Native economic replay", RECONCILING: "Reconciling the original execution receipt" };
const regimes = { BULL: "Rising market", BEAR: "Falling market", RANGE: "Range-bound market", UNKNOWN: "Unknown market regime" };

function Qualification({ value }: { value: string }) {
  useTranslation('shared');
  return <span className="tick-replay-qualification">{value === 'UNQUALIFIED_EXPLORATORY' && <span>{t("Exploratory result, not scientifically qualified")}</span>}<code>{value}</code></span>;
}
function Members({ replay }: { replay: TickEconomicReplaySummary }) {
  useTranslation('shared');
  return replay.members ? <ul className="tick-replay-members">{replay.members.map(member => <li key={member.factorId}><FactorIdentity id={member.factorId} /><span>{t("Weight")} <Exact value={member.weight} /></span></li>)}</ul> : replay.factorId ? <FactorIdentity id={replay.factorId} /> : <span>{t("Not recorded")}</span>;
}
export function TickReplayProgress({ replay }: { replay: TickEconomicReplaySummary }) {
  useTranslation('shared');
  if (!active(replay)) return null;
  const progress = replay.progress; const percent = replayPhasePercentage(progress);
  return <div className="tick-replay-progress">
    <p role="status">{progress ? phases[progress.phase] ? t(phases[progress.phase]!) : progress.phase : t("Task progress is not available yet.")}</p>
    {percent === undefined ? <Spin size="small" aria-label={t("Waiting for task progress; completion percentage unavailable")} /> : <>
      <Progress percent={percent} showInfo={false} aria-label={t("Current phase progress")} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} />
      <p className="quiet">{i18n.t('Current phase: {{percent}}% ({{completed}} / {{total}})', { ns: 'shared', percent: percent.toLocaleString(locale(), { maximumFractionDigits: 1 }), completed: progress!.completed, total: progress!.total })}</p>
    </>}
    <p className="quiet">{t("Task updated")} <Timestamp value={progress?.updatedAt} /></p>
  </div>;
}
function LastRead({ stale, at }: { stale: boolean; at: number }) {
  useTranslation('shared');
  return stale && at > 0 ? <p className="quiet" role="status">{t("Refresh failed. Showing the last retrieved records:")}<Timestamp value={new Date(at).toISOString()} /></p> : null;
}

/** Exploratory replays retain their own list and immutable detail identity. */
export function TickEconomicReplays({ route }: { route: Route }) {
  useTranslation('shared');
  const list = useListWorkspace<TickReplayListItem>(route, collection, 'createdAt');
  return <section className="tick-economic-replays" aria-label={t("Tick economic replays")}>
    <div className="section-heading"><h2>{t("Tick economic replays")}</h2><Button loading={list.query.isFetching} onClick={() => { void list.query.refetch(); }}>{t("Refresh replays")}</Button></div>
    <ListTools list={list} searchLabel={t("Search economic replays")} placeholder={t("Search replays, datasets or source factors")} filters={[
      { key: 'status', label: t("Execution status") }, { key: 'datasetId', label: t("Dataset") },
      { key: 'memberType', label: t("Number of factors"), options: [{ value: 'single-factor', label: t("Single factor") }, { value: 'ensemble', label: t("Multiple factors") }] },
      { key: 'createdFrom', label: t("Created from"), kind: 'date', advanced: true }, { key: 'createdTo', label: t("Created through"), kind: 'date', advanced: true },
      { key: 'source', label: t("Source research"), advanced: true },
    ]} />
    <ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} /><LastRead stale={list.query.isError} at={list.query.dataUpdatedAt} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<TickReplayListItem> {...list.tableProps} rowKey="id" size="small" dataSource={list.items} scroll={{ x: 960 }} locale={{ emptyText: <Empty title={list.hasFilters ? t("No matching tick economic replays") : t("No tick economic replays yet")} /> }} columns={[
      { title: t("Replay name"), key: 'name', width: 220, fixed: 'left', ellipsis: true, ...list.sortColumn('name'), render: (_cell, row) => <a className="record-link" {...list.linkProps(row.id, { view: 'tick' })}>{shortTitle(row.name, undefined, t("Economic replay"))}</a> },
      { title: t("Execution status"), width: 170, render: (_cell, row) => <div><Status plain value={row.status} label={row.status === 'RUNNING' ? t("Running") : undefined} />{(row.status === 'QUEUED' || row.status === 'RUNNING') && row.progress && <span className="quiet">{t(phases[row.progress.phase] ?? "Replay in progress")}{typeof row.progress.completed === 'number' && typeof row.progress.total === 'number' && row.progress.total > 0 ? ` ${Math.min(100, Math.max(0, row.progress.completed / row.progress.total * 100)).toLocaleString(locale(), { maximumFractionDigits: 1 })}%` : ''}</span>}</div> },
      { title: t("Dataset"), key: 'datasetName', width: 170, ellipsis: true, ...list.sortColumn('datasetName'), render: (_cell, row) => row.datasetName ?? t("Name not recorded") },
      { title: t("Factors"), key: 'memberCount', width: 95, align: 'right', ...list.sortColumn('memberCount'), dataIndex: 'memberCount' },
      { title: t("Result type"), width: 120, render: () => t("Exploratory") },
      { title: t("Created"), key: 'createdAt', width: 205, ...list.sortColumn('createdAt'), render: (_cell, row) => <Timestamp value={row.createdAt} /> },
    ]} />}
  </section>;
}
export function TickReplayDetail({ id, route }: { id: string; route: Route }) {
  useTranslation('shared');
  const query = useQuery({ queryKey: [collection, id], queryFn: async ({ signal }) => {
    const replay = await api<TickEconomicReplayDetail>(`${collection}/${encodeURIComponent(id)}`, { signal });
    if (replay.id !== id) throw new Error(t("The replay result does not match the selected ID."));
    return replay;
  }, refetchInterval: current => current.state.data && active(current.state.data) ? 5000 : false, refetchIntervalInBackground: false, refetchOnMount: 'always' });
  const value = query.data; const result = value?.status === 'COMPUTED' ? value.result : null;
  return <article className="tick-replay-detail" aria-label={i18n.t('Economic replay {{id}}', { ns: 'shared', id })}><a className="text-link back-link" href={href(listPathFor(route))}><ArrowLeftIcon size={14} />{t("Tick economic replay list")}</a>
    <div className="section-heading"><h3>{t("Replay details")}</h3><Button loading={query.isFetching} onClick={() => { void query.refetch(); }}>{t("Refresh this replay")}</Button></div>
    <ErrorNotice error={query.error} retry={() => { void query.refetch(); }} /><LastRead stale={query.isError} at={query.dataUpdatedAt} />
    {query.isPending ? <Loading /> : value && <>
      <Facts items={[[t("Replay ID"), value.id], [t("Dataset"), value.datasetId], [t("Execution status"), <Status value={value.status} label={value.status === 'RUNNING' ? t("Running") : undefined} />], [t("Scientific qualification"), <Qualification value={value.scientificQualification} />], [t("Created"), <Timestamp value={value.createdAt} />], [t("Execution model"), value.executionModel ?? t("Not recorded")], [t("Factors and weights"), <Members replay={value} />], [t("Calculation task"), value.taskId], [t("Configured CPU cores"), value.bookCores ?? 1]]} />
      <TickReplayProgress replay={value} />
      {(value.status === 'FAILED' || value.status === 'STOPPED') && <div className="recorded-error" role="status"><strong>{value.status === 'FAILED' ? t("Replay failed") : t("Replay stopped")}</strong>{value.error && <p className="tick-replay-error">{value.error}</p>}{value.error === 'tick_economic_local_stop_remote_receipt_unsettled' && <p>{t("The local task stopped, but the remote execution receipt has no confirmed terminal state.")}</p>}</div>}
      {value.status === 'COMPUTED' && (result ? <>
        <section className="result-section"><Suspense fallback={<Loading />}><EquityPlot points={replayCurve(result.days)} currency="USDT" initialCapital={result.request.scenario.initial_usdt} /></Suspense>
          {result.days.some(day => day.unknown_reason !== null) && <details className="tick-replay-gaps"><summary>{t("Missing equity and original reasons")}</summary><Table components={{ table: TableSurface }} rowKey="day_start_ns" size="small" pagination={{ pageSize: 10, hideOnSinglePage: true }} dataSource={result.days.filter(day => day.unknown_reason !== null)} scroll={{ x: 500 }} columns={[
            { title: t("Time"), render: (_cell, day) => <Timestamp value={replayDayTime(day.day_start_ns)} /> }, { title: t("Equity / USDT"), render: (_cell, day) => <Exact value={day.equity_usdt} /> }, { title: t("Original reason"), dataIndex: 'unknown_reason' },
          ]} /></details>}
        </section>
        <section className="result-section"><h3>{t("Native replay metrics")}</h3>{result.metrics.length ? result.metrics.map((row, index) => <section className="tick-replay-metric-group" key={`${row.partition}/${row.regime}/${index}`} aria-label={`${row.partition} / ${row.regime}`}>
          <h4>{partitionLabel(row.partition)} ({row.partition}) / {t(regimes[row.regime])} ({row.regime})</h4>
          <p className="quiet">{t("Days in partition")} <Exact value={row.days} /> · {t("Valid return days")} <Exact value={row.valid_return_days} /></p><Metrics metrics={replayMetrics(row)} />
        </section>) : <p className="quiet">{t("No metrics provided")}</p>}</section>
        {result.limitations.length > 0 && <section className="result-section"><h3>{t("Original result limitations")}</h3><ul>{result.limitations.map((limitation, index) => <li key={index}>{limitation}</li>)}</ul></section>}
      </> : <p className="quiet" role="status">{t("The original replay result is unavailable.")}</p>)}
    </>}
  </article>;
}
