import { randomUUID } from 'node:crypto';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, defineDoc, defineExtension, defineTask, Harness } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId, PhaseHandler } from '@earendil-works/pi-durable';
import type { NativeExecutionRequestV2, NativeExecutionReceiptV2, NativeRecoveryProfile, NativeJson, NativeCancellationFenceV2 } from '../../../packages/contracts/src/nautilus.ts';
import { openDurableStorage } from '../../research/src/durable-storage.ts';
import { canonicalJsonDigest } from '../../research/src/native-json.ts';
import type { RecoveryDriver } from './checkpoint-driver.ts';

const context = BACKGROUND_CONTEXT;
type Binding = { conversationId: number; taskId: number; request: JsonObject; cancelRequested: boolean };
const Executions = defineDoc<{ instanceId: string; requests: Record<string, Binding>; cancelledBeforeAdmission?: Record<string, boolean> }>({
  kind: 'qz.nautilus.executions', version: 1, scope: 'session',
  initial: () => ({ instanceId: randomUUID(), requests: {}, cancelledBeforeAdmission: {} }),
});
type Phase = 'prepare' | 'capture' | 'restore' | 'start' | 'poll' | 'sleep';
type Checkpoint = { [P in Phase]: {
  phase: P;
  generation: number | null; checkpointAt: string | null;
  nextCheckpointAt: number; wakeAt: number; blockedReason: string | null;
} }[Phase];
type Result = { status: 'COMPLETED'; result: NativeJson } | { status: 'FAILED'; error: string } | { status: 'CANCELLED' };
const terminal = new Set(['COMPLETED', 'FAILED', 'CANCELLED']);

/** The official task owns scheduling and checkpoints. The driver only performs
 * registered process operations; it does not decide to create or replay work. */
