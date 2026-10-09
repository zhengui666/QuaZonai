import { describe, expect, it } from 'vitest';
import { exit, id, source, intent } from './capital-exit-fixtures';
import { exitActionAllowed, futureDeadline, resumeFormValues } from './capital-exit-form';
import type { ExitView } from './capital-exit-state';

const bounded: ExitView = {
  ...exit,
  scope: { kind: 'PORTFOLIO_SCOPE', amount: exit.scope.amount, currency: 'USD', stream_ids: [id, source], release_ids: [intent, id] },
  funds: { ...exit.funds, reserved_amount: { amount: '9007199254740992.000000001', currency: 'USD' } },
  policy: {
    kind: 'BOUNDED_LIMIT', deadline: '2026-10-07T02:03:04.123456789+02:30',
    legs: [{ instrument_id: 'BTC-USD.NATIVE', maximum_reduction_quantity: '9007199254740993.123456789', minimum_sell_price: '1234567890123456.123456789' }],
    max_execution_cost: { amount: '9007199254740991.123456789', currency: 'USD', reference_evidence_id: source },
  },
};

describe('capital exit resume form', () => {
  it('restores all constraints and remaining reservation as exact strings without mutating the record', () => {
    const original = structuredClone(bounded);
    const expected = {
      amount: '9007199254740992.000000001', currency: 'USD', scope: 'PORTFOLIO_SCOPE',
      streamIds: `${id}\n${source}`, releaseIds: `${intent}\n${id}`, policy: 'BOUNDED_LIMIT',
      instrument: 'BTC-USD.NATIVE', quantity: '9007199254740993.123456789', price: '1234567890123456.123456789',
      deadline: '2026-10-07T02:03:04.123456789+02:30', cost: '9007199254740991.123456789', costEvidence: source,
    };
    expect(resumeFormValues(bounded)).toEqual(expected);
    expect(resumeFormValues(structuredClone(bounded))).toEqual(expected);
    expect(bounded).toEqual(original);
    expect(futureDeadline(expected.deadline, Date.parse('2026-10-08T00:00:00Z'))).toBe(false);
  });
  it('clears prior portfolio and bounded fields when restoring a cash-only amount exit', () => {
    expect(resumeFormValues(exit)).toEqual({
      amount: exit.funds.reserved_amount.amount, currency: 'USD', scope: 'AMOUNT', streamIds: '', releaseIds: '',
      policy: 'CASH_ONLY', instrument: '', quantity: '', price: '', deadline: '', cost: '', costEvidence: '',
    });
  });
  it.each([0, 2])('does not truncate an unsupported %i-leg policy into a single-leg form', count => {
    const policy = bounded.policy;
    if (policy.kind !== 'BOUNDED_LIMIT') throw new Error('bounded fixture required');
    expect(resumeFormValues({ ...bounded, policy: { ...policy, legs: Array.from({ length: count }, () => policy.legs[0]!) } })).toBeUndefined();
  });
});

describe('capital exit action_state parity', () => {
  it.each([
    ['REQUESTED', false, true], ['FENCING', false, true], ['CANCELLING_OPENERS', false, true], ['REDUCING', false, true],
    ['WAITING_EVIDENCE', true, true], ['PAUSED', true, true], ['CANCELLING_EXIT', false, false],
    ['CANCELLED_RESERVED', true, false], ['RECONCILING_WITHDRAWAL', false, true], ['COMPLETED', false, false], ['BLOCKED', true, true],
  ] as const)('%s allows resume=%s, pause=%s', (state, resume, pause) => {
    expect(exitActionAllowed(state, 'RESUME')).toBe(resume);
    expect(exitActionAllowed(state, 'PAUSE')).toBe(pause);
    expect(exitActionAllowed(state, 'CANCEL')).toBe(state !== 'COMPLETED');
    expect(exitActionAllowed(state, 'RECONCILE_WITHDRAWAL')).toBe(state !== 'COMPLETED');
  });
});

it('requires an explicit future RFC 3339 deadline and preserves offsets and fractional strings', () => {
  const now = Date.parse('2026-10-08T00:00:00Z');
  for (const value of ['', 'invalid', '2026-10-08T00:00:00', '2026-10-07T00:00:00Z', '2026-10-08T00:00:00Z']) expect(futureDeadline(value, now)).toBe(false);
  expect(futureDeadline('2026-10-09T02:03:04.123456789+02:30', now)).toBe(true);
});
