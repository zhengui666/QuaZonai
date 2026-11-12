import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import type { IncomingMessage } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import type { AccountOptions, NativeAccountRuntime, NativeAccountSnapshot, NativeFill, NativeOrder } from '../src/account-runtime.ts';
import { createCapitalAccount } from '../src/capital-account.ts';
import { parseHyperliquidAccount, units } from '../src/hyperliquid-account.ts';
import type { HyperliquidLedgerRow } from '../src/hyperliquid-account.ts';
import { validateCapitalDocument } from '../../../packages/contracts/src/capital-exit.ts';
import type { CapitalExitNativeAction, CapitalExitPlanningContext, CapitalExitPolicy, CapitalExitPreview, CapitalExitView } from '../../../packages/contracts/src/capital-exit.ts';

const instrumentId = 'HYPE-USDC-SPOT.HYPERLIQUID';
const paperOptions: AccountOptions = { id: 'spot', clientId: 'client', accountId: 'HYPERLIQUID-001', mode: 'paper',
  startingBalance: '1000', paperFeeRate: '0.001', nativeStrategy: { instrument_id: instrumentId, strategy: { kind: 'EMA_CROSS', trade_size: '1', fast_period: 10, slow_period: 50 } }, instruments: [instrumentId] };
type Json = Record<string, unknown>;

// These controlled NDJSON responses exercise receiver recovery and contracts.
// They do not validate native execution, exchange fills, or LIVE balances.
class ControlledProtocol implements NativeAccountRuntime {
  snapshot: NativeAccountSnapshot;
  requests: { operation: string; payload: Json }[] = [];
  listener: (() => void) | null = null;
  constructor(cash = '0') {
    const ns = String(BigInt(Date.now()) * 1_000_000n);
    this.snapshot = { ready: true, recovery_required: false, sequence: '1', native_version: '0.68.0', observed_at_ns: ns,
      observation_id: randomUUID(), native_session_id: 'controlled-original-session', native_trader_id: 'QZ-001', native_account_id: paperOptions.accountId,
      native_snapshot: { account_id: paperOptions.accountId, account_type: 'MARGIN', base_currency: null, balances: [], margins: [], unrealized_pnls: [], realized_pnls: [], total_equity: [],
        base_currency_equity: null, is_stale: false, stale_instruments: [], stale_currencies: [], unpriced_instruments: [], event_id: 'controlled-original-event', ts_event: ns, ts_init: ns },
      positions: [{ instrument_id: instrumentId, quantity: '10', free_quantity: '10', value: '1000', pnl: null }],
      instruments: [{ instrument_id: instrumentId, base_currency: 'HYPE', quote_currency: 'USDC', size_increment: '0.01', price_increment: '0.01',
        minimum_quantity: '0.01', minimum_notional: '10', taker_fee_rate: '0.001', bid: '100', ask: '100.1', bid_depth: [{ price: '100', quantity: '100' }], asof_ns: ns }],
      orders: [], fills: [], cash, equity: '1000', currency: 'USDC', currency_precision: 6 };
  }
  changed() {
    const ns = String(BigInt(Date.now()) * 1_000_000n);
    this.snapshot.sequence = String(BigInt(this.snapshot.sequence) + 1n);
    this.snapshot.observation_id = randomUUID(); this.snapshot.observed_at_ns = ns;
    Object.assign(this.snapshot.native_snapshot!, { event_id: `controlled-event-${this.snapshot.sequence}`, ts_event: ns, ts_init: ns });
    for (const instrument of this.snapshot.instruments) instrument.asof_ns = ns;
  }
  async request(operation: string, payload: Json = {}): Promise<Json> {
    this.requests.push({ operation, payload: structuredClone(payload) });
    if (operation === 'SNAPSHOT') return structuredClone(this.snapshot) as unknown as Json;
    if (operation === 'QUANTIZE_MONEY') return { amount: payload.amount, precision: this.snapshot.currency_precision };
    if (operation === 'CONTROL') return { applied: true, epoch: payload.epoch, revision: payload.revision };
    if (operation === 'CANCEL_EXIT') return { applied: true, epoch: payload.epoch };
    if (operation === 'SLICE') {
      this.snapshot.orders.push({ client_order_id: String(payload.client_order_id), venue_order_id: null, instrument_id: String(payload.instrument_id), side: 'SELL',
        quantity: String(payload.quantity), filled_quantity: '0', leaves_quantity: String(payload.quantity), status: 'ACCEPTED', limit_price: String(payload.minimum_price), reduce_only: false, exit_id: String(payload.exit_id) });
      return { admission: 'SUBMITTED', submitted: true, client_order_id: payload.client_order_id, epoch: payload.epoch };
    }
    if (operation === 'STRATEGY_START' || operation === 'STRATEGY_STOP') return { strategy_id: 'EMACross-001', state: operation === 'STRATEGY_START' ? 'RUNNING' : 'STOPPED', native_version: '0.69.0' };
    if (operation === 'TARGET') return { acknowledged: true, target_id: payload.id, epoch: payload.epoch };
    throw new Error(`unexpected_controlled_operation:${operation}`);
  }
  subscribeTicks(listener: () => void) { this.listener = listener; return () => { this.listener = null; }; }
  tick() { assert.ok(this.listener, 'The receiver must subscribe to native owner clock notifications.'); this.listener(); }
  slices() { return this.requests.filter(value => value.operation === 'SLICE'); }
  async close() {}
}

