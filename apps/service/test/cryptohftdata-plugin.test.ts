import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import cryptohftdata from '../src/providers/cryptohftdata.ts';
import { configDefaults, existingDataset } from '../src/config.ts';
import { dataSources } from '../src/data-sources.ts';

async function archive() {
  const dataDir = await mkdtemp(join(tmpdir(), 'qz-cryptohftdata-'));
  const root = join(dataDir, 'archive');
  const manifest = { status: 'COMPLETE', start_date_utc: '2026-02-12', end_date_utc: '2026-10-13', days: 244,
    objects: 3_368_701, bytes: 279_118_890_859, datasets: ['spot orderbook', 'perpetual orderbook', 'perpetual mark_price'] };
  for (const market of ['hyperliquid_spot', 'hyperliquid_futures']) {
    for (const date of [manifest.start_date_utc, manifest.end_date_utc]) await mkdir(join(root, market, date), { recursive: true });
  }
  const save = () => writeFile(join(root, 'dataset.json'), JSON.stringify(manifest));
  await save();
  return { dataDir, root, manifest, save, clean: () => rm(dataDir, { recursive: true, force: true }) };
}

test('CryptoHFTData replaces the old Hyperliquid provider and retains its historical dataset lookup', async () => {
  const f = await archive();
  const oldDataset = { id: 'hyperliquid-history', name: 'Original HF dataset', assetClass: 'CRYPTO', currency: 'MULTI',
    source: 'original', start: null, end: null, ready: false, data: { acquisition: '/original/market-data.json' } };
  await writeFile(join(f.dataDir, 'data-sources.json'), JSON.stringify({ 'qz.data.hyperliquid': {
    config: { acquisitionDirectory: '/original' }, enabled: true, datasets: [oldDataset], lastLoadedAt: null, error: null,
  } }));
  const config = configDefaults({ dataDir: f.dataDir });
  const sources = await dataSources(config);
  try {
    assert.equal(sources.has('qz.data.hyperliquid'), false);
    const source = sources.view().find(value => value.name === 'qz.data.cryptohftdata')!;
    assert.equal(source.enabled, false); assert.equal(source.lastLoadedAt, null);
    assert.equal(config.datasets.length, 0);
    assert.deepEqual(existingDataset(config, oldDataset.id), oldDataset);
    await sources.save(source.name, { enabled: true, config: { archiveDirectory: 'archive' } });
    await sources.refresh(source.name);
    assert.deepEqual(sources.view().find(value => value.name === source.name)!.datasetIds,
      ['cryptohftdata-hyperliquid-2026-02-12-2026-10-13']);
    assert.equal(config.datasets[0]!.ready, false);
    assert.deepEqual(existingDataset(config, oldDataset.id), oldDataset);
    const stored = JSON.parse(await readFile(join(f.dataDir, 'data-sources.json'), 'utf8'));
    assert.deepEqual(stored['qz.data.hyperliquid'].datasets, [oldDataset]);
  } finally { await sources.close(); await f.clean(); }
});

test('archive loading uses manifest dates and counts without claiming BAR or TradeTick preparation', async () => {
  const f = await archive();
  try {
    const original = await readFile(join(f.root, 'dataset.json'), 'utf8');
    const [dataset] = await cryptohftdata.load({ archiveDirectory: f.root }, { dataDir: f.dataDir });
    assert.equal(dataset!.start, '2026-02-12T00:00:00.000Z');
    assert.equal(dataset!.end, '2026-10-14T00:00:00.000Z');
    assert.equal(dataset!.nativeModality, undefined); assert.equal(dataset!.ready, false);
    assert.deepEqual(dataset!.preparation, { archiveLoaded: true, nativePrepared: false, scientificQualification: 'UNVERIFIED' });
    assert.equal(dataset!.data.archiveDirectory, f.root);
    assert.equal(dataset!.data.acquisition, join(f.root, 'dataset.json'));
    assert.deepEqual(dataset!.data.streams, f.manifest.datasets);
    assert.equal(dataset!.inputCoverage!.objects, 3_368_701);
    assert.equal(dataset!.inputCoverage!.bytes, 279_118_890_859);
    assert.equal(dataset!.data.catalog, undefined);
    assert.equal(await readFile(join(f.root, 'dataset.json'), 'utf8'), original);
  } finally { await f.clean(); }
});

test('incomplete or malformed descriptors and missing archive directories are rejected', async () => {
  const f = await archive();
  const load = () => cryptohftdata.load({ archiveDirectory: 'archive' }, { dataDir: f.dataDir });
  try {
    await assert.rejects(cryptohftdata.load({}, { dataDir: f.dataDir }), /config_invalid/);
    const original = structuredClone(f.manifest);
    for (const changed of [{ status: 'DOWNLOADING' }, { objects: 0 }, { bytes: Number.MAX_SAFE_INTEGER + 1 },
      { start_date_utc: '2026-02-31' }, { start_date_utc: '2026-10-14' }, { days: 1 },
      { datasets: [] }, { datasets: ['trades'] }, { datasets: ['spot orderbook', 'spot orderbook'] }]) {
      Object.assign(f.manifest, original, changed); await f.save();
      await assert.rejects(load(), /cryptohftdata_/);
    }
    Object.assign(f.manifest, original); await f.save();
    await rm(join(f.root, 'hyperliquid_spot'), { recursive: true });
    await assert.rejects(load(), /ENOENT|archive_directory_invalid/);
  } finally { await f.clean(); }
});

test('a cancelled archive refresh stops before reading the descriptor', async () => {
  const controller = new AbortController(); controller.abort();
  await assert.rejects(cryptohftdata.load({ archiveDirectory: '/missing' }, { dataDir: '/missing', signal: controller.signal }),
    { name: 'AbortError' });
});
