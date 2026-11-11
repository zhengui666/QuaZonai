import { randomBytes, randomUUID } from 'node:crypto';
import { mkdir, open, readFile, rename } from 'node:fs/promises';
import { join } from 'node:path';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { AccountRuntime } from './account-runtime.ts';
import type { AccountOptions, NativeAccountRuntime, NativeAccountSnapshot, NativeInstrument } from './account-runtime.ts';
import { HyperliquidAccountReader, ledgerAmount, ledgerTransfer, units, decimal, minimum, maximum, multiply, divide, divideUp, roundDown, roundUp, DECIMAL_SCALE } from './hyperliquid-account.ts';
import type { HyperliquidAccountProof } from './hyperliquid-account.ts';
import { validateCapitalDocument } from '../../../packages/contracts/src/capital-exit.ts';
import type { CapitalExitView, CapitalExitPreview, CapitalExitPlanningContext, CapitalExitPreviewRequest, CapitalExitPolicy, CapitalExitNativeAction, CapitalExitStart, CapitalExitAction } from '../../../packages/contracts/src/capital-exit.ts';

type Json = Record<string, unknown>;
interface Slice { id: string; epoch: string; instrument: string; quantity: string; price: string; submittedAt: string }
interface Exit {
  view: CapitalExitView; preview: CapitalExitPreview; context: CapitalExitPlanningContext;
  childIds: string[]; pendingSlice: Slice | null; lastSliceAt: number; executionCost: string;
  sliceBasis: Record<string, { reference_bid: string; reference_evidence_id: string; fee_rate: string; instrument_id?: string; observed_filled_quantity?: string; not_submitted?: boolean }>;
  fillCosts: Record<string, { original: unknown; client_order_id: string; instrument_id: string; quantity: string; cost: string; fee: string }>;
  costUnresolved?: string | null;
}
interface Command { original: unknown; state: 'PENDING' | 'UNCERTAIN' | 'ACCEPTED' | 'REJECTED'; error: string | null; exitId: string; result?: unknown }
interface AccountState {
  schema_version: 2; sourceId: string; revision: string; epoch: string;
  identity: { mode: 'paper' | 'live'; network: string; accountId: string; accountAddress: string | null; currency: 'USDC' };
  previews: Record<string, { original: unknown; preview: CapitalExitPreview; context: CapitalExitPlanningContext }>;
  commands: Record<string, Command>; exits: Record<string, Exit>; targets: Record<string, { original: Json; receipt: Json; epoch: string; budget_basis?: Json }>;
  ledgerClaims: Record<string, { amount: string; original: unknown }>;
  reverseTransferPending: boolean;
}
const now = () => new Date().toISOString();
const record = (value: unknown, code = 'account_object_required'): Json => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(code, 422); return value as Json;
};
const text = (value: unknown, code: string): string => { if (typeof value !== 'string' || !value.trim()) fail(code, 422); return value as string; };
function fail(code: string, statusCode = 409): never { throw Object.assign(new Error(code), { statusCode }); }
function canonical(value: unknown): string {
  if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
  if (value && typeof value === 'object') return '{' + Object.keys(value).sort().map(key => JSON.stringify(key) + ':' + canonical((value as Json)[key])).join(',') + '}';
  return JSON.stringify(value);
}
function id(): string {
  const time = BigInt(Date.now()).toString(16).padStart(12, '0'), random = randomBytes(10).toString('hex');
  return `${time.slice(0, 8)}-${time.slice(8)}-7${random.slice(0, 3)}-${((parseInt(random[3]!, 16) & 3) | 8).toString(16)}${random.slice(4, 7)}-${random.slice(7, 19)}`;
}
const terminal = (status: string) => ['FILLED', 'CANCELED', 'CANCELLED', 'EXPIRED', 'REJECTED', 'DENIED'].includes(status.toUpperCase());
const money = (amount: string) => ({ amount, currency: 'USDC' });
const obligation = (exit: Exit) => units(exit.view.funds.reserved_amount.amount) + units(exit.view.funds.staged_for_withdrawal_amount.amount);
const active = (exit: Exit) => !exit.view.stop_action && !['PAUSED', 'CANCELLED_RESERVED', 'CANCELLING_EXIT', 'COMPLETED', 'CLOSED'].includes(exit.view.state) && obligation(exit) > 0n;

/** One financial slice calculation, using exact native quantities and net proceeds. */
export function planCapitalSlice(input: { instrument: NativeInstrument; needed: string; maximumNotional: string; participation: string;
  remainingQuantity: string; availableQuantity: string; minimumPrice: string; referenceBid: string; feeRate: string; remainingCost: string; currencyPrecision: number }) {
  const native = input.instrument, bid = units(native.bid, 'capital_bid'), step = units(native.size_increment), rate = units(input.feeRate), costLeft = units(input.remainingCost);
  if (bid <= 0n || rate < 0n || rate >= DECIMAL_SCALE) fail('capital_quote_or_fee_invalid');
  if (native.minimum_notional === null) fail('capital_native_minimum_notional_unavailable');
  const minNotional = units(native.minimum_notional), minQuantity = native.minimum_quantity === null ? step : units(native.minimum_quantity);
  if (!Number.isSafeInteger(input.currencyPrecision) || input.currencyPrecision < 0 || input.currencyPrecision > 18) fail('capital_native_currency_precision_unavailable');
  // An exact zero fee has no currency rounding error. Otherwise a fill may
  // round up by half a native Money quantum; at most quantity / size_increment
  // distinct fills can consume the order.
  const roundingPerFill = rate === 0n ? 0n : (10n ** BigInt(18 - input.currencyPrecision) + 1n) / 2n;
  const roundingBound = (quantity: bigint) => (quantity + step - 1n) / step * roundingPerFill;
  const netPerUnit = multiply(bid, DECIMAL_SCALE - rate) - divideUp(roundingPerFill, step);
  if (netPerUnit <= 0n) fail('capital_fee_rounding_exceeds_net_proceeds');
  const quantityForGoal = roundUp(divideUp(units(input.needed), netPerUnit), step);
  const minimumTrade = roundUp(maximum(minQuantity, divideUp(minNotional, bid)), step);
  let quantity = roundDown(minimum(maximum(quantityForGoal, minimumTrade), divide(units(input.maximumNotional), bid), units(input.remainingQuantity), units(input.availableQuantity)), step);
  if (quantity <= 0n) fail('capital_slice_below_native_minimum');
  if (multiply(quantity, bid) < minNotional) fail('capital_slice_below_native_minimum');
  // Price improvement can cover the fee and its rounding even with a zero
  // remaining loss budget. Solve for net proceeds before quantizing the price.
  const requiredNet = maximum(0n, multiply(quantity, units(input.referenceBid)) - costLeft + roundingBound(quantity));
  let floor = maximum(units(input.minimumPrice), divideUp(divideUp(requiredNet, quantity), DECIMAL_SCALE - rate), divideUp(minNotional, quantity));
  const priceStep = units(native.price_increment), significant = 10n ** BigInt(Math.max(0, floor.toString().length - 5));
  // Integer prices are exempt from Hyperliquid's five-significant-digit rule.
  floor = minimum(roundUp(floor, maximum(priceStep, significant)), roundUp(floor, maximum(priceStep, DECIMAL_SCALE)));
  if (floor > bid) fail('capital_price_floor_unavailable');
  const depth = native.bid_depth.filter(row => units(row.price) >= floor).reduce((sum, row) => sum + units(row.quantity), 0n);
  quantity = roundDown(minimum(quantity, multiply(depth, units(input.participation))), step);
  if (quantity < minQuantity || multiply(quantity, floor) < minNotional) fail('capital_slice_below_native_minimum');
  if (maximum(0n, multiply(quantity, units(input.referenceBid)) - multiply(multiply(quantity, floor), DECIMAL_SCALE - rate) + roundingBound(quantity)) > costLeft) fail('capital_cost_bound_exhausted');
  return { quantity: decimal(quantity), minimum_price: decimal(floor) };
}

export function capitalFillLoss(referenceBid: string, quantity: string, price: string, commissionUsdc: string): string {
  return decimal(maximum(0n, multiply(units(quantity), units(referenceBid)) - (multiply(units(quantity), units(price)) - units(commissionUsdc))));
}

