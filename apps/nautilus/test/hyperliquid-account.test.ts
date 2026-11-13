import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DECIMAL_SCALE, decimal, ledgerAmount, ledgerTransfer, parseHyperliquidAccount, readHyperliquidLedger } from '../src/hyperliquid-account.ts';
import type { HyperliquidLedgerRow } from '../src/hyperliquid-account.ts';

const row = (delta: Record<string, unknown>): HyperliquidLedgerRow => ({ time: 1_794_398_400_000, hash: 'original-ledger-reference', delta });
test('official class-transfer directions preserve the raw platform amount and source row', () => {
  // The official SDK cassette retains this serialized floating-point tail.
  const raw = '2684105.0099999998';
  const transfer = row({ type: 'accountClassTransfer', toPerp: true, usdc: raw });
  const original = structuredClone(transfer);
  const outbound = ledgerTransfer(transfer), inbound = ledgerTransfer(row({ ...transfer.delta, toPerp: false }));
  const exact = 2_684_105n * DECIMAL_SCALE + 9_999_999_800_000_000n;
  assert.deepEqual(outbound, { direction: 'TO_WITHDRAWAL', amount: exact });
  assert.deepEqual(inbound, { direction: 'TO_SPOT', amount: exact });
  assert.equal(decimal(outbound.amount), raw);
  assert.notEqual(outbound.amount, 2_684_105n * DECIMAL_SCALE + 10_000_000_000_000_000n);
  assert.deepEqual(transfer, original);
  assert.equal(ledgerTransfer(row({ type: 'accountClassTransfer', toPerp: false, usdc: '0.000000000000000001' })).amount, 1n);
});
test('class transfers require the actual platform type, boolean direction and positive decimal source amount', () => {
  for (const toPerp of [undefined, null, 0, 1, 'true', 'false']) assert.throws(() => ledgerTransfer(row({ type: 'accountClassTransfer', toPerp, usdc: '100' })), /class_transfer_required/);
  assert.throws(() => ledgerTransfer(row({ type: 'withdraw', toPerp: true, usdc: '100' })), /class_transfer_required/);
  for (const usdc of ['0', '-1', '1e3', 'NaN', 100, '0.0000000000000000001']) assert.throws(() => ledgerTransfer(row({ type: 'accountClassTransfer', toPerp: true, usdc })), /capital_ledger/);
});
test('withdrawals retain the reported USDC source amount without inventing a fee or net payout conversion', () => {
  const withdrawal = row({ type: 'withdraw', usdc: '100.000000000000000001', fee: '1' });
  assert.equal(ledgerAmount(withdrawal, 'WITHDRAWAL'), 100n * DECIMAL_SCALE + 1n);
  assert.throws(() => ledgerAmount(row({ type: 'accountClassTransfer', toPerp: true, usdc: '100' }), 'WITHDRAWAL'), /withdrawal_required/);
});
test('cash evidence is dated from request start and response latency does not extend its lifetime', () => {
  const started = new Date(Date.now() - 20_000).toISOString();
  const proof = parseHyperliquidAccount('0x' + '1'.repeat(40), { balances: [{ coin: 'USDC', total: '100', hold: '0' }] },
    { assetPositions: [], crossMarginSummary: { totalMarginUsed: '0' }, withdrawable: '0' }, 'default', { userSpotCrossRate: '0.001' }, started);
  assert.equal(proof.asof, started);
  assert.equal(Date.parse(proof.valid_until), Date.parse(started) + 10_000);
  assert.ok(Date.parse(proof.valid_until) < Date.now());
  assert.ok(Date.parse(proof.received_at) >= Date.parse(started) + 20_000);
  assert.equal(proof.withdrawable, '0'); assert.equal(proof.spot_free, null);
});

