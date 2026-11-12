import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setup, context } from './support.ts';

test('removed scoring and fitted portfolio producers expose no routes or registered tasks', async () => {
  const state = await setup();
  try {
    const before = await state.service.research.harness.inspect(context);
    for (const url of ['/api/v3/review-policies', '/api/v3/review-allocations', '/api/v3/portfolios']) {
      const response = await state.service.app.inject({ method: 'POST', url, headers: state.headers,
        payload: { requestId: 'removed-producer', name: 'Not admitted' } });
      assert.equal(response.statusCode, 404, response.body);
    }
    for (const name of ['qz.portfolio', 'qz.review-allocation', 'qz.scientific-stage']) {
      assert.equal(state.service.research.registry.snapshot().task(name), undefined);
    }
    assert.equal(state.service.research.registry.snapshot().extension('qz.fitted-ensemble'), undefined);
    assert.deepEqual((await state.service.research.harness.inspect(context)).tasks.map(t => t.record.id), before.tasks.map(t => t.record.id));
    assert.equal(state.faux.state.callCount, 0);
  } finally { await state.cleanup(); }
});
