import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { defineDocFamily, type JsonObject, type TaskId, type TaskRuntime } from '@earendil-works/pi-durable';
import { randomUUID } from 'node:crypto';
import { AsyncLocalStorage } from 'node:async_hooks';
import { lstat, readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { setTimeout } from 'node:timers/promises';
import { Agent, fetch } from 'undici';
import { isTickNativeCommand } from '../../../packages/contracts/src/nautilus.ts';
import type { NativeCapabilities, NativeCapabilitiesV2, NativeExecutionReceipt, NativeExecutionReceiptV2, NativeExecutionRequest, NativeExecutionRequestV2, NativeExecutionRejection, NativeJson, NativeTarget, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { ExecutionFailure } from './execution-errors.ts';
import { JsonReader, jsonChunks, canonicalJsonDigest } from './native-json.ts';

export const nativeCommands = new Set(['review-science', 'review-source', 'review-allocate', 'evaluate-factor', 'evaluate-tick-factor', 'replay-tick-economic', 'forecast', 'forecast-factor', 'evaluate-experiment', 'simulate', 'replay-catalog-ensemble', 'study-portfolio', 'allocate', 'blend-targets', 'assess-capacity', 'size-account', 'validate-capital-exit', 'fit-ensemble', 'apply-ensemble', 'capital-curve', 'freeze-ensemble-calibrations', 'compare-observed-spot', 'study-observed-spot', 'value-observed-spot', 'build-observed-spot', 'catalog-prefix-digest', 'catalog-factor-scope', 'scan-factor-catalog']);
type NativeInterceptor = (command: string, input: NativeJson, next: () => Promise<NativeJson>) => Promise<NativeJson>;
const nativeInterceptor = new AsyncLocalStorage<NativeInterceptor>();
/** Scoped checkpoint adapter for a fixed review computation; other science is unchanged. */
export function withNativeExecutionInterceptor<T>(interceptor: NativeInterceptor, operation: () => Promise<T>): Promise<T> { return nativeInterceptor.run(interceptor, operation); }
const nativeStep = new AsyncLocalStorage<string>();
const nativeObservation = new AsyncLocalStorage<{ command: string; purpose: string }>();
// Provenance observations intentionally inspect current bytes. Scientific
// computations and effectful commands must retain their original step result.
const freshReadCommands = new Set(['catalog-prefix-digest']);
export function withFreshNativeRead<T>(command: string, purpose: string, operation: () => Promise<T>): Promise<T> {
  if (!freshReadCommands.has(command)) throw new Error('native_fresh_read_unsupported');
  if (!purpose || purpose.length > 512 || /[\u0000-\u001f]/.test(purpose)) throw new Error('native_read_purpose_invalid');
  return nativeObservation.run({ command, purpose }, operation);
}
/** Stable business identity within an original Task. Nested adapters add their
 * member/fold/leg identity; parallel completion order never determines a key. */
export function withNativeStep<T>(stepKey: string, operation: () => Promise<T>): Promise<T> {
  if (!stepKey || stepKey.length > 512 || /[\u0000-\u001f]/.test(stepKey)) throw new Error('native_step_key_invalid');
  const parent = nativeStep.getStore();
  return nativeStep.run(parent ? `${parent}/${encodeURIComponent(stepKey)}` : encodeURIComponent(stepKey), operation);
}
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
  kind: 'qz.native-remote-uncertainty', version: 1, scope: 'conversation', history: 'latest', fork: 'initial', family: true, initial: () => ({ requests: [] }),
});
/** Immutable request and collected receipt facts in the existing official
 * store. This is not a task queue; Pi Durable owns execution and retry. */
export const NativeSteps = defineDocFamily<{ steps: Record<string, JsonObject>; observations?: Record<string, { next: number; active: number | null }> }, null>({
  kind: 'qz.native-steps', version: 1, scope: 'conversation', history: 'latest', fork: 'initial', family: true, initial: () => ({ steps: {} }),
});
class RemoteHttpError extends Error {
  readonly status: number;
  readonly evidence?: NativeJson;
  constructor(status: number, evidence?: NativeJson) { super(`nautilus_http_${status}`); this.status = status; this.evidence = evidence; }
}
class RemoteUncertaintyError extends Error {
  readonly reference: JsonObject;
  constructor(code: 'nautilus_execution_unknown' | 'nautilus_cancellation_unknown', requestId: string, target: RemoteNativeTarget, cause?: unknown) {
    super(`${code}:${requestId}`, { cause });
    this.reference = { requestId, target: { ...target }, error: code };
  }
}
class RemoteAdmissionRejectedError extends Error {
  readonly rejection: NativeExecutionRejection;
  constructor(rejection: NativeExecutionRejection) { super(rejection.error); this.rejection = rejection; }
}

