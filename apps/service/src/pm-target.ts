import { defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { assessCapacityCurve } from '../../../packages/contracts/src/decision.ts';
import { Application, json, nowNs, nsTime, object, observation, Problem } from './facts.ts';
import { canonical } from '../../research/src/documents.ts';
import { digest } from './pm-facts.ts';
import { PmDocument, pmRecord, pmState, revise } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';
import { captureAccount } from './pm-account.ts';
import { downstream } from './downstream.ts';
import type { ObservationTaskInput } from './downstream.ts';
import { assertAccountTargetControl } from './account-target-control.ts';

/** Keep the original native cutoff and weights. A derived delivery deadline may
 * shorten its lifetime; receiver clocks never renew the native target. */
export function pmTargetPayload(record: PmRecord, decision: Pm.PortfolioDecision): JsonObject {
  const explanation = object(decision.explanationFacts);
  const ref = typeof explanation.nativeTargetEvidenceRef === 'string' ? record.evidence[explanation.nativeTargetEvidenceRef] : undefined;
  if (!ref || ref.sourceKind !== 'NATIVE_RESULT' || !ref.contentDigest || ref.sourceRevision !== decision.basisRevision) throw new Problem('target_original_native_target_required', 409);
  const target = object(record.artifacts[ref.objectKey], 'original_native_target');
  if (digest(target) !== ref.contentDigest || target.schema_version !== 1 || typeof target.asof_ns !== 'string' || !/^\d+$/.test(target.asof_ns)
    || typeof target.valid_until_ns !== 'string' || !/^\d+$/.test(target.valid_until_ns) || !decision.validUntilNs
    || BigInt(target.asof_ns) > BigInt(nowNs()) || BigInt(ref.availableAtNs) > BigInt(nowNs())
    || BigInt(target.asof_ns) >= BigInt(decision.validUntilNs) || BigInt(decision.validUntilNs) > BigInt(target.valid_until_ns)
    || !Array.isArray(target.targets) || target.targets.length !== decision.assetTargets.length) throw new Problem('target_original_native_target_mismatch', 409);
  const expected = decision.assetTargets.map(item => ({ instrument_id: item.instrumentId, currency: item.targetNotional.currency, weight: item.navWeight }));
  if (canonical(target.targets) !== canonical(expected) || typeof target.cash_weight !== 'string') throw new Problem('target_original_native_target_mismatch', 409);
  return json({ ...target, valid_until_ns: decision.validUntilNs });
}

/** This validates an existing scientific decision; it cannot manufacture one. */
export function targetEligibility(record: PmRecord, decision: Pm.PortfolioDecision, basis: Pm.AccountBasis, mode: 'paper' | 'live') {
  if (record.mandate.operatingAuthority !== 'AUTHORIZED_TARGETS') throw new Problem('target_authority_required', 409);
  if (record.artifacts[`strategy-control:${decision.strategyVersionId}`]?.action === 'pause') throw new Problem('target_strategy_paused', 409);
  if (decision.status !== 'CURRENT' || decision.basisRevision !== record.basisRevision || record.currentDecisionId !== decision.id) throw new Problem('target_decision_superseded', 409);
  if (!decision.accountBasis || !decision.capital || !decision.validUntilNs || BigInt(decision.validUntilNs) <= BigInt(nowNs())) throw new Problem('target_current_evidence_required', 409);
  if (record.mandate.unresolvedIntent.length || Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && item.permittedRemedy === 'CLARIFY_INTENT')) throw new Problem('target_owner_intent_unresolved', 409);
  const strategy = record.strategies[decision.strategyVersionId];
  const assessment = strategy ? record.assessments[strategy.pipelineAssessmentId] : undefined;
  const expectedStage = mode === 'paper' ? 'PAPER' : 'TARGET_OPERATION';
  const expectedRequirements = mode === 'paper' ? 'qz.pm.paper-shadow-source-target-qualification/1' : 'qz.pm.live-source-target-qualification/1';
  if (!assessment || assessment.strategyVersionId !== decision.strategyVersionId || assessment.stage !== expectedStage
    || assessment.requirementsVersion !== expectedRequirements || assessment.outcome !== 'READY') throw new Problem('target_pipeline_evidence_required', 409);
  if (mode === 'live') {
    const protocol = object(object(record.policy.validDomain).qualificationProtocol);
    if (protocol.maximumPromotion !== 'TARGET_OPERATION' || protocol.liveTargetAuthorization !== true) throw new Problem('target_live_method_qualification_required', 409);
  }
  for (const key of ['accountBindingId', 'accountSourceId', 'observationId', 'sequence', 'observedAtNs', 'controlRevision', 'currency', 'positionsEvidenceRef', 'outstandingCommandsRef'] as const) if (basis[key] !== decision.accountBasis[key]) throw new Problem('target_account_basis_changed', 409);
  if (JSON.stringify(basis.equity) !== JSON.stringify(decision.accountBasis.equity) || JSON.stringify(basis.reserved) !== JSON.stringify(decision.accountBasis.reserved)) throw new Problem('target_capital_basis_changed', 409);
  if (decision.assetTargets.some(item => item.currentNotional === null || item.netTradeNotional === null)) throw new Problem('target_actual_position_basis_required', 409);
  const currentComparison = record.comparisons[decision.comparisonRef];
  if (currentComparison?.evaluationContext === 'FORWARD_MODEL_ESTIMATE') {
    const explanation = object(decision.explanationFacts);
    const id = typeof explanation.executionQualificationEvidenceRef === 'string' ? explanation.executionQualificationEvidenceRef : '';
    const reference = record.evidence[id]; const proof = record.artifacts[reference?.objectKey ?? ''];
    if (!reference || reference.sourceKind !== 'DERIVED' || reference.role !== 'CONTROL' || reference.sourceRevision !== decision.basisRevision
      || !reference.contentDigest || !proof || digest(proof) !== reference.contentDigest || BigInt(reference.availableAtNs) > BigInt(nowNs())
      || !assessment.evidenceRefs.includes(id) || currentComparison.basisRevision !== decision.basisRevision
      || proof.schemaVersion !== 1 || proof.scope !== 'CURRENT_TARGET_DELIVERY_ONLY' || proof.decisionId !== decision.id
      || proof.basisRevision !== decision.basisRevision || proof.assessmentOutcome !== 'READY'
      || object(proof.execution).scope !== 'CURRENT_SOURCE_EXECUTION_BOUNDS' || object(proof.execution).outcome !== 'READY'
      || typeof proof.validUntilNs !== 'string' || !/^\d+$/.test(proof.validUntilNs)
      || BigInt(decision.validUntilNs) > BigInt(proof.validUntilNs)
      || proof.nativeEvidenceRef !== decision.solverEvidenceRef || !assessment.evidenceRefs.includes(String(proof.nativeEvidenceRef))
      || !record.datasets.includes(String(proof.datasetRevision))
      || proof.independentMethodEvidenceRef !== explanation.independentMethodEvidenceRef
      || !assessment.evidenceRefs.includes(String(proof.independentMethodEvidenceRef))) throw new Problem('target_current_source_qualification_required', 409);
    if (!Array.isArray(proof.deadlines) || !proof.deadlines.length || proof.deadlines.some(value => {
      const deadline = object(value);
      return typeof deadline.sourceKind !== 'string' || typeof deadline.sourceEvidenceRef !== 'string'
        || typeof deadline.validUntilNs !== 'string' || !/^\d+$/.test(deadline.validUntilNs)
        || BigInt(decision.validUntilNs!) > BigInt(deadline.validUntilNs);
    })) throw new Problem('target_current_source_deadlines_required', 409);
  } else if (!decision.capital.capacityEstimateIds.length && decision.action !== 'CASH') throw new Problem('target_capacity_evidence_required', 409);
  for (const id of decision.capital.capacityEstimateIds) {
    const capacity = record.capacities[id];
    if (!capacity || capacity.basisRevision !== decision.basisRevision || !capacity.validUntilNs || BigInt(capacity.validUntilNs) <= BigInt(nowNs()) || assessCapacityCurve(capacity, decision.capital.committedCapital).feasible !== true) throw new Problem('target_capacity_expired_or_unproven', 409);
  }
  if (Object.values(record.commands).some(item => item.decisionId !== decision.id && ['PENDING', 'UNCERTAIN'].includes(item.state))) throw new Problem('target_delivery_requires_reconciliation', 409);
  if (Object.values(record.retirements).some(item => item.strategyVersionId === decision.strategyVersionId)) throw new Problem('strategy_retired', 409);
  pmTargetPayload(record, decision);
}

