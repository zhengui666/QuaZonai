import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { access, mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { exportDataset } from '../src/dataset-export.ts';
import { loadConfig } from '../src/config.ts';
import { setup } from './support.ts';

const token = '102936224134271070189104847090829839924697394514566827387181305960175107677216';
const instrument = `condition-${token}.POLYMARKET`;
const bar = `${instrument}-1-MINUTE-LAST-EXTERNAL`;
const start = 1788220800000000000n;
const maxDbCounter = 9223372036854775807n;
const at = (seconds: number) => (start + BigInt(seconds) * 1_000_000_000n).toString();
const source = 'https://huggingface.co/datasets/TimeSeventeen/Polymarket-v2/tree/' + 'a'.repeat(40);
const execute = promisify(execFile);

// Only configuration/publication framing is tested here, not actual Parquet decoding or market evidence.
async function fixture(partition: 'DISCOVERY' | 'SEALED' = 'DISCOVERY', fullPrimary = false) {
  const full = partition === 'SEALED' || fullPrimary;
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-v2-export-'));
  const prepared = join(directory, 'prepared'); const catalog = join(prepared, 'catalog');
  await mkdir(join(catalog, 'data', 'bar'), { recursive: true });
  const parquet = join(catalog, 'data', 'bar', 'fixture.parquet');
  await writeFile(parquet, 'PAR1fixture-envelope-not-native-decodedPAR1');
  const selection = { schema_version: 1, bar_types: [bar], event_start_ns: at(0),
    event_end_ns: at(full ? 600 : 300), decision_cutoff_ns: at(900), maximum_rows: 100 };
  const request = { schema_version: 1, forecast: { schema_version: 1, selection: { ...selection, event_end_ns: at(600) },
    parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1, total_fuel: '1000000' } },
    train_end_ns: at(120), validation_end_ns: at(300) };
  const native = { dataset_revision_id: '00000000-0000-7000-8000-000000000001', selection, settlements: [] };
  const metadata = { schema_version: 1, registered_ref: 'fixture:v2:' + partition, native_snapshot_ref: 'fixture:v2-native',
    storage_version: 'original', provider_kind: 'NAUTILUS_CATALOG', data_kind: 'BAR', partition,
    event_start: '2026-09-01T00:00:00.000000Z', event_end: full ? '2026-09-01T00:10:00.000000Z' : '2026-09-01T00:05:00.000000Z',
    available_through: '2026-09-01T00:15:00.000000Z', row_count: full ? '9' : '4',
    origin: 'FIXTURE', pit_status: 'UNVERIFIED', revision_policy: 'UNKNOWN', provenance_reference: source,
    availability_provenance: 'Explicit fixture; no historical availability claim',
    universe: { instrument_definitions: [{ BinaryOption: { id: instrument, raw_symbol: token, currency: 'pUSD',
      ts_event: 'original-native-u64-not-a-research-input', ts_init: 'original-native-u64-not-a-research-input' } }] },
    quality: { schema_version: 1, native_version: 'nautilus-persistence/0.63.0', checked_at: '2026-10-14T00:00:00Z', datasets: [{ ...native,
      row_count: full ? '9' : '4', instrument_ids: [instrument], first_event_ns: at(60),
      last_event_ns: at(full ? 540 : 240), available_through_ns: at(900), last_bar_notionals: null }] } };
  const metadataPath = join(prepared, 'catalog-metadata.json');
  const receiptPath = join(prepared, 'preparation.json');
  const requestPath = join(directory, 'original-factor.json');
  const reportPath = join(directory, 'original-import-report.json');
  const report = { schema_version: 1, native_version: '0.63.0', source_reference: source, bars: 10,
    coverage: 'UNPROVEN', historical_availability: 'UNVERIFIED', registered_in_quazonai: false,
    limitations: ['Configuration fixture only, not a native result'] };
  const reportBytes = JSON.stringify(report);
  await writeFile(reportPath, reportBytes);
  const receipt = { schema: 'qz.source_preparation/1', plugin: 'hf-dataset', source_preset: 'polymarket-v2-hf',
    status: 'CATALOG_PREPARED', output: prepared, catalog_root: catalog, metadata_file: metadataPath, metadata_bytes: 0,
    catalog_registration: { root: catalog, metadata_file: metadataPath },
    identity_hints: { native_catalog_ref: metadata.registered_ref, native_storage_version: metadata.storage_version },
    source_definitions_json: JSON.stringify(metadata.universe.instrument_definitions).replaceAll('"original-native-u64-not-a-research-input"', '1788220800000000001'),
    source_reference: source, source_request: { schema: 'qz.polymarket_v2_request/1', start_date: '2026-09-01', end_date: '2026-09-02', tokens: [token] },
    source_window: { start_seconds: Number(start / 1_000_000_000n), end_seconds: Number(start / 1_000_000_000n) + 600, bar_seconds: 60 },
    source_artifacts: { import_report: { path: reportPath, bytes: Buffer.byteLength(reportBytes) },
      source_evidence: { path: join(directory, 'not-mounted-source-evidence.json'), bytes: 999 } }, native_selection: native };
  async function saveReceipt() { await writeFile(receiptPath, JSON.stringify(receipt)); }
  async function saveMetadata() {
    const bytes = JSON.stringify(metadata).replaceAll('"original-native-u64-not-a-research-input"', '1788220800000000001');
    await writeFile(metadataPath, bytes); receipt.metadata_bytes = Buffer.byteLength(bytes); await saveReceipt();
  }
  async function saveRequest(value: unknown = request) { await writeFile(requestPath, JSON.stringify(value)); }
  await saveMetadata(); await saveRequest();
  const options = { prepared: receiptPath, request: requestPath, output: join(directory, 'dataset.json'), id: 'original-v2', name: 'V2 fixture; UNVERIFIED' };
  return { directory, catalog, parquet, metadata, metadataPath, request, requestPath, report, reportPath, receipt, receiptPath,
    saveMetadata, saveReceipt, saveRequest, options, async cleanup() { await rm(directory, { recursive: true, force: true }); } };
}
type Fixture = Awaited<ReturnType<typeof fixture>>;


