import { expect, test } from 'vitest';
import { equityCoordinates, formatEquity, orderedEquityPoints } from './equity-values';

test('coordinates sort elapsed time without changing original precision or null gaps', () => {
  const points = [{ at: '2026-10-18T00:00:00.000000001Z', value: '1001.000000000000000001' }, { at: '2026-10-17T00:00:00.000000001Z', value: '1000.000000000000000001' }, { at: '2026-10-17T00:00:01.000000001Z', value: null }];
  const original = structuredClone(points); const plotted = equityCoordinates(points);
  expect(plotted[1]![0] - plotted[0]![0]).toBe(1000); expect(plotted[2]![0] - plotted[1]![0]).toBe(86_399_000); expect(plotted[1]![1]).toBeNull(); expect(points).toEqual(original);
});

test('sub-millisecond and duplicate clocks retain chronological and stable event order', () => {
  const points = [{ at: '2026-10-01T00:00:00.000000020Z', value: '2' }, { at: '2026-10-01T00:00:00.000000010Z', value: '1' }, { at: '2026-10-01T00:00:00.000000020Z', value: null }];
  expect(orderedEquityPoints(points).map(point => point.value)).toEqual(['1', '2', null]);
});

test('invalid timestamps are excluded from plot and empty/nonfinite values stay missing', () => {
  const points = [{ at: 'bad', value: '10' }, ...['', ' ', 'NaN', 'Infinity', null].map(value => ({ at: '2026-01-01', value }))];
  expect(orderedEquityPoints(points)).toHaveLength(5);
  expect(equityCoordinates(points).map(point => point[1])).toEqual([null, null, null, null, null]);
});

test('formatting preserves original financial decimal strings', () => {
  expect(formatEquity('9007199254740993.000000000000000001')).toBe('9,007,199,254,740,993.000000000000000001');
  expect(formatEquity('0.000000000000000001')).toBe('0.000000000000000001');
});
