import { TokenType } from '@streamparser/json';
import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { defineDocFamily, defineTask, type Harness, type ConversationId, type JsonObject, type TaskId, type TaskRuntime, type Tx } from '@earendil-works/pi-durable';
import { randomUUID } from 'node:crypto';
import { AsyncLocalStorage } from 'node:async_hooks';
import { readFile, writeFile } from 'node:fs/promises';
import { join, isAbsolute } from 'node:path';
import { runNativeProcess, cancelNativeProcess } from './native-process.ts';
import { Readable } from 'node:stream';
import { Agent, fetch } from 'undici';
import { isTickNativeCommand } from '../../../packages/contracts/src/nautilus.ts';
import type { NativeCapabilities, NativeExecutionReceipt, NativeExecutionRequest, NativeExecutionRejection, NativeJson, NativeTarget, RemoteNativeTarget, NativeExecutionTarget, ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { ExecutionFailure } from './execution-errors.ts';
import { NativeEvidencePart, retainNativeEvidence, readNativeEvidence, readNativeEvidenceParts } from '../plugins/native-evidence.ts';
import { JSONParser, JsonReader, jsonChunks, canonicalJsonDigest } from './native-json.ts';
import { NativeTableReader } from './native-table.ts';
import { canonical } from './documents.ts';

export const nativeCommands = new Set(['review-source', 'native-strategy-backtest', 'native-configured-backtest', 'validate-capital-exit', 'catalog-inspect', 'catalog-coverage', 'catalog-prefix-digest', 'catalog-factor-scope']);
type NativeInterceptor = (command: string, input: NativeJson, next: () => Promise<NativeJson>) => Promise<NativeJson>;
const nativeInterceptor = new AsyncLocalStorage<NativeInterceptor>();
/** Scoped checkpoint adapter for a fixed review computation; other science is unchanged. */
export function withNativeExecutionInterceptor<T>(interceptor: NativeInterceptor, operation: () => Promise<T>): Promise<T> { return nativeInterceptor.run(interceptor, operation); }
const instanceCredentials = new Map<string, string>();
const progressTracking = new AsyncLocalStorage<(snapshots: NativeJson[]) => Promise<void>>();
/** Observes original account snapshots; never changes invocation or cancellation identity. */
export function withNativeProgress<T>(observer: (snapshots: NativeJson[]) => Promise<void>, operation: () => Promise<T>): Promise<T> {
  return progressTracking.run(observer, operation);
}

/** Settings credentials stay outside research facts; targets carry only this random reference. */
export function setRemoteNativeCredential(reference: string, token?: string): void {
  if (token) instanceCredentials.set(reference, token); else instanceCredentials.delete(reference);
}
export function remoteNativeCredential(reference: string): string | undefined {
  return instanceCredentials.get(reference);
}

/** Receipt references only: this document never schedules or retries work. */
export const NativeRemoteUncertainty = defineDocFamily<{ requests: JsonObject[] }, null>({
  // Upstream retires task-scoped documents at task settlement. Retain these
  // original-task-keyed facts with the conversation, as for native evidence.
  kind: 'qz.native-remote-uncertainty', checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, version: 1, scope: 'conversation', history: 'latest', fork: 'initial', family: true, initial: () => ({ requests: [] }),
});
class RemoteUncertaintyError extends Error {
  readonly reference: JsonObject;
  constructor(code: 'nautilus_execution_unknown' | 'nautilus_cancellation_unknown', requestId: string, target: NativeExecutionTarget, cause?: unknown, nativeTaskId?: TaskId) {
    super(`${code}:${requestId}`, { cause });
    this.reference = { requestId, target: { ...target }, error: code };
    if (nativeTaskId !== undefined) this.reference.nativeTaskId = nativeTaskId;
  }
}
class RemoteAdmissionRejectedError extends Error {
  readonly rejection: NativeExecutionRejection;
  constructor(rejection: NativeExecutionRejection) { super(rejection.error); this.rejection = rejection; }
}

type ReceiptRuntime = Pick<TaskRuntime<unknown, unknown, unknown, object>, 'commit' | 'conversationId' | 'signal' | 'getTask' | 'waitForTask' | 'snapshot' | 'snapshotAsOf' | 'watchDoc'>;
export interface NativeScanContext { scanId: string; ordinal: string; implementationDigest?: string }
export type NativeScanCheckpoint = { phase: 'compute' } | { phase: 'reconcile'; nativeTaskId: TaskId<NativeJson> };
const tracking = new AsyncLocalStorage<{ taskId: TaskId; runtime: ReceiptRuntime; context: Context; scanContext?: NativeScanContext }>();

/** Bind transport receipt writes to the original official task; this has no
 * queue or execution state of its own. Register before unsafe network waits. */
export function withNativeRemoteTracking<T>(taskId: TaskId, runtime: ReceiptRuntime, context: Context, operation: () => Promise<T>, scanContext?: NativeScanContext): Promise<T> {
  return tracking.run({ taskId, runtime, context, scanContext }, operation);
}

/** Only an original native invocation can be reconciled; diagnostics alone do not identify one. */
export function nativeUncertainTaskId(error: unknown): TaskId<NativeJson> | null {
  const seen = new Set<Error>();
  for (let current = error; current instanceof Error && !seen.has(current); current = current.cause) {
    if (current instanceof RemoteUncertaintyError && typeof current.reference.nativeTaskId === 'number') return current.reference.nativeTaskId as TaskId<NativeJson>;
    seen.add(current);
  }
  return null;
}

async function retainRemoteReference(reference: JsonObject, owner: { taskId: TaskId; runtime: ReceiptRuntime; context: Context }): Promise<void> {
  await owner.runtime.commit(async tx => {
    const facts = await tx.doc(NativeRemoteUncertainty, owner.runtime.conversationId, String(owner.taskId), null);
    const existing = facts.requests.findIndex(item => item.requestId === reference.requestId);
    const next = { ...reference, taskId: owner.taskId };
    if (existing < 0) facts.requests.push(next); else facts.requests[existing] = next;
    return undefined;
  }, withoutAbortSignal(owner.context));
}

/** Call before the task's abort check, while the original exception still
 * retains its cause. The original task may stop; its remote receipt remains. */
export async function retainNativeRemoteUncertainty(error: unknown, taskId: TaskId, runtime: ReceiptRuntime, context: Context): Promise<void> {
  const seen = new Set<Error>(); let current = error;
  while (current instanceof Error && !seen.has(current)) {
    if (current instanceof RemoteUncertaintyError) {
      // A durable abort mark also closes TaskRuntime.commit regardless of the
      // caller's Context. The reference saved BEFORE POST remains PENDING.
      if (!runtime.signal.aborted) {
        try { await retainRemoteReference({ ...current.reference, status: 'UNKNOWN' }, { taskId, runtime, context }); }
        catch (failure) { if (!runtime.signal.aborted) throw failure; }
      }
      return;
    }
    seen.add(current); current = current.cause;
  }
}

export function validateRemoteTarget(value: unknown): RemoteNativeTarget {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('nautilus_remote_target_invalid');
  const item = value as Record<string, unknown>;
  if (typeof item.id !== 'string' || !item.id.trim() || typeof item.url !== 'string') throw new Error('nautilus_remote_target_invalid');
  let url: URL;
  try { url = new URL(item.url); } catch { throw new Error('nautilus_remote_url_invalid'); }
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.hash || url.search) throw new Error('nautilus_remote_url_invalid');
  if (item.tokenEnv !== undefined && (typeof item.tokenEnv !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(item.tokenEnv))) throw new Error('nautilus_token_env_invalid');
  if (item.credentialRef !== undefined && (typeof item.credentialRef !== 'string' || !/^[a-f0-9]{8}-[a-f0-9]{4}-[1-8][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/i.test(item.credentialRef))) throw new Error('nautilus_credential_ref_invalid');
  return { id: item.id, url: item.url, ...(typeof item.tokenEnv === 'string' ? { tokenEnv: item.tokenEnv } : {}), ...(typeof item.credentialRef === 'string' ? { credentialRef: item.credentialRef } : {}) };
}

export function isProcessNativeTarget(target: NativeExecutionTarget): target is ProcessNativeTarget { return 'kind' in target && target.kind === 'RUST_PROCESS'; }
export function validateNativeTarget(value: unknown): NativeExecutionTarget {
  if (!value || typeof value !== 'object' || Array.isArray(value) || (value as {kind?:unknown}).kind !== 'RUST_PROCESS') return validateRemoteTarget(value);
  const item = value as ProcessNativeTarget;
  if (!item.id || typeof item.binary !== 'string' || !isAbsolute(item.binary) || item.capabilities?.schemaVersion !== 1 || item.capabilities.protocol !== 'quazonai-native-process-v1' || item.capabilities.running !== false
    || !item.capabilities.identity || typeof item.capabilities.research?.configuredRunFactory !== 'boolean' || typeof item.capabilities.research?.singleStrategyRegistration !== 'boolean'
    || typeof item.capabilities.account?.compiled !== 'boolean' || typeof item.capabilities.account?.registration !== 'boolean' || item.capabilities.account.command !== 'account-runtime' || item.capabilities.account.requiresExplicitStart !== true) throw new Error('native_process_target_invalid');
  return structuredClone(item);
}
function processCapabilities(target: ProcessNativeTarget): NativeCapabilities {
  const commands = [...nativeCommands].filter(command => command !== 'native-configured-backtest' || target.capabilities.research.configuredRunFactory);
  return {schemaVersion:1,implementationDigest:'rust-process:'+canonical(target.capabilities.identity as never),commands,buildIdentity:target.capabilities.identity};
}

/** Production sets only the remote binding. Explicit local executables remain
 * available to the separate worker/science test harness, never as a fallback. */
export function defaultNativeTarget(): NativeTarget {
  if (process.env.QZ_NAUTILUS_BACKTEST) return validateRemoteTarget(JSON.parse(process.env.QZ_NAUTILUS_BACKTEST));
  if (process.env.QZ_JOB_BINARY) return process.env.QZ_JOB_BINARY;
  throw new Error('nautilus_backtest_not_configured');
}

/** New admissions retain the instance in their original dataset facts, so a
 * later configuration change cannot redirect a resumed research trial. */
export function nativeTargetForData(data: { [key: string]: unknown }): NativeTarget {
  if (data.nautilusTarget !== undefined) return validateNativeTarget(data.nautilusTarget);
  if (process.env.QZ_REQUIRE_REMOTE_NAUTILUS === '1') throw new Error('local_nautilus_target_disabled');
  return defaultNativeTarget();
}

// Retired WF receipts still need bounded historical recovery; this does not authorize new execution.
const tableCommands = new Set(['native-strategy-backtest', 'native-configured-backtest', 'native-walk-forward']);
type TableOwner = { taskId: TaskId; runtime: ReceiptRuntime; context: Context };
function tableReader(owner: TableOwner) {
  return new NativeTableReader({ taskId: owner.taskId, conversationId: owner.runtime.conversationId }, async (key, data) => {
    await owner.runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, owner.runtime.conversationId, key, null)).data = data; return undefined; }, owner.context);
  });
}

