import { useTranslation } from 'react-i18next';
import { tr, locale } from './i18n';
import { lazy, Suspense } from 'react';
import { Table, Tooltip } from 'antd';
import { ArrowLeftIcon } from '@phosphor-icons/react';
import type { ObservationView } from '../../../packages/contracts/src/index.ts';
import { usePortfolioDetail, useResource } from './data-hooks';
import { href, type Route } from './router';
import { Empty, ErrorNotice, Exact, Facts, Loading, MetricLabel, Metrics, Status, Timestamp } from './ui';
import { RecordTitle } from './disclosure';
import { shortTitle } from './titles';
import { ExecutionFailure } from './diagnostics';
import { FactorIdentity } from './factors';
import { DeleteRecord } from './delete-record';
import { BacktestProgress } from './backtest-progress';
import type { PortfolioListItem } from '../../../packages/contracts/src/lists.ts';
import { ListTools, listPathFor, useListWorkspace } from './list-workspace';
import type { PortfolioPerformanceSummary } from '../../../packages/contracts/src/portfolio-performance.ts';
const EquityPlot = lazy(() => import('./equity-plot'));
export default function Portfolios({ route }: { route: Route }) {
  useTranslation('core');
  if (route.creating || route.params.get('view') === 'allocations') return <Empty title={tr("The custom portfolio creation workflow has been removed")} action={<a href={href('/research/new')}>{tr("Use native research")}</a>} />;
  if (route.params.get('view') === 'tick') return <Empty title={tr("The legacy tick economic replay view has been retired")} action={<a href={href('/portfolios')}>{tr("Portfolio backtests")}</a>} />;
  if (route.id) return <PortfolioDetail id={route.id} route={route} />;
  return <div className="index-page"><header className="page-heading"><h1>{tr("Portfolio backtests")}</h1></header>
    <StrategyPortfolios route={route} />
  </div>;
}
const progressLabels: Record<string, string> = { QUEUED: "Awaiting backtest", FORECASTING: "Generating member predictions", SIMULATING: "Account backtest", CAPACITY: "Assessing capacity", COMPLETED: "Backtest completed", FAILED: "Backtest failed", STOPPED: "Backtest stopped" };
function PortfolioListProgress({ value }: { value: PortfolioListItem }) {
  useTranslation('core');
  if (['COMPUTED', 'FAILED', 'STOPPED'].includes(value.status) || !value.progress) return null;
  const { phase, completed, total } = value.progress;
  const percentage = typeof completed === 'number' && typeof total === 'number' && total > 0 ? Math.min(100, Math.max(0, completed / total * 100)) : undefined;
  return <span className="quiet">{progressLabels[phase] ? tr(progressLabels[phase]!) : tr("Backtest in progress")}{percentage !== undefined ? ` ${percentage.toLocaleString(locale(), { maximumFractionDigits: 1 })}%` : ''}</span>;
}
function PerformanceValue({ value, percent = false }: { value: number | null | undefined; percent?: boolean }) {
  useTranslation('core');
  return <span>{value == null || !Number.isFinite(value) ? tr("Not provided") : percent ? `${(value * 100).toLocaleString(locale(), { maximumFractionDigits: 2 })}%` : value.toLocaleString(locale(), { maximumFractionDigits: 3 })}</span>;
}
function PerformanceSummary({ value }: { value?: PortfolioPerformanceSummary }) {
  useTranslation('core');
  if (!value) return null;
  return <section className="result-section"><h3>{tr("Backtest performance")}</h3><Facts items={[
    [tr("Sharpe ratio"), <PerformanceValue value={value.sharpe} />],
    [tr("Annualized return"), <PerformanceValue value={value.annualizedReturn} percent />],
    [tr("Maximum drawdown (original value)"), <PerformanceValue value={value.maxDrawdown} percent />],
    [tr("Annualization days"), value.annualizationDays ?? tr("Not recorded")],
  ]} /><details><summary>{tr("Calculation basis")}</summary><Facts items={[[tr("Sharpe and maximum drawdown"), value.basis.sharpe === 'NATIVE_SPOT_CASH_COMPLETE_DAY_RETURNS' ? tr("Native spot CASH account returns for complete days, including fees") : tr("Native daily account returns; the first and last days may be incomplete")], [tr("Annualized return"), value.basis.annualizedReturn === 'NATIVE_CAGR' ? tr("Original upstream CAGR") : tr("Not provided upstream")], [tr("Expected complete days"), value.basis.expectedDays ?? tr("Not recorded")], [tr("Observed complete days"), value.basis.observedDays ?? tr("Not recorded")], [tr("Partial boundary days"), value.basis.partialDays ?? tr("Not recorded")]]} /></details></section>;
}
function StrategyPortfolios({ route }: { route: Route }) {
  useTranslation('core');
  const list = useListWorkspace<PortfolioListItem>(route, '/portfolios', 'createdAt');
  const currency = list.value('currency');
  return <section aria-label={tr("Strategy portfolio list")}><ListTools list={list} searchLabel={tr("Search portfolios")} placeholder={tr("Search portfolios, datasets, or sources")} filters={[
    { key: 'status', label: tr("Execution status") }, { key: 'datasetId', label: tr("Dataset") },
    { key: 'memberType', label: tr("Member count"), advanced: true, options: [{ value: 'single-factor', label: tr("Single member") }, { value: 'ensemble', label: tr("Multiple members") }] },
    { key: 'currency', label: tr("Currency"), advanced: true }, { key: 'capitalMin', label: tr("Minimum capital"), kind: 'number', advanced: true, disabled: !currency || currency.includes(',') },
    { key: 'capitalMax', label: tr("Maximum capital"), kind: 'number', advanced: true, disabled: !currency || currency.includes(',') },
    { key: 'createdFrom', label: tr("Created from"), kind: 'date', advanced: true }, { key: 'createdTo', label: tr("Created through"), kind: 'date', advanced: true },
    { key: 'source', label: tr("Source factor or research"), advanced: true },
  ]} /><ErrorNotice error={list.query.error} retry={() => { void list.query.refetch(); }} />
    {list.query.isPending ? <Loading /> : !list.data && list.query.error ? null : <Table<PortfolioListItem> {...list.tableProps} rowKey="id" dataSource={list.items} scroll={{ x: 1340 }} locale={{ emptyText: <Empty title={list.hasFilters ? tr("No matching strategy portfolios") : tr("No strategy portfolios yet")} /> }} columns={[
      { title: tr("Portfolio name"), key: 'name', width: 200, fixed: 'left', ellipsis: true, ...list.sortColumn('name'), render: (_value, row) => <a className="record-link" {...list.linkProps(row.id)}>{shortTitle(row.name, undefined, tr("Portfolio"))}</a> },
      { title: tr("Execution status"), width: 130, render: (_value, row) => <div><Status plain value={row.status} /><PortfolioListProgress value={row} /></div> },
      { title: tr("Dataset"), key: 'datasetName', width: 150, ellipsis: true, ...list.sortColumn('datasetName'), render: (_value, row) => row.datasetName ?? tr("Name not recorded") },
      { title: <Tooltip title={tr("The original member count saved in this record.")}><span>{tr("Members")}</span></Tooltip>, key: 'memberCount', width: 80, align: 'right', ...list.sortColumn('memberCount'), dataIndex: 'memberCount' },
      { title: <MetricLabel metricKey="capital" />, key: 'capital', width: 140, align: 'right', ...(currency && !currency.includes(',') ? list.sortColumn('capital') : {}), render: (_value, row) => <Exact value={row.capital} unit={row.currency} /> },
      { title: <Tooltip title={tr("Sharpe ratio of native daily account returns, using the backtest's original annualization setting.")}><span>Sharpe</span></Tooltip>, key: 'sharpe', width: 100, align: 'right', ...list.sortColumn('sharpe'), sortDirections: ['descend', 'ascend'], render: (_value, row) => <Tooltip title={row.performance.annualizationDays ? tr("{{days}}-day annualization", { days: row.performance.annualizationDays }) : tr("Annualization basis not recorded")}><span><PerformanceValue value={row.performance.sharpe} /></span></Tooltip> },
      { title: <Tooltip title={tr("Shows only the original upstream CAGR. Missing values are not annualized locally.")}><span>{tr("Annualized return")}</span></Tooltip>, key: 'annualizedReturn', width: 125, align: 'right', ...list.sortColumn('annualizedReturn'), sortDirections: ['descend', 'ascend'], render: (_value, row) => <PerformanceValue value={row.performance.annualizedReturn} percent /> },
      { title: <Tooltip title={tr("Maximum drawdown of native daily account returns. The original negative value is retained; closer to zero means a smaller drawdown. The first click sorts original values in descending order without recalculation.")}><span>{tr("Maximum drawdown")}</span></Tooltip>, key: 'maxDrawdown', width: 115, align: 'right', ...list.sortColumn('maxDrawdown'), render: (_value, row) => <PerformanceValue value={row.performance.maxDrawdown} percent /> },
      { title: tr("Created"), key: 'createdAt', width: 205, ...list.sortColumn('createdAt'), render: (_value, row) => <Timestamp value={row.createdAt} /> },
    ]} />}
  </section>;
}
function PortfolioDetail({ id, route }: { id: string; route: Route }) {
  useTranslation('core');
  const query = usePortfolioDetail(id); const value = query.data;
  return <div className="portfolio-result"><a className="text-link back-link" href={href(listPathFor(route))}><ArrowLeftIcon size={14} />{tr("Portfolio backtests")}</a><article className="study-document"><header className="study-heading"><div className="document-toolbar">{value && <><Status value={value.status} /><DeleteRecord key={value.id} kind="portfolio" id={value.id} back={listPathFor(route)} /></>}</div><RecordTitle name={value?.name} fallback={tr("Portfolio backtests")} /></header><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : value && <div className="result-content">
    <ExecutionFailure error={value.error} detail={value.errorDetail} identity={tr("Portfolio {{id}}{{task}}", { id: value.id, task: value.taskId !== null ? tr(" / Task #{{id}}", { id: value.taskId }) : '' })} title={tr("Backtest not completed")} />
    <Facts items={[[tr("Dataset"), value.datasetId], [tr("Starting capital"), <Exact value={value.capital} unit={value.currency} />], [tr("Created"), <Timestamp value={value.createdAt} />], [tr("Computation task"), value.taskId ?? tr("Not recorded")]]} />
    {value.factors && <section className="result-section"><h3>{tr("Original members and weights")}</h3><Table rowKey="factorId" size="small" pagination={false} dataSource={value.factors} columns={[{ title: tr("Source ID"), render: (_cell, row) => <FactorIdentity id={row.factorId} /> }, { title: <MetricLabel metricKey="weight" />, render: (_cell, row) => <Exact value={row.weight} /> }]} /></section>}
    {!value.factors && <section className="result-section"><h3>{tr("Original research and weights")}</h3><Table rowKey="researchId" size="small" pagination={false} dataSource={value.researchIds.map(researchId => ({ researchId, weight: value.weights?.find(item => item.researchId === researchId)?.weight }))} columns={[{ title: tr("Original research"), render: (_cell, row) => <a href={href(`/research/${row.researchId}`)}>{tr("Research #")}{row.researchId}</a> }, { title: <MetricLabel metricKey="weight" />, align: 'right', render: (_cell, row) => <Exact value={row.weight} missing={tr("Not recorded")} /> }]} /></section>}
    <PerformanceSummary value={value.performance} />
    <BacktestProgress portfolio={value} />
    <section className="result-section"><Suspense fallback={<Loading />}><EquityPlot points={value.curve} currency={value.currency} initialCapital={value.capital} /></Suspense></section>
    <section className="result-section"><h3>{tr("Backtest metrics")}</h3><Metrics metrics={value.metrics} /></section>
    <section className="result-section"><div className="section-heading"><h3>{tr("Target weights")}</h3><span className="quiet">{tr("Historical targets (read-only)")}</span></div>{value.targetExpiresAt && <p className="quiet expiry-fact">{tr("Expires")} <Timestamp value={value.targetExpiresAt} /></p>}{value.targets.length ? <Table rowKey="instrumentId" pagination={false} size="small" dataSource={value.targets} columns={[{ title: tr("Instrument"), dataIndex: 'instrumentId' }, { title: <MetricLabel metricKey="weight" label={tr("Target weights")} />, align: 'right', render: (_cell, row) => <Exact value={row.weight} /> }]} /> : <p className="quiet">{tr("No targets yet")}</p>}</section>
    <PortfolioAccounts portfolioId={id} />
  </div>}</article></div>;
}
function PortfolioAccounts({ portfolioId }: { portfolioId: string }) {
  useTranslation('core');
  const query = useResource<ObservationView[]>('/observations');
  const accounts = query.data?.filter(row => row.portfolioId === portfolioId);
  return <section className="result-section"><h3>{tr("Connected accounts and capital withdrawals")}</h3><ErrorNotice error={query.error} retry={() => { void query.refetch(); }} />{query.isPending ? <Loading /> : !accounts?.length ? <p className="quiet">{tr("No linked accounts yet.")}<a href={href('/observations/new')}>{tr("Link an existing account to manage withdrawals")}</a>。</p> : <ul>{accounts.map(row => <li key={row.id}><a href={href(`/observations/${encodeURIComponent(row.id)}?section=capital`)}>{row.name} · {row.mode === 'live' ? tr("Live") : tr("Paper")} · {row.connectionName ?? row.connectionId ?? tr("Account observations")}  {tr("· Manage withdrawals")}</a></li>)}</ul>}</section>;
}
