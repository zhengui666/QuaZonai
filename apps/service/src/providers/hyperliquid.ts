import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFile, readdir, realpath, stat } from 'node:fs/promises';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import type { DataSourcePlugin } from '../data-sources.ts';
import type { JsonObject } from '../../../../packages/contracts/src/index.ts';
import { parseOriginalJson } from '../dataset-json.ts';

const profileFile = fileURLToPath(new URL('../../../../runtimes/data/sources/hyperliquid.json', import.meta.url));
const hubFile = fileURLToPath(new URL('../../../../runtimes/data/hub.py', import.meta.url));
export type HyperliquidOperator = (command: string, args: string[], options: { signal?: AbortSignal; maxMemoryBytes?: number }) => Promise<void>;
const runOperator: HyperliquidOperator = (command, args, { signal, maxMemoryBytes }) => new Promise((accept, reject) => {
  if (maxMemoryBytes && process.platform !== 'linux') return reject(new Error('hyperliquid_native_memory_limit_unsupported'));
  const child = spawn(maxMemoryBytes ? 'prlimit' : command, maxMemoryBytes ? [`--as=${maxMemoryBytes}`, '--', command, ...args] : args, { shell: false, stdio: ['ignore', 'ignore', 'pipe'], signal });
  let failed = false;
  let stderr = '';
  child.stderr?.setEncoding('utf8');
  child.stderr?.on('data', (chunk: string) => { stderr = (stderr + chunk).slice(-8192); });
  child.once('error', () => { failed = true; });
  child.once('close', code => !failed && !signal?.aborted && code === 0 ? accept() : reject(new Error(`hyperliquid_operator_failed${stderr.trim() ? `: ${stderr.trim().split(/\r?\n/).at(-1)}` : ''}`)));
});
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('hyperliquid_manifest_invalid');
  return value as Record<string, unknown>;
}
async function json(file: string) {
  // Operator reports use u64 JSON clocks. Preserve their decimal tokens before JS number parsing.
  const text = (await readFile(file, 'utf8')).replace(/("(?:event_start_ns|event_end_ns|first_event_ns|last_event_ns)"\s*:\s*)(\d+)/g, '$1"$2"');
  return object(parseOriginalJson(text));
}
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
async function validateNativeInventory(root: string, expected: unknown) {
  if (!Array.isArray(expected) || !expected.length) throw new Error('hyperliquid_native_inventory_required');
  const files: { path: string; size: number }[] = [];
  async function visit(directory: string) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await visit(path);
      else {
        if (!entry.isFile()) throw new Error('hyperliquid_native_inventory_invalid');
        const name = relative(root, path).split(sep).join('/');
        if (name !== 'import-provenance.json') files.push({ path: name, size: (await stat(path)).size });
      }
    }
  }
  await visit(root);
  files.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  if (!isDeepStrictEqual(files, expected)) throw new Error('hyperliquid_native_inventory_mismatch');
}
async function validateArchive(manifest: Record<string, unknown>, root: string, profile: Record<string, unknown>, tradesOnly: boolean, prefixes: string[]) {
  if (manifest.schema_version !== 1 || manifest.repository !== profile.repository || typeof manifest.revision !== 'string' || !/^[a-f0-9]{40}$/i.test(manifest.revision)
    || typeof manifest.license !== 'string' || !manifest.license || !Array.isArray(manifest.missing)
    || !Array.isArray(manifest.files) || !manifest.files.length || !isDeepStrictEqual(object(manifest.profile).streams, profile.streams)) throw new Error('hyperliquid_archive_incomplete');
  if (manifest.missing.some(value => {
    const missing = object(value);
    return !prefixes.length || typeof missing.pattern !== 'string' || prefixes.some(prefix => String(missing.pattern).startsWith(prefix));
  })) throw new Error('hyperliquid_archive_incomplete');
  if (Array.isArray(manifest.selected_path_prefixes) && prefixes.some(prefix => !(manifest.selected_path_prefixes as unknown[]).some(selected => typeof selected === 'string' && prefix.startsWith(selected)))) throw new Error('hyperliquid_archive_selection_mismatch');
  const start = date(manifest.start_date); const end = date(manifest.end_date);
  if (start >= end) throw new Error('hyperliquid_date_invalid');
  const streams = new Map((profile.streams as Record<string, unknown>[]).map(stream => [stream.name, stream]));
  let tradeFiles = 0;
  const canonicalRoot = await realpath(root);
  for (const value of manifest.files) {
    const file = object(value);
    if (typeof file.path !== 'string' || !file.path || isAbsolute(file.path) || file.path.includes('\\') || file.path.split('/').some(part => part === '..' || part === '.')) throw new Error('hyperliquid_archive_file_invalid');
    if (prefixes.length && !prefixes.some(prefix => String(file.path).startsWith(prefix))) continue;
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
      { key: 'nativeBinary', label: '原生 market-history 程序', type: 'text', description: '可选；与定义文件一起配置时，按显式类型准备原生 TradeTick 或毫秒 BAR。研究分区和科学资格另行声明。' },
      { key: 'instrumentsFile', label: '已有原生品种定义文件', type: 'text', description: '与原生程序一起配置；仅使用已有文件，不在线采集。历史参数与研究准入仍需单独验证。' },
      { key: 'barMilliseconds', label: '聚合毫秒数', type: 'number', description: '默认 1ms，保留原始毫秒逐笔；可设 100 表示 0.1s BAR。仅从实际成交构建，不填空窗；与 VPS 网络延迟无关。' },
      { key: 'inputModality', label: '历史研究输入类型', type: 'text', choices: [{ value: 'BAR', label: 'BAR（聚合 K 线，兼容默认）' }, { value: 'TRADE_TICK', label: 'TradeTick（原生逐笔）' }], description: '留空兼容 BAR。原生逐笔只选择已有永续和现货成交，不构建 K 线，也不要求 outcome 文件。' },
      { key: 'nativeOutputDirectory', label: '原生准备目录', type: 'text', description: '逐笔模式可指定不可变转换单元目录；完整单元重读核验后复用，中断单元需使用新目录。原始档案不改写。' },
      { key: 'maxTradeRows', label: '逐笔单元成交行上限', type: 'number', description: '逐笔模式默认且最多 1000000；超限失败，按日期缩小转换单元，不采样或静默截断。' },
      { key: 'maxOutputBytes', label: '逐笔单元输出字节上限', type: 'number', description: '逐笔模式默认且最多 10GiB，包含目录和准备报告。原生进程另以 2GiB 地址空间上限运行。' },
      { key: 'eventStartSeconds', label: '逐笔单元开始秒（UTC）', type: 'number', description: '可选 Unix 秒；与结束秒一起指定原始档案内的半开事件区间，便于有界转换。' },
      { key: 'eventEndSeconds', label: '逐笔单元结束秒（不含）', type: 'number', description: '可选 Unix 秒；不会移动原始成交时钟。中断单元使用新输出目录重做，完整单元可核验复用。' },
    ],
    async load(config, { dataDir, signal }) {
      const root = configPath(config, 'acquisitionDirectory', dataDir);
      if (!root) throw new Error('hyperliquid_config_invalid');
      const modality = config.inputModality ?? 'BAR';
      if (modality !== 'BAR' && modality !== 'TRADE_TICK') throw new Error('hyperliquid_input_modality_unsupported');
      const tick = modality === 'TRADE_TICK';
      if (tick && config.barMilliseconds !== undefined) throw new Error('hyperliquid_tick_bar_conflict');
      const grain = config.barMilliseconds ?? 1;
      if (typeof grain !== 'number' || !Number.isSafeInteger(grain) || grain < 1 || grain > 86400000) throw new Error('hyperliquid_config_invalid');
      const binary = configPath(config, 'nativeBinary', dataDir); const instruments = configPath(config, 'instrumentsFile', dataDir);
      if (Boolean(binary) !== Boolean(instruments) || (config.startDate === undefined) !== (config.endDate === undefined)) throw new Error('hyperliquid_config_invalid');
      const profile = await json(profileFile);
      const prefixes = tick ? profile.research_trade_path_prefixes : [];
      if (!Array.isArray(prefixes) || prefixes.some(prefix => typeof prefix !== 'string' || !prefix.endsWith('/')) || tick && !prefixes.length) throw new Error('hyperliquid_profile_scope_invalid');
      const pathArgs = (prefixes as string[]).flatMap(prefix => ['--path-prefix', prefix]);
      const maxRows = config.maxTradeRows ?? 1_000_000; const maxBytes = config.maxOutputBytes ?? 10 * 1024 * 1024 * 1024;
      const eventStart = config.eventStartSeconds; const eventEnd = config.eventEndSeconds;
      if (typeof maxRows !== 'number' || !Number.isSafeInteger(maxRows) || maxRows < 1 || maxRows > 1_000_000
        || typeof maxBytes !== 'number' || !Number.isSafeInteger(maxBytes) || maxBytes < 1 || maxBytes > 10 * 1024 * 1024 * 1024
        || (eventStart === undefined) !== (eventEnd === undefined)
        || eventStart !== undefined && (typeof eventStart !== 'number' || !Number.isSafeInteger(eventStart) || eventStart < 0 || typeof eventEnd !== 'number' || !Number.isSafeInteger(eventEnd) || eventEnd <= eventStart)
        || !tick && [config.nativeOutputDirectory, config.maxTradeRows, config.maxOutputBytes, eventStart, eventEnd].some(value => value !== undefined)) throw new Error('hyperliquid_config_invalid');
      signal?.throwIfAborted();
      if (config.startDate !== undefined) {
        const start = date(config.startDate); const end = date(config.endDate);
        if (start >= end) throw new Error('hyperliquid_date_invalid');
        await run('python3', ['-I', hubFile, 'download', '--profile', profileFile, '--start-date', start, '--end-date', end, '--kind', 'trades', ...pathArgs, '--output', root], { signal });
      }
      signal?.throwIfAborted();
      const manifestFile = join(root, 'market-data.json');
      const manifest = await json(manifestFile); const window = await validateArchive(manifest, root, profile, Boolean(binary), prefixes as string[]);
      if (typeof eventStart === 'number' && typeof eventEnd === 'number' && (eventStart * 1000 < Date.parse(`${window.start}T00:00:00Z`) || eventEnd * 1000 > Date.parse(`${window.end}T00:00:00Z`))) throw new Error('hyperliquid_event_range_outside_archive');
      const selectedStartNs = (BigInt(typeof eventStart === 'number' ? eventStart : Date.parse(`${window.start}T00:00:00Z`) / 1000) * 1_000_000_000n).toString();
      const selectedEndNs = (BigInt(typeof eventEnd === 'number' ? eventEnd : Date.parse(`${window.end}T00:00:00Z`) / 1000) * 1_000_000_000n).toString();
      if (config.startDate !== undefined && (manifest.start_date !== config.startDate || manifest.end_date !== config.endDate)) throw new Error('hyperliquid_archive_selection_mismatch');
      const preparation = { archiveLoaded: true, nativePrepared: false, scientificQualification: 'UNVERIFIED' as const };
      const data: JsonObject = { acquisition: manifestFile, inputModality: modality, sourceRevision: manifest.revision as string, preparation,
        selectedPathPrefixes: prefixes as string[],
        coverage: 'PUBLISHED_ARCHIVE_RECORDS_NOT_EXCHANGE_WIDE_COMPLETENESS', researchAdmission: 'REQUIRES_NATIVE_PREPARATION_AND_ORIGINAL_RESEARCH_DECLARATION' };
      if (!tick) data.barIntervalMilliseconds = grain;
      if (binary && instruments) {
        const output = tick ? configPath(config, 'nativeOutputDirectory', dataDir) ?? join(root, `native-${randomUUID()}`) : join(root, `native-${randomUUID()}`);
        const conversion = tick ? ['--trade-ticks', ...pathArgs, '--max-trade-rows', String(maxRows), '--max-output-bytes', String(maxBytes),
          ...(eventStart === undefined ? [] : ['--start-seconds', String(eventStart), '--end-seconds', String(eventEnd)]), ...(config.nativeOutputDirectory ? ['--resume'] : [])] : ['--bar-milliseconds', String(grain)];
        await run(binary, ['import', '--manifest', manifestFile, '--instruments', instruments, ...conversion, '--native-precision', '--output', output], { signal, ...(tick ? { maxMemoryBytes: 2 * 1024 * 1024 * 1024 } : {}) });
        signal?.throwIfAborted();
        const reportFile = join(output, 'import-report.json'); const report = await json(reportFile);
        if (report.status !== 'IMPORTED' || typeof report.readback_records !== 'number' || !Number.isSafeInteger(report.readback_records) || report.readback_records <= 0
          || Object.keys(object(report.missing_symbols)).length || Object.keys(object(report.rejected_rows)).length || !Array.isArray(report.missing_partitions) || report.missing_partitions.length
          || !(await stat(join(output, 'catalog'))).isDirectory()) throw new Error('hyperliquid_native_import_incomplete');
        const records = object(report.native_records); const rows = object(records[tick ? 'trades' : 'bars']);
        if (!Object.values(rows).length || Object.values(rows).some(count => typeof count !== 'number' || !Number.isSafeInteger(count) || count <= 0)) throw new Error(tick ? 'hyperliquid_native_trades_missing' : 'hyperliquid_native_millisecond_bars_missing');
        if (tick && Object.keys(object(report.missing_definitions)).length) throw new Error('hyperliquid_native_tick_definitions_missing');
        if (tick && (report.source_revision !== manifest.revision || report.modality !== 'TRADE_TICK' || report.duplicate_trades !== 0
          || report.event_start_ns !== selectedStartNs || report.event_end_ns !== selectedEndNs
          || !isDeepStrictEqual(report.selected_path_prefixes, [...prefixes].sort()) || Object.keys(records).some(key => key !== 'trades')
          || Object.values(rows).reduce<number>((sum, value) => sum + Number(value), 0) !== report.readback_records)) throw new Error('hyperliquid_native_tick_receipt_mismatch');
        if (tick) await validateNativeInventory(join(output, 'catalog'), report.native_files);
        data.catalog = join(output, 'catalog'); data.nativeImportReport = reportFile;
        data.inputCoverage = { sourceRevision: manifest.revision as string, selectedPathPrefixes: prefixes as string[], sourceCoverage: object(report.source_coverage ?? {}) as JsonObject,
          event_start_ns: String(report.event_start_ns ?? selectedStartNs), event_end_ns: String(report.event_end_ns ?? selectedEndNs), native_records: records as JsonObject,
          availability: String(report.availability ?? report.historical_availability ?? 'UNVERIFIED') };
        preparation.nativePrepared = true;
        if (typeof report.event_start_ns === 'string' && typeof report.event_end_ns === 'string') data.nativeEventRange = { startNs: report.event_start_ns, endNs: report.event_end_ns };
      }
      return [{ id: tick ? 'hyperliquid-trade-ticks' : 'hyperliquid-history', name: tick ? 'Hyperliquid historical spot/perpetual TradeTick' : `Hyperliquid historical all-published-market trades · ${grain}ms`, assetClass: 'CRYPTO', currency: 'MULTI',
        source: `Hugging Face ${String(profile.repository)}`, start: `${window.start}T00:00:00.000Z`, end: `${window.end}T00:00:00.000Z`, ready: false, nativeModality: modality, preparation, inputCoverage: data.inputCoverage as JsonObject | undefined, data }];
    },
  };
}
export default hyperliquidDataPlugin();