const request = (method: string) => ({ method, headers: { 'x-qz-client-id': paperOptions.clientId } }) as unknown as IncomingMessage;
const delay = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function eventually<T>(read: () => Promise<T>, predicate: (value: T) => boolean, message: string, timeout = 2_000): Promise<T> {
  const until = Date.now() + timeout;
  let value = await read();
  while (!predicate(value) && Date.now() < until) { await delay(10); value = await read(); }
  assert.ok(predicate(value), `${message}: ${JSON.stringify(value)}`); return value;
}
async function openAccount(native: ControlledProtocol, options = paperOptions, reader?: Parameters<typeof createCapitalAccount>[4]) {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-recovery-'));
  const account = await createCapitalAccount(options, directory, '/unused-controlled-protocol', native, reader);
  let serial = 0;
  const call = async (method: string, path: string, body: unknown = {}) => {
    const value = (await account.handle(path, request(method), async () => body)).value as Json;
    if (path.startsWith('/capital-exits') && value.result != null) validateCapitalDocument('VIEW', value.result);
    if (value.exit != null) validateCapitalDocument('VIEW', value.exit);
    for (const exit of value.exits as CapitalExitView[] ?? []) validateCapitalDocument('VIEW', exit);
    return value;
  };
  const envelope = (value: unknown, preview = false) => ({ requestId: `controlled-${++serial}`, clientId: options.clientId, accountId: options.accountId,
    ...(preview ? { request: value } : { command: value }) });
  const status = async () => (await call('GET', '/capital-exits')).exits as CapitalExitView[];
  const exit = async (id: string) => { const value = (await status()).find(value => value.id === id); assert.ok(value); return value; };
  const preview = async (amount: string, policy: (context: CapitalExitPlanningContext) => CapitalExitPolicy, resumed?: CapitalExitView) => {
    const context = (await call('GET', '/capital-exits/planning-context')).context as CapitalExitPlanningContext;
    const value = (await call('POST', '/capital-exits/preview', envelope({ schema_version: 2, account_source_id: context.account_source_id,
      expected_source_observation_id: context.observation_id, expected_account_control_revision: context.account_control_revision, planning_context_id: context.id,
      scope: { kind: 'AMOUNT', amount, currency: 'USDC' }, policy: policy(context),
      ...(resumed ? { resume_exit_id: resumed.id, expected_exit_revision: resumed.revision } : {}) }, true))).preview as CapitalExitPreview;
    assert.equal(value.capability, 'SUPPORTED', value.reason_codes.join(',')); return { context, value };
  };
  const start = async (amount: string, policy: (context: CapitalExitPlanningContext) => CapitalExitPolicy) => {
    const captured = await preview(amount, policy);
    const receipt = await call('POST', '/capital-exits', envelope({ schema_version: 2, preview_id: captured.value.id,
      expected_account_control_revision: captured.context.account_control_revision, acknowledged_plan_artifact_id: captured.value.plan_artifact_id,
      expected_source_observation_id: captured.context.observation_id }));
    assert.equal(receipt.status, 'ACCEPTED', String(receipt.error)); return receipt.result as CapitalExitView;
  };
  const action = async (view: CapitalExitView, command: Omit<CapitalExitNativeAction, 'schema_version' | 'expected_revision'>) =>
    call('POST', `/capital-exits/${view.id}/actions`, envelope({ schema_version: 2, expected_revision: view.revision, ...command }));
  const pause = async (view: CapitalExitView) => {
    const receipt = await action(view, { action: 'PAUSE' }); assert.equal(receipt.status, 'ACCEPTED', String(receipt.error)); native.tick();
    return eventually(() => exit(view.id), value => value.state === 'PAUSED', 'Original IOC must settle before PAUSED');
  };
  const saved = async () => JSON.parse(await readFile(join(directory, 'capital-receipts.json'), 'utf8')) as {
    exits: Record<string, { executionCost: string; fillCosts: Record<string, { cost: string }> }>;
    targets: Record<string, { receipt: Json; original: Json }>;
  };
  return { call, status, exit, preview, start, action, pause, saved, async close() { await account.close(); await rm(directory, { recursive: true, force: true }); } };
}
function bounded(context: CapitalExitPlanningContext, quantity = '8', floor = '99'): CapitalExitPolicy {
  return { kind: 'BOUNDED_LIMIT', deadline: new Date(Date.now() + 60_000).toISOString(), legs: [{ instrument_id: instrumentId, maximum_reduction_quantity: quantity, minimum_sell_price: floor }],
    max_execution_cost: { amount: '20', currency: 'USDC', reference_evidence_id: context.id },
    pacing: { minimum_interval_seconds: 1, maximum_slice_notional: '100', maximum_participation: '0.1' } };
}
function fill(order: NativeOrder, quantity: string, commission: string | null, tradeId: string): NativeFill {
  return { trade_id: tradeId, client_order_id: order.client_order_id, instrument_id: order.instrument_id, side: 'SELL', quantity, price: '99.5',
    commission, commission_currency: commission === null ? null : 'USDC', ts_event_ns: String(BigInt(Date.now()) * 1_000_000n) };
}
async function firstSlice(native: ControlledProtocol) {
  native.tick(); await eventually(async () => native.slices(), value => value.length === 1, 'Native owner tick must propose one IOC');
  return native.snapshot.orders[0]!;
}

