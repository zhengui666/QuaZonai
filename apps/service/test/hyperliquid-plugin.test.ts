import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { hyperliquidDataPlugin } from '../src/providers/hyperliquid.ts';
import type { HyperliquidOperator } from '../src/providers/hyperliquid.ts';
import { configDefaults } from '../src/config.ts';
import { dataSources } from '../src/data-sources.ts';

async function fixture() {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-hyperliquid-history-'));
  const root = join(dataDir, 'archive');
  const profile = JSON.parse(await readFile(new URL('../../../runtimes/data/sources/hyperliquid.json', import.meta.url), 'utf8'));
  const path = 'perp_fills_all/date=2026-10-13/fills.parquet';
  await mkdir(join(root, 'perp_fills_all/date=2026-10-13'), { recursive: true });
  await writeFile(join(root, path), 'original');
  const manifest = { schema_version: 1, repository: profile.repository, revision: profile.revision, license: 'mit', profile,
    start_date: '2026-10-13', end_date: '2026-10-14', files: [{ path, size: 8, stream: 'trades' }], missing: [] as unknown[] };
  const save = () => writeFile(join(root, 'market-data.json'), JSON.stringify(manifest));
  await save();
  return { dataDir, root, manifest, save, clean: () => rm(dataDir, { recursive: true, force: true }) };
}

test('historical Hyperliquid provider is discoverable and disabled without automatic downloads', async () => {
  const f = await fixture();
  const sources = await dataSources(configDefaults({ dataDir: f.dataDir }));
  try {
    const source = sources.view().find(source => source.name === 'qz.data.hyperliquid')!;
    assert.equal(source.enabled, false); assert.equal(source.lastLoadedAt, null); assert.equal(source.fields.length, 12);
    await sources.save(source.name, { enabled: true, config: { acquisitionDirectory: 'archive' } });
    await sources.refresh(source.name);
    assert.deepEqual(sources.view().find(item => item.name === source.name)!.datasetIds, ['hyperliquid-history']);
    assert.equal(sources.view().find(item => item.name === source.name)!.error, null);
  } finally { await sources.close(); await f.clean(); }
});

test('existing all-published-market history loads offline and never claims research readiness', async () => {
  const f = await fixture();
  try {
    const plugin = hyperliquidDataPlugin(async () => { throw new Error('offline loading must not invoke an operator'); });
    const [dataset] = await plugin.load({ acquisitionDirectory: 'archive' }, { dataDir: f.dataDir });
    assert.equal(dataset!.ready, false); assert.equal(dataset!.currency, 'MULTI');
    assert.equal(dataset!.start, '2026-10-13T00:00:00.000Z');
    assert.equal(dataset!.data.barIntervalMilliseconds, 1); assert.equal(dataset!.data.catalog, undefined);
    assert.equal(dataset!.data.acquisition, join(f.root, 'market-data.json'));
    await writeFile(join(f.root, 'published-bars.parquet'), 'candles');
    f.manifest.files.push({ path: 'published-bars.parquet', size: 7, stream: 'bars' }); await f.save();
    assert.equal((await plugin.load({ acquisitionDirectory: 'archive' }, { dataDir: f.dataDir }))[0]!.ready, false, 'raw offline browsing also accepts an existing all-stream archive');
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', nativeBinary: 'operator', instrumentsFile: 'definitions.json' }, { dataDir: f.dataDir }), /trades_only/);
    f.manifest.missing.push({ stream: 'trades', date: '2026-10-13', pattern: 'spot_fills/*' }); await f.save();
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive' }, { dataDir: f.dataDir }), /archive_incomplete/);
  } finally { await f.clean(); }
});

