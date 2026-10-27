import { constants } from 'node:fs';
import { link, lstat, mkdtemp, open, readFile, readdir, realpath, rm, unlink } from 'node:fs/promises';
import { dirname, join, relative, resolve, sep } from 'node:path';
import type { DatasetConfig } from './config.ts';
import type { DatasetExportOptions } from './dataset-export.ts';
import { canonical } from '../../research/src/documents.ts';
import { validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
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
    || !['NAUTILUS_HIGH_PRECISION_16', 'NAUTILUS_STANDARD_9'].includes(String(metadata.catalog_format))) throw new Error('tick_prepared_metadata_invalid');
  const originalRequest = parseOriginalJson(requestBytes.toString()); validateNumbers(originalRequest);
  const full = validateNativeTickFactorRequest(originalRequest);
  if (canonical(full) !== canonical(metadata.request!)) throw new Error('tick_prepared_request_mismatch');
  const research = validateNativeTickFactorRequest(metadata.research_request);
  const clipped = { ...full, input: { ...full.input, event_end_ns: full.validation_end_ns } };
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
  await catalog(primary); await catalog(independent);
  const coverage = object(metadata.coverage, 'tick_input_coverage');
  if (coverage.source_revision !== full.input.source_revision || coverage.availability !== full.input.availability
    || !Array.isArray(coverage.instrument_ids) || coverage.instrument_ids.some(id => typeof id !== 'string')
    || canonical([...coverage.instrument_ids].sort()) !== canonical([...full.input.instrument_ids].sort())
    || typeof coverage.trade_count !== 'string' || !/^[1-9]\d*$/.test(coverage.trade_count)
    || BigInt(coverage.trade_count) > BigInt(full.input.maximum_rows)) throw new Error('tick_prepared_coverage_mismatch');
  const result: DatasetConfig = {
    id: text(options.id, 'dataset_id'), name: text(options.name, 'dataset_name'), assetClass: options.assetClass ?? 'CRYPTO', currency: 'MULTI',
    source: `Native trade archive ${full.input.source_revision} | PIT UNVERIFIED; market completeness UNPROVEN`,
    start: new Date(Number(BigInt(full.input.event_start_ns) / 1_000_000n)).toISOString(),
    end: new Date(Number(BigInt(full.input.event_end_ns) / 1_000_000n)).toISOString(), ready: options.ready === true,
    nativeModality: 'TRADE_TICK', preparation: { archiveLoaded: true, nativePrepared: true, scientificQualification: 'UNVERIFIED' },
    inputCoverage: { ...coverage, event_start_ns: full.input.event_start_ns, event_end_ns: full.input.event_end_ns, catalog_format: metadata.catalog_format! },
    data: { catalog: primary, nativeInput: { modality: 'TRADE_TICK', request: research },
      sealed: { catalog: independent, nativeInput: { modality: 'TRADE_TICK', request: full } } },
  };
  if (options.policy) { result.policy = object(parseOriginalJson(await readFile(options.policy, 'utf8')), 'tick_policy'); validateNumbers(result.policy); }
  result.revisionManifest = datasetRevisionManifest(output, result.id);
  const revision = await prepareDatasetRevision(result, { kind: 'NATIVE_PREPARED_TICK_DATASET_EXPORT', metadata,
    limitations: ['UNQUALIFIED_EXPLORATORY', 'EVENT_TIME_PROXY_UNVERIFIED', 'PUBLICATION_NOT_HISTORICAL_AVAILABILITY'] });
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