class ReceiptStatusReader {
  private parser = new JSONParser({ paths: ['$.schemaVersion', '$.requestId', '$.status', '$.error'], keepStack: false });
  private value: Record<string, NativeJson> = {};
  private hasResult = false;
  constructor() {
    let depth = 0; let rootObject = false; let expectingKey = false;
    // Observe only the root property name. Selecting $.result would retain its
    // descendants; full JSON syntax validation remains the parser's job.
    this.parser.onToken = ({ token, value }) => {
      if (depth === 0 && token === TokenType.LEFT_BRACE) { rootObject = true; expectingKey = true; }
      if (rootObject && depth === 1) {
        if (token === TokenType.COMMA) expectingKey = true;
        else if (token === TokenType.STRING && expectingKey) {
          if (value === 'result') this.hasResult = true;
          expectingKey = false;
        }
      }
      if (token === TokenType.LEFT_BRACE || token === TokenType.LEFT_BRACKET) depth++;
      else if (token === TokenType.RIGHT_BRACE || token === TokenType.RIGHT_BRACKET) depth--;
    };
    this.parser.onValue = ({ key, value }) => { this.value[String(key)] = value as NativeJson; };
  }
  write(text: string) { this.parser.write(text); }
  finish(): NativeJson {
    if (!this.parser.isEnded) this.parser.end();
    if (this.value.status === 'COMPLETED' && !this.hasResult) throw new Error('nautilus_receipt_invalid');
    return this.value;
  }
}

