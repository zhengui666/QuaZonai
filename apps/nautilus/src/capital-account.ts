import { accountStrategyCommand, nativeAccountStrategyConfiguration } from '../../../packages/contracts/src/account-strategy.ts';
import { nativeFixedRiskSizing } from '../../../packages/contracts/src/nautilus.ts';
import { randomBytes } from 'node:crypto';
import { mkdir, open, readFile, rename } from 'node:fs/promises';
import { join } from 'node:path';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { AccountRuntime } from './account-runtime.ts';
import type { AccountOptions, NativeAccountRuntime, NativeAccountSnapshot } from './account-runtime.ts';
import { HyperliquidAccountReader, units } from './hyperliquid-account.ts';
import type { HyperliquidAccountProof } from './hyperliquid-account.ts';
import { validateCapitalDocument } from '../../../packages/contracts/src/capital-exit.ts';
import type { CapitalExitView, CapitalExitPreview, CapitalExitPlanningContext } from '../../../packages/contracts/src/capital-exit.ts';

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
  strategyRevision?: string;
  strategyConfiguration?: AccountOptions['nativeStrategy'];
  strategyCommands?: Record<string, { original: Json; receipt: Json }>;
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
// This is a per-field safety predicate, not a calculated account balance.
const hasObligation = (exit: Exit) => ['reserved_amount', 'staged_for_withdrawal_amount'].some(key => {
  const value = (exit.view.funds as unknown as Json)?.[key] as {amount?: unknown;currency?:unknown}|undefined;
  if (value?.currency !== 'USDC' || typeof value.amount !== 'string') return true;
  try { return units(value.amount) !== 0n; } catch { return true; }
});

