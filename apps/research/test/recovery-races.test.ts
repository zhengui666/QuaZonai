import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setImmediate as turn } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxAssistantMessage, fauxProvider } from '@earendil-works/pi-ai';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { openResearch } from '../src/host.ts';

const context = BACKGROUND_CONTEXT;
const deferred = () => { let resolve!: () => void; const promise = new Promise<void>((done) => { resolve = done; }); return { promise, resolve }; };
async function failedStudy() {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-recovery-race-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const storage = await openNodeSqliteStorage(join(directory, 'original.sqlite'));
  const host = await openResearch({ database: join(directory, 'original.sqlite'), storage, models });
  faux.setResponses([fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 unsupported response test-only' })]);
  const receipt = await host.start('original-owner-key', { goal: 'Original finite research', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: { max_trials: 2 } });
  await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
  return { directory, host, storage, faux, receipt, async close() { await host.close(); await rm(directory, { recursive: true, force: true }); } };
}

test('owner stop and recovery validation/admission are serialized', { timeout: 10_000 }, async () => {
  const state = await failedStudy(); const entered = deferred(); const release = deferred();
  try {
    state.faux.setResponses([fauxAssistantMessage('Recovered original research')]);
    const original = state.host.harness.conversation.bind(state.host.harness);
    state.host.harness.conversation = async (id, ctx) => {
      const handle = await original(id, ctx); if (!handle) return handle;
      const submit = handle.submit.bind(handle);
      handle.submit = async (draft, inner) => { if (draft.requestId?.startsWith('qz.research.resume:')) { entered.resolve(); await release.promise; } return submit(draft, inner); };
      return handle;
    };
    const recovering = state.host.resume(state.receipt.conversationId, state.receipt.submissionId);
    await entered.promise;
    let stopFinished = false; const stopping = state.host.stop(state.receipt.conversationId).then(() => { stopFinished = true; });
    await turn(); const completedBeforeRecoveryAdmission = stopFinished;
    release.resolve(); await recovering; await stopping;
    assert.equal(completedBeforeRecoveryAdmission, false, 'stop cannot finish before the recovery admission it overlaps');
    assert.equal((await state.host.state(state.receipt.conversationId)).research!.final!.reason, 'owner_stop');
  } finally { release.resolve(); await state.close(); }
});

test('duplicate recovery replays the same receipt across a delayed state read', { timeout: 10_000 }, async () => {
  const state = await failedStudy(); const entered = deferred(); const release = deferred();
  const scan = state.storage.scanSubmissions.bind(state.storage); let pause = true;
  try {
    state.faux.setResponses([fauxAssistantMessage('Recovered original research')]);
    state.storage.scanSubmissions = async (...args) => { if (pause) { pause = false; entered.resolve(); await release.promise; } return scan(...args); };
    const first = state.host.resume(state.receipt.conversationId, state.receipt.submissionId);
    const firstResult = first.then(value => ({ value }), error => ({ error }));
    await entered.promise;
    const second = state.host.resume(state.receipt.conversationId, state.receipt.submissionId);
    const secondResult = second.then(value => ({ value }), error => ({ error }));
    await turn(); release.resolve();
    const results = await Promise.all([firstResult, secondResult]);
    assert.ok(results.every(result => 'value' in result), 'duplicate calls must not turn one original recovery into a409');
    assert.deepEqual(results[0], results[1]);
    const recovered = results[0]; assert.ok(recovered && 'value' in recovered);
    await (await state.host.harness.submission(recovered.value.submissionId, context))!.wait(context);
    assert.equal(state.faux.state.callCount, 2);
  } finally { release.resolve(); state.storage.scanSubmissions = scan; await state.close(); }
});

test('a colliding owner request cannot be returned as its own recovery receipt', { timeout: 10_000 }, async () => {
  const state = await failedStudy();
  try {
    const previous = await state.host.submission(state.receipt.submissionId); const original = state.host.harness.commit.bind(state.host.harness); let collide = true;
    // The native lookup is trusted metadata but may match a pre-existing owner request in this namespace.
    state.host.harness.commit = async (change, ctx) => { if (collide) { collide = false; return previous as never; } return original(change, ctx); };
    await assert.rejects(state.host.resume(state.receipt.conversationId, state.receipt.submissionId), /recovery_request_conflict/);
    assert.equal(state.faux.state.callCount, 1);
  } finally { await state.close(); }
});

async function unsubmittedStudy() {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-start-race-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'original.sqlite'), models };
  const host = await openResearch(options);
  const input = { goal: 'Original bounded research', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: { max_trials: 2 } };
  faux.setResponses([fauxAssistantMessage('Original research response')]);
  return { directory, faux, host, input, options };
}