test('offline export preserves original requests and exact decimal strings, without qualification or raw evidence', async () => {
  const f = await fixture('DISCOVERY', true);
  try {
    const originalMetadata = await readFile(f.metadataPath);
    const originalRequest = await readFile(f.requestPath);
    const policy = { max_trials: 2, budget: '9007199254740995', cost: '0.000000000000000001' };
    const strategy = { name: 'original-owner-policy', kind: 'factor-threshold', instrumentId: instrument,
      threshold: 0.1, investedWeight: '0.250000000000000001', targetTtlNs: '9007199254740995', simulationSettings: { base_currency: 'pUSD' } };
    const policyPath = join(f.directory, 'policy.json'); const strategyPath = join(f.directory, 'strategy.json');
    await writeFile(policyPath, JSON.stringify(policy)); await writeFile(strategyPath, JSON.stringify(strategy));
    const result = await exportDataset({ ...f.options, policy: policyPath, strategy: strategyPath });
    assert.equal(result.ready, false);
    assert.equal(result.currency, 'pUSD'); assert.match(result.source, /FIXTURE.*UNVERIFIED.*UNPROVEN/);
    assert.deepEqual(result.data, { catalog: f.catalog, request: f.request });
    assert.deepEqual(result.policy, policy); assert.deepEqual(result.strategy, strategy);
    assert.deepEqual(JSON.parse(await readFile(f.options.output, 'utf8')), result);
    assert.deepEqual(await readFile(f.metadataPath), originalMetadata); assert.deepEqual(await readFile(f.requestPath), originalRequest);
    assert.equal(JSON.stringify(result.data).includes('source-evidence'), false);
    assert.equal(JSON.stringify(result.data).includes('native-u64'), false);
    assert.equal(JSON.stringify(result.data).includes('last_bar_notionals'), false);
    const configPath = join(f.directory, 'service.json');
    await writeFile(configPath, JSON.stringify({ datasets: [result], extensions: [] }));
    const loaded = await loadConfig(configPath);
    assert.deepEqual(loaded.datasets, [result]);
  } finally { await f.cleanup(); }
});

test('prefix-only research preparation cannot silently enable a full-range portfolio strategy', async () => {
  const primary = await fixture(); const sealed = await fixture('SEALED');
  try {
    const strategy = join(primary.directory, 'strategy.json');
    await writeFile(strategy, JSON.stringify({ name: 'original-owner-policy', kind: 'factor-threshold', instrumentId: instrument,
      threshold: 0.1, investedWeight: '0.25', targetTtlNs: '1000000000', simulationSettings: { base_currency: 'pUSD' } }));
    await assert.rejects(exportDataset({ ...primary.options, sealedPrepared: sealed.receiptPath, sealedRequest: sealed.requestPath, strategy }));
    await assert.rejects(readFile(primary.options.output), { code: 'ENOENT' });
  } finally { await primary.cleanup(); await sealed.cleanup(); }
});

