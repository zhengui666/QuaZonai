import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { defineExtension } from '@earendil-works/pi-durable';
import type { TaskId } from '@earendil-works/pi-durable';
import type { NativeExecutionRequestV2, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { openResearch } from '../../research/src/host.ts';
import { CapitalDocumentValidation, CapitalValidationRequests, validateCapitalDocumentDurable } from '../src/capital-validation.ts';
import { configDefaults } from '../src/config.ts';

const context = BACKGROUND_CONTEXT;
const implementationDigest = `sha256:${'a'.repeat(64)}`;
const instanceId = 'capital-protocol-fixture'; const profileId = 'controlled-only';
const extension = defineExtension({ name: 'test.capital-validation', tasks: [CapitalDocumentValidation] });

/** Loopback protocol acceptance only; this is not a native CRIU or scientific
 * validation fixture. The fixture deliberately labels its controlled profile. */
async function peer() {
  const requests: NativeExecutionRequestV2[] = []; let complete = true; let fail = false; let deletes = 0;
  let signal!: () => void; const posted = new Promise<void>(resolve => { signal = resolve; });
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    const send = (value: unknown) => response.end(JSON.stringify(value));
    if (request.url === '/v2/capabilities') return send({ schemaVersion: 2, instanceId, implementationDigest,
      commands: ['validate-capital-exit'], recovery: { profileId, verifiedCommands: ['validate-capital-exit'], checkpointIntervalMs: 60_000 } });
    if (request.method === 'POST') {
      let body = ''; for await (const bytes of request) body += bytes;
      requests.push(JSON.parse(body)); signal();
    } else if (request.method === 'DELETE') deletes++;
    const original = requests[requests.length - 1]!;
    const status = request.method === 'DELETE' ? 'CANCELLED' : fail ? 'FAILED' : complete ? 'COMPLETED' : 'RUNNING';
    send({ schemaVersion: 2, instanceId, requestId: original.requestId, taskId: 700, status,
      recovery: { profileId, phase: 'validate', generation: 3, checkpointAt: '2026-11-09T00:00:00.000Z', blockedReason: null },
      ...(status === 'COMPLETED' ? { result: { valid: true } } : status === 'FAILED' ? { error: 'fixture_contract_rejected' } : {}) });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'capital-worker', url: `http://127.0.0.1:${address.port}`, protocolVersion: 2, instanceId, recoveryProfileId: profileId };
  return { target, requests, posted, get deletes() { return deletes; }, set complete(value: boolean) { complete = value; }, set fail(value: boolean) { fail = value; },
    close: () => new Promise<void>((resolve, reject) => { server.closeAllConnections(); server.close(error => error ? reject(error) : resolve()); }) };
}

test('capital document validation uses one original official task and V2 request across duplicate reads and restart', { timeout: 15_000 }, async () => {
  const remote = await peer(); remote.complete = false;
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-validation-'));
  const config = configDefaults({ dataDir: directory, nautilus: { backtest: remote.target } });
  const input = { amount: '9007199254740993.000000000000000001', revision: '9007199254740995' };
  const options = { database: config.research.database, models: createModels(), extensions: [extension] };
  let host = await openResearch(options);
  let waiting: Promise<unknown> | undefined;
  try {
    waiting = validateCapitalDocumentDurable(host, config, 'VIEW', input).then(() => null, error => error);
    await remote.posted;
    const before = await host.harness.snapshot(CapitalValidationRequests, context);
    assert.equal(Object.keys(before!.records).length, 1);
    assert.deepEqual(remote.requests[0]!.input, { kind: 'VIEW', value: input });
    await host.close(); await waiting;
    assert.equal(remote.deletes, 0, 'host suspension never turns validation into explicit remote cancellation');
    remote.complete = true; host = await openResearch(options);
    await validateCapitalDocumentDurable(host, config, 'VIEW', structuredClone(input));
    await validateCapitalDocumentDurable(host, config, 'VIEW', input);
    assert.equal(remote.requests.length, 1, 'restart and duplicate calls collect the same original invocation');
    assert.deepEqual(await host.harness.snapshot(CapitalValidationRequests, context), before);
    assert.equal(remote.requests[0]!.command, 'validate-capital-exit');
  } finally { await host.close(); await waiting; await remote.close(); await rm(directory, { recursive: true, force: true }); }
});

test('confirmed V2 rejection remains the original failed validation instead of admitting another invocation', { timeout: 15_000 }, async () => {
  const remote = await peer(); remote.fail = true;
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-rejection-'));
  const config = configDefaults({ dataDir: directory, nautilus: { backtest: remote.target } });
  const host = await openResearch({ database: config.research.database, models: createModels(), extensions: [extension] });
  try {
    await assert.rejects(validateCapitalDocumentDurable(host, config, 'REQUEST', { original: true }), /capital_validation_failed/);
    await assert.rejects(validateCapitalDocumentDurable(host, config, 'REQUEST', { original: true }), /capital_validation_failed/);
    assert.equal(remote.requests.length, 1);
    const record = Object.values((await host.harness.snapshot(CapitalValidationRequests, context))!.records)[0]!;
    const task = await host.harness.commit(tx => tx.task(Number(record.taskId) as TaskId), context);
    assert.equal(task?.state.status, 'terminal');
    if (task?.state.status === 'terminal' && task.state.outcome.status === 'failed') assert.equal(task.state.outcome.error?.message, 'fixture_contract_rejected');
  } finally { await host.close(); await remote.close(); await rm(directory, { recursive: true, force: true }); }
});

test('explicit official validation cancellation stops only its original V2 request', { timeout: 15_000 }, async () => {
  const remote = await peer(); remote.complete = false;
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-stop-'));
  const config = configDefaults({ dataDir: directory, nautilus: { backtest: remote.target } });
  const host = await openResearch({ database: config.research.database, models: createModels(), extensions: [extension] });
  const waiting = validateCapitalDocumentDurable(host, config, 'VIEW', { original: true }).then(() => null, error => error);
  try {
    await remote.posted;
    const record = Object.values((await host.harness.snapshot(CapitalValidationRequests, context))!.records)[0]!;
    await host.harness.abortTask(Number(record.taskId) as TaskId, context);
    assert.match(String(await waiting), /capital_validation_cancelled/);
    assert.equal(remote.requests.length, 1); assert.equal(remote.deletes, 1);
  } finally { await host.close(); await waiting; await remote.close(); await rm(directory, { recursive: true, force: true }); }
});

test('legacy local validation retains its public invalid-contract status', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-capital-local-')); const binary = join(directory, 'controlled-job');
  await writeFile(binary, '#!/bin/sh\ncat >/dev/null\nexit 1\n', { mode: 0o700 });
  const config = configDefaults({ dataDir: directory, jobBinary: binary });
  const host = await openResearch({ database: config.research.database, models: createModels(), extensions: [extension] });
  try {
    await assert.rejects(validateCapitalDocumentDurable(host, config, 'REQUEST', {}), error =>
      error instanceof Error && error.message === 'native_capital_exit_contract_invalid' && (error as { statusCode?: number }).statusCode === 422);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