type ReceiptRuntime = Pick<TaskRuntime<unknown, unknown, unknown, object>, 'commit' | 'conversationId' | 'signal'>
  & Partial<Pick<TaskRuntime<unknown, unknown, unknown, object>, 'sleep' | 'now'>>;
const tracking = new AsyncLocalStorage<{ taskId: TaskId; runtime: ReceiptRuntime; context: Context }>();
const detached = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

/** Bind transport receipt writes to the original official task; this has no
 * queue or execution state of its own. Register before unsafe network waits. */
export function withNativeRemoteTracking<T>(taskId: TaskId, runtime: ReceiptRuntime, context: Context, operation: () => Promise<T>): Promise<T> {
  return tracking.run({ taskId, runtime, context }, operation);
}

/** A legacy reservation can only be re-entered when its official task already
 * retained a protected original invocation. No old V1 identity is invented. */
export async function hasNativeStepRecovery(taskId: TaskId, runtime: ReceiptRuntime, context: Context, match?: { command: string; input: NativeJson }): Promise<boolean> {
  let protectedStep = false;
  const scope = nativeStep.getStore();
  await runtime.commit(async tx => {
    const facts = await tx.doc(NativeSteps, runtime.conversationId, String(taskId), null);
    protectedStep = Object.entries(facts.steps).some(([key, step]) => {
      const request = step.request as JsonObject | undefined;
      return request?.schemaVersion === 2 && (!scope || key.startsWith(`${scope}/`))
        && (!match || request.command === match.command && canonicalJsonDigest(request.input as NativeJson) === canonicalJsonDigest(unmeteredNativeInput(match.input)));
    });
    return undefined;
  }, context);
  return protectedStep;
}

async function retainRemoteReference(reference: JsonObject, owner: { taskId: TaskId; runtime: ReceiptRuntime; context: Context }): Promise<void> {
  await owner.runtime.commit(async tx => {
    const facts = await tx.doc(NativeRemoteUncertainty, owner.runtime.conversationId, String(owner.taskId), null);
    const existing = facts.requests.findIndex(item => item.requestId === reference.requestId);
    const next = { ...(existing < 0 ? {} : facts.requests[existing]), ...reference, taskId: owner.taskId };
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
  const recovery = item.protocolVersion !== undefined || item.instanceId !== undefined || item.recoveryProfileId !== undefined;
  if (recovery && (item.protocolVersion !== 2 || typeof item.instanceId !== 'string' || !item.instanceId || typeof item.recoveryProfileId !== 'string' || !item.recoveryProfileId)) throw new Error('nautilus_recovery_binding_invalid');
  return { id: item.id, url: item.url, ...(typeof item.tokenEnv === 'string' ? { tokenEnv: item.tokenEnv } : {}), ...(typeof item.credentialRef === 'string' ? { credentialRef: item.credentialRef } : {}),
    ...(recovery ? { protocolVersion: 2 as const, instanceId: item.instanceId as string, recoveryProfileId: item.recoveryProfileId as string } : {}) };
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
  if (data.nautilusTarget !== undefined) return validateRemoteTarget(data.nautilusTarget);
  if (process.env.QZ_REQUIRE_REMOTE_NAUTILUS === '1') throw new Error('legacy_local_nautilus_task_requires_migration');
  return defaultNativeTarget();
}

async function remoteRequest(target: RemoteNativeTarget, path: string, method: 'GET' | 'POST' | 'DELETE', body?: NativeJson, signal?: AbortSignal): Promise<NativeJson> {
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
      } else if (method === 'GET' && path.startsWith('v2/executions/') && response.status === 404) {
        // Absence authorizes an exact request replay only if the original
        // instance itself supplies matching identity evidence.
        let encoded = ''; let size = 0;
        if (response.body) for await (const bytes of response.body) { size += bytes.length; if (size > 4096) break; encoded += Buffer.from(bytes).toString('utf8'); }
        let evidence: NativeJson | undefined;
        if (size <= 4096) { try { evidence = JSON.parse(encoded) as NativeJson; } catch { /* An unproven 404 remains unknown. */ } }
        throw new RemoteHttpError(404, evidence);
      } else await response.body?.cancel();
      throw new RemoteHttpError(response.status);
    }
    if (!response.body) throw new Error('nautilus_response_missing');
    const parser = new JsonReader(); const decoder = new TextDecoder();
    for await (const bytes of response.body) parser.write(decoder.decode(bytes, { stream: true }));
    parser.write(decoder.decode()); return parser.finish();
  } finally { await dispatcher.close(); }
}

