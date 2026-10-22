import { describe, expect, it } from 'vitest';
import { pmCapitalShare, pmMetricValue, pmRatio, pmTime } from './pm-values';
import type { MetricObservation } from '../../../packages/contracts/src/pm.ts';

describe('PM facts retain monetary precision and evidence semantics', () => {
  it('formats small signed ratios independently from monetary accounting', () => {
    expect(pmRatio('0.333333333333333333')).toBe('33.33%');
    expect(pmRatio('-0.00005')).toBe('-0.01%');
    expect(pmRatio('0.999999999999999999')).toBe('100.00%');
    expect(pmRatio(null)).toBe('证据不足');
    expect(pmCapitalShare({ amount: '1000', currency: 'USD' }, { amount: '3000', currency: 'USD' })).toBe('≈33.33%');
  });
  it('does not display an unavailable finite metric as an investment result', () => {
    const metric = { definition: { unit: 'CURRENCY' }, status: 'AVAILABLE', value: '9007199254740993.000000000000000001', currency: 'pUSD' } as MetricObservation;
    expect(pmMetricValue(metric)).toBe('9007199254740993.000000000000000001 pUSD');
    expect(pmMetricValue({ ...metric, status: 'INSUFFICIENT' })).toBe('证据不足');
    expect(pmTime('invalid')).toBe('未建立时点');
    expect(pmTime('1767225600000000000')).toBe('2026-01-01 00:00:00 UTC');
  });
});
