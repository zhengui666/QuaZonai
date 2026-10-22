import { defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { assessCapacityCurve } from '../../../packages/contracts/src/decision.ts';
import { Application, json, nowNs, nsTime, object, observation, Problem } from './facts.ts';
import { PmDocument, pmRecord, pmState, revise } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';
import { captureAccount } from './pm-account.ts';
import { downstream, isPreTransportRejection } from './downstream.ts';
import type { ObservationTaskInput } from './downstream.ts';

/** This validates an existing scientific decision; it cannot manufacture one. */
export function targetEligibility(record: PmRecord, decision: Pm.PortfolioDecision, basis: Pm.AccountBasis) {
  if (record.mandate.operatingAuthority !== 'AUTHORIZED_TARGETS') throw new Problem('target_authority_required', 409);
  if (decision.status !== 'CURRENT' || decision.basisRevision !== record.basisRevision || record.currentDecisionId !== decision.id) throw new Problem('target_decision_superseded', 409);
  if (!decision.accountBasis || !decision.capital || !decision.validUntilNs || BigInt(decision.validUntilNs) <= BigInt(nowNs())) throw new Problem('target_current_evidence_required', 409);
  if (record.mandate.unresolvedIntent.length || Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && item.permittedRemedy === 'CLARIFY_INTENT')) throw new Problem('target_owner_intent_unresolved', 409);
  const strategy = record.strategies[decision.strategyVersionId];
  const assessment = strategy ? record.assessments[strategy.pipelineAssessmentId] : undefined;
  if (!assessment || assessment.strategyVersionId !== decision.strategyVersionId || assessment.stage !== 'TARGET_OPERATION' || assessment.outcome !== 'READY') throw new Problem('target_pipeline_evidence_required', 409);
  for (const key of ['accountBindingId', 'accountSourceId', 'observationId', 'sequence', 'observedAtNs', 'controlRevision', 'currency', 'positionsEvidenceRef', 'outstandingCommandsRef'] as const) if (basis[key] !== decision.accountBasis[key]) throw new Problem('target_account_basis_changed', 409);
  if (JSON.stringify(basis.equity) !== JSON.stringify(decision.accountBasis.equity) || JSON.stringify(basis.reserved) !== JSON.stringify(decision.accountBasis.reserved)) throw new Problem('target_capital_basis_changed', 409);
  if (decision.assetTargets.some(item => item.currentNotional === null || item.netTradeNotional === null)) throw new Problem('target_actual_position_basis_required', 409);
  if (!decision.capital.capacityEstimateIds.length && decision.action !== 'CASH') throw new Problem('target_capacity_evidence_required', 409);
  for (const id of decision.capital.capacityEstimateIds) {
    const capacity = record.capacities[id];
    if (!capacity || capacity.basisRevision !== decision.basisRevision || !capacity.validUntilNs || BigInt(capacity.validUntilNs) <= BigInt(nowNs()) || assessCapacityCurve(capacity, decision.capital.committedCapital).feasible !== true) throw new Problem('target_capacity_expired_or_unproven', 409);
  }
  if (Object.values(record.commands).some(item => item.decisionId !== decision.id && ['PENDING', 'UNCERTAIN'].includes(item.state))) throw new Problem('target_delivery_requires_reconciliation', 409);
  if (Object.values(record.retirements).some(item => item.strategyVersionId === decision.strategyVersionId)) throw new Problem('strategy_retired', 409);
}

