import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ConversationId } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { built, context, setup, studied } from './support.ts';
import { Application } from '../src/facts.ts';
import { metrics } from '../src/projections.ts';

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

for (const endpoint of ['usage', 'research']) test(`${endpoint} summary does not decode historical native task results`, { timeout: 20_000 }, async () => {
  const state = await setup(); const harness = state.service.research.harness;
  const commit = harness.commit.bind(harness); let historicalReads = 0;
  try {
    await studied(state, `summary-without-history-${endpoint}`);
    const expected = await state.service.app.inject({ url: `/api/v3/${endpoint}`, headers: state.headers });
    assert.equal(expected.statusCode, 200, expected.body);
    harness.commit = (callback, ctx) => commit((tx) => callback(new Proxy(tx, {
      get(target, property) {
        if (property === 'scanTasks') return () => { historicalReads++; throw new Error('unexpected_historical_task_read'); };
        const value = Reflect.get(target, property, target);
        return typeof value === 'function' ? value.bind(target) : value;
      },
    })), ctx);
    const response = await state.service.app.inject({ url: `/api/v3/${endpoint}`, headers: state.headers });
    assert.equal(response.statusCode, 200, response.body);
    assert.deepEqual(response.json(), expected.json(), 'original summary fields and nullable values remain unchanged');
    assert.equal(historicalReads, 0);
  } finally { harness.commit = commit; await state.cleanup(); }
});
