import type { JsonObject } from '@earendil-works/pi-durable';
import type { ConnectionConfig } from './config.ts';
import { accountKey } from './capital-exit.ts';
import { nowNs, observation, Problem } from './facts.ts';
import type { PmRecord, PmState } from './pm-facts.ts';
import { captureAccount } from './pm-account.ts';
import type { CapturedAccount } from './pm-account.ts';
import { campaignAccount, campaignAccountKey } from './pm-campaign-account.ts';
import { retirementAccountFlat } from './pm-account-native.ts';

type AccountFacts = { observations: Record<string, JsonObject>; accountControls?: Record<string, JsonObject> };
type Controller = { kind: 'PM'; mandateId: string; accountBindingId: string; commandId?: string }
  | { kind: 'LEGACY'; observationId?: string; commandId?: string };
const unsettled = (state: string | undefined) => ['PENDING', 'UNCERTAIN', 'SIZED'].includes(state ?? '');

/** Reception leaves a valid target executable. Unknown expiry cannot establish
 * that a previous controller's external responsibility has ended. */
function receiptExpiryNs(expiresAt: string): bigint | null {
  const match = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(?:\.(\d{1,9}))?Z$/.exec(expiresAt);
  if (!match) return null;
  const seconds = Date.parse(`${match[1]}Z`);
  return Number.isFinite(seconds) ? BigInt(seconds) * 1_000_000n + BigInt((match[2] ?? '').padEnd(9, '0')) : null;
}
export function activeAcknowledgedReceipt(receipt: { status: string; expiresAt: string } | null | undefined, atNs = nowNs()): boolean {
  if (receipt?.status !== 'ACKNOWLEDGED') return false;
  const expiry = receiptExpiryNs(receipt.expiresAt);
  return expiry === null || expiry > BigInt(atNs);
}

/** Expiry ends future authority, not the possibility of an earlier fill. A
 * later native flat account closes that responsibility. Recheck every original
 * receipt's own expiry; a different target gets no blanket prior exemption. */
function legacyHandoverClosed(facts: AccountFacts, pm: PmState, claimant: Extract<Controller, { kind: 'PM' }>,
  originalBinding: ReturnType<typeof observation>, receipt: { status: string; expiresAt: string }): boolean {
  if (activeAcknowledgedReceipt(receipt)) return false;
  const expiry = receiptExpiryNs(receipt.expiresAt); if (expiry === null) return false;
  const originalSourceId = originalBinding.snapshot?.accountSourceId;
  if (typeof originalSourceId !== 'string') return false;
  const candidates: CapturedAccount[] = [];
  const own = pm.mandates[claimant.mandateId];
  if (own?.mandate.capitalSource.kind === 'ACCOUNT' && own.mandate.capitalSource.accountBindingId === claimant.accountBindingId) {
    const first = Object.values(own.campaigns)[0];
    // The immutable admission capture proves an already completed handover
    // even after this same PM has acquired its own inventory. A later research
    // campaign cannot substitute a different starting account for that proof.
    if (first && own.artifacts[campaignAccountKey(first.id)]) {
      try { candidates.push(campaignAccount(own, first)); } catch { /* An invalid original proof cannot release authority. */ }
    }
  }
  try { candidates.push(captureAccount(facts, claimant.accountBindingId)); } catch { /* Missing current evidence leaves the obligation open. */ }
  return candidates.some(capture => capture.basis.accountSourceId === originalSourceId
    && BigInt(capture.basis.observedAtNs) > expiry
    && BigInt(String(capture.artifact.snapshotReceivedAtNs)) > expiry
    && retirementAccountFlat(capture).flat === true);
}

/** A mandate combines its strategies before allocating this account. Two
 * independent controllers cannot each allocate the same NAV. Pause retains
 * ownership; retirement releases it only after its original duties settle. */
function ownsAccount(record: PmRecord): boolean {
  if (record.mandate.operatingAuthority !== 'AUTHORIZED_TARGETS') return false;
  const retirements = Object.values(record.retirements);
  return !retirements.some(item => item.strategyVersionId === record.latestStrategyId)
    || retirements.some(item => item.reconciliationRequired);
}

/** Read both original documents inside the caller's Durable transaction. No
 * separate lock store or remote reservation protocol is introduced. Existing
 * uncertain identities are reconciled without invoking this admission guard. */
export function assertAccountTargetControl(facts: AccountFacts, pm: PmState, connection: ConnectionConfig, claimant: Controller): void {
  const key = accountKey(connection, facts.accountControls);
  const sameAccount = (bindingId: string) => {
    const binding = facts.observations[bindingId];
    return binding && accountKey(observation(binding).connection, facts.accountControls) === key;
  };
  for (const record of Object.values(pm.mandates)) {
    if (record.mandate.capitalSource.kind === 'ACCOUNT' && sameAccount(record.mandate.capitalSource.accountBindingId)
      && ownsAccount(record) && !(claimant.kind === 'PM' && claimant.mandateId === record.mandate.id)) {
      throw new Problem('account_pm_controller_already_authorized', 409);
    }
    for (const command of Object.values(record.commands)) {
      const sameController = claimant.kind === 'PM' && claimant.mandateId === record.mandate.id;
      if (sameAccount(command.accountBindingId) && (unsettled(command.state)
        || !sameController && command.state === 'ACKNOWLEDGED' && BigInt(command.validUntilNs) > BigInt(nowNs()))
        && !(claimant.kind === 'PM' && claimant.mandateId === record.mandate.id && claimant.commandId === command.id)) {
        throw new Problem('target_delivery_requires_reconciliation', 409);
      }
    }
  }
  for (const value of Object.values(facts.observations)) {
    const binding = observation(value);
    if (accountKey(binding.connection, facts.accountControls) !== key) continue;
    const self = claimant.kind === 'LEGACY' && claimant.observationId === binding.view.id;
    if (binding.view.automaticPolicy?.enabled && binding.view.status !== 'STOPPED' && !self) {
      throw new Problem('account_automatic_binding_already_enabled', 409);
    }
    const original = binding.view.receipt;
    const allocation = binding.view.allocation;
    if (claimant.kind === 'PM') {
      for (const receipt of [original, allocation?.receipt]) if (receipt?.status === 'ACKNOWLEDGED'
        && !legacyHandoverClosed(facts, pm, claimant, binding, receipt)) {
        throw new Problem(activeAcknowledgedReceipt(receipt) ? 'previous_account_controller_target_still_active'
          : 'previous_account_controller_requires_post_expiry_flat_account', 409);
      }
    }
    if (unsettled(original?.status) && !(self && claimant.commandId !== undefined && claimant.commandId === original?.id)
      || (unsettled(allocation?.status) || unsettled(allocation?.receipt?.status))
        && !(self && claimant.commandId !== undefined && claimant.commandId === allocation?.receipt?.id)) {
      throw new Problem('target_delivery_requires_reconciliation', 409);
    }
  }
}
