import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { configDefaults } from '../src/config.ts';
import { createService } from '../src/service.ts';
import { exportDataset } from '../src/dataset-export.ts';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import heldoutWasm from '../../research/plugins/heldout-wasm.ts';

const fixture = process.env.QZ_TICK_NATIVE_FIXTURE_DIR;
const standard = process.env.QZ_JOB_BINARY; const high = process.env.QZ_TICK_JOB_BINARY;

for (const representation of ['native', 'cache'] as const) test(`HTTP and Durable evaluate actual high-precision ticks (${representation}) with separate BAR kernel and owner-only final evidence`, { skip: !fixture || !standard || !high, timeout: 120_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-tick-native-http-'));
  const worker = await createNautilusWorker({ binary: standard!, tickBinary: high!, dataDir: join(directory, 'worker'), catalogRoots: [fixture!, directory] });
  try {
    await new Promise<void>(resolve => worker.server.listen(0, '127.0.0.1', resolve));
    const address = worker.server.address(); assert.ok(address && typeof address !== 'string');
    const dataset = await exportDataset({ prepared: join(fixture!, representation === 'cache' ? 'cohort-prepared/catalog-metadata.json' : 'prepared/catalog-metadata.json'), request: join(fixture!, representation === 'cache' ? 'cohort-request.json' : 'request.json'),
      output: join(directory, 'dataset.json'), id: 'native-ticks', name: 'Controlled high-precision ticks, not market evidence', ready: true });
    const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
    const config = configDefaults({ dataDir: join(directory, 'service'), research: { database: join(directory, 'research.sqlite') }, datasets: [dataset],
      nautilus: { backtest: { id: 'native-tick-test', url: `http://127.0.0.1:${address.port}` } } });
    const service = await createService({ config, models, extensions: [factorWasm, heldoutWasm] });
    try {
      const session = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'controlled-tick-native-only' } });
      assert.equal(session.statusCode, 200, session.body);
      const headers = { cookie: session.cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ') };
      const model = await readFile(join(fixture!, 'model.wat'), 'utf8');
      faux.setResponses([
        fauxAssistantMessage(fauxToolCall('factor_evaluate', { hypothesis: 'Controlled cohort-price factor; no profitability or PIT claim', model }), { stopReason: 'toolUse' }),
        async () => {
          const studies = await service.research.list(); const id = studies.items[0]!.id;
          const trial = (await service.research.view(id)).research!.trials[0]!;
          assert.equal(trial.status, 'COMPUTED', JSON.stringify(trial));
          return fauxAssistantMessage(fauxToolCall('factor_finalize', { trialId: Number(trial.taskId), reason: 'Fixed controlled selection before held-out calculation' }), { stopReason: 'toolUse' });
        },
      ]);
      const original = { requestId: 'actual-tick-native-http', goal: 'Controlled high-precision tick pipeline', model: { provider: faux.provider.id, modelId: faux.getModel().id }, datasetId: dataset.id, maxTrials: 2 };
      const start = await service.app.inject({ method: 'POST', url: '/api/v3/research', headers, payload: original });
      assert.equal(start.statusCode, 202, start.body); const receipt = start.json();
      await (await service.research.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
      const detail = await service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers });
      assert.equal(detail.statusCode, 200, detail.body); const result = detail.json();
      assert.equal(result.trialRecords.length, 1); assert.equal(result.trialRecords[0].status, 'COMPUTED', detail.body);
      assert.equal(result.final.status, 'COMPUTED', detail.body);
      assert.equal(result.final.inputCoverage.modality, 'TRADE_TICK'); assert.equal(result.final.inputCoverage.availability, 'EVENT_TIME_PROXY_UNVERIFIED');
      assert.equal(result.final.inputCoverage.trade_count, '32');
      assert.equal(result.final.inputCoverage.representation, representation === 'cache' ? 'EVENT_TIME_COHORT_CACHE_V1' : undefined);
      assert.equal(result.trialRecords[0].metrics.filter((metric: { key: string }) => metric.key.startsWith('HELDOUT.pearson_ic'))[0].value, null);
      assert.ok(result.final.metrics.some((metric: { key: string }) => metric.key === 'HELDOUT.pearson_ic'));
      const factors = await service.app.inject({ url: '/api/v3/factors', headers });
      assert.equal(factors.json()[0].scientificQualification, 'UNQUALIFIED_EXPLORATORY'); assert.equal(factors.json()[0].combinable, false);
      const replay = await service.app.inject({ method: 'POST', url: '/api/v3/research', headers, payload: original });
      assert.equal(replay.json().conversationId, receipt.conversationId);
    } finally { await service.close(); }
  } finally { await worker.close(); await rm(directory, { recursive: true, force: true }); }
});
