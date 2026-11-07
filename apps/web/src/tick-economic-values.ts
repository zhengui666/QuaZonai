import type { EquityPoint, Metric } from '../../../packages/contracts/src/index.ts';
import type { NativeTickEconomicResult, TickEconomicReplayProgress } from '../../../packages/contracts/src/economic-tick.ts';

export function replayPhasePercentage(progress: TickEconomicReplayProgress | null): number | undefined {
  const completed = progress?.completed; const total = progress?.total;
  if (typeof completed !== 'number' || typeof total !== 'number' || !Number.isSafeInteger(completed) || !Number.isSafeInteger(total) || total <= 0 || completed < 0 || completed > total) return;
  return completed / total * 100;
}

/** Preserve the original nanosecond clock and USDT value; Number is only used for Date's millisecond clock. */
export function replayDayTime(value: string): string {
  const ns = BigInt(value); const date = new Date(Number(ns / 1_000_000n));
  return `${date.toISOString().slice(0, 19)}.${(ns % 1_000_000_000n).toString().padStart(9, '0')}Z`;
}
export function replayCurve(days: NativeTickEconomicResult['days']): EquityPoint[] {
  return days.map(day => ({ at: replayDayTime(day.day_start_ns), value: day.equity_usdt }));
}
export function replayMetrics(row: NativeTickEconomicResult['metrics'][number]): Metric[] {
  return [
    { key: 'Sharpe Ratio (365 days)', label: 'sharpe_365', value: row.sharpe_365 },
    { key: 'tick.conditional_annualized_return_365', label: 'conditional_annualized_return_365', value: row.conditional_annualized_return_365, unit: '小数比例' },
  ];
}