test('explicit price-volume HELDOUT strategy retains a prefix primary and its original full sealed replay binding', async () => {
  const primary = await fixture(); const sealed = await fixture('SEALED');
  try {
    primary.receipt.source_window.end_seconds -= 300; await primary.saveReceipt();
    const prefixRequest = structuredClone(primary.request);
    Object.assign(prefixRequest.forecast.selection, { event_end_ns: at(300), decision_cutoff_ns: at(300) });
    await primary.saveRequest({ ...prefixRequest, forecast: { ...prefixRequest.forecast, parameters: { ...prefixRequest.forecast.parameters, total_fuel: null } } });
    const { total_fuel: _fuel, ...parameters } = sealed.request.forecast.parameters;
    await sealed.saveRequest({ ...sealed.request, forecast: { ...sealed.request.forecast, parameters } });
    const strategy = { name: 'original-conditional-policy', kind: 'factor-threshold', instrumentId: instrument,
      threshold: 0.1, investedWeight: '0.25', targetTtlNs: '1000000000', forecastMode: 'price-volume', evaluationPartition: 'HELDOUT',
      simulationSettings: { base_currency: 'pUSD', starting_capital: '3000' } };
    const strategyPath = join(primary.directory, 'strategy.json'); await writeFile(strategyPath, JSON.stringify(strategy));
    const originals = await Promise.all([readFile(primary.requestPath, 'utf8'), readFile(sealed.requestPath, 'utf8')]);
    const result = await exportDataset({ ...primary.options, sealedPrepared: sealed.receiptPath, sealedRequest: sealed.requestPath, strategy: strategyPath });
    assert.deepEqual(result.strategy, strategy);
    assert.deepEqual(result.data, { catalog: primary.catalog, request: JSON.parse(originals[0]!), sealed: { catalog: sealed.catalog, request: JSON.parse(originals[1]!) } });
    assert.deepEqual(await Promise.all([readFile(primary.requestPath, 'utf8'), readFile(sealed.requestPath, 'utf8')]), originals);
    assert.notEqual(result.data.catalog, (result.data.sealed as { catalog: string }).catalog);
    const config = join(primary.directory, 'service.json'); await writeFile(config, JSON.stringify({ datasets: [result], extensions: [] }));
    assert.deepEqual((await loadConfig(config)).datasets[0]!.strategy, strategy);
  } finally { await primary.cleanup(); await sealed.cleanup(); }
});

test('a wide declared primary selection cannot hide prefix-only conversion from portfolio export', async () => {
  const primary = await fixture('DISCOVERY', true); const sealed = await fixture('SEALED');
  try {
    primary.receipt.source_window.end_seconds -= 300;
    primary.metadata.row_count = '4'; primary.metadata.quality.datasets[0].row_count = '4';
    primary.metadata.quality.datasets[0].last_event_ns = at(240);
    await primary.saveMetadata();
    const strategy = join(primary.directory, 'strategy.json');
    await writeFile(strategy, JSON.stringify({ name: 'original-owner-policy', kind: 'factor-threshold', instrumentId: instrument,
      threshold: 0.1, investedWeight: '0.25', targetTtlNs: '1000000000', simulationSettings: { base_currency: 'pUSD' } }));
    await assert.rejects(exportDataset({ ...primary.options, sealedPrepared: sealed.receiptPath, sealedRequest: sealed.requestPath, strategy }), /strategy_source_window/);
    await assert.rejects(readFile(primary.options.output), { code: 'ENOENT' });
  } finally { await primary.cleanup(); await sealed.cleanup(); }
});

