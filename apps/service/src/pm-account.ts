import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
import { counter, decimal, json, nowNs, object, observation, Problem } from './facts.ts';
import { accountKey, capitalStatusForBinding, control, physicalCapitalState, transportBindingKey, transportCapability, transportControlFailure } from './capital-exit.ts';
import { identity } from './pm-facts.ts';

export interface CapturedAccount { basis: Pm.AccountBasis; evidence: Pm.EvidenceRef; artifact: JsonObject }

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