test('partial fills keep their original cost basis across pause and a revision-bound resume', { timeout: 10_000 }, async () => {
  const native = new ControlledProtocol(), account = await openAccount(native);
  try {
    const created = await account.start('600', context => bounded(context)), order = await firstSlice(native);
    native.snapshot.fills.push(fill(order, '0.5', '0.04975', 'original-partial-fill'));
    Object.assign(order, { filled_quantity: '0.5', leaves_quantity: '0.5', status: 'CANCELED' });
    Object.assign(native.snapshot, { cash: '49.70025', equity: '999.70025' });
    Object.assign(native.snapshot.positions[0]!, { quantity: '9.5', free_quantity: '9.5' }); native.changed(); native.tick();
    const recorded = await eventually(() => account.exit(created.id), value => value.progress[0]?.filled_quantity === '0.5', 'Partial fill contribution must be recorded');
    assert.equal(recorded.progress[0]!.execution_cost!.amount, '0.29975');
    const paused = await account.pause(recorded);
    Object.assign(native.snapshot.instruments[0]!, { bid: '110', ask: '110.1', bid_depth: [{ price: '110', quantity: '100' }] });
    native.snapshot.equity = '1094.70025'; native.changed();
    const captured = await account.preview(paused.funds.reserved_amount.amount, context => bounded(context, '6', '109'), paused);
    assert.equal(captured.value.resume_exit_id, paused.id); assert.equal(captured.value.expected_exit_revision, paused.revision);
    assert.equal(captured.value.scope.amount, '600', 'Own existing R is reused rather than deducted twice from managed equity.');
    assert.notEqual(captured.context.id, recorded.policy.kind === 'BOUNDED_LIMIT' ? recorded.policy.max_execution_cost.reference_evidence_id : null);
    const receipt = await account.action(paused, { action: 'RESUME', preview_id: captured.value.id }); assert.equal(receipt.status, 'ACCEPTED', String(receipt.error));
    const resumed = receipt.result as CapitalExitView;
    assert.equal(resumed.scope.amount, '600'); assert.equal(resumed.funds.reserved_amount.amount, '600');
    assert.equal(resumed.progress[0]!.filled_quantity, '0.5'); assert.equal(resumed.progress[0]!.authorized_quantity, '6.5'); assert.equal(resumed.progress[0]!.remaining_quantity, '6');
    assert.equal(resumed.progress[0]!.execution_cost!.amount, '0.29975', 'New bid 110 must not reprice the original bid-100 fill.');
    native.tick();
    const refreshed = await eventually(() => account.exit(created.id), value => value.state === 'REDUCING', 'Resume must retain executable remaining authorization');
    assert.equal(refreshed.progress[0]!.execution_cost!.amount, '0.29975'); assert.equal(refreshed.progress[0]!.remaining_quantity, '6');
  } finally { await account.close(); }
});

