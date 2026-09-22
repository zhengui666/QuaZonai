import { describe, expect, it } from 'vitest';
import { validateBaseCurrency, validateCostCurrency } from '@quazonai/web/response-contract';

describe('native research currency contracts', () => {
  it.each(['USDC', 'USDC.e', 'pUSD'])('preserves %s without widening model billing', (currency) => {
    expect(validateBaseCurrency(currency)).toBe(true);
    expect(validateCostCurrency(currency)).toBe(false);
  });

  it.each(['USD', 'EUR', 'CNY'])('retains existing ISO currency %s', (currency) => {
    expect(validateBaseCurrency(currency)).toBe(true);
    expect(validateCostCurrency(currency)).toBe(true);
  });

  it.each(['PUSD', 'usdc', 'USDT', 'USD ', ' USD', '', null, 1])('rejects an unsupported or ambiguous unit %s', (currency) => {
    expect(validateBaseCurrency(currency)).toBe(false);
    expect(validateCostCurrency(currency)).toBe(false);
  });
});
