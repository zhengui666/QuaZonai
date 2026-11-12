import { retainNativeRemoteUncertainty } from '../../research/src/native-execution.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { canonical } from '../../research/src/documents.ts';
import { Application, json, observation, Problem } from './facts.ts';
import { acceptReceipt, downstream } from './downstream.ts';
import type { ObservationFact } from './facts.ts';
import { maintenancePark, resumeMaintenance } from './pm-maintenance.ts';
import type { MaintenanceCheckpoint } from './pm-maintenance.ts';

export interface AutomaticInput { id: string; binary: NativeTarget; dataDir: string }
type RegularCheckpoint = { phase: 'snapshot' } | { phase: 'deliver' } | { phase: 'reconcile' } | { phase: 'wait'; wakeAt: number };
type Checkpoint = RegularCheckpoint | MaintenanceCheckpoint<{ phase: 'snapshot' }>;
export function unsettledAdjustment(fact: ObservationFact) {
  return ['PENDING', 'UNCERTAIN'].includes(fact.view.allocation?.receipt?.status ?? '') || ['UNCERTAIN', 'PENDING'].includes(fact.view.allocation?.status ?? '');
}
export const AdjustAccount = defineTask<AutomaticInput, Checkpoint, null>({
  name: 'qz.account-adjustment', version: 1, initial: () => ({ phase: 'snapshot' }),
  phases: {
    async snapshot(task, runtime, context) {
      await runtime.commit(async tx => {
        const app = await tx.doc(Application); const fact = observation(app.observations[task.input.id]);
        const allocation = fact.view.allocation;
        // SIZED is before the original write-ahead transport checkpoint.
        if (allocation?.status === 'SIZED' && allocation.receipt?.status === 'PENDING') {
          allocation.receipt.status = 'NOT_OFFERED'; allocation.status = 'STOPPED'; allocation.reason = 'native_strategy_only';
        }
        if (fact.view.automaticPolicy) fact.view.automaticPolicy.enabled = false;
        app.observations[task.input.id] = json(fact);
        return unsettledAdjustment(fact) ? { status: 'running', checkpoint: { phase: 'reconcile' } }
          : { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, context);
    },
    async deliver(task, runtime, context) {
      await runtime.commit(async tx => {
        const app = await tx.doc(Application); const fact = observation(app.observations[task.input.id]);
        const allocation = fact.view.allocation;
        if (allocation?.receipt && allocation.receipt.status === 'UNCERTAIN') return { status: 'running', checkpoint: { phase: 'reconcile' } };
        if (allocation?.receipt?.status === 'PENDING') { allocation.receipt.status = 'NOT_OFFERED'; allocation.status = 'STOPPED'; allocation.reason = 'native_strategy_only'; }
        if (fact.view.automaticPolicy) fact.view.automaticPolicy.enabled = false;
        app.observations[task.input.id] = json(fact);
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, context);
    },
    async reconcile(task, runtime, context) {
      let fact!: ObservationFact;
      await runtime.commit(async tx => { fact = observation(json((await tx.doc(Application)).observations[task.input.id])); return undefined; }, context);
      const allocation = fact.view.allocation!;
      try {
        const value = await downstream({ id: fact!.view.id, connection: fact!.connection, receiptId: allocation.receipt!.id, target: fact!.allocationTarget!, allocation: fact!.allocationContext }, 'receipt', runtime.signal);
        await settleAllocation(task.input.id, value, runtime, context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
        if (runtime.signal.aborted) throw error;
        // Keep the original uncertain identity. A refresh never offers a second POST.
        await runtime.commit(async tx => { const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]); fact.view.allocation!.reason = error instanceof Problem ? error.message : 'automatic_receipt_requires_reconciliation'; facts.observations[task.input.id] = json(fact); return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } }; }, context);
      }
    },
    async wait(task, runtime, context) {
      let parked = false;
      await runtime.commit(async tx => {
        const fact = observation((await tx.doc(Application)).observations[task.input.id]);
        if (!unsettledAdjustment(fact)) { const next = await maintenancePark(tx, task.id, 'qz.account-adjustment', { phase: 'snapshot' } as const); if (next) { parked = true; return next; } }
        return undefined;
      }, context);
      if (parked) return;
      await runtime.sleepUntil(task.state.checkpoint.wakeAt, { phase: 'snapshot' }, context);
    },
    async maintenance(task, runtime, context) { await runtime.commit(tx => resumeMaintenance(tx, task.id, 'qz.account-adjustment', task.state.checkpoint), context); },
  },
  async abort(task, runtime, context) {
    await runtime.commit(async tx => {
      const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
      if (fact.view.allocation?.status === 'SIZED' && fact.view.allocation.receipt?.status === 'PENDING') fact.view.allocation.receipt.status = 'NOT_OFFERED';
      const reconcile = unsettledAdjustment(fact);
      if (fact.view.allocation) { fact.view.allocation.status = reconcile ? 'UNCERTAIN' : 'STOPPED'; fact.view.allocation.reason = 'local_adjustment_stopped_not_remote_revocation'; }
      if (reconcile) fact.taskId = await tx.createTask(AdjustAccount, task.input, { conversationId: task.conversationId, ownership: { kind: 'conversation' }, background: true });
      facts.observations[task.input.id] = json(fact);
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, context);
  },
});
export function acceptAllocationReceipt(fact: ObservationFact, value: JsonObject) {
  const allocation = fact.view.allocation!;
  if (canonical(value.allocation ?? null) !== canonical(fact.allocationContext ?? null)) throw new Problem('automatic_receipt_context_mismatch', 409);
  const projection = { ...fact, view: { ...fact.view, receipt: allocation.receipt } };
  acceptReceipt(projection, value); allocation.receipt = projection.view.receipt; fact.view.error = null;
  allocation.status = allocation.receipt?.status === 'ACKNOWLEDGED' ? 'DELIVERED' : allocation.receipt?.status ?? 'UNCERTAIN';
  allocation.reason = allocation.status === 'DELIVERED' ? 'target_acknowledged_not_traded' : null;
}
async function settleAllocation(id: string, value: JsonObject, runtime: Parameters<typeof AdjustAccount.definition.phases.deliver>[1], context: Parameters<typeof AdjustAccount.definition.phases.deliver>[2]) {
  await runtime.commit(async tx => {
    const facts = await tx.doc(Application); const fact = observation(facts.observations[id]);
    acceptAllocationReceipt(fact, value);
    facts.observations[id] = json(fact);
    return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
  }, context);
}
