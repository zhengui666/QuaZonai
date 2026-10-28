import assert from 'node:assert/strict';
import { test } from 'node:test';
import { schemas } from '../src/schema.ts';

test('CLI exposes the same explicit native request source used by service and Web', async () => {
  const value = await schemas('NativeTickFactorRequest');
  assert.equal(value.entry, 'NativeTickFactorRequest');
  assert.match(value.source!, /modality: 'TRADE_TICK'/);
  assert.match(value.source!, /horizon_cohorts: number/);
  assert.match(value.source!, /kind: 'FIXED_EVENT_TIME'; horizon_ns: string/);
  assert.match(value.source!, /EVENT_TIME_PROXY_UNVERIFIED/);
});


test('owner economic replay source contracts are available without a native-schema hand mirror', async () => {
  const value = await schemas('TickEconomicReplayStart');
  assert.equal(value.entry, 'TickEconomicReplayStart');
  assert.match(value.source!, /factorId: string/);
  assert.match(value.source!, /NativeTickEconomicRequest/);
  assert.match(value.source!, /perp_pnl_rounding_bound_usdc/);
});
