import { describe, expect, it } from 'vitest';
import { nanosecondTimestamp } from './capacity-values';

describe('original capacity clocks', () => {
  it('retains nanosecond precision beyond JavaScript safe integers', () => {
    expect(nanosecondTimestamp('1800000000000000003')).toBe('2027-01-15T08:00:00.000000003Z');
    expect(nanosecondTimestamp('1')).toBe('1970-01-01T00:00:00.000000001Z');
  });
  it('can display the native u64 range without first rounding the counter', () => {
    expect(nanosecondTimestamp('18446744073709551615')).toBe('2554-07-21T23:34:33.709551615Z');
  });
  it('does not invent a timestamp for a malformed original clock', () => {
    expect(nanosecondTimestamp('not-a-clock')).toBeUndefined();
    expect(nanosecondTimestamp('999999999999999999999999999999')).toBeUndefined();
  });
});