async function remoteRequest(target: NativeExecutionTarget, path: string, method: 'GET' | 'POST' | 'DELETE', body?: NativeJson, signal?: AbortSignal, tables?: TableOwner, statusOnly = false): Promise<NativeJson> {
  if (isProcessNativeTarget(target)) {
    validateNativeTarget(target);
    if (path === 'v1/capabilities' && method === 'GET') return processCapabilities(target) as unknown as NativeJson;
    const requestId = path.split('/').at(-1)!;
    if (method === 'DELETE') { await cancelNativeProcess(requestId); return {schemaVersion:1,requestId,status:'UNKNOWN',error:'native_process_exit_does_not_prove_cancellation'}; }
    // Only the original Pi Task can persist a successful process result. Lost handles never replay work.
    return {schemaVersion:1,requestId,status:'UNKNOWN',error:'native_process_original_outcome_unknown'};
  }
  target = validateRemoteTarget(target);
  const url = new URL(target.url.endsWith('/') ? target.url : `${target.url}/`); url.pathname += path;
  const headers: Record<string, string> = { 'content-type': 'application/json' };
  const token = target.credentialRef ? instanceCredentials.get(target.credentialRef) : target.tokenEnv ? process.env[target.tokenEnv] : undefined;
  if ((target.credentialRef || target.tokenEnv) && !token) throw new Error('nautilus_credentials_missing');
  if (token) headers.authorization = `Bearer ${token}`;
  const dispatcher = new Agent({ headersTimeout: 0, bodyTimeout: 0, connect: { timeout: 30_000 } });
  try {
    const response = await fetch(url, { method, headers, ...(body === undefined ? {} : { body: Readable.from(jsonChunks(body)), duplex: 'half' as const }), signal, dispatcher, redirect: 'error' });
    if (!response.ok) {
      // Only the original invocation's explicit pre-execution refusal settles
      // non-execution. A proxy error or generic 4xx/5xx is not that evidence.
      if (method === 'POST' && path === 'v1/execute' && [429, 501].includes(response.status) && body && typeof body === 'object' && !Array.isArray(body) && typeof body.command === 'string' && isTickNativeCommand(body.command)) {
        let encoded = ''; let size = 0;
        if (response.body) for await (const bytes of response.body) {
          size += bytes.length;
          if (size > 4096) break;
          encoded += Buffer.from(bytes).toString('utf8');
        }
        if (size <= 4096) {
          let value: unknown;
          try { value = JSON.parse(encoded); } catch { /* Preserve ordinary HTTP failure below. */ }
          const rejected = value as Partial<NativeExecutionRejection> | null;
          const code = response.status === 429 ? 'nautilus_tick_capacity_exhausted' : 'nautilus_tick_resource_limits_unavailable';
          if (rejected && typeof rejected === 'object' && !Array.isArray(rejected)
            && Object.keys(rejected).every(key => ['schemaVersion', 'requestId', 'admission', 'error'].includes(key))
            && rejected.schemaVersion === 1 && rejected.requestId === body.requestId && rejected.admission === 'REJECTED' && rejected.error === code) {
            throw new RemoteAdmissionRejectedError(rejected as NativeExecutionRejection);
          }
        }
      } else await response.body?.cancel();
      throw new Error(`nautilus_http_${response.status}`);
    }
    if (!response.body) throw new Error('nautilus_response_missing');
    // Cancellation consumes status only; never materialize a raced completed result.
    const parser = method === 'DELETE' || statusOnly ? new ReceiptStatusReader() : tables ? tableReader(tables) : new JsonReader(); const decoder = new TextDecoder();
    for await (const bytes of response.body) await parser.write(decoder.decode(bytes, { stream: true }));
    await parser.write(decoder.decode()); return await parser.finish();
  } finally { await dispatcher.close(); }
}

