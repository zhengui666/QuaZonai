import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, defineExtension, defineTask, Harness } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { NativeExecutionRequest, NativeExecutionRequestV2, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { openDurableStorage } from '../src/durable-storage.ts';
import { executeNativeValue, NativeSteps, withFreshNativeRead, withNativeRemoteTracking, withNativeStep } from '../src/native-execution.ts';
import { verifyOriginalPrefix } from '../../service/src/pm-science.ts';

const context = BACKGROUND_CONTEXT;
const implementationDigest = `sha256:${'a'.repeat(64)}`;
const instanceId = 'observation-fixture'; const profileId = 'protocol-only-profile';
const selection = { event_start_ns: '0', event_end_ns: '10' };
const hash = (value: string) => createHash('sha256').update(value).digest('hex');

async function source(protocolVersion: 1 | 2, interrupt: boolean) {
  const directory = await mkdtemp(join(tmpdir(), 'qz-fresh-native-observation-'));
  const catalogFile = join(directory, 'catalog-source'); await writeFile(catalogFile, 'original-catalog-bytes');
  const originalPrefix = { method: 'NAUTILUS_SELECTED_PREFIX_SHA256_V2', selection, content_sha256: hash('original-catalog-bytes') };
  const posts: (NativeExecutionRequest | NativeExecutionRequestV2)[] = [];
  let entered!: () => void; const accepted = new Promise<void>(resolve => { entered = resolve; });
  let waiting = interrupt; let gets = 0;
  const results = new Map<string, JsonObject>();
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json'); const send = (value: unknown) => response.end(JSON.stringify(value));
    if (request.url?.endsWith('/capabilities')) {
      send({ schemaVersion: protocolVersion, implementationDigest, commands: ['catalog-prefix-digest', 'simulate'], ...(protocolVersion === 2 ? { instanceId,
        recovery: { profileId, verifiedCommands: ['catalog-prefix-digest', 'simulate'], checkpointIntervalMs: 60_000 } } : {}) }); return;
    }
    let call: NativeExecutionRequest | NativeExecutionRequestV2;
    if (request.method === 'POST') {
      let body = ''; for await (const chunk of request) body += chunk; call = JSON.parse(body); posts.push(call); entered();
      let value: JsonObject;
      if (call.command === 'catalog-prefix-digest') value = { ...originalPrefix, content_sha256: hash(await readFile(catalogFile, 'utf8')) };
      else value = { originalScience: true };
      results.set(call.requestId, value);
    } else {
      gets++; call = posts.find(item => request.url?.endsWith(`/${item.requestId}`))!; assert.ok(call);
    }
    send({ schemaVersion: protocolVersion, requestId: call.requestId, status: waiting ? 'RUNNING' : 'COMPLETED',
      ...(waiting ? {} : { result: results.get(call.requestId) }), ...(protocolVersion === 2 ? { instanceId, taskId: posts.indexOf(call),
        recovery: { profileId, phase: 'read', generation: null, checkpointAt: null, blockedReason: null } } : {}) });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'observations', url: `http://127.0.0.1:${address.port}`, ...(protocolVersion === 2 ? { protocolVersion: 2 as const, instanceId, recoveryProfileId: profileId } : {}) };
  return { directory, catalogFile, originalPrefix, posts, accepted, target, get gets() { return gets; }, finish() { waiting = false; },
    async close() { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); } };
}