export async function remoteNativeCapabilities(target: RemoteNativeTarget, signal?: AbortSignal): Promise<NativeCapabilities> {
  const value = await remoteRequest(target, 'v1/capabilities', 'GET', undefined, signal) as unknown as NativeCapabilities;
  if (value?.schemaVersion !== 1 || typeof value.implementationDigest !== 'string' || !/^sha256:[a-f0-9]{64}$/.test(value.implementationDigest)
    || !Array.isArray(value.commands) || !value.commands.every(command => typeof command === 'string')) throw new Error('nautilus_capabilities_invalid');
  return value;
}

export async function remoteNativeCapabilitiesV2(target: RemoteNativeTarget, signal?: AbortSignal): Promise<NativeCapabilitiesV2> {
  const value = await remoteRequest(target, 'v2/capabilities', 'GET', undefined, signal) as unknown as NativeCapabilitiesV2;
  if (value?.schemaVersion !== 2 || typeof value.instanceId !== 'string' || !value.instanceId
    || typeof value.implementationDigest !== 'string' || !/^sha256:[a-f0-9]{64}$/.test(value.implementationDigest)
    || !Array.isArray(value.commands) || !value.commands.every(command => typeof command === 'string')
    || value.recovery !== null && (!value.recovery || typeof value.recovery.profileId !== 'string'
      || !Array.isArray(value.recovery.verifiedCommands) || !value.recovery.verifiedCommands.every(command => typeof command === 'string')
      || !Number.isSafeInteger(value.recovery.checkpointIntervalMs) || value.recovery.checkpointIntervalMs <= 0)) throw new Error('nautilus_capabilities_invalid');
  return value;
}

type RemoteReceipt = NativeExecutionReceipt | NativeExecutionReceiptV2;
type RemoteExecutionRequest = NativeExecutionRequest | NativeExecutionRequestV2;

function receipt(value: NativeJson, requestId: string): NativeExecutionReceipt {
  const result = value as unknown as NativeExecutionReceipt;
  if (result?.schemaVersion !== 1 || result.requestId !== requestId || !['COMPLETED', 'FAILED', 'CANCELLED', 'RUNNING', 'UNKNOWN'].includes(result.status)
    || result.status === 'COMPLETED' && !Object.hasOwn(result, 'result')
    || ['FAILED', 'UNKNOWN'].includes(result.status) && typeof (result as { error?: unknown }).error !== 'string') throw new Error('nautilus_receipt_invalid');
  return result;
}

function remoteReceipt(value: NativeJson, requestId: string, target: RemoteNativeTarget): RemoteReceipt {
  if (target.protocolVersion !== 2) return receipt(value, requestId);
  const result = value as unknown as NativeExecutionReceiptV2;
  if (result?.schemaVersion !== 2 || result.instanceId !== target.instanceId || result.requestId !== requestId
    || !Number.isSafeInteger(result.taskId) || result.taskId < 0 || !result.recovery || result.recovery.profileId !== target.recoveryProfileId
    || typeof result.recovery.phase !== 'string' || !result.recovery.phase
    || result.recovery.generation !== null && (!Number.isSafeInteger(result.recovery.generation) || result.recovery.generation < 0)
    || result.recovery.checkpointAt !== null && typeof result.recovery.checkpointAt !== 'string'
    || result.recovery.blockedReason !== null && typeof result.recovery.blockedReason !== 'string'
    || !['ACCEPTED', 'RUNNING', 'WAITING', 'COMPLETED', 'FAILED', 'CANCELLED'].includes(result.status)
    || result.status === 'COMPLETED' && !Object.hasOwn(result, 'result')
    || result.status === 'FAILED' && typeof result.error !== 'string') throw new Error('nautilus_receipt_invalid');
  return result;
}