test('compatible heldout request uses the same clipped criteria and keeps full original suffix in its own binding', async () => {
  const primary = await fixture(); const sealed = await fixture('SEALED');
  try {
    await sealed.saveRequest({ ...sealed.request, forecast: { ...sealed.request.forecast,
      parameters: { ...sealed.request.forecast.parameters, total_fuel: null } } });
    const { total_fuel: _fuel, ...parameters } = primary.request.forecast.parameters;
    await primary.saveRequest({ ...primary.request, forecast: { ...primary.request.forecast, parameters } });
    const result = await exportDataset({ ...primary.options, sealedPrepared: sealed.receiptPath, sealedRequest: sealed.requestPath, ready: true });
    assert.equal(result.ready, true);
    assert.deepEqual(result.data.request, JSON.parse(await readFile(primary.requestPath, 'utf8')));
    assert.deepEqual(result.data.sealed, { catalog: sealed.catalog, request: JSON.parse(await readFile(sealed.requestPath, 'utf8')) });
    assert.equal(result.end, sealed.metadata.event_end);
  } finally { await primary.cleanup(); await sealed.cleanup(); }
});

test('heldout compatibility treats source token enumeration as a set without rewriting provenance', async () => {
  const primary = await fixture(); const sealed = await fixture('SEALED');
  try {
    primary.receipt.source_request.tokens = [token, '123']; sealed.receipt.source_request.tokens = ['123', token];
    await primary.saveReceipt(); await sealed.saveReceipt();
    const original = await readFile(sealed.receiptPath);
    const result = await exportDataset({ ...primary.options, sealedPrepared: sealed.receiptPath, sealedRequest: sealed.requestPath });
    assert.deepEqual(result.data.sealed, { catalog: sealed.catalog, request: sealed.request });
    assert.deepEqual(await readFile(sealed.receiptPath), original);
  } finally { await primary.cleanup(); await sealed.cleanup(); }
});

test('strategy instrument identity is exact rather than a prefix of a native instrument', async () => {
  const f = await fixture('DISCOVERY', true);
  try {
    const strategy = join(f.directory, 'strategy.json');
    await writeFile(strategy, JSON.stringify({ name: 'original-owner-policy', kind: 'factor-threshold', instrumentId: 'condition',
      threshold: 0.1, investedWeight: '0.25', targetTtlNs: '1000000000', simulationSettings: { base_currency: 'pUSD' } }));
    await assert.rejects(exportDataset({ ...f.options, strategy }), /original_strategy_mismatch/);
    await assert.rejects(readFile(f.options.output), { code: 'ENOENT' });
  } finally { await f.cleanup(); }
});

test('full declared Sealed selections cannot hide missing warmup or changed original preprocessing', async () => {
  const primary = await fixture(); const sealed = await fixture('SEALED');
  const options = { ...primary.options, sealedPrepared: sealed.receiptPath, sealedRequest: sealed.requestPath };
  try {
    const originalDefinitions = sealed.receipt.source_definitions_json;
    sealed.receipt.source_window.start_seconds += 300;
    sealed.metadata.quality.datasets[0].first_event_ns = at(360);
    await sealed.saveMetadata();
    await assert.rejects(exportDataset(options), /heldout_criteria/);
    sealed.receipt.source_window.start_seconds -= 300;
    sealed.metadata.quality.datasets[0].first_event_ns = at(60);
    await sealed.saveMetadata();
    primary.receipt.source_window.start_seconds += 60;
    primary.metadata.quality.datasets[0].first_event_ns = at(120);
    await primary.saveMetadata();
    await assert.rejects(exportDataset(options), /heldout_criteria/);
    primary.receipt.source_window.start_seconds -= 60;
    primary.metadata.quality.datasets[0].first_event_ns = at(60);
    await primary.saveMetadata();
    // These adjacent original u64 clocks round to the same JS Number; lineage must not.
    sealed.receipt.source_definitions_json = originalDefinitions.replaceAll('1788220800000000001', '1788220800000000002');
    await sealed.saveReceipt();
    await assert.rejects(exportDataset(options), /heldout_criteria/);
    sealed.receipt.source_definitions_json = originalDefinitions;
    Object.assign(sealed.metadata.universe.instrument_definitions[0].BinaryOption, { price_precision: 2, price_increment: '0.01' });
    sealed.receipt.source_definitions_json = JSON.stringify(sealed.metadata.universe.instrument_definitions).replaceAll('"original-native-u64-not-a-research-input"', '1788220800000000001');
    await sealed.saveMetadata();
    await assert.rejects(exportDataset(options), /heldout_criteria/);
    await assert.rejects(readFile(primary.options.output), { code: 'ENOENT' });
  } finally { await primary.cleanup(); await sealed.cleanup(); }
});

