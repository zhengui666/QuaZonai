import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile, unlink } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, defineExtension, defineTask, Harness } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { NativeExecutionRequestV2, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { openDurableStorage } from '../src/durable-storage.ts';
import { cancelNativeRemoteTasks, executeNativeValue, hasNativeStepRecovery, NativeRemoteUncertainty, NativeSteps, withNativeRemoteTracking, withNativeStep } from '../src/native-execution.ts';

const context = BACKGROUND_CONTEXT;
const implementation = `sha256:${'a'.repeat(64)}`;
const instanceId = 'original-worker'; const profileId = 'verified-test-profile';
function latch() { let signal!: () => void; const promise = new Promise<void>(resolve => { signal = resolve; }); return { promise, signal }; }
async function fixture(mode: 'accepted' | 'lost-before-admission' | 'cached' | 'race' | 'legacy') {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-step-'));
  const posted = latch(); const collected = latch(); const cancellationEntered = latch(); const readEntered = latch();
  const requests: NativeExecutionRequestV2[] = []; let reads = 0; let deletes = 0; let caps = 0;
  let complete = mode === 'cached'; let holdAfterCollection = mode === 'cached';
  let cancellationAvailable = true; let replacementInstance = false;
  let cancellationFence = false; const capabilityWaiters: (() => void)[] = [];
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    const send = (value: unknown) => response.end(JSON.stringify(value));
    if (request.url === '/v2/capabilities') {
      caps++;
      if (mode === 'race') await new Promise<void>(resolve => { capabilityWaiters.push(resolve); if (capabilityWaiters.length === 2) for (const waiter of capabilityWaiters) waiter(); });
      send({ schemaVersion: 2, instanceId, implementationDigest: implementation, commands: ['simulate'], recovery: { profileId, verifiedCommands: ['simulate'], checkpointIntervalMs: 60_000 } }); return;
    }
    if (request.method === 'POST') {
      assert.equal(request.url, '/v2/execute'); let body = ''; for await (const chunk of request) body += chunk;
      requests.push(JSON.parse(body)); posted.signal();
      if (mode === 'lost-before-admission' && requests.length === 1) return;
    } else if (request.method === 'DELETE') { deletes++; cancellationEntered.signal(); if (!cancellationAvailable) return; }
    else { reads++; readEntered.signal(); assert.equal(request.url, `/v2/executions/${requests[0]!.requestId}`); }
    if (replacementInstance) {
      response.statusCode = 404; send({ schemaVersion: 2, instanceId: 'replacement-worker', requestId: requests[0]!.requestId, error: 'nautilus_execution_not_found' }); return;
    }
    if (request.method === 'DELETE' && cancellationFence) {
      send({ schemaVersion: 2, instanceId, requestId: requests[0]!.requestId, cancellation: 'FENCED_NOT_ADMITTED' }); return;
    }
    if (mode === 'lost-before-admission' && requests.length === 1) {
      response.statusCode = 404; send({ schemaVersion: 2, instanceId, requestId: requests[0]!.requestId, error: 'nautilus_execution_not_found' }); return;
    }
    const status = request.method === 'DELETE' ? 'CANCELLED' : complete || mode === 'race' ? 'COMPLETED' : 'RUNNING';
    send({ schemaVersion: 2, instanceId, requestId: requests[0]!.requestId, taskId: 12,
      status, recovery: { profileId, phase: 'execute', generation: 3, checkpointAt: '2026-11-09T00:00:00.000Z', blockedReason: null },
      ...(status === 'COMPLETED' ? { result: { source: 'original-result', amount: '9007199254740993123' } } : {}) });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'original', url: `http://127.0.0.1:${address.port}`, protocolVersion: 2, instanceId, recoveryProfileId: profileId };
  const model = join(directory, 'model.wasm'); await writeFile(model, '(original-model)');
  const otherModel = join(directory, 'other-model.wasm'); await writeFile(otherModel, '(changed-model)');
  const Compute = defineTask<{}, { phase: 'compute' }, JsonObject>({
    name: 'test.protected-native-step', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      try {
        if (mode === 'race') {
          const outcomes = await withNativeRemoteTracking(task.id, runtime, ctx, () => Promise.allSettled([model, otherModel].map(path => withNativeStep('same-original-step', () => executeNativeValue(target, 'simulate', { original: true }, ['--model', path], directory, ctx)))));
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { outcomes: outcomes.map(outcome => { const value: JsonObject = { status: outcome.status }; if (outcome.status === 'rejected') value.reason = String(outcome.reason); return value; }) } } }), ctx);
          return;
        }
        const result = await withNativeRemoteTracking(task.id, runtime, ctx, () => withNativeStep('original-model/account-replay', () => executeNativeValue(target, 'simulate', { original: true }, ['--model', model], directory, ctx)));
        if (mode === 'cached') {
          assert.equal(await hasNativeStepRecovery(task.id, runtime, ctx, { command: 'simulate', input: { original: true } }), true, 'a collected receipt remains recoverable before a consumer commits its own result');
          assert.equal(await hasNativeStepRecovery(task.id, runtime, ctx, { command: 'simulate', input: { unrelated: true } }), false, 'an unrelated reservation cannot borrow this receipt');
        }
        collected.signal();
        if (holdAfterCollection) await runtime.sleep(runtime.now() + 120_000, ctx);
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: result as JsonObject } }), ctx);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: String(error) } } }), ctx);
      }
    } },
    async abort(task, runtime, ctx) {
      while (!await cancelNativeRemoteTasks(task.id, runtime, ctx)) await runtime.sleep(runtime.now() + 1000, ctx);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx);
    },
  });
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.protected-native-steps', tasks: [Compute] }));
  const open = async () => {
    const harness = await Harness.open(await openDurableStorage(join(directory, 'research.sqlite')), { registry, models: createModels() }, context);
    harness.resume(); return harness;
  };
  let harness = await open();
  const original = await harness.commit(async tx => {
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
    const taskId = await tx.createTask(Compute, {}, { ownership: { kind: 'conversation' }, conversationId: conversation.id });
    if (mode === 'legacy') (await tx.doc(NativeRemoteUncertainty, conversation.id, String(taskId), null)).requests.push({ taskId, requestId: 'f'.repeat(64), target: target as unknown as JsonObject, status: 'PENDING', error: 'nautilus_execution_unknown' });
    return { conversationId: conversation.id, taskId };
  }, context);
  return { directory, target, requests, posted, collected, cancellationEntered, readEntered, original,
    get harness() { return harness; }, get reads() { return reads; }, get deletes() { return deletes; }, get caps() { return caps; },
    complete() { complete = true; holdAfterCollection = false; },
    cancellation(available: boolean) { cancellationAvailable = available; },
    replacement(value: boolean) { replacementInstance = value; },
    fence() { cancellationFence = true; },
    async reopen() { await harness.close(context); await unlink(model); harness = await open(); },
    async closeHost() { await harness.close(context); },
    async close() { await harness.close(context); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); },
  };
}