test('history download and 100ms native conversion use fixed argv; controlled receipt is protocol evidence only', async () => {
  const f = await fixture();
  const calls: { command: string; args: string[] }[] = [];
  const runner: HyperliquidOperator = async (command, args) => {
    calls.push({ command, args });
    if (command === 'python3') return;
    const output = args[args.indexOf('--output') + 1]!;
    const grain = args[args.indexOf('--bar-milliseconds') + 1]!;
    await mkdir(join(output, 'catalog'), { recursive: true });
    await writeFile(join(output, 'import-report.json'), JSON.stringify({ status: 'IMPORTED', readback_records: 4,
      missing_symbols: {}, rejected_rows: {}, missing_partitions: [], missing_definitions: { expired: 1 },
      native_records: { trades: { 'BTC-USD-PERP.HYPERLIQUID': 2 }, bars: { 'BTC-USD-PERP.HYPERLIQUID': 2 } } }));
  };
  try {
    const [dataset] = await hyperliquidDataPlugin(runner).load({ acquisitionDirectory: 'archive', startDate: '2026-10-13', endDate: '2026-10-14',
      nativeBinary: 'market-history;literal', instrumentsFile: 'original-definitions.json', barMilliseconds: 100 }, { dataDir: f.dataDir });
    assert.equal(calls.length, 2);
    assert.equal(calls[0]!.command, 'python3'); assert.equal(calls[0]!.args[calls[0]!.args.indexOf('--kind') + 1], 'trades');
    assert.equal(calls[1]!.command, join(f.dataDir, 'market-history;literal'));
    assert.deepEqual(calls[1]!.args.slice(0, 8), ['import', '--manifest', join(f.root, 'market-data.json'), '--instruments', join(f.dataDir, 'original-definitions.json'), '--bar-milliseconds', '100', '--native-precision']);
    assert.equal(dataset!.ready, false); assert.match(String(dataset!.data.catalog), /native-[^/]+\/catalog$/);
    assert.match(String(dataset!.data.nativeImportReport), /import-report.json$/);
    const [defaultDataset] = await hyperliquidDataPlugin(runner).load({ acquisitionDirectory: 'archive', nativeBinary: 'market-history;literal', instrumentsFile: 'original-definitions.json' }, { dataDir: f.dataDir });
    assert.equal(defaultDataset!.data.barIntervalMilliseconds, 1); assert.match(defaultDataset!.name, /1ms$/);
    assert.equal(calls[2]!.args[calls[2]!.args.indexOf('--bar-milliseconds') + 1], '1');
  } finally { await f.clean(); }
});

test('history provider rejects mixed streams, changed bytes, invalid selection and unsuccessful native receipts', async () => {
  const f = await fixture(); const plugin = hyperliquidDataPlugin(async () => undefined);
  try {
    await assert.rejects(plugin.load({}, { dataDir: f.dataDir }), /config_invalid/);
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', startDate: '2026-10-13' }, { dataDir: f.dataDir }), /config_invalid/);
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', startDate: '2026-02-31', endDate: '2026-10-14' }, { dataDir: f.dataDir }), /date_invalid/);
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', nativeBinary: 'operator' }, { dataDir: f.dataDir }), /config_invalid/);
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', barMilliseconds: 0 }, { dataDir: f.dataDir }), /config_invalid/);
    f.manifest.files[0]!.stream = 'bars'; await f.save();
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', nativeBinary: 'operator', instrumentsFile: 'definitions.json' }, { dataDir: f.dataDir }), /trades_only/);
    f.manifest.files[0]!.stream = 'trades'; f.manifest.files[0]!.size = 9; await f.save();
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive' }, { dataDir: f.dataDir }), /archive_file_invalid/);
    f.manifest.files[0]!.size = 8; await f.save();
    const invalidReceipt = hyperliquidDataPlugin(async (_command, args) => {
      const output = args[args.indexOf('--output') + 1]!;
      await mkdir(join(output, 'catalog'), { recursive: true });
      await writeFile(join(output, 'import-report.json'), JSON.stringify({ status: 'INCOMPLETE', readback_records: 1 }));
    });
    await assert.rejects(invalidReceipt.load({ acquisitionDirectory: 'archive', nativeBinary: 'operator', instrumentsFile: 'definitions.json' }, { dataDir: f.dataDir }), /native_import_incomplete/);
  } finally { await f.clean(); }
});

