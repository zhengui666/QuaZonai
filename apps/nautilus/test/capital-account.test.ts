import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import type { IncomingMessage } from 'node:http';
import { createCapitalAccount } from '../src/capital-account.ts';
import type { AccountOptions, NativeAccountRuntime, NativeAccountSnapshot } from '../src/account-runtime.ts';
import { parseHyperliquidAccount, units } from '../src/hyperliquid-account.ts';
import type { CapitalExitPlanningContext, CapitalExitPreview, CapitalExitView, CapitalExitPolicy } from '../../../packages/contracts/src/capital-exit.ts';
import { validateCapitalDocument } from '../../../packages/contracts/src/capital-exit.ts';

function snapshot(): NativeAccountSnapshot {
  const ns = String(BigInt(Date.now()) * 1_000_000n);
  return { ready: true, recovery_required: false, sequence: '1', native_version: '0.68.0', observed_at_ns: ns,
    observation_id: '01900000-0000-7000-8000-000000000002', native_session_id: 'original-native-session', native_trader_id: 'QZ-001', native_account_id: 'HYPERLIQUID-001',
    native_snapshot: { account_id: 'HYPERLIQUID-001', account_type: 'MARGIN', base_currency: null, balances: [], margins: [], unrealized_pnls: [], realized_pnls: [], total_equity: [], base_currency_equity: null,
      is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [], event_id: 'native-original-event', ts_event: ns, ts_init: ns },
    positions: [], instruments: [], orders: [], fills: [], cash: '1000', equity: '1000', currency: 'USDC', currency_precision: 6 };
}
class NativeProtocol implements NativeAccountRuntime {
  snapshot = snapshot(); requests: { operation: string; payload: Record<string, unknown> }[] = [];
  targets = new Map<string, Record<string, unknown>>(); loseTargetResponse = false; rejectTargetBeforeSubmit = false;
  async request(operation: string, payload: Record<string, unknown> = {}) {
    this.requests.push({ operation, payload });
    if (operation === 'SNAPSHOT') return structuredClone(this.snapshot) as unknown as Record<string, unknown>;
    if (operation === 'QUANTIZE_MONEY') return { amount: payload.amount, precision: this.snapshot.currency_precision };
    if (operation === 'TARGET') {
      if (this.rejectTargetBeforeSubmit) {
        this.targets.set(String(payload.id), { acknowledged: false, target_id: payload.id, epoch: payload.epoch, admission: 'NOT_SUBMITTED', state: 'REJECTED', reason: 'account_native_previous_target_buy_unsettled' });
        throw new Error('controlled_native_pre_submit_failure');
      }
      const receipt = { acknowledged: true, target_id: payload.id, epoch: payload.epoch }; this.targets.set(String(payload.id), receipt);
      if (this.loseTargetResponse) throw new Error('controlled_target_response_lost'); return receipt;
    }
    if (operation === 'TARGET_STATUS') return this.targets.get(String(payload.id)) ?? { acknowledged: false };
    return { applied: true };
  }
  async close() {}
}
const request = (method: string) => ({ method, headers: { 'x-qz-client-id': 'client' } }) as unknown as IncomingMessage;
const options: AccountOptions = { id: 'spot', clientId: 'client', accountId: 'HYPERLIQUID-001', mode: 'live', accountAddress: '0x' + '1'.repeat(40), instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'] };
function accountProof(total = '1000', withdrawable = '0', abstraction = 'default', fee: string | null = '0.001') {
  return parseHyperliquidAccount(options.accountAddress!, { balances: [{ coin: 'USDC', total, hold: '0' }] },
    { assetPositions: [], crossMarginSummary: { totalMarginUsed: '0' }, withdrawable }, abstraction, { userSpotCrossRate: fee });
}
test('ordinary spot free cash and the current withdrawal path remain distinct', () => {
  const proof = accountProof('1000', '0');
  assert.equal(proof.spot_free, '1000'); assert.equal(proof.withdrawable, '0');
  assert.equal(accountProof('1000', '0', 'unifiedAccount').reason_codes[0], 'hyperliquid_account_abstraction_unsupported');
  assert.equal(accountProof('1000', '0', 'default', null).spot_taker_fee_rate, null);
});

test('R/X/W/A conservation, transfer proof association and restart preserve physical identity', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-account-')), native = new NativeProtocol();
  let proof = accountProof(); const ledger: { time: number; hash: string; delta: Record<string, unknown> }[] = [];
  const reader = { async account() { return proof; }, async ledger() { return ledger; } };
  let account = await createCapitalAccount(options, directory, '/unused', native, reader);
  const call = async (method: string, path: string, body: unknown = {}) => {
    const value = (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
    if (value.result) validateCapitalDocument('VIEW', value.result);
    for (const exit of value.exits as CapitalExitView[] ?? []) validateCapitalDocument('VIEW', exit);
    return value;
  };
  const wrap = (requestId: string, value: unknown) => ({ requestId, clientId: options.clientId, accountId: options.accountId, ...(value && typeof value === 'object' && 'scope' in value ? { request: value } : { command: value }) });
  const status = async () => (await call('GET', '/capital-exits')).exits as CapitalExitView[];
  try {
    const context = (await call('GET', '/capital-exits/planning-context')).context as CapitalExitPlanningContext;
    const previewBody = wrap('preview', { schema_version: 2, account_source_id: context.account_source_id, expected_source_observation_id: context.observation_id,
      expected_account_control_revision: context.account_control_revision, planning_context_id: context.id, scope: { kind: 'AMOUNT', amount: '100', currency: 'USDC' }, policy: { kind: 'CASH_ONLY' } });
    const preview = (await call('POST', '/capital-exits/preview', previewBody)).preview as CapitalExitPreview;
    assert.equal(preview.capability, 'SUPPORTED');
    const start = wrap('start', { schema_version: 2, preview_id: preview.id, expected_account_control_revision: context.account_control_revision,
      acknowledged_plan_artifact_id: preview.plan_artifact_id, expected_source_observation_id: context.observation_id });
    await call('POST', '/capital-exits', start);
    let exit = (await status())[0]!; validateCapitalDocument('VIEW', exit);
    assert.equal(exit.funds.requested_amount.amount, '100'); assert.equal(exit.funds.reserved_amount.amount, '100');
    const unbudgeted = await call('POST', '/targets', { id: 'signal-while-reserved', clientId: options.clientId, accountId: options.accountId,
      expiresAt: new Date(Date.now() + 60_000).toISOString(), target: { valid_until_ns: String(BigInt(Date.now() + 60_000) * 1_000_000n), targets: [] } });
    assert.equal(unbudgeted.status, 'REJECTED'); assert.equal(unbudgeted.error, 'capital_snapshot_bound_target_allocation_required');
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 0);
    assert.deepEqual(await call('POST', '/capital-exits', start), await call('POST', '/capital-exits', start));
    ledger.push({ time: Date.now(), hash: 'platform-transfer-reference', delta: { type: 'accountClassTransfer', toPerp: true, usdc: '100' } });
    proof = accountProof('900', '100');
    await call('POST', `/capital-exits/${exit.id}/actions`, wrap('stage', { schema_version: 2, expected_revision: exit.revision, action: 'RECONCILE_EXTERNAL_TRANSFER', user_reported_amount: '100', currency: 'USDC', external_transfer_ref: 'platform-transfer-reference' }));
    exit = (await status())[0]!; validateCapitalDocument('VIEW', exit);
    assert.equal(exit.funds.requested_amount.amount, '100'); assert.equal(exit.funds.reserved_amount.amount, '0'); assert.equal(exit.funds.staged_for_withdrawal_amount.amount, '100');
    const control = native.requests.filter(row => row.operation === 'CONTROL').at(-1)!;
    assert.equal(control.payload.spot_free_cash, '900'); assert.equal(control.payload.reserved_amount, '0', 'The actual transfer and managed reservation are not subtracted twice.');
    ledger.push({ time: Date.now(), hash: 'platform-withdrawal-reference', delta: { type: 'withdraw', usdc: '100', fee: '1' } });
    proof = accountProof('900', '0');
    await call('POST', `/capital-exits/${exit.id}/actions`, wrap('withdraw', { schema_version: 2, expected_revision: exit.revision, action: 'RECONCILE_WITHDRAWAL', user_reported_amount: '100', currency: 'USDC', external_transfer_ref: 'platform-withdrawal-reference' }));
    exit = (await status())[0]!; validateCapitalDocument('VIEW', exit); assert.equal(exit.state, 'COMPLETED'); assert.equal(exit.funds.reconciled_withdrawal_amount.amount, '100');
    assert.equal(units(exit.funds.requested_amount.amount), ['reserved_amount', 'staged_for_withdrawal_amount', 'reconciled_withdrawal_amount', 'waived_amount'].reduce((sum, key) => sum + units((exit.funds as unknown as Record<string, { amount: string }>)[key]!.amount), 0n));
    await account.close(); account = await createCapitalAccount({ ...options, clientId: 'client' }, directory, '/unused', native, reader);
    assert.equal((await status())[0]!.funds.reconciled_withdrawal_amount.amount, '100');
    await account.close();
    await assert.rejects(createCapitalAccount({ ...options, accountAddress: '0x' + '2'.repeat(40) }, directory, '/unused', native, reader), /account_receipts_physical_identity_mismatch/);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('a known target admission rejection is queryable; a lost zero-order ACK reconciles after expiry and restart', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-target-receipt-')), native = new NativeProtocol();
  let account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001' }, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
  try {
    const current = await call('GET', '/account'), expiry = BigInt(Date.now() + 200) * 1_000_000n;
    const body = { id: 'original-target', clientId: options.clientId, accountId: options.accountId, expiresAt: new Date(Number(expiry / 1_000_000n)).toISOString(),
      target: { valid_until_ns: String(expiry), targets: [] }, allocation: { accountSourceId: current.accountSourceId, observationId: 'changed-source', snapshotSequence: current.sequence,
        nativeClockNs: current.nativeClockNs, controlRevision: current.controlRevision, reserved: '0', cashBuffer: '0', capital: '900' } };
    assert.equal((await call('POST', '/targets', body)).status, 'REJECTED');
    assert.equal((await call('GET', '/targets/original-target')).status, 'REJECTED');
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 0);
    native.loseTargetResponse = true;
    const accepted = { ...body, id: 'lost-ack', allocation: { ...body.allocation, observationId: current.observationId } };
    assert.equal((await call('POST', '/targets', accepted)).status, 'PENDING');
    assert.equal(native.targets.get('lost-ack')?.acknowledged, true);
    await new Promise(resolve => setTimeout(resolve, 220)); await account.close();
    account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001' }, directory, '/unused', native);
    assert.equal((await call('GET', '/targets/lost-ack')).status, 'ACKNOWLEDGED');
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 1, 'Reading an expired original acceptance never replays financial target execution.');
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('native confirmed zero-child failure ends the pending target and permits the next original target', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-target-not-submitted-')), native = new NativeProtocol();
  const account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001' }, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
  try {
    const current = await call('GET', '/account'), expiry = String(BigInt(Date.now() + 60_000) * 1_000_000n);
    const body = { id: 'no-financial-submit', clientId: options.clientId, accountId: options.accountId, expiresAt: new Date(Date.now() + 60_000).toISOString(),
      target: { valid_until_ns: expiry, targets: [] }, allocation: { accountSourceId: current.accountSourceId, observationId: current.observationId, snapshotSequence: current.sequence,
        nativeClockNs: current.nativeClockNs, controlRevision: current.controlRevision, reserved: '0', cashBuffer: '0', capital: '900' } };
    native.rejectTargetBeforeSubmit = true;
    assert.equal((await call('POST', '/targets', body)).status, 'PENDING');
    const rejected = await call('GET', '/targets/no-financial-submit');
    assert.equal(rejected.status, 'REJECTED'); assert.equal(rejected.acknowledgedAt, null);
    assert.equal(rejected.error, 'account_native_previous_target_buy_unsettled');
    assert.equal((await call('POST', '/targets', body)).status, 'REJECTED');
    native.rejectTargetBeforeSubmit = false;
    assert.equal((await call('POST', '/targets', { ...body, id: 'next-original-target' })).status, 'ACKNOWLEDGED');
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 2, 'No replay of the negative original is needed to unblock target delivery.');
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('an initial weight signal derives and persists a budget from current native equity and the configured cash buffer', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-target-signal-budget-')), native = new NativeProtocol();
  const account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001', cashBuffer: '100' }, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
  try {
    const body = { id: 'ordinary-signal', clientId: options.clientId, accountId: options.accountId, expiresAt: new Date(Date.now() + 60_000).toISOString(),
      target: { valid_until_ns: String(BigInt(Date.now() + 60_000) * 1_000_000n), targets: [] } };
    assert.equal((await call('POST', '/targets', body)).status, 'ACKNOWLEDGED');
    assert.equal(native.requests.find(row => row.operation === 'TARGET')?.payload.capital, '900');
    const saved = JSON.parse(await readFile(join(directory, 'capital-receipts.json'), 'utf8'));
    assert.deepEqual(saved.targets[body.id].original, body);
    assert.equal(saved.targets[body.id].budget_basis.capital, '900'); assert.equal(saved.targets[body.id].budget_basis.origin, 'CURRENT_NATIVE_ACCOUNT');
    assert.equal(saved.targets[body.id].budget_basis.observationId, native.snapshot.observation_id);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('a favorable current bid permits a zero-loss preview and START using the same executable slice bounds', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-preview-zero-loss-')), native = new NativeProtocol(), instrumentId = options.instruments[0]!;
  Object.assign(native.snapshot, { cash: '0', equity: '200', positions: [{ instrument_id: instrumentId, quantity: '2', free_quantity: '2', value: '200', pnl: null }],
    instruments: [{ instrument_id: instrumentId, base_currency: 'HYPE', quote_currency: 'USDC', size_increment: '0.001', price_increment: '0.001',
      minimum_quantity: null, minimum_notional: '10', taker_fee_rate: '0.001', bid: '100', ask: '100.1', asof_ns: native.snapshot.observed_at_ns, bid_depth: [{ price: '100', quantity: '1000' }] }] });
  const account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001' }, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
  try {
    const context = (await call('GET', '/capital-exits/planning-context')).context as CapitalExitPlanningContext;
    const instrument = native.snapshot.instruments[0]!;
    Object.assign(instrument, { bid: '101', ask: '101.1', asof_ns: String(BigInt(Date.now()) * 1_000_000n), bid_depth: [{ price: '101', quantity: '1000' }] });
    native.snapshot.equity = '202';
    const value = (await call('POST', '/capital-exits/preview', { requestId: 'zero-loss-preview', clientId: options.clientId, accountId: options.accountId,
      request: { schema_version: 2, account_source_id: context.account_source_id, expected_source_observation_id: context.observation_id,
        expected_account_control_revision: context.account_control_revision, planning_context_id: context.id, scope: { kind: 'AMOUNT', amount: '100', currency: 'USDC' },
        policy: { kind: 'BOUNDED_LIMIT', deadline: new Date(Date.now() + 60_000).toISOString(), legs: [{ instrument_id: instrumentId, maximum_reduction_quantity: '2', minimum_sell_price: '100' }],
          max_execution_cost: { amount: '0', currency: 'USDC', reference_evidence_id: context.id }, pacing: { minimum_interval_seconds: 5, maximum_slice_notional: '20', maximum_participation: '0.1' } } } })).preview as CapitalExitPreview;
    assert.equal(value.capability, 'SUPPORTED', value.reason_codes.join(',')); assert.equal(value.funds.estimated_execution_cost!.amount, '0');
    assert.equal(value.execution_estimate_scope, 'CURRENT_EXECUTABLE_SLICE');
    const started = await call('POST', '/capital-exits', { requestId: 'zero-loss-start', clientId: options.clientId, accountId: options.accountId,
      command: { schema_version: 2, preview_id: value.id, expected_account_control_revision: context.account_control_revision,
        acknowledged_plan_artifact_id: value.plan_artifact_id, expected_source_observation_id: context.observation_id } });
    assert.equal(started.status, 'ACCEPTED', String(started.error)); validateCapitalDocument('VIEW', started.result);
    assert.deepEqual((started.result as CapitalExitView).policy, value.policy, 'A stronger current executable floor never weakens or rewrites the authorized bounds.');
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('a natural release plan cannot be combined with active liquidation on the same account', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-policy-')), native = new NativeProtocol();
  const account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001' }, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
  const wrap = (requestId: string, value: unknown) => ({ requestId, clientId: options.clientId, accountId: options.accountId, ...(value && typeof value === 'object' && 'scope' in value ? { request: value } : { command: value }) });
  const preview = async (requestId: string, policy: CapitalExitPolicy) => {
    const context = (await call('GET', '/capital-exits/planning-context')).context as CapitalExitPlanningContext;
    return { context, value: (await call('POST', '/capital-exits/preview', wrap(requestId, { schema_version: 2, account_source_id: context.account_source_id, expected_source_observation_id: context.observation_id,
      expected_account_control_revision: context.account_control_revision, planning_context_id: context.id, scope: { kind: 'AMOUNT', amount: '100', currency: 'USDC' }, policy }))).preview as CapitalExitPreview };
  };
  try {
    const first = await preview('natural', { kind: 'NATURAL_RELEASE', deadline: new Date(Date.now() + 3_600_000).toISOString() });
    await call('POST', '/capital-exits', wrap('start', { schema_version: 2, preview_id: first.value.id, expected_account_control_revision: first.context.account_control_revision,
      acknowledged_plan_artifact_id: first.value.plan_artifact_id, expected_source_observation_id: first.context.observation_id }));
    const context = (await call('GET', '/capital-exits/planning-context')).context as CapitalExitPlanningContext;
    const second = await preview('active', { kind: 'BOUNDED_LIMIT', deadline: new Date(Date.now() + 3_600_000).toISOString(), legs: [{ instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID', maximum_reduction_quantity: '1', minimum_sell_price: '1' }],
      max_execution_cost: { amount: '1', currency: 'USDC', reference_evidence_id: context.id }, pacing: { minimum_interval_seconds: 5, maximum_slice_notional: '10', maximum_participation: '0.1' } });
    assert.ok(second.value.reason_codes.includes('capital_exit_policy_conflict'));
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});