test('protected invocation reconnects to the original result after Harness close without resubmission or cancellation', { timeout: 15_000 }, async () => {
  const f = await fixture('accepted');
  try {
    await f.posted.promise;
    await f.closeHost();
    assert.equal(f.deletes, 0, 'closing the Harness only detaches from remote science');
    f.complete(); await f.reopen();
    const task = await f.harness.waitForTask(f.original.taskId as TaskId<JsonObject>, context);
    assert.equal(task.state.outcome.status, 'completed', JSON.stringify(task.state.outcome));
    if (task.state.outcome.status === 'completed') assert.deepEqual(task.state.outcome.result, { source: 'original-result', amount: '9007199254740993123' });
    assert.equal(f.requests.length, 1); assert.equal(f.caps, 1, 'recovery uses the frozen implementation rather than rebinding capabilities');
    assert.ok(f.reads > 0); assert.equal(f.deletes, 0);
    const facts = await f.harness.snapshot(NativeSteps, f.original.conversationId, String(f.original.taskId), context);
    const step = Object.values(facts!.steps)[0]!; const request = step.request as unknown as NativeExecutionRequestV2;
    assert.deepEqual(request, f.requests[0]);
    assert.equal(Buffer.from(request.files['model-0']!, 'base64').toString(), '(original-model)', 'complete attachment bytes survive removal of phase scratch');
  } finally { await f.close(); }
});

test('an original-instance absence receipt permits replay of the exact saved request after lost pre-admission POST', { timeout: 15_000 }, async () => {
  const f = await fixture('lost-before-admission');
  try {
    await f.posted.promise; await f.closeHost(); f.complete(); await f.reopen();
    const task = await f.harness.waitForTask(f.original.taskId, context);
    assert.equal(task.state.outcome.status, 'completed', JSON.stringify(task.state.outcome));
    assert.equal(f.requests.length, 2); assert.deepEqual(f.requests[1], f.requests[0]);
    assert.equal(f.deletes, 0);
  } finally { await f.close(); }
});

test('a result committed before the parent Task finishes is reused locally after reopen', { timeout: 15_000 }, async () => {
  const f = await fixture('cached');
  try {
    await f.collected.promise; await f.closeHost();
    const priorReads = f.reads; f.complete(); await f.reopen();
    const task = await f.harness.waitForTask(f.original.taskId, context);
    assert.equal(task.state.outcome.status, 'completed', JSON.stringify(task.state.outcome));
    assert.equal(f.requests.length, 1); assert.equal(f.reads, priorReads, 'the collected original result does not require another remote read');
    assert.equal(f.deletes, 0);
  } finally { await f.close(); }
});

