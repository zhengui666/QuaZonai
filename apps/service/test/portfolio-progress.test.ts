import assert from 'node:assert/strict';
import { test } from 'node:test';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { TaskId } from '@earendil-works/pi-durable';
import { setup, studied, context } from './support.ts';

test('running portfolio publishes exact native preview and replaces it with full final curve', { timeout: 15_000 }, async () => {
  const state = await setup();
  try {
    const { receipt } = await studied(state);
    const release = join(state.directory, 'release'); const script = join(state.directory, 'live.mjs');
    const preview = { ts_event: '1790000000000000000', total_equity: ['1000.000000000000000001 USD'] };
    const final = { ts_event: '1790000001000000000', total_equity: ['1001.000000000000000001 USD'] };
    await writeFile(script, `import {writeFileSync, existsSync} from 'node:fs';
writeFileSync(process.env.QZ_PROGRESS_PATH, JSON.stringify({snapshot:${JSON.stringify(preview)}})+'\\n');
const timer=setInterval(()=>{if(existsSync(${JSON.stringify(release)})){clearInterval(timer); console.log(JSON.stringify({statistics:[],canonical_result:{portfolio_snapshots:[${JSON.stringify(preview)},${JSON.stringify(final)}]}}));}},20);`);
    const original = new URL('./controlled-job.mjs', import.meta.url).pathname;
    await writeFile(state.options.config.jobBinary, `#!/bin/sh\nif [ "$1" = simulate ]; then exec '${process.execPath}' '${script}' "$@"; else exec '${process.execPath}' '${original}' "$@"; fi\n`, { mode: 0o700 });
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers,
      payload: { requestId: 'live-preview', name: 'Live protocol', researchIds: [receipt.conversationId], datasetId: 'controlled', capital: '1000.000000000000000001' } });
    assert.equal(response.statusCode, 202, response.body); const admitted = response.json();
    assert.equal(admitted.progress.phase, 'QUEUED');
    let running;
    for (let attempt = 0; attempt < 150; attempt++) {
      running = (await state.service.work.getPortfolio(admitted.id)).view;
      if (running.curve.length) break;
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    assert.equal(running?.status, 'RUNNING'); assert.equal(running?.progress?.phase, 'SIMULATING');
    assert.deepEqual(running?.curve, [{ at: '2026-09-21T14:13:20.000000000Z', value: '1000.000000000000000001' }]);
    const list = await state.service.app.inject({ url: '/api/v3/portfolios', headers: state.headers });
    assert.equal(list.json().find((item: { id: string }) => item.id === admitted.id).status, 'RUNNING');
    await writeFile(release, ''); await state.service.research.harness.waitForTask(admitted.taskId as TaskId, context);
    const completed = (await state.service.work.getPortfolio(admitted.id)).view;
    assert.equal(completed.status, 'COMPUTED'); assert.equal(completed.progress?.phase, 'COMPLETED');
    assert.equal(completed.curve.length, 2); assert.equal(completed.curve[1]!.value, '1001.000000000000000001');
  } finally { await state.cleanup(); }
});