test('duplicate original keys, including escaped and nested names, are refused rather than silently rewritten', async () => {
  const f = await fixture();
  try {
    for (const original of [JSON.stringify(f.request).replace('"train_end_ns":', '"train_end_ns":"' + at(121) + '","train_end_ns":'),
      JSON.stringify(f.request).replace('"train_end_ns":', '"train_end_ns":"' + at(121) + '","train\\u005fend_ns":'),
      JSON.stringify(f.request).replace('"fast_period":1', '"fast_period":2,"fast_period":1')]) {
      await writeFile(f.requestPath, original);
      await assert.rejects(exportDataset(f.options), /duplicate_json_key/);
      await assert.rejects(readFile(f.options.output), { code: 'ENOENT' });
    }
  } finally { await f.cleanup(); }
});

test('incompatible optional strategy contracts fail before publication', async () => {
  const f = await fixture('DISCOVERY', true);
  try {
    const file = join(f.directory, 'strategy.json');
    const strategy = { name: 'original-owner-policy', kind: 'factor-threshold', instrumentId: instrument,
      threshold: 0.1, investedWeight: '0.25', targetTtlNs: '1000000000', simulationSettings: { base_currency: 'pUSD' } };
    for (const change of [{ simulationSettings: { base_currency: 'USD' } }, { threshold: null }, { investedWeight: '1.01' },
      { investedWeight: '0' }, { targetTtlNs: '0' }, { targetTtlNs: (maxDbCounter + 1n).toString() },
      { targetTtlNs: (maxDbCounter - BigInt(at(900)) + 1n).toString() }, { name: '' }, { simulationSettings: null },
      { forecastMode: 'unsupported' }, { evaluationPartition: 'VALIDATION' }]) {
      await writeFile(file, JSON.stringify({ ...strategy, ...change }));
      await assert.rejects(exportDataset({ ...f.options, strategy: file }));
      await assert.rejects(readFile(f.options.output), { code: 'ENOENT' });
    }
  } finally { await f.cleanup(); }
});