export async function remoteNativeCapabilities(target: NativeExecutionTarget, signal?: AbortSignal): Promise<NativeCapabilities> {
  const value = await remoteRequest(target, 'v1/capabilities', 'GET', undefined, signal) as unknown as NativeCapabilities;
  if (value?.schemaVersion !== 1 || typeof value.implementationDigest !== 'string' || !isProcessNativeTarget(target) && !/^sha256:[a-f0-9]{64}$/.test(value.implementationDigest)
    || !Array.isArray(value.commands) || !value.commands.every(command => typeof command === 'string')) throw new Error('nautilus_capabilities_invalid');
  return value;
}

function receipt(value: NativeJson, requestId: string, statusOnly = false): NativeExecutionReceipt {
  const result = value as unknown as NativeExecutionReceipt;
  if (result?.schemaVersion !== 1 || result.requestId !== requestId || !['COMPLETED', 'FAILED', 'CANCELLED', 'RUNNING', 'UNKNOWN'].includes(result.status)
    || !statusOnly && result.status === 'COMPLETED' && !Object.hasOwn(result, 'result')
    || ['FAILED', 'UNKNOWN'].includes(result.status) && typeof (result as { error?: unknown }).error !== 'string') throw new Error('nautilus_receipt_invalid');
  return result;
}

/** Read only the receipt for this original invocation and retained instance.
 * This does not submit or cancel any remote execution. */
export async function remoteNativeExecutionReceipt(target: NativeExecutionTarget, requestId: string, signal?: AbortSignal, tables?: TableOwner): Promise<NativeExecutionReceipt> {
  if (!/^[a-f0-9]{64}$/.test(requestId)) throw new Error('nautilus_request_id_invalid');
  return receipt(await remoteRequest(target, `v1/executions/${requestId}`, 'GET', undefined, signal, tables), requestId);
}

/** Metadata-only reconciliation: result is deliberately absent, never fabricated. */
export async function remoteNativeExecutionStatus(target: NativeExecutionTarget, requestId: string, signal?: AbortSignal): Promise<Pick<NativeExecutionReceipt, 'schemaVersion' | 'requestId' | 'status'> & { error?: string }> {
  if (!/^[a-f0-9]{64}$/.test(requestId)) throw new Error('nautilus_request_id_invalid');
  return receipt(await remoteRequest(target, `v1/executions/${requestId}`, 'GET', undefined, signal, undefined, true), requestId, true);
}

type AdoptedReceipt = { requestId: string } & ({ status: 'COMPLETED'; native: JsonObject } | { status: 'FAILED' | 'CANCELLED'; error: string });
const NativeCalls = defineDocFamily<{ taskId: TaskId<NativeJson> | null; adoptedReceipt?: AdoptedReceipt }, null>({
  kind: 'qz.native-call', version: 1, scope: 'task', family: true,
  initial: () => ({ taskId: null }), checkpointWhen: () => true,
});
const NativeExecutionRequestPart = defineDocFamily<{ data: string }, null>({
  kind: 'qz.native-request-part', version: 1, scope: 'conversation', history: 'latest', fork: 'initial', family: true,
  initial: () => ({ data: '' }), checkpointWhen: () => true,
});
export const NativeExecutionProgress = defineDocFamily<{ snapshots: NativeJson[] }, null>({
  kind: 'qz.native-progress', version: 1, scope: 'conversation', history: 'latest', fork: 'initial', family: true,
  initial: () => ({ snapshots: [] }), checkpointWhen: () => true,
});
const nativeBindings = new AsyncLocalStorage<{ harness: Harness; conversationId: ConversationId }>();
/** Bind an explicit service request to its Harness; tasks use their own runtime. */
export function withNativeExecutionHost<T>(harness: Harness, conversationId: ConversationId, operation: () => T): T {
  return nativeBindings.run({ harness, conversationId }, operation);
}
type RemoteTaskInput = { target: NativeExecutionTarget; requestId: string; basis: string; command: string; request: JsonObject; ownerTaskId?: number; receiptRequired?: true };

/** Recover the result of the same invocation after its technical Task settled UNKNOWN.
 * Ordinary deletion/stop reconciliation remains state-only. This is driven by a live science Task. */