for (const incomplete of ['missing_original_fill', 'unknown_commission'] as const) {
  test(`unresolved ${incomplete} cannot fund readiness, release reserves, or authorize another IOC`, { timeout: 10_000 }, async () => {
    const native = new ControlledProtocol(), account = await openAccount(native);
    try {
      const created = await account.start('600', context => bounded(context)), order = await firstSlice(native);
      native.snapshot.fills.push(fill(order, '0.25', '0.024875', 'known-original-fill'));
      Object.assign(order, { filled_quantity: '0.25', leaves_quantity: '0.75', status: 'PARTIALLY_FILLED' }); native.changed(); native.tick();
      const known = await eventually(() => account.exit(created.id), value => value.progress[0]?.filled_quantity === '0.25', 'Known contribution must be persisted before the recovery gap');
      const knownCost = known.progress[0]!.execution_cost!.amount; assert.equal(knownCost, '0.149875');
      if (incomplete === 'unknown_commission') native.snapshot.fills.push(fill(order, '0.25', null, 'fee-unavailable-fill'));
      Object.assign(order, { filled_quantity: '0.5', leaves_quantity: '0.5', status: 'CANCELED' });
      native.snapshot.cash = '600'; native.changed(); native.tick();
      const blocked = await eventually(() => account.exit(created.id), value => value.state === 'BLOCKED', 'Native cumulative fills and fee contributions must reconcile before financial progress');
      assert.equal(blocked.preparation_state, 'BLOCKED'); assert.notEqual(blocked.funds.withdrawability, 'VERIFIED');
      assert.equal(blocked.progress[0]!.filled_quantity, '0.5', 'Native cumulative execution quantity remains authoritative when a fill or its fee is unavailable.');
      assert.equal(blocked.progress[0]!.remaining_quantity, '7.5');
      assert.equal(blocked.progress[0]!.execution_cost, null, 'Unknown total cost must not be presented as a complete known amount.');
      assert.equal(blocked.progress[0]!.actual_fees, null); assert.equal(blocked.funds.actual_fees, null);
      assert.equal((await account.saved()).exits[created.id]!.executionCost, knownCost, 'Missing evidence cannot discard already known losses.');
      assert.equal(blocked.action_availability.RELEASE_RESERVATION?.allowed, false);
      const released = await account.action(blocked, { action: 'RELEASE_RESERVATION' }); assert.equal(released.status, 'REJECTED');
      assert.equal((await account.exit(created.id)).funds.reserved_amount.amount, '600');
      native.snapshot.cash = '49.70025'; native.changed(); await delay(1_020); native.tick(); await delay(30);
      const after = await eventually(() => account.exit(created.id), value => value.state === 'BLOCKED', 'Recovered account must retain the evidence stop');
      assert.equal(after.progress[0]!.execution_cost, null); assert.equal((await account.saved()).exits[created.id]!.executionCost, knownCost);
      assert.equal(native.slices().length, 1, 'The terminal native order does not make unknown fills safe to resubmit.');
    } finally { await account.close(); }
  });
}

