/** Phases for the existing account command task. No new task kind,
 * ledger or session discovery. The scoped account caller must supply
 * a verified session-bound transport; these phases do not create native nodes. */
import type { JsonObject, PhaseHandler, Task, Tx, TaskOptions, TaskId } from '@earendil-works/pi-durable';
import { nativeStrategyControl, assertSameNativeStrategyControl, type NativeStrategyControl } from '../../../packages/contracts/src/native-strategy-control.ts';
import { Application, object, json, Problem } from './facts.ts';
import { prepareNativeStrategyControl, acceptNativeStrategyControl } from './native-strategy-control-state.ts';

// Optional only so the original Task can retain historical key-only inputs.
// This native branch always rejects a missing original instead of upgrading it.
export type NativeControlInput = { key: string; original?: NativeStrategyControl };
type Phase = PhaseHandler<NativeControlInput, { phase: 'send' | 'read' }, { phase: 'send' | 'read' }, null, {}>;
type BoundTask = { id: number; conversationId: number; owner?: number; input: NativeControlInput };
export function assertNativeControlTask(recordValue: unknown, task: BoundTask): void {
  const record = object(recordValue);
  if (record.kind !== 'NATIVE_STRATEGY_CONTROL_V1' || record.taskId !== task.id || record.conversationId !== task.conversationId || record.ownerTaskId !== (task.owner ?? null)) throw new Problem('native_control_task_identity_mismatch');
  assertSameNativeStrategyControl(nativeStrategyControl(record.original), nativeStrategyControl(task.input.original));
}
/** Called inside the original account command admission transaction. */
export async function admitNativeControlTask(tx: Tx, definition: Task<NativeControlInput, { phase: 'send' | 'read' }, null, {}>, input: NativeControlInput, options: TaskOptions): Promise<TaskId<null>> {
  if (definition.definition.name !== 'qz.account-strategy-command') throw new Problem('native_control_task_kind_invalid');
  const original = nativeStrategyControl(input.original);
  const app = await tx.doc(Application); app.strategyCommands ??= {};
  const existing = app.strategyCommands[input.key];
  if (existing) {
    const record = object(existing);
    const task = await tx.task(record.taskId as TaskId);
    if (!task || task.kind !== 'qz.account-strategy-command' || task.conversationId !== options.conversationId || task.owner !== (options.ownership.kind === 'task' ? options.ownership.taskId : undefined)) throw new Problem('native_control_task_identity_mismatch');
    assertNativeControlTask(record, { ...task, input: { key: input.key, original } });
    assertSameNativeStrategyControl(original, object(task.input).original);
    if (object(task.input).key !== input.key) throw new Problem('native_control_task_identity_mismatch');
    return task.id as TaskId<null>;
  }
  // Explicit conversation avoids inferring an owner from an unrelated caller.
  if (options.conversationId === undefined) throw new Problem('native_control_owner_required');
  const taskId = await tx.createTask(definition, { key: input.key, original }, options);
  app.strategyCommands[input.key] = json({ kind: 'NATIVE_STRATEGY_CONTROL_V1', taskId, conversationId: options.conversationId, ownerTaskId: options.ownership.kind === 'task' ? options.ownership.taskId : null, original, status: 'PENDING', receipt: null, error: null });
  return taskId;
}
export interface NativeStrategyControlTransport {
  send(command: NativeStrategyControl, signal: AbortSignal): Promise<JsonObject>;
  read(command: NativeStrategyControl, signal: AbortSignal): Promise<JsonObject>;
}
export function nativeStrategyControlPhases(transport: NativeStrategyControlTransport): { send: Phase; read: Phase } {
  const send: Phase = async (task, runtime, ctx) => {
    let prepared: Awaited<ReturnType<typeof prepareNativeStrategyControl>> | undefined;
    await runtime.commit(async tx => {
      const app = await tx.doc(Application);
      const record = object(app.strategyCommands?.[task.input.key]);
      assertNativeControlTask(record, task);
      prepared = await prepareNativeStrategyControl(tx, task.input.key, nativeStrategyControl(task.input.original));
      // One official Pi transaction commits BOTH intent uncertainty and the
      // read-only continuation. Never send before that transaction completes.
      return { status: 'running', checkpoint: { phase: 'read' } };
    }, ctx);
    if (!prepared?.send) return;
    try {
      const receipt = await transport.send(prepared.original, runtime.signal);
      await runtime.commit(async tx => {
        assertNativeControlTask((await tx.doc(Application)).strategyCommands?.[task.input.key], task);
        await acceptNativeStrategyControl(tx, task.input.key, receipt);
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, ctx);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      // Dispatch or reply failure cannot authorize another send. The committed
      // read checkpoint is retained; the scheduler may only query this identity.
    }
  };
  const read: Phase = async (task, runtime, ctx) => {
    const app = await runtime.snapshot(Application, ctx);
    const record = object(app?.strategyCommands?.[task.input.key]);
    assertNativeControlTask(record, task);
    const original = nativeStrategyControl(task.input.original);
    try {
      const receipt = await transport.read(original, runtime.signal);
      await runtime.commit(async tx => {
        assertNativeControlTask((await tx.doc(Application)).strategyCommands?.[task.input.key], task);
        await acceptNativeStrategyControl(tx, task.input.key, receipt);
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, ctx);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: error instanceof Error ? error.message : 'native_control_receipt_unavailable' } } }), ctx);
    }
  };
  return { send, read };
}