export async function reconcileNativeExecution(ownerTaskId: TaskId, runtime: ReceiptRuntime, nativeTaskId: TaskId<NativeJson>, context: Context): Promise<boolean> {
  const task = await runtime.getTask(nativeTaskId, context);
  const input = task?.input as unknown as RemoteTaskInput | undefined;
  if (!task || task.kind !== NativeExecutionTask.definition.name || task.conversationId !== runtime.conversationId || input?.ownerTaskId !== ownerTaskId) throw new Error('native_reconciliation_identity_conflict');
  const binding = await runtime.snapshot(NativeCalls, ownerTaskId, input.basis, context);
  if (binding?.taskId !== nativeTaskId) throw new Error('native_reconciliation_identity_conflict');
  if (binding.adoptedReceipt) {
    if (binding.adoptedReceipt.requestId !== input.requestId) throw new Error('native_reconciliation_identity_conflict');
    return true;
  }
  // Evidence downloads include durable page writes; a fixed 5s deadline causes
  // repeated UNKNOWN reads of valid large receipts. Caller/runtime owns cancellation.
  let state: NativeExecutionReceipt;
  try { state = await remoteNativeExecutionReceipt(input.target, input.requestId, context.abortSignal ? AbortSignal.any([runtime.signal, context.abortSignal]) : runtime.signal, tableCommands.has(input.command) ? { taskId: nativeTaskId, runtime, context } : undefined); }
  catch (error) { if (context.abortSignal?.aborted || runtime.signal.aborted) throw error; return false; }
  const native = state.status === 'COMPLETED' ? await retainNativeEvidence({ value: state.result }, async (index, data) => {
    await runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, runtime.conversationId, `${nativeTaskId}:${index}`, null)).data = data; return undefined; }, context);
  }) : null;
  await runtime.commit(async tx => {
    const call = await tx.doc(NativeCalls, ownerTaskId, input.basis, null);
    if (call.taskId !== nativeTaskId) throw new Error('native_reconciliation_identity_conflict');
    if (!call.adoptedReceipt) {
      if (state.status === 'COMPLETED') call.adoptedReceipt = { requestId: input.requestId, status: 'COMPLETED', native: native! };
      else if (state.status === 'FAILED' || state.status === 'CANCELLED') call.adoptedReceipt = { requestId: input.requestId, status: state.status, error: state.status === 'FAILED' ? state.error : 'nautilus_execution_cancelled' };
    }
    const facts = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(ownerTaskId), null);
    const reference = facts.requests.find(row => row.requestId === input.requestId && canonical(row.target ?? null) === canonical(input.target as unknown as JsonObject));
    if (!reference) throw new Error('native_reconciliation_reference_missing');
    reference.status = state.status;
    reference.error = state.status === 'FAILED' || state.status === 'UNKNOWN' ? state.error : null;
    reference.reconciledAt = new Date().toISOString();
    if (state.status === 'RUNNING' && state.progress) (await tx.doc(NativeExecutionProgress, runtime.conversationId, String(nativeTaskId), null)).snapshots = state.progress.snapshots;
    return undefined;
  }, context);
  return state.status === 'COMPLETED' || state.status === 'FAILED' || state.status === 'CANCELLED';
}