/** Read only the receipt for this original invocation and retained instance.
 * This does not submit or cancel any remote execution. */
export async function remoteNativeExecutionReceipt(target: RemoteNativeTarget, requestId: string, signal?: AbortSignal): Promise<NativeExecutionReceipt> {
  if (!/^[a-f0-9]{64}$/.test(requestId)) throw new Error('nautilus_request_id_invalid');
  const result = remoteReceipt(await remoteRequest(target, `v${target.protocolVersion ?? 1}/executions/${requestId}`, 'GET', undefined, signal), requestId, target);
  // Existing consumers use the V1 terminal shape. V2 recovery phases remain
  // RUNNING here; the V2 endpoint retains the richer official-task projection.
  return result.schemaVersion === 1 ? result : { schemaVersion: 1, requestId, ...('result' in result ? { status: 'COMPLETED' as const, result: result.result }
    : 'error' in result ? { status: 'FAILED' as const, error: result.error }
    : { status: result.status === 'CANCELLED' ? 'CANCELLED' as const : 'RUNNING' as const }) };
}

/** Poll only an original receipt. Unknown/missing outcomes never authorize a new POST. */
export async function waitForRemoteNativeExecutionReceipt(target: RemoteNativeTarget, requestId: string, signal?: AbortSignal,
  initial?: NativeExecutionReceipt): Promise<NativeExecutionReceipt> {
  try {
    let state = initial ?? await remoteNativeExecutionReceipt(target, requestId, signal);
    while (state.status === 'RUNNING') {
      await setTimeout(100, undefined, { signal });
      state = await remoteNativeExecutionReceipt(target, requestId, signal);
    }
    if (state.status === 'UNKNOWN') throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target);
    return state;
  } catch (error) {
    if (signal?.aborted || error instanceof RemoteUncertaintyError) throw error;
    throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target, error);
  }
}

async function attachments(original: string[]): Promise<{ args: string[]; files: Record<string, string> }> {
  const args = [...original]; const files: Record<string, string> = {}; let index = 0;
  async function attach(path: string, name: string) {
    if (!(await lstat(path)).isFile()) throw new Error('nautilus_artifact_file_required');
    files[name] = (await readFile(path)).toString('base64');
  }
  for (let position = 0; position < args.length; position++) {
    const flag = args[position];
    if (!['--model', '--scope', '--objects'].includes(flag!)) continue;
    const path = args[++position]; if (!path) throw new Error('nautilus_artifact_argument_missing');
    if (flag === '--objects') {
      const prefix = `objects-${index++}`;
      for (const entry of (await readdir(path, { withFileTypes: true })).sort((left, right) => left.name.localeCompare(right.name))) {
        // Only native UUID objects are consumed; request.json and temporary
        // models in the same scratch directory are not object-store inputs.
        if (!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(entry.name)) continue;
        await attach(join(path, entry.name), `${prefix}/${entry.name}`);
      }
      args[position] = `@objects:${prefix}`;
    } else {
      const name = `${flag.slice(2)}-${index++}`; await attach(path, name); args[position] = `@file:${name}`;
    }
  }
  return { args, files };
}

function stepInputDigest(target: RemoteNativeTarget, command: string, input: NativeJson, args: string[]): string {
  // Attachment paths are phase-local scratch. Their bytes are frozen into the
  // original request; recreating a scratch path must not create another step.
  const normalized = [...args];
  for (let i = 0; i < normalized.length; i++) if (['--model', '--scope', '--objects'].includes(normalized[i]!)) {
    if (!normalized[i + 1]) throw new Error('nautilus_artifact_argument_missing');
    normalized[++i] = '@original-attachment';
  }
  return canonicalJsonDigest({ target: target as unknown as NativeJson, command, input, args: normalized });
}

