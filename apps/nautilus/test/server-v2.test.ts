import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import type { NativeCapabilitiesV2, NativeExecutionReceiptV2, NativeExecutionRequestV2 } from '../../../packages/contracts/src/nautilus.ts';
import type { CheckpointInspection, CheckpointManifest, RecoveryDriver } from '../src/checkpoint-driver.ts';
import { canonicalJsonDigest } from '../../research/src/native-json.ts';
import { createNautilusWorker } from '../src/server.ts';

/** Protocol fixture only. This deliberately does not establish CRIU or science
 * acceptance; production defaults advertise recovery:null without a driver. */
class ProtocolDriver implements RecoveryDriver<NativeExecutionRequestV2> {
  prepares = 0; starts = 0; cancellations = 0;
  restores: number[] = [];
  profileGate: Promise<void> | undefined;
  profileEntered: (() => void) | undefined;
  states = new Map<string, CheckpointInspection>();
  async profile() { this.profileEntered?.(); await this.profileGate; return { profileId: 'controlled-protocol-fixture', verifiedCommands: ['forecast'], checkpointIntervalMs: 60_000 }; }
  async inspect(id: string) { return this.states.get(id) ?? { phase: 'absent' as const }; }
  async prepare(request: NativeExecutionRequestV2) { if (!this.states.has(request.requestId)) { this.prepares++; this.states.set(request.requestId, { phase: 'prepared' }); } }
  async checkpoint(id: string, generation: number): Promise<CheckpointManifest> {
    this.states.set(id, { phase: 'stopped', currentGeneration: generation });
    return { generation, createdAt: new Date().toISOString(), profileId: 'controlled-protocol-fixture', requestId: id, contentDigest: 'fixture' };
  }
  async restore(id: string, generation: number) { this.restores.push(generation); this.states.set(id, { phase: 'running', currentGeneration: generation }); }
  async start(_id: string) { this.starts++; }
  async cancel(id: string) { this.cancellations++; this.states.set(id, { phase: 'stopped' }); }
}