const ledgerRow = (time: number, hash = `ledger-${time}`): HyperliquidLedgerRow => ({ time, hash, delta: { type: 'accountClassTransfer', toPerp: false, usdc: '1' } });
test('ledger pagination continues past 2000 rows and returns the reverse-transfer tail only after completion', async () => {
  const source = Array.from({ length: 2001 }, (_, index) => ledgerRow(index + 1));
  const calls: [number, number][] = [];
  const result = await readHyperliquidLedger(0, 3000, async (start, end) => { calls.push([start, end]); return source.filter(row => row.time >= start && row.time <= end).slice(0, 2000); });
  assert.equal(result.length, 2001); assert.equal(result.at(-1)?.hash, 'ledger-2001');
  assert.deepEqual(calls, [[0, 3000], [2000, 3000]], 'The boundary millisecond is inclusive, with a fixed end time.');
});
test('the documented 500 threshold retains every event at an overlapping millisecond', async () => {
  const source = [...Array.from({ length: 498 }, (_, index) => ledgerRow(index + 1)),
    ...['a', 'b', 'c', 'd'].map(suffix => ledgerRow(499, `boundary-${suffix}`)), ledgerRow(500), ledgerRow(501)];
  const calls: number[] = [];
  const result = await readHyperliquidLedger(0, 1000, async start => { calls.push(start); return source.filter(row => row.time >= start).slice(0, 500); });
  assert.equal(result.length, source.length); assert.deepEqual(calls, [0, 499]);
  assert.deepEqual(result.filter(row => row.time === 499).map(row => row.hash), ['boundary-a', 'boundary-b', 'boundary-c', 'boundary-d']);
});
test('overlapping raw identities compare canonical content, without depending on JSON key order', async () => {
  const first = Array.from({ length: 500 }, (_, index) => ledgerRow(index + 1));
  let calls = 0;
  const result = await readHyperliquidLedger(0, 1000, async () => ++calls === 1 ? first : [
    { hash: 'ledger-500', delta: { usdc: '1', toPerp: false, type: 'accountClassTransfer' }, time: 500 }, ledgerRow(501),
  ]);
  assert.equal(result.length, 501); assert.equal(result.filter(row => row.hash === 'ledger-500').length, 1);
});
test('conflicting overlap identities reject the complete read instead of publishing a partial ledger', async () => {
  const first = Array.from({ length: 500 }, (_, index) => ledgerRow(index + 1));
  let calls = 0, published = false;
  await assert.rejects(readHyperliquidLedger(0, 1000, async () => ++calls === 1 ? first : [
    { ...ledgerRow(500), delta: { type: 'accountClassTransfer', toPerp: false, usdc: '2' } }, ledgerRow(501),
  ]).then(() => { published = true; }), /ledger_identity_conflict/);
  assert.equal(published, false);
});
test('a full boundary timestamp is unprovable and never skipped with startTime plus one', async () => {
  const saturated = Array.from({ length: 500 }, (_, index) => ledgerRow(100, `same-millisecond-${index}`));
  const starts: number[] = [];
  await assert.rejects(readHyperliquidLedger(100, 200, async start => { starts.push(start); return saturated; }), /timestamp_saturated/);
  assert.deepEqual(starts, [100]);
});
test('missing inclusive overlap, unordered pages and rows outside the requested range fail closed', async () => {
  const first = Array.from({ length: 500 }, (_, index) => ledgerRow(index + 1)); let calls = 0;
  await assert.rejects(readHyperliquidLedger(0, 1000, async () => ++calls === 1 ? first : [ledgerRow(501)]), /overlap_missing/);
  await assert.rejects(readHyperliquidLedger(0, 1000, async () => [ledgerRow(2), ledgerRow(1)]), /order_or_range_invalid/);
  await assert.rejects(readHyperliquidLedger(100, 200, async () => [ledgerRow(99)]), /order_or_range_invalid/);
  await assert.rejects(readHyperliquidLedger(100, 200, async () => [ledgerRow(201)]), /order_or_range_invalid/);
});
