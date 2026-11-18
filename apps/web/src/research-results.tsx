import { useTranslation } from 'react-i18next';
import i18n from './i18n';
import { useState, useRef } from 'react';
import { Alert, Descriptions, Pagination, Table, Tag } from 'antd';
import { api } from './api';
import { ErrorNotice } from './ui';
import type { ResearchDetail } from '../../../packages/contracts/src/index.ts';

const t = (key: string) => i18n.t(key, { ns: 'shared' });
const record = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
const scalar = (value: unknown) => typeof value === 'string' || typeof value === 'number' ? String(value) : t("Not provided");
export function unavailableReason(value: unknown): string {
  const reasons: Record<string, string> = {
    OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE: t("No native daily equity return series is available yet."),
    UNPRICED_PORTFOLIO_SNAPSHOTS: t("Some account snapshots lack valuations, so return performance cannot be compared."),
    DISABLED_BY_DISPOSE_ON_COMPLETION_CONFIG: t("This configuration does not retain extended reports."),
    UPSTREAM_DISPOSE_ON_COMPLETION: t("This configuration does not retain extended reports."),
    NO_COMPLETED_NATIVE_RESULT: t("No completed native run result is available yet."),
    NO_RETAINED_ENGINE: t("This run did not retain readable extended reports."),
    OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG: t("This configuration discards extended reports after the run."),
    OFFICIAL_RUN_RESULT_UNAVAILABLE: t("The native run has no readable completion result yet."),
    NATIVE_CONFIGURED_ENGINE_UNAVAILABLE: t("This configured run did not retain a readable engine report."),
  };
  return typeof value === 'string' && reasons[value] ? reasons[value] : t("The evidence is not available yet and cannot be used to assess strategy performance.");
}
export function NativeStrategyResultView({ value, researchId }: { value: NonNullable<ResearchDetail['final']>['nativeStrategyResult']; researchId?: number }) {
  useTranslation('shared');
  const [page, setPage] = useState(1);
  const [remote, setRemote] = useState<{ items: unknown[]; total: number; resultTaskId: number }>();
  const [loading, setLoading] = useState(false); const [error, setError] = useState<unknown>(); const inFlight = useRef(false);
  const loadPage = async (next: number) => {
    if (!researchId || typeof value?.resultTaskId !== 'number' || inFlight.current) return;
    inFlight.current = true; setLoading(true); setError(undefined);
    try { const result = await api<{ items: unknown[]; total: number; resultTaskId: number }>(`/research/${researchId}/native-runs?resultTaskId=${value.resultTaskId}&offset=${(next - 1) * 20}`); if (result.resultTaskId !== value.resultTaskId) throw new Error(t("The result has changed. Reopen the results page.")); setRemote(result); setPage(next); }
    catch (failure) { setError(failure); } finally { inFlight.current = false; setLoading(false); }
  };
  if (!value) return null;
  const request = record(value.request); const venue = record(request.venue); const strategy = record(request.strategy);
  const equity = record(value.equityPerformance);
  const statistics = Array.isArray(equity.statistics) ? equity.statistics : [];
  const runs = remote?.items ?? (Array.isArray(value.nativeResults) ? value.nativeResults : []);
  const total = remote?.total ?? (typeof value.nativeResultsTotal === 'number' ? value.nativeResultsTotal : runs.length);
  const canonical = record(value.canonicalResult);
  const tables = [['orders', t("Orders")], ['fills', t("Fills")], ['positions', t("Positions")], ['portfolio_snapshots', t("Equity snapshots")]] as const;
  return <section className="research-result" aria-label={t("Native backtest results")}>
    <header className="result-title"><div><span className="eyebrow">{t("NAUTILUS \u00b7 HISTORICAL BACKTEST")}</span><h3>{t("Backtest results")}</h3></div><Tag>{t("Original native values")}</Tag></header>

    {record(value.instrumentDefinition).mode === 'STATIC_SCENARIO' && <Alert type="warning" title={t("Static instrument scenario")} description={t("Historical instrument specifications have not been verified.")} />}
    <Descriptions size="small" column={{ xs: 1, sm: 2 }} items={[
      { key: 'strategy', label: t("Strategy"), children: scalar(strategy.name ?? strategy.kind ?? request.name) },
      { key: 'instrument', label: t("Instrument"), children: scalar(request.instrument_id) },
      { key: 'native', label: 'Nautilus', children: scalar(value.nativeVersion) },
      { key: 'currency', label: t("Equity currency"), children: scalar(equity.currency) },
    ]} />
    <details className="result-card"><summary>{t("Backtest conditions")}</summary><Descriptions size="small" column={1} items={[
      { key: 'start', label: t("Replay start (original nanosecond timestamp)"), children: scalar(request.event_start_ns) },
      { key: 'end', label: t("Replay end (original nanosecond timestamp)"), children: scalar(request.event_end_ns) },
      { key: 'balance', label: t("Starting balances and currencies"), children: Array.isArray(venue.starting_balances) ? venue.starting_balances.filter(value => typeof value === 'string').slice(0, 20).join('; ') || t("Not provided") : t("Starting balances are not included in this report.") },
      { key: 'maker', label: t("Maker fee (original decimal)"), children: scalar(venue.maker_fee) },
      { key: 'taker', label: t("Taker fee (original decimal)"), children: scalar(venue.taker_fee) },
    ]} /></details>
    <section className="result-card"><h4>{t("Account equity performance")}</h4>
      {equity.status !== 'AVAILABLE' && <Alert type="info" title={t("Not ready for evaluation")} description={unavailableReason(equity.reason ?? value.equityUnavailableReason)} />}
      <Table pagination={false} size="small" rowKey={(_, i) => String(i)} dataSource={statistics.filter(item => ['CAGR', 'Sharpe Ratio'].some(name => String(record(item).native_key).startsWith(name))).slice(0, 8).map(record)} columns={[
        { title: t("Native statistic"), dataIndex: 'native_key', render: value => scalar(value) },
        { title: t("Annualization period"), dataIndex: 'annualization_days', render: value => i18n.t('{{days}} days', { ns: 'shared', days: scalar(value) }) },
        { title: t("Original value"), dataIndex: 'value', render: value => equity.status === 'AVAILABLE' ? scalar(value) : t("Unavailable") },
      ]} locale={{ emptyText: t("No account equity statistics are available yet.") }} />
      <p className="result-footnote">{t("Annualized results are extrapolations, not actual full-year returns.")}</p>
      <Descriptions size="small" column={2} items={[
        { key: 'days', label: t("Observed days"), children: scalar(equity.observed_utc_days) },
        { key: 'returns', label: t("Days in return series"), children: scalar(equity.return_days) },
      ]} />
    </section>
    {total > 0 && <section className="result-card"><h4>{t("Native run records")}</h4><ErrorNotice error={error} /><Table loading={loading} size="small" pagination={false} rowKey={(_, index) => String(index)} dataSource={runs.slice(0, 20).map(record)} columns={[
      { title: t("Order count"), dataIndex: 'total_orders', render: scalar },
      { title: t("Position count"), dataIndex: 'total_positions', render: scalar },
      { title: t("Event count"), dataIndex: 'total_events', render: scalar },
      { title: t("Replay iterations"), dataIndex: 'iterations', render: scalar },
    ]} />{researchId ? <Pagination current={page} pageSize={20} total={total} onChange={next => { void loadPage(next); }} disabled={loading} showSizeChanger={false} hideOnSinglePage aria-label={t("Native run record pages")} /> : total > 20 ? <p>{t("Only the first 20 run records are shown.")}</p> : null}</section>}
    {value.canonicalResult === null && <Alert type="info" title={t("Extended reports were not retained")} description={unavailableReason(value.canonicalUnavailableReason)} />}
    {tables.some(([key]) => canonical[key] !== undefined) && <section className="result-card"><h4>{t("Original order and account evidence")}</h4>{tables.map(([key, label]) => {
      const table = canonical[key]; if (table === undefined) return null;
      const manifest = record(table);
      return <p key={key}><strong>{label}</strong> · {Array.isArray(table) ? i18n.t('{{count}} records', { ns: 'shared', count: table.length }) : manifest.format === 'qz.native-table/v1' ? t("Saved in pages") : t("Original evidence retained")} · {t("Details not loaded")}</p>;
    })}</section>}
  </section>;
}