/** UNKNOWN native Tasks are already terminal, so owned-child abort alone cannot cancel them. */
export async function cancelUnsettledNativeExecutions(ownerTaskId: TaskId, runtime: ReceiptRuntime, context: Context): Promise<void> {
  const facts = await runtime.snapshot(NativeRemoteUncertainty, runtime.conversationId, String(ownerTaskId), withoutAbortSignal(context));
  for (const reference of facts?.requests ?? []) {
    if (['COMPLETED', 'FAILED', 'CANCELLED', 'REJECTED'].includes(String(reference.status))) continue;
    const target = validateNativeTarget(reference.target);
    const requestId = String(reference.requestId);
    let state: NativeExecutionReceipt;
    try { state = receipt(await remoteRequest(target, `v1/executions/${requestId}`, 'DELETE', undefined, AbortSignal.timeout(15_000)), requestId, true); }
    catch { state = { schemaVersion: 1, requestId, status: 'UNKNOWN', error: 'nautilus_cancellation_unknown' }; }
    await runtime.commit(async tx => {
      const current = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(ownerTaskId), null);
      const row = current.requests.find(item => item.requestId === requestId && canonical(item.target ?? null) === canonical(target as unknown as JsonObject));
      if (!row || ['COMPLETED', 'FAILED', 'CANCELLED', 'REJECTED'].includes(String(row.status))) return undefined;
      row.status = state.status;
      row.error = state.status === 'UNKNOWN' ? 'nautilus_cancellation_unknown' : state.status === 'FAILED' ? state.error : null;
      return undefined;
    }, withoutAbortSignal(context));
  }
}
async function recordExecution(task: { id: TaskId<NativeJson>; input: RemoteTaskInput }, runtime: TaskRuntime<RemoteTaskInput, { phase: 'submit' | 'poll' }, NativeJson, object>, state: NativeExecutionReceipt, context: Context) {
  const native = state.status === 'COMPLETED' ? await retainNativeEvidence({ value: state.result }, async (index, data) => {
    await runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, runtime.conversationId, `${task.id}:${index}`, null)).data = data; return undefined; }, context);
  }) : null;
  await runtime.commit(async tx => {
    const ownerId = task.input.ownerTaskId ?? task.id;
    const facts = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(ownerId), null);
    const row = { requestId: state.requestId, target: { ...task.input.target }, taskId: ownerId, status: state.status,
      error: state.status === 'UNKNOWN' ? 'nautilus_execution_unknown' : state.status === 'FAILED' ? state.error : null };
    const old = facts.requests.findIndex(item => item.requestId === state.requestId);
    const admissionRejection = facts.requests[old]?.admissionRejection;
    if (old < 0) facts.requests.push(row); else facts.requests[old] = { ...row, ...(admissionRejection ? { admissionRejection } : {}) };
    if (state.status === 'RUNNING') {
      if (state.progress) (await tx.doc(NativeExecutionProgress, runtime.conversationId, String(task.id), null)).snapshots = state.progress.snapshots;
      return { status: 'sleeping', until: runtime.now() + 1000, checkpoint: { phase: 'poll' } };
    }
    if (state.status === 'COMPLETED') return { status: 'terminal', outcome: { status: 'completed', result: { native: native! } } };
    return { status: 'terminal', outcome: { status: 'failed', error: { message: state.status === 'UNKNOWN' ? 'nautilus_execution_unknown' : state.status === 'FAILED' ? state.error : 'nautilus_execution_cancelled', detail: { requestId: state.requestId, target: { ...task.input.target } } } } };
  }, context);
}
export const NativeExecutionTask = defineTask<RemoteTaskInput, { phase: 'submit' | 'poll' }, NativeJson>({
  name: 'qz.native.execution', version: 1, initial: () => ({ phase: 'submit' }),
  phases: {
    async submit(task, runtime, context) {
      const stored = await readNativeEvidenceParts(task.input.request, async index => {
        const part = await runtime.snapshot(NativeExecutionRequestPart, runtime.conversationId, `${task.input.requestId}:${index}`, context);
        if (!part) throw new Error('native_request_part_missing');
        return part.data;
      });
      const request = stored.request as unknown as NativeExecutionRequest;
      if (request.requestId !== task.input.requestId || request.command !== task.input.command) throw new Error('native_request_identity_conflict');
      if (isProcessNativeTarget(task.input.target)) {
        const target = validateNativeTarget(task.input.target) as ProcessNativeTarget;
        if (request.implementationDigest !== processCapabilities(target).implementationDigest || !processCapabilities(target).commands.includes(request.command)) throw new Error('native_process_original_binding_conflict');
        // Commit away from submit BEFORE spawn. Recovery polls UNKNOWN, never replays this process.
        await runtime.commit(() => ({status:'running',checkpoint:{phase:'poll'}}),context);
        let state: NativeExecutionReceipt;
        try {
          const parser = tableCommands.has(task.input.command) ? tableReader({taskId:task.id,runtime,context}) : new JsonReader();
          const result = await runNativeProcess(request.requestId,target.binary,request.command,request.args,request.input,parser,runtime.signal);
          state={schemaVersion:1,requestId:request.requestId,status:'COMPLETED',result};
        } catch (error) {
          const diagnostic = error instanceof Error ? (error as Error & {nativeDiagnostic?:string}).nativeDiagnostic : undefined;
          if (diagnostic && !runtime.signal.aborted) await runtime.commit(async tx => { (await tx.doc(NativeEvidencePart,runtime.conversationId,`${task.id}:private-native-stderr`,null)).data=diagnostic; return undefined; },context);
          if (runtime.signal.aborted) throw new Error('native_process_execution_interrupted');
          state={schemaVersion:1,requestId:request.requestId,status:'UNKNOWN',error:'native_process_original_outcome_unknown'};
        }
        await recordExecution(task,runtime,state,context); return;
      }
      // A retry can be admitted before its response arrives, so retire the prior refusal before sending it.
      await runtime.commit(async tx => {
        const facts = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(task.input.ownerTaskId ?? task.id), null);
        const prior = facts.requests.find(row => row.requestId === task.input.requestId);
        if (prior?.status === 'REJECTED') { prior.status = 'PENDING'; prior.error = null; delete prior.admission; }
        return undefined;
      }, context);
      let state: NativeExecutionReceipt;
      try { state = receipt(await remoteRequest(task.input.target, 'v1/execute', 'POST', request as unknown as NativeJson, context.abortSignal, tableCommands.has(task.input.command) ? { taskId: task.id, runtime, context } : undefined), task.input.requestId); }
      catch (error) {
        if (context.abortSignal?.aborted) throw error;
        if (error instanceof RemoteAdmissionRejectedError) {
          await runtime.commit(async tx => {
            const ownerId = task.input.ownerTaskId ?? task.id;
            const facts = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(ownerId), null);
            const prior = facts.requests.find(row => row.requestId === task.input.requestId);
            const rejected = { requestId: task.input.requestId, target: { ...task.input.target }, taskId: ownerId, status: 'REJECTED', admission: 'REJECTED', error: error.rejection.error, admissionRejection: error.rejection.error };
            if (prior) Object.assign(prior, rejected); else facts.requests.push(rejected);
            if (error.rejection.error === 'nautilus_tick_capacity_exhausted') return { status: 'sleeping', until: runtime.now() + 5000, checkpoint: { phase: 'submit' } };
            return { status: 'terminal', outcome: { status: 'failed', error: { message: error.rejection.error, detail: { admission: 'REJECTED' } } } };
          }, context); return;
        }
        try { state = await remoteNativeExecutionReceipt(task.input.target, task.input.requestId, context.abortSignal, tableCommands.has(task.input.command) ? { taskId: task.id, runtime, context } : undefined); }
        catch { state = { schemaVersion: 1, requestId: task.input.requestId, status: 'UNKNOWN', error: 'nautilus_execution_outcome_unknown' }; }
      }
      await recordExecution(task, runtime, state, context);
    },
    async poll(task, runtime, context) {
      let state: NativeExecutionReceipt;
      try { state = await remoteNativeExecutionReceipt(task.input.target, task.input.requestId, context.abortSignal, tableCommands.has(task.input.command) ? { taskId: task.id, runtime, context } : undefined); }
      catch (error) {
        if (context.abortSignal?.aborted) throw error;
        // Only a receipt can establish an outcome. Losing observation does not
        // change identity, resubmit work, or assert physical cancellation.
        state = { schemaVersion: 1, requestId: task.input.requestId, status: 'UNKNOWN', error: 'nautilus_execution_outcome_unknown' };
      }
      await recordExecution(task, runtime, state, context);
    },
  },
  async abort(task, runtime, context) {
    if (task.state.checkpoint.phase === 'submit') {
      const facts = await runtime.snapshot(NativeRemoteUncertainty, runtime.conversationId, String(task.input.ownerTaskId ?? task.id), withoutAbortSignal(context));
      if (facts?.requests.some(row => row.requestId === task.input.requestId && row.status === 'REJECTED' && row.admission === 'REJECTED')) {
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), withoutAbortSignal(context));
        return;
      }
    }
    let state: NativeExecutionReceipt;
    try { state = receipt(await remoteRequest(task.input.target, `v1/executions/${task.input.requestId}`, 'DELETE', undefined, AbortSignal.timeout(15_000)), task.input.requestId, true); }
    catch (error) {
      state = { schemaVersion: 1, requestId: task.input.requestId, status: !task.input.receiptRequired && error instanceof Error && error.message === 'nautilus_http_404' ? 'CANCELLED' : 'UNKNOWN', error: 'nautilus_cancellation_unknown' } as NativeExecutionReceipt;
    }
    await runtime.commit(async tx => {
      const ownerId = task.input.ownerTaskId ?? task.id;
      const facts = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(ownerId), null);
      const prior = facts.requests.find(row => row.requestId === state.requestId);
      const row = { requestId: state.requestId, target: { ...task.input.target }, taskId: ownerId, status: state.status, error: state.status === 'UNKNOWN' ? 'nautilus_cancellation_unknown' : null };
      if (prior) Object.assign(prior, row); else facts.requests.push(row);
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, withoutAbortSignal(context));
  },
});

