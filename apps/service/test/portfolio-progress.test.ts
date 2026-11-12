import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Application, json } from '../src/facts.ts';
import { setup, built, context } from './support.ts';

test('retired portfolio execution cannot replace exact saved native curves or original receipts', async () => {
  const state = await setup();
  try {
    const { view, input } = await built(state);
    const saved = await state.service.work.getPortfolio(view.id);
    const curve = [
      { at: '2026-09-21T14:13:20.000000000Z', value: '1000.000000000000000001' },
      { at: '2026-09-21T14:13:21.000000000Z', value: '1001.000000000000000001' },
    ];
    await state.service.research.harness.commit(async tx => {
      (await tx.doc(Application)).portfolios[view.id] = json({ ...saved, view: { ...saved.view, curve } });
    }, context);
    const before = await state.service.work.snapshot();
    const tasks = (await state.service.research.tasks()).records;
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers,
      payload: { ...input, requestId: 'retired-preview' } });
    assert.equal(response.statusCode, 404, response.body);
    assert.equal(response.json().error, 'not_found');
    const read = await state.service.app.inject({ url: `/api/v3/portfolios/${view.id}`, headers: state.headers });
    assert.equal(read.statusCode, 200, read.body);
    assert.deepEqual(read.json().curve, curve); assert.equal(read.json().taskId, view.taskId);
    const list = await state.service.app.inject({ url: '/api/v3/portfolios', headers: state.headers });
    assert.deepEqual(list.json().find((item: { id: string }) => item.id === view.id).curve, curve);
    assert.deepEqual(await state.service.work.snapshot(), before);
    assert.deepEqual((await state.service.research.tasks()).records, tasks);
  } finally { await state.cleanup(); }
});
