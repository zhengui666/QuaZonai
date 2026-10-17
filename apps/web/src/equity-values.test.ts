import { expect, test } from 'vitest';
import { equityCoordinates } from './equity-values';
test('equity plot preserves irregular elapsed time, null gaps and original precision', () => {
  const points = [{ at: '2026-10-17T00:00:00.000000001Z', value: '1000.000000000000000001' }, { at: '2026-10-17T00:00:01.000000001Z', value: null }, { at: '2026-10-18T00:00:00.000000001Z', value: '1001.000000000000000001' }];
  const original = structuredClone(points); const plotted = equityCoordinates(points);
  expect(plotted[1]![0] - plotted[0]![0]).toBe(1000); expect(plotted[2]![0] - plotted[1]![0]).toBe(86_399_000); expect(plotted[1]![1]).toBeNull(); expect(points).toEqual(original);
});