test('disabling or closing a source aborts an active cooperative historical operator', async () => {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-history-cancel-'));
  const module = join(dataDir, 'history.mjs');
  await writeFile(module, `export default {name:'test.history',fields:[],async load(config,{signal}) {
    await new Promise((resolve,reject)=> { if(signal.aborted) reject(new Error('aborted')); else signal.addEventListener('abort',()=>reject(new Error('aborted')),{once:true}); }); return [];
  }};`);
  const sources = await dataSources(configDefaults({ dataDir, extensions: [{ name: 'test.history', module, kind: 'data', enabled: true }] }));
  try {
    let run = sources.refresh('test.history'); let rejected = assert.rejects(run, /data_source_refresh_cancelled/);
    await new Promise(resolve => setImmediate(resolve));
    await sources.save('test.history', { enabled: false, config: {} }); await rejected;
    assert.equal(sources.view().find(item => item.name === 'test.history')!.enabled, false);
    await sources.toggle('test.history', true);
    run = sources.refresh('test.history'); rejected = assert.rejects(run, /data_source_refresh_cancelled/);
    await new Promise(resolve => setImmediate(resolve)); await sources.close(); await rejected;
  } finally { await sources.close(); await rm(dataDir, { recursive: true, force: true }); }
});

test('explicit TradeTick preparation selects spot/perpetual paths and uses no BAR conversion flags', async () => {
  const f = await fixture();
  const calls: { command: string; args: string[]; memory?: number }[] = [];
  let missingNativeFile = false;
  const runner: HyperliquidOperator = async (command, args, options) => {
    calls.push({ command, args, memory: options.maxMemoryBytes });
    if (command === 'python3') return;
    const output = args[args.indexOf('--output') + 1]!;
    await mkdir(join(output, 'catalog'), { recursive: true });
    if (!missingNativeFile) await writeFile(join(output, 'catalog/native-test.parquet'), 'records');
    await writeFile(join(output, 'import-report.json'), JSON.stringify({ status: 'IMPORTED', source_revision: f.manifest.revision,
      modality: 'TRADE_TICK', selected_path_prefixes: ['perp_fills_all/', 'spot_fills/'], duplicate_trades: 0,
      event_start_ns: '1791850200000000000', event_end_ns: '1791850500000000000', native_files: [{ path: 'native-test.parquet', size: 7 }],
      readback_records: 2, missing_symbols: {}, missing_definitions: {}, rejected_rows: {}, missing_partitions: [],
      availability: 'EVENT_TIME_PROXY_UNVERIFIED', source_coverage: { trades: { selected_rows: 2 } },
      native_records: { trades: { 'BTC-USD-PERP.HYPERLIQUID': 2 } } }).replace('"selected_rows":2', '"selected_rows":2,"first_event_ns":1791849600000000001'));
  };
  try {
    f.manifest.missing.push({ stream: 'trades', pattern: 'outcome_fills/all/date=2026-10-13/fills.parquet' });
    f.manifest.files.push({ path: 'outcome_fills/all/date=2026-10-13/fills.parquet', size: 99, stream: 'trades' });
    await f.save();
    const [dataset] = await hyperliquidDataPlugin(runner).load({ acquisitionDirectory: 'archive', inputModality: 'TRADE_TICK',
      startDate: '2026-10-13', endDate: '2026-10-14', nativeBinary: 'market-history', instrumentsFile: 'definitions.json', nativeOutputDirectory: 'prepared-ticks',
      eventStartSeconds: 1791850200, eventEndSeconds: 1791850500 }, { dataDir: f.dataDir });
    assert.equal(calls.length, 2);
    for (const call of calls) {
      assert.deepEqual(call.args.flatMap((arg, index) => arg === '--path-prefix' ? [call.args[index + 1]] : []), ['perp_fills_all/', 'spot_fills/']);
      assert.equal(call.args.includes('--bar-milliseconds'), false); assert.equal(call.args.includes('--bar-seconds'), false);
    }
    assert.equal(calls[1]!.args.includes('--trade-ticks'), true); assert.equal(calls[1]!.args.includes('--resume'), true);
    assert.equal(calls[1]!.args[calls[1]!.args.indexOf('--start-seconds') + 1], '1791850200'); assert.equal(calls[1]!.args[calls[1]!.args.indexOf('--end-seconds') + 1], '1791850500');
    assert.equal(calls[1]!.memory, 2 * 1024 * 1024 * 1024);
    assert.equal(dataset!.id, 'hyperliquid-trade-ticks'); assert.equal(dataset!.nativeModality, 'TRADE_TICK');
    assert.equal(dataset!.ready, false); assert.deepEqual(dataset!.preparation, { archiveLoaded: true, nativePrepared: true, scientificQualification: 'UNVERIFIED' });
    assert.equal(dataset!.data.barIntervalMilliseconds, undefined); assert.equal(dataset!.data.nativeInput, undefined, 'source conversion does not invent research partitions or sampling policy');
    assert.equal(((dataset!.inputCoverage as { sourceCoverage: { trades: { first_event_ns: string } } }).sourceCoverage.trades.first_event_ns), '1791849600000000001', 'u64 clocks remain exact through the Web/service metadata path');
    assert.equal(dataset!.inputCoverage!.event_start_ns, '1791850200000000000'); assert.equal(dataset!.inputCoverage!.event_end_ns, '1791850500000000000');
    assert.equal(dataset!.data.catalog, join(f.dataDir, 'prepared-ticks/catalog'));
    await rm(join(f.dataDir, 'prepared-ticks/catalog/native-test.parquet')); missingNativeFile = true;
    await assert.rejects(hyperliquidDataPlugin(runner).load({ acquisitionDirectory: 'archive', inputModality: 'TRADE_TICK',
      nativeBinary: 'market-history', instrumentsFile: 'definitions.json', nativeOutputDirectory: 'prepared-ticks', eventStartSeconds: 1791850200, eventEndSeconds: 1791850500 }, { dataDir: f.dataDir }), /inventory_mismatch/);
    f.manifest.missing.push({ stream: 'trades', pattern: 'spot_fills/date=2026-10-13/fills.parquet' }); await f.save();
    await assert.rejects(hyperliquidDataPlugin(runner).load({ acquisitionDirectory: 'archive', inputModality: 'TRADE_TICK' }, { dataDir: f.dataDir }), /archive_incomplete/);
  } finally { await f.clean(); }
});

