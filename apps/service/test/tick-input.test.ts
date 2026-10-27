import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { NativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { exportDataset } from '../src/dataset-export.ts';
import { factorViews } from '../src/factors.ts';
import { datasetResearchSettings } from '../../../packages/contracts/src/research.ts';
import { context, fauxAssistantMessage, setup, wat } from './support.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import heldoutWasm from '../../research/plugins/heldout-wasm.ts';

const request: NativeTickFactorRequest = { schema_version: 1,
  input: { modality: 'TRADE_TICK', instrument_ids: ['BTCUSDT.SIM'], event_start_ns: '900719925474099300', event_end_ns: '900719925474099900', decision_cutoff_ns: '900719925474100000', maximum_rows: 100, source_revision: 'pinned-test-source-not-market-evidence', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
  sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100 }, target: { kind: 'FIXED_TRADE_COHORTS', horizon_cohorts: 2 },
  parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 }, train_end_ns: '900719925474099500', validation_end_ns: '900719925474099700' };
const research = { ...request, input: { ...request.input, event_end_ns: request.validation_end_ns } };

// Framing/publication evidence only; native numerical/Parquet checks run in Rust.
async function preparedFixture() {
  const directory = await mkdtemp(join(tmpdir(), 'qz-tick-export-')); const prepared = join(directory, 'prepared');
  for (const part of ['research', 'independent']) {
    const catalog = join(prepared, part, 'catalog'); await mkdir(join(catalog, 'data/trades/BTCUSDT.SIM'), { recursive: true });
    await writeFile(join(catalog, 'data/trades/BTCUSDT.SIM/test.parquet'), 'PAR1framing-fixture-not-native-evidencePAR1');
    await writeFile(join(catalog, 'import-provenance.json'), JSON.stringify({ status: 'IMPORTED', source_revision: request.input.source_revision }));
  }
  const metadata = { schema_version: 1, modality: 'TRADE_TICK', request, research_request: research,
    research_catalog: 'research/catalog', independent_catalog: 'independent/catalog',
    catalog_format: 'NAUTILUS_HIGH_PRECISION_16', operational_state: 'NATIVE_PREPARED', scientific_qualification: 'UNQUALIFIED_EXPLORATORY',
    coverage: { source_revision: request.input.source_revision, instrument_ids: request.input.instrument_ids, availability: request.input.availability, trade_count: '6', cohort_count: '5' } };
  const metadataFile = join(prepared, 'catalog-metadata.json'); const requestFile = join(directory, 'request.json');
  await writeFile(metadataFile, JSON.stringify(metadata)); await writeFile(requestFile, JSON.stringify(request));
  return { directory, prepared, metadata, options: { prepared: metadataFile, request: requestFile, output: join(directory, 'dataset.json'), id: 'ticks', name: 'Explicit tick fixture', ready: true } };
}

test('tick export preserves exact selection and separate partitions with operational readiness and unqualified evidence', async () => {
  const f = await preparedFixture();
  try {
    const exported = await exportDataset(f.options);
    assert.equal(exported.ready, true); assert.equal(exported.preparation?.scientificQualification, 'UNVERIFIED');
    assert.deepEqual(exported.data.nativeInput, { modality: 'TRADE_TICK', request: research });
    assert.deepEqual((exported.data.sealed as Record<string, unknown>).nativeInput, { modality: 'TRADE_TICK', request });
    assert.notEqual(exported.data.catalog, (exported.data.sealed as Record<string, unknown>).catalog);
    assert.equal(exported.inputCoverage?.event_start_ns, request.input.event_start_ns);
    assert.match(await readFile(exported.revisionManifest!, 'utf8'), /TRADE_TICK/);
    const settings = datasetResearchSettings(exported.data);
    assert.equal(settings.modality, 'TRADE_TICK'); assert.equal(settings.horizonUnit, 'TRADE_COHORTS'); assert.equal(settings.horizon, 2); assert.equal(settings.strategy, null);
  } finally { await rm(f.directory, { recursive: true, force: true }); }
});

