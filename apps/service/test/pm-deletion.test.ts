import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { createServer } from 'node:http';
import { test } from 'node:test';
import { awaitWithContext } from '@earendil-works/chord/context';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { TaskId } from '@earendil-works/pi-durable';
import type { PmMandateView } from '../../../packages/contracts/src/pm.ts';
import { fact, PmDocument, pmRecord, pmState } from '../src/pm-facts.ts';
import { idlePmDeleteMonitor } from '../src/pm-deletion.ts';
import { executeNativeValue, NativeRemoteUncertainty, retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import type { NativeExecutionRequest, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { context, setup } from './support.ts';

type Fixture = Awaited<ReturnType<typeof setup>>;
const encoded = encodeURIComponent;
async function record(state: Fixture, id: string) { return pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), id); }
async function fixture() {
  const state = await setup();
  state.options.config.datasets[0]!.data.request = { train_end_ns: '140', validation_end_ns: '180', forecast: {
    schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {},
  } };
  await state.restart();
  const input = { requestId: 'delete-original-pm', goal: 'Original controlled deletion boundary', datasetId: 'controlled', model: state.model, maxTrials: 1,
    funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000.000000000000000001', currency: 'USD' } } };
  const start = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', payload: input, headers: state.headers });
  assert.equal(start.statusCode, 202, start.body);
  const view = start.json<PmMandateView>();
  return { state, input, view, id: view.mandate.id, path: `/api/v3/mandates/${encoded(view.mandate.id)}` };
}
async function retire(f: Awaited<ReturnType<typeof fixture>>) {
  const payload = { requestId: 'delete-original-retirement', expectedRevision: f.view.mandate.revision };
  const response = await f.state.service.app.inject({ method: 'POST', url: `${f.path}/retire`, payload, headers: f.state.headers });
  assert.equal(response.statusCode, 202, response.body);
  const snapshot = await record(f.state, f.id);
  const control = snapshot.artifacts[`campaign-control:${f.view.campaigns[0]!.id}`]!;
  await f.state.service.research.harness.waitForTask(control.taskId as TaskId, context);
}
async function remove(f: Awaited<ReturnType<typeof fixture>>) {
  // Only an idle monitor can be ignored; an in-flight owner phase is a real
  // conflict, so repeat this idempotent DELETE after that phase has settled.
  const deadline = Date.now() + 8000;
  for (;;) {
    const response = await f.state.service.app.inject({ method: 'DELETE', url: f.path, headers: f.state.headers });
    if (response.statusCode !== 409 || response.json().error !== 'pm_delete_tasks_active') return response;
    assert.ok(Date.now() < deadline, response.body); await delay(15);
  }
}

test('PM deletion preserves original facts and identities through SQLite restart and blocks all public reuse', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    assert.equal((await f.state.service.app.inject({ method: 'DELETE', url: f.path })).statusCode, 401);
    const unretired = await f.state.service.app.inject({ method: 'DELETE', url: f.path, headers: f.state.headers });
    assert.equal(unretired.statusCode, 409); assert.equal(unretired.json().error, 'pm_delete_requires_retirement');
    const cycle = await f.state.service.app.inject({ method: 'POST', url: `${f.path}/recompute`, headers: f.state.headers,
      payload: { requestId: 'original-before-delete', expectedRevision: f.view.mandate.revision } });
    assert.equal(cycle.statusCode, 202, cycle.body);
    await f.state.service.research.harness.waitForTask(cycle.json<PmMandateView>().lastCycle!.runtime.taskId as TaskId, context);
    await retire(f);
    const before = await record(f.state, f.id);
    const deleted = await remove(f); assert.equal(deleted.statusCode, 200, deleted.body);
    const receipt = deleted.json(); assert.equal(receipt.kind, 'mandate'); assert.equal(receipt.id, f.id); assert.ok(receipt.deletedAt);
    const after = await record(f.state, f.id);
    const { deletedAt, ...retained } = after;
    assert.deepEqual(retained, before, 'deletion retains original trials, budgets, decisions, policies and receipts');
    assert.deepEqual((await remove(f)).json(), receipt, 'DELETE replay returns its original receipt');
    await f.state.restart();
    assert.deepEqual(await record(f.state, f.id), after, 'restart preserves the original tombstone and complete ledger');
    assert.deepEqual((await remove(f)).json(), receipt);
    const listed = await f.state.service.app.inject({ url: '/api/v3/mandates', headers: f.state.headers });
    assert.deepEqual(listed.json(), []);
    for (const path of [f.path, `${f.path}/events`, `${f.path}/decision`, `${f.path}/decisions`,
      `/api/v3/cycles/${encoded(before.latestCycleId!)}`, `/api/v3/campaigns/${encoded(f.view.campaigns[0]!.id)}`,
      `/api/v3/campaigns/${encoded(f.view.campaigns[0]!.id)}/trials`, `/api/v3/campaigns/${encoded(f.view.campaigns[0]!.id)}/decision`,
      `/api/v3/strategies/${encoded(f.view.strategies[0]!.id)}/versions`, `/api/v3/strategies/${encoded(f.view.strategies[0]!.id)}/assessments`]) {
      const response = await f.state.service.app.inject({ url: path, headers: f.state.headers });
      assert.equal(response.statusCode, 404, `${path}: ${response.body}`);
    }
    const command = { requestId: 'after-deletion', expectedRevision: f.view.mandate.revision };
    const writes = [
      { path: `${f.path}/recompute`, payload: command }, { path: `${f.path}/retire`, payload: command },
      { path: `${f.path}/refresh-data`, payload: command }, { path: `${f.path}/targets`, payload: { ...command, decisionId: before.currentDecisionId } },
      { path: `${f.path}/intent-clarifications`, payload: { ...command, statement: 'Original owner scope', boundaries: [], resolvedAmbiguities: [] } },
      { path: '/api/v3/campaigns', payload: { ...command, mandateId: f.id } },
      ...['pause', 'resume', 'close'].map(action => ({ path: `/api/v3/campaigns/${encoded(f.view.campaigns[0]!.id)}/${action}`, payload: command })),
      ...['pause', 'resume'].map(action => ({ path: `/api/v3/strategies/${encoded(f.view.strategies[0]!.id)}/${action}`, payload: command })),
    ];
    for (const write of writes) {
      const response = await f.state.service.app.inject({ method: 'POST', url: write.path, headers: f.state.headers, payload: write.payload });
      assert.equal(response.statusCode, 409, `${write.path}: ${response.body}`); assert.equal(response.json().error, 'mandate_deleted');
    }
    const replay = await f.state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', payload: f.input, headers: f.state.headers });
    assert.equal(replay.statusCode, 404, replay.body);
    assert.deepEqual(await record(f.state, f.id), after, 'old admissions and new commands cannot restore or alter the removed workspace');
  } finally { await f.state.cleanup(); }
});