async function remotePause(owner: NonNullable<ReturnType<typeof tracking.getStore>>, context: Context): Promise<void> {
  owner.runtime.signal.throwIfAborted();
  if (owner.runtime.sleep && owner.runtime.now) await owner.runtime.sleep(owner.runtime.now() + 1000, context);
  else await setTimeout(1000, undefined, { signal: context.abortSignal ?? owner.runtime.signal });
}
function retryableRemoteFailure(error: unknown): boolean {
  if (error instanceof RemoteHttpError) return [401, 403, 404, 408, 409, 429].includes(error.status) || error.status >= 500;
  return error instanceof RemoteUncertaintyError || error instanceof TypeError || error instanceof Error &&
    (['fetch failed', 'nautilus_receipt_invalid', 'nautilus_capabilities_invalid', 'nautilus_credentials_missing', 'nautilus_recovery_profile_unavailable'].includes(error.message)
      || /^(?:ECONN|EHOST|ENET|ETIMEDOUT|UND_ERR_)/.test(error.message)
      || error.cause instanceof Error && /^(?:ECONN|EHOST|ENET|ETIMEDOUT|UND_ERR_)/.test(String((error.cause as NodeJS.ErrnoException).code)));
}

async function executeRemote(target: RemoteNativeTarget, command: string, input: NativeJson, originalArgs: string[], context: Context): Promise<NativeJson> {
  target = validateRemoteTarget(target);
  const owner = tracking.getStore();
  const explicitKey = nativeStep.getStore();
  const observation = nativeObservation.getStore();
  const legacyFreshRead = observation !== undefined && target.protocolVersion !== 2;
  const bindingDigest = stepInputDigest(target, command, input, originalArgs);
  // The official Task itself supplies the root identity. Repeated independent
  // calls with identical arguments require explicit semantic child keys.
  if (target.protocolVersion === 2 && !owner) throw new Error('native_durable_step_required');
  let artifacts: Awaited<ReturnType<typeof attachments>> | undefined;
  if (!explicitKey) artifacts = await attachments(originalArgs);
  let stepKey = explicitKey ? `${explicitKey}/${encodeURIComponent(command)}` : `request:${command}:${canonicalJsonDigest({ input, ...artifacts! })}`;
  const observationKey = observation ? `${explicitKey ?? 'task'}/${encodeURIComponent(observation.purpose)}/${bindingDigest}` : undefined;
  let observationGeneration: number | undefined;
  let stored: JsonObject | undefined;
  if (owner && !legacyFreshRead) await owner.runtime.commit(async tx => {
    const facts = await tx.doc(NativeSteps, owner.runtime.conversationId, String(owner.taskId), null);
    if (observationKey) {
      facts.observations ??= {};
      if (!facts.observations[observationKey]) facts.observations[observationKey] = { next: 0, active: null };
      const cursor = facts.observations[observationKey]!;
      if (cursor.active === null) {
        if (!Number.isSafeInteger(cursor.next) || cursor.next >= Number.MAX_SAFE_INTEGER) throw new Error('native_read_generation_exhausted');
        cursor.active = cursor.next++;
      }
      observationGeneration = cursor.active;
      stepKey = `observation:${observationKey}/${observationGeneration}/${command}`;
    }
    const steps = facts.steps;
    const original = steps[stepKey];
    stored = original ? detached(original) : undefined;
    if (stored && stored.bindingDigest !== bindingDigest) throw new Error('native_step_identity_conflict');
    if (!stored && Object.keys(steps).length === 0) {
      const legacy = await tx.doc(NativeRemoteUncertainty, owner.runtime.conversationId, String(owner.taskId), null);
      if (legacy.requests.some(reference => !freshReadCommands.has(String(reference.observation)))) throw new Error('native_legacy_step_requires_reconciliation');
    }
    return undefined;
  }, context);
  if (stored && explicitKey) {
    try {
      const current = await attachments(originalArgs);
      const original = stored.request as unknown as RemoteExecutionRequest;
      if (canonicalJsonDigest(current) !== canonicalJsonDigest({ args: original.args, files: original.files })) throw new Error('native_step_identity_conflict');
    } catch (error) {
      // Recreated scratch may have been removed after its bytes were retained.
      // Any present replacement must still match the original frozen request.
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
  let fresh = !stored;
  if (!stored) {
    let implementationDigest: string;
    for (;;) {
      try {
        const capability = target.protocolVersion === 2 ? await remoteNativeCapabilitiesV2(target, context.abortSignal) : await remoteNativeCapabilities(target, context.abortSignal);
        if (!capability.commands.includes(command)) throw new Error('nautilus_command_unsupported');
        if (capability.schemaVersion === 2 && (capability.instanceId !== target.instanceId || !capability.recovery || capability.recovery.profileId !== target.recoveryProfileId
          || !capability.recovery.verifiedCommands.includes(command))) throw new Error('nautilus_recovery_profile_unavailable');
        implementationDigest = capability.implementationDigest;
        break;
      } catch (error) {
        if (!owner || target.protocolVersion !== 2 || context.abortSignal?.aborted || owner.runtime.signal.aborted || !retryableRemoteFailure(error)) throw error;
        await remotePause(owner, context);
      }
    }
    artifacts ??= await attachments(originalArgs);
    const content = { schemaVersion: target.protocolVersion === 2 ? 2 as const : 1 as const, invocationId: randomUUID(), implementationDigest, command, input, ...artifacts,
      ...(target.protocolVersion === 2 ? { instanceId: target.instanceId!, recoveryProfileId: target.recoveryProfileId! } : {}) };
    const request = { ...content, requestId: canonicalJsonDigest(content as unknown as NativeJson) };
    stored = { bindingDigest, request: request as unknown as JsonObject, target: target as unknown as JsonObject };
    if (owner && !legacyFreshRead) await owner.runtime.commit(async tx => {
      const facts = await tx.doc(NativeSteps, owner.runtime.conversationId, String(owner.taskId), null);
      const previous = facts.steps[stepKey];
      if (previous) {
        if (previous.bindingDigest !== bindingDigest) throw new Error('native_step_identity_conflict');
        const original = previous.request as JsonObject;
        const semantic = (value: JsonObject) => Object.fromEntries(Object.entries(value).filter(([key]) => key !== 'invocationId' && key !== 'requestId')) as NativeJson;
        if (canonicalJsonDigest(semantic(original)) !== canonicalJsonDigest(semantic(request as unknown as JsonObject))) throw new Error('native_step_identity_conflict');
        stored = detached(previous); fresh = false;
      } else facts.steps[stepKey] = stored!;
      return undefined;
    }, context);
  }
  const request = stored.request as unknown as RemoteExecutionRequest;
  if (!request || request.command !== command || request.requestId !== canonicalJsonDigest(Object.fromEntries(Object.entries(request).filter(([key]) => key !== 'requestId')) as NativeJson)) throw new Error('native_step_request_invalid');
  const requestId = request.requestId;
  const path = `v${request.schemaVersion}/executions/${requestId}`;
  const reference: JsonObject = { requestId, target: { ...target }, error: 'nautilus_execution_unknown',
    ...(observation ? { observation: observation.command, purpose: observation.purpose } : {}), ...(request.schemaVersion === 2 ? { protocolVersion: 2, stepKey } : {}) };
  if (owner) await retainRemoteReference({ ...reference, status: 'PENDING' }, owner);
  context.abortSignal?.throwIfAborted();
  const observer = progressTracking.getStore(); let progressRead = Promise.resolve(); let progressBusy = false;
  const progressAbort = new AbortController();
  const progressSignal = context.abortSignal ? AbortSignal.any([context.abortSignal, progressAbort.signal]) : progressAbort.signal;
  const timer = observer && ['simulate', 'replay-catalog-ensemble'].includes(command) ? globalThis.setInterval(() => {
    if (progressBusy) return; progressBusy = true;
    progressRead = (async () => {
      try { const state = await remoteNativeExecutionReceipt(target, requestId, progressSignal); if (state.status === 'RUNNING' && state.progress) await observer(state.progress.snapshots); }
      catch { /* Observation cannot change scientific execution. */ }
      finally { progressBusy = false; }
    })();
  }, 1000) : undefined;
  try {
    let state = stored.receipt as unknown as RemoteReceipt | undefined;
    let submit = fresh;
    while (!state || !['COMPLETED', 'FAILED', 'CANCELLED'].includes(state.status)) {
      try {
        if (submit) {
          submit = false;
          state = remoteReceipt(await remoteRequest(target, `v${request.schemaVersion}/execute`, 'POST', request as unknown as NativeJson, context.abortSignal), requestId, target);
        } else state = remoteReceipt(await remoteRequest(target, path, 'GET', undefined, context.abortSignal), requestId, target);
        if (state.status === 'UNKNOWN') throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target);
        if (['COMPLETED', 'FAILED', 'CANCELLED'].includes(state.status)) break;
        if (owner && target.protocolVersion === 2) await remotePause(owner, context);
        else await setTimeout(100, undefined, { signal: context.abortSignal });
      } catch (error) {
        if (error instanceof RemoteAdmissionRejectedError) {
          if (owner && !owner.runtime.signal.aborted) await retainRemoteReference({ ...reference, status: 'REJECTED', admission: 'REJECTED', error: error.rejection.error }, owner);
          throw error;
        }
        if (context.abortSignal?.aborted || owner?.runtime.signal.aborted) throw error;
        if (target.protocolVersion === 2 && error instanceof RemoteHttpError && error.status === 404) {
          const evidence = error.evidence as JsonObject | undefined;
          if (evidence?.schemaVersion === 2 && evidence.instanceId === target.instanceId && evidence.requestId === requestId && evidence.error === 'nautilus_execution_not_found') {
            submit = true;
            continue;
          }
        }
        if (!owner || target.protocolVersion !== 2) {
          // A lost V1 response is GET-reconciled once, never blind-resubmitted.
          if (error instanceof RemoteHttpError && error.status >= 400 && error.status < 500 && error.status !== 429) throw error;
          try { state = remoteReceipt(await remoteRequest(target, path, 'GET', undefined, context.abortSignal), requestId, target); }
          catch { throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target, error); }
          if (state.status === 'UNKNOWN') throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target);
          continue;
        }
        if (!retryableRemoteFailure(error)) throw error;
        state = undefined;
        await remotePause(owner, context);
      }
    }
    if (!state) throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target);
    if (owner && !owner.runtime.signal.aborted) {
      if (!legacyFreshRead) await owner.runtime.commit(async tx => {
        const step = (await tx.doc(NativeSteps, owner.runtime.conversationId, String(owner.taskId), null)).steps[stepKey];
        if (!step || (step.request as JsonObject).requestId !== requestId) throw new Error('native_step_identity_conflict');
        step.receipt = state as unknown as JsonObject;
        if (observationKey) {
          const facts = await tx.doc(NativeSteps, owner.runtime.conversationId, String(owner.taskId), null);
          const cursor = facts.observations?.[observationKey];
          if (!cursor || observationGeneration === undefined || cursor.next <= observationGeneration) throw new Error('native_read_generation_conflict');
          if (cursor.active === observationGeneration) cursor.active = null;
        }
        return undefined;
      }, context);
      await retainRemoteReference({ ...reference, status: state.status, error: state.status === 'FAILED' ? 'nautilus_execution_failed' : null }, owner);
    }
    if (state.status === 'COMPLETED') return state.result;
    if (state.status === 'CANCELLED') throw new Error(`nautilus_execution_cancelled:${requestId}`);
    if (state.status === 'FAILED') throw new Error(state.error);
    throw new Error('nautilus_receipt_invalid');
  } catch (error) {
    // A tracked Task's abort signal also fires for Harness.close(). Remote
    // cancellation belongs to the explicit durable abort handler, never here.
    if (owner || !context.abortSignal?.aborted || error instanceof RemoteAdmissionRejectedError) throw error;
    // Retained compatibility for direct V1 callers with an owner cancellation
    // signal. Protected V2 calls always require an official Task owner.
    try {
      const state = remoteReceipt(await remoteRequest(target, path, 'DELETE', undefined, AbortSignal.timeout(15_000)), requestId, target);
      if (!['CANCELLED', 'COMPLETED', 'FAILED'].includes(state.status)) throw new Error('remote_not_terminal');
    } catch { throw new RemoteUncertaintyError('nautilus_cancellation_unknown', requestId, target, error); }
    throw error;
  } finally { if (timer) clearInterval(timer); progressAbort.abort(); await progressRead; }
}

