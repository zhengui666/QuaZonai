import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ConversationId } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { context, setup, studied } from './support.ts';
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
