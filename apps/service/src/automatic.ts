import { defineTask, type ConversationId } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { canonical } from '../../research/src/documents.ts';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { Application, counter, json, nowNs, nsTime, object, observation, portfolio, Problem } from './facts.ts';
import { acceptReceipt, acceptSnapshot, downstream, isPreTransportRejection } from './downstream.ts';
import { accountKey, capitalStatus, control, readCapitalControl, recordCapitalControl, validateCapitalDocument } from './capital-exit.ts';
import { factorId } from './factors.ts';
import type { AutomaticAllocation, TargetReceipt } from '../../../packages/contracts/src/index.ts';
import type { ObservationFact } from './facts.ts';

export interface AutomaticInput { id: string; binary: string; dataDir: string }
type Checkpoint = { phase: 'snapshot' } | { phase: 'deliver' } | { phase: 'reconcile' } | { phase: 'wait'; wakeAt: number };
function emptyAllocation(fact: ObservationFact): AutomaticAllocation {
  return { status: 'BLOCKED', reason: 'awaiting_authoritative_account', updatedAt: null, sequence: null, nativeClockNs: null,
    equity: null, reserved: null, cashBuffer: fact.view.automaticPolicy!.cashBuffer, budget: null, capital: null, capacity: null, residualCash: null,
    controlRevision: null, members: [], targets: [], receipt: null, epoch: '0' };
}
export function unsettledAdjustment(fact: ObservationFact) {
  return ['PENDING', 'UNCERTAIN'].includes(fact.view.allocation?.receipt?.status ?? '') || ['UNCERTAIN', 'PENDING'].includes(fact.view.allocation?.status ?? '');
}
export const AdjustAccount = defineTask<AutomaticInput, Checkpoint, null>({
  name: 'qz.account-adjustment', version: 1, initial: () => ({ phase: 'snapshot' }),
  phases: {
    async snapshot(task, runtime, context) {
      let read!: JsonObject;
      await runtime.commit(async tx => { const facts = await tx.doc(Application); read = json({ fact: facts.observations[task.input.id], portfolio: facts.portfolios[observation(facts.observations[task.input.id]).input.portfolioId], controls: facts.accountControls ?? {} }); return undefined; }, context);
      const fact = observation(read.fact); const source = portfolio(read.portfolio); const policy = fact.view.automaticPolicy;
      if (unsettledAdjustment(fact)) { await runtime.commit(() => ({ status: 'running', checkpoint: { phase: fact.view.allocation?.status === 'SIZED' ? 'deliver' : 'reconcile' } }), context); return; }
      if (!policy?.enabled || fact.view.status === 'STOPPED') {
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), context); return;
      }
      try {
        const snapshot = await downstream({ id: fact.view.id, connection: fact.connection, receiptId: fact.view.receipt!.id, target: fact.target }, 'account', runtime.signal);
        // Existing manual bindings remain compatible; automatic execution requires mode and source clocks too.
        if (snapshot.mode !== fact.connection.mode) throw new Problem('automatic_account_mode_unknown');
        if (!Array.isArray(snapshot.positions)) throw new Problem('account_positions_unknown');
        const clock = BigInt(counter(snapshot.nativeClockNs, 'account_clock')); const now = BigInt(nowNs());
        if (clock > now || now - clock >= BigInt(policy.maximumSnapshotAgeSeconds) * 1_000_000_000n) throw new Problem('account_snapshot_stale_or_future');
        let capability: JsonObject = {}; let reason: string | undefined;
        let key = accountKey(fact.connection, read.controls as Record<string, JsonObject>);
        try {
          const fresh = await readCapitalControl(fact.connection, fact.view.currency, runtime.signal); capability = fresh.capabilities;
          for (const exit of fresh.status.exits) await validateCapitalDocument({ dataDir: task.input.dataDir, jobBinary: task.input.binary }, 'VIEW', exit);
          await runtime.commit(async tx => {
            const facts = await tx.doc(Application);
            facts.accountControls ??= {}; key = recordCapitalControl(facts.accountControls!, fact.connection, fresh);
            return undefined;
          }, context);
        } catch (error) { if (runtime.signal.aborted) throw error; reason = error instanceof Problem ? error.message : 'capital_exit_status_unavailable'; }
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const current = observation(facts.observations[task.input.id]);
          if (!current.view.automaticPolicy?.enabled || current.view.status === 'STOPPED') return;
          const bindings = Object.values(facts.observations).map(observation).filter(item => accountKey(item.connection, facts.accountControls) === key);
          const enabled = bindings.filter(item => item.view.automaticPolicy?.enabled && item.view.status !== 'STOPPED');
          const owner = enabled.find(item => item.view.allocation && BigInt(item.view.allocation.epoch) > 0n) ?? enabled[0];
          for (const duplicate of enabled.filter(item => item.view.id !== owner?.view.id)) {
            duplicate.view.automaticPolicy!.enabled = false; duplicate.view.allocation = { ...(duplicate.view.allocation ?? emptyAllocation(duplicate)), status: unsettledAdjustment(duplicate) ? 'UNCERTAIN' : 'BLOCKED', reason: 'account_automatic_binding_already_enabled' };
            facts.observations[duplicate.view.id] = json(duplicate);
          }
          if (owner && owner.view.id !== current.view.id) return;
          if (bindings.some(item => ['PENDING', 'UNCERTAIN'].includes(item.view.receipt?.status ?? '') || item.view.id !== current.view.id && unsettledAdjustment(item))) throw new Problem('target_delivery_requires_reconciliation', 409);
          acceptSnapshot(current, snapshot, bindings); current.view.status = 'OBSERVING'; current.view.error = null;
          const status = capitalStatus(control(facts.accountControls?.[key]));
          current.view.capitalExit = status;
          current.view.allocation ??= emptyAllocation(current);
          facts.observations[task.input.id] = json(current);
          return undefined;
        }, context);
        let state!: JsonObject;
        await runtime.commit(async tx => { const facts = await tx.doc(Application); state = json({ fact: facts.observations[task.input.id], control: facts.accountControls?.[key] ?? {} }); return undefined; }, context);
        const current = observation(state.fact); const status = capitalStatus(control(state.control));
        if (current.view.positions.some(position => position.value === null)) throw new Problem('account_position_valuation_unknown');
        if (!current.view.automaticPolicy?.enabled) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), context); return; }
        const capacity = source.view.capacity;
        if (snapshot.equity == null || status.reserved === null) throw new Problem(snapshot.equity == null ? 'account_equity_unknown' : 'account_reservation_unknown');
        if (reason) throw new Problem(reason);
        if (!capacity?.maximumCapital || capacity.status !== 'ESTIMATED' || !capacity.assumptions.participation || !capacity.assumptions.maximumAgeSeconds) throw new Problem('capacity_evidence_unavailable');
        if (capacity.currency !== current.view.currency) throw new Problem('capacity_currency_mismatch');
        const execution = snapshot.execution ? object(snapshot.execution) : null;
        const autoCapability = capability.automaticTargets ? object(capability.automaticTargets) : null;
        const expired = BigInt(counter(fact.target.valid_until_ns, 'target_expiry')) <= now;
        const capacityFresh = capacity.validUntilNs !== null && BigInt(counter(capacity.validUntilNs, 'capacity_expiry')) > now;
        const fenced = status.exits.some(exit => exit.remaining_trading !== 'BUDGET_AWARE');
        const matching = status.observationId === snapshot.observationId && status.accountSourceId === snapshot.accountSourceId && status.controlRevision === snapshot.controlRevision;
        const executable = autoCapability?.supported === true && autoCapability.revisionBound === true && status.capability !== 'BLOCKED' && matching && !fenced && !expired && capacityFresh && execution?.currency === current.view.currency
          && typeof execution.validUntilNs === 'string' && BigInt(counter(execution.validUntilNs, 'execution_expiry')) > now
          && Array.isArray(execution.liquidity) && current.view.positions.every(position => position.value !== null);
        const members = source.view.factors ?? source.view.researchIds.flatMap((researchId, index) => {
          const selected = object(source.evidence[index]?.selected); const proposal = object(selected.proposal);
          if (typeof selected.taskId !== 'number') throw new Problem('original_factor_identity_unavailable');
          const original = Array.isArray(proposal.members) ? proposal.members.map(member => object(member)) : [{ weight: '1' }];
          return original.map((member, memberIndex) => ({ factorId: factorId(researchId, Number(selected.taskId), memberIndex), weight: String(member.weight), poolWeight: source.weights?.[index] ?? '1' }));
        });
        const directory = await mkdtemp(join(task.input.dataDir, 'account-sizing-'));
        let sized: JsonObject;
        try {
          sized = await executeNative(task.input.binary, 'size-account', json({ schemaVersion: 1, projectionOnly: !executable, equity: snapshot.equity, reserved: status.reserved, cashBuffer: policy.cashBuffer,
            maximumCapital: capacity.maximumCapital, minimumCapital: capacity.minimumCapital, participation: capacity.assumptions.participation, maximumAgeSeconds: capacity.assumptions.maximumAgeSeconds,
            currency: current.view.currency, clockNs: now.toString(), target: fact.target,
            holdings: current.view.positions.filter(position => position.value !== null).map(position => ({ instrumentId: position.instrumentId, value: position.value })),
            liquidity: executable ? execution!.liquidity : [], members }), [], directory, context);
        } finally { await rm(directory, { recursive: true, force: true }); }
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const latest = observation(facts.observations[task.input.id]);
          if (!latest.view.automaticPolicy?.enabled || latest.view.status === 'STOPPED') return { status: 'terminal', outcome: { status: 'completed', result: null } };
          if (latest.view.automaticPolicy.cashBuffer !== policy.cashBuffer || latest.view.automaticPolicy.maximumSnapshotAgeSeconds !== policy.maximumSnapshotAgeSeconds) return { status: 'running', checkpoint: { phase: 'snapshot' } };
          const latestStatus = capitalStatus(control(facts.accountControls?.[key]));
          if (latestStatus.controlRevision !== status.controlRevision || latestStatus.reserved !== status.reserved) throw new Problem('capital_exit_changed_during_sizing');
          const previous = latest.view.allocation ?? emptyAllocation(latest);
          const target = sized.target ? object(sized.target) : null;
          const accepted = object(latest.allocationContext ?? {});
          const sameEconomics = accepted.controlRevision === status.controlRevision && accepted.reserved === status.reserved
            && accepted.equity === String(snapshot.equity) && accepted.cashBuffer === policy.cashBuffer && accepted.capital === String(sized.capital)
            && canonical(accepted.members ?? []) === canonical(sized.members ?? []) && !!target && !!latest.allocationTarget && canonical(target) === canonical(latest.allocationTarget);
          const unchanged = previous.receipt?.status === 'ACKNOWLEDGED' && sameEconomics;
          const acceptedCapacity = object(accepted.capacity ?? {});
          const repeatedRejection = sized.status === 'SIZED' && previous.receipt?.status === 'REJECTED' && sameEconomics
            && accepted.snapshotSequence === snapshot.sequence && accepted.nativeClockNs === snapshot.nativeClockNs && accepted.budget === String(sized.budget)
            && accepted.maximumSnapshotAgeSeconds === policy.maximumSnapshotAgeSeconds && canonical(accepted.factors ?? []) === canonical(json(members))
            && acceptedCapacity.maximumCapital === capacity.maximumCapital && canonical(acceptedCapacity.assumptions ?? {}) === canonical(json(capacity.assumptions));
          const deliver = executable && sized.status === 'SIZED' && target !== null && !unchanged && !repeatedRejection;
          const epoch = deliver ? (BigInt(previous.epoch) + 1n).toString() : previous.epoch;
          const receipt: TargetReceipt | null = deliver ? { id: `${latest.view.id}:allocation:${epoch}`, status: 'PENDING', externalId: null,
            expiresAt: nsTime(counter(target!.valid_until_ns, 'target_expiry')), acknowledgedAt: null } : previous.receipt;
          latest.view.allocation = { status: deliver ? 'SIZED' : repeatedRejection && executable ? 'REJECTED' : unchanged && executable ? 'UNCHANGED' : String(sized.status),
            reason: repeatedRejection && executable ? 'awaiting_newer_account_snapshot' : fenced ? 'capital_exit_fenced_pending_target' : expired ? 'target_expired' : !capacityFresh ? 'capacity_evidence_expired' : !matching ? 'account_control_observation_mismatch' : typeof sized.reason === 'string' ? sized.reason : null,
            updatedAt: new Date().toISOString(), sequence: String(snapshot.sequence), nativeClockNs: String(snapshot.nativeClockNs), equity: String(snapshot.equity), reserved: status.reserved,
            cashBuffer: policy.cashBuffer, budget: String(sized.budget), capital: String(sized.capital), capacity: capacity.maximumCapital, residualCash: sized.status === 'BLOCKED' ? null : String(sized.residualCash), controlRevision: status.controlRevision,
            members: (sized.members ?? []) as unknown as AutomaticAllocation['members'], targets: target && Array.isArray(target.targets) ? target.targets.map(value => { const row = object(value); return { instrumentId: String(row.instrument_id), weight: String(row.weight) }; }) : [], receipt, epoch };
          if (deliver) {
            // Match the native sizing scope; unrelated quotes do not limit this allocation.
            const instruments = new Set((fact.target.targets as JsonObject[]).filter(row =>
              (!Array.isArray(fact.target.rebalance_instrument_ids) || fact.target.rebalance_instrument_ids.includes(row.instrument_id!))
              && (/[1-9]/.test(String(row.weight)) || current.view.positions.some(position => position.instrumentId === row.instrument_id && /[1-9]/.test(position.value!))))
              .map(row => row.instrument_id));
            const liquidity = (execution!.liquidity as JsonObject[]).filter(row => instruments.has(row.instrumentId))
              .map(row => ({ instrumentId: row.instrumentId!, eventNs: row.eventNs!, availableNs: row.availableNs! }));
            const deadlines = [BigInt(counter(target!.valid_until_ns, 'target_expiry')), BigInt(counter(capacity.validUntilNs, 'capacity_expiry')),
              clock + BigInt(policy.maximumSnapshotAgeSeconds) * 1_000_000_000n, BigInt(counter(execution!.validUntilNs, 'execution_expiry')),
              ...liquidity.map(row => BigInt(counter(row.eventNs, 'liquidity_clock')) + BigInt(capacity.assumptions.maximumAgeSeconds!) * 1_000_000_000n)];
            latest.allocationTarget = target!;
            latest.allocationContext = json({ epoch, snapshotSequence: snapshot.sequence, nativeClockNs: snapshot.nativeClockNs,
              validUntilNs: deadlines.reduce((earliest, deadline) => deadline < earliest ? deadline : earliest).toString(), maximumSnapshotAgeSeconds: policy.maximumSnapshotAgeSeconds,
              execution: { validUntilNs: execution!.validUntilNs, maximumAgeSeconds: capacity.assumptions.maximumAgeSeconds, liquidity },
              accountSourceId: status.accountSourceId, observationId: status.observationId, controlRevision: status.controlRevision, reserved: status.reserved,
              equity: snapshot.equity, budget: sized.budget!, capital: sized.capital!, cashBuffer: policy.cashBuffer, members: sized.members!, factors: members, capacity: { maximumCapital: capacity.maximumCapital, decisionClockNs: capacity.decisionClockNs, validUntilNs: capacity.validUntilNs, assumptions: capacity.assumptions } });
          }
          facts.observations[task.input.id] = json(latest);
          return { status: 'running', checkpoint: deliver ? { phase: 'deliver' } : { phase: 'wait', wakeAt: runtime.now() + 5000 } };
        }, context);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
          fact.view.allocation ??= emptyAllocation(fact); fact.view.allocation.status = 'BLOCKED'; fact.view.allocation.reason = error instanceof Problem ? error.message : 'automatic_adjustment_unavailable';
          fact.view.allocation.updatedAt = new Date().toISOString();
          fact.view.capitalExit = capitalStatus(control(facts.accountControls?.[accountKey(fact.connection, facts.accountControls)]));
          facts.observations[task.input.id] = json(fact);
          return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
        }, context);
      }
    },
    async deliver(task, runtime, context) {
      let input!: import('./downstream.ts').ObservationTaskInput;
      await runtime.commit(async tx => {
        const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]); const allocation = fact.view.allocation!;
        if (!fact.view.automaticPolicy?.enabled || fact.view.status === 'STOPPED') {
          if (allocation.status === 'SIZED' && allocation.receipt?.status === 'PENDING') { allocation.receipt.status = 'NOT_OFFERED'; allocation.status = 'STOPPED'; facts.observations[task.input.id] = json(fact); }
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }
        const key = accountKey(fact.connection, facts.accountControls);
        const bindings = Object.values(facts.observations).map(observation).filter(other => accountKey(other.connection, facts.accountControls) === key);
        const unsettled = bindings.some(other => ['PENDING', 'UNCERTAIN'].includes(other.view.receipt?.status ?? '') || other.view.id !== fact.view.id && unsettledAdjustment(other));
        const status = capitalStatus(control(facts.accountControls?.[key]));
        if (unsettled || status.reserved !== allocation.reserved || status.controlRevision !== allocation.controlRevision || allocation.cashBuffer !== fact.view.automaticPolicy.cashBuffer || allocation.nativeClockNs === null || BigInt(nowNs()) - BigInt(allocation.nativeClockNs) >= BigInt(fact.view.automaticPolicy.maximumSnapshotAgeSeconds) * 1_000_000_000n) { allocation.status = 'BLOCKED'; allocation.receipt!.status = 'NOT_OFFERED'; allocation.reason = unsettled ? 'target_delivery_requires_reconciliation' : 'capital_exit_changed_before_delivery'; facts.observations[task.input.id] = json(fact); return { status: 'running', checkpoint: { phase: 'snapshot' } }; }
        allocation.status = 'UNCERTAIN'; allocation.receipt!.status = 'UNCERTAIN';
        input = json({ id: fact.view.id, connection: fact.connection, receiptId: allocation.receipt!.id, target: fact.allocationTarget!, allocation: fact.allocationContext }) as unknown as import('./downstream.ts').ObservationTaskInput;
        facts.observations[task.input.id] = json(fact);
        return { status: 'running', checkpoint: { phase: 'reconcile' } };
      }, context);
      if (!input) return;
      let checkingAccount = true;
      try {
        // A manual alias refresh can commit while the uncertainty checkpoint is awaited.
        const facts = (await runtime.snapshot(Application, context))!; const fact = observation(facts.observations[input.id]);
        if (!fact.snapshot || fact.snapshot.sequence !== input.allocation!.snapshotSequence || fact.snapshot.nativeClockNs !== input.allocation!.nativeClockNs) throw new Problem('account_snapshot_out_of_order');
        const key = accountKey(fact.connection, facts.accountControls);
        acceptSnapshot(fact, fact.snapshot, Object.values(facts.observations).map(observation).filter(other => accountKey(other.connection, facts.accountControls) === key));
        checkingAccount = false;
        await settle(await downstream(input, 'deliver', runtime.signal));
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        const notOffered = checkingAccount || isPreTransportRejection(error);
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]); const allocation = fact.view.allocation!;
          allocation.reason = error instanceof Problem ? error.message : 'automatic_delivery_requires_reconciliation';
          if (notOffered) { allocation.status = 'BLOCKED'; allocation.receipt!.status = 'NOT_OFFERED'; }
          facts.observations[task.input.id] = json(fact);
          return notOffered ? { status: 'running', checkpoint: { phase: 'snapshot' } } : undefined;
        }, context);
      }
      async function settle(value: JsonObject) { await settleAllocation(task.input.id, value, runtime, context); }
    },
    async reconcile(task, runtime, context) {
      let fact!: ObservationFact;
      await runtime.commit(async tx => { fact = observation(json((await tx.doc(Application)).observations[task.input.id])); return undefined; }, context);
      const allocation = fact.view.allocation!;
      try {
        const value = await downstream({ id: fact!.view.id, connection: fact!.connection, receiptId: allocation.receipt!.id, target: fact!.allocationTarget!, allocation: fact!.allocationContext }, 'receipt', runtime.signal);
        await settleAllocation(task.input.id, value, runtime, context);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        // Keep the original uncertain identity. A refresh never offers a second POST.
        await runtime.commit(async tx => { const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]); fact.view.allocation!.reason = error instanceof Problem ? error.message : 'automatic_receipt_requires_reconciliation'; facts.observations[task.input.id] = json(fact); return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } }; }, context);
      }
    },
    async wait(task, runtime, context) {
      await runtime.sleep(task.state.checkpoint.wakeAt, context);
      await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'snapshot' } }), context);
    },
  },
  async abort(task, runtime, context) {
    await runtime.commit(async tx => {
      const facts = await tx.doc(Application); const fact = observation(facts.observations[task.input.id]);
      if (fact.view.allocation?.status === 'SIZED' && fact.view.allocation.receipt?.status === 'PENDING') fact.view.allocation.receipt.status = 'NOT_OFFERED';
      const reconcile = unsettledAdjustment(fact);
      if (fact.view.allocation) { fact.view.allocation.status = reconcile ? 'UNCERTAIN' : 'STOPPED'; fact.view.allocation.reason = 'local_adjustment_stopped_not_remote_revocation'; }
      if (reconcile) fact.taskId = await tx.createTask(AdjustAccount, task.input, { conversationId: portfolio(facts.portfolios[fact.input.portfolioId]).view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' }, background: true });
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