test('PM deletion refuses unsettled retirement, target and outcome responsibility before hiding anything', { timeout: 30_000 }, async () => {
  const f = await fixture();
  try {
    await retire(f);
    for (const kind of ['retirement', 'target', 'outcome'] as const) {
      await f.state.service.research.harness.commit(async tx => {
        const current = pmRecord(pmState(await tx.doc(PmDocument)), f.id);
        const retirement = Object.values(current.retirements)[0]!; retirement.reconciliationRequired = kind === 'retirement';
        current.commands = kind === 'target' ? { original: { ...fact('original-target'), decisionId: 'original-decision', idempotencyKey: 'original-target',
          accountBindingId: 'original-account', basisRevision: 'original-basis', controlRevision: '0', payloadEvidenceRef: 'original-payload', validUntilNs: '1', state: 'ACKNOWLEDGED', externalReceiptRef: 'original-receipt' } } : {};
        current.outcomes = kind === 'outcome' ? { original: { ...fact('original-outcome'), receiptState: 'UNCERTAIN' } as never } : {};
      }, context);
      const response = await remove(f);
      assert.equal(response.statusCode, 409, response.body); assert.equal(response.json().error, 'pm_delete_external_obligations');
      assert.equal((await record(f.state, f.id)).deletedAt, undefined);
    }
    await f.state.restart();
    assert.equal((await record(f.state, f.id)).deletedAt, undefined);
  } finally { await f.state.cleanup(); }
});