const invalid: [string, (f: Fixture) => Promise<void>][] = [
  ['wrong source preset', async (f) => { f.receipt.source_preset = 'other'; await f.saveReceipt(); }],
  ['foreign repository', async (f) => { f.receipt.source_reference = source.replace('TimeSeventeen', 'other'); await f.saveReceipt(); }],
  ['wrong original source report', async (f) => { const report = { ...f.report, historical_availability: 'VERIFIED' };
    const bytes = JSON.stringify(report); await writeFile(f.reportPath, bytes); f.receipt.source_artifacts.import_report.bytes = Buffer.byteLength(bytes); await f.saveReceipt(); }],
  ['missing original source report', async (f) => { await rm(f.reportPath); }],
  ['missing prepared metadata', async (f) => { await rm(f.metadataPath); }],
  ['prepared path mismatch', async (f) => { f.receipt.catalog_root = join(f.directory, 'other-catalog'); await f.saveReceipt(); }],
  ['prepared identity mismatch', async (f) => { f.receipt.identity_hints.native_storage_version = 'other'; await f.saveReceipt(); }],
  ['prepared native selection mismatch', async (f) => { f.receipt.native_selection = { ...f.receipt.native_selection,
    selection: { ...f.receipt.native_selection.selection, event_end_ns: at(301) } }; await f.saveReceipt(); }],
  ['PIT promotion', async (f) => { f.metadata.pit_status = 'VERIFIED'; await f.saveMetadata(); }],
  ['empty measured data', async (f) => { f.metadata.row_count = '0'; await f.saveMetadata(); }],
  ['non-BAR metadata', async (f) => { f.metadata.data_kind = 'TRADE'; await f.saveMetadata(); }],
  ['non-native provider', async (f) => { f.metadata.provider_kind = 'TimeSeventeen/Polymarket-v2'; await f.saveMetadata(); }],
  ['import version substituted for native quality version', async (f) => { f.metadata.quality.native_version = '0.63.0'; await f.saveMetadata(); }],
  ['wrong collateral', async (f) => { f.metadata.universe.instrument_definitions[0].BinaryOption.currency = 'USDC.e'; await f.saveMetadata(); }],
  ['wrong token mapping', async (f) => { f.receipt.source_request.tokens = ['123']; await f.saveReceipt(); }],
  ['unverified source window widened', async (f) => { f.receipt.source_window.end_seconds -= 500; await f.saveReceipt(); }],
  ['invalid source date', async (f) => { f.receipt.source_request.start_date = '2026-02-30'; await f.saveReceipt(); }],
  ['BAR outside prepared range', async (f) => { await f.saveRequest({ ...f.request, validation_end_ns: at(301) }); }],
  ['unknown BAR identity', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast,
    selection: { ...f.request.forecast.selection, bar_types: ['other-1-MINUTE-LAST-EXTERNAL'] } } }); }],
  ['row budget widened', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast,
    selection: { ...f.request.forecast.selection, maximum_rows: 101 } } }); }],
  ['numeric rather than string nanoseconds', async (f) => { await f.saveRequest({ ...f.request, train_end_ns: Number(at(120)) }); }],
  ['incorrect train-validation ordering', async (f) => { await f.saveRequest({ ...f.request, train_end_ns: at(301) }); }],
  ['non-increasing native forecast periods', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast,
    parameters: { ...f.request.forecast.parameters, fast_period: 2, slow_period: 1 } } }); }],
  ['zero explicit native fuel', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast,
    parameters: { ...f.request.forecast.parameters, total_fuel: '0' } } }); }],
  ['native cutoff before event end', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast,
    selection: { ...f.request.forecast.selection, decision_cutoff_ns: at(300) } } }); }],
  ['unknown top-level factor fields', async (f) => { await f.saveRequest({ ...f.request, heldout_values: { labels: [1, 0] } }); }],
  ['unknown forecast fields', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast, heldout_values: [1, 0] } }); }],
  ['unknown parameter fields', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast,
    parameters: { ...f.request.forecast.parameters, heldout_values: [1, 0] } } }); }],
  ['unknown selection fields', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast,
    selection: { ...f.request.forecast.selection, heldout_values: [1, 0] } } }); }],
  ['missing or invalid original parameters', async (f) => { await f.saveRequest({ ...f.request, forecast: { ...f.request.forecast, parameters: {} } }); }],
  ['unsafe numeric policy', async (f) => { const policy = join(f.directory, 'policy.json'); await writeFile(policy, '{"budget":9007199254740993}');
    Object.assign(f.options, { policy }); }],
  ['incomplete catalog file', async (f) => { await writeFile(f.parquet, 'incomplete'); }],
  ['empty catalog directory', async (f) => { await rm(f.parquet); }],
  ['symlinked catalog source', async (f) => { const original = await readFile(f.parquet); const target = join(f.directory, 'outside.parquet');
    await writeFile(target, original); await rm(f.parquet); await symlink(target, f.parquet); }],
];
for (const [name, change] of invalid) test(`export rejects ${name} without publishing a dataset`, async () => {
  const f = await fixture();
  try { await change(f); await assert.rejects(exportDataset(f.options)); await assert.rejects(readFile(f.options.output), { code: 'ENOENT' }); }
  finally { await f.cleanup(); }
});

test('existing output, output symlink and catalog-overlapping output never replace user files', async () => {
  const f = await fixture();
  try {
    await writeFile(f.options.output, 'original owner file');
    await assert.rejects(exportDataset(f.options), { code: 'EEXIST' });
    assert.equal(await readFile(f.options.output, 'utf8'), 'original owner file');
    await rm(f.options.output); await symlink(join(f.directory, 'missing'), f.options.output);
    await assert.rejects(exportDataset(f.options), /symlink/);
    await assert.rejects(exportDataset({ ...f.options, output: join(f.catalog, 'dataset.json') }), /overlaps/);
  } finally { await f.cleanup(); }
});

