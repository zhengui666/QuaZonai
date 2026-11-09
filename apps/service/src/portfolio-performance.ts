import type { PortfolioPerformanceSummary } from '../../../packages/contracts/src/portfolio-performance.ts';
import type { PortfolioFact } from './facts.ts';
import type { Metric } from '../../../packages/contracts/src/index.ts';

const record = (value: unknown): Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};

/** Retain exact original CASH return scalars in detail alongside existing PnL. */
export function portfolioMetrics(fact: PortfolioFact): Metric[] {
  const spot = record(record(fact.native).simulation).spot_cash_report;
  if (spot == null) return fact.view.metrics;
  const rows = record(spot).statistics;
  if (!Array.isArray(rows)) return fact.view.metrics;
  const measured = rows.flatMap(item => {
    const row = record(item);
    if (typeof row.native_key !== 'string') return [];
    return [{ key: row.native_key, label: row.native_key, value: typeof row.value === 'number' && Number.isFinite(row.value) && !row.reason_code ? row.value : null }];
  });
  const keys = new Set(measured.map(row => row.key));
  return [...fact.view.metrics.filter(row => !keys.has(row.key)), ...measured];
}

/** No replay, no inferred equity returns: only original native scalar statistics. */
export function portfolioPerformance(fact: PortfolioFact): PortfolioPerformanceSummary {
  const native = record(fact.native); const simulation = record(native.simulation);
  const spot = record(simulation.spot_cash_report);
  const statistics = simulation.spot_cash_report != null ? spot.statistics : simulation.statistics;
  const dailyBasis = simulation.spot_cash_report != null ? 'NATIVE_SPOT_CASH_COMPLETE_DAY_RETURNS' as const : 'NATIVE_ACCOUNT_DAILY_RETURNS' as const;
  const request = record(native.simulationRequest); const selection = record(request.selection);
  const complete = ['COMPUTED', 'REVIEW_BLOCKED'].includes(fact.view.status);
  const source: { key: string; value: unknown; status?: unknown; reason?: unknown }[] = Array.isArray(statistics)
    ? statistics.map(item => { const row = record(item); return { key: String(row.native_key), value: row.value, reason: row.reason_code }; })
    : simulation.spot_cash_report != null ? [] : fact.view.metrics.map(row => ({ key: row.key, value: row.value, status: row.status }));
  const value = (key: string): number | null => {
    const rows = source.filter(row => row.key === key);
    if (!complete || rows.length !== 1) return null;
    const row = rows[0]!;
    return typeof row.value === 'number' && Number.isFinite(row.value) && !row.reason && (row.status === undefined || row.status === 'AVAILABLE') ? row.value : null;
  };
  const periods = [252, 365].filter(days => source.some(row => row.key === `Sharpe Ratio (${days} days)`));
  const annualizationDays = periods.length === 1 ? periods[0]! : null;
  const sharpe = annualizationDays === null ? null : value(`Sharpe Ratio (${annualizationDays} days)`);
  const g = value('Calendar Geometric Daily Return');
  const expectedDays = value('Calendar Expected Days'); const observedDays = value('Calendar Observed Return Days');
  let annualizedReturn: number | null = null;
  if (annualizationDays !== null && g !== null && g >= -1 && expectedDays !== null && expectedDays > 0 && Number.isSafeInteger(expectedDays) && observedDays === expectedDays) {
    const result = g === -1 ? -1 : Math.expm1(annualizationDays * Math.log1p(g));
    if (Number.isFinite(result)) annualizedReturn = result;
  }
  const signedDrawdown = value('Max Drawdown');
  return { sharpe, annualizedReturn, maxDrawdown: signedDrawdown !== null && signedDrawdown <= 0 && signedDrawdown >= -1 ? -signedDrawdown || 0 : null,
    annualizationDays, basis: { sharpe: dailyBasis, maxDrawdown: dailyBasis, annualizedReturn: 'COMPLETE_UTC_DAYS_GEOMETRIC',
      expectedDays, observedDays, partialDays: value('Calendar Partial Days'), startNs: typeof selection.event_start_ns === 'string' ? selection.event_start_ns : null,
      endNs: typeof selection.event_end_ns === 'string' ? selection.event_end_ns : null } };
}
