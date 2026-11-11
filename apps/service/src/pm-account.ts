import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
import { counter, decimal, decimalUnits, json, nowNs, object, observation, Problem } from './facts.ts';
import type { CapitalExitStatus } from '../../../packages/contracts/src/capital-exit.ts';
import { accountKey, capitalStatusForBinding, control, moneyString, physicalCapitalState, transportBindingKey, transportCapability, transportControlFailure } from './capital-exit.ts';
import { identity } from './pm-facts.ts';

export interface CapturedAccount { basis: Pm.AccountBasis; evidence: Pm.EvidenceRef; artifact: JsonObject }

/** Capital preparation constrains buying separately from the approved strategy
 * signal. This applies while paused/cancelled too: a smaller reserve budget must
 * never create an ordinary sale that bypasses the exit's price or cost limits. */
export function capitalExitTargetControl(status: CapitalExitStatus, capabilities: JsonObject, signalTarget: JsonObject, signalCapital: string): JsonObject | null {
  const active = status.exits.filter(exit => !['COMPLETED', 'CLOSED'].includes(exit.state));
  if (!active.length) return null;
  if (!status.controlRevision || status.reserved === null || status.capability === 'BLOCKED') throw new Problem('capital_exit_target_control_unavailable', 409);
  const permission = object(capabilities.capitalExit ?? {}), automatic = object(capabilities.automaticTargets ?? {});
  if (permission.version !== 2 || automatic.revisionBound !== true || automatic.snapshotBound !== true)
    throw new Problem('capital_exit_budget_fence_required', 409);
  const modes = [...new Set(active.map(exit => exit.policy.kind))];
  if (modes.includes('NATURAL_RELEASE') && modes.includes('BOUNDED_LIMIT')) throw new Problem('capital_exit_policy_conflict', 409);
  if (decimalUnits(signalCapital) < 0n) throw new Problem('capital_exit_signal_capital_invalid', 409);
  const mode = modes.includes('NATURAL_RELEASE') ? 'NATURAL_RELEASE' : modes.includes('BOUNDED_LIMIT') ? 'BOUNDED_LIMIT' : 'CASH_ONLY';
  return json({ mode, transition: active.some(exit => exit.remaining_trading === 'FENCED_PENDING_TARGET'),
    controlRevision: status.controlRevision, exitIds: active.map(exit => exit.id).sort(), signalTarget, signalCapital });
}

/** Only reservations still inside the managed account reduce its observed NAV.
 * Staged transfers and withdrawals already left that NAV and are not deducted. */
export function managedTradingBudget(equity: string, reserved: string, cashBuffer: string): string {
  const values = [equity, reserved, cashBuffer].map(decimalUnits);
  if (values[1]! < 0n || values[2]! < 0n) throw new Problem('account_reservation_invalid', 409);
  const budget = values[0]! - values[1]! - values[2]!;
  return moneyString(budget > 0n ? budget : 0n);
}

/** Legacy factor weights precede account sizing. Their original funding base is
 * the same size-account budget with R=0, including its existing buffer/capacity. */
export function unreservedStrategyCapital(equity: string, cashBuffer: string, maximumCapital: string): string {
  const budget = decimalUnits(managedTradingBudget(equity, '0', cashBuffer));
  const maximum = decimalUnits(maximumCapital);
  if (maximum < 0n) throw new Problem('account_capacity_invalid', 409);
  return moneyString(budget < maximum ? budget : maximum);
}

