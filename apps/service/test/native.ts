import assert from 'node:assert/strict';
import './capital-native.ts';
import './pm-science-native.ts';
import './pm-independent-policy.test.ts';
import './pm-account-native.test.ts';
import './pm-account-http.test.ts';
import './pm-forward.native.test.ts';
import './pm-forward-qualification.test.ts';
import './pm-forward-feasibility.test.ts';
import './pm-forward-http.test.ts';
import './pm-paper-receiver.test.ts';
import './pm-campaign-account.test.ts';
import './pm-source-recovery.test.ts';
import './pm-outcome-receipts.test.ts';
import './pm-operations.test.ts';
import './pm-business-acceptance.test.ts';
import { test } from 'node:test';
import { readFile, mkdtemp, mkdir, rm, cp, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import Fastify from 'fastify';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import type { JsonObject, SettledTask, TaskId } from '@earendil-works/pi-durable';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { configDefaults } from '../src/config.ts';
import { createService } from '../src/service.ts';
import { decimalUnits, json, nowNs, object } from '../src/facts.ts';
import { nativeCurve } from '../src/projections.ts';
import { context, settledObservation, wat } from './support.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { CatalogScopes } from '../../research/plugins/native-catalog-scope.ts';
import { openResearch } from '../../research/src/host.ts';
import { factorViews } from '../src/factors.ts';


async function expandedCatalog(directory: string, fixturePath: string, prepare: string, causal = false, count = 599, symbol = 'BTC-USD', interleaved = false) {
  const root = dirname(fixturePath); const source = join(directory, 'synthetic-source');
  await cp(join(root, 'acquired'), source, { recursive: true });
  const manifest = JSON.parse(await readFile(join(source, 'acquisition.json'), 'utf8'));
  const row = JSON.parse((await readFile(join(source, 'records.jsonl'), 'utf8')).split('\n')[0]);
  let instrumentsPath = join(root, 'instruments.json');
  if (symbol !== 'BTC-USD') {
    const instruments = JSON.parse(await readFile(instrumentsPath, 'utf8'));
    Object.assign(instruments[0].CurrencyPair, { id: `${symbol}.COINBASE`, raw_symbol: symbol, base_currency: symbol.split('-')[0] });
    instrumentsPath = join(directory, 'instruments.json');
    await writeFile(instrumentsPath, JSON.stringify(instruments));
  }
  manifest.selection.instrument = symbol;
  // Synthetic receipt batches give the native matching/account engine time to fill a selected target.
  // Each partition also needs later receipt waves to settle initial fills and fee-driven rebalancing.
  const batchSize = interleaved ? 1 : causal ? 50 : 299;
  const receiptLag = interleaved && symbol === 'ETH-USD' ? 2 : 1;
  const rows = Array.from({ length: count }, (_, index) => ({ ...row, instrument: symbol, selection_time_seconds: index * 60, event_time_seconds: (index + 1) * 60,
    source_response: `raw/${String(Math.floor(index / batchSize)).padStart(4, '0')}.json`, source_row_index: index % batchSize,
    observed_at: causal ? new Date((Math.min((Math.floor(index / batchSize) + 1) * batchSize, count) * 60 + receiptLag) * 1000).toISOString() : `2024-01-02T00:00:0${Math.floor(index / batchSize) + 1}Z` }));
  const records = rows.map((row) => JSON.stringify(row)).join('\n') + '\n';
  manifest.selection.end_seconds = count * 60; manifest.created_at = causal ? new Date((count * 60 + 2) * 1000).toISOString() : '2024-01-02T00:00:04Z';
  const template = manifest.responses[0]; manifest.responses = []; manifest.raw_response_bytes = 0;
  const clock = (seconds: number) => new Date(seconds * 1000).toISOString().replace('.000Z', 'Z').replaceAll(':', '%3A');
  for (let start = 0; start < count; start += batchSize) {
    const batch = rows.slice(start, start + batchSize); const first = batch[0]!;
    const raw = JSON.stringify(batch.map((row) => [row.selection_time_seconds, Number(row.low), Number(row.high), Number(row.open), Number(row.close), Number(row.volume)]));
    const response = structuredClone(template); response.observation.retrieved_at = first.observed_at;
    response.observation.request_started_at = causal ? new Date(batch.at(-1)!.event_time_seconds * 1000).toISOString() : `2024-01-02T00:00:0${Math.floor(start / batchSize)}Z`;
    response.request.start_seconds = first.selection_time_seconds; response.request.end_seconds = batch.at(-1)!.event_time_seconds;
    response.request.url = `https://api.exchange.coinbase.com/products/${symbol}/candles?start=${clock(response.request.start_seconds)}&end=${clock(response.request.end_seconds)}&granularity=60`;
    response.file = { path: first.source_response, size: Buffer.byteLength(raw) }; response.counts = { source_rows: batch.length, selected_rows: batch.length, outside_request_window: 0 };
    manifest.responses.push(response); manifest.raw_response_bytes += response.file.size; await writeFile(join(source, first.source_response), raw);
  }
  manifest.requests = manifest.responses.map((response: { request: unknown }) => response.request);
  manifest.records.size = Buffer.byteLength(records); manifest.record_count = count;
  async function ingest(source: string, output: string) {
    const env = new NodeExecutionEnv({ cwd: directory });
    let stderr = '';
    try {
      const result = getOrThrow(await env.exec([prepare, 'ingest-candles', '--acquisition', join(source, 'acquisition.json'), '--instruments', instrumentsPath, '--output', output], {
        timeout: 20, inheritEnv: false, env: { PATH: '/usr/bin:/bin', TMPDIR: process.env.PI_SCRATCH_DIR ?? tmpdir() },
        onOutput: (chunk, _context, info) => { if (info.stream === 'stderr') stderr += chunk; },
      }, context));
      assert.equal(result.exitCode, 0, stderr || 'native synthetic import failed');
    }
    finally { await env.cleanup(context); }
    return join(output, 'catalog');
  }
  if (!causal) {
    await writeFile(join(source, 'records.jsonl'), records); await writeFile(join(source, 'acquisition.json'), JSON.stringify(manifest));
    return ingest(source, join(directory, 'native-import'));
  }
  // The importer requires 299-interval pagination and retrieval after the whole acquisition window.
  // Each causal batch is therefore its own short acquisition, not an invalid mid-window REST receipt.
  const catalog = join(directory, 'catalog');
  for (const [index, original] of manifest.responses.entries()) {
    const batchSource = join(directory, `acquisition-${index}`); await mkdir(join(batchSource, 'raw'), { recursive: true });
    await cp(join(source, 'source-terms.bin'), join(batchSource, 'source-terms.bin'));
    const response = structuredClone(original);
    const raw = await readFile(join(source, response.file.path)); response.file.path = 'raw/0000.json';
    const batchRecords = rows.slice(index * batchSize, (index + 1) * batchSize).map((row) => JSON.stringify({ ...row, source_response: response.file.path })).join('\n') + '\n';
    const batchManifest = { ...manifest, selection: { ...manifest.selection, start_seconds: response.request.start_seconds, end_seconds: response.request.end_seconds },
      created_at: response.observation.retrieved_at, requests: [response.request], responses: [response],
      records: { ...manifest.records, size: Buffer.byteLength(batchRecords) }, record_count: response.counts.selected_rows, raw_response_bytes: raw.length };
    await writeFile(join(batchSource, response.file.path), raw); await writeFile(join(batchSource, 'records.jsonl'), batchRecords);
    await writeFile(join(batchSource, 'acquisition.json'), JSON.stringify(batchManifest));
    // Union only native-validated, nonoverlapping BAR files and the same original instrument definition.
    await cp(await ingest(batchSource, join(directory, `native-import-${index}`)), catalog, { recursive: true });
  }
  return catalog;
}

function nativeTestSettings(capital: string) {
  const modelRef = (adapter_kind: string, upstream_class: string, parameters: Record<string, unknown>) => ({ schema_version: 1, adapter_kind, upstream_class, upstream_version: '0.63.0', parameters });
  return json({ schema_version: 1, base_currency: 'USD', starting_capital: capital, account_kind: 'MARGIN', leverage: '1', exposure_tolerance: '0.000001', snapshot_interval_ms: 60_000,
    fill_model: modelRef('NAUTILUS_DEFAULT_FILL', 'nautilus_execution::models::fill::DefaultFillModel', { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '7' }),
    fee_model: modelRef('NAUTILUS_MAKER_TAKER', 'nautilus_execution::models::fee::MakerTakerFeeModel', {}),
    latency_model: modelRef('NAUTILUS_STATIC_LATENCY', 'nautilus_execution::models::latency::StaticLatencyModel', { base_latency_ns: '0', insert_latency_ns: '1000000', update_latency_ns: '0', cancel_latency_ns: '0' }),
    fee_rates: [{ instrument_id: 'BTC-USD.COINBASE', maker: '0.004', taker: '0.006' }] });
}

for (const grouped of [false, true]) test(`full-catalog factor input reaches the real native kernel and ${grouped ? 'group' : 'single'} final without claiming an account`, { timeout: 60_000 }, async () => {
  const fixture = JSON.parse(await readFile(process.env.QZ_BROWSER_FIXTURE!, 'utf8'));
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-catalog-native-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const request = { schema_version: 1, event_start_ns: '0', train_end_ns: '120000000000', validation_end_ns: '180000000000',
    event_end_ns: (BigInt(fixture.original_receipt_ns) + 1_000_000_000n).toString(), decision_cutoff_ns: (BigInt(fixture.original_receipt_ns) + 1_000_000_000n).toString(), maximum_rows: 3,
    parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: null } };
  const data = { catalog: fixture.catalog_root, catalog_scan_request: request };
  const hold = defineExtension({ name: 'test.native-selection-hold', tools: [defineTool({
    name: 'hold_original_selection', description: 'End this test invocation, leaving the original selection available for joint final.', parameters: Type.Object({}),
    async execute() { return { content: [{ type: 'text', text: 'Original selection held; no final result read.' }], control: { terminate: true } }; },
  })] });
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [hold], modules: ['research', 'catalog-input', 'factor-wasm', 'heldout-wasm'].map(name => resolve('../research/plugins', `${name}.ts`)) };
  const host = await openResearch(options);
  try {
    let id: number;
    let peer: { researchId: number; trialId: number } | undefined;
    if (grouped) {
      faux.setResponses([
        fauxAssistantMessage(fauxToolCall('factor_evaluate', { hypothesis: 'First frozen kernel control', model: wat }), { stopReason: 'toolUse' }),
        fauxAssistantMessage(fauxToolCall('hold_original_selection', {}), { stopReason: 'toolUse' }),
      ]);
      const receipt = await host.start('catalog-kernel-peer', { goal: 'Bounded first selection for native group control', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { ...data, sealed: data }, policy: { max_trials: 1 } });
      await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      const study = (await host.state(receipt.conversationId)).research!;
      assert.equal(study.trials[0]!.status, 'COMPUTED');
      assert.equal(study.final, null, JSON.stringify(study.final));
      peer = { researchId: receipt.conversationId, trialId: Number(study.trials[0]!.taskId) };
    }
    faux.setResponses([
      fauxAssistantMessage(fauxToolCall('catalog_input', {}), { stopReason: 'toolUse' }),
      fauxAssistantMessage(fauxToolCall('factor_evaluate', { hypothesis: 'Constant kernel/control only, not alpha', model: wat }), { stopReason: 'toolUse' }),
      async () => {
        const trial = (await host.state(id)).research!.trials[0]!;
        assert.equal(trial.status, 'COMPUTED', JSON.stringify(trial));
        const result = object(trial.result);
        assert.equal(object(result.coverage).catalog_series, '1');
        assert.equal(object(result.request).event_end_ns, request.validation_end_ns);
        const heldout = object((result.partitions as JsonObject[])[2]);
        assert.equal(object(object(heldout.metric).counts).observations, '0');
        return fauxAssistantMessage(fauxToolCall(peer ? 'factor_finalize_group' : 'factor_finalize', {
          ...(peer ? { selections: [peer, { researchId: id, trialId: trial.taskId }] } : { trialId: trial.taskId }),
          reason: 'Freeze before final; original late clocks remain unqualified.',
        }), { stopReason: 'toolUse' });
      },
    ]);
    const original = { goal: 'Full-catalog native control; no market/PIT/model-quality claim', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { ...data, sealed: data }, policy: { max_trials: 1 } };
    const receipt = await host.start('catalog-kernel-control', original); id = receipt.conversationId;
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await host.state(id)).research!;
    assert.deepEqual(study.input, original); assert.equal(study.trials.length, 1);
    assert.ok(study.final, JSON.stringify((await host.view(id)).entries));
    assert.equal(study.final.status, 'COMPUTED', JSON.stringify(study.final));
    const result = object(study.final.result);
    assert.equal(object(result.native_versions)['nautilus-model'], '0.63.0');
    assert.equal(object(result.coverage).observations, '3');
    assert.equal(object(result.request).event_end_ns, request.event_end_ns);
    assert.equal('points' in result || 'portfolio' in result || 'simulation' in result, false);
    for (const partition of result.partitions as JsonObject[]) {
      assert.deepEqual(partition.assets, []);
      assert.equal(object(object(partition.metric).pearson_ic).value, null, 'unavailable evidence is not zero or success');
    }
    const scopes = (await host.harness.snapshot(CatalogScopes, context))!;
    assert.equal(scopes.roots.length, 1);
    const windows = (await host.harness.snapshot(Exposures, context))!.windows;
    assert.deepEqual(windows.map(window => window.kind), grouped ? ['ITERATION', 'ITERATION', 'FINAL'] : ['ITERATION', 'FINAL']);
    assert.deepEqual(windows[0]!.barTypesRef, windows[1]!.barTypesRef);
    assert.equal(factorViews(id, study)[0]!.combinable, false);
    assert.deepEqual(await host.start('catalog-kernel-control', original), receipt);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

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
  remote.get('/capabilities', () => ({ clientId: 'native-test-client', accountId: 'native-test-account', mode: 'paper', currency: 'USD', capitalExit: { version: 1, supported: false, readable: true } }));
  remote.get('/capital-exits', () => ({ clientId: 'native-test-client', accountId: 'native-test-account', mode: 'paper', currency: 'USD', accountSourceId: '01900000-0000-7000-8000-000000000001', observationId: '01900000-0000-7000-8000-000000000002', controlRevision: '1', reserved: '0', exits: [] }));
  remote.get('/targets/:id', received);
  remote.get('/account', async () => ({ clientId: 'native-test-client', accountId: 'native-test-account', sequence: '9007199254740997', nativeClockNs: nowNs(), currency: 'USD', equity: null, cash: null, positions: [] }));
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  const simulationSettings = nativeTestSettings('1000');
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
      assert.deepEqual(detail.submission, { id: receipt.submissionId, status: 'done', requestId: `native-idea-${receipts.indexOf(receipt)}` });
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
    const factors = (await service.app.inject({ url: '/api/v3/factors', headers })).json();
    const atomic = researchIds.map((researchId, index) => ({ factorId: factors.find((factor: { researchId: number; combinable: boolean }) => factor.researchId === researchId && factor.combinable).id, weight: index === 0 ? '0.25' : '0.75' }));
    // This late-publication control has 1970 BAR events received in 2024: never relabel those observations fresh.
    const capacityPolicy = { scenarios: ['1000', '10000'], participation: '0.01', maximumAgeSeconds: 3600 };
    const original = (await service.app.inject({ url: `/api/v3/research/${researchIds[0]}`, headers })).json();
    const missing = (await service.app.inject({ url: `/api/v3/factors/${atomic[0]!.factorId}/capacity`, headers })).json(); assert.equal(missing.reason, 'explicit_capacity_policy_missing');
    const assessed = await service.app.inject({ method: 'POST', url: `/api/v3/factors/${atomic[0]!.factorId}/capacity`, headers, payload: capacityPolicy }); assert.equal(assessed.statusCode, 200, assessed.body); assert.equal(assessed.json().status, 'UNAVAILABLE', assessed.body); assert.equal(assessed.json().reason, 'liquidity_stale'); assert.equal(assessed.json().assumptions.participation, capacityPolicy.participation); assert.deepEqual(assessed.json().scenarios.map((row: { capital: string }) => row.capital), capacityPolicy.scenarios); assert.ok(assessed.json().scenarios.every((row: { status: string; usableCapital: string | null }) => row.status === 'UNAVAILABLE' && row.usableCapital === null));
    assert.deepEqual((await service.app.inject({ url: `/api/v3/research/${researchIds[0]}`, headers })).json(), original, 'capacity assessment cannot rewrite admission, trials, independent evidence or original identities');
    const atomicBuild = await service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers, payload: { requestId: 'native-atomic-portfolio', name: 'Same original native pool via atomic references', factors: atomic, datasetId: 'late-native-fixture', capital: '1000', capacityPolicy } });
    assert.equal(atomicBuild.statusCode, 202, atomicBuild.body); await service.research.harness.waitForTask(atomicBuild.json().taskId as TaskId, context);
    const atomicFact = await service.work.getPortfolio(atomicBuild.json().id); assert.equal(atomicFact.view.status, 'COMPUTED', atomicFact.view.error ?? '');
    assert.equal(atomicFact.view.capacity?.status, 'UNAVAILABLE'); assert.equal(atomicFact.view.capacity?.reason, 'liquidity_stale'); assert.equal(atomicFact.view.capacity?.assumptions.participation, capacityPolicy.participation);
    assert.equal(missing.requestedAssumptions, undefined);
    assert.deepEqual(assessed.json().requestedAssumptions, capacityPolicy);
    assert.deepEqual(atomicFact.view.capacity?.requestedAssumptions, capacityPolicy);
    assert.deepEqual(atomicFact.view.factors, atomic); assert.deepEqual(atomicFact.view.curve, fact.view.curve); assert.deepEqual(object(atomicFact.native!.blend).target_points, combined);
    assert.deepEqual(atomicFact.evidence.map(source => source.factorId), atomic.map(row => row.factorId));
    const observationInput = { requestId: 'native-observation', portfolioId: fact.view.id, mode: 'paper', connectionId: 'test-paper' };
    const observe = await service.app.inject({ method: 'POST', url: '/api/v3/observations', headers, payload: observationInput }); assert.equal(observe.statusCode, 202, observe.body);
    const observed = await settledObservation(service, observe.json().id); assert.equal(observed.receipt!.status, 'ACKNOWLEDGED', observed.error ?? ''); assert.equal(observed.equity, null); assert.equal(observed.cash, null);
    assert.equal((await service.app.inject({ method: 'POST', url: '/api/v3/observations', headers, payload: observationInput })).json().id, observed.id);
    assert.deepEqual(offered.target, fact.target); assert.equal(posts, 1); assert.equal(faux.state.callCount, 4);
  } finally { await service.close(); await remote.close(); await rm(directory, { recursive: true, force: true }); }
});

