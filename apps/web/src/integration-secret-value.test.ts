import { describe, expect, it } from 'vitest';
import { integrationSecretValueValid } from './integration-secret-value';

describe('purpose-specific integration credential fields', () => {
  it('accepts complete CA input beyond 64 KiB without accepting empty or non-ASCII material', () => {
    const value = 'certificate-content\n'.repeat(4000);
    expect(value.length).toBeGreaterThan(65536);
    expect(integrationSecretValueValid('TLS_CA', value)).toBe(true);
    for (const invalid of ['', '中文', `${value}中`]) {
      expect(integrationSecretValueValid('TLS_CA', invalid)).toBe(false);
    }
  });
  it('retains the distinct Runtime and Downstream credential boundaries', () => {
    for (const purpose of ['RUNTIME', 'DOWNSTREAM'] as const) {
      for (const length of [32, 8192]) expect(integrationSecretValueValid(purpose, 'x'.repeat(length))).toBe(true);
      for (const value of ['', 'x'.repeat(8193), 'x'.repeat(32) + ' ', 'x'.repeat(32) + '\n', '中文']) {
        expect(integrationSecretValueValid(purpose, value)).toBe(false);
      }
    }
    expect(integrationSecretValueValid('RUNTIME', 'x'.repeat(31))).toBe(false);
    expect(integrationSecretValueValid('DOWNSTREAM', 'x')).toBe(true);
  });
});
