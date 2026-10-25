import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import Fastify from 'fastify';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { bindScienceData, configDefaults } from '../src/config.ts';
import { boundScienceTarget } from '../src/science-binding.ts';
import { computePmDecision } from '../src/pm-native.ts';
import { pmAllocationMethod } from '../src/pm-method.ts';
import { digest, PmDocument, pmState } from '../src/pm-facts.ts';
import type { PmRecord } from '../src/pm-facts.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { setup, context } from './support.ts';

async function boundary(id: string) {
  const app = Fastify(); const requests: string[] = [];
  app.addHook('onRequest', async request => { requests.push(`${request.method} ${request.url}`); });
  app.get('/v1/capabilities', () => ({ schemaVersion: 1, implementationDigest: `sha256:${'1'.repeat(64)}`, commands: ['catalog-prefix-digest'] }));
  app.post('/v1/execute', request => ({ schemaVersion: 1, requestId: (request.body as { requestId: string }).requestId,
    status: 'FAILED', error: `${id}_transport_boundary` }));
  const url = await app.listen({ host: '127.0.0.1', port: 0 });
  return { app, requests, target: { id, url } satisfies RemoteNativeTarget };
}

// This rejects at the first native boundary; it supplies no successful
// scientific output, model result, market evidence or financial operation.
function frozenCycle(target?: RemoteNativeTarget) {
  const original: JsonObject = { catalog: '/registered-protocol-catalog', request: { train_end_ns: '100', validation_end_ns: '200', forecast: {
    selection: { event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {} } } };
  const data = target ? bindScienceData(original, target) : original;
  const request = iterationRequest(data, true);
  const cycle = { id: 'original-cycle', strategyVersionId: 'original-strategy', datasetRevisionIds: ['original-dataset'], runtime: { conversationId: 1, taskId: 2 },
    basisRevision: 'original-basis', asOfNs: '300', availableCutoffNs: '300', accountBasis: null, dependencyRevisionRefs: [] } as unknown as Pm.DecisionCycle;
  const record = { mandate: { id: 'original-mandate', allowedUniverseRef: 'original-universe', capitalSource: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, ownerBoundaries: [] },
    definition: { id: 'original-definition' }, policy: { id: 'original-policy', validDomain: { jointAllocationMethod: pmAllocationMethod() } }, gaps: {}, artifacts: {
      [cycle.id]: { maximumTrials: 2, research: { input: { data } }, configured: { data, revision: { id: 'original-dataset', availableThroughNs: '300' }, strategy: {} },
        nativeSource: { request, catalogPrefix: { method: 'NAUTILUS_SELECTED_PREFIX_SHA256_V1', content_sha256: 'a'.repeat(64), selection: (request.forecast as JsonObject).selection },
          fit: { members: [{ factor_version_id: 'protocol-only', weight: '1' }], forecast_contract: { parameters: {} } }, simulationRequest: { settings: { base_currency: 'USD' } } } } },
  } as unknown as PmRecord;
  return { record, cycle };
}

test('resumed PM computation uses its frozen remote instance and refuses legacy local work', { timeout: 20_000 }, async () => {
  const first = await boundary('original'); const second = await boundary('replacement');
  const root = await mkdtemp(join(tmpdir(), 'qz-pm-binding-')); const strict = process.env.QZ_REQUIRE_REMOTE_NAUTILUS;
  try {
    const saved = join(root, 'original.json'); await writeFile(saved, JSON.stringify(frozenCycle(first.target)));
    const recovered = JSON.parse(await readFile(saved, 'utf8')) as ReturnType<typeof frozenCycle>;
    const config = configDefaults({ dataDir: root, nautilus: { backtest: second.target } });
    process.env.QZ_REQUIRE_REMOTE_NAUTILUS = '1';
    await assert.rejects(computePmDecision(recovered.record, recovered.cycle, config, BACKGROUND_CONTEXT), /original_transport_boundary/);
    assert.deepEqual(first.requests, ['GET /v1/capabilities', 'POST /v1/execute']);
    assert.deepEqual(second.requests, [], 'new application configuration cannot redirect a recovered PM cycle');
    const legacy = frozenCycle();
    await assert.rejects(computePmDecision(legacy.record, legacy.cycle, config, BACKGROUND_CONTEXT), /legacy_local_nautilus_task_requires_migration/);
    assert.equal(first.requests.length, 2); assert.equal(second.requests.length, 0);
    assert.throws(() => boundScienceTarget(config, {}), /legacy_local_nautilus_task_requires_migration/);
  } finally {
    if (strict === undefined) delete process.env.QZ_REQUIRE_REMOTE_NAUTILUS; else process.env.QZ_REQUIRE_REMOTE_NAUTILUS = strict;
    await first.app.close(); await second.app.close(); await rm(root, { recursive: true, force: true });
  }
});

test('new PM mandates freeze distinct configured instances without rewriting a shared source revision', { timeout: 30_000 }, async () => {
  const state = await setup();
  try {
    const catalog = join(state.directory, 'source-catalog'); await mkdir(catalog);
    const bytes = Buffer.from('Transport admission fixture only; not a scientific parquet catalog.');
    await writeFile(join(catalog, 'catalog.parquet'), bytes);
    const data = { catalog: 'source-catalog', request: { train_end_ns: '140', validation_end_ns: '180', forecast: {
      schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200',
        bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 }, parameters: {} } } };
    const manifest = { schemaVersion: 1, datasetId: 'controlled', sourceRevision: 'original-source', availableAtNs: '250', revisionKind: 'APPEND', data,
      files: [{ path: 'source-catalog/catalog.parquet', bytes: String(bytes.length), sha256: createHash('sha256').update(bytes).digest('hex') }] };
    const path = join(state.directory, 'revision.json'); await writeFile(path, JSON.stringify({ ...manifest, contentDigest: digest(manifest) }));
    const dataset = state.options.config.datasets[0]!; dataset.revisionManifest = path;
    const targets = [{ id: 'first', url: 'http://127.0.0.1:1/first' }, { id: 'second', url: 'http://127.0.0.1:1/second' }];
    const ids: string[] = [];
    let originalDataset: unknown;
    for (const [index, target] of targets.entries()) {
      state.options.config.nautilus = { backtest: target }; dataset.data = bindScienceData(dataset.data, target); await state.restart();
      const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
        payload: { requestId: `configured-instance-${index}`, goal: 'Retain source facts and the newly admitted remote instance.', datasetId: dataset.id,
          model: state.model, funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, maxTrials: 1 } });
      assert.equal(response.statusCode, 202, response.body); ids.push(response.json<Pm.PmMandateView>().mandate.id);
      const stored = pmState(await state.service.research.harness.snapshot(PmDocument, context));
      const current = stored.mandates[ids[index]!]!;
      assert.deepEqual(current.artifacts['nautilus-binding'], { target });
      assert.equal(Object.keys(stored.datasets).length, 1, 'Changing transport does not create a fabricated source revision');
      const captured = stored.datasets[current.datasets[0]!]!;
      assert.equal(captured.data.nautilusTarget, undefined, 'Shared scientific source facts do not own a transport');
      if (index === 0) originalDataset = structuredClone(captured); else assert.deepEqual(captured, originalDataset);
      assert.deepEqual(stored.mandates[ids[0]!]!.artifacts['nautilus-binding'], { target: targets[0] });
    }
    const stored = pmState(await state.service.research.harness.snapshot(PmDocument, context));
    assert.deepEqual(stored.mandates[ids[0]!]!.datasets, stored.mandates[ids[1]!]!.datasets);
    for (const [index, id] of ids.entries()) {
      let current = stored.mandates[id]!;
      let campaign = Object.values(current.campaigns)[0]!;
      const deadline = Date.now() + 5_000;
      while (campaign.runtime.conversationId === current.conversationId && Date.now() < deadline) {
        await new Promise(resolve => setTimeout(resolve, 10));
        current = pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!;
        campaign = Object.values(current.campaigns)[0]!;
      }
      assert.notEqual(campaign.runtime.conversationId, current.conversationId, 'Campaign admitted a real Durable study');
      const study = (await state.service.research.state(campaign.runtime.conversationId)).research!;
      assert.deepEqual(study.input!.data.nautilusTarget, targets[index]);
      assert.deepEqual(study.trials, [], 'Admission supplies no fabricated scientific results');
    }
  } finally { await state.cleanup(); }
});