async function executeRemote(target: NativeExecutionTarget, command: string, input: NativeJson, originalArgs: string[], context: Context): Promise<NativeJson> {
  const owner = tracking.getStore(); const nativeHost = nativeBindings.getStore();
  if (!owner && !nativeHost) throw new Error('native_harness_required');
  const artifacts = { args: [...originalArgs], files: {} as Record<string, string> };
  // A committed business-stage cache can skip earlier calls after a restart.
  // Bind immutable native inputs, not an invocation-local call ordinal.
  const callsKey = canonicalJsonDigest({ target: target as unknown as NativeJson, command, input, ...artifacts });
  let taskId: TaskId<NativeJson> | null = null;
  if (owner) await owner.runtime.commit(async tx => { taskId = (await tx.doc(NativeCalls, owner.taskId, callsKey, null)).taskId; return undefined; }, context);
  if (taskId && owner) {
    const saved = await owner.runtime.getTask(taskId, context);
    const original = saved?.input as unknown as RemoteTaskInput | undefined;
    if (!original || original.command !== command || original.basis !== callsKey
      || canonicalJsonDigest(original.target as unknown as NativeJson) !== canonicalJsonDigest(target as unknown as NativeJson)) throw new Error('native_invocation_identity_conflict');
  }
  if (!taskId) {
    const capability = await remoteNativeCapabilities(target, context.abortSignal);
    if (owner?.scanContext?.implementationDigest !== undefined && capability.implementationDigest !== owner.scanContext.implementationDigest) throw new Error('nautilus_scan_implementation_changed');
    if (!capability.commands.includes(command)) throw new Error('nautilus_command_unsupported');
    const content = { schemaVersion: 1 as const, invocationId: randomUUID(), implementationDigest: capability.implementationDigest, command, input, ...artifacts };
    const request: NativeExecutionRequest = { ...content, requestId: canonicalJsonDigest(content) };
    const writer = owner?.runtime ?? nativeHost!.harness;
    const conversationId = owner?.runtime.conversationId ?? nativeHost!.conversationId;
    const retained = await retainNativeEvidence({ request: request as unknown as JsonObject }, async (index, data) => {
      await writer.commit(async tx => { (await tx.doc(NativeExecutionRequestPart, conversationId, `${request.requestId}:${index}`, null)).data = data; return undefined; }, context);
    });
    const frozen: RemoteTaskInput = { target, requestId: request.requestId, basis: callsKey, command, request: retained,
      ...(owner ? { ownerTaskId: owner.taskId } : {}), ...(owner && (owner.scanContext || command === 'native-strategy-backtest' || command === 'native-configured-backtest') ? { receiptRequired: true } : {}) };
    if (owner) await owner.runtime.commit(async tx => {
      const call = await tx.doc(NativeCalls, owner.taskId, callsKey, null);
      let selectedRequestId = request.requestId;
      if (call.taskId !== null) {
        // Concurrent identical callers converge on the committed winner. Never
        // retain an uncertainty reference for a candidate that was not sent.
        const selected = await tx.task(call.taskId);
        if (!selected) throw new Error('native_invocation_missing');
        selectedRequestId = (selected.input as unknown as RemoteTaskInput).requestId;
      } else call.taskId = await tx.createTask(NativeExecutionTask, frozen, { ownership: { kind: 'task', taskId: owner.taskId } });
      taskId = call.taskId;
      await tx.doc(NativeExecutionProgress, owner.runtime.conversationId, String(taskId), null);
      const facts = await tx.doc(NativeRemoteUncertainty, owner.runtime.conversationId, String(owner.taskId), null);
      if (!facts.requests.some(row => row.requestId === selectedRequestId)) facts.requests.push({ requestId: selectedRequestId, target: { ...target }, taskId: owner.taskId, status: 'PENDING', error: 'nautilus_execution_unknown' });
      return undefined;
    }, context);
    else taskId = await nativeHost!.harness.commit(async tx => {
      const id = await tx.createTask(NativeExecutionTask, frozen, { conversationId: nativeHost!.conversationId, ownership: { kind: 'conversation' } });
      await tx.doc(NativeExecutionProgress, nativeHost!.conversationId, String(id), null);
      return id;
    }, context);
  }
  const observer = progressTracking.getStore();
  const progressReader = owner?.runtime ?? nativeHost?.harness;
  const progress = observer && progressReader ? await progressReader.watchDoc(NativeExecutionProgress, owner?.runtime.conversationId ?? nativeHost!.conversationId, String(taskId), context) : undefined;
  if (progress && observer) progress.start(async call => { if (call.kind === 'value' && call.value?.snapshots.length) await observer(call.value.snapshots); });
  try {
    const task = owner ? await owner.runtime.waitForTask(taskId!, context) : await nativeHost!.harness.waitForTask(taskId!, context);
    if (owner) {
      const adopted = (await owner.runtime.snapshot(NativeCalls, owner.taskId, callsKey, context))?.adoptedReceipt;
      if (adopted) {
        if (adopted.requestId !== (task.input as unknown as RemoteTaskInput).requestId) throw new Error('native_reconciliation_identity_conflict');
        if (adopted.status !== 'COMPLETED') throw new Error(adopted.error);
        const result = await readNativeEvidence(owner.runtime, task.conversationId, task.id, adopted.native, context, tableCommands.has(command) ? async (key, data) => { await owner.runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, task.conversationId, key, null)).data = data; return undefined; }, context); } : undefined);
        return result.value!;
      }
    }
    if (task.state.outcome.status === 'completed') {
      const reader = owner?.runtime ?? nativeHost!.harness;
      const stored = (task.state.outcome.result as JsonObject).native as JsonObject;
      const result = await readNativeEvidence(reader, task.conversationId, task.id, stored, context, tableCommands.has(command) ? async (key, data) => {
        const commit = async (tx: Tx) => { (await tx.doc(NativeEvidencePart, task.conversationId, key, null)).data = data; return undefined; };
        await reader.commit(commit, context);
      } : undefined);
      return result.value!;
    }
    const error = task.state.outcome.status === 'failed' || task.state.outcome.status === 'faulted' ? task.state.outcome.error : undefined;
    if (error?.message === 'nautilus_execution_unknown') throw new RemoteUncertaintyError('nautilus_execution_unknown', (task.input as unknown as RemoteTaskInput).requestId, target, undefined, task.id);
    if (error?.detail && typeof error.detail === 'object' && !Array.isArray(error.detail) && error.detail.admission === 'REJECTED') throw new RemoteAdmissionRejectedError({ schemaVersion: 1, requestId: (task.input as unknown as RemoteTaskInput).requestId, admission: 'REJECTED', error: error.message } as NativeExecutionRejection);
    throw new Error(error?.message ?? 'nautilus_execution_cancelled');
  } finally { await progress?.stop(); }
}

