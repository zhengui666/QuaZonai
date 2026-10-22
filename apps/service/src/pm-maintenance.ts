import { randomUUID } from 'node:crypto';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId, Tx } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import type { MaintenanceCommandResult, MaintenanceExternalBlocker, MaintenanceReceipt, MaintenanceStatus } from '../../../packages/contracts/src/maintenance.ts';
import { canonical } from '../../research/src/documents.ts';
import { Application, json, nowNs, object, Problem, text } from './facts.ts';
import { PmDocument, pmState } from './pm-facts.ts';

const context = BACKGROUND_CONTEXT;
export type MaintenanceCheckpoint<S extends JsonObject & { phase: string }> = { phase: 'maintenance'; maintenanceEpoch: string; resumeCheckpoint: S };
export type { MaintenanceReceipt } from '../../../packages/contracts/src/maintenance.ts';
export interface MaintenanceState {
  schemaVersion: 1; state: 'RUNNING' | 'DRAINING' | 'QUIESCED'; epoch: string;
  barrierTaskId: number; enteredAtNs: string; quiescedAtNs: string | null; resumedAtNs: string | null;
  parked: Record<string, { taskId: number; kind: string; epoch: string; resumeCheckpoint: JsonObject }>;
  commands: Record<string, { original: JsonObject; receipt: MaintenanceReceipt }>;
}
type ApplicationValue = { maintenance?: JsonObject };
export function maintenanceState(application: ApplicationValue | undefined): MaintenanceState | null {
  return application?.maintenance ? application.maintenance as unknown as MaintenanceState : null;
}
export function assertBusinessAdmission(application: ApplicationValue): void {
  const state = maintenanceState(application);
  if (state && state.state !== 'RUNNING') throw new Problem('service_maintenance', 503);
}
export const MAINTENANCE_PARK_KINDS = new Set(['qz.pm.mandate-monitor', 'qz.account-adjustment']);

/** Called in the ORIGINAL task's commit before starting another business cycle. */
export async function maintenancePark<S extends JsonObject & { phase: string }>(tx: Tx, taskId: TaskId, kind: string, resumeCheckpoint: S) {
  const state = maintenanceState(await tx.doc(Application));
  if (!state || state.state === 'RUNNING') return null;
  if (!MAINTENANCE_PARK_KINDS.has(kind)) throw new Problem('maintenance_task_kind_unsupported');
  const barrier = await tx.task(state.barrierTaskId as TaskId);
  if (!barrier || barrier.kind !== 'qz.maintenance.gate' || barrier.version !== 1 || !barrier.background || barrier.abortRequested || object(barrier.input).epoch !== state.epoch || barrier.state.status === 'terminal' || barrier.state.status === 'completing') throw new Problem('maintenance_gate_unavailable', 409);
  state.parked[String(taskId)] = { taskId, kind, epoch: state.epoch, resumeCheckpoint: json(resumeCheckpoint) };
  return { status: 'waiting' as const, checkpoint: { phase: 'maintenance' as const, maintenanceEpoch: state.epoch, resumeCheckpoint },
    on: [state.barrierTaskId as TaskId], policy: 'allSettled' as const };
}
/** The official join resumes this phase only after its original gate settles. */
export async function resumeMaintenance<S extends JsonObject & { phase: string }>(tx: Tx, taskId: TaskId, kind: string, checkpoint: MaintenanceCheckpoint<S>) {
  const next = await maintenancePark(tx, taskId, kind, checkpoint.resumeCheckpoint);
  if (next) return next;
  const state = maintenanceState(await tx.doc(Application));
  if (state?.parked[String(taskId)]?.epoch === checkpoint.maintenanceEpoch) delete state.parked[String(taskId)];
  return { status: 'running' as const, checkpoint: checkpoint.resumeCheckpoint };
}

export const MaintenanceGate = defineTask<{ epoch: string }, { phase: 'hold' }, null>({
  name: 'qz.maintenance.gate', version: 1, initial: () => ({ phase: 'hold' }),
  phases: { async hold(task, runtime, ctx) {
    // One official read-only document watch releases the official waiting tasks.
    // Host closure cancels only the invocation; reopening reads the same epoch.
    const watch = await runtime.watchDoc(Application, ctx);
    if (!watch) throw new Error('maintenance_document_missing');
    try {
      const released = (value: ApplicationValue | null) => { const state = maintenanceState(value ?? undefined); return !state || state.epoch !== task.input.epoch || state.state === 'RUNNING'; };
      if (!released(watch.value)) {
        let release!: () => void;
        const changed = new Promise<void>(resolve => { release = resolve; });
        watch.start(async value => { if (released(value)) release(); });
        if (!released(watch.value)) await awaitWithContext(changed, ctx);
      }
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
    } finally { await watch.stop(); }
  } },
  async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
});
export const maintenanceExtension = defineExtension({ name: 'qz.maintenance', tasks: [MaintenanceGate] });

