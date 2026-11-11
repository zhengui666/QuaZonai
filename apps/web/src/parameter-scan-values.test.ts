import { describe, expect, it } from 'vitest';
import { scanComparisonOrdinals, scanFixedParameters, scanValidationMetrics } from './parameter-scan-values';

describe('parameter scan display bindings', () => {
  it('never replaces validation measurements with stronger heldout or contradictory rows', () => {
    const metrics = [
      { key: 'VALIDATION.pearson_ic', partition: 'VALIDATION', label: 'IC', value: null, status: 'INSUFFICIENT_SAMPLES' },
      { key: 'HELDOUT.pearson_ic', partition: 'HELDOUT', label: 'IC', value: 0.99 },
      { key: 'VALIDATION.pearson_ic', partition: 'HELDOUT', label: 'IC', value: 1 },
      { key: 'HELDOUT.pearson_ic', partition: 'VALIDATION', label: 'IC', value: 1 },
      { key: 'Sharpe Ratio (365 days)', partition: 'VALIDATION', label: 'Sharpe', value: 0 },
      { key: 'unbound', label: 'Unknown', value: 99 },
    ];
    expect(scanValidationMetrics(metrics)).toEqual([metrics[0], metrics[4]]);
    expect(scanValidationMetrics(metrics)[0]?.value).toBeNull();
    expect(scanValidationMetrics(metrics)[1]?.value).toBe(0);
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