export const PmTargetTask = defineTask<{ mandateId: string; commandId: string; reconcileOnly?: boolean; trackAcknowledgedUntilExpiry?: boolean }, { phase: 'deliver' | 'reconcile' } | { phase: 'wait'; wakeAt: number }, null>({
  name: 'qz.pm.target-command', version: 1, initial: input => ({ phase: input.reconcileOnly ? 'reconcile' : 'deliver' }),
  phases: {
    async deliver(task, runtime, context) {
      await runtime.commit(async tx => {
        const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const command = record.commands[task.input.commandId]!;
        if (command.state === 'UNCERTAIN') return { status: 'running', checkpoint: { phase: 'reconcile' } };
        if (command.state === 'PENDING') { command.state = 'NOT_OFFERED'; revise(command); }
        record.artifacts[`target-error:${command.id}`] = { reason: 'native_strategy_only' };
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, context);
    },
    async reconcile(task, runtime, context) {
      const record = pmRecord(pmState(await runtime.snapshot(PmDocument, context)), task.input.mandateId); const command = record.commands[task.input.commandId]!;
      if (task.input.trackAcknowledgedUntilExpiry === true && command.state === 'ACKNOWLEDGED' && BigInt(command.validUntilNs) <= BigInt(nowNs())) {
        // Expiry ends this receipt watch, not the position responsibility.
        // The mandate monitor still requires a later authoritative flat frame.
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), context); return;
      }
      const application = (await runtime.snapshot(Application, context))!; const source = observation(application.observations[command.accountBindingId]);
      try {
        const receipt = await downstream({ id: source.view.id, connection: source.connection, receiptId: command.idempotencyKey, target: object(record.artifacts[command.payloadEvidenceRef]) }, 'receipt', runtime.signal);
        await runtime.commit(async tx => { const current = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const cmd = current.commands[task.input.commandId]!;
          acceptPmReceipt(current, cmd, receipt, source.connection);
          const stillResponsible = ['PENDING', 'UNCERTAIN'].includes(cmd.state)
            || task.input.trackAcknowledgedUntilExpiry === true && cmd.state === 'ACKNOWLEDGED' && BigInt(cmd.validUntilNs) > BigInt(nowNs());
          return stillResponsible ? { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } } : { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, context);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => { const current = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
          current.artifacts[`target-error:${command.id}`] = json({ reason: error instanceof Problem ? error.message : 'target_receipt_requires_reconciliation' });
          return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
        }, context);
      }
    },
    async wait(task, runtime, context) { await runtime.sleepUntil(task.state.checkpoint.wakeAt, { phase: 'reconcile' }, context); },
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
  const originalAllocation = record.artifacts[`target-allocation:${command.id}`];
  if (originalAllocation && canonical(receipt.allocation ?? null) !== canonical(originalAllocation)) throw new Problem('target_receipt_allocation_mismatch');
  if (receipt.target !== undefined && canonical(receipt.target) !== canonical(object(record.artifacts[command.payloadEvidenceRef]))) throw new Problem('target_receipt_payload_mismatch');
  if (!['PENDING', 'ACKNOWLEDGED', 'REJECTED', 'EXPIRED', 'REVOKED'].includes(String(receipt.status))) throw new Problem('target_receipt_status_unsupported');
  if (command.state === 'ACKNOWLEDGED' && receipt.status === 'PENDING') throw new Problem('target_receipt_status_regression');
  const previous = command.externalReceiptRef ? record.artifacts[command.externalReceiptRef] : undefined;
  if (previous?.externalId && receipt.externalId !== previous.externalId) throw new Problem('target_receipt_external_identity_mismatch');
  command.state = receipt.status === 'ACKNOWLEDGED' ? 'ACKNOWLEDGED' : receipt.status === 'PENDING' ? 'PENDING' : 'REJECTED'; revise(command);
  const reference = `target-receipt:${command.id}`; record.artifacts[reference] = receipt; command.externalReceiptRef = reference;
  const observedRef = `target-receipt-observation:${command.id}:${digest(receipt)}`;
  record.artifacts[observedRef] ??= json({ commandId: command.id, receiptRef: reference, receipt, availableAtNs: nowNs() });
  record.artifacts[`target-receipt-clock:${command.id}`] = json({ availableAtNs: record.artifacts[observedRef]!.availableAtNs,
    status: receipt.status, observationRef: observedRef });
}
