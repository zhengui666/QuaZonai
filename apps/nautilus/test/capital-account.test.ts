import { fencedBody } from './strategy-fence.ts';
import assert from 'node:assert/strict';
import type { NativeOrderPreview } from '../../../packages/contracts/src/native-order-preview.ts';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import type { IncomingMessage } from 'node:http';
import { createCapitalAccount } from '../src/capital-account.ts';
import type { AccountOptions, NativeAccountRuntime, NativeAccountSnapshot } from '../src/account-runtime.ts';
import { parseHyperliquidAccount, units } from '../src/hyperliquid-account.ts';
import type { CapitalExitPlanningContext, CapitalExitPreview, CapitalExitView, CapitalExitPolicy } from '../../../packages/contracts/src/capital-exit.ts';
import type { NativeFixedRiskSizingRequest } from '../../../packages/contracts/src/nautilus.ts';
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
    if (operation === 'STRATEGY_START' || operation === 'STRATEGY_STOP') return { strategy_id: 'EMACross-001', state: operation === 'STRATEGY_START' ? 'RUNNING' : 'STOPPED', native_version: '0.69.0' };
    if (operation === 'TARGET_STATUS') return this.targets.get(String(payload.id)) ?? { acknowledged: false };
    return { applied: true };
  }
  async close() {}
}
const request = (method: string) => ({ method, headers: { 'x-qz-client-id': 'client' } }) as unknown as IncomingMessage;
const options: AccountOptions = { id: 'spot', clientId: 'client', accountId: 'HYPERLIQUID-001', mode: 'live', accountAddress: '0x' + '1'.repeat(40), nativeStrategy: { instrument_id: 'HYPE-USDC-SPOT.HYPERLIQUID', strategy: { kind: 'EMA_CROSS', trade_size: '1', fast_period: 10, slow_period: 50 } }, instruments: ['HYPE-USDC-SPOT.HYPERLIQUID'] };
function accountProof(total = '1000', withdrawable = '0', abstraction = 'default', fee: string | null = '0.001') {
  return parseHyperliquidAccount(options.accountAddress!, { balances: [{ coin: 'USDC', total, hold: '0' }] },
    { assetPositions: [], crossMarginSummary: { totalMarginUsed: '0' }, withdrawable }, abstraction, { userSpotCrossRate: fee });
}
test('raw spot totals and withdrawal stay distinct without deriving free cash', () => {
  const proof = accountProof('1000', '0');
  assert.equal(proof.spot_free, null); assert.equal(proof.withdrawable, '0');
  assert.equal(proof.spot_total, '1000'); assert.equal(proof.spot_hold, '0');
  assert.deepEqual(proof.original.spot, { balances: [{ coin: 'USDC', total: '1000', hold: '0' }] });
  assert.equal(accountProof('1000', '0', 'unifiedAccount').reason_codes[0], 'hyperliquid_account_abstraction_unsupported');
  assert.equal(accountProof('1000', '0', 'default', null).spot_taker_fee_rate, null);
});