test('unpaired, mismatched or suffix-only heldout inputs fail without exposing final data', async () => {
  const primary = await fixture(); const sealed = await fixture('SEALED');
  try {
    await assert.rejects(exportDataset({ ...primary.options, sealedPrepared: sealed.receiptPath }), /paired_sealed/);
    const options = { ...primary.options, sealedPrepared: sealed.receiptPath, sealedRequest: sealed.requestPath };
    await sealed.saveRequest({ ...sealed.request, forecast: { ...sealed.request.forecast,
      parameters: { ...sealed.request.forecast.parameters, label_horizon_observations: 2 } } });
    await assert.rejects(exportDataset(options), /heldout_criteria/);
    await sealed.saveRequest({ ...sealed.request, train_end_ns: at(360), validation_end_ns: at(420), forecast: { ...sealed.request.forecast,
      selection: { ...sealed.request.forecast.selection, event_start_ns: at(300) } } });
    await assert.rejects(exportDataset(options), /heldout_criteria/);
    await sealed.saveRequest(); sealed.metadata.partition = 'DISCOVERY' as 'SEALED'; await sealed.saveMetadata();
    await assert.rejects(exportDataset(options), /prepared_metadata/);
    await assert.rejects(readFile(primary.options.output), { code: 'ENOENT' });
  } finally { await primary.cleanup(); await sealed.cleanup(); }
});

test('the real Python registry exports a fragment consumed by current v3 without mutating a service configuration', async () => {
  const f = await fixture(); const state = await setup();
  try {
    const checkout = fileURLToPath(new URL('../../../runtimes/data/source_plugins.py', import.meta.url));
    const installed = fileURLToPath(new URL('../../../operator/source_plugins.py', import.meta.url));
    const script = await access(checkout).then(() => checkout, () => installed);
    const result = await execute('python3', ['-B', script, 'export-dataset', 'polymarket-v2-hf', '--prepared', f.receiptPath,
      '--request', f.requestPath, '--id', f.options.id, '--name', f.options.name, '--output', f.options.output, '--ready'],
    { env: { PATH: process.env.PATH } });
    const receipt = JSON.parse(result.stdout);
    assert.equal(receipt.status, 'DATASET_CONFIG_EXPORTED'); assert.equal(receipt.registered_in_service, false); assert.equal(receipt.research_started, false);
    const configPath = join(f.directory, 'service.json');
    await writeFile(configPath, JSON.stringify({ datasets: [JSON.parse(await readFile(f.options.output, 'utf8'))], extensions: [] }));
    state.options.config.datasets = (await loadConfig(configPath)).datasets;
    await state.restart();
    const listed = await state.service.app.inject({ url: '/api/v3/datasets', headers: state.headers });
    assert.equal(listed.statusCode, 200, listed.body);
    assert.equal(listed.json()[0].id, f.options.id); assert.equal(listed.json()[0].currency, 'pUSD');
    assert.equal(listed.json()[0].ready, true); assert.match(listed.json()[0].source, /FIXTURE.*UNVERIFIED/);
    assert.equal('data' in listed.json()[0], false);
    assert.equal(state.faux.state.callCount, 0);
  } finally { await state.cleanup(); await f.cleanup(); }
});

test('offline cross-market export preserves a declared multi-market universe rather than silently choosing a base instrument', async () => {
  const f = await fixture('DISCOVERY', true);
  try {
    const otherToken = '7'; const other = `condition-${otherToken}.POLYMARKET`; const otherBar = other + '-1-MINUTE-LAST-EXTERNAL';
    const definition = structuredClone(f.metadata.universe.instrument_definitions[0]!);
    definition.BinaryOption.id = other; definition.BinaryOption.raw_symbol = otherToken;
    f.metadata.universe.instrument_definitions.push(definition);
    f.metadata.quality.datasets[0]!.instrument_ids.push(other);
    f.metadata.quality.datasets[0]!.selection.bar_types.push(otherBar);
    f.receipt.source_request.tokens.push(otherToken);
    f.receipt.source_definitions_json = JSON.stringify(f.metadata.universe.instrument_definitions).replaceAll('"original-native-u64-not-a-research-input"', '1788220800000000001');
    await f.saveMetadata(); await f.saveRequest();
    const strategy = { name: 'many-markets-one-account', kind: 'cross-market-threshold', instrumentIds: [instrument, other],
      threshold: 0, investedWeight: '0.75', targetTtlNs: '1000000000', forecastMode: 'price-volume', simulationSettings: { base_currency: 'pUSD', starting_capital: '3000' } };
    const file = join(f.directory, 'cross-market-strategy.json'); await writeFile(file, JSON.stringify(strategy));
    const result = await exportDataset({ ...f.options, strategy: file });
    assert.deepEqual(result.strategy, strategy);
    assert.ok(result.strategy?.kind === 'cross-market-threshold');
    assert.equal(result.strategy!.instrumentId, undefined);
    assert.deepEqual(result.data.request, f.request);
  } finally { await f.cleanup(); }
});