export type ExternalMaintenanceBlocker = MaintenanceExternalBlocker;
/** Receipt uncertainty and outstanding exit responsibility are independent of task idle. */
export function externalMaintenanceBlockers(application: JsonObject, pm: JsonObject | undefined): ExternalMaintenanceBlocker[] {
  const blockers: ExternalMaintenanceBlocker[] = [];
  const rows = (value: unknown) => value && typeof value === 'object' && !Array.isArray(value) ? Object.entries(value) : [];
  for (const [id, value] of rows(application.observations)) {
    const fact = object(value); const view = object(fact.view); const receipt = view.receipt ? object(view.receipt) : null;
    const allocation = view.allocation ? object(view.allocation) : null; const allocated = allocation?.receipt ? object(allocation.receipt) : null;
    if (['PENDING', 'STARTING', 'UNCERTAIN', 'DELIVERING', 'RUNNING'].includes(String(view.status)) || ['PENDING', 'UNCERTAIN'].includes(String(receipt?.status))) blockers.push({ kind: 'OBSERVATION', id, reason: 'original_receipt_unsettled' });
    if (['PENDING', 'UNCERTAIN'].includes(String(allocated?.status)) || ['PENDING', 'UNCERTAIN'].includes(String(allocation?.status))) blockers.push({ kind: 'ALLOCATION', id, reason: 'allocation_receipt_unsettled' });
  }
  for (const [id, value] of rows(application.accountControls)) for (const [requestId, command] of rows(object(value).commands)) if (['PENDING', 'UNCERTAIN'].includes(String(object(command).state))) blockers.push({ kind: 'CAPITAL_COMMAND', id: `${id}:${requestId}`, reason: 'capital_command_unsettled' });
  for (const mandate of Object.values(pmState(pm).mandates)) {
    for (const command of Object.values(mandate.commands)) if (['PENDING', 'UNCERTAIN'].includes(command.state)) blockers.push({ kind: 'PM_TARGET', id: command.id, reason: 'target_receipt_unsettled' });
    for (const outcome of Object.values(mandate.outcomes)) if (['PENDING', 'UNCERTAIN'].includes(outcome.receiptState)) {
      const command = outcome.targetCommandId ? mandate.commands[outcome.targetCommandId] : null;
      if (!command || command.id !== outcome.targetCommandId || command.decisionId !== outcome.decisionId || !['ACKNOWLEDGED', 'REJECTED', 'NOT_OFFERED'].includes(command.state)) blockers.push({ kind: 'PM_OUTCOME', id: outcome.id, reason: 'historical_receipt_not_reconciled' });
    }
    for (const retirement of Object.values(mandate.retirements)) if (retirement.reconciliationRequired) blockers.push({ kind: 'PM_RETIREMENT', id: retirement.id, reason: 'retirement_responsibility_unsettled' });
  }
  return blockers;
}