test('paused staged plans share one withdrawal balance and refresh preparation evidence independently', { timeout: 10_000 }, async () => {
  const native = new ControlledProtocol('1000'); native.snapshot.positions = []; native.snapshot.instruments = [];
  const options: AccountOptions = { ...paperOptions, mode: 'live', accountAddress: '0x' + '1'.repeat(40) };
  const ledger: HyperliquidLedgerRow[] = []; let spot = '1000', withdrawable = '0', reads = 0;
  const reader = { async account() { reads++; return parseHyperliquidAccount(options.accountAddress!, { balances: [{ coin: 'USDC', total: spot, hold: '0' }] },
    { assetPositions: [], crossMarginSummary: { totalMarginUsed: '0' }, withdrawable }, 'default', { userSpotCrossRate: '0.001' }); }, async ledger() { return ledger; } };
  const account = await openAccount(native, options, reader);
  try {
    const first = await account.pause(await account.start('100', () => ({ kind: 'CASH_ONLY' })));
    ledger.push({ time: Date.now(), hash: 'first-transfer', delta: { type: 'accountClassTransfer', toPerp: true, usdc: '100' } }); spot = '900'; withdrawable = '100';
    const stagedFirst = await account.action(first, { action: 'RECONCILE_EXTERNAL_TRANSFER', user_reported_amount: '100', currency: 'USDC', external_transfer_ref: 'first-transfer' });
    assert.equal(stagedFirst.status, 'ACCEPTED', String(stagedFirst.error));
    const second = await account.pause(await account.start('100', () => ({ kind: 'CASH_ONLY' })));
    ledger.push({ time: Date.now(), hash: 'second-transfer', delta: { type: 'accountClassTransfer', toPerp: true, usdc: '100' } }); spot = '800';
    const stagedSecond = await account.action(second, { action: 'RECONCILE_EXTERNAL_TRANSFER', user_reported_amount: '100', currency: 'USDC', external_transfer_ref: 'second-transfer' });
    assert.equal(stagedSecond.status, 'ACCEPTED', String(stagedSecond.error)); native.tick();
    const allocated = await eventually(account.status, values => values.every(value => value.funds.verified_withdrawable_amount !== null), 'Staged withdrawal evidence must refresh while both executions are paused');
    assert.equal(allocated[0]!.state, 'PAUSED'); assert.equal(allocated[1]!.state, 'PAUSED');
    assert.equal(allocated[0]!.funds.verified_withdrawable_amount!.amount, '100'); assert.equal(allocated[1]!.funds.verified_withdrawable_amount!.amount, '0');
    assert.equal(allocated.filter(value => value.preparation_state === 'READY').length, 1, 'One account-wide 100 balance cannot satisfy both 100 obligations.');
    assert.equal(allocated.reduce((sum, value) => sum + units(value.funds.verified_withdrawable_amount!.amount), 0n), units('100'));
    const previousEvidence = allocated[1]!.funds.evidence_asof, previousReads = reads; withdrawable = '200';
    const refreshed = await eventually(account.status, values => reads > previousReads && values.every(value => value.preparation_state === 'READY'), 'New read-only withdrawal facts must refresh paused plans', 4_500);
    for (const exit of refreshed) { assert.equal(exit.state, 'PAUSED'); assert.equal(exit.funds.verified_withdrawable_amount!.amount, '100'); }
    assert.notEqual(refreshed[1]!.funds.evidence_asof, previousEvidence); assert.equal(native.slices().length, 0);
  } finally { await account.close(); }
});

