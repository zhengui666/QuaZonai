import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { test } from 'node:test';
import { execFile } from 'node:child_process';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { awaitWithContext } from '@earendil-works/chord/context';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { TaskId } from '@earendil-works/pi-durable';
import { Application, json } from '../src/facts.ts';
import { maintenanceState } from '../src/pm-maintenance.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';
import { context, setup } from './support.ts';

type Fixture = Awaited<ReturnType<typeof setup>>;
type Status = Awaited<ReturnType<Fixture['service']['maintenance']['status']>>;
async function read(state: Fixture) {
  const response = await state.service.app.inject({ url: '/api/v3/maintenance', headers: state.headers });
  assert.equal(response.statusCode, 200, response.body); return response.json<Status>();
}
async function post(state: Fixture, action: 'enter' | 'resume', payload: object, code = 202) {
  const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/maintenance/${action}`, headers: state.headers, payload });
  assert.equal(response.statusCode, code, response.body); return response.json();
}
async function until<T>(read: () => Promise<T>, accept: (value: T) => boolean): Promise<T> {
  const deadline = Date.now() + 12_000;
  for (;;) { const value = await read(); if (accept(value)) return value; if (Date.now() >= deadline) assert.fail(JSON.stringify(value)); await delay(15); }
}

// Actual authenticated HTTP, official Durable tasks and SQLite restart. The
// controlled dataset supplies no investment-performance or model-quality claim.
test('maintenance preserves one monitor/gate epoch across restart and replays commands without creating new work', { timeout: 30_000 }, async () => {
  const state = await setup();
  try {
    state.options.config.datasets[0]!.data.request = { train_end_ns: '140', validation_end_ns: '180', forecast: {
      schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 }, parameters: {},
    } };
    await state.restart();
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { requestId: 'maintenance-monitor', goal: 'Preserve original budget-limited campaign', datasetId: 'controlled', model: state.model,
        funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, maxTrials: 1 } });
    assert.equal(response.statusCode, 202, response.body);
    const mandateId = response.json().mandate.id;
    const record = pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates[mandateId]!;
    const monitorTaskId = record.monitorTaskId!;
    const denied = await state.service.app.inject({ method: 'POST', url: '/api/v3/maintenance/enter', payload: { requestId: 'anonymous' } });
    assert.equal(denied.statusCode, 401);
    const entered = await post(state, 'enter', { requestId: 'upgrade', reason: 'Protocol upgrade rehearsal' });
    const parked = await until(() => read(state), value => value.state === 'QUIESCED');
    assert.deepEqual(parked.parkedTaskIds, [monitorTaskId]); assert.equal(parked.epoch, entered.receipt.epoch);
    const original = await state.service.research.harness.commit(tx => tx.task(monitorTaskId as TaskId), context);
    assert.ok(original);
    assert.equal(original!.state.status, 'waiting');
    assert.deepEqual(original!.state.status === 'waiting' ? original.state.on : [], [parked.barrierTaskId]);
    const admission = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: {} });
    assert.equal(admission.statusCode, 503); assert.equal(admission.json().error, 'service_maintenance');
    assert.deepEqual((await post(state, 'enter', { requestId: 'upgrade', reason: 'Protocol upgrade rehearsal' })).receipt, entered.receipt);
    await post(state, 'enter', { requestId: 'upgrade', reason: 'Changed operation' }, 409);
    await post(state, 'resume', { requestId: 'wrong', epoch: 'wrong-epoch' }, 409);
    await state.service.close();
    const config = join(state.directory, 'maintenance-config.json');
    await writeFile(config, JSON.stringify({ dataDir: state.directory, research: { database: state.options.config.research.database } }));
    const inspected = await promisify(execFile)(process.execPath, [new URL('../../../deploy/docker/maintenance.mjs', import.meta.url).pathname, config]);
    assert.match(inspected.stdout, /idle/);
    await state.restart();
    const restored = await until(() => read(state), value => value.state === 'QUIESCED');
    assert.equal(restored.epoch, parked.epoch); assert.equal(restored.barrierTaskId, parked.barrierTaskId); assert.deepEqual(restored.parkedTaskIds, parked.parkedTaskIds);
    assert.deepEqual((await state.service.research.harness.commit(tx => tx.task(monitorTaskId as TaskId), context))!.state, original!.state);
    const resumed = await post(state, 'resume', { requestId: 'continue', epoch: parked.epoch });
    assert.equal(resumed.status.state, 'RUNNING');
    await state.service.research.harness.waitForTask(parked.barrierTaskId as TaskId, context);
    await until(() => state.service.research.harness.commit(tx => tx.task(monitorTaskId as TaskId), context), task => task!.state.status !== 'waiting');
    assert.equal(pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates[mandateId]!.monitorTaskId, monitorTaskId);
    assert.deepEqual((await post(state, 'enter', { requestId: 'upgrade', reason: 'Protocol upgrade rehearsal' })).receipt, entered.receipt);
    assert.equal((await read(state)).state, 'RUNNING', 'old enter replay cannot re-enter maintenance');
    const second = await post(state, 'enter', { requestId: 'next-upgrade' });
    assert.notEqual(second.receipt.epoch, entered.receipt.epoch);
    assert.deepEqual((await post(state, 'resume', { requestId: 'continue', epoch: parked.epoch })).receipt, resumed.receipt);
    assert.equal((await read(state)).epoch, second.receipt.epoch, 'old resume replay cannot release the current gate');
  } finally { await state.cleanup(); }
});

test('maintenance drains real unknown background tasks and active admissions instead of treating background as idle', { timeout: 20_000 }, async () => {
  const state = await setup(); let finish!: () => void; const finished = new Promise<void>(resolve => { finish = resolve; });
  const task = defineTask<Record<string, never>, { phase: 'work' }, null>({ name: 'test.unknown-business', version: 1, initial: () => ({ phase: 'work' }),
    phases: { async work(_task, runtime, ctx) { await awaitWithContext(finished, ctx); await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx); } },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); } });
  try {
    state.service.research.registry.install(defineExtension({ name: 'test.unknown-business', tasks: [task] }));
    const id = await state.service.research.harness.commit(async tx => { const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      return tx.createTask(task, {}, { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true }); }, context);
    await state.service.maintenance.beginRequest('controlled-admission');
    await post(state, 'enter', { requestId: 'drain' });
    const draining = await read(state); assert.equal(draining.state, 'DRAINING'); assert.equal(draining.activeBusinessRequests, 1);
    assert.ok(draining.blockingTasks.some(item => item.taskId === id && item.kind === 'test.unknown-business'));
    finish(); await state.service.research.harness.waitForTask(id, context);
    assert.equal((await read(state)).state, 'DRAINING', 'admitted request still has authority to create finite work');
    state.service.maintenance.endRequest('controlled-admission');
    assert.equal((await until(() => read(state), value => value.state === 'QUIESCED')).state, 'QUIESCED');
  } finally { finish(); await state.cleanup(); }
});

test('an HTTP admission already past its guard keeps drain open and cannot create research after maintenance enters', { timeout: 20_000 }, async t => {
  const state = await setup(); let reached!: () => void; let release!: () => void;
  const admitted = new Promise<void>(resolve => { reached = resolve; }); const released = new Promise<void>(resolve => { release = resolve; });
  try {
    const begin = state.service.maintenance.beginRequest.bind(state.service.maintenance);
    // Delay only the scheduling boundary AFTER the real transactional guard;
    // original HTTP handler, workbench, storage and scheduler all still execute.
    t.mock.method(state.service.maintenance, 'beginRequest', async (id: string, reconcileOnly?: boolean) => { await begin(id, reconcileOnly); reached(); await released; });
    const pending = state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'already-admitted', goal: 'Real request racing service maintenance', model: state.model, datasetId: 'controlled', maxTrials: 1 } });
    await admitted;
    await post(state, 'enter', { requestId: 'during-admission' });
    const draining = await read(state); assert.equal(draining.state, 'DRAINING'); assert.equal(draining.activeBusinessRequests, 1);
    release(); const response = await pending;
    assert.equal(response.statusCode, 503, response.body); assert.equal(response.json().error, 'service_maintenance');
    assert.equal((await state.service.research.harness.snapshot(Application, context))!.researchRequests['already-admitted'], undefined);
    assert.equal((await until(() => read(state), value => value.state === 'QUIESCED')).activeBusinessRequests, 0);
  } finally { release(); await state.cleanup(); }
});

test('original uncertain external facts block quiescence across restart even when every business task is terminal', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    // Original external boundary fact: no remote POST is simulated or inferred.
    await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).accountControls = { account: { commands: { original: { state: 'UNCERTAIN' } } } }; }, context);
    await post(state, 'enter', { requestId: 'uncertain' });
    const blocked = await read(state); assert.equal(blocked.state, 'DRAINING');
    assert.deepEqual(blocked.blockingExternal, [{ kind: 'CAPITAL_COMMAND', id: 'account:original', reason: 'capital_command_unsettled' }]);
    await state.restart(); assert.deepEqual((await read(state)).blockingExternal, blocked.blockingExternal);
    await state.service.research.harness.commit(async tx => { (await tx.doc(Application)).accountControls!.account = { commands: { original: { state: 'ACKNOWLEDGED' } } }; }, context);
    const quiesced = await until(() => read(state), value => value.state === 'QUIESCED');
    assert.equal(quiesced.epoch, blocked.epoch);
  } finally { await state.cleanup(); }
});

test('maintenance never accepts a parked registry from another epoch or a missing original task', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    await post(state, 'enter', { requestId: 'corrupt-fixture' });
    await until(() => read(state), value => value.state === 'QUIESCED');
    await state.service.research.harness.commit(async tx => { const app = await tx.doc(Application); const stored = maintenanceState(app)!;
      app.maintenance = json({ ...stored, epoch: 'mismatched-document-epoch' }); }, context);
    const blocked = await read(state); assert.equal(blocked.state, 'DRAINING');
    assert.ok(blocked.blockingTasks.some(item => item.kind === 'qz.maintenance.gate'));
    await state.service.research.harness.commit(async tx => { const stored = maintenanceState(await tx.doc(Application))!;
      stored.parked['999999'] = { taskId: 999999, kind: 'qz.pm.mandate-monitor', epoch: stored.epoch, resumeCheckpoint: { phase: 'observe' } }; }, context);
    assert.ok((await read(state)).blockingTasks.some(item => item.taskId === 999999 && item.reason === 'registered_parked_task_not_verified'));
  } finally { await state.cleanup(); }
});
