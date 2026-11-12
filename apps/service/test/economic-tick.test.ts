import assert from 'node:assert/strict';
import { test } from 'node:test';
import { researchInput } from '../../research/src/input.ts';
import { context, setup } from './support.ts';

test('retired Wasm-factor economic replay routes cannot start or recreate a Pi task', async () => {
  const state = await setup();
  try {
    const before = await state.service.research.harness.commit(tx => tx.scanTasks({}, 100), context);
    for (const [method, url] of [
      ['GET', '/api/v3/tick-economic-replays'], ['GET', '/api/v3/tick-economic-replays/list'],
      ['POST', '/api/v3/tick-economic-replays'], ['GET', '/api/v3/tick-economic-replays/old-id'],
      ['POST', '/api/v3/tick-economic-replays/old-id/stop'],
    ] as const) {
      const response = await state.service.app.inject({ method, url, headers: state.headers,
        ...(method === 'POST' ? { payload: { requestId: 'retired', factorId: 'r1.t1.m0', datasetId: 'controlled' } } : {}) });
      assert.equal(response.statusCode, 404, response.body);
    }
    assert.deepEqual(await state.service.research.harness.commit(tx => tx.scanTasks({}, 100), context), before);
    assert.equal(state.faux.state.callCount, 0);
    assert.throws(() => researchInput({ goal: 'No retired factor policy', model: state.model, data: {}, policy: { tickEconomic: {} } }), /custom_tick_factor_research_retired/);
    const datasets = await state.service.app.inject({ url: '/api/v3/datasets', headers: state.headers });
    assert.equal(datasets.statusCode, 200); assert.equal(datasets.json()[0].id, 'controlled');
  } finally { await state.cleanup(); }
});
