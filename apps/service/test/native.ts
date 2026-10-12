import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, mkdtemp, rm, cp, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import Fastify from 'fastify';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { configDefaults } from '../src/config.ts';
import { createService } from '../src/service.ts';
import { decimalUnits, json, nowNs, object } from '../src/facts.ts';
import { nativeCurve } from '../src/projections.ts';
import { context, settledObservation, wat } from './support.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';


async function expandedCatalog(directory: string, fixturePath: string, prepare: string) {
  const root = dirname(fixturePath); const source = join(directory, 'synthetic-source');
  await cp(join(root, 'acquired'), source, { recursive: true });
  const manifest = JSON.parse(await readFile(join(source, 'acquisition.json'), 'utf8'));
  const row = JSON.parse((await readFile(join(source, 'records.jsonl'), 'utf8')).split('\n')[0]);
  // Three real response batches give the native matching/account engine time to fill a selected target.
  const count = 599; const batchSize = 299;
  const rows = Array.from({ length: count }, (_, index) => ({ ...row, selection_time_seconds: index * 60, event_time_seconds: (index + 1) * 60,
    source_response: `raw/${String(Math.floor(index / batchSize)).padStart(4, '0')}.json`, source_row_index: index % batchSize,
    observed_at: `2024-01-02T00:00:0${Math.floor(index / batchSize) + 1}Z` }));
  const records = rows.map((row) => JSON.stringify(row)).join('\n') + '\n';
  manifest.selection.end_seconds = count * 60; manifest.created_at = '2024-01-02T00:00:04Z';
  const template = manifest.responses[0]; manifest.responses = []; manifest.raw_response_bytes = 0;
  const clock = (seconds: number) => new Date(seconds * 1000).toISOString().replace('.000Z', 'Z').replaceAll(':', '%3A');
  for (let start = 0; start < count; start += batchSize) {
    const batch = rows.slice(start, start + batchSize); const first = batch[0]!;
    const raw = JSON.stringify(batch.map((row) => [row.selection_time_seconds, Number(row.low), Number(row.high), Number(row.open), Number(row.close), Number(row.volume)]));
    const response = structuredClone(template); response.observation.retrieved_at = first.observed_at;
    response.observation.request_started_at = `2024-01-02T00:00:0${Math.floor(start / batchSize)}Z`;
    response.request.start_seconds = first.selection_time_seconds; response.request.end_seconds = batch.at(-1)!.event_time_seconds;
    response.request.url = `https://api.exchange.coinbase.com/products/BTC-USD/candles?start=${clock(response.request.start_seconds)}&end=${clock(response.request.end_seconds)}&granularity=60`;
    response.file = { path: first.source_response, size: Buffer.byteLength(raw) }; response.counts = { source_rows: batch.length, selected_rows: batch.length, outside_request_window: 0 };
    manifest.responses.push(response); manifest.raw_response_bytes += response.file.size; await writeFile(join(source, first.source_response), raw);
  }
  manifest.requests = manifest.responses.map((response: { request: unknown }) => response.request);
  manifest.records.size = Buffer.byteLength(records); manifest.record_count = count;
  await writeFile(join(source, 'records.jsonl'), records); await writeFile(join(source, 'acquisition.json'), JSON.stringify(manifest));
  const output = join(directory, 'native-import'); const env = new NodeExecutionEnv({ cwd: directory });
  try { const result = getOrThrow(await env.exec([prepare, 'ingest-candles', '--acquisition', join(source, 'acquisition.json'), '--instruments', join(root, 'instruments.json'), '--output', output], { timeout: 20, inheritEnv: false, env: { PATH: '/usr/bin:/bin' } }, context)); assert.equal(result.exitCode, 0, 'native synthetic import failed'); }
  finally { await env.cleanup(context); }
  return join(output, 'catalog');
}