/** Run only from an official Task's explicit abort handler. The intent is
 * persisted before DELETE; unresolved cancellation survives host restart. */
export async function cancelNativeRemoteTasks(taskId: TaskId, runtime: ReceiptRuntime, context: Context): Promise<boolean> {
  let requests: JsonObject[] = [];
  await runtime.commit(async tx => {
    const facts = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(taskId), null);
    const pending = facts.requests.filter(item => !['COMPLETED', 'FAILED', 'CANCELLED', 'REJECTED'].includes(String(item.status)));
    for (const item of pending) item.cancellationRequested = true;
    requests = detached(pending);
    return undefined;
  }, context);
  let settled = true;
  for (const reference of requests) {
    const target = validateRemoteTarget(reference.target); const requestId = String(reference.requestId);
    try {
      const value = await remoteRequest(target, `v${target.protocolVersion ?? 1}/executions/${requestId}`, 'DELETE', undefined, AbortSignal.timeout(15_000));
      if (target.protocolVersion === 2 && value && typeof value === 'object' && !Array.isArray(value)
        && Object.keys(value).every(key => ['schemaVersion', 'instanceId', 'requestId', 'cancellation'].includes(key))
        && value.schemaVersion === 2 && value.instanceId === target.instanceId && value.requestId === requestId && value.cancellation === 'FENCED_NOT_ADMITTED') {
        await retainRemoteReference({ ...reference, status: 'CANCELLED', admission: 'NOT_ADMITTED', error: null }, { taskId, runtime, context });
        continue;
      }
      const state = remoteReceipt(value, requestId, target);
      if (!['CANCELLED', 'COMPLETED', 'FAILED'].includes(state.status)) { settled = false; continue; }
      await retainRemoteReference({ ...reference, status: state.status, error: state.status === 'FAILED' ? 'nautilus_execution_failed' : null }, { taskId, runtime, context });
    } catch { settled = false; }
  }
  return settled;
}

