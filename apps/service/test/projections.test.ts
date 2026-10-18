import assert from 'node:assert/strict';
import { test } from 'node:test';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, TaskQuery } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { awaitWithContext } from '@earendil-works/chord/context';
import { Study } from '../../research/src/documents.ts';
import { built, context, setup, studied } from './support.ts';
import { Application } from '../src/facts.ts';
import { metrics, researchTaskViews } from '../src/projections.ts';

test('list and usage do not load historical task results; list retains its existing summary fields', { timeout: 20_000 }, async t => {
  const storage = await openNodeSqliteStorage(':memory:'); const state = await setup(true, undefined, storage);
  try {
    const { receipt } = await studied(state);
    const get = (path: string) => state.service.app.inject({ url: `/api/v3/${path}`, headers: state.headers });
    const detail = (await get(`research/${receipt.conversationId}`)).json();
    const { input: _input, trialRecords: _trials, final: _final, tasks: _tasks, submission: _submission, previousStops: _stops, ...expected } = detail;
    const scan = storage.scanTasks.bind(storage); const scans: TaskQuery[] = [];
    t.mock.method(storage, 'scanTasks', (...args: Parameters<typeof scan>) => { scans.push(args[0]); return scan(...args); });
    const list = await get('research'); const usage = await get('usage');
    assert.equal(list.statusCode, 200); assert.deepEqual(list.json().items, [expected]);
    assert.equal(usage.statusCode, 200); assert.equal(usage.json().activeTasks, 0);
    assert.ok(scans.every(query => query.status && query.status !== 'terminal'), 'summary reads must not scan full historical task receipts');
  } finally { await state.cleanup(); }
});

test('detail hydrates task metadata once, shares concurrent reads and follows committed newest-100 updates', { timeout: 20_000 }, async t => {
  const storage = await openNodeSqliteStorage(':memory:'); const state = await setup(true, undefined, storage);
  try {
    const { receipt } = await studied(state); const host = state.service.research;
    const scan = storage.scanTasks.bind(storage); let scans = 0;
    t.mock.method(storage, 'scanTasks', (...args: Parameters<typeof scan>) => { if (args[0].conversationId === receipt.conversationId && !args[0].status) scans++; return scan(...args); });
    const get = () => state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    const [first, concurrent] = await Promise.all([get(), get()]);
    assert.equal(first.statusCode, 200); assert.deepEqual(first.json(), concurrent.json()); assert.equal(scans, 1);
    let finishFailure!: () => void; const failure = new Promise<void>(resolve => { finishFailure = resolve; });
    const task = defineTask<{ fail: boolean; result: string | null }, { phase: 'compute' }, string | null>({ name: 'test.metadata-only', version: 1, initial: () => ({ phase: 'compute' }),
      phases: { async compute(task, runtime, ctx) { if (task.input.fail) await awaitWithContext(failure, ctx); await runtime.commit(() => ({ status: 'terminal', outcome: task.input.fail ? { status: 'failed', error: { message: 'test_failure' } } : { status: 'completed', result: task.input.result } }), ctx); } },
      async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); } });
    host.registry.install(defineExtension({ name: 'test.task-metadata', tasks: [task] }));
    const ids = await host.harness.commit(async tx => {
      const ids = [];
      for (let i = 0; i < 101; i++) ids.push(await tx.createTask(task, { fail: false, result: i === 0 ? 'large protocol result '.repeat(100_000) : null }, { conversationId: receipt.conversationId as ConversationId, ownership: { kind: 'conversation' } }));
      return ids;
    }, context);
    await Promise.all(ids.map(id => host.harness.waitForTask(id, context)));
    const updated = (await get()).json(); assert.equal(updated.tasks.length, 100);
    assert.deepEqual(updated.tasks.map((task: { id: number }) => task.id), ids.slice(-100).reverse());
    assert.ok(updated.tasks.every((task: { outcome: string }) => task.outcome === 'completed'));
    const pending = await host.harness.commit(tx => tx.createTask(task, { fail: true, result: null }, { conversationId: receipt.conversationId as ConversationId, ownership: { kind: 'conversation' } }), context);
    const running = (await get()).json().tasks[0]; assert.equal(running.id, pending); assert.notEqual(running.status, 'terminal');
    const usage = await state.service.app.inject({ url: '/api/v3/usage', headers: state.headers }); assert.ok(usage.json().activeTasks > 0);
    finishFailure(); await host.harness.waitForTask(pending, context);
    const final = (await get()).json(); const failed = final.tasks[0]; assert.equal(failed.id, pending); assert.equal(failed.status, 'terminal'); assert.equal(failed.outcome, 'failed'); assert.equal(failed.error, 'task_failed');
    assert.equal(scans, 1, 'committed changes must update metadata without reloading evidence');
    const cold = await researchTaskViews(host)(receipt.conversationId); assert.deepEqual(cold, final.tasks); assert.equal(scans, 11, 'cold history is read in ten small pages, not one huge allocation');
    assert.equal((await storage.task(ids[0]!, context))!.state.outcome!.status, 'completed');
  } finally { await state.cleanup(); }
});