test('a delayed reverse-transfer refresh publishes new spot cash only behind its new paused epoch', { timeout: 10_000 }, async () => {
  const native = new ControlledProtocol('1000'); native.snapshot.positions = []; native.snapshot.instruments = [];
  const options: AccountOptions = { ...paperOptions, mode: 'live', accountAddress: '0x' + '1'.repeat(40) };
  const ledger: HyperliquidLedgerRow[] = []; let spot = '1000', withdrawable = '0', defer = false, waiting = false;
  let releaseRead: (() => void) | null = null;
  const reader = { async account() {
    const started = new Date().toISOString();
    if (defer) { waiting = true; await new Promise<void>(resolve => { releaseRead = resolve; }); }
    return parseHyperliquidAccount(options.accountAddress!, { balances: [{ coin: 'USDC', total: spot, hold: '0' }] },
      { assetPositions: [], crossMarginSummary: { totalMarginUsed: '0' }, withdrawable }, 'default', { userSpotCrossRate: '0.001' }, started);
  }, async ledger() { return ledger; } };
  const account = await openAccount(native, options, reader);
  try {
    const created = await account.start('100', () => ({ kind: 'CASH_ONLY' }));
    ledger.push({ time: Date.now(), hash: 'outward-source', delta: { type: 'accountClassTransfer', toPerp: true, usdc: '100' } }); spot = '900'; withdrawable = '100';
    const staged = await account.action(created, { action: 'RECONCILE_EXTERNAL_TRANSFER', user_reported_amount: '100', currency: 'USDC', external_transfer_ref: 'outward-source' });
    assert.equal(staged.status, 'ACCEPTED', String(staged.error)); native.tick();
    const prepared = await eventually(() => account.exit(created.id), value => value.preparation_state === 'READY', 'Original external staging must be observed');
    const phase = native.requests.length;
    defer = true; spot = '1000'; withdrawable = '0';
    ledger.push({ time: Date.now(), hash: 'reverse-source', delta: { type: 'accountClassTransfer', toPerp: false, usdc: '100' } });
    await delay(3_050);
    const reading = account.call('GET', '/account');
    await eventually(async () => waiting, value => value, 'Controlled Info read must be in flight');
    // A stop command must use the previously published facts and finish while
    // the read-only HTTP request is still waiting for its response.
    const stopped = await Promise.race([account.action(await account.exit(created.id), { action: 'PAUSE' }),
      delay(700).then(() => { throw new Error('PAUSE blocked behind a read-only Info request'); })]);
    assert.equal(stopped.status, 'ACCEPTED', String(stopped.error)); assert.equal(waiting, true);
    const duringRead = native.requests.slice(phase).filter(value => value.operation === 'CONTROL');
    assert.ok(duringRead.length > 0); assert.ok(duringRead.every(value => value.payload.spot_free_cash === '900'));
    const pausedEpoch = String(duringRead.at(-1)!.payload.epoch);
    defer = false; assert.ok(releaseRead); (releaseRead as () => void)();
    const observed = await reading;
    const published = await eventually(async () => account.call('GET', '/capital-exits'), value => value.blocked === true,
      'Reverse-transfer evidence must block further financial targets before exposing increased spendable cash');
    assert.equal(published.reason, 'capital_external_reverse_transfer_requires_reconciliation');
    const exit = (published.exits as CapitalExitView[])[0]!;
    assert.equal(exit.funds.reserved_amount.amount, '0'); assert.equal(exit.funds.staged_for_withdrawal_amount.amount, '100');
    assert.equal(observed.cash, '1000');
    const newCashControls = native.requests.slice(phase).filter(value => value.operation === 'CONTROL' && value.payload.spot_free_cash === '1000');
    assert.ok(newCashControls.length > 0);
    for (const control of newCashControls) { assert.equal(control.payload.mode, 'PAUSED'); assert.ok(BigInt(String(control.payload.epoch)) > BigInt(pausedEpoch)); }
    await assert.rejects(account.call('POST', '/strategy/commands', { requestId: 'start-after-unreconciled-reverse', action: 'START', clientId: options.clientId, accountId: options.accountId }), /capital_external_reverse_transfer_requires_reconciliation/);
    assert.equal(native.requests.filter(value => value.operation === 'STRATEGY_START').length, 0);
    assert.equal(native.requests.filter(value => value.operation === 'TARGET').length, 0);
    const reconciled = await account.action(await account.exit(prepared.id), { action: 'RECONCILE_EXTERNAL_TRANSFER', user_reported_amount: '100', currency: 'USDC', external_transfer_ref: 'reverse-source' });
    assert.equal(reconciled.status, 'ACCEPTED', String(reconciled.error));
    const result = reconciled.result as CapitalExitView;
    assert.equal(result.funds.reserved_amount.amount, '100'); assert.equal(result.funds.staged_for_withdrawal_amount.amount, '0');
    assert.equal((await account.call('GET', '/capital-exits')).blocked, false);
  } finally { if (releaseRead) (releaseRead as () => void)(); await account.close(); }
});

