import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFile, realpath, stat } from 'node:fs/promises';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import type { DataSourcePlugin } from '../data-sources.ts';
import type { JsonObject } from '../../../../packages/contracts/src/index.ts';

const profileFile = fileURLToPath(new URL('../../../../runtimes/data/sources/hyperliquid.json', import.meta.url));
const hubFile = fileURLToPath(new URL('../../../../runtimes/data/hub.py', import.meta.url));
export type HyperliquidOperator = (command: string, args: string[], options: { signal?: AbortSignal }) => Promise<void>;
const runOperator: HyperliquidOperator = (command, args, { signal }) => new Promise((accept, reject) => {
  const child = spawn(command, args, { shell: false, stdio: 'ignore', signal });
  let failed = false;
  child.once('error', () => { failed = true; });
  child.once('close', code => !failed && !signal?.aborted && code === 0 ? accept() : reject(new Error('hyperliquid_operator_failed')));
});
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('hyperliquid_manifest_invalid');
  return value as Record<string, unknown>;
}
async function json(file: string) { return object(JSON.parse(await readFile(file, 'utf8'))); }
function date(value: unknown): string {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value) || !Number.isFinite(Date.parse(value)) || new Date(value).toISOString().slice(0, 10) !== value) throw new Error('hyperliquid_date_invalid');
  return value;
}
function configPath(config: JsonObject, key: string, dataDir: string): string | undefined {
  const value = config[key];
  if (value === undefined) return undefined;
  if (typeof value !== 'string' || !value.trim()) throw new Error('hyperliquid_config_invalid');
  return resolve(dataDir, value);
}
async function validateArchive(manifest: Record<string, unknown>, root: string, profile: Record<string, unknown>, tradesOnly: boolean) {
  if (manifest.schema_version !== 1 || manifest.repository !== profile.repository || typeof manifest.revision !== 'string' || !/^[a-f0-9]{40}$/i.test(manifest.revision)
    || typeof manifest.license !== 'string' || !manifest.license || !Array.isArray(manifest.missing) || manifest.missing.length
    || !Array.isArray(manifest.files) || !manifest.files.length || !isDeepStrictEqual(object(manifest.profile).streams, profile.streams)) throw new Error('hyperliquid_archive_incomplete');
  const start = date(manifest.start_date); const end = date(manifest.end_date);
  if (start >= end) throw new Error('hyperliquid_date_invalid');
  const streams = new Map((profile.streams as Record<string, unknown>[]).map(stream => [stream.name, stream]));
  let tradeFiles = 0;
  const canonicalRoot = await realpath(root);
  for (const value of manifest.files) {
    const file = object(value);
    if (typeof file.path !== 'string' || !file.path || isAbsolute(file.path) || file.path.includes('\\') || file.path.split('/').some(part => part === '..' || part === '.')) throw new Error('hyperliquid_archive_file_invalid');
    if (file.stream !== null && (typeof file.stream !== 'string' || !streams.has(file.stream))) throw new Error('hyperliquid_archive_file_invalid');
    const kind = file.stream === null ? null : streams.get(file.stream)?.kind;
    if (tradesOnly && kind !== null && kind !== 'trades') throw new Error('hyperliquid_trades_only_required');
    const path = await realpath(join(root, file.path)); const local = relative(canonicalRoot, path);
    if (local === '..' || local.startsWith(`..${sep}`) || isAbsolute(local)) throw new Error('hyperliquid_archive_file_invalid');
    const info = await stat(path);
    if (!info.isFile() || !Number.isSafeInteger(file.size) || file.size !== info.size) throw new Error('hyperliquid_archive_file_invalid');
    if (kind === 'trades') tradeFiles++;
  }
  if (!tradeFiles) throw new Error('hyperliquid_trades_only_required');
  return { start, end };
}
/** Existing historical files only. This provider never opens an exchange market-data feed. */
export function hyperliquidDataPlugin(run: HyperliquidOperator = runOperator): DataSourcePlugin {
  return {
    name: 'qz.data.hyperliquid', version: '1',
    fields: [
      { key: 'acquisitionDirectory', label: '历史档案目录', type: 'text', required: true, description: '包含 market-data.json 的已有 HF 档案目录；相对服务数据目录解析。' },
      { key: 'startDate', label: '下载开始日期（UTC）', type: 'text', description: '可选 YYYY-MM-DD，与结束日期一起填写时仅下载已有历史逐笔文件；留空则完全离线。' },
      { key: 'endDate', label: '下载结束日期（不含）', type: 'text', description: '可选 YYYY-MM-DD，不含当天；保留原始全市场档案，不采集实时行情。' },
      { key: 'nativeBinary', label: '原生 market-history 程序', type: 'text', description: '可选；与定义文件一起配置时，从仅含 trades 的原始档案构建官方原生毫秒 BAR。已有全流档案可离线浏览，转换前需单独下载 trades 选择。' },
      { key: 'instrumentsFile', label: '已有原生品种定义文件', type: 'text', description: '与原生程序一起配置；仅使用已有文件，不在线采集。历史参数与研究准入仍需单独验证。' },
      { key: 'barMilliseconds', label: '聚合毫秒数', type: 'number', description: '默认 1ms，保留原始毫秒逐笔；可设 100 表示 0.1s BAR。仅从实际成交构建，不填空窗；与 VPS 网络延迟无关。' },
    ],
    async load(config, { dataDir, signal }) {
      const root = configPath(config, 'acquisitionDirectory', dataDir);
      if (!root) throw new Error('hyperliquid_config_invalid');
      const grain = config.barMilliseconds ?? 1;
      if (typeof grain !== 'number' || !Number.isSafeInteger(grain) || grain < 1 || grain > 86400000) throw new Error('hyperliquid_config_invalid');
      const binary = configPath(config, 'nativeBinary', dataDir); const instruments = configPath(config, 'instrumentsFile', dataDir);
      if (Boolean(binary) !== Boolean(instruments) || (config.startDate === undefined) !== (config.endDate === undefined)) throw new Error('hyperliquid_config_invalid');
      const profile = await json(profileFile);
      signal?.throwIfAborted();
      if (config.startDate !== undefined) {
        const start = date(config.startDate); const end = date(config.endDate);
        if (start >= end) throw new Error('hyperliquid_date_invalid');
        await run('python3', ['-I', hubFile, 'download', '--profile', profileFile, '--start-date', start, '--end-date', end, '--kind', 'trades', '--output', root], { signal });
      }
      signal?.throwIfAborted();
      const manifestFile = join(root, 'market-data.json');
      const manifest = await json(manifestFile); const window = await validateArchive(manifest, root, profile, Boolean(binary));
      if (config.startDate !== undefined && (manifest.start_date !== config.startDate || manifest.end_date !== config.endDate)) throw new Error('hyperliquid_archive_selection_mismatch');
      const data: JsonObject = { acquisition: manifestFile, barIntervalMilliseconds: grain, sourceRevision: manifest.revision as string,
        coverage: 'PUBLISHED_ARCHIVE_RECORDS_NOT_EXCHANGE_WIDE_COMPLETENESS', researchAdmission: 'REQUIRES_NATIVE_PREPARATION_AND_ORIGINAL_RESEARCH_DECLARATION' };
      if (binary && instruments) {
        const output = join(root, `native-${randomUUID()}`);
        await run(binary, ['import', '--manifest', manifestFile, '--instruments', instruments, '--bar-milliseconds', String(grain), '--native-precision', '--output', output], { signal });
        signal?.throwIfAborted();
        const reportFile = join(output, 'import-report.json'); const report = await json(reportFile);
        if (report.status !== 'IMPORTED' || typeof report.readback_records !== 'number' || !Number.isSafeInteger(report.readback_records) || report.readback_records <= 0
          || Object.keys(object(report.missing_symbols)).length || Object.keys(object(report.rejected_rows)).length || !Array.isArray(report.missing_partitions) || report.missing_partitions.length
          || !(await stat(join(output, 'catalog'))).isDirectory()) throw new Error('hyperliquid_native_import_incomplete');
        const bars = object(object(report.native_records).bars);
        if (!Object.values(bars).length || Object.values(bars).some(count => !Number.isSafeInteger(count) || count <= 0)) throw new Error('hyperliquid_native_millisecond_bars_missing');
        data.catalog = join(output, 'catalog'); data.nativeImportReport = reportFile;
      }
      return [{ id: 'hyperliquid-history', name: `Hyperliquid historical all-published-market trades · ${grain}ms`, assetClass: 'CRYPTO', currency: 'MULTI',
        source: `Hugging Face ${String(profile.repository)}`, start: `${window.start}T00:00:00.000Z`, end: `${window.end}T00:00:00.000Z`, ready: false, data }];
    },
  };
}
export default hyperliquidDataPlugin();