test('PM deletion refuses an actual active Durable child and leaves its original execution intact', { timeout: 30_000 }, async () => {
  const f = await fixture();
  let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; });
  let entered!: () => void; const active = new Promise<void>(resolve => { entered = resolve; });
  const child = defineTask<{ mandateId: string }, { phase: 'hold' }, null>({ name: 'test.pm-delete-bound-child', version: 1,
    initial: () => ({ phase: 'hold' }), phases: { async hold(_task, runtime, ctx) {
      entered(); await awaitWithContext(held, ctx); await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
    } }, async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); } });
  try {
    f.state.options.extensions.push(defineExtension({ name: 'test.pm-delete-bound-child', tasks: [child] })); await f.state.restart();
    await retire(f);
    const taskId = await f.state.service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      return tx.createTask(child, { mandateId: f.id }, { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
    }, context);
    await active;
    const response = await f.state.service.app.inject({ method: 'DELETE', url: f.path, headers: f.state.headers });
    assert.equal(response.statusCode, 409, response.body); assert.equal(response.json().error, 'pm_delete_tasks_active');
    assert.equal((await f.state.service.research.harness.getTask(taskId, context))!.state.status, 'running');
    assert.equal((await record(f.state, f.id)).deletedAt, undefined);
    release(); await f.state.service.research.harness.waitForTask(taskId, context);
    assert.equal((await remove(f)).statusCode, 200);
  } finally { release(); await f.state.cleanup(); }
});

test('only the original idle monitor checkpoint can be ignored by PM deletion', () => {
  const task = (phase: string, kind = 'qz.pm.mandate-monitor', status = 'running') => ({ id: 7, kind, state: { status, checkpoint: { phase } } });
  for (const phase of ['start', 'observe']) assert.equal(idlePmDeleteMonitor(task(phase), 7), false, `${phase} can still be admitting or reading work`);
  assert.equal(idlePmDeleteMonitor(task('wait'), 7), true);
  assert.equal(idlePmDeleteMonitor(task('maintenance', 'qz.pm.mandate-monitor', 'waiting'), 7), true);
  assert.equal(idlePmDeleteMonitor(task('wait'), 8), false);
  assert.equal(idlePmDeleteMonitor(task('wait', 'another-task'), 7), false);
  assert.equal(idlePmDeleteMonitor(task('wait', 'qz.pm.mandate-monitor', 'completing'), 7), false);
});

