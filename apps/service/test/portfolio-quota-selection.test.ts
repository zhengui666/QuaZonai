import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { TaskId } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { object } from '../src/facts.ts';
import { context, setup, studied } from './support.ts';

// Controlled old qualified-study records exercise owner HTTP admission only;
// these protocol fixtures make no native/performance qualification claim.
test('portfolio member admission ignores only retired row quotas across the original selected results', async () => {
  const state = await setup();
  try {
    const first = await studied(state, 'first-quota'); const second = await studied(state, 'second-quota');
    const ids = [first.receipt.conversationId, second.receipt.conversationId];
    await state.service.research.harness.commit(async tx => {
      for (const [index, id] of ids.entries()) {
        const study = await tx.doc(Study, id);
        for (const result of [study.final!.result, study.trials[0]!.result]) {
          object(object(object(result).request).forecast).selection = { maximum_rows: index + 1 };
        }
      }
    }, context);
    const input = { requestId: 'different-retired-row-quotas', name: 'TEST_ONLY selected members', datasetId: 'controlled', capital: '1000',
      researchIds: ids, weights: ids.map(researchId => ({ researchId, weight: '0.5' })) };
    const accepted = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: input });
    assert.equal(accepted.statusCode, 202, accepted.body);
    await state.service.research.harness.waitForTask(accepted.json().taskId as TaskId, context);
    await state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, ids[1]);
      object(object(object(study.final!.result).request).forecast).selection = { maximum_rows: 2, bar_types: ['OTHER.SIM-1-MINUTE-LAST-EXTERNAL'] };
    }, context);
    const changed = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: { ...input, requestId: 'changed-scientific-market' } });
    assert.equal(changed.statusCode, 422); assert.equal(changed.json().error, 'strategy_selection_mismatch');
  } finally { await state.cleanup(); }
});
