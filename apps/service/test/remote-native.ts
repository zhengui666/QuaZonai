import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { compileWasm, evaluateWasm } from '../../research/plugins/native-wasm.ts';
import { remoteNativeCapabilities } from '../../research/src/native-execution.ts';
import { object } from '../src/facts.ts';

// Run only in the native gate, after check-science exports its original
// fixtures. Missing assets fail this gate; a mocked or skipped run is not proof
// that the independent HTTP worker executes Nautilus science.
async function sourceFixture(variable: string, directory = false): Promise<JsonObject> {
  const configured = process.env[variable]; assert.ok(configured, `${variable} is required for actual remote native acceptance`);
  return object(JSON.parse(await readFile(directory ? join(configured, 'source.json') : configured, 'utf8')));
}

// JSON serializes -0 as 0. Preserve exact comparison for every other number
// and value; this does not apply an epsilon to scientific results.
function normalizeSignedZero(value: unknown): unknown {
  if (Object.is(value, -0)) return 0;
  if (Array.isArray(value)) return value.map(normalizeSignedZero);
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, normalizeSignedZero(item)]));
  }
  return value;
}

async function worker(binary: string, directory: string, catalogRoots: string[], id: string) {
  const tokenEnv = `QZ_TEST_REMOTE_NATIVE_${id.toUpperCase()}`;
  const previous = process.env[tokenEnv]; const token = `controlled-local-worker-${id}`;
  process.env[tokenEnv] = token;
  const instance = await createNautilusWorker({ binary, dataDir: directory, catalogRoots, token });
  instance.server.listen(0, '127.0.0.1'); await once(instance.server, 'listening');
  const target: RemoteNativeTarget = { id, url: `http://127.0.0.1:${(instance.server.address() as AddressInfo).port}`, tokenEnv };
  return { ...instance, target, async close() {
    await instance.close();
    if (previous === undefined) delete process.env[tokenEnv]; else process.env[tokenEnv] = previous;
  } };
}

test('actual remote Nautilus preserves factor, forecast and shared-account results with no cross-instance fallback', { timeout: 180_000 }, async () => {
  const binary = process.env.QZ_JOB_BINARY; assert.ok(binary, 'Matching QZ_JOB_BINARY is required for the independent worker');
  const source = await sourceFixture('QZ_PM_NATIVE_FIXTURE');
  assert.equal(source.fixtureProvenance, 'SYNTHETIC_TEST_ONLY_MICRO_LOT_ORIGINAL_NATIVE_PARQUET_WASM_CLARABEL_NAUTILUS');
  const catalog = String(source.catalog); const request = object(source.request);
  assert.ok(Array.isArray(source.members) && source.members.length >= 2);
  const model = String(object(source.members[0]).model);
  const root = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-remote-native-'));
  const scratch = join(root, 'client'); const isolated = join(root, 'other-catalog');
  await mkdir(scratch); await mkdir(isolated);
  const first = await worker(binary, join(root, 'first'), [catalog], 'first');
  const second = await worker(binary, join(root, 'second'), [isolated], 'second');
  try {
    const hash = createHash('sha256'); for await (const bytes of createReadStream(binary)) hash.update(bytes);
    const capability = await remoteNativeCapabilities(first.target);
    assert.equal(capability.implementationDigest, `sha256:${hash.digest('hex')}`);
    assert.ok(capability.commands.includes('simulate'));
    assert.equal((await remoteNativeCapabilities(second.target)).implementationDigest, capability.implementationDigest);
    const args = ['--catalog', catalog, '--model', join(scratch, 'selected.wasm')];
    await writeFile(args[3]!, compileWasm(model));
    const localFactor = await executeNative(binary, 'evaluate-factor', request, args, scratch, BACKGROUND_CONTEXT);
    const remoteFactor = await executeNative(first.target, 'evaluate-factor', request, args, scratch, BACKGROUND_CONTEXT);
    assert.deepEqual(remoteFactor, localFactor, 'Transferred WASM and native JSON must preserve complete original factor evidence');
    assert.deepEqual(remoteFactor.request, request);
    const summary = await evaluateWasm(model, { catalog, request }, BACKGROUND_CONTEXT, undefined, first.target);
    assert.equal(Object.hasOwn(summary, 'points'), false);
    assert.deepEqual(summary.partitions, remoteFactor.partitions);
    const forecastRequest = object(request.forecast);
    const localForecast = await executeNative(binary, 'forecast-factor', forecastRequest, args, scratch, BACKGROUND_CONTEXT);
    const remoteForecast = await executeNative(first.target, 'forecast-factor', forecastRequest, args, scratch, BACKGROUND_CONTEXT);
    assert.deepEqual(remoteForecast, localForecast);
    assert.ok(Array.isArray(remoteForecast.points) && remoteForecast.points.length > 0);
    const simulationRequest = object(source.simulationRequest);
    const localAccount = await executeNative(binary, 'simulate', simulationRequest, ['--catalog', catalog], scratch, BACKGROUND_CONTEXT);
    const remoteAccount = await executeNative(first.target, 'simulate', simulationRequest, ['--catalog', catalog], scratch, BACKGROUND_CONTEXT);
    for (const key of ['native_version', 'events', 'orders', 'positions', 'consumed_target_points', 'statistics', 'returns_kind', 'returns_status', 'returns_reason', 'returns', 'settled_replay_ns']) {
      assert.deepEqual(normalizeSignedZero(remoteAccount[key]), normalizeSignedZero(localAccount[key]), `Native account field ${key} must agree across the HTTP boundary`);
    }
    assert.equal(remoteAccount.native_version, '0.63.0');
    assert.ok(BigInt(String(remoteAccount.consumed_target_points)) > 0n);
    assert.ok(BigInt(String(remoteAccount.orders)) > 0n, 'The fixture must exercise native orders, not an empty account response');
    assert.ok(remoteAccount.canonical_result);
    const receiptsBefore = await readdir(join(root, 'first', 'executions'));
    const separate = await executeNative(first.target, 'simulate', simulationRequest, ['--catalog', catalog], scratch, BACKGROUND_CONTEXT);
    assert.deepEqual(separate.statistics, remoteAccount.statistics);
    const receiptsAfter = await readdir(join(root, 'first', 'executions'));
    assert.equal(receiptsAfter.length, receiptsBefore.length + 1,
      'Separate science calls retain distinct invocation identities even with identical input, so cancellation cannot affect another task');
    await assert.rejects(executeNative(second.target, 'evaluate-factor', request, args, scratch, BACKGROUND_CONTEXT), /nautilus_catalog_outside_configured_roots/);
    assert.deepEqual(await readdir(join(root, 'first', 'executions')), receiptsAfter, 'A different selected instance cannot fall back to the working instance');
    await assert.rejects(remoteNativeCapabilities({ ...first.target, tokenEnv: second.target.tokenEnv }), /nautilus_http_401/);
  } finally { await first.close(); await second.close(); await rm(root, { recursive: true, force: true }); }
});

