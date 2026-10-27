import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { defineDocFamily, type JsonObject, type TaskId, type TaskRuntime } from '@earendil-works/pi-durable';
import { createHash, randomUUID } from 'node:crypto';
import { AsyncLocalStorage } from 'node:async_hooks';
import { lstat, readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { setTimeout } from 'node:timers/promises';
import { Agent, fetch } from 'undici';
import type { NativeCapabilities, NativeExecutionReceipt, NativeExecutionRequest, NativeExecutionRejection, NativeJson, NativeTarget, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { canonical } from './documents.ts';
import { ExecutionFailure } from './execution-errors.ts';
import { JsonReader, jsonChunks } from './native-json.ts';

export const nativeCommands = new Set(['evaluate-factor', 'evaluate-tick-factor', 'forecast', 'forecast-factor', 'evaluate-experiment', 'simulate', 'replay-catalog-ensemble', 'study-portfolio', 'allocate', 'blend-targets', 'assess-capacity', 'size-account', 'validate-capital-exit', 'fit-ensemble', 'apply-ensemble', 'capital-curve', 'freeze-ensemble-calibrations', 'compare-observed-spot', 'study-observed-spot', 'value-observed-spot', 'build-observed-spot', 'catalog-prefix-digest', 'catalog-factor-scope', 'scan-factor-catalog']);
const instanceCredentials = new Map<string, string>();

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

type ReceiptRuntime = Pick<TaskRuntime<unknown, unknown, unknown, object>, 'commit' | 'conversationId' | 'signal'>;
const tracking = new AsyncLocalStorage<{ taskId: TaskId; runtime: ReceiptRuntime; context: Context }>();

/** Bind transport receipt writes to the original official task; this has no
 * queue or execution state of its own. Register before unsafe network waits. */
export function withNativeRemoteTracking<T>(taskId: TaskId, runtime: ReceiptRuntime, context: Context, operation: () => Promise<T>): Promise<T> {
  return tracking.run({ taskId, runtime, context }, operation);
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
      if (method === 'POST' && path === 'v1/execute' && [429, 501].includes(response.status) && body && typeof body === 'object' && !Array.isArray(body) && body.command === 'evaluate-tick-factor') {
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

function receipt(value: NativeJson, requestId: string): NativeExecutionReceipt {
  const result = value as unknown as NativeExecutionReceipt;
  if (result?.schemaVersion !== 1 || result.requestId !== requestId || !['COMPLETED', 'FAILED', 'CANCELLED', 'RUNNING', 'UNKNOWN'].includes(result.status)
    || result.status === 'COMPLETED' && !Object.hasOwn(result, 'result')
    || ['FAILED', 'UNKNOWN'].includes(result.status) && typeof (result as { error?: unknown }).error !== 'string') throw new Error('nautilus_receipt_invalid');
  return result;
}

/** Read only the receipt for this original invocation and retained instance.
 * This does not submit or cancel any remote execution. */
export async function remoteNativeExecutionReceipt(target: RemoteNativeTarget, requestId: string, signal: AbortSignal): Promise<NativeExecutionReceipt> {
  if (!/^[a-f0-9]{64}$/.test(requestId)) throw new Error('nautilus_request_id_invalid');
  return receipt(await remoteRequest(target, `v1/executions/${requestId}`, 'GET', undefined, signal), requestId);
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

async function executeRemote(target: RemoteNativeTarget, command: string, input: NativeJson, originalArgs: string[], context: Context): Promise<NativeJson> {
  const capability = await remoteNativeCapabilities(target, context.abortSignal);
  if (!capability.commands.includes(command)) throw new Error('nautilus_command_unsupported');
  const artifacts = await attachments(originalArgs);
  const content = { schemaVersion: 1 as const, invocationId: randomUUID(), implementationDigest: capability.implementationDigest, command, input, ...artifacts };
  const requestId = createHash('sha256').update(canonical(content)).digest('hex');
  const request: NativeExecutionRequest = { ...content, requestId };
  const path = `v1/executions/${requestId}`;
  const owner = tracking.getStore();
  const reference: JsonObject = { requestId, target: { ...target }, error: 'nautilus_execution_unknown' };
  if (owner) await retainRemoteReference({ ...reference, status: 'PENDING' }, owner);
  context.abortSignal?.throwIfAborted();
  try {
    let state: NativeExecutionReceipt;
    try { state = receipt(await remoteRequest(target, 'v1/execute', 'POST', request as unknown as NativeJson, context.abortSignal), requestId); }
    catch (error) {
      if (error instanceof RemoteAdmissionRejectedError) {
        if (owner && !owner.runtime.signal.aborted) await retainRemoteReference({ ...reference, status: 'REJECTED', admission: 'REJECTED', error: error.rejection.error }, owner);
        throw error;
      }
      if (context.abortSignal?.aborted) throw error;
      if (error instanceof Error && /^nautilus_http_4\d\d$/.test(error.message)) throw error;
      // A lost POST response is reconciled with its original receipt; a new
      // execution identity or local fallback can never replace it.
      try { state = receipt(await remoteRequest(target, path, 'GET', undefined, context.abortSignal), requestId); }
      catch { throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target, error); }
    }
    while (state.status === 'RUNNING') {
      await setTimeout(100, undefined, { signal: context.abortSignal });
      try { state = receipt(await remoteRequest(target, path, 'GET', undefined, context.abortSignal), requestId); }
      catch (error) {
        if (context.abortSignal?.aborted) throw error;
        throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target, error);
      }
    }
    if (owner && !owner.runtime.signal.aborted && ['COMPLETED', 'FAILED', 'CANCELLED'].includes(state.status)) {
      await retainRemoteReference({ ...reference, status: state.status, error: state.status === 'FAILED' ? 'nautilus_execution_failed' : null }, owner);
    }
    if (state.status === 'COMPLETED') return state.result;
    if (state.status === 'UNKNOWN') throw new RemoteUncertaintyError('nautilus_execution_unknown', requestId, target);
    if (state.status === 'CANCELLED') throw new Error(`nautilus_execution_cancelled:${requestId}`);
    if (state.status === 'FAILED') throw new Error(state.error);
    throw new Error('nautilus_receipt_invalid');
  } catch (error) {
    if (error instanceof RemoteAdmissionRejectedError) throw error;
    if (!context.abortSignal?.aborted) throw error;
    // Cancellation is confirmed only after the worker joins the actual child.
    // If the transport is unavailable, retain an explicit unknown diagnosis;
    // the local Durable stop must not be interpreted as remote cancellation.
    try {
      const state = receipt(await remoteRequest(target, path, 'DELETE', undefined, AbortSignal.timeout(15_000)), requestId);
      if (!['CANCELLED', 'COMPLETED', 'FAILED'].includes(state.status)) throw new Error('remote_not_terminal');
    } catch { throw new RemoteUncertaintyError('nautilus_cancellation_unknown', requestId, target, error); }
    throw error;
  }
}

/** Shared transport for the research and service scientific entry points. */
export async function executeNativeValue(target: NativeTarget, command: string, input: NativeJson, args: string[], directory: string, context: Context): Promise<NativeJson> {
  if (!nativeCommands.has(command)) throw new Error('native_command_unsupported');
  if (typeof target !== 'string') {
    try { return await executeRemote(target, command, input, args, context); }
    catch (error) { throw new ExecutionFailure(error, { operation: command, stage: 'execute' }); }
  }
  if (process.env.QZ_REQUIRE_REMOTE_NAUTILUS === '1') throw new Error('legacy_local_nautilus_task_requires_migration');
  if (!target) throw new Error('native_executable_required');
  const inputPath = join(directory, 'request.json'); await writeFile(inputPath, jsonChunks(input));
  const env = new NodeExecutionEnv({ cwd: directory });
  const stdout = new JsonReader(); let parseError: unknown; let stderr = ''; let stage: 'execute' | 'decode' = 'execute';
  try {
    const result = getOrThrow(await env.exec(['bash', '-c', 'file="$1"; shift; exec "$@" < "$file"', 'qz-native', inputPath, target, command, ...args], {
      inheritEnv: false, env: { PATH: '/usr/bin:/bin', TMPDIR: directory },
      onOutput: (chunk, _ctx, info) => { if (info.stream === 'stderr') stderr += chunk; else if (parseError === undefined) { try { stdout.write(chunk); } catch (error) { parseError = error; } } },
    }, context));
    if (result.exitCode !== 0) throw new Error(stderr.trim() || `native_${command}_failed`);
    stage = 'decode'; if (parseError !== undefined) throw parseError; return stdout.finish();
  } catch (error) { throw new ExecutionFailure(error, { operation: command, stage }); }
  finally { await env.cleanup(withoutAbortSignal(context)); }
}
