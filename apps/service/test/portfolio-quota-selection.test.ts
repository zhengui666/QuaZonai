import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Study } from '../../research/src/documents.ts';
import { object } from '../src/facts.ts';
import { context, setup, studied } from './support.ts';

// Historical study records remain readable; no retired member-selection execution is supported.
test('retired portfolio admission preserves original scientific selections without creating work', async () => {
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
    const before = await state.service.work.snapshot();
    const tasks = (await state.service.research.tasks()).records;
    const originals = await Promise.all(ids.map(id => state.service.research.harness.snapshot(Study, id, context)));
    assert.deepEqual(originals.map(study => object(object(object(study!.final!.result).request).forecast).selection), [{ maximum_rows: 1 }, { maximum_rows: 2 }]);
    const accepted = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: input });
    assert.equal(accepted.statusCode, 404, accepted.body);
    assert.equal(accepted.json().error, 'not_found');
    assert.deepEqual(await Promise.all(ids.map(id => state.service.research.harness.snapshot(Study, id, context))), originals);
    await state.service.research.harness.commit(async tx => {
      const study = await tx.doc(Study, ids[1]);
      object(object(object(study.final!.result).request).forecast).selection = { maximum_rows: 2, bar_types: ['OTHER.SIM-1-MINUTE-LAST-EXTERNAL'] };
    }, context);
    const changed = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: { ...input, requestId: 'changed-scientific-market' } });
    assert.equal(changed.statusCode, 404); assert.equal(changed.json().error, 'not_found');
    const retained = await state.service.research.harness.snapshot(Study, ids[1], context);
    assert.deepEqual(object(object(object(retained!.final!.result).request).forecast).selection, { maximum_rows: 2, bar_types: ['OTHER.SIM-1-MINUTE-LAST-EXTERNAL'] });
    assert.deepEqual(await state.service.work.snapshot(), before);
    assert.deepEqual((await state.service.research.tasks()).records, tasks);
  } finally { await state.cleanup(); }
});