test('a replacement worker absence response never authorizes another protected POST', { timeout: 15_000 }, async () => {
  const f = await fixture('accepted');
  try {
    await f.posted.promise; await f.closeHost(); f.replacement(true); await f.reopen();
    await f.readEntered.promise;
    await new Promise(resolve => setTimeout(resolve, 20));
    const waiting = await f.harness.commit(tx => tx.task(f.original.taskId), context);
    assert.notEqual(waiting!.state.status, 'terminal');
    assert.equal(f.requests.length, 1, 'a different instance cannot prove the original invocation absent');
    f.replacement(false); f.complete();
    const task = await f.harness.waitForTask(f.original.taskId, context);
    assert.equal(task.state.outcome.status, 'completed'); assert.equal(f.requests.length, 1);
  } finally { await f.close(); }
});

test('protected owner cancellation stays pending through transport loss and completes the original DELETE after restart', { timeout: 15_000 }, async t => {
  const timeout = AbortSignal.timeout.bind(AbortSignal);
  t.mock.method(AbortSignal, 'timeout', (ms: number) => timeout(ms === 15_000 ? 10 : ms));
  const f = await fixture('accepted');
  try {
    f.cancellation(false); await f.posted.promise;
    await f.harness.abortTask(f.original.taskId, context); await f.cancellationEntered.promise;
    await new Promise(resolve => setTimeout(resolve, 30));
    const waiting = await f.harness.commit(tx => tx.task(f.original.taskId), context);
    assert.notEqual(waiting!.state.status, 'terminal', 'unconfirmed remote termination remains pending');
    const pending = await f.harness.snapshot(NativeRemoteUncertainty, f.original.conversationId, String(f.original.taskId), context);
    assert.equal(pending!.requests[0]!.cancellationRequested, true); assert.equal(pending!.requests[0]!.status, 'PENDING');
    await f.closeHost(); const beforeRepair = f.deletes; f.cancellation(true); await f.reopen();
    const task = await f.harness.waitForTask(f.original.taskId, context);
    assert.equal(task.state.outcome.status, 'aborted'); assert.equal(f.requests.length, 1); assert.equal(f.deletes, beforeRepair + 1);
    const settled = await f.harness.snapshot(NativeRemoteUncertainty, f.original.conversationId, String(f.original.taskId), context);
    assert.equal(settled!.requests[0]!.requestId, f.requests[0]!.requestId); assert.equal(settled!.requests[0]!.status, 'CANCELLED');
  } finally { await f.close(); }
});

test('an explicit cancellation fence settles a request that never reached remote admission', { timeout: 15_000 }, async () => {
  const f = await fixture('lost-before-admission');
  try {
    await f.posted.promise; f.fence(); await f.harness.abortTask(f.original.taskId, context);
    const task = await f.harness.waitForTask(f.original.taskId, context);
    assert.equal(task.state.outcome.status, 'aborted'); assert.equal(f.requests.length, 1);
    const facts = await f.harness.snapshot(NativeRemoteUncertainty, f.original.conversationId, String(f.original.taskId), context);
    assert.equal(facts!.requests[0]!.status, 'CANCELLED'); assert.equal(facts!.requests[0]!.admission, 'NOT_ADMITTED');
    assert.equal(facts!.requests[0]!.requestId, f.requests[0]!.requestId);
  } finally { await f.close(); }
});

test('concurrent same-step calls with different model bytes conflict before a second POST', { timeout: 15_000 }, async () => {
  const f = await fixture('race');
  try {
    const task = await f.harness.waitForTask(f.original.taskId as TaskId<JsonObject>, context);
    assert.equal(task.state.outcome.status, 'completed', JSON.stringify(task.state.outcome));
    if (task.state.outcome.status === 'completed') {
      const outcomes = task.state.outcome.result.outcomes as JsonObject[];
      assert.equal(outcomes.filter(outcome => outcome.status === 'fulfilled').length, 1);
      assert.equal(outcomes.filter(outcome => outcome.status === 'rejected' && String(outcome.reason).includes('native_step_identity_conflict')).length, 1);
    }
    assert.equal(f.requests.length, 1);
  } finally { await f.close(); }
});

test('a legacy uncertainty without the original complete request cannot authorize a replacement invocation', { timeout: 15_000 }, async () => {
  const f = await fixture('legacy');
  try {
    const task = await f.harness.waitForTask(f.original.taskId, context);
    assert.equal(task.state.outcome.status, 'failed');
    if (task.state.outcome.status === 'failed') assert.match(task.state.outcome.error.message, /native_legacy_step_requires_reconciliation/);
    assert.equal(f.requests.length, 0); assert.equal(f.caps, 0);
    const original = await f.harness.snapshot(NativeRemoteUncertainty, f.original.conversationId, String(f.original.taskId), context);
    assert.equal(original!.requests[0]!.requestId, 'f'.repeat(64));
  } finally { await f.close(); }
});