test('owner stop waits for an overlapping original start; replay never submits after stop', { timeout: 10_000 }, async () => {
  const state = await unsubmittedStudy(); const entered = deferred(); const release = deferred(); let host = state.host;
  let conversationId = -1;
  try {
    const original = host.harness.conversation.bind(host.harness);
    host.harness.conversation = async (id, ctx) => {
      const handle = await original(id, ctx); if (!handle) return handle;
      const submit = handle.submit.bind(handle);
      handle.submit = async (draft, inner) => { if (draft.requestId === 'original-start') { conversationId = id; entered.resolve(); await release.promise; } return submit(draft, inner); };
      return handle;
    };
    const starting = host.start('original-start', state.input);
    await entered.promise;
    let stopped = false; const stopping = host.stop(conversationId).then(() => { stopped = true; });
    await turn(); const stoppedBeforeAdmission = stopped;
    release.resolve(); const receipt = await starting; await stopping;
    assert.equal(stoppedBeforeAdmission, false, 'stop must fence the original submit before reporting completion');
    assert.equal((await host.state(conversationId)).research!.final!.reason, 'owner_stop');
    const calls = state.faux.state.callCount;
    assert.deepEqual(await host.start('original-start', state.input), receipt);
    await assert.rejects(host.start('original-start', { ...state.input, goal: 'changed' }), /request_id_conflict/);
    await host.close(); host = await openResearch(state.options);
    assert.deepEqual(await host.start('original-start', state.input), receipt);
    assert.equal(state.faux.state.callCount, calls, 'stopped original requests are receipts, not fresh model submissions');
    assert.equal((await host.list()).items.length, 1);
  } finally { release.resolve(); await host.close(); await rm(state.directory, { recursive: true, force: true }); }
});

test('close drains an original start waiting at native admission', { timeout: 10_000 }, async () => {
  const state = await unsubmittedStudy(); const entered = deferred(); const release = deferred();
  try {
    const original = state.host.harness.conversation.bind(state.host.harness);
    state.host.harness.conversation = async (id, ctx) => {
      const handle = await original(id, ctx); if (!handle) return handle;
      const submit = handle.submit.bind(handle);
      handle.submit = async (draft, inner) => { entered.resolve(); await release.promise; return submit(draft, inner); };
      return handle;
    };
    const starting = state.host.start('start-before-close', state.input); await entered.promise;
    let closed = false; const closing = state.host.close().then(() => { closed = true; });
    await turn(); const closedBeforeAdmission = closed;
    await assert.rejects(state.host.start('late-start', state.input), /research_closing/);
    release.resolve(); await starting; await closing;
    assert.equal(closedBeforeAdmission, false, 'host storage stays open until its admitted start settles');
  } finally { release.resolve(); await state.host.close(); await rm(state.directory, { recursive: true, force: true }); }
});

test('an owner stop before missing original admission prevents retry and startup repair', { timeout: 10_000 }, async () => {
  const state = await unsubmittedStudy(); let host = state.host; let conversationId = -1;
  try {
    const original = host.harness.conversation.bind(host.harness);
    host.harness.conversation = async (id, ctx) => {
      const handle = await original(id, ctx); if (!handle) return handle;
      handle.submit = async () => { conversationId = id; throw new Error('controlled_admission_gap'); };
      return handle;
    };
    await assert.rejects(host.start('interrupted-start', state.input), /controlled_admission_gap/);
    host.harness.conversation = original;
    await host.stop(conversationId);
    await assert.rejects(host.start('interrupted-start', state.input), /research_already_final/);
    await host.close(); host = await openResearch(state.options);
    await assert.rejects(host.start('interrupted-start', state.input), /research_already_final/);
    assert.equal(await host.submissionFor(conversationId), null);
    const study = (await host.state(conversationId)).research!;
    assert.deepEqual(study.input, state.input); assert.deepEqual(study.trials, []);
    assert.equal(study.final!.reason, 'owner_stop'); assert.equal(state.faux.state.callCount, 0);
  } finally { await host.close(); await rm(state.directory, { recursive: true, force: true }); }
});
