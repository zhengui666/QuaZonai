import { createHash, timingSafeEqual } from 'node:crypto';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { createReadStream, createWriteStream } from 'node:fs';
import { appendFile, readFile, mkdir, mkdtemp, realpath, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';
import { Readable, Transform } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { JsonReader, JSONParser, jsonChunks, canonicalJsonDigest } from '../../research/src/native-json.ts';
import { isTickNativeCommand } from '../../../packages/contracts/src/nautilus.ts';
import { createCapitalAccount } from './capital-account.ts';
import type { AccountOptions } from './account-runtime.ts';
import type { NativeJson as Json, NativeExecutionRequest as Request, NativeExecutionReceipt as Receipt, NativeExecutionRejection } from '../../../packages/contracts/src/nautilus.ts';

type StoredReceipt = Receipt & { receiptFile?: string; progressFile?: string };
type Active = { promise: Promise<StoredReceipt>; cancel: () => void; progressPath?: string };

// The science /v1/executions allowlist exposes no broker, order or shell commands.
// Explicitly configured /accounts owners use the separate closed account API.
export const commands = ['review-source', 'native-strategy-backtest', 'native-configured-backtest',
  'validate-capital-exit',
  'catalog-inspect', 'catalog-coverage', 'catalog-prefix-digest', 'catalog-factor-scope'] as const;
const scalarOptions = new Set(['--dataset-revision-id']);
const catalogOptions = new Set(['--catalog']);

export interface WorkerOptions {
  binary: string;
  /** Explicit matching native format for tick research; never inferred from catalog paths. */
  tickBinary?: string;
  manifestPath?: string;
  dataDir: string;
  catalogRoots: string[];
  token?: string;
  maximumRequestBytes?: number;
  /** Concurrent worker capacity, separate from cumulative research execution. */
  maximumConcurrentTickExecutions?: number;
  /** Explicit Paper/LIVE account modes in this same native worker process. */
  accounts?: AccountOptions[];
  accountBinary?: string;
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
  const buildIdentity = options.manifestPath ? JSON.parse(await readFile(options.manifestPath, 'utf8')) : undefined;
  if (buildIdentity && (buildIdentity.schemaVersion !== 1 || !/^[a-f0-9]{40}$/.test(buildIdentity.nautilusRevision)
    || buildIdentity.binaries?.job?.digest !== await digestFile(binary)
    || options.tickBinary !== undefined && buildIdentity.binaries?.['job-high-precision']?.digest !== await digestFile(tickBinary))) {
    throw new Error('nautilus_build_identity_mismatch');
  }
  const root = resolve(options.dataDir);
  const receipts = join(root, 'executions'); const scratch = join(root, 'scratch');
  await mkdir(receipts, { recursive: true }); await mkdir(scratch, { recursive: true });
  const roots = await Promise.all(options.catalogRoots.map(path => realpath(path)));
  const accounts = new Map<string, Awaited<ReturnType<typeof createCapitalAccount>>>();
  const physicalAccounts = new Set<string>();
  for (const account of options.accounts ?? []) {
    const physical = JSON.stringify(account.mode === 'live' ? ['live', account.network ?? 'mainnet', account.accountAddress?.toLowerCase(), 'SPOT_CASH', 'USDC']
      : ['paper', account.network ?? 'mainnet', account.accountId, 'SPOT_CASH', 'USDC']);
    if (physicalAccounts.has(physical)) fail('account_physical_owner_already_configured');
    physicalAccounts.add(physical);
  }
  for (const account of options.accounts ?? []) {
    if (accounts.has(account.id)) fail('account_configuration_duplicate');
    accounts.set(account.id, await createCapitalAccount(account, join(root, 'accounts', account.id), options.accountBinary ?? join(dirname(binary), 'account-runtime')));
  }
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
  async function read(id: string): Promise<StoredReceipt | null> {
    try {
      const file = join(receipts, id, 'receipt.json');
      // Select only envelope fields. The parser still validates the complete JSON,
      // but never retains the potentially hundreds-of-MB result object.
      async function envelope(): Promise<StoredReceipt> {
        const parser = new JSONParser({ paths: ['$.schemaVersion', '$.requestId', '$.status', '$.error'], keepStack: false });
        const fields: Record<string, unknown> = {};
        parser.onValue = ({ key, value }) => { fields[String(key)] = value; };
        for await (const chunk of createReadStream(file)) parser.write(chunk);
        if (!parser.isEnded) parser.end();
        if (fields.schemaVersion !== 1 || fields.requestId !== id || !['RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED', 'UNKNOWN'].includes(String(fields.status))) throw new Error('nautilus_receipt_invalid');
        return { ...fields, ...(fields.status === 'RUNNING' ? {} : { receiptFile: file }) } as StoredReceipt;
      }
      let receipt = await envelope();
      // An old RUNNING receipt never grants permission to repeat an uncertain run.
      if (receipt.status === 'RUNNING') {
        const execution = active.get(id);
        if (!execution) {
          if (launching.has(id)) return receipt;
          // A reader can have opened RUNNING just before the terminal rename
          // and active-map removal. Reopen after observing no active owner.
          receipt = await envelope();
          return receipt.status === 'RUNNING' ? { ...receipt, status: 'UNKNOWN', error: 'nautilus_execution_outcome_unknown' } : receipt;
        }
        if (execution.progressPath) {
          try {
            return { ...receipt, progressFile: execution.progressPath };
          } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
        }
      }
      return receipt;
    } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null; throw error; }
  }
  async function argumentsFor(request: Request): Promise<string[]> {
    if (Object.keys(request.files).length) fail('nautilus_attachments_unsupported');
    const args: string[] = [];
    for (let index = 0; index < request.args.length; index++) {
      const option = request.args[index]!;
      const value = request.args[++index];
      if (value === undefined) fail('nautilus_argument_missing');
      if (catalogOptions.has(option)) {
        if (!isAbsolute(value)) fail('nautilus_catalog_path_must_be_absolute');
        const path = await realpath(value);
        if (!roots.some(root => below(root, path))) fail('nautilus_catalog_outside_configured_roots');
        args.push(option, path);
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
      return { promise: Promise.resolve(receipt), cancel() {} };
    }
    const running: Receipt = { schemaVersion: 1, requestId: request.requestId, status: 'RUNNING' };
    await save(running);
    let child: ChildProcess | undefined; let cancelRequested = closing; let killTimer: NodeJS.Timeout | undefined;
    function signal(signal: NodeJS.Signals) {
      if (!child?.pid) return;
      try { process.kill(-child.pid, signal); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; }
    }
    const execution: Active = { promise: undefined as unknown as Promise<StoredReceipt>, cancel() {
      if (cancelRequested) return;
      cancelRequested = true;
      signal('SIGTERM');
      killTimer = setTimeout(() => signal('SIGKILL'), 5_000); killTimer.unref();
    } };
    active.set(request.requestId, execution);
    execution.promise = (async (): Promise<StoredReceipt> => {
      let directory: string | undefined;
      let receipt: StoredReceipt;
      try {
        directory = await mkdtemp(join(scratch, 'run-'));
        const args = await argumentsFor(request);
        if (await implementationIdentity() !== implementationDigest) fail('nautilus_implementation_changed', 409);
        if (cancelRequested) receipt = { ...running, status: 'CANCELLED' };
        else {
          // The worker launches only the configured scientific executable. Provider
          // tokens, worker auth and unrelated host environment never reach it.
          const tick = isTickNativeCommand(request.command);
          const selectedBinary = tick ? tickBinary : binary;
          const childArgs = [request.command, ...args];
          const temporary = join(receiptDirectory, 'receipt.next.json');
          await writeFile(temporary, JSON.stringify({ ...running, status: 'COMPLETED' }).slice(0, -1) + ',"result":', { mode: 0o600 });
          // Cancellation may arrive while the temporary receipt is being opened.
          if (cancelRequested) throw new Error('nautilus_execution_cancelled');
          child = spawn(selectedBinary, childArgs, { cwd: directory, detached: true,
            env: { PATH: '/usr/bin:/bin', TMPDIR: directory,
              ...(execution.progressPath ? { QZ_PROGRESS_PATH: execution.progressPath } : {}) }, stdio: ['pipe', 'pipe', 'pipe'] });
          const closed = new Promise<number | null>((resolve, reject) => { child!.once('error', reject); child!.once('close', resolve); });
          const parser = new JSONParser({ paths: [], keepStack: false });
          let sawToken = false;
          parser.onToken = () => { sawToken = true; };
          let parseError: unknown; let stderr = '';
          child.stderr!.setEncoding('utf8');
          const output = pipeline(child.stdout!, new Transform({ transform(chunk, _encoding, callback) {
            if (!parseError) { try { parser.write(chunk); } catch (error) { parseError = error; } }
            callback(null, chunk);
          } }), createWriteStream(temporary, { flags: 'a', mode: 0o600 })).then(() => undefined, error => { parseError ??= error; });
          child.stderr!.on('data', chunk => { if (stderr.length < 65_536) stderr += chunk.slice(0, 65_536 - stderr.length); });
          void pipeline(Readable.from(jsonChunks(request.input as never)), child.stdin!).catch(() => {});
          const code = await closed.catch(async error => { await output; throw error; });
          await output;
          if (cancelRequested) {
            // The main process can exit before a compiler child. Stop its entire
            // process group before acknowledging cancellation and clearing timers.
            signal('SIGKILL');
            receipt = { ...running, status: 'CANCELLED' };
          }
          else if (code !== 0) receipt = { ...running, status: 'FAILED', error: stderr.trim() || `native_${request.command}_exit_${code}` };
          else {
            if (parseError) throw parseError;
            if (!parser.isEnded) parser.end();
            if (!sawToken) throw new Error('nautilus_empty_native_result');
            await appendFile(temporary, '}');
            await rename(temporary, join(receiptDirectory, 'receipt.json'));
            receipt = { ...running, status: 'COMPLETED', receiptFile: join(receiptDirectory, 'receipt.json') } as StoredReceipt;
          }
        }
      } catch (error) { receipt = cancelRequested ? { ...running, status: 'CANCELLED' }
        : { ...running, status: 'FAILED', error: error instanceof Error ? error.message : 'nautilus_execution_failed' }; }
      finally { if (killTimer) clearTimeout(killTimer); if (directory) await rm(directory, { recursive: true, force: true }); }
      if (!receipt.receiptFile) await save(receipt);
      return receipt;
    })().finally(() => active.delete(request.requestId));
    return execution;
  }
  async function sendReceipt(response: ServerResponse, status: number, receipt: StoredReceipt): Promise<void> {
    if (response.destroyed) return;
    if (!receipt.receiptFile && !receipt.progressFile) return send(response, status, receipt);
    response.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' });
    if (receipt.receiptFile) {
      await pipeline(createReadStream(receipt.receiptFile), response).catch(() => {});
      return;
    }
    const { progressFile, ...metadata } = receipt;
    async function* progress() {
      yield JSON.stringify(metadata).slice(0, -1) + ',"progress":{"snapshots":[';
      let pending = ''; let first = true;
      try {
        for await (const chunk of createReadStream(progressFile!, { encoding: 'utf8' })) {
          pending += chunk;
          let newline: number;
          while ((newline = pending.indexOf('\n')) >= 0) {
            const line = pending.slice(0, newline); pending = pending.slice(newline + 1);
            if (!line) continue;
            const snapshot = (JSON.parse(line) as { snapshot: Json }).snapshot;
            if (!first) yield ','; first = false;
            yield* jsonChunks(snapshot);
          }
        }
      } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
      // A partial final NDJSON row is not committed progress.
      yield ']}}';
    }
    await pipeline(Readable.from(progress()), response).catch(() => {});
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
        if (original) return { promise: Promise.resolve(original), cancel() {} };
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
      const accountPath = /^\/accounts\/([^/?]+)(\/[^?]*)?(?:\?.*)?$/.exec(request.url ?? '');
      if (accountPath) {
        const account = accounts.get(decodeURIComponent(accountPath[1]!));
        if (!account) return await send(response, 404, { error: 'account_not_configured' });
        if (closing && request.method !== 'GET') return await send(response, 503, { error: 'nautilus_worker_closing' });
        const result = await account.handle(accountPath[2] ?? '/', request, () => body(request, options.maximumRequestBytes ?? 1_048_576));
        return await send(response, result.status, result.value);
      }
      if (request.method === 'GET' && request.url === '/v1/capabilities') return await send(response, 200, { schemaVersion: 1, implementationDigest, commands, ...(buildIdentity ? { buildIdentity } : {}) });
      const match = /^\/v1\/executions\/([a-f0-9]{64})$/.exec(request.url ?? '');
      if (match && ['GET', 'DELETE'].includes(request.method ?? '')) {
        const execution = active.get(match[1]!);
        if (request.method === 'DELETE' && execution) {
          execution.cancel();
          return await sendReceipt(response, 200, await execution.promise);
        }
        const receipt = await read(match[1]!);
        return receipt ? await sendReceipt(response, receipt.status === 'RUNNING' ? 202 : 200, receipt) : await send(response, 404, { error: 'nautilus_execution_not_found' });
      }
      if (request.method !== 'POST' || request.url !== '/v1/execute') return await send(response, 404, { error: 'not_found' });
      if (closing) return await send(response, 503, { error: 'nautilus_worker_closing' });
      const value = validateRequest(await body(request, options.maximumRequestBytes ?? 256 * 1024 * 1024), implementationDigest);
      if (closing) return await send(response, 503, { error: 'nautilus_worker_closing' });
      const execution = await launch(value);
      // Admission outlives the HTTP connection. Only DELETE (or worker shutdown)
      // cancels a running process; reconnecting reads this same physical receipt.
      // launch() has persisted RUNNING before acknowledging admission.
      void execution.promise.catch(() => {});
      const receipt = await read(value.requestId) ?? await execution.promise;
      await sendReceipt(response, receipt.status === 'RUNNING' ? 202 : 200, receipt);
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
    const accountStops = await Promise.allSettled([...accounts.values()].map(account => account.close()));
    await Promise.allSettled([...active.values()].map(execution => execution.promise));
    server.closeIdleConnections();
    await closed;
    const failedAccount = accountStops.find(result => result.status === 'rejected');
    if (failedAccount?.status === 'rejected') throw failedAccount.reason;
  } };
}