test('TradeTick mode rejects implicit BAR adaptation, unsupported modality and mismatched native evidence', async () => {
  const f = await fixture();
  try {
    const plugin = hyperliquidDataPlugin(async (_command, args) => {
      const output = args[args.indexOf('--output') + 1]!;
      await mkdir(join(output, 'catalog'), { recursive: true });
      await writeFile(join(output, 'import-report.json'), JSON.stringify({ status: 'IMPORTED', readback_records: 1,
        source_revision: f.manifest.revision, modality: 'TRADE_TICK', selected_path_prefixes: ['perp_fills_all/', 'spot_fills/'],
        duplicate_trades: 1, missing_symbols: {}, missing_definitions: {}, rejected_rows: {}, missing_partitions: [], native_records: { trades: { BTC: 1 } } }));
    });
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', inputModality: 'TRADE_TICK', barMilliseconds: 100 }, { dataDir: f.dataDir }), /tick_bar_conflict/);
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', inputModality: 'TICKS' }, { dataDir: f.dataDir }), /modality_unsupported/);
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', inputModality: 'TRADE_TICK', maxTradeRows: 1_000_001 }, { dataDir: f.dataDir }), /config_invalid/);
    await assert.rejects(plugin.load({ acquisitionDirectory: 'archive', inputModality: 'TRADE_TICK', nativeBinary: 'market-history', instrumentsFile: 'definitions.json' }, { dataDir: f.dataDir }), /tick_receipt_mismatch/);
    const [raw] = await plugin.load({ acquisitionDirectory: 'archive', inputModality: 'TRADE_TICK' }, { dataDir: f.dataDir });
    assert.deepEqual(raw!.preparation, { archiveLoaded: true, nativePrepared: false, scientificQualification: 'UNVERIFIED' });
  } finally { await f.clean(); }
});
