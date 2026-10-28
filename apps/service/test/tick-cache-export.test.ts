import assert from 'node:assert/strict';
import { link, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { clipTickRegimeEvaluation } from '../../../packages/contracts/src/tick-input.ts';
import type { NativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { exportDataset } from '../src/dataset-export.ts';
import { readSourceRevision } from '../src/pm-source.ts';
import { context, fauxAssistantMessage, setup } from './support.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import heldoutWasm from '../../research/plugins/heldout-wasm.ts';

// Controlled framing/publication only; native numerical cache checks belong to Rust.
async function preparedCache(target: NativeTickFactorRequest['target'] = { kind: 'FIXED_TRADE_COHORTS', horizon_cohorts: 1 }, flow = false, regimes = false) {
  const representation = flow ? 'EVENT_TIME_COHORT_FLOW_CACHE_V1' : 'EVENT_TIME_COHORT_CACHE_V1'; const frameBytes = flow ? 40 : 32;
  const dayNs = 86_400_000_000_000n; const clock = (value: string | number) => String(BigInt(value) * (regimes ? dayNs : 1n));
  const directory = await mkdtemp(join(tmpdir(), 'qz-cohort-export-')); const prepared = join(directory, 'prepared');
  const request: NativeTickFactorRequest = { schema_version: 1,
    input: { modality: 'TRADE_TICK', representation, maximum_source_rows: '1308222597',
      instrument_ids: ['BTCUSDT.SIM'], event_start_ns: '0', event_end_ns: clock(80), decision_cutoff_ns: clock(80),
      source_revision: 'controlled-source-not-market-evidence', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
    sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100 }, target,
    parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 }, train_end_ns: clock(30), validation_end_ns: clock(50) };
  if (regimes) request.evaluation_regimes = { kind: 'BTC_COMPLETED_30_DAY_RETURN_V1', days: Array.from({ length: 80 }, (_, index) => ({
    day_start_ns: clock(index), previous: index === 79 ? { event_ns: String(BigInt(clock(index)) - 1n), price: '91314159.987654321' } : null, lookback: null })) };
  const research = clipTickRegimeEvaluation(request, request.validation_end_ns);
  const source = join(directory, 'derived-cache.bin'); const bytes = Buffer.alloc(6 * frameBytes);
  for (const [index, event] of [10, 20, 40, 50, 60, 70].entries()) {
    bytes.writeBigUInt64LE(BigInt(clock(event)), index * frameBytes); bytes.writeDoubleLE(index + 1, index * frameBytes + 8);
    bytes.writeDoubleLE(flow ? 2 : 1, index * frameBytes + 16); bytes.writeBigUInt64LE(flow ? 2n : 1n, index * frameBytes + 24);
    if (flow) bytes.writeDoubleLE([2, -2, 0][index % 3]!, index * frameBytes + 32);
  }
  await writeFile(source, bytes);
  for (const [part, selection, count, last] of [['research', research, '3', '40'], ['independent', request, '6', '70']] as const) {
    const root = join(prepared, part, 'catalog'); const path = 'data/cohorts/BTCUSDT.SIM.bin';
    await mkdir(join(root, 'data/cohorts'), { recursive: true }); await link(source, join(root, path));
    await writeFile(join(root, 'cohort-cache.json'), JSON.stringify({ schema_version: 1, representation,
      source_revision: selection.input.source_revision, availability: selection.input.availability,
      event_start_ns: selection.input.event_start_ns, event_end_ns: selection.input.event_end_ns, trade_count: String(Number(count) * (flow ? 2 : 1)), cohort_count: count,
      source_report: { status: 'IMPORTED', source_revision: selection.input.source_revision },
      files: [{ instrument_id: 'BTCUSDT.SIM', path, bytes: String(bytes.length), byte_start: '0', byte_end: String(Number(count) * frameBytes),
        trade_count: String(Number(count) * (flow ? 2 : 1)), cohort_count: count, first_event_ns: clock(10), last_event_ns: clock(last) }] }));
  }
  const metadata = { schema_version: 1, modality: 'TRADE_TICK', representation, request, research_request: research,
    research_catalog: 'research/catalog', independent_catalog: 'independent/catalog', catalog_format: flow ? 'QZ_EVENT_TIME_COHORT_FLOW_CACHE_V1' : 'QZ_EVENT_TIME_COHORT_CACHE_V1',
    operational_state: 'NATIVE_PREPARED', scientific_qualification: 'UNQUALIFIED_EXPLORATORY',
    coverage: { source_revision: request.input.source_revision, availability: request.input.availability, instrument_ids: request.input.instrument_ids,
      trade_count: flow ? '12' : '6', cohort_count: '6' } };
  const metadataFile = join(prepared, 'catalog-metadata.json'); const requestFile = join(directory, 'request.json');
  await writeFile(metadataFile, JSON.stringify(metadata)); await writeFile(requestFile, JSON.stringify(request));
  return { directory, prepared, source, bytes, request, research, options: { prepared: metadataFile, request: requestFile,
    output: join(directory, 'dataset.json'), id: 'cohort-cache', name: 'Controlled cohort publication', ready: true } };
}

test('regime-aware export binds the clipped primary calendar and keeps future BTC marks in the sealed request', async () => {
  const f = await preparedCache(undefined, true, true);
  try {
    const exported = await exportDataset(f.options);
    const primary = (exported.data.nativeInput as { request: NativeTickFactorRequest }).request;
    const sealed = ((exported.data.sealed as { nativeInput: { request: NativeTickFactorRequest } }).nativeInput).request;
    assert.equal(primary.evaluation_regimes!.days.length, 50);
    assert.equal(sealed.evaluation_regimes!.days.length, 80);
    assert.equal(JSON.stringify(primary).includes('91314159.987654321'), false);
    assert.equal(JSON.stringify(sealed).includes('91314159.987654321'), true);
    const metadata = JSON.parse(await readFile(f.options.prepared, 'utf8'));
    metadata.research_request.evaluation_regimes = f.request.evaluation_regimes;
    await writeFile(f.options.prepared, JSON.stringify(metadata));
    await assert.rejects(exportDataset({ ...f.options, output: join(f.directory, 'invalid.json') }), /regime_days_invalid/);
  } finally { await rm(f.directory, { recursive: true, force: true }); }
});

test('cohort range views publish one owned snapshot file while keeping source and held-out boundaries separate', async () => {
  const f = await preparedCache();
  try {
    const exported = await exportDataset(f.options); const revision = (await readSourceRevision(exported))!;
    const data = revision.configured.data; const primary = String(data.catalog); const independent = String((data.sealed as Record<string, unknown>).catalog);
    assert.equal(exported.data.catalog, primary, 'research configuration must point to the immutable published revision');
    assert.deepEqual(JSON.parse(await readFile(f.options.output, 'utf8')).data, data);
    assert.deepEqual(exported.data.nativeInput, { modality: 'TRADE_TICK', request: f.research });
    assert.equal(exported.inputCoverage?.representation, 'EVENT_TIME_COHORT_CACHE_V1');
    const path = 'data/cohorts/BTCUSDT.SIM.bin'; const left = await stat(join(primary, path)); const right = await stat(join(independent, path));
    assert.equal(left.ino, right.ino); assert.equal(left.dev, right.dev);
    assert.notEqual(left.ino, (await stat(f.source)).ino, 'published snapshot never hardlinks to the mutable producer');
    assert.equal(JSON.parse(await readFile(join(primary, 'cohort-cache.json'), 'utf8')).files[0].byte_end, '96');
    assert.equal(JSON.parse(await readFile(join(independent, 'cohort-cache.json'), 'utf8')).files[0].byte_end, '192');
    await writeFile(f.source, Buffer.alloc(f.bytes.length));
    assert.deepEqual(await readFile(join(primary, path)), f.bytes, 'producer mutation cannot change either published view');
    assert.equal(exported.preparation?.scientificQualification, 'UNVERIFIED');
  } finally { await rm(f.directory, { recursive: true, force: true }); }
});

test('explicit funding sidecars survive owned publication while unrelated files still fail', async () => {
  const f = await preparedCache(undefined, true);
  try {
    const rates = '{"instrument_id":"BTCUSDT.SIM","event_ns":"10","rate":"0.00001"}\n';
    const funding = JSON.stringify({ source_revision: f.request.input.source_revision, start_ns: '0', end_ns: '80',
      instruments: ['BTCUSDT.SIM'], rows: 1, missing_instruments: [] });
    const extras = [{ path: 'funding-rates.jsonl', bytes: String(Buffer.byteLength(rates)) },
      { path: 'funding-rates.manifest.json', bytes: String(Buffer.byteLength(funding)) }];
    for (const part of ['research', 'independent']) {
      const root = join(f.prepared, part, 'catalog');
      await writeFile(join(root, extras[0]!.path), rates); await writeFile(join(root, extras[1]!.path), funding);
    }
    const metadata = JSON.parse(await readFile(f.options.prepared, 'utf8')); metadata.economic_sources = extras;
    await writeFile(f.options.prepared, JSON.stringify(metadata));
    const result = await exportDataset(f.options);
    assert.equal(await readFile(join(String(result.data.catalog), extras[0]!.path), 'utf8'), rates);
    await writeFile(join(f.prepared, 'research/catalog/unrelated.json'), '{}');
    await assert.rejects(exportDataset({ ...f.options, output: join(f.directory, 'extra.json') }), /unmanifested/);
  } finally { await rm(f.directory, { recursive: true, force: true }); }
});

test('40-byte signed-flow views preserve positive, negative and balanced-zero marks in owned publication', async () => {
  const f = await preparedCache({ kind: 'FIXED_EVENT_TIME', horizon_ns: '10' }, true);
  try {
    const exported = await exportDataset(f.options);
    assert.equal(exported.inputCoverage?.representation, 'EVENT_TIME_COHORT_FLOW_CACHE_V1');
    assert.equal(exported.inputCoverage?.catalog_format, 'QZ_EVENT_TIME_COHORT_FLOW_CACHE_V1');
    const bytes = await readFile(join(String(exported.data.catalog), 'data/cohorts/BTCUSDT.SIM.bin'));
    assert.equal(bytes.length, 240); assert.equal(bytes.readDoubleLE(32), 2);
    assert.equal(bytes.readDoubleLE(40 + 32), -2); assert.equal(bytes.readDoubleLE(80 + 32), 0);
    assert.equal(JSON.parse(await readFile(join(String(exported.data.catalog), 'cohort-cache.json'), 'utf8')).files[0].byte_end, '120');
    assert.deepEqual(bytes, f.bytes);
  } finally { await rm(f.directory, { recursive: true, force: true }); }
});

test('signed-flow export rejects wrong representation, format and 32-byte frames', async () => {
  for (const failure of ['representation', 'format', 'frame'] as const) {
    const f = await preparedCache(undefined, true);
    try {
      if (failure === 'format') {
        const metadata = JSON.parse(await readFile(f.options.prepared, 'utf8'));
        metadata.catalog_format = 'QZ_EVENT_TIME_COHORT_CACHE_V1'; await writeFile(f.options.prepared, JSON.stringify(metadata));
      } else {
        for (const [part, count] of [['research', 3], ['independent', 6]] as const) {
          const path = join(f.prepared, part, 'catalog/cohort-cache.json'); const manifest = JSON.parse(await readFile(path, 'utf8'));
          if (failure === 'representation') manifest.representation = 'EVENT_TIME_COHORT_CACHE_V1';
          else Object.assign(manifest.files[0], { bytes: '192', byte_end: String(count * 32) });
          await writeFile(path, JSON.stringify(manifest));
        }
        if (failure === 'frame') await writeFile(f.source, Buffer.alloc(192));
      }
      await assert.rejects(exportDataset(f.options), failure === 'format' ? /representation_mismatch/ : failure === 'representation' ? /manifest_mismatch/ : /frame_range_invalid/);
    } finally { await rm(f.directory, { recursive: true, force: true }); }
  }
});

test('cohort export rejects silently omitted in-range prefix or suffix frames', async () => {
  for (const failure of ['suffix', 'prefix'] as const) {
    const f = await preparedCache();
    try {
      for (const part of failure === 'prefix' ? ['research', 'independent'] : ['research']) {
        const path = join(f.prepared, part, 'catalog/cohort-cache.json'); const manifest = JSON.parse(await readFile(path, 'utf8'));
        const entry = manifest.files[0];
        if (failure === 'prefix') Object.assign(entry, { byte_start: '32', first_event_ns: '20',
          cohort_count: String(Number(entry.cohort_count) - 1), trade_count: String(Number(entry.trade_count) - 1) });
        else Object.assign(entry, { byte_end: '64', cohort_count: '2', trade_count: '2', last_event_ns: '20' });
        manifest.cohort_count = entry.cohort_count; manifest.trade_count = entry.trade_count;
        await writeFile(path, JSON.stringify(manifest));
      }
      if (failure === 'prefix') {
        const metadata = JSON.parse(await readFile(f.options.prepared, 'utf8'));
        Object.assign(metadata.coverage, { trade_count: '5', cohort_count: '5' });
        await writeFile(f.options.prepared, JSON.stringify(metadata));
      }
      await assert.rejects(exportDataset(f.options), /boundary_incomplete/);
    } finally { await rm(f.directory, { recursive: true, force: true }); }
  }
});

test('configured research admits the published cache snapshot after producer bytes change', async () => {
  const f = await preparedCache(); const state = await setup();
  try {
    const exported = await exportDataset(f.options); await writeFile(f.source, Buffer.alloc(f.bytes.length));
    state.options.extensions.push(factorWasm, heldoutWasm); state.options.config.datasets = [exported]; await state.restart();
    state.faux.setResponses([fauxAssistantMessage('Controlled snapshot admission only; no numerical result claimed.')]);
    const started = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
      payload: { requestId: 'cohort-snapshot-admission', goal: 'Retain the original published source', datasetId: exported.id, model: state.model, maxTrials: 0 } });
    assert.equal(started.statusCode, 202, started.body); const receipt = started.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const detail = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    assert.equal(detail.json().input.data.catalog, exported.data.catalog);
    assert.deepEqual(await readFile(join(detail.json().input.data.catalog, 'data/cohorts/BTCUSDT.SIM.bin')), f.bytes);
  } finally { await state.cleanup(); await rm(f.directory, { recursive: true, force: true }); }
});