/** Derive the unmetered execution payload without changing frozen scientific facts. */
export function unmeteredNativeInput<T extends NativeJson>(value: T): T {
  // Most payloads are large scientific arrays without quotas. Preserve unchanged
  // branches instead of duplicating native evidence solely to inspect settings.
  if (Array.isArray(value)) {
    let copy: NativeJson[] | undefined;
    for (let index = 0; index < value.length; index++) {
      const child = unmeteredNativeInput(value[index]!);
      if (child !== value[index]) { copy ??= [...value]; copy[index] = child; }
    }
    return (copy ?? value) as T;
  }
  if (value !== null && typeof value === 'object') {
    let copy: Record<string, NativeJson> | undefined;
    for (const [key, original] of Object.entries(value)) {
      if (key === 'total_fuel') { copy ??= { ...value }; delete copy[key]; continue; }
      const child = unmeteredNativeInput(original);
      if (child !== original) { copy ??= { ...value }; copy[key] = child; }
    }
    return (copy ?? value) as T;
  }
  return value;
}

/** Shared transport for the research and service scientific entry points. */
export async function executeNativeValue(target: NativeTarget, command: string, input: NativeJson, args: string[], directory: string, context: Context): Promise<NativeJson> {
  const interceptor = nativeInterceptor.getStore();
  const execute = () => executeNativeUnwrapped(target, command, unmeteredNativeInput(input), args, directory, context);
  return interceptor ? interceptor(command, input, execute) : execute();
}

async function executeNativeUnwrapped(target: NativeTarget, command: string, input: NativeJson, args: string[], directory: string, context: Context): Promise<NativeJson> {
  if (!nativeCommands.has(command)) throw new Error('native_command_unsupported');
  const observation = nativeObservation.getStore();
  if (observation && observation.command !== command) throw new Error('native_fresh_read_command_mismatch');
  if (typeof target !== 'string') {
    try { return await executeRemote(target, command, input, args, context); }
    catch (error) { throw new ExecutionFailure(error, { operation: command, stage: 'execute' }); }
  }
  if (process.env.QZ_REQUIRE_REMOTE_NAUTILUS === '1') throw new Error('legacy_local_nautilus_task_requires_migration');
  if (!target) throw new Error('native_executable_required');
  const inputPath = join(directory, 'request.json'); await writeFile(inputPath, jsonChunks(input));
  const env = new NodeExecutionEnv({ cwd: directory });
  const observer = progressTracking.getStore();
  const progressPath = observer && ['simulate', 'replay-catalog-ensemble'].includes(command) ? join(directory, `progress-${randomUUID()}.ndjson`) : undefined;
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