export function createMaintenance(host: ResearchHost) {
  const activeBusinessRequests = new Set<string>();
  async function beginRequest(id: string, reconcileOnly = false) {
    await host.harness.commit(async tx => {
      const app = await tx.doc(Application); const state = maintenanceState(app);
      if (!reconcileOnly || state?.state === 'QUIESCED') assertBusinessAdmission(app);
      activeBusinessRequests.add(id);
    }, context);
  }
  function endRequest(id: string) { activeBusinessRequests.delete(id); }
  async function status(): Promise<MaintenanceStatus> {
    const inspection = await host.harness.inspect(context);
    return host.harness.commit(async tx => {
      const application = await tx.doc(Application); const state = maintenanceState(application);
      const blockingTasks: { taskId: number; kind: string; status: string; reason: string }[] = []; const parkedTaskIds: number[] = [];
      for (const status of ['pending', 'running', 'waiting', 'completing'] as const) {
        let cursor: import('@earendil-works/pi-durable').Cursor | undefined;
        do {
          const page = await tx.scanTasks({ status }, 100, cursor);
          for (const task of page.items) {
            const checkpoint = task.state.status !== 'terminal' && task.state.status !== 'completing' ? object(task.state.checkpoint) : null;
            const original = task.version === 1 && task.background && !task.abortRequested;
            const gate = original && state && state.state !== 'RUNNING' && task.id === state.barrierTaskId && task.kind === MaintenanceGate.definition.name
              && task.background && task.state.status === 'running' && checkpoint?.phase === 'hold' && object(task.input).epoch === state.epoch;
            const expected = state?.parked[String(task.id)];
            const parked = original && state && state.state !== 'RUNNING' && expected && expected.taskId === task.id && expected.kind === task.kind && expected.epoch === state.epoch
              && MAINTENANCE_PARK_KINDS.has(task.kind) && task.state.status === 'waiting' && task.state.policy === 'allSettled'
              && task.state.on.length === 1 && task.state.on[0] === state.barrierTaskId && checkpoint?.phase === 'maintenance' && checkpoint.maintenanceEpoch === state.epoch
              && canonical(checkpoint.resumeCheckpoint!) === canonical(expected.resumeCheckpoint);
            if (parked) parkedTaskIds.push(task.id);
            else if (!gate) blockingTasks.push({ taskId: task.id, kind: task.kind, status: task.state.status, reason: 'task_not_parked_for_current_epoch' });
          }
          cursor = page.next;
        } while (cursor);
      }
      // Public inspect reads submissions on the official Session line. While
      // draining, any pre-admitted submission producer remains either an active
      // HTTP admission or an unparked original task, both checked in this commit.
      const blockingSubmissions = inspection.submissions.map(item => ({ submissionId: item.id as number, status: item.status }));
      const blockingExternal = externalMaintenanceBlockers(json(application), json(await tx.doc(PmDocument)));
      if (state && state.state !== 'RUNNING') {
        for (const expected of Object.values(state.parked)) if (!parkedTaskIds.includes(expected.taskId)) blockingTasks.push({ taskId: expected.taskId, kind: expected.kind, status: 'unverified', reason: 'registered_parked_task_not_verified' });
        const gate = await tx.task(state.barrierTaskId as TaskId);
        if (!gate || gate.state.status !== 'running') blockingTasks.push({ taskId: state.barrierTaskId, kind: MaintenanceGate.definition.name, status: gate?.state.status ?? 'missing', reason: 'maintenance_gate_not_waiting' });
        const quiesced = !blockingTasks.length && !blockingSubmissions.length && !blockingExternal.length && activeBusinessRequests.size === 0;
        state.state = quiesced ? 'QUIESCED' : 'DRAINING';
        state.quiescedAtNs = quiesced ? state.quiescedAtNs ?? nowNs() : null;
      }
      return { state: state?.state ?? 'RUNNING', epoch: state?.epoch ?? null, barrierTaskId: state?.barrierTaskId ?? null,
        enteredAtNs: state?.enteredAtNs ?? null, quiescedAtNs: state?.quiescedAtNs ?? null, resumedAtNs: state?.resumedAtNs ?? null,
        parkedTaskIds, blockingTasks, blockingSubmissions, blockingExternal, activeBusinessRequests: activeBusinessRequests.size };
    }, context);
  }
  async function enter(value: unknown): Promise<MaintenanceCommandResult> {
    const original = object(value); const requestId = text(original.requestId, 'request_id');
    if (Object.keys(original).some(key => !['requestId', 'reason'].includes(key)) || original.reason !== undefined && typeof original.reason !== 'string') throw new Problem('maintenance_enter_invalid');
    const receipt = await host.harness.commit(async tx => {
      const app = await tx.doc(Application); const prior = maintenanceState(app); const key = `enter:${requestId}`; const old = prior?.commands[key];
      if (old) { if (canonical(old.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return json(old.receipt) as unknown as MaintenanceReceipt; }
      if (prior && prior.state !== 'RUNNING') throw new Problem('maintenance_already_entered', 409);
      const epoch = randomUUID(); const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const barrierTaskId = await tx.createTask(MaintenanceGate, { epoch }, { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
      const receipt: MaintenanceReceipt = { requestId, action: 'enter', epoch };
      app.maintenance = json({ schemaVersion: 1, epoch, state: 'DRAINING', barrierTaskId, enteredAtNs: nowNs(), quiescedAtNs: null, resumedAtNs: null,
        parked: {}, commands: { ...prior?.commands, [key]: { original, receipt } } } satisfies MaintenanceState);
      return receipt;
    }, context);
    host.harness.resume(); return { receipt, status: await status() };
  }
  async function resume(value: unknown): Promise<MaintenanceCommandResult> {
    const original = object(value); const requestId = text(original.requestId, 'request_id'); const epoch = text(original.epoch, 'maintenance_epoch');
    if (Object.keys(original).some(key => !['requestId', 'epoch'].includes(key))) throw new Problem('maintenance_resume_invalid');
    const receipt = await host.harness.commit(async tx => {
      const state = maintenanceState(await tx.doc(Application)); const key = `resume:${requestId}`; const old = state?.commands[key];
      if (old) { if (canonical(old.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return json(old.receipt) as unknown as MaintenanceReceipt; }
      if (!state || state.epoch !== epoch) throw new Problem('maintenance_epoch_conflict', 409);
      if (state.state === 'RUNNING') throw new Problem('maintenance_already_resumed', 409);
      const receipt: MaintenanceReceipt = { requestId, action: 'resume', epoch }; state.state = 'RUNNING'; state.resumedAtNs = nowNs();
      state.commands[key] = { original, receipt }; return receipt;
    }, context);
    host.harness.resume(); return { receipt, status: await status() };
  }
  return { beginRequest, endRequest, enter, resume, status };
}
