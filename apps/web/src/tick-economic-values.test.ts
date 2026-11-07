import { expect, it } from 'vitest';
import { replayCurve, replayMetrics, replayPhasePercentage } from './tick-economic-values';

it('never manufactures percentages from missing, unsafe or invalid phase counters', () => {
  const base = { phase: 'REPLAYING', updatedAt: '2026-11-07T00:00:00Z' };
  expect(replayPhasePercentage(null)).toBeUndefined();
  for (const counters of [{}, { completed: 1 }, { completed: 0, total: 0 }, { completed: -1, total: 5 }, { completed: 6, total: 5 }, { completed: 1.5, total: 5 }, { completed: 1, total: Infinity }, { completed: 1, total: Number.MAX_SAFE_INTEGER + 1 }]) expect(replayPhasePercentage({ ...base, ...counters })).toBeUndefined();
  expect(replayPhasePercentage({ ...base, completed: 0, total: 4 })).toBe(0);
  expect(replayPhasePercentage({ ...base, completed: 1, total: 4 })).toBe(25);
});

it('keeps original USDT money, nanosecond clocks and missing equity without a replacement curve', () => {
  const days = [
    { day_start_ns: '1767225600000000001', equity_usdt: '9007199254740995.000000000000000001', net_return: 0, unknown_reason: null },
    { day_start_ns: '1767312000000000000', equity_usdt: null, net_return: null, unknown_reason: 'missing_mark' },
  ];
  expect(replayCurve(days)).toEqual([{ at: '2026-01-01T00:00:00.000000001Z', value: days[0]!.equity_usdt }, { at: '2026-01-02T00:00:00.000000000Z', value: null }]);
  expect(days[1]!.unknown_reason).toBe('missing_mark');
});

it('retains original native zero and unavailable statistics without annualizing in the browser', () => {
  const row = { partition: 'HELDOUT' as const, regime: 'BEAR' as const, days: '12', valid_return_days: '4', sharpe_365: 0, conditional_annualized_return_365: null };
  expect(replayMetrics(row).map(metric => metric.value)).toEqual([0, null]);
  expect(row).toMatchObject({ partition: 'HELDOUT', regime: 'BEAR', days: '12', valid_return_days: '4' });
});