async function fixture(driver?: ProtocolDriver) {
  const root = await mkdtemp(join(tmpdir(), 'qz-worker-v2-')); const binary = join(root, 'job'); await writeFile(binary, '#!/bin/sh\nexit 0\n', { mode: 0o700 });
  const options = { binary, dataDir: join(root, 'data'), catalogRoots: [root], recoveryDriver: driver };
  let worker = await createNautilusWorker(options);
  async function listen() { worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening'); return `http://127.0.0.1:${(worker.server.address() as AddressInfo).port}`; }
  let url = await listen();
  return { get url() { return url; }, async reopen() { await worker.close(); worker = await createNautilusWorker(options); url = await listen(); }, async close() { await worker.close(); await rm(root, { recursive: true, force: true }); } };
}
async function request(url: string): Promise<NativeExecutionRequestV2> {
  const cap = await (await fetch(`${url}/v2/capabilities`)).json() as NativeCapabilitiesV2;
  const content = { schemaVersion: 2 as const, instanceId: cap.instanceId, recoveryProfileId: cap.recovery?.profileId ?? 'not-verified',
    invocationId: randomUUID(), implementationDigest: cap.implementationDigest, command: 'forecast', input: {}, args: [], files: {} };
  return { ...content, requestId: canonicalJsonDigest(content) };
}
async function submit(url: string, value: NativeExecutionRequestV2) { return fetch(`${url}/v2/execute`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) }); }
async function poll(url: string, id: string, accept: (receipt: NativeExecutionReceiptV2) => boolean) {
  for (let count = 0; count < 100; count++) {
    const response = await fetch(`${url}/v2/executions/${id}`); const receipt = await response.json() as NativeExecutionReceiptV2;
    if (accept(receipt)) return receipt;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  throw new Error('controlled receipt did not settle');
}

test('V2 does not admit or advertise internal recovery without verified driver', async () => {
  const f = await fixture();
  try {
    const cap = await (await fetch(`${f.url}/v2/capabilities`)).json() as NativeCapabilitiesV2; assert.equal(cap.recovery, null);
    const value = await request(f.url); const response = await submit(f.url, value); assert.equal(response.status, 503);
    const missing = await fetch(`${f.url}/v2/executions/${value.requestId}`); assert.equal(missing.status, 404);
    assert.deepEqual(await missing.json(), { schemaVersion: 2, instanceId: cap.instanceId, requestId: value.requestId, error: 'nautilus_execution_not_found' });
    await f.reopen();
    assert.equal((await (await fetch(`${f.url}/v2/capabilities`)).json() as NativeCapabilitiesV2).instanceId, cap.instanceId);
  } finally { await f.close(); }
});

test('controlled V2 protocol retains original task across detached HTTP and Worker close/reopen', { timeout: 15_000 }, async () => {
  const driver = new ProtocolDriver(); const f = await fixture(driver);
  try {
    const value = await request(f.url);
    const initial = await submit(f.url, value); assert.equal(initial.status, 202);
    const receipt = await initial.json() as NativeExecutionReceiptV2;
    await poll(f.url, value.requestId, row => row.recovery.generation === 0 && driver.starts === 1);
    await f.reopen();
    const original = await (await submit(f.url, value)).json() as NativeExecutionReceiptV2;
    assert.equal(original.taskId, receipt.taskId); assert.equal(driver.prepares, 1); assert.equal(driver.starts, 1); assert.equal(driver.cancellations, 0);
    driver.states.set(value.requestId, { phase: 'completed', completion: { exitCode: 0, signal: null, result: { proof: 'controlled-original-result' } } });
    const result = await poll(f.url, value.requestId, row => row.status === 'COMPLETED');
    assert.equal(result.taskId, receipt.taskId); assert.equal(result.status, 'COMPLETED');
    if (result.status === 'COMPLETED') assert.deepEqual(result.result, { proof: 'controlled-original-result' });
    const cancelled = await fetch(`${f.url}/v2/executions/${value.requestId}`, { method: 'DELETE' });
    assert.equal((await cancelled.json() as NativeExecutionReceiptV2).status, 'COMPLETED'); assert.equal(driver.cancellations, 0);
  } finally { await f.close(); }
});

test('controlled V2 owner cancellation persists and only publishes after process driver confirms stop', { timeout: 15_000 }, async () => {
  const driver = new ProtocolDriver(); const f = await fixture(driver);
  try {
    const value = await request(f.url); await (await submit(f.url, value)).body?.cancel();
    await poll(f.url, value.requestId, row => row.recovery.generation === 0 && driver.starts === 1);
    const stopped = await fetch(`${f.url}/v2/executions/${value.requestId}`, { method: 'DELETE' });
    assert.equal((await stopped.json() as NativeExecutionReceiptV2).status, 'CANCELLED'); assert.equal(driver.cancellations, 1);
    await f.reopen(); assert.equal((await (await submit(f.url, value)).json() as NativeExecutionReceiptV2).status, 'CANCELLED'); assert.equal(driver.starts, 1);
  } finally { await f.close(); }
});

test('V2 changed original instance or body never admits a replacement execution', async () => {
  const f = await fixture(new ProtocolDriver());
  try {
    const value = await request(f.url);
    assert.equal((await submit(f.url, { ...value, input: { changed: true } })).status, 409);
    const { requestId: _id, ...content } = { ...value, instanceId: randomUUID() };
    assert.equal((await submit(f.url, { ...content, requestId: canonicalJsonDigest(content) })).status, 409);
  } finally { await f.close(); }
});

test('controlled V2 signal interruption restores the committed generation without another prepare', { timeout: 15_000 }, async () => {
  const driver = new ProtocolDriver(); const f = await fixture(driver);
  try {
    const value = await request(f.url); const original = await (await submit(f.url, value)).json() as NativeExecutionReceiptV2;
    await poll(f.url, value.requestId, row => row.recovery.generation === 0 && driver.starts === 1);
    driver.states.set(value.requestId, { phase: 'stopped', completion: { exitCode: null, signal: 'SIGKILL' } });
    const recovered = await poll(f.url, value.requestId, row => row.status === 'RUNNING' && driver.restores.length === 2);
    assert.equal(recovered.taskId, original.taskId); assert.deepEqual(driver.restores, [0, 0]); assert.equal(driver.prepares, 1);
    driver.states.set(value.requestId, { phase: 'completed', completion: { exitCode: 0, signal: null, result: { restored: true } } });
    await poll(f.url, value.requestId, row => row.status === 'COMPLETED');
  } finally { await f.close(); }
});

test('V2 pre-admission cancellation fences a delayed POST across Worker restart', async () => {
  const driver = new ProtocolDriver(); const f = await fixture(driver);
  try {
    const value = await request(f.url);
    const cancellation = await fetch(`${f.url}/v2/executions/${value.requestId}`, { method: 'DELETE' });
    assert.equal(cancellation.status, 200);
    assert.deepEqual(await cancellation.json(), { schemaVersion: 2, instanceId: value.instanceId, requestId: value.requestId, cancellation: 'FENCED_NOT_ADMITTED' });
    await f.reopen();
    const late = await submit(f.url, value); assert.equal(late.status, 409);
    assert.deepEqual(await late.json(), { error: 'nautilus_execution_cancelled_before_admission' });
    assert.equal(driver.prepares, 0); assert.equal(driver.starts, 0);
  } finally { await f.close(); }
});

test('V2 cancellation atomically fences POST already awaiting admission capability', async () => {
  const driver = new ProtocolDriver(); const f = await fixture(driver);
  let release!: () => void;
  try {
    const value = await request(f.url);
    const entered = new Promise<void>(resolve => { driver.profileEntered = resolve; });
    driver.profileGate = new Promise<void>(resolve => { release = resolve; });
    const admission = submit(f.url, value); await entered;
    const cancelled = await fetch(`${f.url}/v2/executions/${value.requestId}`, { method: 'DELETE' });
    assert.equal(cancelled.status, 200);
    assert.deepEqual(await cancelled.json(), { schemaVersion: 2, instanceId: value.instanceId, requestId: value.requestId, cancellation: 'FENCED_NOT_ADMITTED' });
    release(); assert.equal((await admission).status, 409);
    assert.equal(driver.prepares, 0); assert.equal(driver.starts, 0);
  } finally { release?.(); await f.close(); }
});
