import { createServer, request as httpRequest, type Server } from 'node:http';
import { connect } from 'node:net';
import { chmod, lstat, mkdir, readFile, realpath, unlink } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { pathToFileURL } from 'node:url';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { JsonReader, jsonChunks } from '../../research/src/native-json.ts';
import type { CheckpointInspection, CheckpointManifest, CheckpointProfile, RecoveryDriver } from './checkpoint-driver.ts';
import { RuncRecoveryDriver } from './checkpoint-runc.ts';
import type { RuncRecoveryOptions } from './checkpoint-runc.ts';

const methods = new Set(['profile', 'prepare', 'inspect', 'checkpoint', 'restore', 'start', 'cancel']);
const maximumBytes = 256 * 1024 * 1024;
type Operation = { method: string; requestId?: string; generation?: number; request?: unknown };

async function readJson(stream: AsyncIterable<Buffer | string>): Promise<unknown> {
  const parser = new JsonReader(); let bytes = 0;
  for await (const chunk of stream) {
    bytes += Buffer.byteLength(chunk); if (bytes > maximumBytes) throw new Error('checkpoint_broker_request_too_large');
    parser.write(typeof chunk === 'string' ? chunk : chunk.toString('utf8'));
  }
  return parser.finish();
}
async function removeStaleSocket(path: string): Promise<void> {
  try { if (!(await lstat(path)).isSocket()) throw new Error('checkpoint_broker_socket_invalid'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return; throw error; }
  await new Promise<void>((resolve, reject) => {
    const socket = connect(path);
    socket.once('connect', () => { socket.destroy(); reject(new Error('checkpoint_broker_already_running')); });
    socket.once('error', error => {
      socket.destroy(); if ((error as NodeJS.ErrnoException).code === 'ECONNREFUSED') resolve(); else reject(error);
    });
  });
  await unlink(path);
}

/** Only an operator-owned options file selects rootfs, binaries, mounts and
 * profile. The local caller can request fixed operations on original IDs; it
 * cannot pass host paths, shell commands, OCI specs or privilege settings. */
export async function createCheckpointBroker(options: { socketPath: string; driver: RecoveryDriver; ownerDirectory: string; socketMode?: number }): Promise<{ server: Server; close(): Promise<void> }> {
  const socketPath = resolve(options.socketPath); const owner = resolve(options.ownerDirectory);
  await mkdir(owner, { recursive: true, mode: 0o700 }); await mkdir(dirname(socketPath), { recursive: true, mode: 0o700 });
  if (await realpath(owner) !== owner || await realpath(dirname(socketPath)) !== dirname(socketPath)) throw new Error('checkpoint_broker_path_invalid');
  const mutex = new DatabaseSync(`${owner}/broker-owner.sqlite`);
  try { mutex.exec('PRAGMA busy_timeout=0; BEGIN IMMEDIATE'); await removeStaleSocket(socketPath); }
  catch (error) { mutex.close(); throw error; }
  const server = createServer(async (request, response) => {
    try {
      if (request.method !== 'POST' || request.url !== '/operation') throw new Error('checkpoint_broker_route_invalid');
      request.setEncoding('utf8');
      const input = await readJson(request) as Operation;
      if (!input || typeof input !== 'object' || !methods.has(input.method)
        || Object.keys(input).some(key => !['method', 'requestId', 'generation', 'request'].includes(key))) throw new Error('checkpoint_broker_operation_invalid');
      let value: unknown;
      if (input.method === 'profile') value = await options.driver.profile();
      else if (input.method === 'prepare') {
        if (!input.request || typeof input.request !== 'object') throw new Error('checkpoint_broker_request_invalid');
        await options.driver.prepare(input.request); value = null;
      } else {
        if (typeof input.requestId !== 'string' || !/^[a-f0-9]{64}$/.test(input.requestId)) throw new Error('checkpoint_request_id_invalid');
        if (input.method === 'checkpoint' || input.method === 'restore') {
          if (!Number.isSafeInteger(input.generation) || input.generation! < 0) throw new Error('checkpoint_generation_invalid');
          value = await options.driver[input.method](input.requestId, input.generation!);
        } else value = await options.driver[input.method as 'inspect' | 'start' | 'cancel'](input.requestId);
      }
      response.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
      await pipeline(Readable.from(jsonChunks({ value: value ?? null } as never)), response).catch(() => {});
    } catch (error) {
      // execFile errors can include command arguments and host diagnostics.
      // Keep them local rather than returning them to the remote model.
      const message = error instanceof Error && /^[a-z][a-z0-9_]+$/.test(error.message) ? error.message : 'checkpoint_broker_operation_failed';
      if (!response.headersSent) response.writeHead(503, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ error: message }));
    }
  });
  try { await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(socketPath, resolve); }); await chmod(socketPath, options.socketMode ?? 0o600); }
  catch (error) { if (mutex.isTransaction) mutex.exec('ROLLBACK'); mutex.close(); throw error; }
  return { server, async close() {
    await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    if (mutex.isTransaction) mutex.exec('ROLLBACK'); mutex.close();
  } };
}

/** HTTP worker uses this client without privileges. Disconnecting an observer
 * does not cancel an already running broker operation or scientific process. */
export class BrokerRecoveryDriver implements RecoveryDriver {
  readonly socketPath: string;
  constructor(socketPath: string) { this.socketPath = resolve(socketPath); }
  private async call<T>(operation: Operation): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const request = httpRequest({ socketPath: this.socketPath, path: '/operation', method: 'POST', headers: { 'content-type': 'application/json' } }, response => {
        response.setEncoding('utf8');
        void readJson(response).then(body => {
          const result = body as { error?: string; value: T };
          if (response.statusCode !== 200 || result.error) reject(new Error(result.error ?? 'checkpoint_broker_operation_failed'));
          else resolve(result.value);
        }, reject);
      });
      request.once('error', reject);
      void pipeline(Readable.from(jsonChunks(operation as never)), request).catch(reject);
    });
  }
  profile(): Promise<CheckpointProfile | null> { return this.call({ method: 'profile' }); }
  prepare(request: unknown): Promise<void> { return this.call({ method: 'prepare', request }); }
  inspect(requestId: string): Promise<CheckpointInspection> { return this.call({ method: 'inspect', requestId }); }
  checkpoint(requestId: string, generation: number): Promise<CheckpointManifest> { return this.call({ method: 'checkpoint', requestId, generation }); }
  restore(requestId: string, generation: number): Promise<void> { return this.call({ method: 'restore', requestId, generation }); }
  start(requestId: string): Promise<void> { return this.call({ method: 'start', requestId }); }
  cancel(requestId: string): Promise<void> { return this.call({ method: 'cancel', requestId }); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (!process.argv[2]) throw new Error('checkpoint_broker_configuration_required');
  const configuration = JSON.parse(await readFile(process.argv[2], 'utf8')) as { socketPath: string; socketMode?: number; driver: RuncRecoveryOptions };
  const broker = await createCheckpointBroker({ socketPath: configuration.socketPath, socketMode: configuration.socketMode,
    ownerDirectory: configuration.driver.dataRoot, driver: new RuncRecoveryDriver(configuration.driver) });
  process.once('SIGTERM', () => void broker.close()); process.once('SIGINT', () => void broker.close());
}
