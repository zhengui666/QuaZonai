import { constants } from 'node:fs';
import { createHash } from 'node:crypto';
import { link, lstat, mkdtemp, open, readFile, readdir, rm, unlink } from 'node:fs/promises';
import { dirname, isAbsolute, join, parse, relative, resolve, sep } from 'node:path';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import type { DatasetConfig, ThresholdPolicy } from './config.ts';
import type { JsonObject } from '../../../packages/contracts/src/index.ts';
import { object, text, decimal, decimalUnits, counter } from './facts.ts';
import { validateNumbers as safeNumbers } from '../../research/src/input.ts';
import { canonical } from '../../research/src/documents.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { thresholdPolicy } from '../../research/src/strategy.ts';
import { datasetRevisionManifest, prepareDatasetRevision } from './dataset-revision-publish.ts';
import { exportTickDataset } from './tick-dataset-export.ts';
import { parseOriginalJson } from './dataset-json.ts';

export interface DatasetExportOptions {
  prepared: string; request: string; output: string; id: string; name: string;
  assetClass?: string; ready?: boolean;
  sealedPrepared?: string; sealedRequest?: string; policy?: string; strategy?: string;
}

const preset = 'polymarket-v2-hf';
const maxDbCounter = 9223372036854775807n;
const sourcePattern = /^https:\/\/huggingface\.co\/datasets\/TimeSeventeen\/Polymarket-v2\/tree\/[a-f0-9]{40}$/;

function ns(value: unknown): bigint {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('native_nanosecond_string_required');
  return BigInt(value);
}
function uint(value: unknown): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 1 || value > 4294967295) throw new Error('native_positive_u32_required');
  return value;
}
function dayNs(value: unknown): bigint {
  const day = text(value, 'source_date');
  const milliseconds = Date.parse(day + 'T00:00:00Z');
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day) || !Number.isFinite(milliseconds)
    || new Date(milliseconds).toISOString().slice(0, 10) !== day) throw new Error('invalid_source_date');
  return BigInt(milliseconds) * 1_000_000n;
}
function nativeKeys(value: JsonObject, allowed: string[]): void {
  if (Object.keys(value).some((key) => !allowed.includes(key))) throw new Error('unknown_native_request_field');
}
function selection(value: unknown, catalog = false): JsonObject {
  const result = object(value, 'native_selection');
  if (!catalog) nativeKeys(result, ['schema_version', 'bar_types', 'event_start_ns', 'event_end_ns', 'decision_cutoff_ns', 'maximum_rows']);
  if (result.schema_version !== 1 || !catalog && (!Array.isArray(result.bar_types) || !result.bar_types.length
    || result.bar_types.some((id) => typeof id !== 'string' || !id)
    || new Set(result.bar_types).size !== result.bar_types.length)
    || ns(result.event_start_ns) >= ns(result.event_end_ns)
    || ns(result.event_end_ns) > ns(result.decision_cutoff_ns)) throw new Error('native_selection_required');
  ns(result.decision_cutoff_ns); uint(result.maximum_rows);
  return result;
}
export function factorRequest(value: JsonObject, catalog = false): JsonObject {
  safeNumbers(value);
  const forecast = catalog ? value : object(value.forecast, 'forecast');
  const parameters = object(forecast.parameters, 'parameters');
  nativeKeys(value, catalog ? ['schema_version', 'event_start_ns', 'event_end_ns', 'decision_cutoff_ns', 'maximum_rows', 'parameters', 'train_end_ns', 'validation_end_ns'] : ['schema_version', 'forecast', 'train_end_ns', 'validation_end_ns']);
  if (!catalog) nativeKeys(forecast, ['schema_version', 'selection', 'parameters']);
  nativeKeys(parameters, ['schema_version', 'fast_period', 'slow_period', 'label_horizon_observations', 'total_fuel']);
  if (value.schema_version !== 1 || forecast.schema_version !== 1 || parameters.schema_version !== 1) throw new Error('native_schema_version_required');
  selection(catalog ? value : forecast.selection, catalog);
  uint(parameters.fast_period); uint(parameters.slow_period); uint(parameters.label_horizon_observations);
  if (Number(parameters.slow_period) <= Number(parameters.fast_period)
    || (parameters.total_fuel !== null && parameters.total_fuel !== undefined && ns(parameters.total_fuel) === 0n)) throw new Error('native_forecast_parameters_required');
  // Reuse the exact current research boundary and optional-fuel normalization.
  iterationRequest(catalog ? { catalog_scan_request: value } : { request: value });
  return value;
}
function contained(wanted: JsonObject, prepared: JsonObject): void {
  const bars = prepared.bar_types as string[];
  if ((wanted.bar_types as string[]).some((id) => !bars.includes(id))
    || ns(wanted.event_start_ns) < ns(prepared.event_start_ns)
    || ns(wanted.event_end_ns) > ns(prepared.event_end_ns)
    || ns(wanted.decision_cutoff_ns) > ns(prepared.decision_cutoff_ns)
    || uint(wanted.maximum_rows) > uint(prepared.maximum_rows)) throw new Error('request_outside_prepared_selection');
}
async function noSymlinks(path: string, checked = new Set<string>()): Promise<void> {
  let current = parse(path).root;
  for (const part of relative(current, path).split(sep).filter(Boolean)) {
    current = join(current, part);
    if (checked.has(current)) continue;
    try {
      if ((await lstat(current)).isSymbolicLink()) throw new Error('dataset_symlink_not_allowed');
      checked.add(current);
    }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  }
}
function within(path: string, root: string): boolean {
  return path === root || path.startsWith(root + sep);
}