test('PM deletion retains unknown remote work across restart until GET confirms the original terminal receipt', { timeout: 30_000 }, async t => {
  const f = await fixture();
  const requests: string[] = []; const failures: unknown[] = [];
  let original: NativeExecutionRequest | undefined;
  let mode: 'UNKNOWN' | 'RUNNING' | 'MISMATCH' | 'UNAVAILABLE' | 'HANG' | 'COMPLETED' = 'UNKNOWN';
  const realTimeout = AbortSignal.timeout.bind(AbortSignal); const deadlines: number[] = [];
  t.mock.method(AbortSignal, 'timeout', (milliseconds: number) => { deadlines.push(milliseconds); return realTimeout(milliseconds === 5000 && mode === 'HANG' ? 20 : milliseconds); });
  const server = createServer((request, response) => { void (async () => {
    requests.push(`${request.method} ${request.url}`);
    const reply = (value: unknown) => { response.setHeader('content-type', 'application/json'); response.end(JSON.stringify(value)); };
    if (request.url === '/v1/capabilities') return reply({ schemaVersion: 1, implementationDigest: `sha256:${'d'.repeat(64)}`, commands: ['simulate'] });
    if (request.method === 'POST' && request.url === '/v1/execute') {
      const chunks: Buffer[] = []; for await (const chunk of request) chunks.push(Buffer.from(chunk));
      original = JSON.parse(Buffer.concat(chunks).toString('utf8')) as NativeExecutionRequest;
      return reply({ schemaVersion: 1, requestId: original.requestId, status: 'UNKNOWN', error: 'controlled_original_outcome_unknown' });
    }
    assert.equal(request.method, 'GET'); assert.equal(request.url, `/v1/executions/${original!.requestId}`);
    if (mode === 'HANG') return;
    if (mode === 'UNAVAILABLE') { response.statusCode = 503; response.end(); return; }
    return reply({ schemaVersion: 1, requestId: mode === 'MISMATCH' ? '0'.repeat(64) : original!.requestId,
      status: mode === 'MISMATCH' ? 'COMPLETED' : mode, ...(mode === 'UNKNOWN' ? { error: 'controlled_original_outcome_unknown' } : {}),
      ...(mode === 'COMPLETED' || mode === 'MISMATCH' ? { result: { protocolOnly: true } } : {}) });
  })().catch(error => { failures.push(error); response.statusCode = 500; response.end(); }); });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'original-pm-deletion-worker', url: `http://127.0.0.1:${address.port}` };
  const child = defineTask<{ mandateId: string }, { phase: 'compute' }, null>({ name: 'test.pm-delete-remote-child', version: 1,
    initial: () => ({ phase: 'compute' }), phases: { async compute(task, runtime, ctx) {
      try {
        await withNativeRemoteTracking(task.id, runtime, ctx, () => executeNativeValue(target, 'simulate', {}, [], f.state.directory, ctx));
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
      } catch (error) { await retainNativeRemoteUncertainty(error, task.id, runtime, ctx); throw error; }
    } }, async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); } });
  try {
    f.state.options.extensions.push(defineExtension({ name: 'test.pm-delete-remote-child', tasks: [child] })); await f.state.restart();
    await retire(f);
    const taskId = await f.state.service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      return tx.createTask(child, { mandateId: f.id }, { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
    }, context);
    const task = await f.state.service.research.harness.waitForTask(taskId, context);
    assert.equal(task.state.outcome?.status, 'faulted', 'local failure does not establish remote cancellation');
    const family = async () => f.state.service.research.harness.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), context);
    const before = await family(); assert.equal(before!.requests[0]!.status, 'UNKNOWN');
    const denied = async () => {
      const response = await remove(f); assert.equal(response.statusCode, 409, response.body); assert.equal(response.json().error, 'native_remote_execution_unsettled');
      assert.equal((await record(f.state, f.id)).deletedAt, undefined); assert.deepEqual(await family(), before);
    };
    await denied();
    f.state.options.config.nautilus = { backtest: { id: 'changed-configuration-must-not-replace-original', url: 'http://127.0.0.1:1' } };
    await f.state.restart(); await denied();
    for (const next of ['RUNNING', 'MISMATCH', 'UNAVAILABLE', 'HANG'] as const) { mode = next; await denied(); }
    assert.ok(deadlines.includes(5000), 'receipt GETs use a bounded total deadline');
    mode = 'COMPLETED';
    const deleted = await remove(f); assert.equal(deleted.statusCode, 200, deleted.body);
    const settled = await family(); assert.equal(settled!.requests[0]!.status, 'COMPLETED');
    assert.equal(settled!.requests[0]!.requestId, original!.requestId); assert.deepEqual(settled!.requests[0]!.target, target);
    await f.state.restart(); assert.equal((await record(f.state, f.id)).deletedAt, deleted.json().deletedAt);
    assert.deepEqual(await family(), settled);
    assert.equal(requests.filter(item => item.startsWith('POST ')).length, 1);
    assert.equal(requests.filter(item => item.startsWith('DELETE ')).length, 0, 'deletion never issues remote cancellation or resubmission');
    assert.deepEqual(failures, []);
  } finally { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await f.state.cleanup(); }
});
