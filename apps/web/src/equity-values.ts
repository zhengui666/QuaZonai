import type { EquityPoint } from '../../../packages/contracts/src/index.ts';

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

export function formatEquity(value: string | null | undefined): string | undefined {
  if (value == null) return;
  const [whole, fraction] = value.split('.');
  return whole!.replace(/\B(?=(\d{3})+(?!\d))/g, ',') + (fraction === undefined ? '' : `.${fraction}`);
}
function percentage(change: bigint, base: bigint): number | null {
  const numerator = (change < 0n ? -change : change).toString(); const denominator = base.toString();
  const mantissa = (digits: string) => Number(`${digits[0]}.${digits.slice(1, 17)}`);
  // Convert significant digits only, so very large common decimal scales cannot overflow.
  const value = (change < 0n ? -1 : 1) * mantissa(numerator) / mantissa(denominator) * 10 ** (numerator.length - denominator.length + 2);
  return Number.isFinite(value) ? value : null;
}
/** Backtest display projection, never an account-flow-adjusted or annualized return. */
export function equityPerformance(points: readonly EquityPoint[], initialCapital?: string) {
  const decimals = [initialCapital, ...points.map(point => point.value)].map(value => typeof value === 'string' && /^-?\d+(?:\.\d+)?$/.test(value) ? value.split('.') : undefined);
  const scale = decimals.reduce((length, parts) => Math.max(length, parts?.[1]?.length ?? 0), 0);
  const [capital, ...amounts] = decimals.map(parts => parts ? BigInt(parts[0]! + (parts[1] ?? '').padEnd(scale, '0')) : undefined);
  const hasCapital = capital !== undefined && capital > 0n;
  const complete = hasCapital && points.length > 0 && amounts.every(value => value !== undefined);
  let peak = hasCapital ? capital : 0n; let maxDrawdown: number | null = complete ? 0 : null;
  const returns = amounts.map(value => hasCapital && value !== undefined ? percentage(value - capital, capital) : null);
  const drawdowns = amounts.map(value => {
    if (!complete || value === undefined) return null;
    if (value > peak) peak = value;
    const drawdown = percentage(value - peak, peak);
    maxDrawdown = maxDrawdown === null || drawdown === null ? null : Math.min(maxDrawdown, drawdown);
    return drawdown;
  });
  return { hasCapital, returns, drawdowns, maxDrawdown };
}