/** Capital metadata stays beside existing worker receipts, never a second OMS. */
export async function createCapitalAccount(options: AccountOptions, directory: string, binary: string,
  suppliedRuntime?: NativeAccountRuntime, suppliedReader?: Pick<HyperliquidAccountReader, 'account' | 'ledger'>,
  syncDirectory = async (directory: string) => { const root = await open(directory, 'r'); try { await root.sync(); } finally { await root.close(); } }) {
  options = { ...options };
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
  state.strategyRevision ??= '1';
  if (!/^\d+$/.test(state.strategyRevision)) fail('strategy_revision_invalid');
  let writeFailed = false;
  async function save() {
    if (writeFailed) fail('account_receipts_write_failed');
    let renamed = false;
    try {
      const file = await open(statePath + '.next', 'w', 0o600);
      try { await file.writeFile(JSON.stringify(state)); await file.sync(); } finally { await file.close(); }
      await rename(statePath + '.next', statePath); renamed = true; await syncDirectory(directory);
    } catch (error) { writeFailed = true; throw Object.assign(error instanceof Error ? error : new Error('account_receipts_write_failed'), { accountWriteRenamed: renamed }); }
  }
  // Opening retained records never reinterprets their original control epoch.
  await save();
  if (state.strategyConfiguration) options.nativeStrategy = nativeAccountStrategyConfiguration(state.strategyConfiguration);
  const runtime = suppliedRuntime ?? new AccountRuntime(binary, options, state.sourceId, directory);
  const reader = options.mode === 'live' ? suppliedReader ?? new HyperliquidAccountReader(options.accountAddress!, options.network) : null;
  let latest: NativeAccountSnapshot | null = null, proof: HyperliquidAccountProof | null = null, failure: string | null = 'account_runtime_not_started';
  let stopped = false;
  let pendingReverse = state.reverseTransferPending ?? false;
  let refreshInFlight: Promise<void> | null = null;
  let queue: Promise<unknown> = Promise.resolve();
  function serial<T>(operation: () => Promise<T>): Promise<T> { const result = queue.then(operation); queue = result.catch(() => {}); return result; }
  const envelope = () => ({ clientId: options.clientId, accountId: options.accountId, mode: options.mode, currency: 'USDC' });
  function reason(): string | null {
    if (writeFailed) return 'account_receipts_write_failed';
    if (pendingReverse || state.reverseTransferPending) return 'capital_external_reverse_transfer_requires_reconciliation';
    if (failure) return failure;
    if (!latest?.ready || latest.recovery_required) return latest?.reason ?? 'account_native_recovery_required';
    if (reader && (!proof || Date.parse(proof.valid_until) <= Date.now())) return 'hyperliquid_spot_evidence_unavailable';
    return proof?.reason_codes[0] ?? null;
  }
  function equity(): string | null { return latest?.equity ?? null; }
  function event(exit: Exit, kind: string, message: string, reference: string | null = null) {
    exit.view.revision = String(BigInt(exit.view.revision) + 1n); exit.view.updated_at = now();
    exit.view.events.push({ id: id(), at: exit.view.updated_at, kind, message, reference });
  }



  function actions(exit: Exit) {
    exit.view.action_availability = Object.fromEntries(['PAUSE','CANCEL','RESUME','RELEASE_RESERVATION','RECONCILE_EXTERNAL_TRANSFER','RECONCILE_WITHDRAWAL'].map(action=>[action,{allowed:false,reason:['PAUSE','CANCEL'].includes(action)?'capital_original_control_evidence_unavailable':'capital_exit_planning_retired'}])) as CapitalExitView['action_availability'];
  }
  async function readNativeSnapshot(): Promise<NativeAccountSnapshot> {
    const value = await runtime.request('SNAPSHOT');
    if (!Array.isArray(value.orders) || !Array.isArray(value.positions) || !Array.isArray(value.instruments) || !Array.isArray(value.fills)
      || typeof value.sequence !== 'string' || typeof value.observation_id !== 'string') fail('account_native_snapshot_invalid');
    return value as unknown as NativeAccountSnapshot;
  }
  async function applyControl() {
    // Only a fresh, explicitly requested START with individually proven zero
    // obligations may establish this new epoch. This is not old CONTROL replay.
    if (Object.values(state.exits).some(exit => !['COMPLETED','CLOSED'].includes(exit.view.state) || hasObligation(exit))
      || state.reverseTransferPending || Object.values(state.commands).some(command => ['PENDING','UNCERTAIN'].includes(command.state))) fail('capital_original_control_evidence_unavailable');
    const result = await runtime.request('CONTROL', { epoch: state.epoch, revision: state.revision, reserved_amount: '0', cash_buffer: options.cashBuffer ?? '0',
      mode: 'NORMAL', exit_id: null, exit_ids: [], minimum_interval_seconds: null, deadline: null,
      taker_fee_rate: reader ? proof?.spot_taker_fee_rate ?? null : options.paperFeeRate ?? null,
      spot_free_cash: reader ? proof?.spot_free ?? null : latest?.cash ?? null, spot_total_cash: reader ? proof?.spot_total ?? null : latest?.cash ?? null,
      spot_balances: reader ? proof?.balances ?? null : null,
      spot_facts_asof_ns: reader && proof ? String(BigInt(Date.parse(proof.asof)) * 1_000_000n) : latest?.observed_at_ns ?? null,
      spot_facts_valid_until_ns: reader && proof ? String(BigInt(Date.parse(proof.valid_until)) * 1_000_000n) : latest ? String(BigInt(latest.observed_at_ns) + 5_000_000_000n) : null });
    if (result.applied !== true) fail(typeof result.reason === 'string' ? result.reason : 'capital_native_fence_not_confirmed');
    // The official owner produces a new observation when control or spot
    // economic values change. Read it before publishing the corresponding
    // /account projection; TypeScript never manufactures a native sequence.
    latest = await readNativeSnapshot();
  }



  async function refresh(): Promise<void> {
    if(refreshInFlight)return refreshInFlight;
    refreshInFlight=(async()=>{
      try{
        const frame=await readNativeSnapshot();
        const incoming=reader?await reader.account():null;
        await serial(async()=>{if(stopped)return;latest=frame;proof=incoming;failure=null;});
      }catch(error){await serial(async()=>{failure=error instanceof Error?error.message:'account_native_status_unavailable';});}
    })().finally(()=>{refreshInFlight=null;});
    return refreshInFlight;
  }
  async function command(body: Json, exitId?: string) {
    const requestId=text(body.requestId,'request_id'),original={...body,...(exitId?{exitId}:{})};
    const previous=state.commands[requestId];
    if(previous){if(canonical(previous.original)!==canonical(original))fail('request_id_conflict');return commandReceipt(requestId);}
    const native=record(body.command);
    if(!exitId||!['PAUSE','CANCEL'].includes(String(native.action)))fail('capital_exit_planning_retired',410);
    validateCapitalDocument('ACTION',native);
    const exit=state.exits[exitId];if(!exit)fail('capital_exit_not_found',404);
    if(native.expected_revision!==exit.view.revision)fail('capital_exit_revision_conflict');
    fail('capital_original_control_evidence_unavailable', 409);
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
        try { await save(); }
        catch { state.commands[requestId]!.state = 'UNCERTAIN'; state.commands[requestId]!.error = 'account_receipts_write_failed'; }
        return commandReceipt(requestId);
      }
      throw error;
    }
  }
  async function advance() {
    if(!latest?.ready||latest.recovery_required)return;
    // Restoring observations never retries an old cancellation or pushes control.
    // Only a new explicit cancellation command may send CANCEL_EXIT.
    for(const [targetId,target] of Object.entries(state.targets))if(target.receipt.status==='PENDING')await settleTarget(targetId);
    let changed=false;
    for(const exit of Object.values(state.exits)){
      // Absence from a snapshot is not evidence of cancellation.
      const ids=[...new Set([...exit.childIds,...(exit.pendingSlice?[exit.pendingSlice.id]:[])])];
      const settled=ids.every(id=>latest!.orders.some(order=>order.client_order_id===id&&terminal(order.status)));
      if(exit.view.state==='CANCELLING_EXIT'&&settled){
        exit.pendingSlice=null;exit.view.state=exit.view.stop_action==='CANCEL'?'CANCELLED_RESERVED':'PAUSED';
        event(exit,'ORDERS_SETTLED','原订单均有官方终态；预留金额保持不变。');changed=true;
      }
    }
    if(changed)await save();
  }

  async function settleTarget(targetId: string) {
    const target = state.targets[targetId]; if (!target || target.receipt.status !== 'PENDING') return;
    try {
      const result = await runtime.request('TARGET_STATUS', { id: targetId, epoch: target.epoch, expires_at: record(target.original.target).valid_until_ns });
      if (result.acknowledged === true && result.target_id === targetId && result.epoch === target.epoch) {
        target.receipt.status = result.revoked === true ? 'REVOKED' : 'ACKNOWLEDGED'; target.receipt.acknowledgedAt ??= now(); delete target.receipt.error;
        if (result.native_protection != null) target.receipt.nativeProtection = result.native_protection;
        if (state.epoch === target.epoch && result.revoked !== true) for (const exit of Object.values(state.exits)) if (hasObligation(exit)) exit.view.remaining_trading = 'BUDGET_AWARE';
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
  function strategyConfiguration() {
    return { ...envelope(), accountSourceId: state.sourceId, strategyRevision: state.strategyRevision,
      configuration: structuredClone(options.nativeStrategy), connected: runtime.isConnected?.() ?? null,
      pendingConfigurationRequests: Object.entries(state.strategyCommands ?? {}).filter(([, value]) => value.original.action === 'CONFIGURE' && value.receipt.status !== 'COMPLETED').map(([requestId]) => requestId),
      runnerValidated: false, note: 'Configuration only. REGISTERED names require a user-built account runner; saving never connects or starts trading.' };
  }
  async function verifyConfiguration(requestId: string) {
    const command = state.strategyCommands?.[requestId];
    if (!command || command.original.action !== 'CONFIGURE') fail('account_configuration_request_not_found', 404);
    if (command.receipt.status === 'COMPLETED') return structuredClone(command.receipt);
    if (runtime.isConnected?.() !== false) fail('account_strategy_configuration_requires_disconnected_runtime');
    // Explicitly inspect the original persisted request, not today's edited form.
    const persisted = JSON.parse(await readFile(statePath, 'utf8')) as AccountState;
    const saved = persisted.strategyCommands?.[requestId];
    if (!saved || canonical(saved.original) !== canonical(command.original)
      || canonical(persisted.strategyConfiguration) !== canonical(command.original.configuration)
      || persisted.strategyRevision !== command.receipt.strategyRevision) fail('account_configuration_original_persistence_mismatch');
    const withoutReceipt = (value: AccountState) => { const copy = structuredClone(value); delete copy.strategyCommands?.[requestId]; return copy; };
    if (canonical(withoutReceipt(persisted)) !== canonical(withoutReceipt(state))) fail('account_configuration_other_facts_changed');
    // Do not clear the failed-write latch until the actual existing file and its
    // directory are synced. This does not connect or load a native strategy.
    const file = await open(statePath, 'r');
    try { await file.sync(); } finally { await file.close(); }
    await syncDirectory(directory);
    writeFailed = false;
    command.receipt.status = 'COMPLETED'; delete command.receipt.persisted; delete command.receipt.error;
    try { await save(); } catch {
      command.receipt.status = 'UNCERTAIN'; command.receipt.persisted = false; command.receipt.error = 'account_configuration_persistence_uncertain';
    }
    return structuredClone(command.receipt);
  }
  async function configureStrategy(body: Json) {
    const requestId = text(body.requestId, 'request_id');
    if (Object.keys(body).some(key => !['requestId','clientId','accountId','expectedAccountSourceId','expectedStrategyRevision','configuration'].includes(key))) fail('account_strategy_config_invalid', 422);
    const original = { ...body, action: 'CONFIGURE' };
    const commands = state.strategyCommands ??= {};
    const prior = commands[requestId];
    if (prior) { if (canonical(prior.original) !== canonical(original)) fail('strategy_request_id_conflict'); return structuredClone(prior.receipt); }
    if (runtime.isConnected?.() !== false) fail('account_strategy_configuration_requires_disconnected_runtime');
    if (body.expectedAccountSourceId !== state.sourceId || body.expectedStrategyRevision !== state.strategyRevision) fail('strategy_start_fence_changed');
    const configuration = nativeAccountStrategyConfiguration(body.configuration);
    if (!options.instruments.includes(configuration.instrument_id)) fail('account_strategy_instrument_not_configured');
    const nextRevision = String(BigInt(state.strategyRevision!) + 1n);
    const receipt: Json = { ...envelope(), requestId, action: 'CONFIGURE', status: 'UNCERTAIN', accountSourceId: state.sourceId, strategyRevision: nextRevision,
      configuration, connected: false, runnerValidated: false };
    const previousConfiguration = state.strategyConfiguration;
    const previousRevision = state.strategyRevision;
    state.strategyConfiguration = configuration; state.strategyRevision = nextRevision;
    commands[requestId] = { original: structuredClone(original), receipt: structuredClone(receipt) };
    try { await save(); } catch (error) {
      if (!(error as { accountWriteRenamed?: boolean }).accountWriteRenamed) {
        state.strategyConfiguration = previousConfiguration; state.strategyRevision = previousRevision;
        delete commands[requestId]; throw error;
      }
      // The new file exists but directory durability is unknown. Preserve the
      // original request and new configuration; never pretend it did not occur.
      receipt.persisted = false; receipt.error = 'account_configuration_persistence_uncertain';
      commands[requestId]!.receipt = structuredClone(receipt);
      options.nativeStrategy = configuration;
      return structuredClone(receipt);
    }
    options.nativeStrategy = configuration;
    // Configuration is durable before acknowledging completion. If this second
    // receipt write fails, original identity resolves it; no trading is allowed.
    receipt.status = 'COMPLETED'; commands[requestId]!.receipt = structuredClone(receipt);
    try { await save(); } catch {
      receipt.status = 'UNCERTAIN'; receipt.persisted = false; receipt.error = 'account_configuration_persistence_uncertain';
      commands[requestId]!.receipt = structuredClone(receipt);
    }
    return structuredClone(receipt);
  }
  async function strategyCommand(body: Json) {
    if (Object.keys(body).some(key => !['requestId', 'action', 'clientId', 'accountId', 'expectedAccountSourceId', 'expectedStrategyRevision'].includes(key))) fail('strategy_command_invalid', 422);
    const command = accountStrategyCommand({ requestId: body.requestId, action: body.action, ...(body.action === 'START' || body.action === 'RECOVER' ? { expectedAccountSourceId: body.expectedAccountSourceId, expectedStrategyRevision: body.expectedStrategyRevision } : {}) });
    const commands = state.strategyCommands ??= {};
    const prior = commands[command.requestId];
    if (prior) { if (canonical(prior.original) !== canonical(body)) fail('strategy_request_id_conflict'); return structuredClone(prior.receipt); }
    if (command.action === 'START') {
      if (Object.values(state.strategyCommands ?? {}).some(value => value.original.action === 'CONFIGURE' && value.receipt.status !== 'COMPLETED')) fail('account_configuration_save_unsettled');
      if(Object.values(state.commands).some(command=>['PENDING','UNCERTAIN'].includes(command.state)))fail('capital_original_command_unsettled');
      if (writeFailed) fail('account_receipts_write_failed');
      if (pendingReverse || state.reverseTransferPending) fail('capital_external_reverse_transfer_requires_reconciliation');
      if (Object.values(state.exits).some(exit => hasObligation(exit) || !['COMPLETED', 'CLOSED'].includes(exit.view.state))) fail('strategy_account_exit_unsettled');
      if (Object.values(state.targets).some(target => target.receipt.status === 'PENDING')) fail('strategy_original_target_unsettled');
    }
    if (command.action === 'START' || command.action === 'RECOVER') {
      if (typeof body.expectedAccountSourceId !== 'string' || typeof body.expectedStrategyRevision !== 'string' || !/^\d+$/.test(body.expectedStrategyRevision)) fail('strategy_start_fence_required', 422);
      if (body.expectedAccountSourceId !== state.sourceId || body.expectedStrategyRevision !== state.strategyRevision) {
        const receipt: Json = { ...envelope(), requestId: command.requestId, action: command.action, status: 'REJECTED', result: { admission: 'REJECTED', started: false, reason: 'strategy_start_fence_changed' }, error: 'strategy_start_fence_changed' };
        commands[command.requestId] = { original: structuredClone(body), receipt }; await save(); return structuredClone(receipt);
      }
    }
    // The Info payload only provides total/hold, not an authoritative native
    // spot-free value. Never synthesize it or substitute merged account cash.
    // This known unsupported capability is rejected before any BOOT or trading.
    if (command.action === 'START' && options.mode === 'live') {
      const receipt: Json = { ...envelope(), requestId: command.requestId, action: command.action, status: 'REJECTED',
        result: { admission: 'REJECTED', started: false, reason: 'hyperliquid_native_spot_free_required' }, error: 'hyperliquid_native_spot_free_required' };
      commands[command.requestId] = { original: structuredClone(body), receipt };
      try { await save(); } catch {
        receipt.status = 'UNCERTAIN'; receipt.result = null; receipt.persisted = false;
        receipt.error = 'account_receipts_write_failed';
      }
      return structuredClone(receipt);
    }
    // Worker-owned, persisted CAS fence; never infer order from Pi Task IDs.
    state.strategyRevision = String(BigInt(state.strategyRevision!) + 1n);
    if (command.action === 'START') { state.epoch = String(BigInt(state.epoch) + 1n); state.revision = String(BigInt(state.revision) + 1n); }
    const receipt: Json = { ...envelope(), requestId: command.requestId, action: command.action, status: 'UNCERTAIN', result: null, error: null };
    commands[command.requestId] = { original: structuredClone(body), receipt };
    // START must have a durable intent. An explicitly requested STOP must remain
    // available when receipt storage fails; it is never advertised as durable.
    let persisted = true;
    try { await save(); } catch (error) {
      if (command.action !== 'STOP') throw error;
      persisted = false;
    }
    const unrecorded = () => {
      receipt.status = 'UNCERTAIN'; receipt.persisted = false;
      receipt.persistenceError = 'account_receipts_write_failed';
      receipt.error ??= 'strategy_outcome_not_durably_recorded';
      return structuredClone(receipt);
    };
    try {
      if (command.action === 'RECOVER') {
        // Explicit recovery may connect despite unresolved obligations, but must
        // not send CONTROL, start a strategy, cancel or resubmit old orders.
        await runtime.start?.();
        latest = await readNativeSnapshot();
        failure = null;
        const recovered = await runtime.request('RECOVERY_STATUS');
        if (recovered.recovery_only !== true || typeof recovered.native_version !== 'string' || typeof recovered.native_session_id !== 'string') fail('account_native_recovery_result_invalid');
        receipt.result = recovered;
        receipt.status = 'COMPLETED';
        await save();
        return structuredClone(receipt);
      }
      if (command.action === 'START') {
        // Only this explicit, fenced and durably recorded START may connect the
        // configured native account for trading. BOOT itself only restores observations.
        await runtime.start?.();
        latest = await readNativeSnapshot();
        proof = reader ? await reader.account() : null;
        failure = null;
        if (reason()) fail(reason()!);
        await applyControl();
      }
      const result = await runtime.request(command.action === 'START' ? 'STRATEGY_START' : 'STRATEGY_STOP', { epoch: state.epoch });
      if (result.admission === 'REJECTED' && result.started === false && command.action === 'START' && typeof result.reason === 'string') {
        receipt.result = result; receipt.status = 'REJECTED'; receipt.error = result.reason;
        try { await save(); } catch { return unrecorded(); }
        return structuredClone(receipt);
      }
      if (typeof result.strategy_id !== 'string' || typeof result.state !== 'string' || typeof result.native_version !== 'string') fail('strategy_native_result_invalid');
      receipt.result = result;
      receipt.status = 'COMPLETED';
    } catch (error) { receipt.error = error instanceof Error ? error.message : 'strategy_native_outcome_uncertain'; }
    if (!persisted) return unrecorded();
    try { await save(); } catch { return unrecorded(); }
    return structuredClone(receipt);
  }
  function status() {
    const blocked = reason(); for (const exit of Object.values(state.exits)) actions(exit);
    return { ...envelope(), accountSourceId: state.sourceId, observationId: latest?.observation_id ?? null, controlRevision: state.revision,
      reserved: null, blocked: !!blocked, reason: blocked, exits: Object.values(state.exits).map(exit => structuredClone(exit.view)),
      commands: Object.entries(state.commands).map(([requestId, value]) => ({ requestId, state: value.state, error: value.error })) };
  }
  function account() {
    if (failure || !latest?.native_snapshot || !latest.ready || latest.recovery_required || typeof latest.native_version !== 'string') fail(reason() ?? 'account_native_observation_unavailable', 503);
    return { ...envelope(), accountSourceId: state.sourceId, observationId: latest.observation_id, controlRevision: state.revision,
      sequence: latest.sequence, nativeClockNs: latest.native_snapshot.ts_event, equity: equity(), cash: latest.cash ?? null,
      positions: latest.positions.map(position => ({ instrumentId: position.instrument_id, quantity: position.quantity, value: position.value, pnl: position.pnl })),
      nativeObservation: { schema_version: 1, binding: { schema_version: 1, project_id: state.sourceId, environment: options.mode === 'paper' ? 'PAPER' : 'LIVE',
        native_trader_id: latest.native_trader_id, native_session_id: latest.native_session_id, native_account_id: latest.native_account_id, native_version: latest.native_version },
        sequence: latest.sequence, dropped_events: '0', observed_at_ns: latest.observed_at_ns, connection: 'CONNECTED', snapshot: latest.native_snapshot },
      openOrders: { schema_version: 1, account_id: latest.native_account_id, asof_ns: latest.native_snapshot.ts_event, available_ns: latest.observed_at_ns, count: latest.orders.filter(order => !terminal(order.status)).length } };
  }
  let backgroundQueued = false;
  function background() {
    if (stopped || backgroundQueued) return; backgroundQueued = true;
    // Market/venue I/O is read-only and runs outside the command mutation lane,
    // so a slow Info endpoint cannot queue a stop command behind network polling.
    void refresh().then(() => stopped ? undefined : serial(() => advance())).catch(error => { failure = error instanceof Error ? error.message : 'capital_account_progress_unavailable'; }).finally(() => { backgroundQueued = false; });
  }
  const unsubscribe = runtime.subscribeTicks?.(() => background());
  // This timer refreshes read projections and reconciles existing commands.
  // Owner clock notifications only refresh original observations; financial slicing is retired.
  const interval = setInterval(() => background(), 1_000); interval.unref();
  background();
  async function dispatch(path: string, method: 'GET' | 'POST', clientId: unknown, input?: unknown): Promise<{ status: number; value: unknown }> {
      if (clientId !== options.clientId) fail('account_client_identity_mismatch', 403);
      const body = method === 'POST' ? record(input) : null;
      if (body && (body.clientId !== options.clientId || body.accountId !== options.accountId)) fail('account_identity_mismatch');
      if (method === 'GET' && ['/account'].includes(path)
        || method === 'POST' && (path === '/strategy/commands' && body?.action === 'START')) await refresh();
      return serial(async () => {
        if (method === 'GET' && path === '/capabilities') return { status: 200, value: { ...envelope(),
          recovery: { supported: true, action: 'RECOVER', automaticConnection: false, automaticStrategyStart: false },
          capitalExit: { version: 2, supported: false, readable: true, reason: 'capital_original_control_evidence_unavailable', policies: [], actions: [] },
          orderPreview: { version: 1, supported: true, previewOnly: true, automaticProtectionEnabled: false, kinds: ['STOP_LOSS', 'TAKE_PROFIT', 'BRACKET'] },
          nativeBrackets: { supported: false, reason: options.mode === 'paper' ? 'account_native_paper_bracket_recovery_unverified' : 'account_native_live_brackets_not_supported', defaultEnabled: false, scope: 'NEW_LONG_POSITION', cancellationPath: '/targets/<id>/cancel-protection', atomicOco: false },
          nativeStrategy: { version: 1, supported: options.nativeStrategy.strategy.kind === 'EMA_CROSS' && (!failure || failure === 'account_runtime_not_started'),
            configuredKind: options.nativeStrategy.strategy.kind, supportedKinds: ['EMA_CROSS'],
            reason: options.nativeStrategy.strategy.kind === 'REGISTERED' ? 'account_registered_runner_support_unverified' : options.nativeStrategy.strategy.kind !== 'EMA_CROSS' ? 'account_native_strategy_account_lifecycle_unsupported_for_orderbook_imbalance' : failure === 'account_runtime_not_started' ? null : failure, defaultStart: 'NOT_STARTED', stopSemantics: 'UPSTREAM_STRATEGY_CONFIG' },
          automaticTargets: { version: 1, supported: false, reason: 'native_strategy_only' } } };
        if (method === 'GET' && path === '/strategy/config') return { status: 200, value: strategyConfiguration() };
        const verifyConfig = /^\/strategy\/config\/commands\/([^/]+)\/verify$/.exec(path);
        if (method === 'POST' && verifyConfig) return { status: 200, value: await verifyConfiguration(decodeURIComponent(verifyConfig[1]!)) };
        if (method === 'POST' && path === '/strategy/config') return { status: 200, value: await configureStrategy(body!) };
        if (method === 'GET' && path === '/strategy/fence') return { status: 200, value: { ...envelope(), accountSourceId: state.sourceId, strategyRevision: state.strategyRevision } };
        if (method === 'GET' && path === '/strategy') return { status: 200, value: { ...envelope(), accountSourceId: state.sourceId, strategyRevision: state.strategyRevision, strategy: await runtime.request('STRATEGY_STATUS') } };
        const strategyReceipt = /^\/strategy\/commands\/([^/]+)$/.exec(path);
        if (method === 'GET' && strategyReceipt) { const item = state.strategyCommands?.[decodeURIComponent(strategyReceipt[1]!)]; if (!item) fail('strategy_command_not_found', 404); return { status: 200, value: structuredClone(item.receipt) }; }
        if (method === 'GET' && path === '/account') return { status: 200, value: account() };
        if (method === 'GET' && path === '/capital-exits') return { status: 200, value: status() };

        const receipt = /^\/capital-exits\/commands\/([^/]+)$/.exec(path);
        if (method === 'GET' && receipt) return { status: 200, value: commandReceipt(decodeURIComponent(receipt[1]!)) };
        const exitPath = /^\/capital-exits\/([^/]+)$/.exec(path);
        if (method === 'GET' && exitPath) { const exit = state.exits[decodeURIComponent(exitPath[1]!)]; if (!exit) fail('capital_exit_not_found', 404); actions(exit); return { status: 200, value: { ...envelope(), exit: exit.view } }; }
        const protectionPath = /^\/targets\/([^/]+)\/cancel-protection$/.exec(path);
        if (method === 'POST' && protectionPath) return { status: 200, value: await runtime.request('CANCEL_PROTECTION', { id: decodeURIComponent(protectionPath[1]!) }) };
        const targetPath = /^\/targets\/([^/]+)$/.exec(path);
        if (method === 'GET' && targetPath) { const targetId = decodeURIComponent(targetPath[1]!), stored = state.targets[targetId]; if (!stored) fail('target_receipt_not_found', 404); await settleTarget(targetId); return { status: 200, value: structuredClone(stored.receipt) }; }
        if (method !== 'POST') return { status: 404, value: { error: 'not_found' } };
        if (path === '/sizing/fixed-risk') return { status: 200, value: { ...await runtime.request('FIXED_RISK_SIZE', { ...nativeFixedRiskSizing(body!.sizing) }), ...envelope() } };
        if (path === '/orders/preview') return { status: 200, value: { ...await runtime.request('ORDER_PREVIEW', { instrument_id: body!.instrument_id, order: body!.order }), ...envelope() } };

        const action = /^\/capital-exits\/([^/]+)\/actions$/.exec(path);
        if (action) return { status: 202, value: await admitCommand(body!, decodeURIComponent(action[1]!)) };
        if (path === '/targets') fail('native_strategy_only', 410);
        if (path === '/strategy/commands') return { status: 202, value: await strategyCommand(body!) };
        return { status: 404, value: { error: 'not_found' } };
      });
  }
  return {
    request: dispatch,
    async handle(path: string, request: IncomingMessage, readBody: () => Promise<unknown>) {
      if (request.headers['x-qz-client-id'] !== options.clientId) fail('account_client_identity_mismatch',403);
      if (request.method !== 'GET' && request.method !== 'POST') return {status:405,value:{error:'method_not_allowed'}};
      return dispatch(path,request.method,request.headers['x-qz-client-id'],request.method === 'POST' ? await readBody() : undefined);
    },
    async close() { stopped = true; clearInterval(interval); unsubscribe?.(); await queue; await runtime.close(); await save(); },
  };
}