/** Shared transport for the research and service scientific entry points. */
export async function executeNativeValue(target: NativeTarget, command: string, input: NativeJson, args: string[], directory: string, context: Context): Promise<NativeJson> {
  const interceptor = nativeInterceptor.getStore();
  const execute = () => executeNativeUnwrapped(target, command, input, args, directory, context);
  return interceptor ? interceptor(command, input, execute) : execute();
}

async function executeNativeUnwrapped(target: NativeTarget, command: string, input: NativeJson, args: string[], directory: string, context: Context): Promise<NativeJson> {
  if (!nativeCommands.has(command)) throw new Error('native_command_unsupported');
  if (typeof target !== 'string') {
    try { return await executeRemote(target, command, input, args, context); }
    catch (error) { throw new ExecutionFailure(error, { operation: command, stage: 'execute' }); }
  }
  if (process.env.QZ_REQUIRE_REMOTE_NAUTILUS === '1') throw new Error('local_nautilus_target_disabled');
  if (!target) throw new Error('native_executable_required');
  const inputPath = join(directory, 'request.json'); await writeFile(inputPath, jsonChunks(input));
  const env = new NodeExecutionEnv({ cwd: directory });
  const observer = progressTracking.getStore();
  const progressPath = observer && command === 'simulate' ? join(directory, `progress-${randomUUID()}.ndjson`) : undefined;
  let progressRead = Promise.resolve(); let progressBusy = false;
  const timer = progressPath ? globalThis.setInterval(() => {
    if (progressBusy) return; progressBusy = true;
    progressRead = (async () => {
      try {
        const rows = (await readFile(progressPath, 'utf8')).split('\n'); rows.pop();
        await observer!(rows.filter(Boolean).map(row => (JSON.parse(row) as { snapshot: NativeJson }).snapshot));
      } catch { /* Optional observation cannot change the original scientific outcome. */ }
      finally { progressBusy = false; }
    })();
  }, 1000) : undefined;
  const stdout = new JsonReader(); let parseError: unknown; let stderr = ''; let stage: 'execute' | 'decode' = 'execute';
  try {
    const result = getOrThrow(await env.exec(['bash', '-c', 'file="$1"; shift; exec "$@" < "$file"', 'qz-native', inputPath, target, command, ...args], {
      inheritEnv: false, env: { PATH: '/usr/bin:/bin', TMPDIR: directory, ...(progressPath ? { QZ_PROGRESS_PATH: progressPath } : {}) },
      onOutput: (chunk, _ctx, info) => { if (info.stream === 'stderr') stderr += chunk; else if (parseError === undefined) { try { stdout.write(chunk); } catch (error) { parseError = error; } } },
    }, context));
    if (result.exitCode !== 0) throw new Error(stderr.trim() || `native_${command}_failed`);
    stage = 'decode'; if (parseError !== undefined) throw parseError; return stdout.finish();
  } catch (error) { throw new ExecutionFailure(error, { operation: command, stage }); }
  finally { if (timer) clearInterval(timer); await progressRead; await env.cleanup(withoutAbortSignal(context)); }
}

/** Typed object projection only; execution and receipt ownership stay in the native adapter. */
export async function executeNative(binary: NativeTarget, command: string, input: JsonObject, args: string[], directory: string, context: Context): Promise<JsonObject> {
  const result = await executeNativeValue(binary, command, input, args, directory, context);
  if (!result || typeof result !== 'object' || Array.isArray(result)) throw new Error('native_result');
  return result;
}
