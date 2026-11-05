import assert from 'node:assert/strict';
import test from 'node:test';
import { adverseReviewRequest, nativeReviewReturnPanel } from '../src/review-account-science.ts';

test('registered BAR delay/slippage stress retains the original venue fee source', () => {
  const fee = { adapter_kind: 'NAUTILUS_MAKER_TAKER', parameters: {} };
  const request = { selection: { bar_types: ['BTCUSD.VENUE-15-MINUTE-LAST-EXTERNAL'] }, settings: {
    fee_model: fee, fee_rates: [{ instrument_id: 'BTCUSD.VENUE', maker: '0.0001', taker: '0.0004' }],
    latency_model: { adapter_kind: 'NAUTILUS_STATIC_LATENCY', parameters: { base_latency_ns: '11', insert_latency_ns: '17', update_latency_ns: '19', cancel_latency_ns: '23' } },
    fill_model: { adapter_kind: 'NAUTILUS_DEFAULT_FILL', parameters: { prob_slippage: '0', random_seed: '42' } },
  } };
  const changed = adverseReviewRequest(request) as unknown as typeof request;
  assert.equal(changed.settings.latency_model.parameters.base_latency_ns, '900000000011');
  assert.equal(changed.settings.fill_model.parameters.prob_slippage, '1');
  assert.deepEqual(changed.settings.fee_model, request.settings.fee_model);
  assert.deepEqual(changed.settings.fee_rates, request.settings.fee_rates);
  assert.equal(request.settings.latency_model.parameters.base_latency_ns, '11');
  assert.throws(() => adverseReviewRequest({ ...request, selection: { bar_types: ['BTCUSD.VENUE-1-MONTH-LAST-EXTERNAL'] } }), /unsupported/);
});

test('account diagnostics require original native complete returns without curve fallbacks', () => {
  const native = { returns_kind: 'PORTFOLIO_DAILY', returns_status: 'OK', returns: Array.from({ length: 30 }, (_, i) => ({ timestamp_ns: String(BigInt(i)*86_400_000_000_000n), value: i%2===0 ? 0.01 : -0.005, reason_code: null })) };
  assert.equal(nativeReviewReturnPanel(native).returns.length, 30);
  assert.throws(() => nativeReviewReturnPanel({ ...native, returns_status: 'INCOMPLETE' }), /required/);
  assert.throws(() => nativeReviewReturnPanel({ ...native, returns: [...native.returns].reverse() }), /incomplete/);
  assert.throws(() => nativeReviewReturnPanel({ canonical_result: { portfolio_snapshots: [{ total: '100' }] } }), /required/);
});