test('reserved capital prevents native strategy restart and pause stops reduction slices without releasing the reservation', { timeout: 10_000 }, async () => {
  const native = new ControlledProtocol('200'); Object.assign(native.snapshot.positions[0]!, { quantity: '8', free_quantity: '8', value: '800' });
  const account = await openAccount(native);
  const command = (requestId: string, action: 'START' | 'STOP') => ({ requestId, action, clientId: paperOptions.clientId, accountId: paperOptions.accountId });
  try {
    assert.equal((await account.call('POST', '/strategy/commands', command('initial-native-start', 'START'))).status, 'COMPLETED');
    assert.equal(native.requests.filter(value => value.operation === 'STRATEGY_START').length, 1);
    const created = await account.start('600', context => bounded(context, '6'));
    assert.equal(created.funds.requested_amount.amount, '600'); assert.equal(created.funds.reserved_amount.amount, '600');
    const observed = await account.call('GET', '/account'); assert.equal(observed.equity, '1000'); assert.equal(observed.cash, '200');
    await assert.rejects(account.call('POST', '/strategy/commands', command('reserved-native-start', 'START')), /strategy_account_exit_unsettled/);
    assert.equal(native.requests.filter(value => value.operation === 'STRATEGY_START').length, 1, 'An unsettled exit cannot be bypassed by starting an upstream strategy.');
    const paused = await account.pause(await account.exit(created.id));
    const control = native.requests.filter(value => value.operation === 'CONTROL').at(-1)!;
    assert.equal(control.payload.mode, 'CASH_ONLY', 'Pausing discretionary reduction keeps the original spot cash reservation.');
    assert.equal(control.payload.reserved_amount, '600'); assert.equal(control.payload.spot_free_cash, '200');
    await assert.rejects(account.call('POST', '/strategy/commands', command('paused-native-start', 'START')), /strategy_account_exit_unsettled/);
    assert.equal((await account.call('POST', '/strategy/commands', command('owner-native-stop', 'STOP'))).status, 'COMPLETED');
    native.tick(); await delay(30);
    const after = await account.exit(paused.id);
    assert.equal(after.state, 'PAUSED'); assert.equal(after.funds.reserved_amount.amount, '600');
    assert.equal(after.funds.staged_for_withdrawal_amount.amount, '0'); assert.equal(after.funds.reconciled_withdrawal_amount.amount, '0');
    assert.equal(after.remaining_trading, 'FENCED_PENDING_TARGET', 'No retired target ACK may be fabricated to claim budget-aware strategy execution.');
    assert.equal(native.slices().length, 0, 'A paused exit never turns an owner tick into another forced reduction.');
    assert.equal(native.requests.some(value => value.operation === 'TARGET'), false);
    assert.deepEqual((await account.saved()).targets, {});
  } finally { await account.close(); }
});

test('strategy admission rejection remains distinct from lost transport and is never replayed', async () => {
  const native = new ControlledProtocol();
  const original = native.request.bind(native);
  let calls = 0, loseReply = false;
  native.request = async (operation, payload = {}) => {
    if (operation === 'STRATEGY_START') {
      calls++;
      if (loseReply) throw new Error('account_runtime_outcome_uncertain');
      return { admission: 'REJECTED', started: false, reason: 'account_native_strategy_capital_reserved' };
    }
    return original(operation, payload);
  };
  const account = await openAccount(native);
  try {
    const body = (requestId: string) => ({ requestId, action: 'START', clientId: paperOptions.clientId, accountId: paperOptions.accountId });
    const rejected = await account.call('POST', '/strategy/commands', body('rejected'));
    assert.equal(rejected.status, 'REJECTED'); assert.equal(rejected.error, 'account_native_strategy_capital_reserved');
    assert.deepEqual(await account.call('POST', '/strategy/commands', body('rejected')), rejected); assert.equal(calls, 1);
    loseReply = true;
    const uncertain = await account.call('POST', '/strategy/commands', body('lost'));
    assert.equal(uncertain.status, 'UNCERTAIN');
    assert.deepEqual(await account.call('GET', '/strategy/commands/lost'), uncertain); assert.equal(calls, 2);
  } finally { await account.close(); }
});

