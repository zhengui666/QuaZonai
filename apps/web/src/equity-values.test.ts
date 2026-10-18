import { expect, test } from 'vitest';
import { equityCoordinates, equityPerformance, orderedEquityPoints } from './equity-values';

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

test('returns use explicit capital; drawdown includes initial capital and negative equity', () => {
  const points = ['90', '120', '60', '-10'].map((value, i) => ({ at: `2026-01-0${i + 1}`, value }));
  const values = equityPerformance(points, '100');
  values.returns.forEach((value, i) => expect(value).toBeCloseTo([-10, 20, -40, -110][i]!, 10));
  values.drawdowns.forEach((value, i) => expect(value).toBeCloseTo([-10, 0, -50, -108.33333333333333][i]!, 10));
  expect(values.maxDrawdown).toBeCloseTo(-108.33333333333333, 10);
  expect(equityPerformance(points).returns).toEqual([null, null, null, null]);
  expect(equityPerformance(points, '0').maxDrawdown).toBeNull();
});

test('incomplete history never publishes a complete-history drawdown or a fake latest return', () => {
  const points = [{ at: '2026-01-01', value: '100' }, { at: '2026-01-02', value: null }, { at: '2026-01-03', value: '110' }];
  expect(equityPerformance(points, '100')).toMatchObject({ returns: [0, null, 10], drawdowns: [null, null, null], maxDrawdown: null });
});

test('decimal subtraction happens before conversion to plotting numbers', () => {
  const values = equityPerformance([{ at: '2026-01-01', value: '10000000000000000.01' }], '10000000000000000');
  expect(values.returns[0]).toBeCloseTo(1e-16, 25);
});
