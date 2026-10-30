import { createHash, timingSafeEqual } from 'node:crypto';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { constants, createReadStream } from 'node:fs';
import { access, readFile, mkdir, mkdtemp, realpath, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { JsonReader, jsonChunks, canonicalJsonDigest } from '../../research/src/native-json.ts';
import { isTickNativeCommand } from '../../../packages/contracts/src/nautilus.ts';
import type { NativeJson as Json, NativeExecutionRequest as Request, NativeExecutionReceipt as Receipt, NativeExecutionRejection } from '../../../packages/contracts/src/nautilus.ts';

type Active = { promise: Promise<Receipt>; cancel: () => void; observers: number; progressPath?: string };

// No runtime, broker, order, installation or arbitrary shell command is exposed.
export const commands = ['evaluate-factor', 'evaluate-tick-factor', 'replay-tick-economic', 'forecast', 'forecast-factor', 'evaluate-experiment', 'simulate', 'replay-catalog-ensemble',
  'study-portfolio', 'allocate', 'blend-targets', 'assess-capacity', 'size-account', 'validate-capital-exit', 'fit-ensemble', 'apply-ensemble',
  'capital-curve', 'freeze-ensemble-calibrations', 'compare-observed-spot', 'study-observed-spot', 'value-observed-spot', 'build-observed-spot',
  'catalog-prefix-digest', 'catalog-factor-scope', 'scan-factor-catalog'] as const;
const flags = new Set(['--summary-only', '--plan-only']);
const scalarOptions = new Set(['--dataset-revision-id', '--model-artifact-id', '--feature-artifact-id']);
const fileOptions = new Set(['--model', '--scope', '--features', '--calibration']);
const catalogOptions = new Set(['--catalog', '--funding-catalog', '--partitions']);

export interface WorkerOptions {
  binary: string;
  /** Explicit matching native format for tick research; never inferred from catalog paths. */
  tickBinary?: string;
  dataDir: string;
  catalogRoots: string[];
  token?: string;
  maximumRequestBytes?: number;
  /** Worker-owned tick limits; scientific JSON cannot raise these limits. */
  maximumConcurrentTickExecutions?: number;
  tickMaximumMemoryBytes?: number;
  tickMaximumOutputBytes?: number;
}

function positiveLimit(value: number | undefined, fallback: number, code: string): number {
  const limit = value ?? fallback;
  if (!Number.isSafeInteger(limit) || limit <= 0) fail(code);
  return limit;
}

export function executionId(request: Omit<Request, 'requestId'>): string {
  return canonicalJsonDigest(request);
}
function fail(message: string, statusCode = 400): never { throw Object.assign(new Error(message), { statusCode }); }
function rejectAdmission(requestId: string, error: NativeExecutionRejection['error'], statusCode: number): never {
  const rejection: NativeExecutionRejection = { schemaVersion: 1, requestId, admission: 'REJECTED', error };
  throw Object.assign(new Error(error), { statusCode, rejection });
}
function object(value: unknown): value is Record<string, Json> { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function validName(name: string): boolean { return /^[a-zA-Z0-9_-][a-zA-Z0-9_./-]*$/.test(name) && name.split('/').every(part => part !== '.' && part !== '..' && part !== ''); }
function below(root: string, path: string): boolean { const suffix = relative(root, path); return suffix === '' || !isAbsolute(suffix) && suffix !== '..' && !suffix.startsWith(`..${sep}`); }

async function digestFile(path: string): Promise<string> {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return `sha256:${hash.digest('hex')}`;
}
async function body(request: IncomingMessage, maximum: number): Promise<unknown> {
  const parser = new JsonReader(); let size = 0;
  request.setEncoding('utf8');
  for await (const chunk of request) {
    size += Buffer.byteLength(chunk);
    if (size > maximum) fail('nautilus_request_too_large', 413);
    parser.write(chunk);
  }
  return parser.finish();
}
async function send(response: ServerResponse, statusCode: number, value: unknown): Promise<void> {
  if (response.destroyed) return;
  response.writeHead(statusCode, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' });
  await pipeline(Readable.from(jsonChunks(value as never)), response).catch(() => {});
}
function authorized(request: IncomingMessage, token?: string): boolean {
  if (!token) return true;
  const expected = Buffer.from(`Bearer ${token}`); const actual = Buffer.from(request.headers.authorization ?? '');
  return actual.length === expected.length && timingSafeEqual(actual, expected);
}
function validateRequest(value: unknown, digest: string): Request {
  if (!object(value) || value.schemaVersion !== 1 || typeof value.requestId !== 'string' || !/^[a-f0-9]{64}$/.test(value.requestId)
    || typeof value.invocationId !== 'string' || !/^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(value.invocationId)
    || typeof value.implementationDigest !== 'string' || typeof value.command !== 'string' || !commands.includes(value.command as typeof commands[number])
    || !Array.isArray(value.args) || !value.args.every(arg => typeof arg === 'string') || !object(value.files) || value.input === undefined
    || Object.keys(value).some(key => !['schemaVersion', 'requestId', 'invocationId', 'implementationDigest', 'command', 'input', 'args', 'files'].includes(key))) fail('nautilus_request_invalid');
  if (value.implementationDigest !== digest) fail('nautilus_implementation_changed', 409);
  for (const [name, encoded] of Object.entries(value.files)) {
    if (!validName(name) || typeof encoded !== 'string' || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded)) fail('nautilus_attachment_invalid');
  }
  const { requestId, ...request } = value as unknown as Request;
  if (executionId(request) !== requestId) fail('nautilus_request_identity_conflict', 409);
  return value as unknown as Request;
}

export async function createNautilusWorker(options: WorkerOptions) {
  const tickConcurrency = positiveLimit(options.maximumConcurrentTickExecutions, 1, 'nautilus_tick_concurrency_invalid');
  const tickMemoryBytes = positiveLimit(options.tickMaximumMemoryBytes, 2 * 1024 ** 3, 'nautilus_tick_memory_limit_invalid');
  const tickOutputBytes = positiveLimit(options.tickMaximumOutputBytes, 64 * 1024 ** 2, 'nautilus_tick_output_limit_invalid');
  const binary = await realpath(options.binary);
  const tickBinary = options.tickBinary === undefined ? binary : await realpath(options.tickBinary);
  async function implementationIdentity() {
    const standard = await digestFile(binary);
    // Preserve existing single-binary receipt identity when no companion is configured.
    if (options.tickBinary === undefined) return standard;
    const tick = await digestFile(tickBinary);
    return `sha256:${canonicalJsonDigest({ standard, tick })}`;
  }
  const implementationDigest = await implementationIdentity();
  const root = resolve(options.dataDir);
  const receipts = join(root, 'executions'); const scratch = join(root, 'scratch');
  await mkdir(receipts, { recursive: true }); await mkdir(scratch, { recursive: true });
  const roots = await Promise.all(options.catalogRoots.map(path => realpath(path)));
  const active = new Map<string, Active>();
  const launching = new Map<string, Promise<Active>>();
  // Admission reservations only, not a queue. Existing receipts never consume
  // another slot or authorize a second execution.
  const tickSlots = new Set<string>();
  let closing = false;

  async function save(receipt: Receipt) {
    const directory = join(receipts, receipt.requestId); const temporary = join(directory, 'receipt.next.json');
    await writeFile(temporary, jsonChunks(receipt as never), { mode: 0o600 });
    await rename(temporary, join(directory, 'receipt.json'));
  }
  async function read(id: string): Promise<Receipt | null> {
    try {
      const parser = new JsonReader();
      for await (const chunk of createReadStream(join(receipts, id, 'receipt.json'), { encoding: 'utf8' })) parser.write(chunk);
      const receipt = parser.finish() as unknown as Receipt;
      // An old RUNNING receipt never grants permission to repeat an uncertain run.
      if (receipt.status === 'RUNNING') {
        const execution = active.get(id);
        if (!execution) return { ...receipt, status: 'UNKNOWN', error: 'nautilus_execution_outcome_unknown' };
        if (execution.progressPath) {
          try {
            const rows = (await readFile(execution.progressPath, 'utf8')).split('\n'); rows.pop();
            const snapshots = rows.filter(Boolean).map(row => (JSON.parse(row) as { snapshot: Json }).snapshot);
            return { ...receipt, progress: { snapshots } };
          } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
        }
      }
      return receipt;
    } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null; throw error; }
  }
  async function argumentsFor(request: Request, directory: string): Promise<string[]> {
    const attachments = join(directory, 'attachments'); await mkdir(attachments);
    for (const [name, encoded] of Object.entries(request.files)) {
      const path = join(attachments, name); await mkdir(dirname(path), { recursive: true });
      await writeFile(path, Buffer.from(encoded, 'base64'), { mode: 0o600 });
    }
    const args: string[] = [];
    for (let index = 0; index < request.args.length; index++) {
      const option = request.args[index]!;
      if (flags.has(option)) { args.push(option); continue; }
      const value = request.args[++index];
      if (value === undefined) fail('nautilus_argument_missing');
      if (catalogOptions.has(option)) {
        if (!isAbsolute(value)) fail('nautilus_catalog_path_must_be_absolute');
        const path = await realpath(value);
        if (!roots.some(root => below(root, path))) fail('nautilus_catalog_outside_configured_roots');
        args.push(option, path);
      } else if (fileOptions.has(option)) {
        const name = value.startsWith('@file:') ? value.slice(6) : '';
        if (!validName(name) || !(name in request.files)) fail('nautilus_attachment_required');
        args.push(option, join(attachments, name));
      } else if (option === '--objects') {
        const name = value.startsWith('@objects:') ? value.slice(9) : '';
        if (!validName(name)) fail('nautilus_object_directory_invalid');
        const objects = join(attachments, name); await mkdir(objects, { recursive: true });
        args.push(option, objects);
      } else if (scalarOptions.has(option) && !value.startsWith('-')) args.push(option, value);
      else fail('nautilus_argument_unsupported');
    }
    return args;
  }

  async function launchNew(request: Request): Promise<Active> {
    const existing = active.get(request.requestId);
    if (existing) return existing;
    const receiptDirectory = join(receipts, request.requestId);
    try { await mkdir(receiptDirectory); }
    catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      const receipt = await read(request.requestId) ?? { schemaVersion: 1, requestId: request.requestId, status: 'UNKNOWN', error: 'nautilus_execution_outcome_unknown' } as Receipt;
      return { promise: Promise.resolve(receipt), cancel() {}, observers: 0 };
    }
    const running: Receipt = { schemaVersion: 1, requestId: request.requestId, status: 'RUNNING' };
    await save(running);
    let child: ChildProcess | undefined; let cancelRequested = closing; let killTimer: NodeJS.Timeout | undefined;
    function signal(signal: NodeJS.Signals) {
      if (!child?.pid) return;
      try { process.kill(-child.pid, signal); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; }
    }
    const execution: Active = { observers: 0, promise: undefined as unknown as Promise<Receipt>, cancel() {
      if (cancelRequested) return;
      cancelRequested = true;
      signal('SIGTERM');
      killTimer = setTimeout(() => signal('SIGKILL'), 5_000); killTimer.unref();
    } };
    active.set(request.requestId, execution);
    execution.promise = (async (): Promise<Receipt> => {
      let directory: string | undefined;
      let receipt: Receipt;
      try {
        directory = await mkdtemp(join(scratch, 'run-'));
        const args = await argumentsFor(request, directory);
        if (await implementationIdentity() !== implementationDigest) fail('nautilus_implementation_changed', 409);
        if (cancelRequested) receipt = { ...running, status: 'CANCELLED' };
        else {
          // The worker launches only the configured scientific executable. Provider
          // tokens, worker auth and unrelated host environment never reach it.
          const tick = isTickNativeCommand(request.command);
          const selectedBinary = tick ? tickBinary : binary;
          const executable = tick ? '/usr/bin/prlimit' : selectedBinary;
          const childArgs = tick ? [`--as=${tickMemoryBytes}:${tickMemoryBytes}`, '--', selectedBinary, request.command, ...args] : [request.command, ...args];
          if (['simulate', 'replay-catalog-ensemble'].includes(request.command)) execution.progressPath = join(directory, 'progress.ndjson');
          child = spawn(executable, childArgs, { cwd: directory, detached: true,
            env: { PATH: '/usr/bin:/bin', TMPDIR: directory, ...(execution.progressPath ? { QZ_PROGRESS_PATH: execution.progressPath } : {}) }, stdio: ['pipe', 'pipe', 'pipe'] });
          const parser = new JsonReader(); let parseError: unknown; let stderr = ''; let outputBytes = 0; let outputExceeded = false;
          child.stdout!.setEncoding('utf8'); child.stderr!.setEncoding('utf8');
          child.stdout!.on('data', chunk => {
            if (tick) {
              outputBytes += Buffer.byteLength(chunk);
              if (outputBytes > tickOutputBytes) { outputExceeded = true; signal('SIGKILL'); return; }
            }
            if (!parseError) { try { parser.write(chunk); } catch (error) { parseError = error; } }
          });
          child.stderr!.on('data', chunk => { if (stderr.length < 65_536) stderr += chunk.slice(0, 65_536 - stderr.length); });
          const closed = new Promise<number | null>((resolve, reject) => { child!.once('error', reject); child!.once('close', resolve); });
          void pipeline(Readable.from(jsonChunks(request.input as never)), child.stdin!).catch(() => {});
          const code = await closed;
          if (cancelRequested) {
            // The main process can exit before a compiler child. Stop its entire
            // process group before acknowledging cancellation and clearing timers.
            signal('SIGKILL');
            receipt = { ...running, status: 'CANCELLED' };
          }
          else if (outputExceeded) receipt = { ...running, status: 'FAILED', error: 'nautilus_tick_output_limit_exceeded' };
          else if (code !== 0) receipt = { ...running, status: 'FAILED', error: stderr.trim() || `native_${request.command}_exit_${code}` };
          else {
            if (parseError) throw parseError;
            receipt = { ...running, status: 'COMPLETED', result: parser.finish() as Json };
          }
        }
      } catch (error) { receipt = cancelRequested ? { ...running, status: 'CANCELLED' }
        : { ...running, status: 'FAILED', error: error instanceof Error ? error.message : 'nautilus_execution_failed' }; }
      finally { if (killTimer) clearTimeout(killTimer); if (directory) await rm(directory, { recursive: true, force: true }); }
      await save(receipt);
      return receipt;
    })().finally(() => active.delete(request.requestId));
    return execution;
  }
  function launch(request: Request): Promise<Active> {
    const running = active.get(request.requestId);
    if (running) return Promise.resolve(running);
    const existing = launching.get(request.requestId);
    if (existing) return existing;
    const pending = (async () => {
      const tick = isTickNativeCommand(request.command);
      if (tick) {
        // A completed or uncertain original receipt remains readable even while
        // another tick invocation occupies the installation's capacity.
        const original = await read(request.requestId);
        if (original) return { promise: Promise.resolve(original), cancel() {}, observers: 0 };
        if (process.platform !== 'linux') rejectAdmission(request.requestId, 'nautilus_tick_resource_limits_unavailable', 501);
        try { await access('/usr/bin/prlimit', constants.X_OK); }
        catch { rejectAdmission(request.requestId, 'nautilus_tick_resource_limits_unavailable', 501); }
        if (tickSlots.size >= tickConcurrency) rejectAdmission(request.requestId, 'nautilus_tick_capacity_exhausted', 429);
        tickSlots.add(request.requestId);
      }
      try {
        const execution = await launchNew(request);
        if (tick) void execution.promise.then(() => tickSlots.delete(request.requestId), () => tickSlots.delete(request.requestId));
        return execution;
      } catch (error) { if (tick) tickSlots.delete(request.requestId); throw error; }
    })().finally(() => launching.delete(request.requestId));
    launching.set(request.requestId, pending);
    return pending;
  }

  const server = createServer(async (request, response) => {
    try {
      if (request.method === 'GET' && request.url === '/health') return await send(response, closing ? 503 : 200, { ready: !closing });
      if (!authorized(request, options.token)) return await send(response, 401, { error: 'nautilus_authentication_required' });
      if (request.method === 'GET' && request.url === '/v1/capabilities') return await send(response, 200, { schemaVersion: 1, implementationDigest, commands });
      const match = /^\/v1\/executions\/([a-f0-9]{64})$/.exec(request.url ?? '');
      if (match && ['GET', 'DELETE'].includes(request.method ?? '')) {
        const execution = active.get(match[1]!);
        if (request.method === 'DELETE' && execution) {
          execution.cancel();
          return await send(response, 200, await execution.promise);
        }
        const receipt = await read(match[1]!);
        return await send(response, receipt ? receipt.status === 'RUNNING' ? 202 : 200 : 404, receipt ?? { error: 'nautilus_execution_not_found' });
      }
      if (request.method !== 'POST' || request.url !== '/v1/execute') return await send(response, 404, { error: 'not_found' });
      if (closing) return await send(response, 503, { error: 'nautilus_worker_closing' });
      const value = validateRequest(await body(request, options.maximumRequestBytes ?? 256 * 1024 * 1024), implementationDigest);
      if (closing) return await send(response, 503, { error: 'nautilus_worker_closing' });
      const execution = await launch(value);
      execution.observers++;
      let detached = false;
      const disconnect = () => {
        if (detached) return;
        detached = true; execution.observers--;
        if (!response.writableFinished && execution.observers === 0 && active.has(value.requestId)) execution.cancel();
      };
      response.once('close', disconnect);
      if (response.destroyed) disconnect();
      await send(response, 200, await execution.promise);
      disconnect();
    } catch (error) {
      await send(response, typeof (error as { statusCode?: unknown }).statusCode === 'number' ? (error as { statusCode: number }).statusCode : 400,
        error instanceof Error && 'rejection' in error ? error.rejection : { error: error instanceof Error ? error.message : 'nautilus_request_failed' });
    }
  });
  // Native work may outlast an HTTP server's ordinary request/idle timeout.
  server.requestTimeout = 0; server.timeout = 0;
  return { server, implementationDigest, async close() {
    closing = true;
    const closed = new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    for (const execution of active.values()) execution.cancel();
    await Promise.allSettled([...active.values()].map(execution => execution.promise));
    server.closeIdleConnections();
    await closed;
  } };
}
