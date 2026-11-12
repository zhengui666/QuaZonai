import './pm-prefix.test.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import { executeNative } from '../../research/plugins/native-portfolio.ts';
import { remoteNativeCapabilities } from '../../research/src/native-execution.ts';
import { object } from '../src/facts.ts';

// This explicit native acceptance test reuses the original exported catalog.
// Missing assets fail rather than substituting mocked native computation.
// It covers source identity/transport, not strategy performance or trading.
async function sourceFixture(variable: string, directory = false): Promise<JsonObject> {
  const configured = process.env[variable]; assert.ok(configured, `${variable} is required for actual remote native acceptance`);
  return object(JSON.parse(await readFile(directory ? join(configured, 'source.json') : configured, 'utf8')));
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

test('actual remote Nautilus preserves original catalog evidence with no cross-instance fallback', { timeout: 180_000 }, async () => {
  const binary = process.env.QZ_JOB_BINARY; assert.ok(binary, 'Matching QZ_JOB_BINARY is required for the independent worker');
  const source = await sourceFixture('QZ_PM_NATIVE_FIXTURE');
  const catalog = String(source.catalog_root ?? source.catalog);
  const request = source.catalog_root ? object(object(source.selection).selection)
    : object(object(object(source.request).forecast).selection);
  const frozenRequest = structuredClone(request);
  const root = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-remote-native-'));
  const scratch = join(root, 'client'); const isolated = join(root, 'other-catalog');
  await mkdir(scratch); await mkdir(isolated);
  const first = await worker(binary, join(root, 'first'), [catalog], 'first');
  const second = await worker(binary, join(root, 'second'), [isolated], 'second');
  try {
    const hash = createHash('sha256'); for await (const bytes of createReadStream(binary)) hash.update(bytes);
    const capability = await remoteNativeCapabilities(first.target);
    assert.equal(capability.implementationDigest, `sha256:${hash.digest('hex')}`);
    assert.ok(capability.commands.includes('catalog-prefix-digest'));
    assert.equal((await remoteNativeCapabilities(second.target)).implementationDigest, capability.implementationDigest);
    const args = ['--catalog', catalog];
    const local = await executeNative(binary, 'catalog-prefix-digest', request, args, scratch, BACKGROUND_CONTEXT);
    const remote = await executeNative(first.target, 'catalog-prefix-digest', request, args, scratch, BACKGROUND_CONTEXT);
    assert.deepEqual(remote, local, 'HTTP must preserve all original native digest rows, clocks and selection evidence');
    assert.match(String(remote.content_sha256), /^[a-f0-9]{64}$/);
    assert.ok(BigInt(String(remote.row_count)) > 1n, 'Requires real original catalog rows, not a fabricated worker result');
    assert.deepEqual(request, frozenRequest, 'Execution cannot rewrite the original temporal/availability selection');
    const receiptsBefore = await readdir(join(root, 'first', 'executions'));
    const separate = await executeNative(first.target, 'catalog-prefix-digest', request, args, scratch, BACKGROUND_CONTEXT);
    assert.deepEqual(separate, remote);
    const receiptsAfter = await readdir(join(root, 'first', 'executions'));
    assert.equal(receiptsAfter.length, receiptsBefore.length + 1,
      'Separate native invocations retain distinct identities, so cancellation cannot affect another task');
    await assert.rejects(executeNative(second.target, 'catalog-prefix-digest', request, args, scratch, BACKGROUND_CONTEXT), /nautilus_catalog_outside_configured_roots/);
    assert.deepEqual(await readdir(join(root, 'first', 'executions')), receiptsAfter, 'A selected instance cannot fall back to another worker');
    await assert.rejects(remoteNativeCapabilities({ ...first.target, tokenEnv: second.target.tokenEnv }), /nautilus_http_401/);
    assert.equal((await readdir(join(root, 'first', 'scratch'))).length, 0, 'Original native invocation scratch is cleaned after execution');
  } finally { await first.close(); await second.close(); await rm(root, { recursive: true, force: true }); }
});
