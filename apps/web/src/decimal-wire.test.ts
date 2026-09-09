import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { isDecimal } from './api';

// The same corpus is checked by Rust DecimalValue and the OpenAPI validators.
const cases: { input: unknown; valid: boolean }[] = JSON.parse(readFileSync(
  new URL('../../../tests/contracts/decimal-wire.json', import.meta.url), 'utf8',
));
describe('exact native decimal input boundaries', () => {
  it.each(cases)('matches the Rust wire grammar for $input', ({ input, valid }) => {
    expect(isDecimal(input)).toBe(valid);
  });
  it.each([
    '0', '1', '-1', '0.1', '-0.1', '+1', '01', '.1', '1.', '+000.0100',
    '12345678901234567890.123456789012345678',
    '-12345678901234567890.123456789012345678',
  ])('accepts complete native decimal representation %j', value => {
    expect(isDecimal(value)).toBe(true);
  });
  it.each(['\n', '\r', '\r\n', '\u2028', '\u2029', ' ', '\t', '\0'])
    ('rejects a trailing or leading character %j without trimming it', suffix => {
      for (const decimal of ['0', '1', '-1', '0.125', '-1.25', '+000.0100', '.1']) {
        expect(isDecimal(decimal + suffix)).toBe(false);
        expect(isDecimal(suffix + decimal)).toBe(false);
      }
    });
  it.each([
    '', '1e3', 'NaN', 'Infinity', '123456789012345678901', '0.1234567890123456789',
    '1\n2', '1\r2', '1\u20282', '1\u20292', null, 1, {}, [],
  ])('rejects nondecimal, non-string or out-of-range input %j', value => {
    expect(isDecimal(value)).toBe(false);
  });
});
