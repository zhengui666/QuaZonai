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