/** Offline configuration and immutable revision handoff; existing registered
 * monitors may consume it. No native decoding or source qualification is implied. */
export async function exportDataset(options: DatasetExportOptions): Promise<DatasetConfig> {
  const declaration = object(parseOriginalJson(await readFile(resolve(options.prepared), 'utf8')), 'prepared_declaration');
  if (declaration.modality === 'TRADE_TICK') return exportTickDataset(options);
  const observed = new Map<string, string>();
  const checkedAncestors = new Set<string>();
  async function observe(path: string): Promise<void> {
    await noSymlinks(path, checkedAncestors);
    const state = await lstat(path, { bigint: true });
    if (!state.isFile() && !state.isDirectory()) throw new Error('dataset_regular_input_required');
    const identity = [state.dev, state.ino, state.size, state.mtimeNs, state.ctimeNs].join(':');
    if (observed.has(path) && observed.get(path) !== identity) throw new Error('dataset_export_input_changed');
    observed.set(path, identity);
  }
  async function jsonFile(path: string): Promise<{ value: JsonObject; bytes: number; sha256: string }> {
    path = resolve(path);
    await observe(path);
    const bytes = await readFile(path);
    await observe(path);
    return { value: object(parseOriginalJson(bytes.toString('utf8')), 'original_json'), bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
  }
  async function catalog(path: string): Promise<void> {
    let parquet = 0;
    async function walk(directory: string): Promise<void> {
      await observe(directory);
      for (const entry of await readdir(directory, { withFileTypes: true })) {
        const file = join(directory, entry.name);
        await observe(file);
        if (entry.isDirectory()) await walk(file);
        else if (entry.isFile() && entry.name.endsWith('.parquet')) {
          const state = await lstat(file);
          if (state.size < 12) throw new Error('incomplete_prepared_catalog');
          const handle = await open(file, constants.O_RDONLY | constants.O_NOFOLLOW);
          try {
            const head = Buffer.alloc(4); const tail = Buffer.alloc(4);
            await handle.read(head, 0, 4, 0); await handle.read(tail, 0, 4, state.size - 4);
            if (head.toString() !== 'PAR1' || tail.toString() !== 'PAR1') throw new Error('incomplete_prepared_catalog');
          } finally { await handle.close(); }
          parquet++;
        }
      }
    }
    await walk(path);
    if (!parquet) throw new Error('empty_prepared_catalog');
  }
  async function prepared(path: string, requestPath: string, heldout: boolean) {
    const receipt = (await jsonFile(path)).value;
    if (receipt.schema !== 'qz.source_preparation/1' || receipt.status !== 'CATALOG_PREPARED'
      || receipt.plugin !== 'hf-dataset' || receipt.source_preset !== preset
      || !sourcePattern.test(text(receipt.source_reference, 'source_reference'))) throw new Error('polymarket_v2_preparation_required');
    const root = text(receipt.output, 'prepared_output');
    const catalogPath = text(receipt.catalog_root, 'catalog_root');
    const metadataPath = text(receipt.metadata_file, 'metadata_file');
    const registration = object(receipt.catalog_registration, 'catalog_registration');
    if (!isAbsolute(root) || root !== resolve(root) || catalogPath !== join(root, 'catalog')
      || metadataPath !== join(root, 'catalog-metadata.json')
      || registration.root !== catalogPath || registration.metadata_file !== metadataPath) throw new Error('prepared_path_mismatch');
    const { value: metadata, bytes, sha256: metadataSha256 } = await jsonFile(metadataPath);
    const hints = object(receipt.identity_hints, 'identity_hints');
    const quality = object(metadata.quality, 'native_quality');
    if (bytes !== receipt.metadata_bytes || metadata.schema_version !== 1 || metadata.provider_kind !== 'NAUTILUS_CATALOG' || metadata.data_kind !== 'BAR'
      || metadata.pit_status !== 'UNVERIFIED' || ns(metadata.row_count) < 1n
      || metadata.registered_ref !== hints.native_catalog_ref || metadata.storage_version !== hints.native_storage_version
      || quality.schema_version !== 1 || quality.native_version !== 'nautilus-persistence/0.63.0'
      || !Array.isArray(quality.datasets) || quality.datasets.length !== 1
      || (heldout ? metadata.partition !== 'SEALED' : metadata.partition === 'SEALED')) throw new Error('prepared_metadata_mismatch');
    const measured = object(quality.datasets[0], 'measured_dataset');
    const native = object(receipt.native_selection, 'native_dataset_selection');
    const measuredSelection = selection(measured.selection);
    if (native.dataset_revision_id !== measured.dataset_revision_id
      || canonical(object(native.selection)) !== canonical(measuredSelection)
      || measured.row_count !== metadata.row_count) throw new Error('prepared_selection_mismatch');
    const source = object(receipt.source_request, 'source_request');
    if (source.schema !== 'qz.polymarket_v2_request/1' || !Array.isArray(source.tokens) || !source.tokens.length
      || source.tokens.some((token) => typeof token !== 'string' || !/^[1-9][0-9]{0,77}$/.test(token) || BigInt(token) >= 1n << 256n)
      || new Set(source.tokens).size !== source.tokens.length) throw new Error('original_token_request_required');
    const window = object(receipt.source_window, 'source_window');
    if (typeof window.start_seconds !== 'number' || !Number.isSafeInteger(window.start_seconds)
      || typeof window.end_seconds !== 'number' || !Number.isSafeInteger(window.end_seconds)
      || window.start_seconds >= window.end_seconds || uint(window.bar_seconds) > 86400
      || BigInt(window.start_seconds) * 1_000_000_000n < dayNs(source.start_date)
      || BigInt(window.end_seconds) * 1_000_000_000n > dayNs(source.end_date)
      || ns(measured.first_event_ns) < BigInt(window.start_seconds) * 1_000_000_000n
      || ns(measured.last_event_ns) > BigInt(window.end_seconds) * 1_000_000_000n
      || ns(measured.first_event_ns) > ns(measured.last_event_ns)
      || ns(measured.available_through_ns) > ns(measuredSelection.decision_cutoff_ns)) throw new Error('prepared_source_window_mismatch');
    const definitions = object(metadata.universe, 'native_universe').instrument_definitions;
    const definitionJson = text(receipt.source_definitions_json, 'source_definitions_json');
    const sourceDefinitions = parseOriginalJson(definitionJson);
    if (!Array.isArray(definitions) || !definitions.length || !Array.isArray(sourceDefinitions) || !sourceDefinitions.length
      || definitions.some((definition) => !sourceDefinitions.some((original) => canonical(original) === canonical(definition)))) throw new Error('native_definitions_required');
    const instruments = new Map<string, string>();
    for (const definition of definitions) {
      const payload = object(object(definition).BinaryOption, 'binary_option');
      const token = text(payload.raw_symbol, 'token');
      const id = text(payload.id, 'instrument_id');
      if (payload.currency !== 'pUSD' || !(source.tokens as string[]).includes(token)) throw new Error('prepared_token_or_currency_mismatch');
      instruments.set(id, token);
    }
    if (!Array.isArray(measured.instrument_ids) || measured.instrument_ids.some((id) => !instruments.has(String(id)))) throw new Error('prepared_instrument_mismatch');
    const sourceReport = object(object(receipt.source_artifacts).import_report, 'original_import_report');
    const reportFile = await jsonFile(text(sourceReport.path, 'original_import_report_path'));
    if (reportFile.bytes !== sourceReport.bytes || reportFile.value.source_reference !== receipt.source_reference
      || reportFile.value.coverage !== 'UNPROVEN' || reportFile.value.historical_availability !== 'UNVERIFIED'
      || reportFile.value.registered_in_quazonai !== false) throw new Error('original_source_report_mismatch');
    const request = factorRequest((await jsonFile(requestPath)).value);
    const effective = heldout ? request : iterationRequest({ request }, clippedStrategy);
    contained(selection(object(effective.forecast).selection), measuredSelection);
    await catalog(catalogPath);
    return { catalog: catalogPath, request, metadata, metadataSha256, nativeDatasetRevisionId: native.dataset_revision_id!, selection: measuredSelection, instrumentIds: [...instruments.keys()],
      window, definitionJson, firstEvent: ns(measured.first_event_ns), source: receipt.source_reference as string, tokens: source.tokens };
  }

  if (!!options.sealedPrepared !== !!options.sealedRequest) throw new Error('paired_sealed_inputs_required');
  const strategy = options.strategy ? (await jsonFile(options.strategy)).value : null;
  if (strategy) { safeNumbers(strategy); thresholdPolicy(strategy); }
  const clippedStrategy = strategy?.forecastMode === 'price-volume' || strategy?.evaluationPartition === 'HELDOUT';
  const output = resolve(options.output);
  await noSymlinks(output);
  const primary = await prepared(options.prepared, options.request, false);
  const sealed = options.sealedPrepared && options.sealedRequest ? await prepared(options.sealedPrepared, options.sealedRequest, true) : null;
  if (sealed && (sealed.source !== primary.source || canonical([...(sealed.tokens as string[])].sort()) !== canonical([...(primary.tokens as string[])].sort())
    || sealed.definitionJson !== primary.definitionJson || sealed.window.bar_seconds !== primary.window.bar_seconds
    || sealed.window.start_seconds !== primary.window.start_seconds
    || Number(sealed.window.end_seconds) < Number(primary.window.end_seconds) || sealed.firstEvent !== primary.firstEvent
    || canonical(iterationRequest({ request: primary.request }, clippedStrategy)) !== canonical(iterationRequest({ request: sealed.request }, clippedStrategy))
    || ns(object(object(sealed.request.forecast).selection).event_end_ns) <= ns(sealed.request.validation_end_ns))) throw new Error('heldout_criteria_mismatch');
  if (within(output, primary.catalog) || (sealed && within(output, sealed.catalog))) throw new Error('dataset_output_overlaps_catalog');
  const data: JsonObject = { catalog: primary.catalog, request: primary.request };
  if (sealed) data.sealed = { catalog: sealed.catalog, request: sealed.request };
  const result: DatasetConfig = { id: text(options.id, 'dataset_id'), name: text(options.name, 'dataset_name'),
    assetClass: text(options.assetClass ?? 'BINARY_OPTION', 'asset_class'), currency: 'pUSD',
    source: `${primary.source} | ${text(primary.metadata.origin, 'origin')} | PIT UNVERIFIED; coverage UNPROVEN`,
    start: text(primary.metadata.event_start, 'event_start'), end: text((sealed ?? primary).metadata.event_end, 'event_end'),
    ready: options.ready === true, data };
  if (options.policy) { result.policy = (await jsonFile(options.policy)).value; safeNumbers(result.policy); }
  if (strategy) {
    const ids = strategy.kind === 'cross-market-threshold' ? strategy.instrumentIds as string[] : [strategy.instrumentId];
    const bars = object(object(primary.request.forecast).selection).bar_types as string[];
    if (ids.some((id) => typeof id !== 'string' || !primary.instrumentIds.includes(id)
      || !bars.some((bar) => bar.split('-').slice(0, -4).join('-') === id))) throw new Error('original_strategy_mismatch');
    text(strategy.name, 'strategy_name');
    const weight = decimalUnits(decimal(strategy.investedWeight, 'strategy_weight'));
    const targetTtl = BigInt(counter(strategy.targetTtlNs, 'strategy_ttl'));
    const finalSelection = selection(object((sealed ?? primary).request.forecast).selection);
    if (typeof strategy.threshold !== 'number' || !Number.isFinite(strategy.threshold) || weight <= 0n || weight > 10n ** 18n
      || targetTtl <= 0n || targetTtl > maxDbCounter
      || ns(finalSelection.decision_cutoff_ns) + targetTtl > maxDbCounter
      || object(strategy.simulationSettings, 'simulation_settings').base_currency !== result.currency) throw new Error('original_strategy_mismatch');
    // HELDOUT replay captures the original full sealed binding, not the researcher-visible prefix.
    const replay = strategy.evaluationPartition === 'HELDOUT' && sealed ? sealed : primary;
    contained(finalSelection, replay.selection);
    const interval = BigInt(replay.window.bar_seconds as number) * 1_000_000_000n;
    const lastLabel = (ns(finalSelection.event_end_ns) - 1n) / interval * interval;
    if (BigInt(replay.window.start_seconds as number) * 1_000_000_000n > ns(finalSelection.event_start_ns)
      || BigInt(replay.window.end_seconds as number) * 1_000_000_000n < lastLabel) throw new Error('strategy_source_window_does_not_cover_final_request');
    result.strategy = strategy as unknown as ThresholdPolicy;
  }
  // An exclusive hard link publishes only the complete document, never replacing an existing output.
  result.revisionManifest = datasetRevisionManifest(output, result.id);
  const revision = await prepareDatasetRevision(result, { kind: 'NATIVE_PREPARED_DATASET_EXPORT',
    primary: { source: primary.source, metadataSha256: primary.metadataSha256, nativeDatasetRevisionId: primary.nativeDatasetRevisionId },
    sealed: sealed ? { source: sealed.source, metadataSha256: sealed.metadataSha256, nativeDatasetRevisionId: sealed.nativeDatasetRevisionId } : null,
    limitations: ['LOCAL_PUBLICATION_AVAILABILITY_NOT_HISTORICAL_PIT', 'NATIVE_PREPARATION_NOT_INVESTMENT_QUALIFICATION'] });
  result.data = revision.data;
  const temporary = await mkdtemp(join(dirname(output), '.qz-dataset-'));
  try {
    const file = join(temporary, 'dataset.json');
    const handle = await open(file, 'wx', 0o600);
    const contents = Buffer.from(JSON.stringify(result, null, 2) + '\n');
    try { await handle.writeFile(contents); await handle.sync(); } finally { await handle.close(); }
    checkedAncestors.clear();
    for (const path of observed.keys()) await observe(path);
    await noSymlinks(output);
    await link(file, output);
    try { await revision.publish(); }
    catch (error) {
      // A failed alias CAS leaves this export retryable. Remove ONLY the exact
      // new hard link we created; preserve an independently replaced/edited file.
      const own = await lstat(file); const current = await lstat(output).catch(() => null);
      if (current?.dev === own.dev && current.ino === own.ino && (await readFile(output)).equals(contents)) await unlink(output);
      throw error;
    }
  } finally { await rm(temporary, { recursive: true, force: true }); }
  return result;
}

export async function main(argv = process.argv.slice(2)): Promise<void> {
  const { values } = parseArgs({ args: argv, options: {
    prepared: { type: 'string' }, request: { type: 'string' }, output: { type: 'string' },
    id: { type: 'string' }, name: { type: 'string' }, 'asset-class': { type: 'string' }, ready: { type: 'boolean' },
    'sealed-prepared': { type: 'string' }, 'sealed-request': { type: 'string' }, policy: { type: 'string' }, strategy: { type: 'string' },
    help: { type: 'boolean' },
  } });
  if (values.help) { console.log('Offline DatasetConfig and source revision export: --prepared preparation.json --request ORIGINAL_FACTOR.json --id ID --name NAME --output NEW.json [--sealed-prepared RECEIPT --sealed-request ORIGINAL.json] [--policy FILE] [--strategy FILE] [--ready]. Existing registered monitors may consume completed revisions; this does not register a new source or establish qualification.'); return; }
  const result = await exportDataset({ prepared: text(values.prepared, 'prepared'), request: text(values.request, 'request'),
    output: text(values.output, 'output'), id: text(values.id, 'id'), name: text(values.name, 'name'),
    assetClass: values['asset-class'], ready: values.ready, sealedPrepared: values['sealed-prepared'], sealedRequest: values['sealed-request'],
    policy: values.policy, strategy: values.strategy });
  console.log(JSON.stringify({ status: 'DATASET_CONFIG_EXPORTED', file: resolve(values.output!), dataset_id: result.id, revision_manifest: result.revisionManifest,
    source_revision_published: true, existing_registered_monitors_may_consume_revision: true,
    ready: result.ready, registered_in_service: false, research_started: false, historical_availability: 'UNVERIFIED' }));
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error: unknown) => { console.error(error instanceof Error ? error.message : 'dataset_export_failed'); process.exitCode = 1; });
}
