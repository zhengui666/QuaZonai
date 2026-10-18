import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
import { decimalUnits } from './commands';

/** Sort a display copy, including native sub-ms clocks; preserve same-clock event order. */
export function orderedEquityPoints(points: readonly EquityPoint[]): EquityPoint[] {
  return points.map(point => ({ point, time: Date.parse(point.at), fraction: (point.at.match(/\.(\d+)/)?.[1] ?? '').padEnd(9, '0') }))
    .filter(item => Number.isFinite(item.time))
    .sort((a, b) => a.time - b.time || a.fraction.localeCompare(b.fraction))
    .map(item => item.point);
}
function plotValue(value: string | null): number | null {
  return value === null || value.trim() === '' || !Number.isFinite(Number(value)) ? null : Number(value);
}
/** Plot coordinates only; original decimal/time strings remain in the record and readout. */
export function equityCoordinates(points: readonly EquityPoint[]): [number, number | null][] {
  return orderedEquityPoints(points).map(point => [Date.parse(point.at), plotValue(point.value)]);
}

/** Backtest display projection, never an account-flow-adjusted or annualized return. */
export function equityPerformance(points: readonly EquityPoint[], initialCapital?: string) {
  const capital = initialCapital === undefined ? undefined : decimalUnits(initialCapital);
  const hasCapital = capital !== undefined && capital > 0n;
  const amounts = points.map(point => point.value === null ? undefined : decimalUnits(point.value));
  const complete = hasCapital && points.length > 0 && amounts.every(value => value !== undefined);
  let peak = hasCapital ? capital : 0n; let maxDrawdown: number | null = complete ? 0 : null;
  const returns = amounts.map(value => hasCapital && value !== undefined ? Number(value - capital) / Number(capital) * 100 : null);
  const drawdowns = amounts.map(value => {
    if (!complete || value === undefined) return null;
    if (value > peak) peak = value;
    const drawdown = Number(value - peak) / Number(peak) * 100;
    maxDrawdown = Math.min(maxDrawdown!, drawdown);
    return drawdown;
  });
  return { hasCapital, returns, drawdowns, maxDrawdown };
}
