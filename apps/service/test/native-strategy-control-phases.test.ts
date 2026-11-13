import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineTask, defineExtension } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { Application, json } from '../src/facts.ts';
import { nativeStrategyControlPhases, admitNativeControlTask, assertNativeControlTask, type NativeControlInput } from '../src/native-strategy-control-phases.ts';
import { nativeStrategyControl } from '../../../packages/contracts/src/native-strategy-control.ts';

test('existing account task identity checkpoints UNKNOWN before queue send and only reads after SQLite reopen', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-control-phase-'));
  const original = nativeStrategyControl({ schemaVersion: 1, requestId: 'retained-id', action: 'START', strategyId: 'SECOND-002', binding: { sourceId: 'source', environmentId: 'environment', buildId: 'build', nativeSessionId: 'session' } });
  let sent = 0; let reads = 0; let entered = false;
  const models = createModels(); models.setProvider(fauxProvider().provider);
  // Test-only installation under the EXISTING name. Production registration and
  // transport remain unchanged. Queue boundary is controlled, not a native Actor.
  const Task = defineTask<NativeControlInput, { phase: 'send' | 'read' }, null>({
    name: 'qz.account-strategy-command', version: 1, initial: () => ({ phase: 'send' }),
    phases: nativeStrategyControlPhases({
      async send(command, signal) {
        sent++; assert.deepEqual(command, original);
        const app = await host.harness.snapshot(Application, context);
        assert.equal(app!.strategyCommands!.retained!.status, 'UNCERTAIN');
        const rows = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.account-strategy-command' }, 10), context);
        assert.equal((rows.items[0]!.state as unknown as { checkpoint: { phase: string } }).checkpoint.phase, 'read');
        entered = true;
        // The queue has consumed the original command, but no reply arrives.
        // Host shutdown aborts this waiter, not the already consumed command.
        await new Promise<void>((_resolve, reject) => signal.addEventListener('abort', () => reject(new Error('reply_lost_on_host_shutdown')), { once: true }));
        throw new Error('unreachable');
      },
      async read(command) { reads++; assert.deepEqual(command, original); return json({ command, status: 'UNKNOWN', error: 'original reply unavailable' }); },
    }),
    abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
  });
  const options = { database: join(directory, 'original.sqlite'), models, extensions: [defineExtension({ name: 'test-original-account-command', tasks: [Task] })] };
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    const admission = (tx: Parameters<typeof admitNativeControlTask>[0]) => admitNativeControlTask(tx, Task, { key: 'retained', original }, { conversationId: root.id, ownership: { kind: 'conversation' } });
    const id = await root.commit(admission, context);
    assert.equal(await root.commit(admission, context), id, 'duplicate admission returns the original task');
    await assert.rejects(root.commit(tx => admitNativeControlTask(tx, Task, { key: 'retained', original: { ...original, strategyId: 'WRONG' } }, { conversationId: root.id, ownership: { kind: 'conversation' } }), context), /identity_mismatch/);
    await assert.rejects(root.commit(tx => admitNativeControlTask(tx, Task, { key: 'retained', original }, { conversationId: (Number(root.id) + 1) as typeof root.id, ownership: { kind: 'conversation' } }), context), /identity_mismatch/);
    const retained = (await host.harness.snapshot(Application, context))!.strategyCommands!.retained!;
    assert.throws(() => assertNativeControlTask(retained, { id, conversationId: root.id, owner: 999, input: { key: 'retained', original } }), /identity_mismatch/);
    assert.throws(() => assertNativeControlTask(retained, { id: Number(id) + 1, conversationId: root.id, input: { key: 'retained', original } }), /identity_mismatch/);
    host.harness.resume();
    for (let i = 0; i < 200 && !entered; i++) await delay(10);
    assert.equal(entered, true); assert.equal(sent, 1);
    await host.close(); host = await openResearch(options);
    await host.harness.waitForTask(id, context);
    assert.equal(sent, 1); assert.equal(reads, 1);
    const app = await host.harness.snapshot(Application, context);
    assert.equal(app!.strategyCommands!.retained!.status, 'UNCERTAIN');
    assert.deepEqual(app!.strategyCommands!.retained!.original, original);
    await host.close(); host = await openResearch(options);
    await host.harness.waitForTask(id, context);
    assert.equal(sent, 1, 'terminal unknown remains original and is never retried');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
