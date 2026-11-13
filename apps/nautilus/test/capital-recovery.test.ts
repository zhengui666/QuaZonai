import { once } from 'node:events';
import { fencedBody } from './strategy-fence.ts';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
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
      positions: [{ instrument_id: instrumentId, quantity: '10', value: '1000', pnl: null }],
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
  const call = async (method: string, path: string, body: unknown = {}) => {
    const value = (await account.handle(path, request(method), async () => fencedBody(account, method, path, body, request))).value as Json;
    if (path.startsWith('/capital-exits') && value.result != null) validateCapitalDocument('VIEW', value.result);
    if (value.exit != null) validateCapitalDocument('VIEW', value.exit);
    for (const exit of value.exits as CapitalExitView[] ?? []) validateCapitalDocument('VIEW', exit);
    return value;
  };
  return { call, async close() { await account.close(); await rm(directory, { recursive: true, force: true }); } };
}

// Original persisted account facts only: no retired preview, START or slicing producer.
for (const evidence of ['known_partial_fill', 'missing_original_fill', 'unknown_commission']) test(`retained ${evidence} survives cancellation and reopen without recalculating funds`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-retained-cost-'));
  const native = new ControlledProtocol(); let settled = false;
  const original = native.request.bind(native);
  native.request = async (op, payload = {}) => {
    if (op === 'CANCEL_EXIT') { native.requests.push({ operation: op, payload }); return { cancellation_requested: settled ? 0 : 1, settled }; }
    return original(op, payload);
  };
  const funds = { requested_amount: { amount: '600', currency: 'USDC' }, reserved_amount: { amount: '600', currency: 'USDC' }, staged_for_withdrawal_amount: { amount: '17', currency: 'USDC' }, reconciled_withdrawal_amount: { amount: '3', currency: 'USDC' } };
  const progress = [{ instrument_id: instrumentId, filled_quantity: '0.5', execution_cost: evidence === 'known_partial_fill' ? { amount: '0.29975', currency: 'USDC' } : null }];
  const retained = { schema_version: 2, sourceId: 'original-source', identity: { mode: 'paper', network: 'mainnet', accountId: paperOptions.accountId, accountAddress: null, currency: 'USDC' }, epoch: '3', revision: '3', strategyRevision: '1', previews: {}, targets: {}, ledgerClaims: { original: 'proof-original' }, reverseTransferPending: false,
    commands: { unknown: { original: { requestId: 'unknown', command: { preview_id: 'old-preview' } }, state: 'UNCERTAIN', error: 'original_unknown', exitId: 'old-exit' } },
    exits: { 'old-exit': { view: { id: 'old-exit', revision: '1', state: 'BLOCKED', policy: { kind: 'BOUNDED_LIMIT', pacing: { minimum_interval_seconds: 1 }, deadline: '2030-01-01T00:00:00Z' }, funds, progress, events: [], reason_codes: [evidence] }, childIds: ['original-order'], pendingSlice: { id: 'original-order' }, executionCost: '0.29975', fillCosts: { original: { cost: '0.29975' } } } } };
  await writeFile(join(directory, 'capital-receipts.json'), JSON.stringify(retained));
  let account = await createCapitalAccount(paperOptions, directory, '/unused', native);
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => body)).value as Json;
  const saved = async () => JSON.parse(await readFile(join(directory, 'capital-receipts.json'), 'utf8'));
  try {
    native.tick(); await delay(30);
    let disk = await saved(); assert.deepEqual(disk.exits['old-exit'].view.funds, funds); assert.deepEqual(disk.exits['old-exit'].view.progress, progress);
    for (const action of ['RESUME', 'RELEASE_RESERVATION', 'RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL']) {
      const receipt = await call('POST', '/capital-exits/old-exit/actions', { requestId: action, clientId: paperOptions.clientId, accountId: paperOptions.accountId, command: { action, schema_version: 2, expected_revision: disk.exits['old-exit'].view.revision } });
      assert.equal(receipt.status, 'REJECTED');
    }
    const stop = { requestId: 'cancel-original', clientId: paperOptions.clientId, accountId: paperOptions.accountId, command: { schema_version: 2, action: 'CANCEL', expected_revision: disk.exits['old-exit'].view.revision } };
    await call('POST', '/capital-exits/old-exit/actions', stop); native.tick(); await delay(30);
    disk = await saved(); assert.ok(disk.exits['old-exit'].pendingSlice); assert.equal(disk.commands.unknown.state, 'UNCERTAIN');
    await account.close(); account = await createCapitalAccount(paperOptions, directory, '/unused', native);
    assert.equal((await call('GET', '/capital-exits/commands/unknown')).status, 'UNCERTAIN');
    settled = true; native.snapshot.orders.push({ client_order_id: 'original-order', status: 'CANCELED' } as NativeAccountSnapshot['orders'][number]); native.tick(); disk = await eventually(saved, value => value.exits['old-exit'].view.state === 'CANCELLED_RESERVED', 'Original terminal order must settle without release');
    assert.equal(disk.exits['old-exit'].view.state, 'CANCELLED_RESERVED'); assert.deepEqual(disk.exits['old-exit'].view.funds, funds); assert.deepEqual(disk.exits['old-exit'].view.progress, progress);
    assert.equal(disk.exits['old-exit'].executionCost, '0.29975'); assert.deepEqual(disk.exits['old-exit'].fillCosts, retained.exits['old-exit'].fillCosts); assert.deepEqual(disk.ledgerClaims, retained.ledgerClaims);
    assert.equal(native.slices().length, 0); assert.equal(native.requests.filter(row => ['TARGET', 'STRATEGY_START'].includes(row.operation)).length, 0);
    assert.ok(native.requests.filter(row => row.operation === 'CANCEL_EXIT').every(row => row.payload.exit_id === 'old-exit'));
  } finally { await account.close(); await rm(directory, { recursive: true, force: true }); }
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
  const call = async (method: string, path: string, body: unknown = {}) => (await account.handle(path, request(method), async () => fencedBody(account, method, path, body, request))).value as Json;
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