export function captureAccount(application: { observations: Record<string, JsonObject>; accountControls?: Record<string, JsonObject> }, id: string, atNs = nowNs()): CapturedAccount {
  const value = application.observations[id]; if (!value) throw new Problem('account_binding_not_found', 404);
  const observed = observation(value); if (!observed.snapshot) throw new Problem('account_snapshot_required', 409);
  const snapshot = object(observed.snapshot);
  const storedControl = application.accountControls?.[accountKey(observed.connection, application.accountControls)];
  const originalControl = control(storedControl); const status = capitalStatusForBinding(originalControl, observed.connection, atNs);
  const capability = transportCapability(originalControl, observed.connection, atNs);
  const sourceFailure = transportControlFailure(originalControl, observed.connection);
  // /account.currency is the transport's declared reporting currency. A native
  // account base currency (possibly null) is a different field and cannot
  // silently replace the report unit in the captured decision basis.
  const currency = String(snapshot.currency ?? observed.view.currency ?? '');
  if (!currency) throw new Problem('account_currency_required');
  const observedAtNs = counter(snapshot.nativeClockNs, 'account_clock');
  if (BigInt(observedAtNs) > BigInt(atNs)) throw new Problem('account_snapshot_from_future', 409);
  // Source heartbeat is not receiver availability. Legacy facts have no receipt
  // clock and become available only at this capture, never retroactively.
  const snapshotReceivedAtNs = observed.snapshotReceivedAtNs === undefined ? atNs : counter(observed.snapshotReceivedAtNs, 'account_received_clock');
  if (BigInt(snapshotReceivedAtNs) < BigInt(observedAtNs) || BigInt(snapshotReceivedAtNs) > BigInt(atNs)) throw new Problem('account_snapshot_receipt_clock_invalid', 409);
  // Callers capture inside the official document transaction. Persist the
  // conservative first capture of a legacy fact so polling cannot create a new
  // scientific revision for the very same source sequence.
  observed.snapshotReceivedAtNs ??= snapshotReceivedAtNs;
  const statusDigest = identity('capital-status', physicalCapitalState(status));
  const bindingKey = transportBindingKey(observed.connection);
  const captures = storedControl?.pmStatusCaptures ? object(storedControl.pmStatusCaptures) : {};
  const previousStatus = captures[bindingKey] ? object(captures[bindingKey]) : null;
  const hasLocalReservation = Object.values(originalControl.commands).some(command => ['UNCERTAIN', 'PENDING', 'ACCEPTED'].includes(command.state));
  const capitalControlReceivedAtNs = sourceFailure ? counter(sourceFailure.receivedAtNs, 'capital_control_failure_received')
    : previousStatus?.statusDigest === statusDigest ? counter(previousStatus.availableAtNs, 'capital_status_received')
    : previousStatus === null && !hasLocalReservation && originalControl.receivedAtNs !== undefined ? counter(originalControl.receivedAtNs, 'capital_control_received')
      : storedControl ? atNs : snapshotReceivedAtNs;
  if (BigInt(capitalControlReceivedAtNs) > BigInt(atNs)) throw new Problem('capital_control_receipt_clock_invalid', 409);
  if (storedControl) { captures[bindingKey] = { statusDigest, availableAtNs: capitalControlReceivedAtNs }; storedControl.pmStatusCaptures = captures; }
  const capabilitiesReceivedAtNs = capability?.receivedAtNs ?? null;
  if (capabilitiesReceivedAtNs !== null && BigInt(capabilitiesReceivedAtNs) > BigInt(atNs)) throw new Problem('capital_capabilities_receipt_clock_invalid', 409);
  const availableAtNs = [snapshotReceivedAtNs, capitalControlReceivedAtNs, ...(capabilitiesReceivedAtNs ? [capabilitiesReceivedAtNs] : [])]
    .reduce((latest, value) => BigInt(value) > BigInt(latest) ? value : latest);
  const artifact = json({ snapshot, snapshotReceivedAtNs, capitalControlReceivedAtNs, capitalControl: status,
    ...(capability ? { capitalCapabilities: capability } : {}),
    ...(sourceFailure ? { capitalControlFailure: sourceFailure } : {}),
    observation: { id, connectionId: observed.connection.id, clientId: observed.connection.clientId, accountId: observed.connection.accountId, mode: observed.connection.mode } });
  const sourceRevision = identity('account-source', artifact);
  const evidence: Pm.EvidenceRef = { id: identity('evidence', sourceRevision), sourceKind: 'ACCOUNT', role: 'OBSERVED', runtime: null,
    objectKey: sourceRevision, contentDigest: null, mediaType: 'application/json', sourceRevision,
    eventInterval: null, availableAtNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.account-snapshot/1', visibility: 'OWNER_ONLY' };
  return { evidence, artifact, basis: { accountBindingId: id, accountSourceId: String(snapshot.accountSourceId ?? status.accountSourceId ?? observed.connection.accountId),
    observationId: String(snapshot.observationId ?? status.observationId ?? id), sequence: counter(snapshot.sequence, 'account_sequence'), observedAtNs, availableAtNs,
    controlRevision: String(status.controlRevision ?? ''), currency, equity: snapshot.equity === null || snapshot.equity === undefined ? null : { amount: decimal(snapshot.equity, 'account_equity'), currency },
    reserved: status.reserved === null ? null : { amount: decimal(status.reserved, 'account_reserved'), currency }, positionsEvidenceRef: evidence.id, outstandingCommandsRef: sourceRevision } };
}