/** Real job and native BAR catalog; controlled provider/downstream, fixture data, no PIT/quality/account-performance claim. */
test('two original studies → native target blending → one fee-affected shared account → original target receipt/account nulls', { timeout: 90_000 }, async () => {
  assert.ok(process.env.QZ_JOB_BINARY, 'QZ_JOB_BINARY must name the existing scientific job');
  assert.ok(process.env.QZ_BROWSER_FIXTURE, 'QZ_BROWSER_FIXTURE must name browser_catalog_fixture prepared.json');
  const fixture = JSON.parse(await readFile(process.env.QZ_BROWSER_FIXTURE, 'utf8'));
  const receiptNs = fixture.original_receipt_ns as string;
  // This late-arrival fixture explicitly has no historical PIT. The final window includes original receipt clocks,
  // rather than backdating availability. Empty iterative statistics stay unavailable, never qualified.
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-service-native-'));
  const catalog = await expandedCatalog(directory, process.env.QZ_BROWSER_FIXTURE, process.env.QZ_CATALOG_PREPARE_BINARY ?? join(dirname(process.env.QZ_JOB_BINARY), 'catalog-prepare'));
  const end = (BigInt(receiptNs) + 9_000_000_000n).toString();
  const selection = { ...fixture.selection.selection, maximum_rows: 599, event_end_ns: end, decision_cutoff_ns: end };
  const request = { schema_version: 1, forecast: { schema_version: 1, selection, parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: '1000000' } }, train_end_ns: '180000000000', validation_end_ns: '300000000000' };
  const data = { catalog, request };
  const remote = Fastify(); let posts = 0; let offered: Record<string, unknown> = {};
  const received = () => ({ ...offered, status: 'ACKNOWLEDGED', externalId: 'native-test-receipt', acknowledgedAt: '2026-09-30T00:00:00.000000001Z' });
  remote.post('/targets', async (req) => { posts++; offered = req.body as Record<string, unknown>; return received(); });
  remote.get('/targets/:id', received);
  remote.get('/account', async () => ({ clientId: 'native-test-client', accountId: 'native-test-account', sequence: '9007199254740997', nativeClockNs: nowNs(), currency: 'USD', equity: null, cash: null, positions: [] }));
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  const modelRef = (adapter_kind: string, upstream_class: string, parameters: Record<string, unknown>) => ({ schema_version: 1, adapter_kind, upstream_class, upstream_version: '0.63.0', parameters });
  const simulationSettings = json({ schema_version: 1, base_currency: 'USD', starting_capital: '1000', account_kind: 'MARGIN', leverage: '1', exposure_tolerance: '0.000001', snapshot_interval_ms: 60_000,
    fill_model: modelRef('NAUTILUS_DEFAULT_FILL', 'nautilus_execution::models::fill::DefaultFillModel', { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '7' }),
    fee_model: modelRef('NAUTILUS_MAKER_TAKER', 'nautilus_execution::models::fee::MakerTakerFeeModel', {}),
    latency_model: modelRef('NAUTILUS_STATIC_LATENCY', 'nautilus_execution::models::latency::StaticLatencyModel', { base_latency_ns: '0', insert_latency_ns: '1000000', update_latency_ns: '0', cancel_latency_ns: '0' }),
    fee_rates: [{ instrument_id: 'BTC-USD.COINBASE', maker: '0.004', taker: '0.006' }] });
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const config = configDefaults({ dataDir: directory, jobBinary: process.env.QZ_JOB_BINARY, research: { database: join(directory, 'research.sqlite'), modules: [resolve('../research/plugins/factor-wasm.ts'), resolve('../research/plugins/heldout-wasm.ts')] },
    datasets: [{ id: 'late-native-fixture', name: 'Native fixture; unverified', currency: 'USD', source: 'FIXTURE', assetClass: 'SPOT', start: null, end: null, ready: true,
      data: { ...data, sealed: data }, strategy: { name: 'test-fixed-threshold-before-selection', kind: 'factor-threshold', instrumentId: 'BTC-USD.COINBASE', threshold: 0, investedWeight: '0.25',
        targetTtlNs: (BigInt(nowNs()) - BigInt(receiptNs) + 120_000_000_000n).toString(), simulationSettings } }],
    connections: [{ id: 'test-paper', name: 'Controlled protocol only', mode: 'paper', url, clientId: 'native-test-client', accountId: 'native-test-account' }] });
  const service = await createService({ config, models });
  try {
    const owner = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'native-test-only' } });
    const headers = { cookie: owner.cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join('; ') };
    const receipts: { conversationId: number; submissionId: number }[] = []; const selections: { researchId: number; trialId: number }[] = [];
    for (const [index, model] of [wat, wat.replace('f64.const 1', 'f64.const -1')].entries()) {
      faux.setResponses([
        fauxAssistantMessage(fauxToolCall('factor_evaluate', { hypothesis: `Frozen constant ${index}, not an alpha qualification`, model }), { stopReason: 'toolUse' }),
        index === 0 ? fauxAssistantMessage('Selection fixed before the joint final assessment.') : async () => {
          const original = receipts.at(-1)!;
          const trial = (await service.research.view(original.conversationId)).research!.trials[0]!;
          return fauxAssistantMessage(fauxToolCall('factor_finalize_group', { reason: 'Both original selections made before final exposure',
            selections: [...selections, { researchId: original.conversationId, trialId: Number(trial.taskId) }] }), { stopReason: 'toolUse' });
        },
      ]);
      const start = await service.app.inject({ method: 'POST', url: '/api/v3/research', headers, payload: { requestId: `native-idea-${index}`, goal: 'Late-arrival native control, not historical PIT', model: { provider: faux.provider.id, modelId: faux.getModel().id }, datasetId: 'late-native-fixture', maxTrials: 1 } });
      assert.equal(start.statusCode, 202, start.body); const receipt = start.json(); receipts.push(receipt);
      await (await service.research.harness.submission(receipt.submissionId, context))!.wait(context);
      const study = (await service.research.view(receipt.conversationId)).research!;
      assert.equal(study.trials[0]!.status, 'COMPUTED', JSON.stringify(study.trials));
      selections.push({ researchId: receipt.conversationId, trialId: Number(study.trials[0]!.taskId) });
    }
    for (const receipt of receipts) {
      const study = (await service.research.view(receipt.conversationId)).research!;
      const failures = (await service.research.tasks()).records.items.flatMap((record) => record.state.status === 'terminal' && 'error' in record.state.outcome ? [record.state.outcome.error] : []);
      assert.ok(study.final, JSON.stringify(failures));
      assert.equal(study.final!.status, 'COMPUTED', JSON.stringify(study.final));
      assert.equal(object(object(study.final!.result).native_versions)['nautilus-model'], '0.63.0');
      const detail = (await service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers })).json();
      assert.deepEqual(detail.submission, { id: receipt.submissionId, status: 'done' });
    }
    assert.deepEqual((await service.research.harness.snapshot(Exposures, context))!.windows.map((window) => window.kind), ['ITERATION', 'ITERATION', 'FINAL']);
    const researchIds = receipts.map((receipt) => receipt.conversationId);
    const weights = researchIds.map((researchId, index) => ({ researchId, weight: index === 0 ? '0.25' : '0.75' }));
    const portfolioInput = { requestId: 'native-portfolio', name: 'One shared native account from two studies', researchIds, weights, datasetId: 'late-native-fixture', capital: '1000' };
    const invalid = await service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers, payload: { ...portfolioInput, weights: [weights[0], { ...weights[1], weight: '0.70' }] } });
    assert.equal(invalid.statusCode, 422); assert.equal(invalid.json().error, 'portfolio_weights_invalid');
    const build = await service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers, payload: portfolioInput }); assert.equal(build.statusCode, 202, build.body);
    await service.research.harness.waitForTask(build.json().taskId as TaskId, context);
    const fact = await service.work.getPortfolio(build.json().id); assert.equal(fact.view.status, 'COMPUTED', fact.view.error ?? '');
    assert.deepEqual(fact.input, portfolioInput); assert.deepEqual(fact.view.researchIds, researchIds); assert.deepEqual(fact.weights, ['0.25', '0.75']);
    const native = fact.native!; const simulation = object(native.simulation);
    const sources = native.sources as JsonObject[]; const rows = native.rows as JsonObject[][]; const combined = object(native.blend).target_points as JsonObject[];
    assert.deepEqual(sources.map((source) => source.researchId), researchIds);
    assert.deepEqual(sources.map((source) => source.trialId), selections.map((selection) => selection.trialId));
    assert.deepEqual(fact.evidence.map((source) => object(object(source.selected).proposal).model), [wat, wat.replace('f64.const 1', 'f64.const -1')]);
    assert.deepEqual(sources.map((source) => (object(source.forecasts).points as JsonObject[]).find((point) => point.forecast !== null)!.forecast), [1, -1]);
    assert.deepEqual(rows[0]!.map((target) => decimalUnits(String(object((target.targets as unknown[])[0]).weight))), [decimalUnits('0.25'), 0n]);
    assert.deepEqual(rows[0]!.map((target) => decimalUnits(String(target.cash_weight))), [decimalUnits('0.75'), decimalUnits('1')]);
    assert.ok(combined.length);
    for (const [index, target] of combined.entries()) {
      assert.equal(object((target.targets as unknown[])[0]).weight, '0.0625'); assert.equal(target.cash_weight, '0.9375');
      const expiry = rows[index]!.map((target) => BigInt(String(target.valid_until_ns))).reduce((minimum, expiry) => expiry < minimum ? expiry : minimum);
      assert.equal(target.valid_until_ns, expiry.toString());
    }
    assert.deepEqual(object(native.simulationRequest).target_points, combined);
    assert.ok(sources.every((source) => !('simulation' in source))); // There is only the combined shared account, never two equity curves.
    assert.equal(simulation.native_version, '0.63.0'); assert.deepEqual(fact.view.curve, nativeCurve(simulation, 'USD'));
    assert.ok(fact.view.curve.length); assert.equal(fact.view.targets[0]!.weight, '0.0625');
    assert.ok(BigInt(String(simulation.orders)) > 0n); assert.ok(BigInt(String(simulation.positions)) > 0n);
    assert.ok(fact.view.curve.some((point) => point.value !== null && decimalUnits(point.value) !== decimalUnits('1000')), 'blended target must affect the native account, including original fees');
    const observationInput = { requestId: 'native-observation', portfolioId: fact.view.id, mode: 'paper', connectionId: 'test-paper' };
    const observe = await service.app.inject({ method: 'POST', url: '/api/v3/observations', headers, payload: observationInput }); assert.equal(observe.statusCode, 202, observe.body);
    const observed = await settledObservation(service, observe.json().id); assert.equal(observed.receipt!.status, 'ACKNOWLEDGED', observed.error ?? ''); assert.equal(observed.equity, null); assert.equal(observed.cash, null);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/observations', headers, payload: observationInput })).json().id, observed.id);
    assert.deepEqual(offered.target, fact.target); assert.equal(posts, 1); assert.equal(faux.state.callCount, 4);
  } finally { await service.close(); await remote.close(); await rm(directory, { recursive: true, force: true }); }
});
