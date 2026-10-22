import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { CapturedAccount } from './pm-account.ts';
import { nowNs } from './facts.ts';

/** ACK proves reception, not cancellation or completed execution. A received
 * target retains responsibility until it can no longer act and the account
 * reports a later authoritative state. Definitive original receipts keep
 * their own meaning; a local timer never rewrites a command as EXPIRED. */
export function retirementOutstandingTargets(commands: readonly Pm.TargetCommand[], account?: CapturedAccount,
  atNs = nowNs(), artifacts: Record<string, JsonObject> = {}): string[] {
  return commands.filter(command => {
    if (['PENDING', 'UNCERTAIN'].includes(command.state)) return true;
    if (command.state !== 'ACKNOWLEDGED' && command.state !== 'REJECTED') return false;
    if (!account) return true;
    const receiptClock = artifacts[`target-receipt-clock:${command.id}`];
    // A revocation can follow an earlier fill. Its receipt ends future target
    // authority but cannot make an older flat snapshot current. If an upgraded
    // record has no original receipt clock, retain the original TTL fallback.
    const resolutionClock = command.state === 'REJECTED' && receiptClock
      && ['REJECTED', 'REVOKED', 'EXPIRED'].includes(String(receiptClock.status))
      && typeof receiptClock.availableAtNs === 'string' && /^\d+$/.test(receiptClock.availableAtNs)
      ? receiptClock.availableAtNs : command.validUntilNs;
    const boundary = BigInt(resolutionClock);
    return BigInt(atNs) < boundary || BigInt(account.basis.observedAtNs) <= boundary || BigInt(account.basis.availableAtNs) <= boundary;
  }).map(command => command.id);
}