test('cohort export rejects primary widening, partial frames and altered boundary clocks', async () => {
  for (const failure of ['widen', 'frame', 'clock'] as const) {
    const f = await preparedCache();
    try {
      const path = join(f.prepared, 'research/catalog/cohort-cache.json'); const manifest = JSON.parse(await readFile(path, 'utf8'));
      if (failure === 'widen') Object.assign(manifest.files[0], { byte_end: '192', cohort_count: '6', trade_count: '6', last_event_ns: '70' });
      if (failure === 'frame') manifest.files[0].byte_end = '95';
      if (failure === 'clock') manifest.files[0].last_event_ns = '41';
      await writeFile(path, JSON.stringify(manifest));
      await assert.rejects(exportDataset(f.options), failure === 'widen' ? /event_range_invalid/ : failure === 'frame' ? /frame_range_invalid/ : /boundary_mismatch/);
    } finally { await rm(f.directory, { recursive: true, force: true }); }
  }
});

test('cohort export requires both range views to bind the same derived file and source report', async () => {
  for (const failure of ['file', 'source'] as const) {
    const f = await preparedCache();
    try {
      const root = join(f.prepared, 'research/catalog');
      if (failure === 'file') {
        const path = join(root, 'data/cohorts/BTCUSDT.SIM.bin'); await rm(path); await writeFile(path, f.bytes);
      } else {
        const path = join(root, 'cohort-cache.json'); const manifest = JSON.parse(await readFile(path, 'utf8'));
        manifest.source_report.processing_view = 'different-source-view'; await writeFile(path, JSON.stringify(manifest));
      }
      await assert.rejects(exportDataset(f.options), /source_view_mismatch/);
    } finally { await rm(f.directory, { recursive: true, force: true }); }
  }
});


