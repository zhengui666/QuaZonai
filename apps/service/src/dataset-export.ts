import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import type { DatasetConfig } from './config.ts';
import type { JsonObject } from '../../../packages/contracts/src/index.ts';
import { object, text } from './facts.ts';
import { validateNumbers as safeNumbers } from '../../research/src/input.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { exportTickDataset } from './tick-dataset-export.ts';
import { parseOriginalJson } from './dataset-json.ts';

export interface DatasetExportOptions {
  prepared: string; request: string; output: string; id: string; name: string;
  assetClass?: string; ready?: boolean;
  sealedPrepared?: string; sealedRequest?: string; policy?: string; strategy?: string;
}

function ns(value: unknown): bigint {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('native_nanosecond_string_required');
  return BigInt(value);
}
function uint(value: unknown): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 1 || value > 4294967295) throw new Error('native_positive_u32_required');
  return value;
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
  ns(result.decision_cutoff_ns);
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
  if (Number(parameters.slow_period) <= Number(parameters.fast_period)) throw new Error('native_forecast_parameters_required');
  // Reuse the exact current research boundary and optional-fuel normalization.
  iterationRequest(catalog ? { catalog_scan_request: value } : { request: value });
  return value;
}
export async function exportDataset(options: DatasetExportOptions): Promise<DatasetConfig> {
  const declaration = object(parseOriginalJson(await readFile(resolve(options.prepared), 'utf8')), 'prepared_declaration');
  if (declaration.modality !== 'TRADE_TICK') throw new Error('tick_prepared_metadata_required');
  return exportTickDataset(options);
}

export async function main(argv = process.argv.slice(2)): Promise<void> {
  const { values } = parseArgs({ args: argv, options: {
    prepared: { type: 'string' }, request: { type: 'string' }, output: { type: 'string' },
    id: { type: 'string' }, name: { type: 'string' }, 'asset-class': { type: 'string' }, ready: { type: 'boolean' },
    'sealed-prepared': { type: 'string' }, 'sealed-request': { type: 'string' }, policy: { type: 'string' }, strategy: { type: 'string' },
    help: { type: 'boolean' },
  } });
  if (values.help) { console.log('Offline tick DatasetConfig and source revision export: --prepared catalog-metadata.json --request ORIGINAL_FACTOR.json --id ID --name NAME --output NEW.json [--policy FILE] [--ready]. Existing registered monitors may consume completed revisions; this does not register a new source or establish qualification.'); return; }
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
