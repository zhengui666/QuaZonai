import assert from 'node:assert/strict';
import test from 'node:test';
import { sampleCounts } from '../src/parameter-scans.ts';
import { nativeCurve } from '../src/projections.ts';

const table = { format: 'qz.native-table/v1', conversationId: 1, taskId: 2, rowCount: 1000000, pages: [{ key: 'original', start: 0, count: 1000000 }] };
test('scan counts retain paged native totals without loading rows or inventing missing counts', () => {
  assert.deepEqual(sampleCounts({ canonical_result: { orders: table, fills: [{}], positions: [] } }), { orders: '1000000', fills: '1', positions: '0' });
  assert.deepEqual(sampleCounts({ canonical_result: {} }), {});
});
test('paged equity is explicitly not loaded, not reported missing or replaced by an empty curve', () => {
  assert.throws(() => nativeCurve({ canonical_result: { portfolio_snapshots: table } }, 'USD'), /native_equity_pages_not_loaded/);
  assert.throws(() => nativeCurve({ canonical_result: { portfolio_snapshots: [] } }, 'USD'), /native_shared_account_equity_missing/);
  assert.deepEqual(nativeCurve({ canonical_result: { portfolio_snapshots: [{ ts_event: '1000000000', total_equity: ['5000.123456789 USD'] }] } }, 'USD'), [{ at: '1970-01-01T00:00:01.000000000Z', value: '5000.123456789' }]);
});