test('fixed event-time target survives prepared cache export in both bounded requests', async () => {
  const target = { kind: 'FIXED_EVENT_TIME' as const, horizon_ns: '60000000000' };
  const f = await preparedCache(target);
  try {
    const exported = await exportDataset(f.options);
    const primary = exported.data.nativeInput as { request: NativeTickFactorRequest };
    const independent = (exported.data.sealed as { nativeInput: { request: NativeTickFactorRequest } }).nativeInput;
    assert.deepEqual(primary.request.target, target); assert.deepEqual(independent.request.target, target);
    assert.equal(primary.request.input.event_end_ns, '50'); assert.equal(independent.request.input.event_end_ns, '80');
  } finally { await rm(f.directory, { recursive: true, force: true }); }
});


test('explicit context sidecars retain exact source identity and owned bytes', async () => {
  for (const failure of [null, 'revision', 'rows', 'path', 'bytes', 'view'] as const) {
    const f = await preparedCache(undefined, true);
    try {
      const rates = '{"instrument_id":"BTCUSDT.SIM","event_ns":"10","rate":"0.00001"}\n';
      const funding = JSON.stringify({ source_revision: f.request.input.source_revision, start_ns: '0', end_ns: '80',
        instruments: ['BTCUSDT.SIM'], rows: 1, missing_instruments: [] });
      const csv = 'event_ns,oracle_px,mark_px\n10,0.000000000000000001,3.044\n20,,3.045\n';
      const manifest = { schema_version: 1, format: 'EVENT_TIME_CONTEXT_PRICE_CSV_V1', clock: 'EVENT_TIME_PROXY_UNVERIFIED',
        source_revision: 'separate-original-context-source', research_source_revision: f.request.input.source_revision,
        start_ns: '0', end_ns: '80', rows: '2', instrument_ids: ['BTCUSDT.SIM'],
        files: [{ instrument_id: 'BTCUSDT.SIM', path: 'context-prices-000.csv', bytes: String(Buffer.byteLength(csv)), rows: '2', first_event_ns: '10', last_event_ns: '20' }],
        missing_dates: ['controlled missing source date'], limitations: ['EVENT_TIME_PROXY_UNVERIFIED'] };
      if (failure === 'revision') manifest.research_source_revision = 'different-source';
      if (failure === 'rows') manifest.rows = '3';
      if (failure === 'path') manifest.files[0]!.path = '../context-prices-000.csv';
      if (failure === 'bytes') manifest.files[0]!.bytes = String(Buffer.byteLength(csv) + 1);
      const sources = new Map([['funding-rates.jsonl', rates], ['funding-rates.manifest.json', funding],
        ['context-prices.manifest.json', JSON.stringify(manifest)], ['context-prices-000.csv', csv]]);
      for (const [name, bytes] of sources) {
        const source = join(f.directory, name); await writeFile(source, bytes);
        for (const part of ['research', 'independent']) await link(source, join(f.prepared, part, 'catalog', name));
      }
      if (failure === 'view') {
        const path = join(f.prepared, 'research/catalog/context-prices-000.csv'); await rm(path); await writeFile(path, csv);
      }
      const metadata = JSON.parse(await readFile(f.options.prepared, 'utf8'));
      metadata.economic_sources = [...sources].map(([path, bytes]) => ({ path, bytes: String(Buffer.byteLength(bytes)) }));
      await writeFile(f.options.prepared, JSON.stringify(metadata));
      if (failure) {
        await assert.rejects(exportDataset(f.options), failure === 'revision' ? /context_source_coverage_mismatch/ : failure === 'rows' ? /context_row_count_mismatch/
          : failure === 'path' ? /context_file_invalid/ : failure === 'bytes' ? /economic_source_size_mismatch/ : /context_source_view_mismatch/);
      } else {
        const result = await exportDataset(f.options); const primary = String(result.data.catalog);
        const independent = String((result.data.sealed as Record<string, unknown>).catalog);
        assert.equal(await readFile(join(primary, 'context-prices-000.csv'), 'utf8'), csv);
        assert.equal((await stat(join(primary, 'context-prices-000.csv'))).ino, (await stat(join(independent, 'context-prices-000.csv'))).ino);
        await writeFile(join(f.directory, 'context-prices-000.csv'), 'changed producer');
        assert.equal(await readFile(join(primary, 'context-prices-000.csv'), 'utf8'), csv);
        assert.equal(JSON.parse(await readFile(join(primary, 'context-prices.manifest.json'), 'utf8')).source_revision, 'separate-original-context-source');
      }
    } finally { await rm(f.directory, { recursive: true, force: true }); }
  }
});