for (const version of [1, 2] as const) test(`V${version} verifies current catalog bytes again after science while reusing a completed scientific step`, { timeout: 15_000 }, async () => {
  const f = await source(version, false);
  const Compute = defineTask<{}, { phase: 'compute' }, JsonObject>({
    name: 'test.fresh-prefix', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      await withNativeRemoteTracking(task.id, runtime, ctx, async () => {
        const input = { catalogPrefix: f.originalPrefix, request: { forecast: { selection } } };
        await verifyOriginalPrefix(input, f.directory, f.target, f.directory, ctx, 'before-science');
        const run = () => withNativeStep('original-scientific-step', () => executeNativeValue(f.target, 'simulate', {}, [], f.directory, ctx));
        await run(); await writeFile(f.catalogFile, 'changed-during-science'); await run();
        let failure: string | null = null;
        try { await verifyOriginalPrefix(input, f.directory, f.target, f.directory, ctx, 'after-science'); }
        catch (error) { failure = String(error); }
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { failure } } }), ctx);
      });
    } }, async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.fresh-prefix', tasks: [Compute] }));
  const harness = await Harness.open(await openDurableStorage(join(f.directory, 'ledger.sqlite')), { registry, models: createModels() }, context); harness.resume();
  try {
    const id = await harness.commit(async tx => { const c = await tx.createConversation({ ownership: { kind: 'ownerless' } }); return tx.createTask(Compute, {}, { conversationId: c.id, ownership: { kind: 'conversation' } }); }, context);
    const task = await harness.waitForTask(id as TaskId<JsonObject>, context);
    assert.equal(task.state.outcome.status, 'completed', JSON.stringify(task.state.outcome));
    if (task.state.outcome.status === 'completed') assert.match(String(task.state.outcome.result.failure), /pm_original_catalog_prefix_changed/);
    const reads = f.posts.filter(call => call.command === 'catalog-prefix-digest');
    assert.equal(reads.length, 2); assert.notEqual(reads[0]!.requestId, reads[1]!.requestId);
    assert.equal(f.posts.filter(call => call.command === 'simulate').length, 1, 'a fresh-read observation never bypasses scientific result reuse');
  } finally { await harness.close(context); await f.close(); }
});

test('an interrupted protected observation reconciles its original request, then a new observation gets a new durable generation', { timeout: 15_000 }, async () => {
  const f = await source(2, true);
  const Compute = defineTask<{}, { phase: 'compute' }, JsonObject>({
    name: 'test.interrupted-observation', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      await withNativeRemoteTracking(task.id, runtime, ctx, async () => {
        const read = () => withFreshNativeRead('catalog-prefix-digest', 'current-source', () => executeNativeValue(f.target, 'catalog-prefix-digest', selection, ['--catalog', f.directory], f.directory, ctx));
        const before = await read(); await writeFile(f.catalogFile, 'new-observation-bytes'); const after = await read();
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { before, after } } }), ctx);
      });
    } }, async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.interrupted-observation', tasks: [Compute] }));
  const open = async () => { const h = await Harness.open(await openDurableStorage(join(f.directory, 'ledger.sqlite')), { registry, models: createModels() }, context); h.resume(); return h; };
  let harness = await open();
  try {
    const original = await harness.commit(async tx => { const c = await tx.createConversation({ ownership: { kind: 'ownerless' } }); return { conversationId: c.id, taskId: await tx.createTask(Compute, {}, { conversationId: c.id, ownership: { kind: 'conversation' } }) }; }, context);
    await f.accepted; await harness.close(context); f.finish(); harness = await open();
    const task = await harness.waitForTask(original.taskId as TaskId<JsonObject>, context);
    assert.equal(task.state.outcome.status, 'completed', JSON.stringify(task.state.outcome));
    if (task.state.outcome.status === 'completed') {
      assert.equal((task.state.outcome.result.before as JsonObject).content_sha256, hash('original-catalog-bytes'));
      assert.equal((task.state.outcome.result.after as JsonObject).content_sha256, hash('new-observation-bytes'));
    }
    assert.equal(f.posts.length, 2); assert.ok(f.gets > 0, 'the interrupted observation is collected by its original receipt');
    assert.notEqual(f.posts[0]!.requestId, f.posts[1]!.requestId);
    const facts = await harness.snapshot(NativeSteps, original.conversationId, String(original.taskId), context);
    assert.equal(Object.keys(facts!.steps).length, 2);
    assert.ok(Object.values(facts!.observations!).every(cursor => cursor.active === null && cursor.next === 2));
  } finally { await harness.close(context); await f.close(); }
});

test('fresh-read semantics reject scientific commands', () => {
  assert.throws(() => withFreshNativeRead('simulate', 'bypass', async () => null), /native_fresh_read_unsupported/);
});
