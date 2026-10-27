import type { Context } from '@earendil-works/chord';
import type { ConversationId, Harness, JsonObject, Tx } from '@earendil-works/pi-durable';
import { canonical } from './documents.ts';
import { NativeRemoteUncertainty, remoteNativeExecutionReceipt, validateRemoteTarget } from './native-execution.ts';

export interface NativeRemoteTaskReference { id: number; conversationId: number }
const terminal = new Set(['COMPLETED', 'FAILED', 'CANCELLED']);
const admissionRejected = (reference: JsonObject) => reference.status === 'REJECTED' && reference.admission === 'REJECTED'
  && ['nautilus_tick_capacity_exhausted', 'nautilus_tick_resource_limits_unavailable'].includes(String(reference.error));
const settled = (reference: JsonObject) => terminal.has(String(reference.status)) || admissionRejected(reference);
function validReference(reference: JsonObject, taskId: number) {
  if (reference.taskId !== taskId || typeof reference.requestId !== 'string' || !/^[a-f0-9]{64}$/.test(reference.requestId)) return false;
  try { validateRemoteTarget(reference.target); return true; } catch { return false; }
}

/** Reconcile only existing original references by GET, with one total deadline.
 * Unavailable, mismatched and nonterminal receipts keep their original state.
 * No transport diagnostic, credentials, native result or replacement identity
 * is exposed or persisted by this read-only operation. */
export async function reconcileNativeRemoteTasks(harness: Harness, tasks: readonly NativeRemoteTaskReference[], context: Context): Promise<void> {
  const deadline = AbortSignal.timeout(5000);
  const signal = context.abortSignal ? AbortSignal.any([deadline, context.abortSignal]) : deadline;
  const seen = new Set<string>();
  for (const task of tasks) {
    const key = `${task.conversationId}:${task.id}`; if (seen.has(key)) continue; seen.add(key);
    if (signal.aborted) return;
    const facts = await harness.snapshot(NativeRemoteUncertainty, task.conversationId as ConversationId, String(task.id), context);
    for (const reference of facts?.requests ?? []) {
      if (signal.aborted) return;
      if (settled(reference) || !validReference(reference, task.id)) continue;
      try {
        const result = await remoteNativeExecutionReceipt(validateRemoteTarget(reference.target), String(reference.requestId), signal);
        if (!terminal.has(result.status)) continue;
        await harness.commit(async tx => {
          const current = await tx.doc(NativeRemoteUncertainty, task.conversationId as ConversationId, String(task.id), null);
          const stored = current.requests.find(item => item.requestId === reference.requestId && item.taskId === task.id
            && canonical(item.target ?? null) === canonical(reference.target ?? null));
          if (!stored || settled(stored)) return;
          stored.status = result.status;
          stored.error = result.status === 'FAILED' ? 'nautilus_execution_failed' : null;
          stored.reconciledAt = new Date().toISOString();
        }, context);
      } catch { /* Preserve the original uncertainty; callers fence it in their final transaction. */ }
    }
  }
}

/** A local terminal Task does not establish remote termination. Check retained
 * original-task families inside the same transaction that records deletion. */
export async function assertNativeRemoteTaskSettled(tx: Tx, conversationId: number, taskId: number): Promise<void> {
  const facts = await tx.doc(NativeRemoteUncertainty, conversationId as ConversationId, String(taskId), null);
  if (facts.requests.some(item => !validReference(item, taskId) || !settled(item))) {
    throw Object.assign(new Error('native_remote_execution_unsettled'), { statusCode: 409 });
  }
}
