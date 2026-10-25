import { retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { scienceTarget } from './config.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { CapitalExitStatus } from '../../../packages/contracts/src/index.ts';
import { canonical } from '../../research/src/documents.ts';
import type { ConnectionConfig, ServiceConfig } from './config.ts';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { Context } from '@earendil-works/chord';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { Application, counter, decimal, decimalUnits, json, nowNs, object, Problem, text } from './facts.ts';
import { connectionBase, downstreamRequest } from './downstream.ts';

export async function validateCapitalDocument(config: Pick<ServiceConfig, 'dataDir' | 'nautilus'> & { jobBinary: NativeTarget }, kind: string, value: unknown, context: Context = BACKGROUND_CONTEXT) {
  const directory = await mkdtemp(join(config.dataDir, 'capital-contract-'));
  try { await executeNative(scienceTarget(config), 'validate-capital-exit', json({ kind, value }), [], directory, context); }
  catch (error) {
    if (context.abortSignal?.aborted || typeof scienceTarget(config) !== 'string') throw error;
    throw new Problem('native_capital_exit_contract_invalid', 422);
  }
  finally { await rm(directory, { recursive: true, force: true }); }
}
const connectionKey = (connection: ConnectionConfig) => `${connectionBase(connection).href}|${connection.mode}|${connection.accountId}`;
export function accountKey(connection: ConnectionConfig, records?: Record<string, JsonObject>): string {
  const key = connectionKey(connection);
  return Object.entries(records ?? {}).find(([, value]) => control(value).bindings?.includes(key))?.[0] ?? key;
}
export interface CapitalCommandPermission { bindingKey: string; capability: TransportCapability; status: CapitalExitStatus }
export interface CapitalCommand { original: JsonObject; body: JsonObject; path: string; reserved: string; state: string; result: JsonObject | null; error: string | null; taskId: number; permissionBasis?: CapitalCommandPermission }
export interface TransportCapability { document: JsonObject; receivedAtNs: string }
export interface AccountControl { status?: CapitalExitStatus; native?: JsonObject; commands: Record<string, CapitalCommand>; bindings?: string[]; receivedAtNs?: string; capabilities?: JsonObject; capabilitiesReceivedAtNs?: string;
  capabilitiesByBinding?: Record<string, TransportCapability>; refreshFailuresByBinding?: Record<string, { code: string; receivedAtNs: string }> }
export function control(value: unknown): AccountControl { return value as AccountControl ?? { commands: {} }; }
export const transportBindingKey = (connection: ConnectionConfig) => JSON.stringify([connectionBase(connection).href, connection.mode, connection.clientId, connection.accountId]);
/** Permission envelopes belong to a transport binding, while reservations and
 * command ownership belong to the shared physical account. */
export function transportCapability(value: AccountControl, connection: ConnectionConfig, atNs = nowNs()): TransportCapability | null {
  const key = transportBindingKey(connection);
  if (value.refreshFailuresByBinding?.[key]) return null;
  const saved = value.capabilitiesByBinding?.[key];
  // Once a record has transport lineage, a different URL is a different
  // source even when its client/account envelope happens to have the same names.
  if (value.capabilitiesByBinding !== undefined && !saved) return null;
  const document = saved?.document ?? value.capabilities;
  if (!document || document.clientId !== connection.clientId || document.accountId !== connection.accountId || document.mode !== connection.mode || typeof document.currency !== 'string') return null;
  const receivedAtNs = counter(saved?.receivedAtNs ?? value.capabilitiesReceivedAtNs ?? atNs, 'capital_capabilities_received');
  if (!saved) { value.capabilitiesByBinding ??= {}; value.capabilitiesByBinding[key] = { document: json(document), receivedAtNs }; }
  return { document: json(document), receivedAtNs };
}
export function transportControlFailure(value: AccountControl, connection: ConnectionConfig) { return value.refreshFailuresByBinding?.[transportBindingKey(connection)] ?? null; }
/** Compare physical state without treating a different client's initiation
 * permission as a new reservation/account event. */
export function physicalCapitalState(status: CapitalExitStatus): JsonObject {
  return json({ accountSourceId: status.accountSourceId, observationId: status.observationId, controlRevision: status.controlRevision,
    reserved: status.reserved, exits: status.exits, blocked: status.capability === 'BLOCKED',
    reason: status.reason === 'capital_exit_initiation_unavailable' ? null : status.reason });
}
export function recordCapitalControlFailure(records: Record<string, JsonObject>, connection: ConnectionConfig, code: string): void {
  const key = accountKey(connection, records), value = control(records[key]), binding = transportBindingKey(connection);
  value.refreshFailuresByBinding ??= {};
  if (value.refreshFailuresByBinding[binding]?.code !== code) value.refreshFailuresByBinding[binding] = { code, receivedAtNs: nowNs() };
  records[key] = json(value);
}
export function moneyString(units: bigint): string {
  const sign = units < 0n ? '-' : ''; const value = units < 0n ? -units : units;
  return `${sign}${value / 10n ** 18n}.${(value % 10n ** 18n).toString().padStart(18, '0')}`;
}
export function outstandingReserved(value: AccountControl): string | null {
  if (value.status?.reserved == null) return null;
  return moneyString(decimalUnits(value.status.reserved) + Object.values(value.commands).filter(command => ['UNCERTAIN', 'PENDING', 'ACCEPTED'].includes(command.state)).reduce((sum, command) => sum + decimalUnits(command.reserved), 0n));
}
export function capitalStatus(value: AccountControl): CapitalExitStatus {
  return value.status ? { ...value.status, reserved: outstandingReserved(value) } : { capability: 'UNSUPPORTED', reason: 'downstream_capability_not_observed', accountSourceId: null, observationId: null, controlRevision: null, reserved: null, exits: [] };
}
/** Bind transport permissions/read failures to their original client while
 * retaining physical revisions, exit fences and reservations across aliases. */
export function capitalStatusForBinding(value: AccountControl, connection: ConnectionConfig, atNs = nowNs()): CapitalExitStatus {
  const status = capitalStatus(value), failure = transportControlFailure(value, connection);
  if (failure) return { ...status, capability: 'BLOCKED', reason: failure.code, reserved: null };
  const capability = transportCapability(value, connection, atNs);
  if (!capability) return value.capabilitiesByBinding === undefined ? status
    : { ...status, capability: status.capability === 'BLOCKED' ? 'BLOCKED' : 'UNSUPPORTED', reason: 'transport_capability_not_observed', reserved: null };
  const permission = object(capability.document.capitalExit ?? {});
  const readable = permission.version === 1 && (permission.supported === true || permission.readable === true);
  if (!readable) return { ...status, capability: status.capability === 'BLOCKED' ? 'BLOCKED' : 'UNSUPPORTED',
    reason: status.capability === 'BLOCKED' ? status.reason : 'downstream_capital_exit_unsupported', reserved: null };
  if (status.capability !== 'BLOCKED' && status.accountSourceId) {
    status.capability = permission.supported === true ? 'SUPPORTED' : 'UNSUPPORTED';
    if (status.reason === 'capital_exit_initiation_unavailable' || status.reason === null) status.reason = status.capability === 'SUPPORTED' ? null : 'capital_exit_initiation_unavailable';
  }
  return status;
}
/** An original command binds both its exact permission source and the physical
 * account state. This is independent of the downstream receipt identity. */
export function capitalCommandPermission(value: AccountControl, connection: ConnectionConfig): CapitalCommandPermission {
  const capability = transportCapability(value, connection), status = capitalStatusForBinding(value, connection);
  if (!capability || status.capability !== 'SUPPORTED' || !status.accountSourceId || !status.observationId || !status.controlRevision || status.reserved === null)
    throw new Problem(status.reason ?? 'capital_exit_permission_unavailable', 409);
  return { bindingKey: transportBindingKey(connection), capability, status: json(status) as unknown as CapitalExitStatus };
}
export function assertCapitalCommandPermission(value: AccountControl, connection: ConnectionConfig, expected: CapitalCommandPermission | undefined, ownRequestId?: string): void {
  if (!expected) throw new Problem('capital_exit_permission_basis_missing', 409);
  // The command's own newly retained reservation must not invalidate the
  // original pre-admission basis; every other command remains in that basis.
  const comparable = ownRequestId === undefined ? value : { ...value, commands: Object.fromEntries(Object.entries(value.commands).filter(([id]) => id !== ownRequestId)) };
  const current = capitalCommandPermission(comparable, connection);
  if (canonical(json(current)) !== canonical(json(expected))) throw new Problem('capital_exit_permission_basis_changed', 409);
}
export function assertEnvelope(connection: ConnectionConfig, value: JsonObject, currency: string) {
  if (value.clientId !== connection.clientId || value.accountId !== connection.accountId || value.mode !== connection.mode || value.currency !== currency) throw new Problem('downstream_control_identity_mismatch', 409);
}
export async function readCapitalControl(connection: ConnectionConfig, currency: string, signal?: AbortSignal): Promise<{ status: CapitalExitStatus; native: JsonObject; capabilities: JsonObject }> {
  const capabilities = await downstreamRequest(connection, 'capabilities', 'GET', undefined, signal);
  assertEnvelope(connection, capabilities, currency);
  const capability = capabilities.capitalExit ? object(capabilities.capitalExit) : null;
  if (capability?.version !== 1 || capability.supported !== true && capability.readable !== true) return { capabilities, native: {}, status: { capability: 'UNSUPPORTED', reason: capability && capability.version !== 1 ? 'capital_exit_version_unsupported' : typeof capability?.reason === 'string' ? capability.reason : 'downstream_capital_exit_unsupported', accountSourceId: null, observationId: null, controlRevision: null, reserved: null, exits: [] } };
  const native = await downstreamRequest(connection, 'capital-exits', 'GET', undefined, signal);
  assertEnvelope(connection, native, currency);
  const exits = Array.isArray(native.exits) ? native.exits.map(value => object(value)) : [];
  const source = text(native.accountSourceId, 'account_source_id');
  for (const exit of exits) {
    if (exit.account_source_id !== source || exit.schema_version !== 1 || exit.environment !== (connection.mode === 'paper' ? 'PAPER' : 'LIVE')) throw new Problem('capital_exit_native_scope_mismatch', 409);
    counter(exit.revision, 'capital_exit_revision'); const funds = object(exit.funds);
    for (const key of ['requested_amount', 'reserved_amount', 'reconciled_withdrawal_amount']) {
      const money = object(funds[key]); if (money.currency !== currency || decimalUnits(decimal(money.amount, 'capital_exit_amount')) < 0n) throw new Problem('capital_exit_money_mismatch');
    }
  }
  const reserved = native.reserved == null ? null : decimal(native.reserved, 'outstanding_reservation'); if (reserved !== null && decimalUnits(reserved) < 0n) throw new Problem('capital_exit_reservation_invalid');
  return { capabilities, native, status: { capability: native.blocked === true ? 'BLOCKED' : capability?.supported === true ? 'SUPPORTED' : 'UNSUPPORTED', reason: typeof native.reason === 'string' ? native.reason : capability?.supported === true ? null : 'capital_exit_initiation_unavailable',
    accountSourceId: source, observationId: text(native.observationId, 'source_observation_id'), controlRevision: counter(native.controlRevision, 'account_control_revision'), reserved, exits: exits as unknown as CapitalExitStatus['exits'] } };
}

interface CommandInput { key: string; requestId: string; connection: ConnectionConfig; reconcile: boolean; binary: NativeTarget; dataDir: string }
export const CapitalExitCommand = defineTask<CommandInput, { phase: 'deliver' } | { phase: 'reconcile' } | { phase: 'wait'; wakeAt: number }, null>({
  name: 'qz.capital-exit-command', version: 1, initial: input => ({ phase: input.reconcile ? 'reconcile' : 'deliver' }),
  phases: {
    async deliver(task, runtime, context) {
      let command: CapitalCommand | undefined;
      await runtime.commit(async tx => {
        const facts = await tx.doc(Application); const state = control(facts.accountControls?.[task.input.key]); const current = state.commands[task.input.requestId]!;
        if (current.state === 'UNCERTAIN') return { status: 'running', checkpoint: { phase: 'reconcile' } };
        if (current.state !== 'PENDING') return { status: 'terminal', outcome: { status: 'completed', result: null } };
        try { assertCapitalCommandPermission(state, task.input.connection, current.permissionBasis, task.input.requestId); }
        catch (error) {
          if (!(error instanceof Problem)) throw error;
          current.state = 'REJECTED'; current.reserved = '0'; current.error = error.message;
          facts.accountControls![task.input.key] = json(state);
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }
        current.state = 'UNCERTAIN'; command = json(current) as unknown as CapitalCommand; facts.accountControls![task.input.key] = json(state);
        return { status: 'running', checkpoint: { phase: 'reconcile' } };
      }, context);
      if (!command) return;
      let allowed = false;
      // Only this live deliver invocation knows no POST has been attempted.
      // A recovered UNCERTAIN task starts in reconcile and never enters this
      // local-refusal branch or releases a remotely uncertain reservation.
      await runtime.commit(async tx => {
        const facts = await tx.doc(Application); const state = control(facts.accountControls?.[task.input.key]); const current = state.commands[task.input.requestId]!;
        try {
          if (current.state !== 'UNCERTAIN' || canonical(current.original) !== canonical(command!.original) || canonical(current.body) !== canonical(command!.body)) throw new Problem('capital_exit_command_changed_before_delivery', 409);
          assertCapitalCommandPermission(state, task.input.connection, command!.permissionBasis, task.input.requestId);
          allowed = true;
        } catch (error) {
          if (!(error instanceof Problem)) throw error;
          current.state = 'REJECTED'; current.reserved = '0'; current.error = error.message;
          facts.accountControls![task.input.key] = json(state);
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }
      }, context);
      if (!allowed) return;
      try {
        const value = await downstreamRequest(task.input.connection, command.path, 'POST', { requestId: task.input.requestId, clientId: task.input.connection.clientId, accountId: task.input.connection.accountId, command: command.body }, runtime.signal);
        await settle(value);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const state = control(facts.accountControls?.[task.input.key]);
          state.commands[task.input.requestId]!.error = 'capital_exit_command_requires_reconciliation'; facts.accountControls![task.input.key] = json(state);
          return { status: 'running', checkpoint: { phase: 'reconcile' } };
        }, context);
      }
      async function settle(value: JsonObject) {
        if (value.status === 'ACCEPTED') await withNativeRemoteTracking(runtime.taskId, runtime, context, () => validateCapitalDocument({ jobBinary: task.input.binary, dataDir: task.input.dataDir }, 'VIEW', value.result, context));
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const state = control(facts.accountControls?.[task.input.key]); acceptCommand(task.input, state, value);
          facts.accountControls![task.input.key] = json(state);
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, context);
      }
    },
    async reconcile(task, runtime, context) {
      try {
        const value = await downstreamRequest(task.input.connection, `capital-exits/commands/${encodeURIComponent(task.input.requestId)}`, 'GET', undefined, runtime.signal);
        if (value.status === 'ACCEPTED') await withNativeRemoteTracking(runtime.taskId, runtime, context, () => validateCapitalDocument({ jobBinary: task.input.binary, dataDir: task.input.dataDir }, 'VIEW', value.result, context));
        await runtime.commit(async tx => {
          const facts = await tx.doc(Application); const state = control(facts.accountControls?.[task.input.key]); acceptCommand(task.input, state, value);
          facts.accountControls![task.input.key] = json(state);
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } }), context);
      }
    },
    async wait(task, runtime, context) {
      await runtime.sleep(task.state.checkpoint.wakeAt ?? runtime.now(), context);
      await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'reconcile' } }), context);
    },
  },
  async abort(_task, runtime, context) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context); },
});
function acceptCommand(input: CommandInput, state: AccountControl, value: JsonObject) {
  const command = state.commands[input.requestId]!;
  if (value.requestId !== input.requestId || value.clientId !== input.connection.clientId || value.accountId !== input.connection.accountId || canonical(value.command!) !== canonical(command.body)) throw new Problem('capital_exit_command_identity_mismatch', 409);
  if (!['ACCEPTED', 'REJECTED'].includes(String(value.status))) throw new Problem('capital_exit_command_unsettled');
  command.state = String(value.status); command.result = value; command.error = null;
  // Do not release a local reservation until an authoritative status includes this command.
  // Its reservation remains part of the shared account control fact until status reconciliation.
}
export function reconcileControl(previous: AccountControl, fresh: Awaited<ReturnType<typeof readCapitalControl>>): AccountControl {
  const capabilities = { capabilities: json(fresh.capabilities), capabilitiesReceivedAtNs: previous.capabilities
    && canonical(previous.capabilities) === canonical(fresh.capabilities) ? previous.capabilitiesReceivedAtNs ?? nowNs() : nowNs() };
  if (previous.status?.accountSourceId && fresh.status.accountSourceId && previous.status.accountSourceId !== fresh.status.accountSourceId) throw new Problem('capital_control_identity_mismatch', 409);
  const oldRevision = previous.status?.controlRevision; const revision = fresh.status.controlRevision;
  if (oldRevision && revision && BigInt(revision) < BigInt(oldRevision)) throw new Problem('capital_control_out_of_order', 409);
  if (oldRevision && revision === oldRevision && previous.status && (previous.status.reserved !== null && fresh.status.reserved !== null && previous.status.reserved !== fresh.status.reserved || canonical(json(previous.status.exits)) !== canonical(json(fresh.status.exits)))) throw new Problem('capital_control_conflict', 409);
  // No permission to read is not an authoritative physical account update.
  // recordCapitalControl retains that failure only on its transport binding.
  if (!fresh.status.accountSourceId && previous.status?.accountSourceId) return { ...previous, ...capabilities };
  const commands = previous.commands;
  for (const command of Object.values(commands)) {
    if (command.state === 'ACCEPTED' && command.reserved !== '0') {
      const accepted = object(command.result); const original = object(accepted.result);
      if (!fresh.status.exits.some(exit => exit.command_id === original.command_id || exit.id === original.id && BigInt(exit.revision) >= BigInt(counter(original.revision, 'exit_revision')))) throw new Problem('capital_exit_status_not_reconciled', 409);
      command.reserved = '0';
    }
  }
  const unchanged = previous.status && canonical(physicalCapitalState(previous.status)) === canonical(physicalCapitalState(fresh.status));
  const receivedAtNs = unchanged ? previous.receivedAtNs ?? nowNs() : nowNs();
  return { ...previous, ...capabilities, status: fresh.status, native: fresh.native, commands, receivedAtNs };
}
export function recordCapitalControl(records: Record<string, JsonObject>, connection: ConnectionConfig, fresh: Awaited<ReturnType<typeof readCapitalControl>>): string {
  const original = accountKey(connection, records); const prior = control(records[original]);
  if (prior.status?.accountSourceId && fresh.status.accountSourceId && prior.status.accountSourceId !== fresh.status.accountSourceId) throw new Problem('capital_control_identity_mismatch', 409);
  const same = fresh.status.accountSourceId && Object.entries(records).find(([, value]) => { const record = control(value); return record.status?.accountSourceId === fresh.status.accountSourceId && record.native?.mode === connection.mode && record.native?.currency === fresh.native.currency; });
  const key = same ? same[0] : original;
  if (key !== original && Object.keys(prior.commands).length) throw new Problem('account_alias_requires_reconciliation', 409);
  const next = reconcileControl(control(records[key]), fresh);
  if (key !== original) {
    // A provisional, command-free record may have been retained before its
    // physical source was readable. Move its binding facts rather than leave
    // an older accountKey() match shadowing the now-authoritative record.
    next.capabilitiesByBinding = { ...prior.capabilitiesByBinding, ...next.capabilitiesByBinding };
    next.refreshFailuresByBinding = { ...prior.refreshFailuresByBinding, ...next.refreshFailuresByBinding };
  }
  const binding = transportBindingKey(connection);
  const saved = next.capabilitiesByBinding?.[binding];
  // A repeated read through A retains A's first receipt even if B was the
  // latest transport to read the same physical account.
  next.capabilitiesByBinding ??= {};
  next.capabilitiesByBinding[binding] = { document: json(fresh.capabilities), receivedAtNs: saved && canonical(saved.document) === canonical(fresh.capabilities) ? saved.receivedAtNs : nowNs() };
  if (fresh.status.accountSourceId) { if (next.refreshFailuresByBinding) delete next.refreshFailuresByBinding[binding]; }
  else {
    const code = fresh.status.reason ?? 'downstream_capital_exit_unsupported';
    next.refreshFailuresByBinding ??= {};
    if (next.refreshFailuresByBinding[binding]?.code !== code) next.refreshFailuresByBinding[binding] = { code, receivedAtNs: nowNs() };
  }
  next.bindings = [...new Set([...(prior.bindings ?? []), ...(next.bindings ?? []), key, connectionKey(connection)])];
  records[key] = json(next);
  if (key !== original) delete records[original];
  return key;
}
