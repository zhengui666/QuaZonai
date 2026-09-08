import { describe, expect, it } from 'vitest';
import { isDecimal } from './api';

// Research budget/money inputs must preserve their exact wire representation.
// No trim/coercion may turn an invalid submitted value into a different value.
describe('exact decimal input boundaries', () => {
  it.each([
    '0', '1', '-1', '0.1', '-0.1',
    '12345678901234567890.123456789012345678',
    '-12345678901234567890.123456789012345678',
  ])('accepts the complete decimal representation %j', (value) => {
    expect(isDecimal(value)).toBe(true);
  });

  it.each(['\n', '\r', '\r\n', '\u2028', '\u2029', ' ', '\t', '\0'])
    ('rejects a trailing or leading character %j', (suffix) => {
      for (const decimal of ['0', '1', '-1', '0.125', '-1.25']) {
        expect(isDecimal(decimal + suffix)).toBe(false);
        expect(isDecimal(suffix + decimal)).toBe(false);
      }
    });

  it.each([
    '', '+1', '01', '.1', '1.', '1e3', 'NaN', 'Infinity',
    '123456789012345678901', '0.1234567890123456789',
    '1\n2', '1\r2', '1\u20282', '1\u20292',
  ])('rejects nondecimal, noncanonical or out-of-range input %j', (value) => {
    expect(isDecimal(value)).toBe(false);
  });
});
