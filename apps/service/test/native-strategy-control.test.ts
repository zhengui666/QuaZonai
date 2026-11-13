import assert from 'node:assert/strict';
import test from 'node:test';
import { nativeStrategyControl, assertSameNativeStrategyControl } from '../../../packages/contracts/src/native-strategy-control.ts';

const original = nativeStrategyControl({ schemaVersion: 1, requestId: 'original-request', action: 'START', strategyId: 'OWNER-001',
  binding: { sourceId: 'source-A', environmentId: 'environment-A', buildId: 'build-A', nativeSessionId: 'session-A' } });

test('original native control identity is preserved without currency or implicit config', () => {
  assertSameNativeStrategyControl(original, structuredClone(original));
  assert.deepEqual(nativeStrategyControl(original), original);
  for (const extra of [{ currency: 'USDC' }, { config: {} }, { accountId: 'HL-001' }]) {
    assert.throws(() => nativeStrategyControl({ ...original, ...extra }), /invalid/);
  }
});

test('same request ID cannot change target, operation, session, source or selected build', () => {
  for (const field of ['requestId', 'strategyId'] as const) {
    assert.throws(() => assertSameNativeStrategyControl(original, { ...original, [field]: 'other' }), /identity_mismatch/);
  }
  assert.throws(() => assertSameNativeStrategyControl(original, { ...original, action: 'STOP' }), /identity_mismatch/);
  for (const field of ['sourceId', 'environmentId', 'buildId', 'nativeSessionId'] as const) {
    assert.throws(() => assertSameNativeStrategyControl(original, { ...original, binding: { ...original.binding, [field]: 'other' } }), /identity_mismatch/);
  }
});

test('missing or unknown identity is not silently completed or rebound', () => {
  for (const value of [null, {}, { ...original, binding: {} }, { ...original, action: 'RECOVER' }]) {
    assert.throws(() => assertSameNativeStrategyControl(original, value), /invalid/);
  }
  const spaced = nativeStrategyControl({ ...original, requestId: ' original-request ' });
  assert.equal(spaced.requestId, ' original-request ');
  assert.throws(() => assertSameNativeStrategyControl(original, spaced), /identity_mismatch/);
});


test('action must be an actual string enum, never coerced', () => {
  for (const action of [['START'], ['STOP'], { toString: () => 'START' }, 1, null, true]) {
    assert.throws(() => nativeStrategyControl({ ...original, action }), /invalid/);
  }
  for (const action of ['START', 'STOP'] as const) {
    assert.equal(nativeStrategyControl({ ...original, action }).action, action);
  }
});
