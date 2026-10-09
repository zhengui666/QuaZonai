import type { ExitAction, ExitView } from './capital-exit-state';

export type FormValues = { amount: string; currency: string; scope: 'AMOUNT' | 'PORTFOLIO_SCOPE'; streamIds: string; releaseIds: string; policy: 'CASH_ONLY' | 'BOUNDED_LIMIT'; instrument: string; quantity: string; price: string; deadline: string; cost: string; costEvidence: string };

/** Restore exact strings, including an expired deadline. Never widen bounds or
 * silently discard legs unsupported by the existing single-leg execution path. */
export function resumeFormValues(exit: ExitView): FormValues | undefined {
  const policy = exit.policy;
  if (policy.kind === 'BOUNDED_LIMIT' && policy.legs.length !== 1) return undefined;
  const leg = policy.kind === 'BOUNDED_LIMIT' ? policy.legs[0]! : undefined;
  return {
    amount: exit.funds.reserved_amount.amount, currency: exit.funds.reserved_amount.currency,
    scope: exit.scope.kind,
    streamIds: exit.scope.kind === 'PORTFOLIO_SCOPE' ? exit.scope.stream_ids.join('\n') : '',
    releaseIds: exit.scope.kind === 'PORTFOLIO_SCOPE' ? exit.scope.release_ids.join('\n') : '',
    policy: policy.kind, instrument: leg?.instrument_id ?? '',
    quantity: leg?.maximum_reduction_quantity ?? '', price: leg?.minimum_sell_price ?? '',
    deadline: policy.kind === 'BOUNDED_LIMIT' ? policy.deadline : '',
    cost: policy.kind === 'BOUNDED_LIMIT' ? policy.max_execution_cost.amount : '',
    costEvidence: policy.kind === 'BOUNDED_LIMIT' ? policy.max_execution_cost.reference_evidence_id : '',
  };
}

// Mirror domain::capital_exit::action_state; the server remains authoritative.
export function exitActionAllowed(state: ExitView['state'], action: ExitAction['action']) {
  if (state === 'COMPLETED') return false;
  if (action === 'RESUME') return ['PAUSED', 'BLOCKED', 'WAITING_EVIDENCE', 'CANCELLED_RESERVED'].includes(state);
  if (action === 'PAUSE') return state !== 'CANCELLED_RESERVED' && state !== 'CANCELLING_EXIT';
  return true;
}

export function futureDeadline(value: string, now: number) {
  return /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/.test(value) && Date.parse(value) > now;
}