test('native account capabilities do not advertise replay-only order-book strategy', async () => {
  const native = new ControlledProtocol();
  const account = await openAccount(native, { ...paperOptions, nativeStrategy: { instrument_id: instrumentId, strategy: {
    kind: 'ORDER_BOOK_IMBALANCE', max_trade_size: '1', trigger_min_size: '1', trigger_imbalance_ratio: '0.2',
    min_seconds_between_triggers: 1, time_in_force: 'IOC', dry_run: false,
  } } });
  try {
    const capabilities = await account.call('GET', '/capabilities');
    assert.equal((capabilities.nativeStrategy as Json).supported, false);
    assert.deepEqual((capabilities.nativeStrategy as Json).supportedKinds, ['EMA_CROSS']);
    assert.equal((capabilities.nativeStrategy as Json).reason, 'account_native_strategy_account_lifecycle_unsupported_for_orderbook_imbalance');
  } finally { await account.close(); }
});

test('explicit STOP crosses receipt disk failure without claiming a durable result or retrying', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-stop-disk-failure-'));
  const native = new ControlledProtocol();
  const account = await createCapitalAccount(paperOptions, directory, '/unused-controlled-protocol', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Json;
  const body = (requestId: string, action: string) => ({ requestId, action, clientId: paperOptions.clientId, accountId: paperOptions.accountId });
  try {
    assert.equal((await call('POST', '/strategy/commands', body('before-failure', 'START'))).status, 'COMPLETED');
    // Real atomic-write failure on an isolated test directory, not a mocked save.
    await (await import('node:fs/promises')).mkdir(join(directory, 'capital-receipts.json.next'));
    const receipt = await call('POST', '/strategy/commands', body('emergency-stop', 'STOP'));
    assert.equal(receipt.status, 'UNCERTAIN'); assert.equal(receipt.persisted, false);
    assert.equal(receipt.persistenceError, 'account_receipts_write_failed');
    assert.equal((receipt.result as Json).state, 'STOPPED', 'Preserve the actual runtime reply separately from durable receipt status.');
    const disk = JSON.parse(await readFile(join(directory, 'capital-receipts.json'), 'utf8')) as Json;
    assert.equal((disk.strategyCommands as Json)['emergency-stop'], undefined);
    assert.deepEqual(await call('POST', '/strategy/commands', body('emergency-stop', 'STOP')), receipt);
    assert.deepEqual(await call('GET', '/strategy/commands/emergency-stop'), receipt);
    assert.equal(native.requests.filter(row => row.operation === 'STRATEGY_STOP').length, 1);
    await assert.rejects(call('POST', '/strategy/commands', body('unsafe-restart', 'START')), /account_receipts_write_failed/);
    assert.equal(native.requests.filter(row => row.operation === 'STRATEGY_START').length, 1);
    const original = native.request.bind(native);
    native.request = async (operation, payload) => {
      const response = await original(operation, payload);
      if (operation === 'STRATEGY_STOP') throw new Error('account_runtime_outcome_uncertain');
      return response;
    };
    const lost = await call('POST', '/strategy/commands', body('emergency-stop-lost', 'STOP'));
    assert.equal(lost.status, 'UNCERTAIN'); assert.equal(lost.persisted, false);
    assert.equal(lost.result, null); assert.equal(lost.error, 'account_runtime_outcome_uncertain');
    assert.deepEqual(await call('POST', '/strategy/commands', body('emergency-stop-lost', 'STOP')), lost);
    assert.equal(native.requests.filter(row => row.operation === 'STRATEGY_STOP').length, 2);
  } finally {
    await account.close().catch(() => undefined);
    await rm(directory, { recursive: true, force: true });
  }
});
