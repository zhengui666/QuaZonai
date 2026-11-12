import { describe, expect, it } from 'vitest';
import { scanComparisonOrdinals, scanFixedParameters, scanNativeMetrics } from './parameter-scan-values';

describe('parameter scan display bindings', () => {
  it('only displays full-selection native statistics without borrowing validation or heldout values', () => {
    const metrics = [
      { key: 'Custom Native Statistic', partition: 'FULL_SELECTION', label: 'Native', value: null },
      { key: 'Sharpe Ratio (252 days)', partition: 'FULL_SELECTION', label: 'Sharpe', value: 0 },
      { key: 'Sharpe Ratio (252 days)', partition: 'VALIDATION', label: 'Sharpe', value: 99 },
      { key: 'Sharpe Ratio (252 days)', partition: 'HELDOUT', label: 'Sharpe', value: 99 },
    ];
    expect(scanNativeMetrics(metrics)).toEqual(metrics.slice(0, 2));
    expect(scanNativeMetrics(metrics)[0]?.value).toBeNull();
    expect(scanNativeMetrics(metrics)[1]?.value).toBe(0);
  });
  it('keeps slice decimals and nanoseconds as strings instead of rounding URL values', () => {
    expect(scanFixedParameters('{"threshold":"0.123456789012345678","targetTtlNs":"9007199254740993"}')).toEqual({ threshold: '0.123456789012345678', targetTtlNs: '9007199254740993' });
    for (const value of ['{invalid', 'null', '[]', '{"threshold":0.1}']) expect(scanFixedParameters(value)).toBeUndefined();
    expect(scanFixedParameters(null)).toEqual({});
  });
  it('retains exact large result ordinals, deduplicates and bounds display to five curves', () => {
    expect(scanComparisonOrdinals('0,2,2,-1,1.1,9007199254740993,3,4,5,6')).toEqual(['0', '2', '9007199254740993', '3', '4']);
    expect(scanComparisonOrdinals(null)).toEqual([]);
  });
});