test('read projections retain partition/component availability and native scalar precision', () => {
  const result = metrics({ partitions_component: { index: 2 }, partitions: [{ partition: 'HELDOUT', metric: {
    pearson_ic: { value: null, status: 'CONSTANT_SCORE' }, counts: { included: '9007199254740995' },
  } }] });
  assert.deepEqual(result.find(item => item.key === 'HELDOUT.pearson_ic'), {
    key: 'HELDOUT.pearson_ic', label: 'component 2: HELDOUT.pearson_ic', value: null,
    status: 'CONSTANT_SCORE', partition: 'HELDOUT', component: 2,
  });
  assert.equal(result.find(item => item.key === 'HELDOUT.counts.included')!.value, '9007199254740995');
});

test('public research failures retain codes but do not expose native stderr', { timeout: 20_000 }, async () => {
  const state = await setup(); const diagnostic = 'Native failure at /private/test-only/catalog\n' + 'internal diagnostic '.repeat(30);
  try {
    const { receipt } = await studied(state, 'private-errors');
    await state.service.research.harness.commit(async tx => { const study = await tx.doc(Study, receipt.conversationId as ConversationId); study.final!.status = 'FAILED'; study.final!.error = diagnostic; study.trials[0]!.error = diagnostic; }, context);
    const response = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers }); const view = response.json();
    assert.equal(view.final.error, 'scientific_execution_failed'); assert.equal(view.trialRecords[0].error, 'scientific_execution_failed'); assert.ok(!response.body.includes('/private/test-only'));
    const stored = await state.service.research.harness.snapshot(Study, receipt.conversationId as ConversationId, context); assert.equal(stored!.final!.error, diagnostic); assert.equal(stored!.trials[0]!.error, diagnostic);
  } finally { await state.cleanup(); }
});

test('portfolio reads and original admission replay share public failure projection', { timeout: 20_000 }, async () => {
  const state = await setup(); const diagnostic = 'Native backtest failed at /private/test-only/catalog';
  try {
    const { input, view } = await built(state);
    await state.service.research.harness.commit(async tx => { const facts = await tx.doc(Application); const fact = facts.portfolios[view.id]!; (fact.view as Record<string, unknown>).error = diagnostic; }, context);
    const get = await state.service.app.inject({ url: `/api/v3/portfolios/${view.id}`, headers: state.headers }); assert.equal(get.json().error, 'backtest_failed'); assert.ok(!get.body.includes('/private/test-only'));
    const replay = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: input }); assert.equal(replay.statusCode, 202); assert.equal(replay.json().id, view.id); assert.equal(replay.json().error, 'backtest_failed'); assert.equal((await state.service.work.getPortfolio(view.id)).view.error, diagnostic);
  } finally { await state.cleanup(); }
});

test('captured dataset identity survives registry changes and identical inputs in different studies', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const first = await studied(state, 'original-display-facts');
    state.options.config.datasets[0]!.name = 'Changed current registry';
    state.options.config.datasets[0]!.source = 'Changed current source';
    await state.restart();
    const second = await studied(state, 'second-display-facts');
    const get = async (id: number) => (await state.service.app.inject({ url: `/api/v3/research/${id}`, headers: state.headers })).json();
    assert.equal((await get(first.receipt.conversationId)).dataset.name, 'Protocol dataset');
    assert.equal((await get(second.receipt.conversationId)).dataset.name, 'Changed current registry');
    await state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, first.receipt.conversationId as ConversationId);
      study.final!.reason = 'Original selection rationale'; study.final!.error = 'independent_test_failure'; study.final!.status = 'FAILED';
    }, context);
    const detail = await get(first.receipt.conversationId);
    assert.equal(detail.final.reason, 'Original selection rationale');
    assert.equal(detail.final.error, 'independent_test_failure');
    assert.equal(detail.createdAt, null);
    assert.equal(detail.datasetId, 'controlled');
    assert.ok(detail.tasks.some((task: { status: string; outcome?: string }) => task.status === 'terminal' && task.outcome === 'completed'));
    const extensions = (await state.service.app.inject({ url: '/api/v3/extensions', headers: state.headers })).json();
    assert.equal(extensions.find((item: { name: string }) => item.name === 'qz.science').required, true);
  } finally { await state.cleanup(); }
});
