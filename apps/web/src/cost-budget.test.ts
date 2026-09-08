import { describe, expect, it } from 'vitest';
import { costBudgetErrors } from './cost-budget';
import { validateCostCurrency } from './generated/responses.cjs';

const estimated = { cost_enforcement: 'ESTIMATED', max_cost_decimal: '0.000000000000000001', cost_currency: 'USD' };
describe('joint cost mode, exact amount and native currency validation', () => {
  it('accepts only empty amount/currency with unavailable metering', () => {
    for (const amount of [null, undefined, '']) for (const currency of [null, undefined, '']) {
      expect(costBudgetErrors({ cost_enforcement: 'UNAVAILABLE', max_cost_decimal: amount, cost_currency: currency })).toEqual({});
    }
    expect(costBudgetErrors({ ...estimated, cost_enforcement: 'UNAVAILABLE' })).toHaveProperty('cost_enforcement');
  });
  it('requires both positive exact amount and a recognized currency for estimates', () => {
    expect(costBudgetErrors(estimated)).toEqual({});
    expect(costBudgetErrors({ ...estimated, max_cost_decimal: '99999999999999999999.999999999999999999' })).toEqual({});
    expect(costBudgetErrors({ cost_enforcement: 'ESTIMATED' })).toHaveProperty('max_cost_decimal');
    expect(costBudgetErrors({ cost_enforcement: 'ESTIMATED' })).toHaveProperty('cost_currency');
  });
  it.each([null, undefined, '', '0', '0.000', '-0', '-1', '1e3', ' 1', '1\n', 1, '0.0000000000000000001'])('rejects invalid estimated amount %j without coercion', amount => {
    expect(costBudgetErrors({ ...estimated, max_cost_decimal: amount })).toHaveProperty('max_cost_decimal');
  });
  it.each([null, undefined, '', 'ZZZ', 'usd', ' USD', 'USD\n', 'US', 'USDC', 123])('rejects unsupported or malformed currency %j', currency => {
    expect(costBudgetErrors({ ...estimated, cost_currency: currency })).toHaveProperty('cost_currency');
  });
  it('uses the generated native currency schema, not a three-letter regex', () => {
    expect(validateCostCurrency('USD')).toBe(true);
    expect(validateCostCurrency('EUR')).toBe(true);
    expect(validateCostCurrency('ZZZ')).toBe(false);
    expect(validateCostCurrency(null)).toBe(true);
    expect(costBudgetErrors({ ...estimated, cost_currency: null })).toHaveProperty('cost_currency');
  });
  it.each(['EXACT', 'UNKNOWN', undefined, null])('does not advertise or coerce unsupported metering %j', mode => {
    expect(costBudgetErrors({ ...estimated, cost_enforcement: mode })).toHaveProperty('cost_enforcement');
  });
  it('never clears or changes an inconsistent loaded record merely by validating it', () => {
    const record = Object.freeze({ ...estimated, cost_enforcement: 'UNAVAILABLE' });
    const before = JSON.stringify(record);
    expect(Object.keys(costBudgetErrors(record)).length).toBeGreaterThan(0);
    expect(JSON.stringify(record)).toBe(before);
  });
});
