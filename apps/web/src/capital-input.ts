import type { CapitalExitAction, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
export function capitalAction(exit: CapitalExitView, action: CapitalExitAction) {
  return exit.action_availability[action] ?? { allowed: false, reason: 'capital_exit_action_unavailable' };
}
export function capitalEvidenceIsCurrent(exit: CapitalExitView, now = Date.now()): boolean {
  return exit.funds.withdrawability === 'VERIFIED' && !!exit.funds.verified_withdrawable_amount && !!exit.funds.evidence_valid_until
    && Date.parse(exit.funds.evidence_valid_until) > now;
}
