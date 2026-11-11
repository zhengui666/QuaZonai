import { defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, Tx } from '@earendil-works/pi-durable';
import type { ConnectionConfig } from './config.ts';
import type { AccountControl } from './capital-exit.ts';
import { canonical } from '../../research/src/documents.ts';
import { accountKey, capitalStatusForBinding, control, readCapitalControl, recordCapitalControl, recordCapitalControlFailure, transportBindingKey } from './capital-exit.ts';
import { acceptSnapshot, downstreamRequest } from './downstream.ts';
import { Application, counter, json, nowNs, observation, Problem } from './facts.ts';

interface WatchControl extends AccountControl {
  conversationId?: number;
  monitorTaskId?: number;
  watchSnapshot?: JsonObject;
  watchSnapshotReceivedAtNs?: string;
}
interface WatchInput { connection: ConnectionConfig; currency: string }
type WatchCheckpoint = { phase: 'read' } | { phase: 'wait'; wakeAt: number };
export const CAPITAL_WATCH_INTERVAL_MS = 5000;
export const CAPITAL_WATCH_READ_TIMEOUT_MS = 10_000;

/** READY ends preparation, while its cash protection and external reconciliation remain live. */
export function capitalWatchNeeded(value: AccountControl): boolean {
  return Object.values(value.commands).some(command => ['PENDING', 'UNCERTAIN'].includes(command.state)
    || command.state === 'ACCEPTED' && /[1-9]/.test(command.reserved))
    || (value.status?.exits ?? []).some(exit => !['COMPLETED', 'CLOSED'].includes(exit.state))
    || value.status?.reserved !== null && /[1-9]/.test(value.status?.reserved ?? '');
}

/** Call before the transaction's first table write. Account ownership never borrows a research conversation. */
export async function ensureCapitalWatch(tx: Tx, connection: ConnectionConfig, currency: string) {
  const app = await tx.doc(Application); app.accountControls ??= {};
  const key = accountKey(connection, app.accountControls); const value = control(app.accountControls[key]) as WatchControl;
  const previous = value.monitorTaskId === undefined ? undefined : await tx.task(value.monitorTaskId as TaskId);
  if (previous && previous.kind === 'qz.capital-exit-watch' && previous.background && previous.owner === undefined
    && !previous.abortRequested && !['terminal', 'completing'].includes(previous.state.status)) {
    value.conversationId = previous.conversationId;
    app.accountControls[key] = json(value);
    return { key, conversationId: value.conversationId as ConversationId, taskId: value.monitorTaskId as TaskId };
  }
  if (!value.conversationId) value.conversationId = (await tx.createConversation({ ownership: { kind: 'ownerless' } })).id;
  value.monitorTaskId = await tx.createTask(CapitalExitWatch, { connection: structuredClone(connection), currency }, {
    conversationId: value.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true,
  });
  app.accountControls[key] = json(value);
  return { key, conversationId: value.conversationId as ConversationId, taskId: value.monitorTaskId as TaskId };
}

function assertWatchSnapshot(connection: ConnectionConfig, currency: string, value: JsonObject, previous?: JsonObject, sourceId?: string | null) {
  if (value.clientId !== connection.clientId || value.accountId !== connection.accountId || value.mode !== connection.mode || value.currency !== currency
    || sourceId && value.accountSourceId !== sourceId) throw new Problem('capital_watch_account_identity_mismatch', 409);
  const sequence = BigInt(counter(value.sequence, 'account_sequence')); const clock = BigInt(counter(value.nativeClockNs, 'account_clock'));
  if (clock > BigInt(nowNs())) throw new Problem('capital_watch_account_clock_invalid', 409);
  if (previous && (sequence < BigInt(counter(previous.sequence, 'prior_account_sequence')) || clock < BigInt(counter(previous.nativeClockNs, 'prior_account_clock'))))
    throw new Problem('capital_watch_account_out_of_order', 409);
  if (previous && previous.accountSourceId !== value.accountSourceId) throw new Problem('capital_watch_account_identity_mismatch', 409);
  const physical = (snapshot: JsonObject) => canonical({ ...snapshot, clientId: null, accountId: null });
  if (previous && sequence === BigInt(counter(previous.sequence, 'prior_account_sequence')) && physical(value) !== physical(previous))
    throw new Problem('capital_watch_account_conflict', 409);
}