/** Capital metadata stays beside existing worker receipts, never a second OMS. */
export async function createCapitalAccount(options: AccountOptions, directory: string, binary: string,
  suppliedRuntime?: NativeAccountRuntime, suppliedReader?: Pick<HyperliquidAccountReader, 'account' | 'ledger'>) {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}$/.test(options.id) || !options.clientId || !options.accountId || !options.instruments.length) fail('account_configuration_invalid', 422);
  if (options.mode === 'paper' && (options.paperFeeRate == null || units(options.paperFeeRate) < 0n || units(options.startingBalance ?? '0') <= 0n)) fail('paper_balance_and_explicit_fee_required', 422);
  if (options.mode === 'live' && !options.accountAddress) fail('hyperliquid_master_account_required', 422);
  await mkdir(directory, { recursive: true });
  const statePath = join(directory, 'capital-receipts.json');
  const identity = { mode: options.mode, network: options.network ?? 'mainnet', accountId: options.accountId, accountAddress: options.accountAddress?.toLowerCase() ?? null, currency: 'USDC' as const };
  let state: AccountState;
  try { state = JSON.parse(await readFile(statePath, 'utf8')) as AccountState; if (state.schema_version !== 2 || !state.sourceId) fail('account_receipts_version_unsupported');
    if (canonical(state.identity) !== canonical(identity)) fail('account_receipts_physical_identity_mismatch'); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    state = { schema_version: 2, sourceId: id(), identity, revision: '1', epoch: '1', previews: {}, commands: {}, exits: {}, targets: {}, ledgerClaims: {}, reverseTransferPending: false };
  }
  let writeFailed = false;
  async function save() {
    if (writeFailed) fail('account_receipts_write_failed');
    try {
      const file = await open(statePath + '.next', 'w', 0o600);
      try { await file.writeFile(JSON.stringify(state)); await file.sync(); } finally { await file.close(); }
      await rename(statePath + '.next', statePath); const root = await open(directory, 'r'); try { await root.sync(); } finally { await root.close(); }
    } catch (error) { writeFailed = true; throw error; }
  }
  await save();
  const runtime = suppliedRuntime ?? new AccountRuntime(binary, options, state.sourceId, directory);
  const reader = options.mode === 'live' ? suppliedReader ?? new HyperliquidAccountReader(options.accountAddress!, options.network) : null;
  let latest: NativeAccountSnapshot | null = null, proof: HyperliquidAccountProof | null = null, failure: string | null = 'account_runtime_starting';
  let proofReadAt = 0, appliedEpoch: string | null = null, stopped = false;
  let pendingReverse = state.reverseTransferPending ?? false;
  let reverseRows: Awaited<ReturnType<HyperliquidAccountReader['ledger']>> = [];
  const ledgerQuantized = new Map<string, { original: string; amount: bigint }>();
  let refreshInFlight: Promise<void> | null = null;
  const contexts = new Map<string, CapitalExitPlanningContext>();
  let queue: Promise<unknown> = Promise.resolve();
  function serial<T>(operation: () => Promise<T>): Promise<T> { const result = queue.then(operation); queue = result.catch(() => {}); return result; }
  const envelope = () => ({ clientId: options.clientId, accountId: options.accountId, mode: options.mode, currency: 'USDC' });
  const reserved = () => Object.values(state.exits).reduce((sum, exit) => sum + units(exit.view.funds.reserved_amount.amount), 0n);
  const cashBuffer = () => units(options.cashBuffer ?? '0');
  function reason(): string | null {
    if (writeFailed) return 'account_receipts_write_failed';
    if (pendingReverse || state.reverseTransferPending) return 'capital_external_reverse_transfer_requires_reconciliation';
    if (failure) return failure;
    if (!latest?.ready || latest.recovery_required) return latest?.reason ?? 'account_native_recovery_required';
    if (reader && (!proof || Date.parse(proof.valid_until) <= Date.now())) return 'hyperliquid_spot_evidence_unavailable';
    return proof?.reason_codes[0] ?? null;
  }
  function spotCash() { return reader ? proof?.spot_free ?? null : latest?.cash ?? null; }
  function equity(): string | null {
    if (!reader) return latest?.equity ?? null;
    if (!proof || !latest) return null;
    let value = units(proof.spot_total);
    for (const balance of proof.balances.filter(row => row.coin !== 'USDC' && units(row.total) !== 0n)) {
      const instrument = latest.instruments.find(row => row.base_currency === balance.coin && row.quote_currency === 'USDC');
      if (!instrument?.bid) return null; value += multiply(units(balance.total), units(instrument.bid));
    }
    return decimal(value);
  }
  function event(exit: Exit, kind: string, message: string, reference: string | null = null) {
    exit.view.revision = String(BigInt(exit.view.revision) + 1n); exit.view.updated_at = now();
    exit.view.events.push({ id: id(), at: exit.view.updated_at, kind, message, reference });
    if (exit.view.events.length > 256) exit.view.events.splice(0, exit.view.events.length - 256);
  }
  function clearCurrentProof(exit: Exit) {
    exit.view.funds.released_cash_amount = money('0'); exit.view.funds.verified_withdrawable_amount = null;
    exit.view.funds.evidence_asof = null; exit.view.funds.evidence_valid_until = null;
    exit.view.funds.withdrawability = options.mode === 'paper' ? 'SIMULATED' : 'UNVERIFIED';
  }
  function revisionBasis(view: CapitalExitView) {
    const { updated_at: _updatedAt, ...value } = structuredClone(view); delete value.funds.evidence_asof; delete value.funds.evidence_valid_until;
    return canonical(value);
  }
  function principalQuantum(): bigint {
    const precision = latest?.currency_precision;
    if (!Number.isSafeInteger(precision) || precision! < 0 || precision! > 18) fail('capital_native_currency_precision_unavailable');
    return 10n ** BigInt(18 - precision!);
  }
  function actions(exit: Exit) {
    const closed = ['COMPLETED', 'CLOSED'].includes(exit.view.state), pending = !!exit.pendingSlice || !!latest?.orders.some(order => !terminal(order.status) && order.side.toUpperCase() === 'BUY');
    const unresolved = pending ? 'capital_original_order_unresolved' : exit.costUnresolved ?? null;
    const stop = !closed && obligation(exit) > 0n;
    exit.view.action_availability = {
      PAUSE: { allowed: stop && !['PAUSED', 'CANCELLED_RESERVED'].includes(exit.view.state), reason: stop ? null : 'capital_plan_closed' },
      CANCEL: { allowed: stop && exit.view.state !== 'CANCELLED_RESERVED', reason: stop ? null : 'capital_plan_closed' },
      RESUME: { allowed: stop && ['PAUSED', 'BLOCKED', 'CANCELLED_RESERVED'].includes(exit.view.state) && !unresolved && !reason(), reason: unresolved ?? reason() },
      RELEASE_RESERVATION: { allowed: stop && ['PAUSED', 'CANCELLED_RESERVED', 'BLOCKED', 'READY'].includes(exit.view.state) && !unresolved, reason: unresolved },
      RECONCILE_EXTERNAL_TRANSFER: { allowed: !!reader && stop, reason: reader ? null : 'paper_has_no_external_cash_movement' },
      RECONCILE_WITHDRAWAL: { allowed: !!reader && stop && units(exit.view.funds.staged_for_withdrawal_amount.amount) > 0n, reason: reader ? 'capital_staged_transfer_required_if_unavailable' : 'paper_has_no_external_cash_movement' },
    };
  }
  function feeRate(value: NativeInstrument): string | null { return reader ? proof?.spot_taker_fee_rate ?? null : value.taker_fee_rate; }
  async function readNativeSnapshot(): Promise<NativeAccountSnapshot> {
    const value = await runtime.request('SNAPSHOT');
    if (!Array.isArray(value.orders) || !Array.isArray(value.positions) || !Array.isArray(value.instruments) || !Array.isArray(value.fills)
      || typeof value.sequence !== 'string' || typeof value.observation_id !== 'string') fail('account_native_snapshot_invalid');
    return value as unknown as NativeAccountSnapshot;
  }
  async function applyControl() {
    const candidates = Object.values(state.exits).filter(active);
    const selected = candidates.find(exit => exit.view.policy.kind === 'NATURAL_RELEASE') ?? candidates.find(exit => exit.view.policy.kind === 'BOUNDED_LIMIT') ?? candidates[0];
    const bounded = selected?.view.policy.kind === 'BOUNDED_LIMIT' ? selected.view.policy : null;
    const result = await runtime.request('CONTROL', { epoch: state.epoch, revision: state.revision, reserved_amount: decimal(reserved()), cash_buffer: decimal(cashBuffer()),
      mode: state.reverseTransferPending ? 'PAUSED' : selected?.view.policy.kind ?? (Object.values(state.exits).some(exit => obligation(exit) > 0n) ? 'CASH_ONLY' : 'NORMAL'), exit_id: selected?.view.id ?? null,
      exit_ids: candidates.map(exit => exit.view.id),
      minimum_interval_seconds: bounded?.pacing.minimum_interval_seconds ?? null, deadline: selected?.view.policy.kind !== 'CASH_ONLY' ? selected?.view.policy.deadline ?? null : null,
      taker_fee_rate: reader ? proof?.spot_taker_fee_rate ?? null : options.paperFeeRate ?? null,
      spot_free_cash: reader ? proof?.spot_free ?? null : latest?.cash ?? null, spot_total_cash: reader ? proof?.spot_total ?? null : latest?.cash ?? null,
      spot_balances: reader ? proof?.balances ?? null : null,
      spot_facts_asof_ns: reader && proof ? String(BigInt(Date.parse(proof.asof)) * 1_000_000n) : latest?.observed_at_ns ?? null,
      spot_facts_valid_until_ns: reader && proof ? String(BigInt(Date.parse(proof.valid_until)) * 1_000_000n) : latest ? String(BigInt(latest.observed_at_ns) + 5_000_000_000n) : null });
    if (result.applied !== true) fail(typeof result.reason === 'string' ? result.reason : 'capital_native_fence_not_confirmed');
    appliedEpoch = state.epoch;
    // The official owner produces a new observation when control or spot
    // economic values change. Read it before publishing the corresponding
    // /account projection; TypeScript never manufactures a native sequence.
    latest = await readNativeSnapshot();
  }
  function instrument(id_: string): NativeInstrument {
    const value = latest?.instruments.find(row => row.instrument_id === id_ && row.quote_currency === 'USDC');
    if (!value) fail('capital_spot_instrument_unavailable'); return value;
  }
  function freshInstrument(value: NativeInstrument) {
    if (!value.asof_ns || !value.bid || !value.ask || BigInt(Date.now()) * 1_000_000n - BigInt(value.asof_ns) > 5_000_000_000n) fail('capital_quote_or_book_stale');
    if (units(value.bid) <= 0n || units(value.ask) < units(value.bid)) fail('capital_quote_invalid');
  }
  function planningContext(): CapitalExitPlanningContext {
    const blocked = reason(), frame = latest;
    if (!frame) fail(blocked ?? 'account_native_observation_unavailable', 503);
    const existing = [...contexts.values()].find(value => value.observation_id === frame.observation_id && value.account_control_revision === state.revision && Date.parse(value.valid_until) > Date.now());
    if (existing) return existing;
    const contextId = id();
    const cash = spotCash(), total = reader ? proof?.spot_total ?? null : cash;
    const context: CapitalExitPlanningContext = { schema_version: 2, id: contextId, account_source_id: state.sourceId, observation_id: frame.observation_id,
      account_control_revision: state.revision, currency: 'USDC', environment: options.mode === 'paper' ? 'PAPER' : 'LIVE',
      asof: new Date(Number(BigInt(frame.observed_at_ns) / 1_000_000n)).toISOString(), valid_until: new Date(Date.now() + 60_000).toISOString(),
      funds: { total_cash: total === null ? null : money(total), free_cash: cash === null ? null : money(cash), locked_cash: reader ? proof ? money(proof.spot_hold) : null : total !== null && cash !== null ? money(decimal(units(total) - units(cash))) : null,
        verified_withdrawable: reader && proof?.withdrawable !== null && proof?.withdrawable !== undefined ? money(proof.withdrawable) : null },
      instruments: frame.positions.map(position => {
        const source = frame.instruments.find(row => row.instrument_id === position.instrument_id);
        return source ? { instrument_id: source.instrument_id, symbol: source.instrument_id.split('.')[0]!, quantity: position.quantity, eligible_quantity: position.free_quantity,
          quantity_increment: source.size_increment, price_increment: source.price_increment, bid_price: source.bid, ask_price: source.ask,
          quote_asof: source.asof_ns ? new Date(Number(BigInt(source.asof_ns) / 1_000_000n)).toISOString() : null, fee_rate: feeRate(source), reference_evidence_id: contextId } : null;
      }).filter((value): value is NonNullable<typeof value> => value !== null), capability: blocked ? 'BLOCKED' : 'SUPPORTED', reason_codes: blocked ? [blocked] : [],
      supported_policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'] };
    contexts.set(contextId, context); if (contexts.size > 64) contexts.delete(contexts.keys().next().value!); return context;
  }
  async function refresh(): Promise<void> {
    if (refreshInFlight) return refreshInFlight;
    refreshInFlight = (async () => {
      try {
        const frame = await readNativeSnapshot();
        let incomingProof: HyperliquidAccountProof | null = null;
        let incomingRows: typeof reverseRows = [];
        const incomingAmounts = new Map<string, { original: string; amount: bigint }>();
        if (reader && Date.now() - proofReadAt >= 3_000) {
          // Read I/O outside the mutation lane. Publish the complete balance and
          // reverse-transfer evidence together, after any current command ends.
          const obligations = Object.values(state.exits).filter(exit => obligation(exit) > 0n);
          incomingProof = await reader.account();
          if (obligations.length) {
            incomingRows = await reader.ledger(Math.min(...obligations.map(exit => Date.parse(exit.view.created_at))));
            for (const row of incomingRows) if (row.delta.type === 'accountClassTransfer' && row.delta.toPerp === false) {
              incomingAmounts.set(row.hash, await readQuantizedLedger(row, frame.currency_precision));
            }
          }
        }
        await serial(async () => {
          if (stopped) return;
          if (!latest || BigInt(frame.observed_at_ns) >= BigInt(latest.observed_at_ns)) latest = frame;
          failure = null;
          let publishedProof = false;
          if (incomingProof && (!proof || Date.parse(incomingProof.asof) >= Date.parse(proof.asof))) {
            proof = incomingProof; proofReadAt = Date.now(); appliedEpoch = null; publishedProof = true;
          }
          const merged = new Map(reverseRows.map(row => [row.hash, row]));
          for (const row of incomingRows) {
            const old = merged.get(row.hash); if (old && canonical(old) !== canonical(row)) fail('capital_external_ledger_conflict');
            merged.set(row.hash, row);
          }
          reverseRows = [...merged.values()];
          for (const [ref, entry] of incomingAmounts) {
            const old = ledgerQuantized.get(ref); if (old && old.original !== entry.original) fail('capital_external_ledger_conflict');
            ledgerQuantized.set(ref, entry);
          }
          pendingReverse = hasUnclaimedReverse();
          if ((state.reverseTransferPending ?? false) !== pendingReverse) {
            state.reverseTransferPending = pendingReverse; bumpControl(); await save();
            // The new balance never becomes spendable under the previous epoch.
            await applyControl();
          } else if (publishedProof) await applyControl();
        });
      } catch (error) { await serial(async () => { failure = error instanceof Error ? error.message : 'account_native_status_unavailable'; }); }
    })().finally(() => { refreshInFlight = null; });
    return refreshInFlight;
  }
  function hasUnclaimedReverse() {
    if (!Object.values(state.exits).some(exit => units(exit.view.funds.staged_for_withdrawal_amount.amount) > 0n)) return false;
    return reverseRows.some(row => row.delta.type === 'accountClassTransfer' && row.delta.toPerp === false
      && (ledgerQuantized.get(row.hash)?.amount ?? units(row.delta.usdc, 'capital_reverse_transfer_amount')) > units(state.ledgerClaims[row.hash]?.amount ?? '0'));
  }
  async function readQuantizedLedger(row: Awaited<ReturnType<HyperliquidAccountReader['ledger']>>[number], precision: number) {
    if (!Number.isSafeInteger(precision) || precision < 0 || precision > 18) fail('capital_native_currency_precision_unavailable');
    const rawAmount = units(row.delta.usdc, 'capital_ledger_usdc');
    const value = await runtime.request('QUANTIZE_MONEY', { amount: decimal(rawAmount), currency: 'USDC' });
    if (typeof value.amount !== 'string' || value.precision !== precision) fail('capital_native_money_quantization_unavailable');
    const amount = units(value.amount); if (amount < 0n || amount % (10n ** BigInt(18 - precision)) !== 0n) fail('capital_native_money_quantization_invalid');
    return { original: canonical(row), amount };
  }
  async function quantizedLedger(row: Awaited<ReturnType<HyperliquidAccountReader['ledger']>>[number]): Promise<bigint> {
    const cached = ledgerQuantized.get(row.hash);
    if (cached) { if (cached.original !== canonical(row)) fail('capital_external_ledger_conflict'); return cached.amount; }
    principalQuantum();
    const entry = await readQuantizedLedger(row, latest!.currency_precision); ledgerQuantized.set(row.hash, entry); return entry.amount;
  }
  function evaluatePreview(request: CapitalExitPreviewRequest, context: CapitalExitPlanningContext): CapitalExitPreview {
    const reasons: string[] = [], desired = units(request.scope.amount), cash = spotCash(), accountEquity = equity();
    const resumed = request.resume_exit_id ? state.exits[request.resume_exit_id] : null;
    if (request.resume_exit_id && (!resumed || resumed.view.revision !== request.expected_exit_revision || units(resumed.view.funds.reserved_amount.amount) !== desired)) reasons.push('capital_resume_exit_basis_changed');
    const otherReserved = reserved() - (resumed ? units(resumed.view.funds.reserved_amount.amount) : 0n);
    if (reason()) reasons.push(reason()!);
    if (desired <= 0n || request.scope.currency !== 'USDC') reasons.push('capital_amount_or_currency_invalid');
    if (desired % principalQuantum() !== 0n) reasons.push('capital_currency_quantum_invalid');
    if (accountEquity === null) reasons.push('capital_managed_equity_unavailable');
    else if (desired > maximum(0n, units(accountEquity) - otherReserved - cashBuffer())) reasons.push('capital_amount_exceeds_unreserved_managed_capital');
    const free = cash === null ? null : maximum(0n, units(cash) - otherReserved - cashBuffer());
    let estimatedCost: bigint | null = null;
    if (request.policy.kind === 'CASH_ONLY' && (free === null || free < desired)) reasons.push('capital_cash_only_insufficient');
    if (request.policy.kind !== 'CASH_ONLY' && Date.parse(request.policy.deadline) <= Date.now()) reasons.push('capital_deadline_expired');
    if (request.policy.kind !== 'CASH_ONLY' && Object.values(state.exits).some(exit => exit !== resumed && obligation(exit) > 0n && exit.view.policy.kind !== 'CASH_ONLY' && exit.view.policy.kind !== request.policy.kind)) reasons.push('capital_exit_policy_conflict');
    if (request.policy.kind === 'BOUNDED_LIMIT') {
      const policy = request.policy; estimatedCost = resumed ? units(resumed.executionCost) : 0n; let release = free ?? 0n;
      const outstanding = maximum(0n, desired - release), executionReasons: string[] = []; let currentSliceChecked = outstanding === 0n;
      if (policy.max_execution_cost.currency !== 'USDC' || policy.max_execution_cost.reference_evidence_id !== context.id) reasons.push('capital_cost_reference_mismatch');
      for (const leg of policy.legs) {
        try {
          const native = instrument(leg.instrument_id); freshInstrument(native);
          const original = context.instruments.find(row => row.instrument_id === leg.instrument_id);
          const quantity = units(leg.maximum_reduction_quantity), floor = units(leg.minimum_sell_price), fee = feeRate(native);
          const holding = latest?.positions.find(row => row.instrument_id === leg.instrument_id);
          if (!holding || quantity > units(holding.free_quantity) || !original?.bid_price || fee === null) fail('capital_holdings_or_fee_evidence_unavailable');
          if (roundDown(quantity, units(native.size_increment)) !== quantity || floor <= 0n) fail('capital_native_precision_invalid');
          const roundingBound = (value: bigint) => units(fee) === 0n ? 0n : (value + units(native.size_increment) - 1n) / units(native.size_increment) * ((principalQuantum() + 1n) / 2n);
          // This is authorization capacity at the current bid, not a promise
          // about future fills. Only the next executable batch is costed below.
          const grossAtBid = multiply(quantity, units(native.bid));
          release += maximum(0n, grossAtBid - multiply(grossAtBid, units(fee)) - roundingBound(quantity));
          if (!currentSliceChecked) {
            try {
              const planned = planCapitalSlice({ instrument: native, needed: decimal(outstanding), maximumNotional: policy.pacing.maximum_slice_notional,
                participation: policy.pacing.maximum_participation, remainingQuantity: leg.maximum_reduction_quantity, availableQuantity: holding.free_quantity,
                minimumPrice: leg.minimum_sell_price, referenceBid: original.bid_price, feeRate: fee,
                remainingCost: decimal(maximum(0n, units(policy.max_execution_cost.amount) - estimatedCost)), currencyPrecision: latest!.currency_precision });
              const plannedQuantity = units(planned.quantity), gross = multiply(plannedQuantity, units(planned.minimum_price));
              estimatedCost += maximum(0n, multiply(plannedQuantity, units(original.bid_price)) - (gross - multiply(gross, units(fee)) - roundingBound(plannedQuantity)));
              currentSliceChecked = true;
            } catch (error) { executionReasons.push(error instanceof Error ? error.message : 'capital_slice_unavailable'); }
          }
          if (native.minimum_notional === null) fail('capital_native_minimum_notional_unavailable');
          if (units(policy.pacing.maximum_slice_notional) < units(native.minimum_notional) || divide(units(policy.pacing.maximum_slice_notional), floor) < units(native.minimum_quantity ?? native.size_increment)) reasons.push('capital_slice_below_native_minimum');
        } catch (error) { reasons.push(error instanceof Error ? error.message : 'capital_leg_unavailable'); }
      }
      if (!currentSliceChecked) reasons.push(executionReasons[0] ?? 'capital_current_slice_unavailable');
      if (estimatedCost > units(policy.max_execution_cost.amount)) reasons.push('capital_cost_bound_exceeded');
      if (release < desired) reasons.push('capital_authorized_quantity_insufficient');
    }
    return { schema_version: 2, id: id(), plan_artifact_id: id(), account_source_id: state.sourceId, original_observation_id: request.expected_source_observation_id,
      expected_account_control_revision: state.revision, environment: options.mode === 'paper' ? 'PAPER' : 'LIVE', scope: request.scope, policy: request.policy,
      capability: reasons.length ? 'BLOCKED' : 'SUPPORTED', reason_codes: [...new Set(reasons)], valid_until: new Date(Date.now() + 60_000).toISOString(),
      ...(resumed ? { resume_exit_id: resumed.view.id, expected_exit_revision: request.expected_exit_revision } : {}),
      ...(request.policy.kind === 'BOUNDED_LIMIT' ? { execution_estimate_scope: 'CURRENT_EXECUTABLE_SLICE' } : {}),
      funds: { requested: money(request.scope.amount), remaining_managed_capital: accountEquity === null ? null : money(decimal(maximum(0n, units(accountEquity) - otherReserved - desired))),
        estimated_execution_cost: estimatedCost === null ? null : money(decimal(estimatedCost)), native_free_cash: cash === null ? null : money(cash) },
      proposed_cancellations: (latest?.orders ?? []).filter(order => !terminal(order.status) && order.side.toUpperCase() === 'BUY').map(order => ({ native_client_order_id: order.client_order_id, native_strategy_id: latest!.native_trader_id, native_instrument_id: order.instrument_id })),
      retained_protective_orders: (latest?.orders ?? []).filter(order => !terminal(order.status) && order.reduce_only).map(order => ({ native_client_order_id: order.client_order_id, native_strategy_id: latest!.native_trader_id, native_instrument_id: order.instrument_id })),
      reduction_legs: request.policy.kind === 'BOUNDED_LIMIT' ? request.policy.legs : [],
      original_fee_evidence: reader ? proof?.original.fees ?? null : { kind: 'EXPLICIT_PAPER_FEE', rate: options.paperFeeRate } };
  }
  async function preview(body: Json) {
    const requestId = text(body.requestId, 'request_id'), request = record(body.request) as unknown as CapitalExitPreviewRequest;
    const prior = state.previews[requestId]; if (prior) { if (canonical(prior.original) !== canonical(body)) fail('request_id_conflict'); return { ...envelope(), requestId, preview: prior.preview }; }
    validateCapitalDocument('REQUEST', request);
    const context = contexts.get(request.planning_context_id);
    if (!context || Date.parse(context.valid_until) <= Date.now() || context.account_control_revision !== state.revision || request.account_source_id !== state.sourceId || request.expected_account_control_revision !== state.revision || request.expected_source_observation_id !== context.observation_id) fail('capital_planning_context_stale');
    const value = evaluatePreview(request, context); state.previews[requestId] = { original: body, preview: value, context }; await save();
    return { ...envelope(), requestId, preview: value };
  }
  function assertPreview(start: CapitalExitStart) {
    const captured = Object.values(state.previews).find(value => value.preview.id === start.preview_id);
    if (!captured || captured.preview.capability !== 'SUPPORTED' || Date.parse(captured.preview.valid_until) <= Date.now() || captured.preview.expected_account_control_revision !== state.revision
      || start.expected_account_control_revision !== state.revision || start.acknowledged_plan_artifact_id !== captured.preview.plan_artifact_id || start.expected_source_observation_id !== captured.preview.original_observation_id) fail('capital_preview_invalid_or_stale');
    const fresh = evaluatePreview({ schema_version: 2, account_source_id: state.sourceId, expected_source_observation_id: captured.context.observation_id,
      expected_account_control_revision: state.revision, planning_context_id: captured.context.id, scope: captured.preview.scope, policy: captured.preview.policy,
      ...(captured.preview.resume_exit_id ? { resume_exit_id: captured.preview.resume_exit_id, expected_exit_revision: captured.preview.expected_exit_revision } : {}) }, captured.context);
    if (fresh.capability !== 'SUPPORTED') fail(fresh.reason_codes[0] ?? 'capital_preview_no_longer_executable'); return captured;
  }
  function bumpControl() { state.revision = String(BigInt(state.revision) + 1n); state.epoch = String(BigInt(state.epoch) + 1n); appliedEpoch = null;
    for (const exit of Object.values(state.exits)) if (obligation(exit) > 0n) { exit.view.account_control_revision = state.revision; exit.view.remaining_trading = 'FENCED_PENDING_TARGET'; } }
  async function command(body: Json, exitId?: string) {
    const requestId = text(body.requestId, 'request_id'), original = { ...body, ...(exitId ? { exitId } : {}) };
    const previous = state.commands[requestId]; if (previous) { if (canonical(previous.original) !== canonical(original)) fail('request_id_conflict'); return commandReceipt(requestId); }
    const native = record(body.command); let exit: Exit;
    if (!exitId) {
      validateCapitalDocument('START', native); const captured = assertPreview(native as unknown as CapitalExitStart), stamp = now(), value = captured.preview;
      if (value.resume_exit_id) fail('capital_resume_preview_cannot_start_new_exit');
      exit = { preview: value, context: captured.context, childIds: [], sliceBasis: {}, fillCosts: {}, pendingSlice: null, lastSliceAt: 0, executionCost: '0', view: {
        schema_version: 2, id: id(), revision: '1', account_source_id: state.sourceId, environment: value.environment, state: 'FENCING', preparation_state: 'WAITING_CASH',
        account_control_revision: state.revision, command_id: requestId, reason_codes: [], policy: value.policy, scope: value.scope, remaining_trading: 'FENCED_PENDING_TARGET',
        funds: { requested_amount: money(value.funds.requested.amount), reserved_amount: money(value.funds.requested.amount), staged_for_withdrawal_amount: money('0'), reconciled_withdrawal_amount: money('0'), waived_amount: money('0'),
          released_cash_amount: null, verified_withdrawable_amount: null, withdrawability: options.mode === 'paper' ? 'SIMULATED' : 'UNVERIFIED' },
        progress: value.reduction_legs.map(leg => ({ instrument_id: leg.instrument_id, authorized_quantity: leg.maximum_reduction_quantity, filled_quantity: '0', remaining_quantity: leg.maximum_reduction_quantity, active_order_ids: [], execution_cost: money('0') })),
        action_availability: {}, events: [], created_at: stamp, updated_at: stamp } };
      state.exits[exit.view.id] = exit; bumpControl(); event(exit, 'REQUESTED', '资金准备请求已保存，等待原生预算隔离确认。', requestId);
    } else {
      validateCapitalDocument('ACTION', native); exit = state.exits[exitId]!; if (!exit) fail('capital_exit_not_found', 404);
      const action = native as unknown as CapitalExitNativeAction; actions(exit);
      if (action.expected_revision !== exit.view.revision) fail('capital_exit_revision_conflict');
      if (exit.view.action_availability[action.action]?.allowed !== true) fail(exit.view.action_availability[action.action]?.reason ?? 'capital_action_unavailable');
      if (action.action === 'RESUME') {
        const captured = Object.values(state.previews).find(value => value.preview.id === action.preview_id);
        if (!captured || captured.preview.resume_exit_id !== exit.view.id || captured.preview.expected_exit_revision !== exit.view.revision
          || units(captured.preview.scope.amount) !== units(exit.view.funds.reserved_amount.amount)) fail('capital_resume_scope_mismatch');
        assertPreview({ schema_version: 2, preview_id: captured.preview.id, expected_account_control_revision: state.revision, acknowledged_plan_artifact_id: captured.preview.plan_artifact_id, expected_source_observation_id: captured.preview.original_observation_id });
        const additional = captured.preview.policy.kind === 'BOUNDED_LIMIT' ? captured.preview.policy.legs : [];
        const instruments = new Set([...exit.view.progress.map(row => row.instrument_id), ...additional.map(row => row.instrument_id)]);
        exit.view.progress = [...instruments].map(instrument_id => {
          const filled = Object.values(exit.fillCosts).filter(row => row.instrument_id === instrument_id).reduce((sum, row) => sum + units(row.quantity), 0n);
          const newAuthorization = additional.find(row => row.instrument_id === instrument_id)?.maximum_reduction_quantity ?? '0';
          return { instrument_id, filled_quantity: decimal(filled), authorized_quantity: decimal(filled + units(newAuthorization)), remaining_quantity: newAuthorization,
            active_order_ids: [], execution_cost: money(decimal(Object.values(exit.fillCosts).filter(row => row.instrument_id === instrument_id).reduce((sum, row) => sum + units(row.cost), 0n))) };
        });
        exit.preview = captured.preview; exit.context = captured.context; exit.view.policy = captured.preview.policy; delete exit.view.stop_action; exit.view.state = 'FENCING'; bumpControl();
      } else if (action.action === 'PAUSE' || action.action === 'CANCEL') { exit.view.state = 'CANCELLING_EXIT'; exit.view.stop_action = action.action; bumpControl(); }
      else if (action.action === 'RELEASE_RESERVATION') {
        exit.view.funds.waived_amount.amount = decimal(units(exit.view.funds.waived_amount.amount) + obligation(exit));
        exit.view.funds.reserved_amount = money('0'); exit.view.funds.staged_for_withdrawal_amount = money('0'); exit.view.state = 'CLOSED'; exit.view.preparation_state = 'CLOSED'; bumpControl();
      } else {
        if (!reader) fail('paper_has_no_external_cash_movement');
        const transfer = action.action === 'RECONCILE_EXTERNAL_TRANSFER', amount = units(action.user_reported_amount, 'capital_reconcile_amount'), ref = text(action.external_transfer_ref, 'capital_external_reference');
        if (action.currency !== 'USDC' || amount <= 0n) fail('capital_amount_or_currency_invalid');
        if (amount % principalQuantum() !== 0n) fail('capital_currency_quantum_invalid');
        const rows = (await reader.ledger(Date.parse(exit.view.created_at))).filter(row => row.hash === ref);
        if (rows.length !== 1) fail('capital_external_ledger_reference_unresolved');
        const row = rows[0]!, movement = transfer ? ledgerTransfer(row) : null, claim = state.ledgerClaims[ref];
        if (!transfer) ledgerAmount(row, 'WITHDRAWAL');
        const sourceAmount = await quantizedLedger(row);
        if (row.time < Date.parse(exit.view.created_at) || row.time > Date.now()) fail('capital_external_ledger_outside_plan');
        if (claim && canonical(claim.original) !== canonical(row)) fail('capital_external_ledger_conflict');
        if (amount + units(claim?.amount ?? '0') > sourceAmount) fail('capital_external_ledger_already_allocated');
        const outward = !movement || movement.direction === 'TO_WITHDRAWAL';
        const from = transfer && outward ? exit.view.funds.reserved_amount : exit.view.funds.staged_for_withdrawal_amount;
        if (amount > units(from.amount)) fail(transfer ? 'capital_transfer_exceeds_spot_reservation' : 'capital_withdrawal_exceeds_staged_funds');
        // Refresh after reading the confirmed movement, before lowering the
        // spot reservation. An older pre-transfer cash proof cannot increase BUY allowance.
        const after = await reader.account();
        if (after.reason_codes.length) fail(after.reason_codes[0]!);
        proof = after; proofReadAt = Date.now();
        from.amount = decimal(units(from.amount) - amount);
        const to = transfer ? outward ? exit.view.funds.staged_for_withdrawal_amount : exit.view.funds.reserved_amount : exit.view.funds.reconciled_withdrawal_amount; to.amount = decimal(units(to.amount) + amount);
        state.ledgerClaims[ref] = { amount: decimal(amount + units(claim?.amount ?? '0')), original: row };
        pendingReverse = hasUnclaimedReverse(); state.reverseTransferPending = pendingReverse;
        if (obligation(exit) === 0n) { exit.view.state = 'COMPLETED'; exit.view.preparation_state = 'CLOSED'; }
        bumpControl();
      }
      clearCurrentProof(exit);
      exit.view.command_id = requestId; event(exit, String(native.action), '已记录原始账户操作，资金与外部流水分别核对。', requestId);
    }
    state.commands[requestId] = { original, state: 'PENDING', error: null, exitId: exit.view.id }; await save();
    await settleControl(requestId); actions(exit); return commandReceipt(requestId);
  }
  function commandReceipt(requestId: string) {
    const value = state.commands[requestId]; if (!value) fail('capital_command_not_found', 404);
    return { ...envelope(), requestId, status: value.state, state: value.state, error: value.error,
      command: record(value.original).command, result: value.state === 'ACCEPTED' ? structuredClone(state.exits[value.exitId]?.view) : null };
  }
  async function admitCommand(body: Json, exitId?: string) {
    try { return await command(body, exitId); }
    catch (error) {
      const requestId = text(body.requestId, 'request_id');
      if (!state.commands[requestId] && !writeFailed) {
        state.commands[requestId] = { original: { ...body, ...(exitId ? { exitId } : {}) }, state: 'REJECTED', error: error instanceof Error ? error.message : 'capital_native_admission_rejected', exitId: exitId ?? '' };
        await save(); return commandReceipt(requestId);
      }
      throw error;
    }
  }
  async function settleControl(requestId: string) {
    const command = state.commands[requestId]!;
    try {
      await applyControl(); const exit = state.exits[command.exitId]!;
      if (exit.view.state === 'CANCELLING_EXIT') await runtime.request('CANCEL_EXIT', { epoch: state.epoch, exit_id: exit.view.id });
      command.state = 'ACCEPTED'; command.error = null;
    } catch (error) { command.state = 'UNCERTAIN'; command.error = error instanceof Error ? error.message : 'capital_native_control_uncertain'; }
    await save();
  }
  async function advance(allowSlice = false) {
    if (writeFailed) return;
    if (!latest?.ready || latest.recovery_required) return;
    if ((state.reverseTransferPending ?? false) !== pendingReverse) { state.reverseTransferPending = pendingReverse; bumpControl(); await save(); }
    for (const [requestId, command] of Object.entries(state.commands)) if (['PENDING', 'UNCERTAIN'].includes(command.state)) await settleControl(requestId);
    for (const [targetId, target] of Object.entries(state.targets)) if (target.receipt.status === 'PENDING') await settleTarget(targetId);
    if (appliedEpoch !== state.epoch) { try { await applyControl(); } catch { return; } }
    let available = spotCash() === null ? null : maximum(0n, units(spotCash()!) - cashBuffer());
    let withdrawalAvailable = reader && proof?.withdrawable != null ? units(proof.withdrawable) : null; let changed = false;
    for (const exit of Object.values(state.exits)) {
      if (['COMPLETED', 'CLOSED'].includes(exit.view.state)) { actions(exit); continue; }
      const original = canonical(exit.view), originalRevisionBasis = revisionBasis(exit.view), holding = units(exit.view.funds.reserved_amount.amount);
      const released = available === null ? null : minimum(holding, available); if (available !== null) available -= released!;
      exit.view.funds.released_cash_amount = released === null ? null : money(decimal(released));
      if (exit.pendingSlice) {
        const order = latest.orders.find(value => value.client_order_id === exit.pendingSlice!.id);
        if (order && terminal(order.status)) { exit.pendingSlice = null; exit.lastSliceAt = Date.now(); }
        else exit.view.reason_codes = order ? [] : ['capital_original_order_requires_reconciliation'];
      }
      let costFailure: string | null = null;
      exit.fillCosts ??= {}; exit.sliceBasis ??= {};
      for (const fill of latest.fills.filter(value => exit.childIds.includes(value.client_order_id))) {
        const basis = exit.sliceBasis[fill.client_order_id], key = `${fill.client_order_id}:${fill.trade_id}`;
        if (!basis) { costFailure = 'capital_original_slice_cost_basis_unavailable'; continue; }
        if (typeof fill.commission !== 'string' || typeof fill.commission_currency !== 'string') { costFailure = 'capital_fill_commission_evidence_unavailable'; continue; }
        let commission = units(fill.commission), source = latest.instruments.find(value => value.instrument_id === fill.instrument_id);
        if (fill.commission_currency !== 'USDC') {
          if (source && fill.commission_currency === source.base_currency) commission = multiply(commission, units(fill.price));
          else { costFailure = 'capital_fee_currency_evidence_unsupported'; continue; }
        }
        const contribution = { original: fill, client_order_id: fill.client_order_id, instrument_id: fill.instrument_id, quantity: fill.quantity,
          fee: decimal(commission), cost: capitalFillLoss(basis.reference_bid, fill.quantity, fill.price, decimal(commission)) };
        if (exit.fillCosts[key] && canonical(exit.fillCosts[key]!.original) !== canonical(fill)) { costFailure = 'capital_native_fill_identity_conflict'; continue; }
        exit.fillCosts[key] ??= contribution;
      }
      const authoritativeFilled = new Map<string, bigint>(), unresolvedInstruments = new Set<string>();
      for (const childId of exit.childIds) {
        const basis = exit.sliceBasis[childId], order = latest.orders.find(value => value.client_order_id === childId);
        const contributions = Object.values(exit.fillCosts).filter(value => value.client_order_id === childId);
        const contributed = contributions.reduce((sum, value) => sum + units(value.quantity), 0n);
        const instrumentId = order?.instrument_id ?? basis?.instrument_id ?? contributions[0]?.instrument_id;
        if (basis?.not_submitted && !order && contributed === 0n) continue;
        const observed = order ? units(order.filled_quantity) : 0n, previous = units(basis?.observed_filled_quantity ?? '0');
        const filled = maximum(observed, previous, contributed);
        if (instrumentId) authoritativeFilled.set(instrumentId, (authoritativeFilled.get(instrumentId) ?? 0n) + filled);
        if (!order || observed < previous || observed !== contributed || basis?.not_submitted) {
          costFailure ??= !order ? 'capital_original_order_requires_reconciliation' : 'capital_fill_history_incomplete';
          if (instrumentId) unresolvedInstruments.add(instrumentId);
        }
        if (basis && order) { basis.instrument_id = order.instrument_id; basis.observed_filled_quantity = decimal(maximum(previous, observed)); }
      }
      let cost = 0n;
      for (const progress of exit.view.progress) {
        const contributions = Object.values(exit.fillCosts).filter(value => value.instrument_id === progress.instrument_id);
        const filled = authoritativeFilled.get(progress.instrument_id) ?? contributions.reduce((sum, value) => sum + units(value.quantity), 0n), legCost = contributions.reduce((sum, value) => sum + units(value.cost), 0n);
        progress.filled_quantity = decimal(filled); progress.remaining_quantity = decimal(maximum(0n, units(progress.authorized_quantity) - filled));
        progress.active_order_ids = latest.orders.filter(value => exit.childIds.includes(value.client_order_id) && value.instrument_id === progress.instrument_id && !terminal(value.status)).map(value => value.client_order_id);
        progress.execution_cost = costFailure && (unresolvedInstruments.size === 0 || unresolvedInstruments.has(progress.instrument_id)) ? null : money(decimal(legCost)); cost += legCost;
        progress.actual_fees = progress.execution_cost === null ? null : money(decimal(contributions.reduce((sum, value) => sum + units(value.fee), 0n)));
      }
      exit.executionCost = decimal(maximum(cost, units(exit.executionCost))); exit.costUnresolved = costFailure;
      exit.view.funds.actual_fees = costFailure ? null : money(decimal(Object.values(exit.fillCosts).reduce((sum, value) => sum + units(value.fee), 0n)));
      // Preparation evidence is independent of execution being active or paused.
      const staged = units(exit.view.funds.staged_for_withdrawal_amount.amount);
      const withdrawal = withdrawalAvailable === null ? null : minimum(staged, withdrawalAvailable);
      if (withdrawalAvailable !== null) withdrawalAvailable -= withdrawal!;
      const openingUnsettled = latest.orders.some(order => !terminal(order.status) && order.side.toUpperCase() === 'BUY');
      const blocked = reason() ?? (openingUnsettled ? 'capital_opening_orders_require_reconciliation' : null);
      if (blocked) {
        exit.view.preparation_state = 'STALE'; exit.view.funds.withdrawability = 'STALE'; exit.view.funds.verified_withdrawable_amount = null;
      } else {
        exit.view.funds.verified_withdrawable_amount = reader ? withdrawal === null ? null : money(decimal(withdrawal)) : released === null ? null : money(decimal(released));
        exit.view.funds.withdrawability = reader ? withdrawal === null ? 'UNVERIFIED' : 'VERIFIED' : 'SIMULATED';
        const nativeAsOf = new Date(Number(BigInt(latest.observed_at_ns) / 1_000_000n)).toISOString();
        exit.view.funds.evidence_asof = proof?.asof ?? nativeAsOf;
        exit.view.funds.evidence_valid_until = proof?.valid_until ?? new Date(Date.parse(nativeAsOf) + 5_000).toISOString();
        const ready = released !== null && released >= holding && (!reader || holding === 0n && withdrawal !== null && withdrawal >= staged);
        exit.view.preparation_state = ready ? 'READY' : released !== null && released >= holding ? 'WAITING_EXTERNAL_TRANSFER' : active(exit) && exit.view.policy.kind === 'BOUNDED_LIMIT' ? 'REDUCING' : 'WAITING_CASH';
      }
      if (costFailure) { exit.view.state = 'BLOCKED'; exit.view.preparation_state = 'BLOCKED'; exit.view.reason_codes = [costFailure]; clearCurrentProof(exit); }
      else if (exit.view.state === 'CANCELLING_EXIT' && !exit.pendingSlice && !openingUnsettled && !latest.orders.some(order => exit.childIds.includes(order.client_order_id) && !terminal(order.status))) {
        exit.view.state = exit.view.stop_action === 'CANCEL' ? 'CANCELLED_RESERVED' : 'PAUSED'; exit.view.reason_codes = [];
      } else if (active(exit)) {
        if (blocked) { exit.view.reason_codes = [blocked]; }
        else if (exit.view.policy.kind !== 'CASH_ONLY' && Date.parse(exit.view.policy.deadline) <= Date.now()) {
          exit.view.state = 'CANCELLING_EXIT'; exit.view.stop_action = 'PAUSE'; event(exit, 'DEADLINE', '截止时间已到，停止新增减仓并核对原订单。'); bumpControl(); await save(); await applyControl(); await runtime.request('CANCEL_EXIT', { epoch: state.epoch, exit_id: exit.view.id });
        } else if (released !== null && released >= holding) {
          exit.view.state = exit.view.preparation_state === 'READY' ? 'READY' : 'WAITING_EVIDENCE'; exit.view.reason_codes = [];
        } else if (!exit.pendingSlice) {
          exit.view.state = exit.view.policy.kind === 'BOUNDED_LIMIT' ? 'REDUCING' : 'WAITING_CASH';
          exit.view.preparation_state = exit.view.policy.kind === 'BOUNDED_LIMIT' ? 'REDUCING' : 'WAITING_CASH';
        }
      }
      actions(exit);
      if (canonical(exit.view) !== original) {
        if (revisionBasis(exit.view) !== originalRevisionBasis) { exit.view.revision = String(BigInt(exit.view.revision) + 1n); exit.view.updated_at = now(); }
        changed = true;
      }
    }
    if (changed) await save();
    if (!allowSlice || reason() || Object.values(state.exits).some(exit => exit.pendingSlice)) return;
    const next = Object.values(state.exits).find(exit => active(exit) && exit.view.policy.kind === 'BOUNDED_LIMIT' && exit.view.state === 'REDUCING');
    if (next) await nextSlice(next);
  }
  async function nextSlice(exit: Exit) {
    if (exit.view.policy.kind !== 'BOUNDED_LIMIT' || !latest) return;
    const policy = exit.view.policy, elapsed = Date.now() - exit.lastSliceAt;
    if (elapsed < policy.pacing.minimum_interval_seconds * 1_000) return;
    const released = exit.view.funds.released_cash_amount;
    if (!released) return; const needed = maximum(0n, units(exit.view.funds.reserved_amount.amount) - units(released.amount)); if (needed === 0n) return;
    const costLeft = maximum(0n, units(policy.max_execution_cost.amount) - units(exit.executionCost));
    let lastReason = 'capital_authorized_quantity_exhausted';
    for (const progress of exit.view.progress) {
      if (units(progress.remaining_quantity) === 0n) continue;
      try {
        const native = instrument(progress.instrument_id); freshInstrument(native);
        const fee = feeRate(native); if (fee === null) fail('capital_fee_evidence_unavailable');
        const reference = exit.context.instruments.find(value => value.instrument_id === progress.instrument_id)?.bid_price;
        const leg = policy.legs.find(value => value.instrument_id === progress.instrument_id)!;
        const position = latest.positions.find(value => value.instrument_id === progress.instrument_id); if (!position) fail('capital_native_holdings_unavailable');
        if (latest.orders.some(order => order.instrument_id === progress.instrument_id && order.side.toUpperCase() === 'SELL' && !terminal(order.status))) fail('capital_protective_quantity_overlap');
        if (!reference) fail('capital_original_cost_reference_unavailable');
        const planned = planCapitalSlice({ instrument: native, needed: decimal(needed), maximumNotional: policy.pacing.maximum_slice_notional,
          participation: policy.pacing.maximum_participation, remainingQuantity: progress.remaining_quantity, availableQuantity: position.free_quantity,
          minimumPrice: leg.minimum_sell_price, referenceBid: reference, feeRate: fee, remainingCost: decimal(costLeft), currencyPrecision: latest.currency_precision });
        const slice: Slice = { id: `QZ-X-${randomUUID()}`, epoch: state.epoch, instrument: native.instrument_id, quantity: planned.quantity, price: planned.minimum_price, submittedAt: now() };
        exit.pendingSlice = slice; exit.childIds.push(slice.id); exit.sliceBasis[slice.id] = { reference_bid: reference, reference_evidence_id: exit.context.id, fee_rate: fee, instrument_id: slice.instrument, observed_filled_quantity: '0' };
        event(exit, 'SLICE_INTENT', '已保存本批原始身份，等待原生 IOC 结果。', slice.id); await save();
        try {
          const result = await runtime.request('SLICE', { epoch: slice.epoch, exit_id: exit.view.id, client_order_id: slice.id, instrument_id: slice.instrument, quantity: slice.quantity, minimum_price: slice.price,
            minimum_interval_seconds: policy.pacing.minimum_interval_seconds, deadline: policy.deadline,
            reference_bid: reference, remaining_cost_budget: decimal(costLeft), fee_rate_bound: fee,
            maximum_slice_notional: policy.pacing.maximum_slice_notional, maximum_participation: policy.pacing.maximum_participation });
          if (result.admission === 'REJECTED' && result.submitted === false) {
            exit.pendingSlice = null; exit.sliceBasis[slice.id]!.not_submitted = true; exit.view.reason_codes = [typeof result.reason === 'string' ? result.reason : 'capital_native_admission_rejected'];
            event(exit, 'SLICE_REJECTED_BEFORE_SUBMIT', '本批未提交到金融执行入口，保留原拒绝原因。', slice.id); await save();
          }
        }
        catch (error) { exit.view.reason_codes = [error instanceof Error ? error.message : 'capital_slice_outcome_uncertain']; await save(); }
        return;
      } catch (error) { lastReason = error instanceof Error ? error.message : 'capital_slice_unavailable'; }
    }
    if (exit.view.reason_codes[0] !== lastReason) { exit.view.reason_codes = [lastReason]; event(exit, 'WAITING_BOUND', '等待满足已授权价格、成本、持仓和批量界限。', lastReason); await save(); }
  }
  async function target(body: Json) {
    const targetId = text(body.id, 'target_id'), old = state.targets[targetId];
    if (old) { if (canonical(old.original) !== canonical(body)) fail('target_request_id_conflict'); if (old.receipt.status === 'PENDING') await settleTarget(targetId); return structuredClone(old.receipt); }
    if (reason()) fail(reason()!); const targetValue = record(body.target);
    if (Object.values(state.targets).some(value => value.receipt.status === 'PENDING')) fail('target_previous_original_requires_reconciliation');
    if (body.clientId !== options.clientId || body.accountId !== options.accountId) fail('account_identity_mismatch');
    let allocation: Json;
    if (body.allocation !== undefined) allocation = record(body.allocation);
    else {
      if (reserved() !== 0n || Object.values(state.exits).some(exit => obligation(exit) > 0n)
        || Object.values(state.commands).some(command => ['PENDING', 'UNCERTAIN'].includes(command.state))) fail('capital_snapshot_bound_target_allocation_required');
      const currentEquity = equity(); if (currentEquity === null) fail('capital_managed_equity_unavailable');
      // The ordinary initial-delivery protocol is a weight signal. With no
      // outstanding capital intent, derive its one-time native budget from the
      // current trusted account facts and persist that exact admission basis.
      allocation = { accountSourceId: state.sourceId, observationId: latest!.observation_id, snapshotSequence: latest!.sequence,
        nativeClockNs: String(latest!.native_snapshot?.ts_event ?? latest!.observed_at_ns), controlRevision: state.revision,
        reserved: '0', cashBuffer: decimal(cashBuffer()), capital: decimal(maximum(0n, units(currentEquity) - cashBuffer())), origin: 'CURRENT_NATIVE_ACCOUNT' };
    }
    for (const [key, expected] of Object.entries({ accountSourceId: state.sourceId, observationId: latest!.observation_id, snapshotSequence: latest!.sequence, nativeClockNs: String(latest!.native_snapshot?.ts_event ?? latest!.observed_at_ns), controlRevision: state.revision })) if (allocation[key] !== expected) fail('account_snapshot_changed');
    if (units(allocation.reserved, 'target_reserved') !== reserved()) fail('account_reservation_changed');
    const expires = text(targetValue.valid_until_ns, 'target_expiry'); if (BigInt(expires) <= BigInt(Date.now()) * 1_000_000n) fail('target_expired');
    const capital = units(allocation.capital, 'target_capital'), accountEquity = equity();
    if (accountEquity === null || capital > maximum(0n, units(accountEquity) - reserved() - units(allocation.cashBuffer ?? '0'))) fail('target_exceeds_unreserved_capital');
    const control = allocation.capitalExit ? record(allocation.capitalExit) : null;
    const exits = Object.values(state.exits).filter(exit => obligation(exit) > 0n);
    if (exits.length && (!control || control.controlRevision !== state.revision || !Array.isArray(control.exitIds)
      || canonical([...control.exitIds].sort()) !== canonical(exits.map(exit => exit.view.id).sort()))) fail('capital_target_control_scope_mismatch');
    const reservationProtected = reserved() > 0n;
    if (reservationProtected && (!control?.signalTarget || typeof control.signalCapital !== 'string')) fail('capital_original_signal_budget_required');
    const signal = reservationProtected ? record(control!.signalTarget) : targetValue;
    const signalCapital = reservationProtected ? units(control!.signalCapital, 'capital_original_signal_capital') : capital;
    if (signalCapital < 0n || signalCapital > units(accountEquity)) fail('capital_original_signal_capital_exceeds_managed_capital');
    if (signal.valid_until_ns !== targetValue.valid_until_ns) fail('capital_signal_target_expiry_mismatch');
    const rows = Array.isArray(signal.targets) ? signal.targets : Array.isArray(signal.weights) ? signal.weights : null; if (!rows) fail('target_weights_required');
    const receipt: Json = { id: targetId, clientId: options.clientId, accountId: options.accountId, expiresAt: body.expiresAt, externalId: `native:${targetId}`, status: 'PENDING', acknowledgedAt: null };
    state.targets[targetId] = { original: body, receipt, epoch: state.epoch, budget_basis: structuredClone(allocation) }; await save();
    try {
      await applyControl();
      const applied = await runtime.request('TARGET', { epoch: state.epoch, id: targetId, expires_at: expires, capital: decimal(capital), signal_capital: decimal(signalCapital), reserved_amount: decimal(reserved()),
        weights: rows.map(value => { const row = record(value); return { instrument_id: row.instrument_id ?? row.instrumentId, weight: row.weight }; }) });
      if (applied.acknowledged !== true || applied.target_id !== targetId || applied.epoch !== state.epoch) fail(typeof applied.reason === 'string' ? applied.reason : 'target_native_acceptance_unconfirmed');
      receipt.status = 'ACKNOWLEDGED'; receipt.acknowledgedAt = now();
      for (const exit of Object.values(state.exits)) if (obligation(exit) > 0n) exit.view.remaining_trading = 'BUDGET_AWARE';
    } catch (error) { receipt.error = error instanceof Error ? error.message : 'target_outcome_uncertain'; }
    await save(); return receipt;
  }
  async function settleTarget(targetId: string) {
    const target = state.targets[targetId]; if (!target || target.receipt.status !== 'PENDING') return;
    try {
      const result = await runtime.request('TARGET_STATUS', { id: targetId, epoch: target.epoch, expires_at: record(target.original.target).valid_until_ns });
      if (result.acknowledged === true && result.target_id === targetId && result.epoch === target.epoch) {
        target.receipt.status = result.revoked === true ? 'REVOKED' : 'ACKNOWLEDGED'; target.receipt.acknowledgedAt ??= now(); delete target.receipt.error;
        if (state.epoch === target.epoch && result.revoked !== true) for (const exit of Object.values(state.exits)) if (obligation(exit) > 0n) exit.view.remaining_trading = 'BUDGET_AWARE';
        await save();
      } else if (result.acknowledged === false && result.target_id === targetId && result.admission === 'NOT_SUBMITTED'
        && (result.epoch === target.epoch || result.epoch === null && result.recovery === 'OFFICIAL_CACHE_AND_RECONCILIATION')) {
        // A native proof that no child reached submit_order is a final negative
        // receipt. Reading it never re-executes the expired original intent.
        const expiry = record(target.original.target).valid_until_ns;
        const expired = typeof expiry === 'string' && BigInt(expiry) <= BigInt(Date.now()) * 1_000_000n;
        target.receipt.status = result.state === 'EXPIRED' || expired ? 'EXPIRED' : 'REJECTED';
        target.receipt.error = typeof result.reason === 'string' ? result.reason : 'target_native_confirmed_not_submitted';
        target.receipt.acknowledgedAt = null; await save();
      }
    } catch (error) { target.receipt.error = error instanceof Error ? error.message : 'target_original_receipt_unavailable'; }
  }
  async function admitTarget(body: Json) {
    try { return await target(body); }
    catch (error) {
      const targetId = text(body.id, 'target_id');
      if (!state.targets[targetId] && !writeFailed) {
        const receipt = { id: targetId, clientId: options.clientId, accountId: options.accountId, expiresAt: body.expiresAt,
          externalId: null, acknowledgedAt: null, status: 'REJECTED', error: error instanceof Error ? error.message : 'target_admission_rejected' };
        state.targets[targetId] = { original: body, receipt, epoch: state.epoch }; await save(); return receipt;
      }
      throw error;
    }
  }
  function status() {
    const blocked = reason(); for (const exit of Object.values(state.exits)) actions(exit);
    return { ...envelope(), accountSourceId: state.sourceId, observationId: latest?.observation_id ?? null, controlRevision: state.revision,
      reserved: decimal(reserved()), blocked: !!blocked, reason: blocked, exits: Object.values(state.exits).map(exit => structuredClone(exit.view)),
      commands: Object.entries(state.commands).map(([requestId, value]) => ({ requestId, state: value.state, error: value.error })) };
  }
  function account() {
    if (failure || state.epoch !== '1' && appliedEpoch !== state.epoch || !latest?.native_snapshot || !latest.ready || latest.recovery_required || typeof latest.native_version !== 'string') fail(reason() ?? 'account_native_observation_unavailable', 503);
    return { ...envelope(), accountSourceId: state.sourceId, observationId: latest.observation_id, controlRevision: state.revision,
      sequence: latest.sequence, nativeClockNs: latest.native_snapshot.ts_event, equity: equity(), cash: spotCash(),
      positions: latest.positions.map(position => ({ instrumentId: position.instrument_id, quantity: position.quantity, value: position.value, pnl: position.pnl })),
      nativeObservation: { schema_version: 1, binding: { schema_version: 1, project_id: state.sourceId, environment: options.mode === 'paper' ? 'PAPER' : 'LIVE',
        native_trader_id: latest.native_trader_id, native_session_id: latest.native_session_id, native_account_id: latest.native_account_id, native_version: latest.native_version },
        sequence: latest.sequence, dropped_events: '0', observed_at_ns: latest.observed_at_ns, connection: 'CONNECTED', snapshot: latest.native_snapshot },
      openOrders: { schema_version: 1, account_id: latest.native_account_id, asof_ns: latest.native_snapshot.ts_event, available_ns: latest.observed_at_ns, count: latest.orders.filter(order => !terminal(order.status)).length } };
  }
  let backgroundQueued = false;
  function background(allowSlice: boolean) {
    if (stopped || backgroundQueued) return; backgroundQueued = true;
    // Market/venue I/O is read-only and runs outside the command mutation lane,
    // so a slow Info endpoint cannot queue a stop command behind network polling.
    void refresh().then(() => stopped ? undefined : serial(() => advance(allowSlice))).catch(error => { failure = error instanceof Error ? error.message : 'capital_account_progress_unavailable'; }).finally(() => { backgroundQueued = false; });
  }
  const unsubscribe = runtime.subscribeTicks?.(() => background(true));
  // This timer refreshes read projections and reconciles existing commands.
  // Only the official owner clock notification may propose a new financial slice.
  const interval = setInterval(() => background(false), 1_000); interval.unref();
  background(false);
  return {
    async handle(path: string, request: IncomingMessage, readBody: () => Promise<unknown>): Promise<{ status: number; value: unknown }> {
      if (request.headers['x-qz-client-id'] !== options.clientId) fail('account_client_identity_mismatch', 403);
      const body = request.method === 'POST' ? record(await readBody()) : null;
      if (body && (body.clientId !== options.clientId || body.accountId !== options.accountId)) fail('account_identity_mismatch');
      const actionKind = body?.command ? record(body.command).action : null;
      if (request.method === 'GET' && ['/account', '/capital-exits/planning-context'].includes(path)
        || request.method === 'POST' && (['/capital-exits/preview', '/capital-exits', '/targets'].includes(path) || actionKind === 'RESUME')) await refresh();
      return serial(async () => {
        if (request.method === 'GET' && path === '/capabilities') return { status: 200, value: { ...envelope(),
          capitalExit: { version: 2, supported: !reason(), readable: true, reason: reason(), policies: ['CASH_ONLY', 'NATURAL_RELEASE', 'BOUNDED_LIMIT'], actions: ['START', 'PAUSE', 'CANCEL', 'RESUME', 'RELEASE_RESERVATION', 'RECONCILE_EXTERNAL_TRANSFER', 'RECONCILE_WITHDRAWAL'] },
          automaticTargets: { version: 1, supported: !reason(), revisionBound: true, snapshotBound: true } } };
        if (request.method === 'GET' && path === '/account') return { status: 200, value: account() };
        if (request.method === 'GET' && path === '/capital-exits') return { status: 200, value: status() };
        if (request.method === 'GET' && path === '/capital-exits/planning-context') return { status: 200, value: { ...envelope(), context: planningContext() } };
        const receipt = /^\/capital-exits\/commands\/([^/]+)$/.exec(path);
        if (request.method === 'GET' && receipt) return { status: 200, value: commandReceipt(decodeURIComponent(receipt[1]!)) };
        const exitPath = /^\/capital-exits\/([^/]+)$/.exec(path);
        if (request.method === 'GET' && exitPath) { const exit = state.exits[decodeURIComponent(exitPath[1]!)]; if (!exit) fail('capital_exit_not_found', 404); actions(exit); return { status: 200, value: { ...envelope(), exit: exit.view } }; }
        const targetPath = /^\/targets\/([^/]+)$/.exec(path);
        if (request.method === 'GET' && targetPath) { const targetId = decodeURIComponent(targetPath[1]!), stored = state.targets[targetId]; if (!stored) fail('target_receipt_not_found', 404); await settleTarget(targetId); return { status: 200, value: structuredClone(stored.receipt) }; }
        if (request.method !== 'POST') return { status: 404, value: { error: 'not_found' } };
        if (path === '/capital-exits/preview') return { status: 200, value: await preview(body!) };
        if (path === '/capital-exits') return { status: 202, value: await admitCommand(body!) };
        const action = /^\/capital-exits\/([^/]+)\/actions$/.exec(path);
        if (action) return { status: 202, value: await admitCommand(body!, decodeURIComponent(action[1]!)) };
        if (path === '/targets') return { status: 202, value: await admitTarget(body!) };
        return { status: 404, value: { error: 'not_found' } };
      });
    },
    async close() { stopped = true; clearInterval(interval); unsubscribe?.(); await queue; await runtime.close(); await save(); },
  };
}
