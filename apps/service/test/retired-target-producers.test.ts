import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import { Application, json, observation } from '../src/facts.ts';
import { ObserveAccount } from '../src/science.ts';
import { AdjustAccount } from '../src/automatic.ts';
import { PmTargetTask } from '../src/pm-target.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

test('retained pre-send target tasks cannot submit new targets after native-only transition', async () => {
  const state = await setup(); const source = accountSourceFixture(); const remote = Fastify(); let posts = 0;
  remote.post('/targets', () => { posts++; return {}; });
  try {
    source.connection.url = await remote.listen({ host: '127.0.0.1', port: 0 });
    const tasks = await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application); const pm = await tx.doc(PmDocument);
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const ownership = { conversationId: conversation.id, ownership: { kind: 'conversation' as const }, background: true };
      const pendingReceipt = { id: 'never-sent', status: 'PENDING', expiresAt: '2099-01-01T00:00:00Z', externalId: null, acknowledgedAt: null };
      app.observations['unoffered-observation'] = json({ ...source.observation, connection: source.connection,
        view: { ...source.observation.view, id: 'unoffered-observation', receipt: pendingReceipt } });
      app.observations['unoffered-sizing'] = json({ ...source.observation, connection: source.connection,
        view: { ...source.observation.view, id: 'unoffered-sizing', automaticPolicy: { enabled: true, cashBuffer: '0', maximumSnapshotAgeSeconds: 5 },
          allocation: { status: 'SIZED', receipt: pendingReceipt, reason: null } } });
      // Exact pre-send checkpoint inputs only; no accepted order/fill or computed model result is claimed.
      pmState(pm).mandates['pre-send-only'] = { commands: { pending: { id: 'pending', revision: '0', state: 'PENDING' } }, artifacts: {} } as never;
      return [await tx.createTask(ObserveAccount, { id: 'unoffered-observation', connection: source.connection, target: {}, receiptId: 'never-sent', refresh: false }, ownership),
        await tx.createTask(AdjustAccount, { id: 'unoffered-sizing', binary: '/must-not-run', dataDir: state.directory }, ownership),
        await tx.createTask(PmTargetTask, { mandateId: 'pre-send-only', commandId: 'pending' }, ownership)];
    }, context);
    for (const task of tasks) await state.service.research.harness.waitForTask(task, context);
    const app = (await state.service.research.harness.snapshot(Application, context))!;
    assert.equal(observation(app.observations['unoffered-observation']).view.receipt!.status, 'NOT_OFFERED');
    assert.equal(observation(app.observations['unoffered-sizing']).view.allocation!.receipt!.status, 'NOT_OFFERED');
    assert.equal(pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates['pre-send-only']!.commands.pending!.state, 'NOT_OFFERED');
    assert.equal(posts, 0);
  } finally { await state.cleanup(); await remote.close(); }
});