for (const variant of ['legacy', 'fixed-pool', 'historical', 'cross-market', 'single-member'] as const) test(`${variant} ensemble research trades validation then owner-only final in one native account and replays the original selection`, { timeout: 120_000 }, async () => {
  const crossMarket = variant === 'cross-market';
  const single = variant === 'single-member'; const universe = variant !== 'legacy' && !single; const historical = variant === 'historical' || crossMarket;
  const prefixCount = crossMarket ? 24 : 400; const finalCount = crossMarket ? 40 : 599;
  const allocations = [{ instrumentId: 'ETH-USD.COINBASE', weight: '0.4' }, { instrumentId: 'BTC-USD.COINBASE', weight: '0.6' }];
  const selectedBars = crossMarket ? ['ETH-USD.COINBASE-1-MINUTE-LAST-EXTERNAL', 'BTC-USD.COINBASE-1-MINUTE-LAST-EXTERNAL'] : ['BTC-USD.COINBASE-1-MINUTE-LAST-EXTERNAL'];
  assert.ok(process.env.QZ_JOB_BINARY, 'QZ_JOB_BINARY must include forecast-factor');
  assert.ok(process.env.QZ_BROWSER_FIXTURE, 'QZ_BROWSER_FIXTURE must name the matching prepared.json');
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-native-ensemble-'));
  let service: Awaited<ReturnType<typeof createService>> | undefined;
  try {
    const prefixDir = join(directory, 'prefix'); const sealedDir = join(directory, 'sealed');
    await mkdir(prefixDir); await mkdir(sealedDir);
    const prepare = process.env.QZ_CATALOG_PREPARE_BINARY ?? join(dirname(process.env.QZ_JOB_BINARY), 'catalog-prepare');
    const catalog = await expandedCatalog(prefixDir, process.env.QZ_BROWSER_FIXTURE, prepare, true, prefixCount, 'BTC-USD', crossMarket);
    const sealed = await expandedCatalog(sealedDir, process.env.QZ_BROWSER_FIXTURE, prepare, true, finalCount, 'BTC-USD', crossMarket);
    if (universe) {
      for (const [root, destination, count] of [[prefixDir, catalog, prefixCount], [sealedDir, sealed, finalCount]] as const) {
        const other = join(root, 'other-instrument'); await mkdir(other);
        await cp(await expandedCatalog(other, process.env.QZ_BROWSER_FIXTURE, prepare, true, count, 'ETH-USD', crossMarket), destination, { recursive: true });
      }
    }
    const fixture = JSON.parse(await readFile(process.env.QZ_BROWSER_FIXTURE, 'utf8'));
    const request = { schema_version: 1, forecast: { schema_version: 1,
      selection: { ...fixture.selection.selection, ...(universe ? { bar_types: ['ETH-USD.COINBASE-1-MINUTE-LAST-EXTERNAL', ...fixture.selection.selection.bar_types] } : {}), maximum_rows: universe ? 1198 : 599, event_end_ns: '24000000000000', decision_cutoff_ns: '24000000000000' },
      parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1 } },
      train_end_ns: '12000000000000', validation_end_ns: '24000000000000' };
    if (crossMarket) {
      // Original request order deliberately differs from registered allocation order.
      request.forecast.selection.bar_types.reverse();
      request.forecast.selection.maximum_rows = 2 * finalCount;
      request.train_end_ns = '720000000000'; request.validation_end_ns = '1440000000000';
      Object.assign(request.forecast.selection, { event_end_ns: request.validation_end_ns, decision_cutoff_ns: request.validation_end_ns });
    }
    const finalRequest = structuredClone(request);
    Object.assign(finalRequest.forecast.selection, { event_end_ns: '40000000000000', decision_cutoff_ns: '40000000000000' });
    if (crossMarket) Object.assign(finalRequest.forecast.selection, { event_end_ns: '2500000000000', decision_cutoff_ns: '2500000000000' });
    const members = single ? [{ model: wat, weight: '1' }] : [{ model: wat, weight: '0.25' }, { model: wat.replace('f64.const 1', 'f64.const -1'), weight: '0.75' }];
    const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
    const simulationSettings = nativeTestSettings('3000');
    if (universe) simulationSettings.fee_rates = [{ instrument_id: 'ETH-USD.COINBASE', maker: '0.004000', taker: '0.006000' }, ...(simulationSettings.fee_rates as JsonObject[])];
    const selectedSettings = universe && !crossMarket ? { ...simulationSettings, fee_rates: (simulationSettings.fee_rates as JsonObject[]).filter((fee) => fee.instrument_id === 'BTC-USD.COINBASE') } : simulationSettings;
    const config = configDefaults({ dataDir: directory, jobBinary: process.env.QZ_JOB_BINARY,
      research: { database: join(directory, 'research.sqlite'), modules: ['research', 'strategy-portfolio'].map((name) => resolve('../research/plugins', `${name}.ts`)) },
      datasets: [{ id: 'ensemble-native-fixture', name: 'Synthetic native protocol control', currency: 'USD', source: 'FIXTURE', assetClass: 'SPOT', start: null, end: null, ready: true,
        data: { catalog, request, sealed: { catalog: sealed, request: finalRequest } },
        strategy: { name: 'original-conditional-threshold', kind: crossMarket ? 'cross-market-threshold' : 'factor-threshold', ...(!crossMarket ? { instrumentId: universe ? 'ETH-USD.COINBASE' : 'BTC-USD.COINBASE' } : {}), ...(universe ? { instrumentIds: ['ETH-USD.COINBASE', 'BTC-USD.COINBASE'] } : {}), threshold: 0, investedWeight: '0.25',
          targetTtlNs: '7200000000000', forecastMode: 'price-volume', evaluationPartition: 'HELDOUT', ...(historical ? { evidenceMode: 'HISTORICAL_REASSESSMENT' } : {}), simulationSettings } }] });
    service = await createService({ config, models });
    const priorExposure = { catalog: sealed, barTypes: request.forecast.selection.bar_types, start: request.validation_end_ns, end: finalRequest.forecast.selection.event_end_ns, kind: 'FINAL', taskId: 9000, conversationId: 8000 };
    if (historical) await service.research.harness.commit(async (tx) => { (await tx.doc(Exposures)).windows.push(priorExposure); }, context);
    let selected: number | undefined; let validationFeedback: JsonObject | undefined;
    faux.setResponses([
      fauxAssistantMessage(fauxToolCall('strategy_portfolio_evaluate', { hypothesis: 'Native causal ensemble control, not market performance.', members, ...(crossMarket ? { allocations } : universe ? { instrumentId: 'BTC-USD.COINBASE' } : {}) }), { stopReason: 'toolUse' }),
      (transcript) => {
        const feedback = transcript.messages.findLast((message) => message.role === 'toolResult');
        assert.ok(feedback && feedback.role === 'toolResult');
        const result = JSON.parse(feedback.content.filter((item) => item.type === 'text').map((item) => item.text).join(''));
        assert.equal(result.portfolio.partition, 'VALIDATION');
        assert.equal(result.portfolio.evidenceMode, historical ? 'HISTORICAL_REASSESSMENT' : undefined);
        assert.ok(result.portfolio.statistics.length);
        assert.equal(JSON.stringify(result).includes('canonical_result'), false);
        assert.equal(JSON.stringify(result).includes('label_return'), false);
        selected = result.trialId; validationFeedback = result;
        return fauxAssistantMessage(fauxToolCall('research_trial', { trialId: selected! }), { stopReason: 'toolUse' });
      },
      (transcript) => {
        const message = transcript.messages.findLast((message) => message.role === 'toolResult');
        assert.ok(message && message.role === 'toolResult');
        const read = JSON.parse(message.content.filter((item) => item.type === 'text').map((item) => item.text).join(''));
        assert.equal(read.trialId, selected); assert.equal(read.status, 'COMPUTED');
        assert.deepEqual(read.result.portfolio, validationFeedback!.portfolio);
        assert.equal(read.result.evidenceTaskId, validationFeedback!.evidenceTaskId);
        assert.equal(JSON.stringify(read).includes('canonical_result'), false);
        assert.equal(JSON.stringify(read).includes('label_return'), false);
        return fauxAssistantMessage(fauxToolCall('research_brief', {}), { stopReason: 'toolUse' });
      },
      (transcript) => {
        const message = transcript.messages.findLast((message) => message.role === 'toolResult');
        assert.ok(message && message.role === 'toolResult');
        const brief = JSON.parse(message.content.filter((item) => item.type === 'text').map((item) => item.text).join(''));
        assert.equal(brief.totalTrials, 1); assert.equal(brief.nextAfterTrialId, null);
        assert.deepEqual(brief.trials.map((trial: JsonObject) => trial.taskId), [selected]);
        assert.equal('sealed' in brief.data, false);
        return fauxAssistantMessage(fauxToolCall('strategy_portfolio_finalize', { trialId: selected!, reason: 'Select original ensemble after read-only recovery; before final exposure.' }), { stopReason: 'toolUse' });
      },
    ]);
    const receipt = await service.work.startResearch({ requestId: 'native-ensemble', goal: 'Native ensemble isolation control; synthetic data only.',
      model: { provider: faux.provider.id, modelId: faux.getModel().id }, datasetId: 'ensemble-native-fixture', maxTrials: 1 });
    await (await service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const facts = (await service.research.view(receipt.conversationId)).research!;
    assert.equal(facts.trials.length, 1, 'read-only retrieval must not register another candidate');
    assert.equal(facts.trials[0]!.status, 'COMPUTED', JSON.stringify(facts.trials));
    assert.deepEqual(object(object(facts.trials[0]!.result).portfolio).statistics, object(validationFeedback!.portfolio).statistics);
    assert.equal(facts.final!.status, 'COMPUTED', JSON.stringify(facts.final));
    assert.equal(facts.final!.trialId, selected);
    assert.deepEqual(object(facts.trials[0]!.proposal).members, members);
    assert.deepEqual(object(facts.trials[0]!.proposal).strategy, config.datasets[0]!.strategy);
    assert.deepEqual(object(facts.trials[0]!.proposal).request, request);
    assert.equal(object(facts.trials[0]!.proposal).instrumentId, universe && !crossMarket ? 'BTC-USD.COINBASE' : undefined);
    if (crossMarket) assert.deepEqual(object(facts.trials[0]!.proposal).allocations, allocations);
    for (const outcome of [object(facts.trials[0]!.result), object(facts.final!.result)]) {
      const task: SettledTask<JsonObject> = await service.research.harness.waitForTask(outcome.evidenceTaskId as TaskId<JsonObject>, context);
      assert.equal(task.state.outcome.status, 'completed');
      const evidence = object(task.state.outcome.status === 'completed' ? task.state.outcome.result.native : null);
      assert.deepEqual(object(evidence.policy).simulationSettings, selectedSettings);
      assert.deepEqual(object(evidence.policy).instrumentId, crossMarket ? undefined : 'BTC-USD.COINBASE');
      if (crossMarket) assert.deepEqual(object(evidence.policy).allocations, allocations);
      assert.deepEqual(object(object(evidence.simulationRequest).selection).bar_types, selectedBars);
      assert.deepEqual(object(evidence.simulationRequest).settings, selectedSettings);
      for (const component of evidence.components as JsonObject[]) {
        assert.deepEqual(object(object(component.forecastRequest).selection).bar_types, crossMarket ? selectedBars : request.forecast.selection.bar_types);
      }
      assert.equal(object(evidence.simulation).native_version, '0.63.0');
      assert.ok(BigInt(String(object(evidence.simulation).orders)) > 0n);
      assert.ok(BigInt(String(object(evidence.simulation).positions)) > 0n);
      if (crossMarket) {
        const simulation = object(evidence.simulation);
        const canonical = object(simulation.canonical_result);
        assert.equal((canonical.accounts as JsonObject[]).length, 1);
        assert.deepEqual(new Set((canonical.orders as JsonObject[]).map((order) => object(object(Object.values(order)[0]).core).instrument_id)), new Set(allocations.map((item) => item.instrumentId)));
        const targets = object(evidence.simulationRequest).target_points as JsonObject[];
        assert.ok(targets.some((point) => JSON.stringify(point.rebalance_instrument_ids) === JSON.stringify(['BTC-USD.COINBASE'])));
        assert.ok(targets.some((point) => JSON.stringify(point.rebalance_instrument_ids) === JSON.stringify(['ETH-USD.COINBASE'])));
        assert.ok(targets.every((point) => (point.targets as JsonObject[]).length === 2));
      }
      assert.ok(nativeCurve(object(evidence.simulation), 'USD').some((point) => point.value !== null && decimalUnits(point.value) !== decimalUnits('3000')), 'original native fees must affect this flat-price account');
    }
    const final = object(facts.final!.result);
    const heldout = (final.partitions as JsonObject[]).find((partition) => partition.partition === 'HELDOUT')!;
    assert.ok(BigInt(String(object(object(heldout.metric).counts).included)) > 0n);
    assert.equal(object(final.portfolio).partition, 'HELDOUT');
    assert.equal(object(final.portfolio).evidenceMode, historical ? 'HISTORICAL_REASSESSMENT' : undefined);
    const finalTask = await service.research.harness.waitForTask(final.evidenceTaskId as TaskId<JsonObject>, context);
    assert.equal(finalTask.state.outcome.status, 'completed');
    const native = object(finalTask.state.outcome.status === 'completed' ? finalTask.state.outcome.result.native : null);
    assert.deepEqual(object(final.portfolio).statistics, object(native.simulation).statistics);
    assert.equal(object(object(native.simulationRequest).selection).event_start_ns, request.validation_end_ns);
    assert.ok(nativeCurve(object(native.simulation), 'USD').length);
    const built = await service.work.startPortfolio({ requestId: 'native-ensemble-replay', name: 'Original heldout ensemble',
      researchIds: [receipt.conversationId], datasetId: 'ensemble-native-fixture', capital: '3000' });
    await service.research.harness.waitForTask(built.taskId as TaskId, context);
    const fact = await service.work.getPortfolio(built.id);
    assert.equal(fact.view.status, 'COMPUTED', fact.view.error ?? '');
    assert.deepEqual(fact.view.curve, nativeCurve(object(native.simulation), 'USD'));
    const replay = object(fact.native!.simulationRequest);
    assert.equal(object(replay.selection).event_start_ns, request.validation_end_ns);
    assert.deepEqual(object(replay.selection).bar_types, selectedBars);
    assert.deepEqual(replay.settings, selectedSettings);
    assert.deepEqual(object(object((fact.native!.sources as JsonObject[])[0]).policy).simulationSettings, selectedSettings);
    assert.equal(object((fact.native!.sources as JsonObject[])[0]!.policy).evidenceMode, historical ? 'HISTORICAL_REASSESSMENT' : undefined);
    assert.deepEqual(object(fact.evidence[0]!.input).data, config.datasets[0]!.data);
    assert.deepEqual(object(object(fact.evidence[0]!.selected).proposal).strategy, config.datasets[0]!.strategy);
    assert.equal((simulationSettings.fee_rates as JsonObject[]).length, universe ? 2 : 1);
    for (const point of replay.target_points as JsonObject[]) {
      if (crossMarket) {
        assert.equal((point.targets as JsonObject[])[0]!.instrument_id, 'ETH-USD.COINBASE');
        assert.equal((point.targets as JsonObject[])[1]!.instrument_id, 'BTC-USD.COINBASE');
        assert.ok([0n, decimalUnits('0.025')].includes(decimalUnits(String((point.targets as JsonObject[])[0]!.weight))));
        assert.ok([0n, decimalUnits('0.0375')].includes(decimalUnits(String((point.targets as JsonObject[])[1]!.weight))));
        assert.equal((point.rebalance_instrument_ids as string[]).length, 1);
      } else assert.equal(object((point.targets as JsonObject[])[0]).weight, single ? '0.25' : '0.0625');
      assert.ok(BigInt(String(point.asof_ns)) >= BigInt(request.validation_end_ns));
      assert.ok(BigInt(String(point.asof_ns)) < BigInt(finalRequest.forecast.selection.event_end_ns));
    }
    if (single) {
      const owner = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'native-capacity-test-only' } });
      const headers = { cookie: owner.cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ') };
      const original = (await service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers })).json();
      const factors = (await service.app.inject({ url: '/api/v3/factors', headers })).json(); const factor = factors.find((row: { combinable: boolean }) => row.combinable); assert.ok(factor);
      assert.equal((await service.app.inject({ url: `/api/v3/factors/${factor.id}/capacity`, headers })).json().reason, 'explicit_capacity_policy_missing');
      const policy = { scenarios: ['1000', '10000'], participation: '0.01', maximumAgeSeconds: 300 };
      const assessed = await service.app.inject({ method: 'POST', url: `/api/v3/factors/${factor.id}/capacity`, headers, payload: policy }); assert.equal(assessed.statusCode, 200, assessed.body); assert.equal(assessed.json().status, 'ESTIMATED', assessed.body); assert.equal(assessed.json().assumptions.participation, policy.participation); assert.ok(assessed.json().scenarios.some((row: { capital: string }) => decimalUnits(row.capital) === decimalUnits('1000')));
      assert.deepEqual(assessed.json().requestedAssumptions, policy);
      assert.ok(assessed.json().scenarios.length > policy.scenarios.length, 'native diagnostic rows remain visible, separate from the original request');
      const originalPortfolio = await service.work.getPortfolio(built.id);
      assert.equal(originalPortfolio.view.capacity?.reason, 'explicit_capacity_policy_missing_or_incompatible');
      assert.equal(originalPortfolio.view.capacity?.requestedAssumptions, undefined);
      const portfolioPolicy = { ...policy, participation: '0.0100', scenarios: ['10000.000000000000000001', '01000.00', '10000.000000000000000001'] };
      const reassessed = await service.app.inject({ method: 'POST', url: `/api/v3/portfolios/${built.id}/capacity`, headers, payload: portfolioPolicy });
      assert.equal(reassessed.statusCode, 200, reassessed.body); assert.equal(reassessed.json().status, 'ESTIMATED', reassessed.body);
      assert.equal(reassessed.json().scope, built.id); assert.equal(reassessed.json().scenarios[0].capital, portfolioPolicy.scenarios[0]);
      assert.deepEqual(reassessed.json().requestedAssumptions, portfolioPolicy, 'keep explicit order, duplicates, decimal strings and participation spelling');
      assert.equal(reassessed.json().scenarios[1].capital, '1000', 'native canonical decimals are not a lossless copy of the request');
      assert.equal(reassessed.json().scenarios.length, portfolioPolicy.scenarios.length + 3);
      assert.ok(reassessed.json().scenarios.some((row: { capital: string }) => decimalUnits(row.capital) === decimalUnits('3000')), 'use the original portfolio starting capital, not a regenerated factor mapping');
      assert.deepEqual(await service.work.getPortfolio(built.id), originalPortfolio, 'read-only capacity must retain the original native request, sources, target, status and old report');
      assert.deepEqual((await service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers })).json(), original, 'post-research capacity must not change legacy admission, trials, independent evidence or identities');
      const explicitBuild = await service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers, payload: { requestId: 'native-explicit-capacity', name: 'Original native pool with explicit capacity assumptions',
        researchIds: [receipt.conversationId], datasetId: 'ensemble-native-fixture', capital: '3000', capacityPolicy: portfolioPolicy } });
      assert.equal(explicitBuild.statusCode, 202, explicitBuild.body);
      await service.research.harness.waitForTask(explicitBuild.json().taskId as TaskId, context);
      const saved = await service.work.getPortfolio(explicitBuild.json().id);
      assert.equal(saved.view.status, 'COMPUTED', saved.view.error ?? '');
      assert.deepEqual(saved.view.capacity?.requestedAssumptions, portfolioPolicy);
      assert.equal(saved.view.capacity?.scenarios.length, portfolioPolicy.scenarios.length + 3);
      assert.deepEqual((await service.app.inject({ url: `/api/v3/portfolios/${saved.view.id}`, headers })).json().capacity, saved.view.capacity, 'existing portfolio storage/HTTP preserve the enriched report');
      assert.deepEqual(await service.work.getPortfolio(built.id), originalPortfolio);
    }
    const exposures = (await service.research.harness.snapshot(Exposures, context))!.windows;
    assert.deepEqual(exposures.map((window) => window.kind), historical ? ['FINAL', 'HISTORICAL_ITERATION', 'HISTORICAL_FINAL'] : ['ITERATION', 'FINAL']);
    if (historical) assert.deepEqual(exposures[0], priorExposure);
    assert.equal(faux.state.callCount, 4); // Two extra read-only rounds, not extra native evaluations.
  } finally { await service?.close(); await rm(directory, { recursive: true, force: true }); }
});