export async function openNativeExecutions(database: string, driver?: RecoveryDriver) {
  const run: PhaseHandler<NativeExecutionRequestV2, Checkpoint, Checkpoint, Result, object> = async (task, runtime, ctx) => {
      const state = task.state.checkpoint; const phase = state.phase;
      const request = task.input as NativeExecutionRequestV2;
      try {
        if (!driver) throw new Error('nautilus_recovery_driver_unavailable');
        const binding = (await runtime.snapshot(Executions, ctx))?.requests[request.requestId] as Binding | undefined;
        if (!binding || binding.taskId !== task.id) throw new Error('nautilus_execution_binding_missing');
        // A persisted terminal completion takes precedence over a later stop.
        const inspection = await driver.inspect(request.requestId);
        if (inspection.completion && inspection.completion.signal === null) {
          const completed = inspection.completion;
          const result: Result = completed.exitCode === 0 && completed.signal === null && completed.result !== undefined
            ? { status: 'COMPLETED', result: completed.result }
            : { status: 'FAILED', error: completed.error ?? `native_exit_${completed.exitCode ?? completed.signal ?? 'unknown'}` };
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), ctx); return;
        }
        if (binding.cancelRequested) {
          await driver.cancel(request.requestId);
          const stopped = await driver.inspect(request.requestId);
          if (!['absent', 'stopped', 'prepared'].includes(stopped.phase)) throw new Error('nautilus_cancellation_unsettled');
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { status: 'CANCELLED' } } }), ctx); return;
        }
        const profile = await driver.profile();
        if (!profile || profile.profileId !== request.recoveryProfileId || !profile.verifiedCommands.includes(request.command)) throw new Error('nautilus_recovery_profile_unavailable');
        const commit = async (checkpoint: Checkpoint) => runtime.commit(() => ({ status: 'running', checkpoint }), ctx);
        if (inspection.completion?.signal) {
          // OOM and forced termination interrupt an invocation; they do not
          // establish a scientific failure. Restore the same internal state.
          if (state.generation === null) throw new Error('nautilus_interrupted_without_checkpoint');
          await driver.restore(request.requestId, state.generation);
          await driver.start(request.requestId);
          await commit({ ...state, phase: 'poll', nextCheckpointAt: runtime.now() + profile.checkpointIntervalMs, blockedReason: null }); return;
        }
        if (phase === 'prepare') {
          await driver.prepare(request);
          await commit({ ...state, phase: 'capture', blockedReason: null }); return;
        }
        if (phase === 'capture') {
          const generation = (state.generation ?? -1) + 1;
          const manifest = await driver.checkpoint(request.requestId, generation);
          // The process images and workdir are durable before this pointer.
          await commit({ ...state, phase: 'restore', generation, checkpointAt: manifest.createdAt, blockedReason: null }); return;
        }
        if (phase === 'restore') {
          if (state.generation === null) throw new Error('nautilus_checkpoint_missing');
          await driver.restore(request.requestId, state.generation);
          await commit({ ...state, phase: 'start', blockedReason: null }); return;
        }
        if (phase === 'start') {
          await driver.start(request.requestId);
          await commit({ ...state, phase: 'poll', nextCheckpointAt: runtime.now() + profile.checkpointIntervalMs, blockedReason: null }); return;
        }
        if (phase === 'sleep') {
          await runtime.sleep(state.wakeAt, ctx);
          await commit({ ...state, phase: 'poll' }); return;
        }
        if (inspection.phase === 'absent' || inspection.phase === 'stopped') {
          if (state.generation === null) throw new Error('nautilus_checkpoint_missing');
          await commit({ ...state, phase: 'restore' }); return;
        }
        if (inspection.phase !== 'running') throw new Error('nautilus_execution_outcome_unknown');
        if (runtime.now() >= state.nextCheckpointAt) { await commit({ ...state, phase: 'capture' }); return; }
        await commit({ ...state, phase: 'sleep', wakeAt: runtime.now() + 250, blockedReason: null });
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        // Infrastructure failure is not a scientific failure. Keep the exact
        // interrupted phase and original invocation for the next official run.
        const reason = error instanceof Error ? error.message : 'nautilus_recovery_unavailable';
        if (reason === 'checkpoint_candidate_incomplete') {
          // No candidate pointer was committed. Return to the previous complete
          // generation; before the start gate, only the waiting baseline may be
          // recreated, never an already-running scientific operation.
          await runtime.commit(() => ({ status: 'running', checkpoint: { ...state,
            phase: state.generation === null ? 'prepare' : 'restore', blockedReason: reason } }), ctx);
          return;
        }
        await runtime.commit(() => ({ status: 'running', checkpoint: { ...state, blockedReason: reason } }), ctx);
        await runtime.sleep(runtime.now() + 1_000, ctx);
      }
    };
  const Task = defineTask<NativeExecutionRequestV2, Checkpoint, Result>({
    name: 'qz.nautilus.execution', version: 2,
    initial: () => ({ phase: 'prepare', generation: null, checkpointAt: null, nextCheckpointAt: 0, wakeAt: 0, blockedReason: null }),
    phases: { prepare: run, capture: run, restore: run, start: run, poll: run, sleep: run },
    async abort(_task, runtime, ctx) {
      // Administrative abort must not certify remote cancellation. Only the
      // durable owner-stop path above may publish a CANCELLED receipt.
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx);
    },
  });
  const registry = createRegistry(); registry.install(defineExtension({ name: 'qz.nautilus', tasks: [Task] }));
  const storage = await openDurableStorage(database);
  let harness: Harness;
  try { harness = await Harness.open(storage, { models: createModels(), registry }, context); }
  catch (error) { await storage.close(context); throw error; }
  const instanceId = await harness.commit(async tx => (await tx.doc(Executions)).instanceId, context);
  let accepting = true;

  async function profile(): Promise<NativeRecoveryProfile | null> { return driver ? await driver.profile() : null; }
  async function read(requestId: string): Promise<NativeExecutionReceiptV2 | null> {
    return harness.commit(async tx => {
      const binding = (await tx.doc(Executions)).requests[requestId]; if (!binding) return null;
      const task = await tx.task(binding.taskId as TaskId); if (!task) throw new Error('nautilus_execution_task_missing');
      const checkpoint = 'checkpoint' in task.state ? task.state.checkpoint as Checkpoint : null;
      const base = { schemaVersion: 2 as const, instanceId, requestId, taskId: binding.taskId,
        recovery: { profileId: String(binding.request.recoveryProfileId), phase: checkpoint?.phase ?? task.state.status,
          generation: checkpoint?.generation ?? null, checkpointAt: checkpoint?.checkpointAt ?? null, blockedReason: checkpoint?.blockedReason ?? null } };
      if (task.state.status === 'terminal') {
        if (task.state.outcome.status !== 'completed') return { ...base, status: 'WAITING' as const, recovery: { ...base.recovery, blockedReason: 'nautilus_execution_administratively_interrupted' } };
        return { ...base, ...task.state.outcome.result as Result };
      }
      return { ...base, status: checkpoint?.blockedReason ? 'WAITING' as const : checkpoint?.phase === 'prepare' ? 'ACCEPTED' as const : 'RUNNING' as const };
    }, context);
  }
  async function submit(request: NativeExecutionRequestV2) {
    if (!accepting) throw Object.assign(new Error('nautilus_worker_closing'), { statusCode: 503 });
    // Idempotent receipt reads remain available even if the original profile is
    // temporarily unavailable after a restart or node repair.
    const prior = await harness.commit(async tx => {
      const document = await tx.doc(Executions);
      if (document.cancelledBeforeAdmission?.[request.requestId]) throw Object.assign(new Error('nautilus_execution_cancelled_before_admission'), { statusCode: 409 });
      const binding = document.requests[request.requestId];
      if (binding && canonicalJsonDigest(binding.request) !== canonicalJsonDigest({ ...request })) throw Object.assign(new Error('nautilus_request_identity_conflict'), { statusCode: 409 });
      return Boolean(binding);
    }, context);
    if (prior) return (await read(request.requestId))!;
    if (request.instanceId !== instanceId) throw Object.assign(new Error('nautilus_instance_changed'), { statusCode: 409 });
    const capability = await profile();
    if (!capability || capability.profileId !== request.recoveryProfileId || !capability.verifiedCommands.includes(request.command)) throw Object.assign(new Error('nautilus_internal_checkpoint_unverified'), { statusCode: 503 });
    await harness.commit(async tx => {
      const document = await tx.doc(Executions);
      // DELETE can arrive while the POST is waiting for profile/transport I/O.
      // Both admission and this fence are serialized by the official commit.
      if (document.cancelledBeforeAdmission?.[request.requestId]) throw Object.assign(new Error('nautilus_execution_cancelled_before_admission'), { statusCode: 409 });
      const existing = document.requests[request.requestId];
      if (existing) { if (canonicalJsonDigest(existing.request) !== canonicalJsonDigest({ ...request })) throw Object.assign(new Error('nautilus_request_identity_conflict'), { statusCode: 409 }); return; }
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const taskId = await tx.createTask(Task, request, { conversationId: conversation.id, ownership: { kind: 'conversation' } });
      document.requests[request.requestId] = { conversationId: conversation.id, taskId, request: JSON.parse(JSON.stringify(request)) as JsonObject, cancelRequested: false };
    }, context);
    harness.resume(); return (await read(request.requestId))!;
  }
  async function cancel(requestId: string) {
    const prior = await read(requestId); if (prior && terminal.has(prior.status)) return prior;
    const fenced = await harness.commit(async tx => {
      const document = await tx.doc(Executions); const binding = document.requests[requestId];
      if (binding) { binding.cancelRequested = true; return false; }
      (document.cancelledBeforeAdmission ??= {})[requestId] = true; return true;
    }, context);
    // This fact certifies atomically fenced non-admission, rather than making
    // up a scientific Task or claiming to have terminated an unknown process.
    if (fenced) return { schemaVersion: 2, instanceId, requestId, cancellation: 'FENCED_NOT_ADMITTED' } satisfies NativeCancellationFenceV2;
    harness.resume();
    // The HTTP caller may detach; this only waits for the already durable stop.
    while (true) { const receipt = await read(requestId); if (!receipt || terminal.has(receipt.status) || receipt.recovery.blockedReason) return receipt; await new Promise(resolve => setTimeout(resolve, 25)); }
  }
  harness.resume();
  return { instanceId, profile, read, submit, cancel, async close() { accepting = false; await harness.close(context); } };
}