export const CapitalExitWatch = defineTask<WatchInput, WatchCheckpoint, null>({
  name: 'qz.capital-exit-watch', version: 1, initial: () => ({ phase: 'read' }),
  phases: {
    async read(task, runtime, context) {
      let active = false; let activeConnection = task.input.connection;
      await runtime.commit(async tx => {
        const app = await tx.doc(Application); app.accountControls ??= {};
        const key = accountKey(task.input.connection, app.accountControls); const value = control(app.accountControls[key]) as WatchControl;
        // A source alias can join a physical account after task creation. The account's retained monitor wins.
        if (value.monitorTaskId !== undefined && value.monitorTaskId !== task.id)
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        value.monitorTaskId = task.id; value.conversationId ??= runtime.conversationId;
        app.accountControls[key] = json(value);
        // A missing local cache is not proof that a native responsibility ended: reacquire it once.
        if (value.status && !capitalWatchNeeded(value)) return { status: 'terminal', outcome: { status: 'completed', result: null } };
        // Only successful source reads install a replacement binding or rotated credential.
        activeConnection = json(value.monitorConnection ?? task.input.connection) as unknown as ConnectionConfig;
        active = true; return undefined;
      }, context);
      if (!active) return;

      // Both operations are reads. Their source clocks remain distinct; no order or transfer can be initiated here.
      const timeoutSignal = AbortSignal.timeout(CAPITAL_WATCH_READ_TIMEOUT_MS);
      const readSignal = AbortSignal.any([runtime.signal, timeoutSignal]);
      const [capitalRead, accountRead] = await Promise.allSettled([
        readCapitalControl(activeConnection, task.input.currency, readSignal),
        downstreamRequest(activeConnection, 'account', 'GET', undefined, readSignal),
      ]);
      if (runtime.signal.aborted) throw runtime.signal.reason;
      const readTimedOut = timeoutSignal.aborted;
      await runtime.commit(async tx => {
        const app = await tx.doc(Application); app.accountControls ??= {};
        let key = accountKey(activeConnection, app.accountControls);
        const savedConnection = control(app.accountControls[key]).monitorConnection;
        const preferredConnection = savedConnection ? json(savedConnection) as unknown as ConnectionConfig : undefined;
        const retainNewerBinding = preferredConnection && canonical(json(preferredConnection)) !== canonical(json(activeConnection));
        let failure: string | undefined = readTimedOut ? 'capital_watch_read_timeout' : undefined;
        if (capitalRead.status === 'fulfilled') {
          try { key = recordCapitalControl(app.accountControls, activeConnection, capitalRead.value); }
          catch (error) { failure ??= error instanceof Problem ? error.message : 'capital_watch_control_invalid'; }
        } else failure ??= capitalRead.reason instanceof Problem ? capitalRead.reason.message : 'capital_watch_control_unavailable';
        let value = control(app.accountControls[key]) as WatchControl;
        // A late successful response from the old transport must not undo a concurrently verified replacement.
        if (retainNewerBinding) value.monitorConnection = preferredConnection;
        if (value.monitorTaskId !== undefined && value.monitorTaskId !== task.id)
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        value.monitorTaskId = task.id; value.conversationId ??= runtime.conversationId;
        if (accountRead.status === 'fulfilled') {
          try {
            assertWatchSnapshot(activeConnection, task.input.currency, accountRead.value, value.watchSnapshot, value.status?.accountSourceId);
            const candidates = Object.values(app.observations).map(observation).filter(binding => accountKey(binding.connection, app.accountControls) === key);
            const original = candidates.find(binding => transportBindingKey(binding.connection) === transportBindingKey(activeConnection));
            if (original) { acceptSnapshot(original, accountRead.value, candidates); original.view.error = null; app.observations[original.view.id] = json(original); }
            value.watchSnapshot = accountRead.value; value.watchSnapshotReceivedAtNs = nowNs();
          } catch (error) { failure ??= error instanceof Problem ? error.message : 'capital_watch_account_invalid'; }
        } else failure ??= accountRead.reason instanceof Problem ? accountRead.reason.message : 'capital_watch_account_unavailable';
        app.accountControls[key] = json(value);
        if (failure) {
          recordCapitalControlFailure(app.accountControls, activeConnection, failure);
          value = control(app.accountControls[key]) as WatchControl;
        }
        for (const raw of Object.values(app.observations)) {
          const binding = observation(raw); if (accountKey(binding.connection, app.accountControls) !== key) continue;
          binding.view.capitalExit = capitalStatusForBinding(value, binding.connection);
          app.observations[binding.view.id] = json(binding);
        }
        if (!failure && !capitalWatchNeeded(value)) return { status: 'terminal', outcome: { status: 'completed', result: null } };
        return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + CAPITAL_WATCH_INTERVAL_MS } };
      }, context);
    },
    async wait(task, runtime, context) {
      await runtime.sleepUntil(task.state.checkpoint.wakeAt ?? runtime.now(), { phase: 'read' }, context);
    },
  },
  async abort(_task, runtime, context) {
    // Aborting a local reader does not remove an account reservation or settle its native orders.
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});