test('worker durable CAS rejects a delayed older START after STOP, across restart and replay', async () => {
  const { createServer } = await import('node:http');
  const directory = await mkdtemp(join(tmpdir(), 'qz-strategy-cas-'));
  const native = new ControlledProtocol();
  let account = await createCapitalAccount(paperOptions, directory, '/unused-controlled-protocol', native);
  let release!: () => void, received!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  const entered = new Promise<void>(resolve => { received = resolve; });
  let hold = true;
  const server = createServer(async (request_, response) => {
    let bytes = ''; for await (const chunk of request_) bytes += chunk;
    const body = bytes ? JSON.parse(bytes) as Json : null;
    if (hold && body?.requestId === 'delayed-start') { hold = false; received(); await held; }
    try { const result = await account.handle(request_.url!, request_, async () => body); response.writeHead(result.status, { 'content-type': 'application/json' }); response.end(JSON.stringify(result.value)); }
    catch (error) { response.writeHead(409, { 'content-type': 'application/json' }); response.end(JSON.stringify({error:String(error)})); }
  });
  server.listen(0,'127.0.0.1'); await once(server,'listening');
  const base=`http://127.0.0.1:${(server.address() as {port:number}).port}`;
  const get = async (path: string) => (await (await fetch(base+path,{headers:{'x-qz-client-id':paperOptions.clientId}})).json()) as Json;
  const post = async (body: Json) => (await (await fetch(base+'/strategy/commands',{method:'POST',headers:{'content-type':'application/json','x-qz-client-id':paperOptions.clientId},body:JSON.stringify(body)})).json()) as Json;
  const command = (requestId:string,action:string) => ({requestId,action,clientId:paperOptions.clientId,accountId:paperOptions.accountId});
  try {
    const first = await get('/strategy/fence');
    const original = {...command('delayed-start','START'),expectedAccountSourceId:first.accountSourceId,expectedStrategyRevision:first.strategyRevision};
    const pending = post(original); await entered;
    const stopped = await post(command('later-stop','STOP')); assert.equal(stopped.status,'COMPLETED');
    release(); const stale = await pending;
    assert.equal(stale.status,'REJECTED'); assert.equal(stale.error,'strategy_start_fence_changed');
    assert.equal(native.requests.filter(row=>row.operation==='STRATEGY_START').length,0);
    await account.close(); account = await createCapitalAccount(paperOptions,directory,'/unused-controlled-protocol',native);
    assert.deepEqual(await post(original),stale);
    assert.deepEqual(await post(command('later-stop','STOP')),stopped);
    assert.equal(native.requests.filter(row=>row.operation==='STRATEGY_STOP').length,1);
    const current=await get('/strategy/fence');assert.equal(current.accountSourceId,first.accountSourceId);assert.notEqual(current.strategyRevision,first.strategyRevision);
    const newStart={...command('fresh-start','START'),expectedAccountSourceId:current.accountSourceId,expectedStrategyRevision:current.strategyRevision};
    assert.equal((await post(newStart)).status,'COMPLETED');assert.equal((await post(newStart)).status,'COMPLETED');
    assert.equal(native.requests.filter(row=>row.operation==='STRATEGY_START').length,1);
    const replaced=await post({...command('other-worker','START'),expectedAccountSourceId:'wrong-source',expectedStrategyRevision:(await get('/strategy/fence')).strategyRevision});
    assert.equal(replaced.status,'REJECTED');
  } finally {release();server.close();await once(server,'close');await account.close();await rm(directory,{recursive:true,force:true});}
});


test('missing native position value remains null without a derived available quantity', async () => {
  const native = new ControlledProtocol();
  native.snapshot.positions[0]!.value = null;
  const account = await openAccount(native);
  try {
    const result = await account.call('GET', '/account');
    assert.deepEqual(result.positions, [{ instrumentId, quantity: '10', value: null, pnl: null }]);
    assert.equal('free_quantity' in native.snapshot.positions[0]!, false);
  } finally { await account.close(); }
});
