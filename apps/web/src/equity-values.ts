import type { EquityPoint } from '../../../packages/contracts/src/index.ts';
/** Plot coordinates only; original decimal/time strings remain in the record and readout. */
export function equityCoordinates(points: readonly EquityPoint[]): [number, number | null][] {
  return points.map(point => [Date.parse(point.at), point.value === null || !Number.isFinite(Number(point.value)) ? null : Number(point.value)]);
}