export const PmTargetTask = defineTask<{ mandateId: string; commandId: string; reconcileOnly?: boolean }, { phase: 'deliver' | 'reconcile' } | { phase: 'wait'; wakeAt: number }, null>({
  name: 'qz.pm.target-command', version: 1, initial: input => ({ phase: input.reconcileOnly ? 'reconcile' : 'deliver' }),
  phases: {
    async deliver(task, runtime, context) {
      let input: ObservationTaskInput | null = null;
      await runtime.commit(async tx => {
        const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const command = record.commands[task.input.commandId]!;
        const application = await tx.doc(Application); const source = observation(application.observations[command.accountBindingId]);
        try {
          targetEligibility(record, record.decisions[command.decisionId]!, captureAccount(application, command.accountBindingId).basis);
        } catch (error) {
          command.state = 'NOT_OFFERED'; revise(command); record.artifacts[`target-error:${command.id}`] = json({ reason: error instanceof Problem ? error.message : 'target_evidence_unavailable' });
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }
        command.state = 'UNCERTAIN'; revise(command);
        input = json({ id: source.view.id, connection: source.connection, receiptId: command.idempotencyKey, target: object(record.artifacts[command.payloadEvidenceRef], 'target_payload') }) as unknown as ObservationTaskInput;
        return { status: 'running', checkpoint: { phase: 'reconcile' } };
      }, context);
      if (!input) return;
      try { await settle(await downstream(input, 'deliver', runtime.signal)); }
      catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => {
          const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const command = record.commands[task.input.commandId]!;
          if (isPreTransportRejection(error)) { revise(command); command.state = 'NOT_OFFERED'; }
          record.artifacts[`target-error:${command.id}`] = json({ reason: error instanceof Problem ? error.message : 'target_receipt_requires_reconciliation' });
          return isPreTransportRejection(error) ? { status: 'terminal', outcome: { status: 'completed', result: null } } : { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
        }, context);
      }
      async function settle(receipt: JsonObject) { await runtime.commit(async tx => {
        const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const command = record.commands[task.input.commandId]!;
        const application = await tx.doc(Application); acceptPmReceipt(record, command, receipt, observation(application.observations[command.accountBindingId]).connection);
        return ['PENDING', 'UNCERTAIN'].includes(command.state) ? { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } } : { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, context); }
    },
    async reconcile(task, runtime, context) {
      const record = pmRecord(pmState(await runtime.snapshot(PmDocument, context)), task.input.mandateId); const command = record.commands[task.input.commandId]!;
      const application = (await runtime.snapshot(Application, context))!; const source = observation(application.observations[command.accountBindingId]);
      try {
        const receipt = await downstream({ id: source.view.id, connection: source.connection, receiptId: command.idempotencyKey, target: object(record.artifacts[command.payloadEvidenceRef]) }, 'receipt', runtime.signal);
        await runtime.commit(async tx => { const current = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const cmd = current.commands[task.input.commandId]!;
          acceptPmReceipt(current, cmd, receipt, source.connection);
          return ['PENDING', 'UNCERTAIN'].includes(cmd.state) ? { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } } : { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, context);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => { const current = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
          current.artifacts[`target-error:${command.id}`] = json({ reason: error instanceof Problem ? error.message : 'target_receipt_requires_reconciliation' });
          return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
        }, context);
      }
    },
    async wait(task, runtime, context) { await runtime.sleep(task.state.checkpoint.wakeAt, context); await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'reconcile' } }), context); },
  },
  async abort(task, runtime, context) { await runtime.commit(async tx => {
    const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const command = record.commands[task.input.commandId]!;
    // Local abort cannot prove an uncertain command was not received.
    if (task.state.checkpoint.phase === 'deliver') { command.state = 'NOT_OFFERED'; revise(command); }
    return { status: 'terminal', outcome: { status: 'aborted' } };
  }, context); },
});

function acceptPmReceipt(record: PmRecord, command: Pm.TargetCommand, receipt: JsonObject, connection: { clientId: string; accountId: string }) {
  if (receipt.id !== command.idempotencyKey || receipt.clientId !== connection.clientId || receipt.accountId !== connection.accountId || receipt.expiresAt !== nsTime(command.validUntilNs)) throw new Problem('target_receipt_identity_mismatch');
  if (!['PENDING', 'ACKNOWLEDGED', 'REJECTED', 'EXPIRED', 'REVOKED'].includes(String(receipt.status))) throw new Problem('target_receipt_status_unsupported');
  const previous = command.externalReceiptRef ? record.artifacts[command.externalReceiptRef] : undefined;
  if (previous?.externalId && receipt.externalId !== previous.externalId) throw new Problem('target_receipt_external_identity_mismatch');
  command.state = receipt.status === 'ACKNOWLEDGED' ? 'ACKNOWLEDGED' : receipt.status === 'PENDING' ? 'PENDING' : 'REJECTED'; revise(command);
  const reference = `target-receipt:${command.id}`; record.artifacts[reference] = receipt; command.externalReceiptRef = reference;
}