test('tick export rejects duplicated original keys, qualification relabeling, missing ticks and changed sampling', async () => {
  for (const failure of ['duplicate', 'qualification', 'missing', 'sampling'] as const) {
    const f = await preparedFixture();
    try {
      if (failure === 'duplicate') await writeFile(f.options.request, JSON.stringify(request).replace('"schema_version":1', '"schema_version":0,"schema_version":1'));
      if (failure === 'qualification') { f.metadata.scientific_qualification = 'PIT_QUALIFIED'; await writeFile(f.options.prepared, JSON.stringify(f.metadata)); }
      if (failure === 'missing') await rm(join(f.prepared, 'research/catalog/data/trades'), { recursive: true });
      if (failure === 'sampling') await writeFile(f.options.request, JSON.stringify({ ...request, target: { ...request.target, horizon_cohorts: 3 } }));
      await assert.rejects(exportDataset(f.options), failure === 'duplicate' ? /duplicate_json_key/ : failure === 'qualification' ? /metadata_invalid/ : failure === 'missing' ? /trades_required/ : /request_mismatch/);
    } finally { await rm(f.directory, { recursive: true, force: true }); }
  }
});

test('HTTP registration and replay retain one explicit tick contract and prevent mixed inputs or portfolio admission', async () => {
  const state = await setup();
  try {
    state.options.extensions.push(factorWasm, heldoutWasm);
    const data = { catalog: join(state.directory, 'catalog'), nativeInput: { modality: 'TRADE_TICK', request: research }, sealed: { catalog: join(state.directory, 'catalog'), nativeInput: { modality: 'TRADE_TICK', request } } };
    state.options.config.datasets = [{ id: 'ticks', name: 'Ticks', assetClass: 'CRYPTO', currency: 'MULTI', source: 'Controlled protocol only', start: null, end: null, ready: true, nativeModality: 'TRADE_TICK', preparation: { archiveLoaded: true, nativePrepared: true, scientificQualification: 'UNVERIFIED' }, data }];
    await state.restart(); state.faux.setResponses([fauxAssistantMessage('Input accepted; no scientific calculation claimed.')]);
    const payload = { requestId: 'tick-admission', goal: 'Controlled tick admission', datasetId: 'ticks', model: state.model, maxTrials: 0 };
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload });
    assert.equal(response.statusCode, 202, response.body); const receipt = response.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const detail = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    assert.deepEqual(detail.json().input.data, data); assert.equal(detail.json().dataset.nativeModality, 'TRADE_TICK');
    const replay = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload });
    assert.equal(replay.json().conversationId, receipt.conversationId);
    const list = await state.service.app.inject({ url: '/api/v3/datasets', headers: state.headers });
    assert.equal(list.json()[0].researchSettings.horizonUnit, 'TRADE_COHORTS'); assert.equal(list.json()[0].preparation.scientificQualification, 'UNVERIFIED');
    const portfolio = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: { requestId: 'unsupported-tick', name: 'No implicit simulation', datasetId: 'ticks', capital: '1', researchIds: [receipt.conversationId] } });
    assert.equal(portfolio.statusCode, 422); assert.match(portfolio.body, /tick_portfolio_unsupported/);
    state.options.config.datasets[0]!.data.request = {};
    await state.restart();
    const mixed = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { ...payload, requestId: 'mixed' } });
    assert.equal(mixed.statusCode, 400); assert.match(mixed.body, /ambiguous_factor_request/);
  } finally { await state.cleanup(); }
});

test('factor library keeps process-independent tick metrics visibly exploratory and non-combinable', () => {
  const result = { partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '2' }, pearson_ic: { value: 0.2 } } }] };
  const input = { goal: 'Ticks', model: { provider: 'test', modelId: 'test' }, data: { nativeInput: { modality: 'TRADE_TICK', request: research } }, policy: { max_trials: 1 } };
  const factor = factorViews(1, { input, trials: [{ taskId: 2, status: 'COMPUTED', proposal: { model: wat }, result }], final: { status: 'COMPUTED', trialId: 2, result } })[0]!;
  assert.equal(factor.evidence, 'independent'); assert.equal(factor.nativeModality, 'TRADE_TICK');
  assert.equal(factor.availability, 'EVENT_TIME_PROXY_UNVERIFIED'); assert.equal(factor.scientificQualification, 'UNQUALIFIED_EXPLORATORY');
  assert.equal(factor.combinable, false); assert.equal(factor.reason, 'tick_portfolio_unsupported');
});
