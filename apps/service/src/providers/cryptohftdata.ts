import { readFile, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import type { DataSourcePlugin } from '../data-sources.ts';

const dayMilliseconds = 86_400_000;
const streamDirectories: Record<string, string> = {
  'spot orderbook': 'hyperliquid_spot',
  'perpetual orderbook': 'hyperliquid_futures',
  'perpetual mark_price': 'hyperliquid_futures',
};
function date(value: unknown): number {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) throw new Error('cryptohftdata_date_invalid');
  const parsed = Date.parse(`${value}T00:00:00Z`);
  if (!Number.isFinite(parsed) || new Date(parsed).toISOString().slice(0, 10) !== value) throw new Error('cryptohftdata_date_invalid');
  return parsed;
}

/** Load the downloaded archive descriptor without scanning millions of source files. */
export const cryptohftdataDataPlugin: DataSourcePlugin = {
  name: 'qz.data.cryptohftdata', version: '1',
  fields: [{ key: 'archiveDirectory', label: 'CryptoHFTData 历史档案目录', type: 'text', required: true,
    description: '包含 dataset.json、hyperliquid_spot 和 hyperliquid_futures 的已下载目录；相对服务数据目录解析。' }],
  async load(config, { dataDir, signal }) {
    if (typeof config.archiveDirectory !== 'string' || !config.archiveDirectory.trim()) throw new Error('cryptohftdata_config_invalid');
    const root = resolve(dataDir, config.archiveDirectory);
    const manifestFile = join(root, 'dataset.json');
    signal?.throwIfAborted();
    const manifest = JSON.parse(await readFile(manifestFile, 'utf8'));
    if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest) || manifest.status !== 'COMPLETE'
      || !Number.isSafeInteger(manifest.bytes) || manifest.bytes <= 0
      || !Number.isSafeInteger(manifest.objects) || manifest.objects <= 0
      || !Array.isArray(manifest.datasets) || !manifest.datasets.length
      || manifest.datasets.some((stream: unknown) => typeof stream !== 'string' || !Object.hasOwn(streamDirectories, stream))
      || new Set(manifest.datasets).size !== manifest.datasets.length) throw new Error('cryptohftdata_manifest_invalid');
    const start = date(manifest.start_date_utc); const end = date(manifest.end_date_utc);
    if (start > end || manifest.days !== (end - start) / dayMilliseconds + 1) throw new Error('cryptohftdata_date_range_invalid');
    const streams = manifest.datasets as string[];
    for (const directory of new Set(streams.map(stream => streamDirectories[stream]!))) {
      for (const path of new Set([directory, join(directory, manifest.start_date_utc), join(directory, manifest.end_date_utc)])) {
        if (!(await stat(join(root, path))).isDirectory()) throw new Error('cryptohftdata_archive_directory_invalid');
      }
    }
    signal?.throwIfAborted();
    const preparation = { archiveLoaded: true, nativePrepared: false, scientificQualification: 'UNVERIFIED' as const };
    const coverage = { startDateUtc: manifest.start_date_utc as string, endDateUtcInclusive: manifest.end_date_utc as string,
      days: manifest.days as number, objects: manifest.objects as number, bytes: manifest.bytes as number, streams };
    return [{ id: `cryptohftdata-hyperliquid-${manifest.start_date_utc}-${manifest.end_date_utc}`,
      name: `CryptoHFTData Hyperliquid · ${streams.join(', ')}`, assetClass: 'CRYPTO', currency: 'MULTI',
      source: 'CryptoHFTData Hyperliquid published historical Parquet files', start: new Date(start).toISOString(),
      end: new Date(end + dayMilliseconds).toISOString(), ready: false, preparation, inputCoverage: coverage,
      data: { archiveDirectory: root, acquisition: manifestFile, preparation, ...coverage,
        coverage: 'PUBLISHED_ARCHIVE_RECORDS_NOT_EXCHANGE_WIDE_COMPLETENESS' } }];
  },
};
export default cryptohftdataDataPlugin;