test('actual remote forward calculation transfers original model/calibration objects and preserves native output', { timeout: 120_000 }, async () => {
  const binary = process.env.QZ_JOB_BINARY; assert.ok(binary, 'Matching QZ_JOB_BINARY is required for the independent worker');
  const source = await sourceFixture('QZ_PM_FORWARD_FIXTURE_DIR', true);
  assert.equal(source.evidenceKind, 'CONTROLLED_NATIVE_FORWARD_MODEL_TEST_NOT_LIVE_QUALIFICATION');
  const root = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-remote-objects-'));
  const scratch = join(root, 'client'); await mkdir(scratch);
  const instance = await worker(binary, join(root, 'worker'), [String(source.catalog)], 'objects');
  try {
    const request = object(source.request);
    const args = ['--catalog', String(source.catalog), '--objects', String(source.objectsDirectory)];
    const local = await executeNative(binary, 'build-observed-spot', request, args, scratch, BACKGROUND_CONTEXT);
    const remote = await executeNative(instance.target, 'build-observed-spot', request, args, scratch, BACKGROUND_CONTEXT);
    assert.deepEqual(remote, local, 'WASM/calibration UUID objects are transported with their exact bytes and original identifiers');
    assert.equal(remote.kind, 'OBSERVED_SPOT_FORWARD_BUILD'); assert.deepEqual(remote.request, request);
    assert.equal(object(remote.valuation).total, '1499.3');
    assert.ok(remote.target); assert.equal(remote.target_block_reason, null);
    assert.deepEqual(object(object(remote.valuation).request).initial_state, request.initial_state);
    assert.ok(Array.isArray(remote.trade_estimates) && remote.trade_estimates.length > 0);
    assert.equal(Object.hasOwn(remote, 'statistics'), false, 'A model estimate is not relabeled as historical or live account performance');
    assert.equal((await readdir(join(root, 'worker', 'scratch'))).length, 0, 'The worker cleans uploaded object bytes after the real child exits');
  } finally { await instance.close(); await rm(root, { recursive: true, force: true }); }
});