test('retained rejection and lost original zero-order ACK reconcile after expiry and restart without a new target', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-target-receipt-')), native = new NativeProtocol();
  const configured = { ...options, mode: 'paper' as const, startingBalance: '1000', paperFeeRate: '0.001' };
  let account = await createCapitalAccount(configured, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => fencedBody(account, method, path, body, request))).value as Record<string, unknown>;
  try {
    await call('GET', '/account'); await account.close();
    // Persist only the original pre-upgrade receipt input. Recovery itself must
    // query the native protocol, not synthesize an acknowledged financial action.
    const file = join(directory, 'capital-receipts.json'), saved = JSON.parse(await readFile(file, 'utf8'));
    const original = { id: 'lost-ack', clientId: options.clientId, accountId: options.accountId,
      target: { valid_until_ns: String(BigInt(Date.now() - 1) * 1_000_000n), targets: [] } };
    saved.targets['original-target'] = { epoch: saved.epoch, original: { ...original, id: 'original-target' }, receipt: { status: 'REJECTED', error: 'target_observation_mismatch', acknowledgedAt: null } };
    saved.targets['lost-ack'] = { epoch: saved.epoch, original, receipt: { status: 'PENDING', acknowledgedAt: null } };
    await writeFile(file, JSON.stringify(saved));
    native.targets.set('lost-ack', { acknowledged: true, target_id: 'lost-ack', epoch: saved.epoch });
    account = await createCapitalAccount(configured, directory, '/unused', native);
    assert.equal((await call('GET', '/targets/original-target')).status, 'REJECTED');
    assert.equal((await call('GET', '/targets/lost-ack')).status, 'ACKNOWLEDGED');
    await account.close(); account = await createCapitalAccount(configured, directory, '/unused', native);
    assert.equal((await call('GET', '/targets/lost-ack')).status, 'ACKNOWLEDGED');
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 0, 'Reading an expired original acceptance never replays financial target execution.');
    assert.deepEqual(JSON.parse(await readFile(file, 'utf8')).targets['lost-ack'].original, original);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('native confirmed zero-child failure settles a retained target before a new native strategy can start', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-target-not-submitted-')), native = new NativeProtocol();
  const configured = { ...options, mode: 'paper' as const, startingBalance: '1000', paperFeeRate: '0.001' };
  let account = await createCapitalAccount(configured, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => fencedBody(account, method, path, body, request))).value as Record<string, unknown>;
  try {
    await call('GET', '/account'); await account.close();
    const file = join(directory, 'capital-receipts.json'), saved = JSON.parse(await readFile(file, 'utf8'));
    saved.targets['no-financial-submit'] = { epoch: saved.epoch, original: { id: 'no-financial-submit', target: { valid_until_ns: String(BigInt(Date.now() + 60_000) * 1_000_000n), targets: [] } }, receipt: { status: 'PENDING', acknowledgedAt: null } };
    await writeFile(file, JSON.stringify(saved)); account = await createCapitalAccount(configured, directory, '/unused', native);
    const start = { requestId: 'next-native-start', action: 'START', clientId: options.clientId, accountId: options.accountId };
    await assert.rejects(call('POST', '/strategy/commands', start), /strategy_original_target_unsettled/);
    native.targets.set('no-financial-submit', { acknowledged: false, target_id: 'no-financial-submit', epoch: saved.epoch, admission: 'NOT_SUBMITTED', state: 'REJECTED', reason: 'account_native_previous_target_buy_unsettled' });
    const rejected = await call('GET', '/targets/no-financial-submit');
    assert.equal(rejected.status, 'REJECTED'); assert.equal(rejected.acknowledgedAt, null);
    assert.equal(rejected.error, 'account_native_previous_target_buy_unsettled');
    assert.deepEqual(await call('GET', '/targets/no-financial-submit'), rejected);
    assert.equal((await call('POST', '/strategy/commands', start)).status, 'COMPLETED');
    assert.equal(native.requests.filter(row => row.operation === 'STRATEGY_START').length, 1);
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 0, 'No replay of the negative original is needed to unblock native strategy control.');
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('native strategy startup preserves the configured cash buffer and native equity without deriving weight targets', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-cash-buffer-')), native = new NativeProtocol();
  const account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001', cashBuffer: '100' }, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => fencedBody(account, method, path, body, request))).value as Record<string, unknown>;
  try {
    const observed = await call('GET', '/account'); assert.equal(observed.equity, '1000');
    const body = { requestId: 'ordinary-start', action: 'START', clientId: options.clientId, accountId: options.accountId };
    assert.equal((await call('POST', '/strategy/commands', body)).status, 'COMPLETED');
    const control = native.requests.filter(row => row.operation === 'CONTROL').at(-1)!;
    assert.equal(control.payload.cash_buffer, '100'); assert.equal(control.payload.reserved_amount, '0'); assert.equal(control.payload.spot_free_cash, '1000');
    const saved = JSON.parse(await readFile(join(directory, 'capital-receipts.json'), 'utf8'));
    assert.deepEqual(saved.strategyCommands[body.requestId].original, body);
    assert.deepEqual(saved.targets, {}); assert.equal(native.requests.some(row => row.operation === 'TARGET'), false);
    assert.equal(observed.observationId, native.snapshot.observation_id);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('retired planning and sizing cannot create a target, slice, or release funds', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-retired-planning-')), native = new NativeProtocol();
  const account = await createCapitalAccount(options, directory, '/unused', native);
  try {
    await assert.rejects(account.handle('/capital-exits/planning-context', request('GET'), async () => ({})), /capital_exit_not_found/);
    assert.equal((await account.handle('/capital-exits/preview', request('POST'), async () => ({ clientId: options.clientId, accountId: options.accountId }))).status, 404);
    assert.equal((await account.handle('/capital-exits', request('POST'), async () => ({ requestId: 'retired-start', clientId: options.clientId, accountId: options.accountId, command: {} }))).status, 404);
    assert.equal(native.requests.filter(row => ['TARGET', 'SLICE', 'FIXED_RISK_SIZE'].includes(row.operation)).length, 0);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('fixed risk sizing routes a read-only request without target submission', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-sizing-')), native = new NativeProtocol();
  const account = await createCapitalAccount(options, directory, '/unused', native, { async account() { return accountProof(); }, async ledger() { return []; } });
  try {
    const sizing: NativeFixedRiskSizingRequest['sizing'] = { instrument_id: 'BTC-USDC-SPOT.HYPERLIQUID', entry: '100', stop_loss: '95', equity: '1000', currency: 'USDC', risk: '0.01', commission_rate: '0.001', exchange_rate: '1', unit_batch_size: '0' };
    const before = native.requests.length;
    const result = await account.handle('/sizing/fixed-risk', request('POST'), async () => ({ clientId: options.clientId, accountId: options.accountId, sizing }));
    assert.equal(result.status, 200);
    assert.equal((result.value as Record<string, unknown>).clientId, options.clientId);
    assert.equal((result.value as Record<string, unknown>).accountId, options.accountId);
    assert.equal((result.value as Record<string, unknown>).mode, options.mode);
    assert.deepEqual(native.requests.slice(before).filter(row => row.operation !== 'CONTROL'), [{ operation: 'FIXED_RISK_SIZE', payload: sizing }]);
    assert.equal(native.requests.filter(row => row.operation === 'TARGET' || row.operation === 'SLICE').length, 0);
    const protectedBefore = native.requests.filter(row => ['TARGET', 'SHUTDOWN'].includes(row.operation)).length;
    for (const extra of [{ operation: 'TARGET' }, { operation: 'CONTROL' }, { operation: 'SHUTDOWN' }, { operation: 'FIXED_RISK_SIZE' }, { requestId: 'forged' }, { unknown: 'field' }]) {
      await assert.rejects(account.handle('/sizing/fixed-risk', request('POST'), async () => ({ clientId: options.clientId, accountId: options.accountId,
        sizing: { ...sizing, ...extra } })), /native_sizing_unknown_field/);
    }
    assert.equal(native.requests.filter(row => ['TARGET', 'SHUTDOWN'].includes(row.operation)).length, protectedBefore);
    await account.handle('/sizing/fixed-risk', request('POST'), async () => ({ clientId: options.clientId, accountId: options.accountId, sizing: { ...sizing, hard_limit: null } }));
    assert.deepEqual(native.requests.filter(row => row.operation === 'FIXED_RISK_SIZE').at(-1)!.payload, sizing);

  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('order preview exposes construction only and never sends a target or slice', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-order-preview-')), native = new NativeProtocol();
  const account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001' }, directory, '/unused', native);
  try {
    const capability = (await account.handle('/capabilities', request('GET'), async () => ({}))).value as { orderPreview: { previewOnly: boolean; automaticProtectionEnabled: boolean } };
    assert.equal(capability.orderPreview.previewOnly, true);
    assert.equal(capability.orderPreview.automaticProtectionEnabled, false);
    const order: NativeOrderPreview = { kind: 'BRACKET', side: 'BUY', quantity: '1.00', entry_price: '100.00', stop_loss: '90.00', take_profit: '110.00' };
    const response = await account.handle('/orders/preview', request('POST'), async () => ({ clientId: options.clientId, accountId: options.accountId,
      instrument_id: options.instruments[0], order }));
    assert.equal((response.value as Record<string, unknown>).accountId, options.accountId);
    assert.equal((response.value as Record<string, unknown>).mode, 'paper');
    const preview = native.requests.filter(item => item.operation === 'ORDER_PREVIEW');
    assert.equal(preview.length, 1);
    assert.deepEqual(preview[0]!.payload, { instrument_id: options.instruments[0], order });
    assert.equal(native.requests.some(item => ['TARGET', 'SLICE'].includes(item.operation)), false);
    await assert.rejects(account.handle('/orders/preview', request('POST'), async () => ({ clientId: 'other', accountId: options.accountId, order })), /account_identity_mismatch/);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('retired paper bracket targets never submit while cancellation remains available for original protections', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-brackets-')), native = new NativeProtocol();
  const account = await createCapitalAccount({ ...options, mode: 'paper', startingBalance: '1000', paperFeeRate: '0.001' }, directory, '/unused', native);
  const call = async (path: string, body: unknown) => (await account.handle(path, request('POST'), async () => body)).value as Record<string, unknown>;
  try {
    const brackets = { [options.instruments[0]!]: { enabled: true, stop_loss: '90.00', take_profit: '110.00' } };
    const body = { id: 'protected-signal', clientId: options.clientId, accountId: options.accountId, expiresAt: new Date(Date.now() + 60_000).toISOString(), nativeBrackets: brackets,
      target: { valid_until_ns: String(BigInt(Date.now() + 60_000) * 1_000_000n), targets: [{ instrument_id: options.instruments[0], weight: '0.2' }] } };
    await assert.rejects(call('/targets', body), /native_strategy_only/);
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 0);
    await assert.rejects(call('/targets', { ...body, nativeBrackets: {} }), /native_strategy_only/);
    await call('/targets/protected-signal/cancel-protection', { clientId: options.clientId, accountId: options.accountId });
    assert.deepEqual(native.requests.find(row => row.operation === 'CANCEL_PROTECTION')?.payload, { id: 'protected-signal' });
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});


test('live brackets are explicitly unsupported before native submission while preview remains available', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-live-bracket-reject-')), native = new NativeProtocol();
  const reader = { async account() { return accountProof(); }, async ledger() { return []; } };
  const account = await createCapitalAccount(options, directory, '/unused', native, reader);
  try {
    const capabilities = (await account.handle('/capabilities', request('GET'), async () => null)).value as any;
    assert.equal(capabilities.nativeBrackets.supported, false);
    assert.equal(capabilities.orderPreview.supported, true);
    await assert.rejects(account.handle('/targets', request('POST'), async () => ({ id: 'unsupported-live-bracket',
      clientId: options.clientId, accountId: options.accountId, nativeBrackets: { [options.instruments[0]!]: { enabled: true, stop_loss: '90', take_profit: '110' } },
      expiresAt: new Date(Date.now() + 60_000).toISOString(), target: { targets: [] } })), /native_strategy_only/);
    assert.equal(native.requests.filter(row => row.operation === 'TARGET').length, 0);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});

test('native strategy control persists uncertain original commands without replay and never creates legacy targets', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-strategy-control-'));
  class StrategyProtocol extends NativeProtocol {
    starts = 0; stops = 0;
    override async request(operation: string, payload: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
      if (operation === 'STRATEGY_START') { this.starts++; throw new Error('controlled_native_response_lost'); }
      if (operation === 'STRATEGY_STOP') { this.stops++; return { strategy_id: 'EMACross-001', state: 'STOPPED', native_version: 'test-only-upstream' }; }
      if (operation === 'STRATEGY_STATUS') return { strategy_id: 'EMACross-001', state: 'STOPPED', native_version: 'test-only-upstream', config: options.nativeStrategy.strategy };
      return super.request(operation, payload);
    }
  }
  const native = new StrategyProtocol();
  const paper = { ...options, mode: 'paper' as const, startingBalance: '1000', paperFeeRate: '0.001' };
  let account = await createCapitalAccount(paper, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => fencedBody(account, method, path, body, request))).value as Record<string, unknown>;
  const body = { requestId: 'start-once', action: 'START', clientId: options.clientId, accountId: options.accountId };
  try {
    assert.equal((await call('POST', '/strategy/commands', body)).status, 'UNCERTAIN'); assert.equal(native.starts, 1);
    await account.close(); account = await createCapitalAccount(paper, directory, '/unused', native);
    assert.equal((await call('POST', '/strategy/commands', body)).status, 'UNCERTAIN'); assert.equal(native.starts, 1);
    assert.equal((await call('GET', '/strategy/commands/start-once')).status, 'UNCERTAIN');
    await assert.rejects(call('POST', '/strategy/commands', { ...body, action: 'STOP' }), /strategy_request_id_conflict/);
    assert.equal((await call('POST', '/strategy/commands', { ...body, requestId: 'stop-once', action: 'STOP' })).status, 'COMPLETED');
    assert.equal(native.stops, 1); assert.equal(native.starts, 1);
    await assert.rejects(call('POST', '/targets', { clientId: options.clientId, accountId: options.accountId }), /native_strategy_only/);
    assert.equal(native.requests.some(row => row.operation === 'TARGET'), false);
    assert.deepEqual(native.snapshot.positions, []); assert.deepEqual(native.snapshot.orders, []);
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
});


test('live START without native spot-free capability is durably rejected before BOOT and replay never trades', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-live-unsupported-'));
  const native = Object.assign(new NativeProtocol(), { async start() { throw new Error('BOOT_MUST_NOT_RUN'); } });
  const reader = { async account() { return accountProof('1000'); }, async ledger() { return []; } };
  let account = await createCapitalAccount(options, directory, '/unused', native, reader);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
  try {
    const fence = await call('GET', '/strategy/fence');
    const body = { requestId: 'live-start-original', action: 'START', clientId: options.clientId, accountId: options.accountId,
      expectedAccountSourceId: fence.accountSourceId, expectedStrategyRevision: fence.strategyRevision };
    const receipt = await call('POST', '/strategy/commands', body);
    assert.equal(receipt.status, 'REJECTED');
    assert.deepEqual(receipt.result, { admission: 'REJECTED', started: false, reason: 'hyperliquid_native_spot_free_required' });
    assert.equal(native.requests.some(row => ['CONTROL', 'STRATEGY_START'].includes(row.operation)), false);
    await account.close();account = await createCapitalAccount(options, directory, '/unused', native, reader);
    assert.deepEqual(await call('POST', '/strategy/commands', body), receipt);
    assert.deepEqual(await call('GET', '/strategy/commands/live-start-original'), receipt);
    assert.equal(native.requests.some(row => ['CONTROL', 'STRATEGY_START'].includes(row.operation)), false);
    const saved = JSON.parse(await readFile(join(directory, 'capital-receipts.json'), 'utf8'));
    assert.deepEqual(saved.strategyCommands[body.requestId].original, body);
    assert.equal(saved.strategyCommands[body.requestId].receipt.status, 'REJECTED');
    // Retained unresolved live intent is not converted to a new rejection.
    await account.close();
    const old = { ...body, requestId: 'historical-live-unknown' };
    saved.strategyCommands[old.requestId] = { original: old, receipt: { ...receipt, requestId: old.requestId, status: 'UNCERTAIN', result: null, error: 'original_response_lost' } };
    await writeFile(join(directory, 'capital-receipts.json'), JSON.stringify(saved));
    account = await createCapitalAccount(options, directory, '/unused', native, reader);
    assert.deepEqual(await call('POST', '/strategy/commands', old), saved.strategyCommands[old.requestId].receipt);
    assert.equal(native.requests.some(row => ['CONTROL', 'STRATEGY_START'].includes(row.operation)), false);
  } finally { await account.close();await rm(directory, { recursive: true, force: true }); }
});


test('unrecorded live rejection remains uncertain and never launches a node', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-live-rejection-write-failed-'));
  let starts = 0;
  const native = Object.assign(new NativeProtocol(), { async start() { starts++; } });
  const account = await createCapitalAccount(options, directory, '/unused', native,
    { async account() { return accountProof(); }, async ledger() { return []; } });
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Record<string, unknown>;
  try {
    const fence = await call('GET', '/strategy/fence');
    const body = { requestId: 'unrecorded-original', action: 'START', clientId: options.clientId, accountId: options.accountId,
      expectedAccountSourceId: fence.accountSourceId, expectedStrategyRevision: fence.strategyRevision };
    await mkdir(join(directory, 'capital-receipts.json.next'));
    const first = await call('POST', '/strategy/commands', body);
    assert.equal(first.status, 'UNCERTAIN'); assert.equal(first.persisted, false);
    assert.deepEqual(await call('POST', '/strategy/commands', body), first);
    assert.deepEqual(await call('GET', '/strategy/commands/unrecorded-original'), first);
    assert.equal(starts, 0); assert.equal(native.requests.some(row => ['CONTROL', 'STRATEGY_START'].includes(row.operation)), false);
  } finally { await account.close().catch(()=>{}); await rm(directory, { recursive: true, force: true }); }
});
