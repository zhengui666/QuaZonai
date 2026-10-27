import assert from 'node:assert/strict';
import { test } from 'node:test';
import { schemas } from '../src/schema.ts';

test('CLI exposes the same explicit native request source used by service and Web', async () => {
  const value = await schemas('NativeTickFactorRequest');
  assert.equal(value.entry, 'NativeTickFactorRequest');
  assert.match(value.source!, /modality: 'TRADE_TICK'/);
  assert.match(value.source!, /horizon_cohorts: number/);
  assert.match(value.source!, /EVENT_TIME_PROXY_UNVERIFIED/);
});
