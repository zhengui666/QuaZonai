import { constants, createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { link, lstat, mkdtemp, open, readFile, readdir, realpath, rm, unlink } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import type { DatasetConfig } from './config.ts';
import type { DatasetExportOptions } from './dataset-export.ts';
import { canonical } from '../../research/src/documents.ts';
import { clipTickRegimeEvaluation, validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import type { NativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
import { datasetRevisionManifest, prepareDatasetRevision } from './dataset-revision-publish.ts';
import { object, text } from './facts.ts';
import { parseOriginalJson } from './dataset-json.ts';
import { validateNumbers } from '../../research/src/input.ts';
import type { JsonObject } from '../../../packages/contracts/src/index.ts';

/** Consume the existing native preparation receipt and publication conventions.
 * This framing check is not a second decoder or a source/PIT qualification. */
export async function exportTickDataset(options: DatasetExportOptions): Promise<DatasetConfig> {
  if (options.strategy || options.sealedPrepared || options.sealedRequest) throw new Error('tick_prepared_partitions_required');
  const metadataPath = resolve(options.prepared); const requestPath = resolve(options.request);
  const metadataBytes = await readFile(metadataPath); const requestBytes = await readFile(requestPath);
  const metadata = object(parseOriginalJson(metadataBytes.toString()), 'tick_prepared_metadata');
  validateNumbers(metadata);
  if (metadata.schema_version !== 1 || metadata.modality !== 'TRADE_TICK' || metadata.operational_state !== 'NATIVE_PREPARED'
    || metadata.scientific_qualification !== 'UNQUALIFIED_EXPLORATORY'
    || !['NAUTILUS_HIGH_PRECISION_16', 'NAUTILUS_STANDARD_9', 'QZ_EVENT_TIME_COHORT_CACHE_V1', 'QZ_EVENT_TIME_COHORT_FLOW_CACHE_V1'].includes(String(metadata.catalog_format))) throw new Error('tick_prepared_metadata_invalid');
  const originalRequest = parseOriginalJson(requestBytes.toString()); validateNumbers(originalRequest);
  const full = validateNativeTickFactorRequest(originalRequest);
  const flow = full.input.representation === 'EVENT_TIME_COHORT_FLOW_CACHE_V1';
  const cache = flow || full.input.representation === 'EVENT_TIME_COHORT_CACHE_V1';
  if ((cache ? metadata.catalog_format !== (flow ? 'QZ_EVENT_TIME_COHORT_FLOW_CACHE_V1' : 'QZ_EVENT_TIME_COHORT_CACHE_V1')
    : !['NAUTILUS_HIGH_PRECISION_16', 'NAUTILUS_STANDARD_9'].includes(String(metadata.catalog_format)))
    || (cache ? metadata.representation !== full.input.representation : metadata.representation !== undefined)) throw new Error('tick_prepared_representation_mismatch');
  if (canonical(full) !== canonical(metadata.request!)) throw new Error('tick_prepared_request_mismatch');
  const research = validateNativeTickFactorRequest(metadata.research_request);
  const clipped = clipTickRegimeEvaluation(full, full.validation_end_ns);
  if (canonical(research) !== canonical(clipped) || full.validation_end_ns === full.input.event_end_ns) throw new Error('tick_prepared_partitions_required');
  if (metadata.research_catalog !== 'research/catalog' || metadata.independent_catalog !== 'independent/catalog') throw new Error('tick_prepared_path_mismatch');
  const root = dirname(metadataPath); const primary = join(root, 'research/catalog'); const independent = join(root, 'independent/catalog');
  const observed = new Map<string, string>();
  async function observe(path: string) {
    const state = await lstat(path, { bigint: true });
    const identity = [state.dev, state.ino, state.size, state.mtimeNs, state.ctimeNs].join(':');
    if (observed.has(path) && observed.get(path) !== identity) throw new Error('dataset_export_input_changed');
    observed.set(path, identity);
  }
  const output = resolve(options.output);
  const location = relative(root, output);
  if (location === '' || location !== '..' && !location.startsWith(`..${sep}`)) throw new Error('dataset_output_overlaps_catalog');
  async function catalog(directory: string): Promise<void> {
    let trades = 0;
    async function visit(path: string): Promise<void> {
      await observe(path);
      const state = await lstat(path);
      if (state.isSymbolicLink() || await realpath(path) !== path) throw new Error('dataset_symlink_not_allowed');
      if (state.isDirectory()) { for (const name of await readdir(path)) await visit(join(path, name)); }
      else if (state.isFile() && path.endsWith('.parquet')) {
        if (state.size < 12) throw new Error('incomplete_prepared_catalog');
        const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
        try {
          const head = Buffer.alloc(4); const tail = Buffer.alloc(4);
          await file.read(head, 0, 4, 0); await file.read(tail, 0, 4, state.size - 4);
          if (head.toString() !== 'PAR1' || tail.toString() !== 'PAR1') throw new Error('incomplete_prepared_catalog');
        } finally { await file.close(); }
        if (relative(directory, path).split(sep).includes('trades')) trades++;
      } else if (!state.isFile()) throw new Error('dataset_regular_input_required');
      await observe(path);
    }
    await visit(directory);
    if (!trades) throw new Error('tick_prepared_trades_required');
    await readFile(join(directory, 'import-provenance.json'));
  }
  function cacheCounter(value: unknown): bigint {
    if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('tick_cache_counter_invalid');
    return BigInt(value);
  }
  type CacheFile = { inode: string; bytes: bigint; begin: bigint; end: bigint };
  async function cacheCatalog(directory: string, request: NativeTickFactorRequest): Promise<{ trades: bigint; cohorts: bigint; source: string; assets: Map<string, CacheFile>; contextFiles: Map<string, string> }> {
    const manifestPath = join(directory, 'cohort-cache.json'); await observe(manifestPath);
    const manifest = object(parseOriginalJson(await readFile(manifestPath, 'utf8')), 'tick_cache_manifest'); validateNumbers(manifest);
    if (manifest.schema_version !== 1 || manifest.representation !== request.input.representation
      || manifest.source_revision !== request.input.source_revision || manifest.availability !== request.input.availability
      || manifest.event_start_ns !== request.input.event_start_ns || manifest.event_end_ns !== request.input.event_end_ns
      || !Array.isArray(manifest.files)) throw new Error('tick_cache_manifest_mismatch');
    const source = object(manifest.source_report, 'tick_cache_source_report');
    if (source.status !== 'IMPORTED' || (source.source_revision ?? source.revision) !== manifest.source_revision) throw new Error('tick_cache_source_report_mismatch');
    const frameBytes = request.input.representation === 'EVENT_TIME_COHORT_FLOW_CACHE_V1' ? 40n : 32n;
    const contextFiles = new Map<string, string>();
    const files = new Set<string>([manifestPath]); const instruments = new Set<string>(); const assets = new Map<string, CacheFile>(); let trades = 0n; let cohorts = 0n;
    for (const raw of manifest.files) {
      const entry = object(raw, 'tick_cache_file'); const name = text(entry.path, 'tick_cache_path'); const id = text(entry.instrument_id, 'tick_cache_instrument');
      if (isAbsolute(name) || name.includes('\\') || name.split('/').some(part => !part || part === '.' || part === '..')
        || instruments.has(id) || !request.input.instrument_ids.includes(id)) throw new Error('tick_cache_file_invalid');
      const path = join(directory, name); if (files.has(path)) throw new Error('tick_cache_file_invalid');
      files.add(path); instruments.add(id); await observe(path);
      const state = await lstat(path, { bigint: true });
      const bytes = cacheCounter(entry.bytes); const begin = cacheCounter(entry.byte_start); const end = cacheCounter(entry.byte_end);
      const count = cacheCounter(entry.cohort_count); const rows = cacheCounter(entry.trade_count);
      if (!state.isFile() || state.isSymbolicLink() || await realpath(path) !== path || state.size !== bytes
        || bytes % frameBytes !== 0n || begin % frameBytes !== 0n || end % frameBytes !== 0n || begin > end || end > bytes
        || (end - begin) / frameBytes !== count || rows < count || (count === 0n) !== (rows === 0n)
        || !Number.isSafeInteger(Number(end))) throw new Error('tick_cache_frame_range_invalid');
      assets.set(id, { inode: `${state.dev}:${state.ino}`, bytes, begin, end });
      const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
      try {
        async function clock(position: bigint): Promise<bigint> {
          const value = Buffer.alloc(8); const read = await file.read(value, 0, 8, Number(position));
          if (read.bytesRead !== 8) throw new Error('tick_cache_boundary_mismatch');
          return value.readBigUInt64LE();
        }
        if (count > 0n) {
          const first = cacheCounter(entry.first_event_ns); const last = cacheCounter(entry.last_event_ns);
          if (first > last || first < BigInt(request.input.event_start_ns) || last >= BigInt(request.input.event_end_ns)) throw new Error('tick_cache_event_range_invalid');
          if (await clock(begin) !== first || await clock(end - frameBytes) !== last) throw new Error('tick_cache_boundary_mismatch');
        } else if (entry.first_event_ns !== null || entry.last_event_ns !== null) throw new Error('tick_cache_event_range_invalid');
        if (begin > 0n && await clock(begin - frameBytes) >= BigInt(request.input.event_start_ns)
          || end < bytes && await clock(end) < BigInt(request.input.event_end_ns)) throw new Error('tick_cache_boundary_incomplete');
      } finally { await file.close(); }
      trades += rows; cohorts += count; await observe(path);
    }
    if (instruments.size !== request.input.instrument_ids.length || trades !== cacheCounter(manifest.trade_count)
      || cohorts !== cacheCounter(manifest.cohort_count)) throw new Error('tick_cache_coverage_mismatch');
    if (metadata.economic_sources !== undefined) {
      const extras = metadata.economic_sources;
      if (!Array.isArray(extras)) throw new Error('tick_economic_source_manifest_invalid');
      const names = new Set(['funding-rates.jsonl', 'funding-rates.manifest.json']);
      if (extras.some(extra => object(extra, 'tick_economic_source').path === 'spot-valuation.jsonl')) names.add('spot-valuation.jsonl');
      const contextSizes = new Map<string, bigint>();
      if (extras.some(extra => object(extra, 'tick_economic_source').path === 'context-prices.manifest.json')) {
        const path = join(directory, 'context-prices.manifest.json'); await observe(path);
        const context = object(parseOriginalJson(await readFile(path, 'utf8')), 'tick_context_manifest'); validateNumbers(context);
        const ids = context.instrument_ids;
        if (context.schema_version !== 1 || context.format !== 'EVENT_TIME_CONTEXT_PRICE_CSV_V1' || context.clock !== 'EVENT_TIME_PROXY_UNVERIFIED'
          || typeof context.source_revision !== 'string' || !context.source_revision.trim()
          || context.research_source_revision !== request.input.source_revision
          || cacheCounter(context.start_ns) > BigInt(request.input.event_start_ns) || cacheCounter(context.end_ns) < BigInt(request.input.event_end_ns)
          || !Array.isArray(ids) || ids.length === 0 || ids.some((id, index) => typeof id !== 'string' || !id || !request.input.instrument_ids.includes(id) || index > 0 && String(ids[index - 1]) >= id)
          || !Array.isArray(context.files) || context.files.length !== ids.length) throw new Error('tick_context_source_coverage_mismatch');
        names.add('context-prices.manifest.json');
        const seenIds = new Set<string>(); let rows = 0n;
        for (const [index, raw] of context.files.entries()) {
          const file = object(raw, 'tick_context_file'); const id = text(file.instrument_id, 'tick_context_instrument'); const name = text(file.path, 'tick_context_path');
          if (id !== ids[index] || seenIds.has(id) || name !== `context-prices-${String(index).padStart(3, '0')}.csv` || names.has(name)) throw new Error('tick_context_file_invalid');
          const count = cacheCounter(file.rows); const bytes = cacheCounter(file.bytes);
          if (bytes === 0n || (count === 0n ? file.first_event_ns !== null || file.last_event_ns !== null
            : cacheCounter(file.first_event_ns) > cacheCounter(file.last_event_ns) || cacheCounter(file.first_event_ns) < cacheCounter(context.start_ns)
              || cacheCounter(file.last_event_ns) >= cacheCounter(context.end_ns))) throw new Error('tick_context_file_coverage_invalid');
          rows += count; names.add(name); seenIds.add(id); contextSizes.set(name, bytes);
        }
        if (rows !== cacheCounter(context.rows)) throw new Error('tick_context_row_count_mismatch');
      }
      for (const extra of extras) {
        const row = object(extra, 'tick_economic_source');
        if (typeof row.path !== 'string' || !names.delete(row.path)) throw new Error('tick_economic_source_manifest_invalid');
        const path = join(directory, row.path); await observe(path);
        const state = await lstat(path, { bigint: true });
        if (!state.isFile() || state.isSymbolicLink() || state.size !== cacheCounter(row.path === 'spot-valuation.jsonl' && directory === primary ? row.research_bytes ?? row.bytes : row.bytes)
          || contextSizes.has(row.path) && state.size !== contextSizes.get(row.path)) throw new Error('tick_economic_source_size_mismatch');
        if (row.path === 'context-prices.manifest.json' || contextSizes.has(row.path)) contextFiles.set(row.path, `${state.dev}:${state.ino}:${state.size}`);
        files.add(path);
      }
      if (names.size) throw new Error('tick_economic_source_manifest_invalid');
      const funding = object(parseOriginalJson(await readFile(join(directory, 'funding-rates.manifest.json'), 'utf8')), 'funding_manifest');
      const fundingInstruments = Array.isArray(funding.instruments) ? funding.instruments : [];
      if (funding.source_revision !== request.input.source_revision || cacheCounter(funding.start_ns) > BigInt(request.input.event_start_ns)
        || cacheCounter(funding.end_ns) < BigInt(request.input.event_end_ns) || !Array.isArray(funding.instruments)
        || funding.instruments.some(id => typeof id !== 'string' || !id) || new Set(funding.instruments).size !== funding.instruments.length
        || typeof funding.rows !== 'number' || !Number.isSafeInteger(funding.rows) || funding.rows < 0
        || !Array.isArray(funding.missing_instruments) || funding.missing_instruments.some(id => typeof id !== 'string' || !fundingInstruments.includes(id))) throw new Error('tick_economic_source_coverage_mismatch');
    }
    async function visit(path: string): Promise<void> {
      await observe(path); const state = await lstat(path);
      if (state.isSymbolicLink() || await realpath(path) !== path) throw new Error('dataset_symlink_not_allowed');
      if (state.isDirectory()) { for (const name of await readdir(path)) await visit(join(path, name)); }
      else if (!state.isFile() || !files.delete(path)) throw new Error('tick_cache_file_unmanifested');
      await observe(path);
    }
    await visit(directory); if (files.size) throw new Error('tick_cache_file_missing');
    return { trades, cohorts, source: canonical(source), assets, contextFiles };
  }
  const cacheCoverage = cache ? await cacheCatalog(primary, research) : null;
  const fullCacheCoverage = cache ? await cacheCatalog(independent, full) : null;
  if (cacheCoverage && fullCacheCoverage) {
    if (cacheCoverage.source !== fullCacheCoverage.source) throw new Error('tick_cache_source_view_mismatch');
    if (cacheCoverage.contextFiles.size !== fullCacheCoverage.contextFiles.size
      || [...cacheCoverage.contextFiles].some(([name, identity]) => fullCacheCoverage.contextFiles.get(name) !== identity)) throw new Error('tick_context_source_view_mismatch');
    const sources = Array.isArray(metadata.economic_sources) ? metadata.economic_sources : [];
    if (sources.some(extra => object(extra, 'tick_economic_source').path === 'spot-valuation.jsonl')) {
      const instruments = new Set(full.input.instrument_ids);
      const primaryLines = createInterface({ input: createReadStream(join(primary, 'spot-valuation.jsonl')), crlfDelay: Infinity });
      const fullLines = createInterface({ input: createReadStream(join(independent, 'spot-valuation.jsonl')), crlfDelay: Infinity });
      const expected = primaryLines[Symbol.asyncIterator]();
      try {
        for await (const line of fullLines) {
          const quote = object(parseOriginalJson(line), 'tick_spot_valuation_row');
          if (typeof quote.instrument_id !== 'string' || !instruments.has(quote.instrument_id)) throw new Error('tick_spot_valuation_instrument_invalid');
          const received = cacheCounter(quote.received_ns);
          if (received >= BigInt(full.input.event_end_ns)) throw new Error('tick_spot_valuation_window_invalid');
          if (received < BigInt(research.input.event_end_ns)) {
            const row = await expected.next();
            if (row.done || row.value !== line) throw new Error('tick_spot_valuation_view_mismatch');
          }
        }
        if (!(await expected.next()).done) throw new Error('tick_spot_valuation_view_mismatch');
      } finally { primaryLines.close(); fullLines.close(); }
    }
    for (const [id, view] of cacheCoverage.assets) {
      const original = fullCacheCoverage.assets.get(id)!;
      if (view.inode !== original.inode || view.bytes !== original.bytes || view.begin !== original.begin || view.end > original.end) throw new Error('tick_cache_source_view_mismatch');
    }
  }
  if (!cache) { await catalog(primary); await catalog(independent); }
  const coverage = object(metadata.coverage, 'tick_input_coverage');
  if (coverage.source_revision !== full.input.source_revision || coverage.availability !== full.input.availability
    || !Array.isArray(coverage.instrument_ids) || coverage.instrument_ids.some(id => typeof id !== 'string')
    || canonical([...coverage.instrument_ids].sort()) !== canonical([...full.input.instrument_ids].sort())
    || typeof coverage.trade_count !== 'string' || !/^[1-9]\d*$/.test(coverage.trade_count)
    || BigInt(coverage.trade_count) > BigInt(cache ? full.input.maximum_source_rows! : full.input.maximum_rows!)) throw new Error('tick_prepared_coverage_mismatch');
  if (fullCacheCoverage && (fullCacheCoverage.trades !== BigInt(coverage.trade_count)
    || fullCacheCoverage.cohorts !== cacheCounter(coverage.cohort_count)
    || cacheCoverage!.trades > fullCacheCoverage.trades)) throw new Error('tick_cache_coverage_mismatch');
  const result: DatasetConfig = {
    id: text(options.id, 'dataset_id'), name: text(options.name, 'dataset_name'), assetClass: options.assetClass ?? 'CRYPTO', currency: 'MULTI',
    source: `${flow ? 'Derived event-time signed-flow cohort cache' : cache ? 'Derived event-time cohort cache' : 'Native trade archive'} ${full.input.source_revision} | PIT UNVERIFIED; market completeness UNPROVEN`,
    start: new Date(Number(BigInt(full.input.event_start_ns) / 1_000_000n)).toISOString(),
    end: new Date(Number(BigInt(full.input.event_end_ns) / 1_000_000n)).toISOString(), ready: options.ready === true,
    nativeModality: 'TRADE_TICK', preparation: { archiveLoaded: true, nativePrepared: true, scientificQualification: 'UNVERIFIED' },
    inputCoverage: { ...coverage, event_start_ns: full.input.event_start_ns, event_end_ns: full.input.event_end_ns, catalog_format: metadata.catalog_format!, ...(cache ? { representation: full.input.representation! } : {}) },
    data: { catalog: primary, nativeInput: { modality: 'TRADE_TICK', request: research },
      sealed: { catalog: independent, nativeInput: { modality: 'TRADE_TICK', request: full } } },
  };
  if (options.policy) { result.policy = object(parseOriginalJson(await readFile(options.policy, 'utf8')), 'tick_policy'); validateNumbers(result.policy); }
  result.revisionManifest = datasetRevisionManifest(output, result.id);
  const revision = await prepareDatasetRevision(result, { kind: 'NATIVE_PREPARED_TICK_DATASET_EXPORT', metadata,
    limitations: ['UNQUALIFIED_EXPLORATORY', 'EVENT_TIME_PROXY_UNVERIFIED', 'PUBLICATION_NOT_HISTORICAL_AVAILABILITY'] }, { shareDuplicateFiles: cache });
  result.data = revision.data;
  for (const path of observed.keys()) await observe(path);
  const temporary = await mkdtemp(join(dirname(output), '.qz-tick-dataset-'));
  try {
    const file = join(temporary, 'dataset.json'); const contents = Buffer.from(JSON.stringify(result, null, 2) + '\n');
    const handle = await open(file, 'wx', 0o600);
    try { await handle.writeFile(contents); await handle.sync(); } finally { await handle.close(); }
    if (!(await readFile(metadataPath)).equals(metadataBytes) || !(await readFile(requestPath)).equals(requestBytes)) throw new Error('dataset_export_input_changed');
    for (const path of observed.keys()) await observe(path);
    await link(file, output);
    try { await revision.publish(); }
    catch (error) {
      const own = await lstat(file); const current = await lstat(output).catch(() => null);
      if (current?.dev === own.dev && current.ino === own.ino && (await readFile(output)).equals(contents)) await unlink(output);
      throw error;
    }
  } finally { await rm(temporary, { recursive: true, force: true }); }
  return result;
}
