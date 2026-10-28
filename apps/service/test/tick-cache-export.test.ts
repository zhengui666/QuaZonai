import assert from 'node:assert/strict';
import { link, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import type { NativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { exportDataset } from '../src/dataset-export.ts';
import { readSourceRevision } from '../src/pm-source.ts';
import { context, fauxAssistantMessage, setup } from './support.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import heldoutWasm from '../../research/plugins/heldout-wasm.ts';

// Controlled framing/publication only; native numerical cache checks belong to Rust.
async function preparedCache(target: NativeTickFactorRequest['target'] = { kind: 'FIXED_TRADE_COHORTS', horizon_cohorts: 1 }) {
  const directory = await mkdtemp(join(tmpdir(), 'qz-cohort-export-')); const prepared = join(directory, 'prepared');
  const request: NativeTickFactorRequest = { schema_version: 1,
    input: { modality: 'TRADE_TICK', representation: 'EVENT_TIME_COHORT_CACHE_V1', maximum_source_rows: '1308222597',
      instrument_ids: ['BTCUSDT.SIM'], event_start_ns: '0', event_end_ns: '80', decision_cutoff_ns: '80',
      source_revision: 'controlled-source-not-market-evidence', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
    sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100 }, target,
    parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 }, train_end_ns: '30', validation_end_ns: '50' };
  const research = { ...request, input: { ...request.input, event_end_ns: request.validation_end_ns } };
  const source = join(directory, 'derived-cache.bin'); const bytes = Buffer.alloc(6 * 32);
  for (const [index, event] of [10, 20, 40, 50, 60, 70].entries()) {
    bytes.writeBigUInt64LE(BigInt(event), index * 32); bytes.writeDoubleLE(index + 1, index * 32 + 8);
    bytes.writeDoubleLE(1, index * 32 + 16); bytes.writeBigUInt64LE(1n, index * 32 + 24);
  }
  await writeFile(source, bytes);
  for (const [part, selection, count, last] of [['research', research, '3', '40'], ['independent', request, '6', '70']] as const) {
    const root = join(prepared, part, 'catalog'); const path = 'data/cohorts/BTCUSDT.SIM.bin';
    await mkdir(join(root, 'data/cohorts'), { recursive: true }); await link(source, join(root, path));
    await writeFile(join(root, 'cohort-cache.json'), JSON.stringify({ schema_version: 1, representation: 'EVENT_TIME_COHORT_CACHE_V1',
      source_revision: selection.input.source_revision, availability: selection.input.availability,
      event_start_ns: selection.input.event_start_ns, event_end_ns: selection.input.event_end_ns, trade_count: count, cohort_count: count,
      source_report: { status: 'IMPORTED', source_revision: selection.input.source_revision },
      files: [{ instrument_id: 'BTCUSDT.SIM', path, bytes: String(bytes.length), byte_start: '0', byte_end: String(Number(count) * 32),
        trade_count: count, cohort_count: count, first_event_ns: '10', last_event_ns: last }] }));
  }
  const metadata = { schema_version: 1, modality: 'TRADE_TICK', representation: 'EVENT_TIME_COHORT_CACHE_V1', request, research_request: research,
    research_catalog: 'research/catalog', independent_catalog: 'independent/catalog', catalog_format: 'QZ_EVENT_TIME_COHORT_CACHE_V1',
    operational_state: 'NATIVE_PREPARED', scientific_qualification: 'UNQUALIFIED_EXPLORATORY',
    coverage: { source_revision: request.input.source_revision, availability: request.input.availability, instrument_ids: request.input.instrument_ids,
      trade_count: '6', cohort_count: '6' } };
  const metadataFile = join(prepared, 'catalog-metadata.json'); const requestFile = join(directory, 'request.json');
  await writeFile(metadataFile, JSON.stringify(metadata)); await writeFile(requestFile, JSON.stringify(request));
  return { directory, prepared, source, bytes, request, research, options: { prepared: metadataFile, request: requestFile,
    output: join(directory, 'dataset.json'), id: 'cohort-cache', name: 'Controlled cohort publication', ready: true } };
}

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
