import assert from 'node:assert/strict';
import { test } from 'node:test';
import { capitalFillLoss, planCapitalSlice } from '../src/capital-account.ts';
import type { NativeInstrument } from '../src/account-runtime.ts';
import { units, decimal, multiply, DECIMAL_SCALE } from '../src/hyperliquid-account.ts';

const instrument: NativeInstrument = { instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID', base_currency: 'HYPE', quote_currency: 'USDC',
  size_increment: '0.001', price_increment: '0.001', minimum_quantity: null, minimum_notional: '10', taker_fee_rate: '0.001',
  bid: '100', ask: '100.1', asof_ns: '1', bid_depth: [{ price: '100', quantity: '1000' }] };
const bounds = { instrument, needed: '100', maximumNotional: '10', participation: '0.1', remainingQuantity: '1.1', availableQuantity: '1.1', minimumPrice: '100', referenceBid: '100', feeRate: '0.001', remainingCost: '0.2', currencyPrecision: 8 };
test('nonzero fees and native minimum notional allow a final legal slice to finish the cash goal', () => {
  let cash = 0n, filled = 0n, cost = 0n, count = 0;
  while (cash < units('100')) {
    const planned = planCapitalSlice({ ...bounds, needed: decimal(units('100') - cash), remainingQuantity: decimal(units('1.1') - filled), remainingCost: decimal(units('0.2') - cost) });
    const quantity = units(planned.quantity), gross = multiply(quantity, units('100')), fee = multiply(gross, units('0.001'));
    assert.ok(gross <= units('10')); assert.ok(gross >= units('10'));
    filled += quantity; cash += gross - fee; cost += units(capitalFillLoss('100', planned.quantity, '100', decimal(fee)));
    assert.ok(++count <= 11);
  }
  assert.equal(count, 11); assert.ok(cash >= units('100')); assert.ok(filled <= units('1.1')); assert.ok(cost <= units('0.2'));
});
test('an explicit batch cap below the venue minimum cannot be silently increased', () => {
  assert.throws(() => planCapitalSlice({ ...bounds, maximumNotional: '5', needed: '0.01' }), /capital_slice_below_native_minimum/);
});
test('Hyperliquid integer prices keep all significant digits without weakening the minimum price', () => {
  const planned = planCapitalSlice({ ...bounds, instrument: { ...instrument, bid: '100001', ask: '100002', bid_depth: [{ price: '100001', quantity: '10' }] },
    needed: '80', maximumNotional: '200', remainingQuantity: '0.01', availableQuantity: '0.01', minimumPrice: '100001', referenceBid: '100001', remainingCost: '1' });
  assert.equal(planned.minimum_price, '100001');
});
test('price improvement covers its own fee without reversing earlier economic losses', () => {
  assert.equal(capitalFillLoss('100', '1', '200', '0.2'), '0');
  const old = units(capitalFillLoss('100', '1', '99', '0.099'));
  assert.equal(decimal(old + units(capitalFillLoss('100', '1', '200', '0.2'))), '1.099');
});
test('the fee-aware floor preserves the remaining economic loss limit', () => {
  const planned = planCapitalSlice({ ...bounds, needed: '9', minimumPrice: '98', maximumNotional: '10', remainingQuantity: '1', availableQuantity: '1', feeRate: '0.01', remainingCost: '0.11' });
  const worstFee = decimal(multiply(multiply(units(planned.quantity), units(planned.minimum_price)), units('0.01')));
  assert.ok(units(capitalFillLoss('100', planned.quantity, planned.minimum_price, worstFee)) <= units('0.11'));
});
test('an explicit zero-fee scenario can execute with zero permitted economic loss', () => {
  const planned = planCapitalSlice({ ...bounds, feeRate: '0', remainingCost: '0' });
  assert.equal(planned.minimum_price, '100');
  assert.equal(capitalFillLoss('100', planned.quantity, planned.minimum_price, '0'), '0');
});
test('a favorable executable price covers positive fees and rounding with zero loss budget', () => {
  const planned = planCapitalSlice({ ...bounds, instrument: { ...instrument, bid: '101', ask: '101.1', bid_depth: [{ price: '101', quantity: '1000' }] },
    needed: '10', maximumNotional: '20', remainingCost: '0' });
  assert.ok(units(planned.minimum_price) <= units('101'));
  const perFillQuantity = units(instrument.size_increment), moneyQuantum = 10n ** 10n;
  let loss = 0n;
  for (let filled = 0n; filled < units(planned.quantity); filled += perFillQuantity) {
    const exactFee = multiply(multiply(perFillQuantity, units(planned.minimum_price)), units('0.001'));
    const whole = exactFee / moneyQuantum, remainder = exactFee % moneyQuantum;
    const roundedFee = (whole + (remainder * 2n > moneyQuantum || remainder * 2n === moneyQuantum && whole % 2n === 1n ? 1n : 0n)) * moneyQuantum;
    loss += units(capitalFillLoss('100', decimal(perFillQuantity), planned.minimum_price, decimal(roundedFee)));
  }
  assert.equal(loss, 0n);
});
test('a flat price cannot satisfy zero economic loss when actual taker fees are positive', () => {
  assert.throws(() => planCapitalSlice({ ...bounds, remainingCost: '0' }), /capital_price_floor_unavailable/);
});
