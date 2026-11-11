import test from 'node:test';
import assert from 'node:assert/strict';
import { validateCapitalDocument as validateContract } from '../../../packages/contracts/src/capital-exit.ts';
import type { CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';
import { capitalControlEconomics, reconcileControl, validateCapitalDocument } from '../src/capital-exit.ts';
import type { AccountControl } from '../src/capital-exit.ts';
import { json } from '../src/facts.ts';
const money = (amount: string) => ({ amount, currency: 'USDC' });
function exit(): CapitalExitView {
  return { schema_version: 2, id: 'exit', revision: '1', account_source_id: 'source', environment: 'LIVE', command_id: 'command', account_control_revision: '7',
    state: 'WAITING_CASH', preparation_state: 'WAITING_CASH', reason_codes: [], policy: { kind: 'NATURAL_RELEASE', deadline: '2030-01-01T00:00:00Z' },
    scope: { kind: 'AMOUNT', amount: '100.000000000000000001', currency: 'USDC' }, remaining_trading: 'FENCED_PENDING_TARGET',
    funds: { requested_amount: money('100.000000000000000001'), reserved_amount: money('100.000000000000000001'), staged_for_withdrawal_amount: money('0'),
      reconciled_withdrawal_amount: money('0'), waived_amount: money('0'), released_cash_amount: money('0'), verified_withdrawable_amount: null, withdrawability: 'UNVERIFIED' },
    action_availability: { PAUSE: { allowed: true, reason: null } }, progress: [], events: [], created_at: '2026-11-11T00:00:00Z', updated_at: '2026-11-11T00:00:00Z' };
}
test('financial control boundary validation works without a science binary or data directory', async () => {
  await validateCapitalDocument({ dataDir: '/does-not-exist', jobBinary: '/does-not-exist' }, 'ACTION', { schema_version: 2, expected_revision: '1', action: 'PAUSE' });
  await validateCapitalDocument({ dataDir: '/does-not-exist', jobBinary: '/does-not-exist' }, 'ACTION', {
    schema_version: 2, expected_revision: '1', action: 'RECONCILE_EXTERNAL_TRANSFER', user_reported_amount: '1.000000000000000001', currency: 'USDC', external_transfer_ref: 'original-ledger-reference',
  });
  assert.throws(() => validateContract('ACTION', { schema_version: 2, expected_revision: '1', action: 'WITHDRAW', amount: '100' }), /capital_action_invalid/);
  assert.throws(() => validateContract('ACTION', { schema_version: 2, expected_revision: '1', action: 'PAUSE', user_reported_amount: '100' }), /unknown_field/);
});
test('managed reservation, external staging, actual withdrawal and waived requirements conserve exact plan value', () => {
  const view = exit(); validateContract('VIEW', view);
  view.funds.reserved_amount = money('40.000000000000000001'); view.funds.staged_for_withdrawal_amount = money('60'); validateContract('VIEW', view);
  view.funds.staged_for_withdrawal_amount = money('20'); view.funds.reconciled_withdrawal_amount = money('40'); validateContract('VIEW', view);
  view.funds.reserved_amount = money('0'); view.funds.staged_for_withdrawal_amount = money('0'); view.funds.waived_amount = money('60.000000000000000001'); validateContract('VIEW', view);
  view.funds.waived_amount = money('60'); assert.throws(() => validateContract('VIEW', view), /capital_obligation_mismatch/);
  const wrongScope = exit(); wrongScope.scope.amount = '101'; assert.throws(() => validateContract('VIEW', wrongScope), /scope_funds_mismatch/);
  const inventedWithdrawable = exit(); inventedWithdrawable.funds.verified_withdrawable_amount = money('101'); assert.throws(() => validateContract('VIEW', inventedWithdrawable), /exceeds_obligation/);
});
test('native progress and freshness can advance without changing the account financial control revision', () => {
  const original = exit(); const status = { capability: 'SUPPORTED' as const, reason: null, accountSourceId: 'source', observationId: 'observation', controlRevision: '7', reserved: original.funds.reserved_amount.amount, exits: [original] };
  const prior: AccountControl = { status, commands: {} };
  const advanced = structuredClone(status); advanced.exits[0]!.revision = '2'; advanced.exits[0]!.state = 'REDUCING'; advanced.exits[0]!.updated_at = '2026-11-11T00:00:01Z';
  assert.deepEqual(capitalControlEconomics(status), capitalControlEconomics(advanced));
  const fresh = { status: advanced, capabilities: json({ capitalExit: { version: 2 } }), native: {} };
  assert.equal(reconcileControl(prior, fresh).status?.exits[0]?.revision, '2');
  advanced.exits[0]!.funds.reserved_amount = money('99'); assert.throws(() => reconcileControl(prior, fresh), /capital_control_conflict/);
});
