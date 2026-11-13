import assert from 'node:assert/strict';
import test from 'node:test';
import { context, setup } from './support.ts';
import { Application, json } from '../src/facts.ts';
import { prepareNativeStrategyControl, acceptNativeStrategyControl } from '../src/native-strategy-control-state.ts';
import { nativeStrategyControl } from '../../../packages/contracts/src/native-strategy-control.ts';

test('original Pi doc retains unknown before dispatch and never prepares replay after SQLite reopen', async () => {
  const state = await setup();
  const command = nativeStrategyControl({ schemaVersion: 1, requestId: 'original-request', action: 'START', strategyId: 'OWNER-001',
    binding: { sourceId: 'original-source', environmentId: 'original-environment', buildId: 'original-build', nativeSessionId: 'original-session' } });
  const key = 'internal-native-control-original';
  const snapshot = async () => (await state.service.research.harness.snapshot(Application, context))!.strategyCommands![key]!;
  try {
    await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); app.strategyCommands ??= {};
      // Test admission input only. No production task/transport is enabled.
      app.strategyCommands[key] = json({ kind: 'NATIVE_STRATEGY_CONTROL_V1', original: command, status: 'PENDING', receipt: null, error: null });
    }, context);
    await assert.rejects(state.service.research.harness.commit(tx => prepareNativeStrategyControl(tx, key, { ...command, strategyId: 'OTHER-002' }), context), /identity_mismatch/);
    assert.equal((await snapshot()).status, 'PENDING');
    const prepared = await Promise.all([0, 1].map(() => state.service.research.harness.commit(tx => prepareNativeStrategyControl(tx, key, command), context)));
    assert.deepEqual(prepared.map(value => value.send).sort(), [false, true]);
    assert.equal((await snapshot()).status, 'UNCERTAIN', 'durable write precedes any hypothetical transport dispatch');
    await state.restart();
    assert.deepEqual((await snapshot()).original, command);
    assert.equal((await state.service.research.harness.commit(tx => prepareNativeStrategyControl(tx, key, command), context)).send, false);
    const complete = json({ command, status: 'COMPLETED', persisted: true, result: { strategyId: command.strategyId, success: true } });
    for (const invalid of [
      { ...complete, command: { ...command, action: 'STOP' } },
      { ...complete, command: { ...command, binding: { ...command.binding, nativeSessionId: 'replacement-session' } } },
      { ...complete, persisted: false },
      { ...complete, result: { strategyId: 'OTHER-002', success: true } },
    ]) await assert.rejects(state.service.research.harness.commit(tx => acceptNativeStrategyControl(tx, key, json(invalid)), context));
    assert.equal((await snapshot()).status, 'UNCERTAIN');
    await state.service.research.harness.commit(tx => acceptNativeStrategyControl(tx, key, complete), context);
    await state.restart();
    assert.equal((await snapshot()).status, 'COMPLETED');
    await state.service.research.harness.commit(tx => acceptNativeStrategyControl(tx, key, json({ command, status: 'UNKNOWN', error: 'late waiter loss' })), context);
    assert.equal((await snapshot()).status, 'COMPLETED');
    assert.equal((await state.service.research.harness.commit(tx => prepareNativeStrategyControl(tx, key, command), context)).send, false);
  } finally { await state.cleanup(); }
});
