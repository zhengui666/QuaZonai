import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, test } from 'vitest';
import type { CapitalExitPlanningContext, CapitalExitPreview, CapitalExitStatus, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import { capitalAction, capitalEvidenceIsCurrent, capitalPreviewInput, capitalPreviewIsCurrent, capitalResumePreviewMatches, suggestedCapitalSlice } from './capital-input';
import { CapitalFundsSummary } from './account-controls';

const now = Date.parse('2026-11-11T12:00:00Z');
const money = (amount: string) => ({ amount, currency: 'USDC' });
const context: CapitalExitPlanningContext = {
  schema_version: 2, id: 'native-context', account_source_id: 'native-account', observation_id: 'original-observation', account_control_revision: '4',
  currency: 'USDC', environment: 'LIVE', asof: '2026-11-11T12:00:00Z', valid_until: '2026-11-11T12:01:00Z',
  funds: { total_cash: money('40'), free_cash: money('30'), locked_cash: money('10'), verified_withdrawable: null },
  instruments: [{ instrument_id: '@107.HYPERLIQUID', symbol: 'BTC/USDC', quantity: '9007199254740995.000000000000000001', eligible_quantity: '9007199254740995.000000000000000001', quantity_increment: '0.000000000000000001', price_increment: '0.01', bid_price: '100.01', ask_price: '100.02', quote_asof: '2026-11-11T12:00:00Z', fee_rate: '0.0001', reference_evidence_id: 'native-context' }],
  supported_policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'], capability: 'SUPPORTED', reason_codes: [],
};
const exit: CapitalExitView = {
  schema_version: 2, id: 'exit-original', revision: '3', account_source_id: 'native-account', environment: 'LIVE', state: 'READY', preparation_state: 'WAITING_EXTERNAL_TRANSFER', account_control_revision: '4', command_id: 'original-command', reason_codes: [],
  scope: { kind: 'AMOUNT', amount: '100.000000000000000001', currency: 'USDC' }, policy: { kind: 'NATURAL_RELEASE', deadline: '2026-11-12T12:00:00Z' }, remaining_trading: 'BUDGET_AWARE',
  funds: { requested_amount: money('100.000000000000000001'), reserved_amount: money('60.000000000000000001'), staged_for_withdrawal_amount: money('20'), waived_amount: money('10'), released_cash_amount: money('60.000000000000000001'), verified_withdrawable_amount: money('60.000000000000000001'), reconciled_withdrawal_amount: money('10'), withdrawability: 'UNVERIFIED', evidence_asof: '2026-11-11T12:00:00Z', evidence_valid_until: '2026-11-11T12:01:00Z' },
  action_availability: { CANCEL: { allowed: true, reason: null }, RELEASE_RESERVATION: { allowed: false, reason: 'capital_exit_release_requires_cancel' } }, progress: [], events: [], created_at: '2026-11-11T12:00:00Z', updated_at: '2026-11-11T12:00:00Z',
};
const status: CapitalExitStatus = { capability: 'SUPPORTED', reason: null, accountSourceId: 'native-account', observationId: 'new-observation', controlRevision: '4', reserved: '20', exits: [exit] };
const preview: CapitalExitPreview = {
  schema_version: 2, id: 'approved-preview', plan_artifact_id: 'approved-plan', account_source_id: 'native-account', original_observation_id: 'original-observation', expected_account_control_revision: '4', environment: 'LIVE', scope: exit.scope, policy: exit.policy,
  capability: 'SUPPORTED', reason_codes: [], valid_until: '2026-11-11T12:01:00Z', funds: { requested: exit.funds.requested_amount, remaining_managed_capital: null, estimated_execution_cost: null }, proposed_cancellations: [], retained_protective_orders: [], reduction_legs: [],
};

test('natural release preserves exact request and carries no active sell authority', () => {
  const result = capitalPreviewInput({ amount: '100.000000000000000001', policy: 'NATURAL_RELEASE', deadline: '2026-11-12T12:00:00Z' }, context, now);
  expect(result).toEqual({ amount: '100.000000000000000001', currency: 'USDC', contextId: 'native-context', policy: { kind: 'NATURAL_RELEASE', deadline: '2026-11-12T12:00:00.000Z' } });
  expect(() => capitalPreviewInput({ amount: '100', policy: 'NATURAL_RELEASE', deadline: '2026-11-10T12:00:00Z' }, context, now)).toThrow('deadline');
});
test('bounded plan uses native context identity and exact native increments', () => {
  const fields = { amount: '100', policy: 'BOUNDED_LIMIT' as const, deadline: '2026-11-12T12:00:00Z', maxCost: '0.000000000000000001', maxSlice: '9', minimumInterval: '5', participation: '0.1', legs: [{ instrumentId: '@107.HYPERLIQUID', quantity: '9007199254740995.000000000000000001', minimumPrice: '100.01' }] };
  const result = capitalPreviewInput(fields, context, now);
  if (result.policy.kind !== 'BOUNDED_LIMIT') throw new Error('bounded policy required');
  expect(result.policy.legs[0]?.maximum_reduction_quantity).toBe(fields.legs[0]!.quantity);
  expect(result.policy.max_execution_cost.reference_evidence_id).toBe(context.id);
  expect(result.policy.pacing).toEqual({ minimum_interval_seconds: 5, maximum_slice_notional: '9', maximum_participation: '0.1' });
  expect(() => capitalPreviewInput({ ...fields, participation: '0.100000000000000001' }, context, now)).toThrow('participation');
  expect(() => capitalPreviewInput({ ...fields, minimumInterval: '4' }, context, now)).toThrow('interval');
  expect(() => capitalPreviewInput({ ...fields, legs: [{ ...fields.legs[0]!, minimumPrice: '100.011' }] }, context, now)).toThrow('price_increment');
  expect(() => capitalPreviewInput({ ...fields, legs: [fields.legs[0]!, fields.legs[0]!] }, context, now)).toThrow('instrument_invalid');
  expect(() => capitalPreviewInput(fields, { ...context, valid_until: 'invalid' }, now)).toThrow('context_expired');
});
test('slice suggestions account for existing spot reservation without rounding up dust', () => {
  expect(suggestedCapitalSlice('100.000000000000000011', '30', '20')).toBe('9.000000000000000001');
  expect(suggestedCapitalSlice('0.000000000000000001', '0', '0')).toBe('0');
  expect(suggestedCapitalSlice('100', null, '20')).toBeUndefined();
  expect(suggestedCapitalSlice('10', '30', '20')).toBeUndefined();
});
test('new observations retain approved basis but changed control or account invalidates preview', () => {
  expect(capitalPreviewIsCurrent(preview, status, now)).toBe(true);
  expect(capitalPreviewIsCurrent(preview, { ...status, controlRevision: '5' }, now)).toBe(false);
  expect(capitalPreviewIsCurrent(preview, { ...status, accountSourceId: 'other-account' }, now)).toBe(false);
  expect(capitalPreviewIsCurrent(preview, status, Date.parse(preview.valid_until))).toBe(false);
  const resumed: CapitalExitPreview = { ...preview, resume_exit_id: exit.id, expected_exit_revision: exit.revision, scope: { ...preview.scope, amount: exit.funds.reserved_amount.amount } };
  expect(capitalResumePreviewMatches(exit, resumed, status, exit.id, now)).toBe(true);
  expect(capitalResumePreviewMatches(exit, preview, status, exit.id, now)).toBe(false);
  expect(capitalResumePreviewMatches(exit, resumed, status, 'other-plan', now)).toBe(false);
  expect(capitalResumePreviewMatches(exit, { ...resumed, expected_exit_revision: '2' }, status, exit.id, now)).toBe(false);
  expect(capitalResumePreviewMatches(exit, { ...resumed, scope: { ...resumed.scope, amount: exit.scope.amount } }, status, exit.id, now)).toBe(false);
  expect(capitalResumePreviewMatches(exit, { ...resumed, scope: { ...resumed.scope, currency: 'USD' } }, status, exit.id, now)).toBe(false);
  expect(capitalResumePreviewMatches(exit, { ...resumed, environment: 'PAPER' }, status, exit.id, now)).toBe(false);
});
test('resume preparation binds the current remaining spot reservation and original plan revision', () => {
  const paused: CapitalExitView = { ...exit, state: 'PAUSED', action_availability: { RESUME: { allowed: true, reason: null } } };
  const fields = { amount: paused.funds.reserved_amount.amount, policy: 'NATURAL_RELEASE' as const, deadline: '2026-11-12T12:00:00Z' };
  const result = capitalPreviewInput(fields, context, now, paused);
  expect(result.amount).toBe('60.000000000000000001');
  expect(result.resumeExitId).toBe(paused.id); expect(result.expectedExitRevision).toBe(paused.revision);
  expect(() => capitalPreviewInput({ ...fields, amount: paused.scope.amount }, context, now, paused)).toThrow('resume_basis_changed');
  expect(() => capitalPreviewInput(fields, { ...context, account_source_id: 'other' }, now, paused)).toThrow('resume_basis_changed');
  expect(() => capitalPreviewInput(fields, { ...context, environment: 'PAPER' }, now, paused)).toThrow('resume_basis_changed');
});
test('existing stop actions remain available without an initiation switch', () => {
  expect(capitalAction(exit, 'CANCEL').allowed).toBe(true);
  expect(capitalAction(exit, 'START').allowed).toBe(false);
  expect(capitalAction(exit, 'RELEASE_RESERVATION').reason).toBe('capital_exit_release_requires_cancel');
});
test('cash-ready and external transfers never become verified withdrawal or wallet arrival', () => {
  const html = renderToStaticMarkup(createElement(CapitalFundsSummary, { exit, now }));
  expect(html).toContain('现货 USDC 已备妥');
  expect(html).toContain('100.000000000000000001');
  expect(html).toContain('已外部划转，尚待提取');
  expect(html).toContain('未知，不能用现货余额代替');
  expect(html).toContain('已取消的退出金额');
  expect(capitalEvidenceIsCurrent(exit, now)).toBe(false);
  const verified = { ...exit, funds: { ...exit.funds, withdrawability: 'VERIFIED' as const } };
  expect(capitalEvidenceIsCurrent(verified, now)).toBe(true);
  expect(capitalEvidenceIsCurrent(verified, Date.parse(verified.funds.evidence_valid_until!))).toBe(false);
  expect(capitalEvidenceIsCurrent({ ...verified, funds: { ...verified.funds, withdrawability: 'SIMULATED' } }, now)).toBe(false);
});
test('real fees remain separate from economic loss and retain native decimal precision', () => {
  const withFees = { ...exit, funds: { ...exit.funds, actual_fees: money('0.000000000000000007') } };
  const html = renderToStaticMarkup(createElement(CapitalFundsSummary, { exit: withFees, now }));
  expect(html).toContain('真实手续费（单列）');
  expect(html).toContain('0.000000000000000007');
  expect(html).toContain('有利价格只抵本笔费用，不抵销过去损失');
  expect(html).toContain('不再与经济损失相加');
  const unknown = renderToStaticMarkup(createElement(CapitalFundsSummary, { exit, now }));
  expect(unknown).toMatch(/真实手续费（单列）<\/dt><dd><span[^>]*>尚未核实/);
});
