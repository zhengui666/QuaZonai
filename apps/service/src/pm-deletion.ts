import type { Cursor, Tx } from '@earendil-works/pi-durable';
import type { JsonValue } from '@earendil-works/chord';
import type { DeletionReceipt } from '../../../packages/contracts/src/index.ts';
import { Application, json, observation, Problem } from './facts.ts';
import { accountKey } from './capital-exit.ts';
import { captureAccount } from './pm-account.ts';
import { retirementAccountFlat } from './pm-account-native.ts';
import { externalMaintenanceBlockers } from './pm-maintenance.ts';
import { pmRecord } from './pm-facts.ts';
import type { PmState } from './pm-facts.ts';
import { retirementOutstandingTargets } from './pm-retirement.ts';
import { Requests, Study } from '../../research/src/documents.ts';
import { assertNativeRemoteTaskSettled } from '../../research/src/native-remote-receipts.ts';

export async function pmMandateTasks(tx: Tx, record: import('./pm-facts.ts').PmRecord) {
  const conversations = new Set([record.conversationId, ...Object.values(record.campaigns).map(item => item.runtime.conversationId)]);
  const tasks: Awaited<ReturnType<Tx['scanTasks']>>['items'][number][] = [];
  let cursor: Cursor | undefined;
  do {
    const page = await tx.scanTasks({}, 100, cursor);
    for (const task of page.items) {
      const input = task.input && typeof task.input === 'object' && !Array.isArray(task.input) ? task.input : null;
      if (conversations.has(task.conversationId) || input?.mandateId === record.mandate.id) tasks.push(task);
    }
    cursor = page.next;
  } while (cursor);
  return tasks;
}

export function idlePmDeleteMonitor(task: { id: number; kind: string; state: { status: string; checkpoint?: JsonValue } }, monitorTaskId: number | null) {
  const checkpoint = task.state.checkpoint;
  const phase = checkpoint && typeof checkpoint === 'object' && !Array.isArray(checkpoint) ? checkpoint.phase : null;
  return task.id === monitorTaskId && task.kind === 'qz.pm.mandate-monitor' && task.state.status !== 'terminal' && task.state.status !== 'completing'
    && ['wait', 'maintenance'].includes(String(phase));
}

/** Only the owner projection is removed. Original ledgers and all original
 * command identities remain in Durable, including across host restarts. */
export async function deletePmMandate(tx: Tx, state: PmState, id: string): Promise<DeletionReceipt> {
  const record = pmRecord(state, id);
  if (record.deletedAt) return { kind: 'mandate', id, deletedAt: record.deletedAt };
  if (!Object.values(record.retirements).some(item => item.strategyVersionId === record.latestStrategyId)) throw new Problem('pm_delete_requires_retirement', 409);
  if (Object.values(record.retirements).some(item => item.reconciliationRequired)) throw new Problem('pm_delete_external_obligations', 409);

  const application = await tx.doc(Application);
  const source = record.mandate.capitalSource;
  let account: ReturnType<typeof captureAccount> | undefined;
  const relevant = { observations: {}, accountControls: {} } as { observations: Record<string, unknown>; accountControls: Record<string, unknown> };
  if (source.kind === 'ACCOUNT') {
    try { account = captureAccount(application, source.accountBindingId); }
    catch { throw new Problem('pm_delete_external_obligations', 409); }
    if (retirementAccountFlat(account).flat !== true) throw new Problem('pm_delete_external_obligations', 409);
    const binding = application.observations[source.accountBindingId];
    if (!binding) throw new Problem('pm_delete_external_obligations', 409);
    const key = accountKey(observation(binding).connection, application.accountControls);
    relevant.observations[source.accountBindingId] = binding;
    if (application.accountControls?.[key]) relevant.accountControls[key] = application.accountControls[key];
  }
  if (retirementOutstandingTargets(Object.values(record.commands), account, undefined, record.artifacts).length
    || externalMaintenanceBlockers(json(relevant), json({ mandates: { [id]: record } })).length) throw new Problem('pm_delete_external_obligations', 409);

  const conversations = new Set([record.conversationId, ...Object.values(record.campaigns).map(item => item.runtime.conversationId)]);
  const requests = await tx.doc(Requests);
  for (const [key, request] of Object.entries(requests.requests)) {
    if (!conversations.has(request.conversationId)) continue;
    const submission = await tx.submissionByRequest(request.conversationId, key.slice('request:'.length));
    if (submission && ['queued', 'placed'].includes(submission.status)
      || !submission && !(await tx.doc(Study, request.conversationId)).final) throw new Problem('pm_delete_tasks_active', 409);
  }
  for (const task of await pmMandateTasks(tx, record)) {
    // Keep checking original receipt families after the local Task is terminal.
    await assertNativeRemoteTaskSettled(tx, task.conversationId, task.id);
    if (task.state.status === 'terminal') continue;
    // Only an idle monitor may finish on its next native wake. No external
    // command or scientific work is aborted by deletion.
    if (idlePmDeleteMonitor(task, record.monitorTaskId)) continue;
    throw new Problem('pm_delete_tasks_active', 409);
  }
  record.deletedAt = new Date().toISOString();
  return { kind: 'mandate', id, deletedAt: record.deletedAt };
}
