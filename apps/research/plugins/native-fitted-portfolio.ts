import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { executeNative } from './native-portfolio.ts';
import { compileWasm, iterationRequest, normalizedRequest } from './native-wasm.ts';
import { canonical } from '../src/documents.ts';
import { object } from '../src/trials.ts';
import { intervalTargets, simulationSelection, thresholdTargets } from '../src/strategy.ts';
import type { StrategyPartition, ThresholdPolicy } from '../src/strategy.ts';

async function readCatalogPrefix(binary: NativeTarget, catalog: string, selection: JsonObject, directory: string, context: Context): Promise<JsonObject> {
  const prefix = await executeNative(binary, 'catalog-prefix-digest', selection, ['--catalog', catalog], directory, context);
  if (prefix.schema_version !== 1 || prefix.method !== 'NAUTILUS_SELECTED_PREFIX_SHA256_V1' || canonical(prefix.selection!) !== canonical(selection)
    || typeof prefix.content_sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(prefix.content_sha256)) throw new Error('native_catalog_prefix_evidence_invalid');
  return prefix;
}

export type FittedMember = { factorVersionId: string; model: string; sourceTrialId: number };
/** Frozen technical protocol; all folds remain strictly inside registered TRAIN. */
export function expandingTrainingFolds(request: JsonObject): JsonObject[] {
  const selection = object(object(request.forecast, 'forecast').selection, 'selection');
  const start = BigInt(selection.event_start_ns as string); const end = BigInt(request.train_end_ns as string);
  const span = end - start;
  if (span < 4n) throw new Error('ensemble_training_interval_insufficient');
  return [1n, 2n, 3n].map(index => ({ train_end_ns: (start + span * index / 4n).toString(),
    validation_start_ns: (start + span * index / 4n).toString(), validation_end_ns: (start + span * (index + 1n) / 4n).toString() }));
}
export async function fitNativeEnsemble(input: { data: JsonObject; members: FittedMember[]; binary: NativeTarget; evidenceIdentity: string }, context: Context) {
  if (input.data.catalog_ensemble_request !== undefined || input.data.catalog_scan_request !== undefined) throw new Error('ensemble_full_prediction_panel_required');
  if (typeof input.data.catalog !== 'string') throw new Error('catalog_required');
  if (input.members.length < 2) throw new Error('ensemble_two_registered_factors_required');
  const request = iterationRequest(input.data, true);
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-fit-'));
  try {
    const prefixSelection = object(object(request.forecast, 'forecast').selection, 'selection');
    const catalogPrefix = await readCatalogPrefix(input.binary, input.data.catalog, prefixSelection, directory, context);
    const reports: JsonObject[] = [];
    for (const [index, member] of input.members.entries()) {
      const model = join(directory, `factor-${index}.wasm`); await writeFile(model, compileWasm(member.model));
      const report = await executeNative(input.binary, 'evaluate-factor', request, ['--catalog', input.data.catalog, '--model', model], directory, context);
      if (canonical(report.request!) !== canonical(request)) throw new Error('ensemble_report_request_mismatch');
      reports.push({ factor_version_id: member.factorVersionId, source_evidence_ref: `${input.evidenceIdentity}:panel:${index}`, report });
    }
    if (canonical(await readCatalogPrefix(input.binary, input.data.catalog, prefixSelection, directory, context)) !== canonical(catalogPrefix)) throw new Error('catalog_prefix_changed_during_fit');
    const fitRequest = { schema_version: 1, fit_end_ns: request.train_end_ns!, folds: expandingTrainingFolds(request), members: reports };
    const fit = await executeNative(input.binary, 'fit-ensemble', fitRequest, [], directory, context);
    if (!Array.isArray(fit.members) || fit.members.length !== input.members.length || fit.fit_end_ns !== request.train_end_ns) throw new Error('ensemble_fit_result_mismatch');
    return { summary: { schema_version: 1, kind: 'ENSEMBLE_FIT', request, method: fit.method ?? null, fit_end_ns: fit.fit_end_ns,
      members: fit.members.map(value => { const member = object(value, 'fitted_member'); return { factorVersionId: member.factor_version_id!, weight: member.weight! }; }),
      qualification: 'TRAINING_ONLY_NOT_SELECTION' }, evidence: { fitRequest, fit, catalogPrefix, members: input.members.map(member => ({ ...member })), data: input.data } };
  } finally { await rm(directory, { recursive: true, force: true }); }
}

/** Reuse frozen calibration+weights; one calibrated ensemble forecast, one account. */
export async function replayFittedEnsemble(input: { data: JsonObject; members: FittedMember[]; fit: JsonObject; catalogPrefix: JsonObject; policy: ThresholdPolicy; partition: StrategyPartition; binary: NativeTarget; datasetBridge?: JsonObject }, context: Context) {
  if (typeof input.data.catalog !== 'string') throw new Error('catalog_required');
  const request = input.partition === 'VALIDATION' ? iterationRequest(input.data, true) : normalizedRequest(input.data);
  const forecastRequest = object(request.forecast, 'forecast_request');
  const selection = simulationSelection(request, input.partition, input.policy);
  const spot = input.policy.simulationSettings.multi_currency_spot_cash != null;
  if (spot && (input.datasetBridge?.kind !== 'RESEARCH_DATASET_REVISION_BRIDGE' || typeof input.datasetBridge.id !== 'string'
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(input.datasetBridge.id)
    || canonical(input.datasetBridge.simulationSettings!) !== canonical(input.policy.simulationSettings))) throw new Error('original_research_dataset_bridge_required');
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-fitted-replay-'));
  try {
    const prefixSelection = object(input.catalogPrefix.selection, 'original_fit_catalog_prefix_selection');
    const verifyOriginalPrefix = async () => {
      if (canonical(await readCatalogPrefix(input.binary, input.data.catalog as string, prefixSelection, directory, context)) !== canonical(input.catalogPrefix)) throw new Error('ensemble_replay_original_catalog_prefix_mismatch');
    };
    await verifyOriginalPrefix();
    const forecasts: JsonObject[] = [];
    for (const [index, member] of input.members.entries()) {
      const model = join(directory, `factor-${index}.wasm`); await writeFile(model, compileWasm(member.model));
      const forecast = await executeNative(input.binary, input.policy.forecastMode === 'price-volume' ? 'forecast-factor' : 'forecast', forecastRequest,
        ['--catalog', input.data.catalog, '--model', model], directory, context);
      forecasts.push({ factor_version_id: member.factorVersionId, forecast_request: forecastRequest, forecast });
    }
    const application = await executeNative(input.binary, 'apply-ensemble', { schema_version: 1, fit: input.fit, members: forecasts }, [], directory, context);
    const forecast = object(application.forecast, 'ensemble_forecast');
    const points = intervalTargets(thresholdTargets(forecast, input.policy, String(input.policy.simulationSettings.base_currency), selection), selection);
    const simulationRequest = { schema_version: 1, selection, settings: input.policy.simulationSettings, target_points: points };
    const simulation = await executeNative(input.binary, 'simulate', simulationRequest,
      ['--catalog', input.data.catalog, ...(spot ? ['--dataset-revision-id', String(input.datasetBridge!.id)] : [])], directory, context);
    await verifyOriginalPrefix();
    const report = spot ? object(simulation.spot_cash_report, 'spot_cash_report') : simulation;
    const daily = spot ? object(report.daily_returns, 'spot_cash_daily_returns') : null;
    if (daily && (daily.kind !== 'REPORT_CURRENCY_DAILY' || daily.report_currency !== input.policy.simulationSettings.base_currency || !Array.isArray(daily.days))) throw new Error('fitted_spot_report_currency_or_basis_mismatch');
    const days = daily?.days as JsonObject[] | undefined;
    const returnsStatus = !daily ? simulation.returns_status ?? null : !days!.length ? 'INSUFFICIENT_DATA' : days!.some(day => day.value === null) ? 'FAILED' : 'OK';
    const returnsReason = !daily ? simulation.returns_reason ?? null : !days!.length ? 'REPORT_CURRENCY_DAILY_RETURNS_UNAVAILABLE' : days!.some(day => day.value === null) ? 'REPORT_CURRENCY_DAILY_RETURN_GAP' : null;
    return { summary: { schema_version: 1, request, kind: 'FITTED_ENSEMBLE_REPLAY', fit_end_ns: input.fit.fit_end_ns!,
      members: input.fit.members!, portfolio: { partition: input.partition, strategy_kind: 'fitted-ensemble', statistics: report.statistics ?? [],
        summary: simulation.summary ?? {}, native_version: simulation.native_version ?? null, returns_status: simulation.returns_status ?? null,
        returns_kind: daily?.kind ?? simulation.returns_kind ?? null, returns_reason: returnsReason,
        ...(daily ? { returns_status: returnsStatus, report_currency: daily.report_currency!, fee_acceptance: report.fee_acceptance ?? null } : {}),
        provenance: { command: 'simulate', account: 'ONE_NATIVE_SHARED_ACCOUNT', selection, nativeDatasetRevisionId: input.datasetBridge?.id ?? null } } },
      evidence: { fit: input.fit, catalogPrefix: input.catalogPrefix, members: input.members.map(member => ({ ...member })), application, simulationRequest, simulation,
        policy: JSON.parse(JSON.stringify(input.policy)), request, datasetBridge: input.datasetBridge ?? null } };
  } finally { await rm(directory, { recursive: true, force: true }); }
}
