import type { JsonObject } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import wabtFactory from 'wabt';
import { object } from '../src/trials.ts';
import { canonical } from '../src/documents.ts';
import { partitionBoundaries } from './native-exposure.ts';
import { ExecutionFailure } from '../src/execution-errors.ts';
import { jsonChunks } from '../src/native-json.ts';

const wabt = await wabtFactory();

export function compileWasm(model: string, filename = 'factor.wat', operation = 'evaluate-factor'): Uint8Array {
  let module: ReturnType<typeof wabt.parseWat> | undefined;
  try {
    module = wabt.parseWat(filename, model);
    module.validate();
    return module.toBinary({}).buffer;
  } catch (error) { throw new ExecutionFailure(error, { operation, stage: 'compile' }); }
  finally { module?.destroy(); }
}

export function normalizedRequest(data: JsonObject): JsonObject {
  // Native transaction drafts are JSON proxies, which structuredClone cannot copy.
  const ensemble = data.catalog_ensemble_request !== undefined;
  const catalog = data.catalog_scan_request !== undefined;
  if ([data.catalog_ensemble_request, data.catalog_scan_request, data.request].filter(value => value !== undefined).length > 1) throw new Error('ambiguous_factor_request');
  const request = JSON.parse(JSON.stringify(object(ensemble ? data.catalog_ensemble_request : catalog ? data.catalog_scan_request : data.request, 'native_request'))) as JsonObject;
  if (ensemble) {
    partitionBoundaries({ ...request, train_end_ns: data.train_end_ns!, validation_end_ns: data.validation_end_ns! }, true);
    object(request.settings, 'simulation_settings');
    if (request.score_max_age_ns === null) delete request.score_max_age_ns;
    if (Array.isArray(request.members)) for (const value of request.members) {
      const member = object(value, 'member_template');
      if (member.total_fuel === null) delete member.total_fuel;
    }
    return request;
  }
  partitionBoundaries(request, catalog);
  const parameters = object(catalog ? request.parameters : object(request.forecast, 'forecast').parameters, 'parameters');
  // The native optional-fuel contract serializes both null and absence as omitted.
  if (parameters.total_fuel === null) delete parameters.total_fuel;
  return request;
}

export function iterationRequest(data: JsonObject, clipDecisionCutoff = false): JsonObject {
  const request = normalizedRequest(data);
  if (data.catalog_ensemble_request !== undefined) {
    request.event_end_ns = data.validation_end_ns!;
    request.evaluation_start_ns = data.train_end_ns!;
    if (clipDecisionCutoff) request.decision_cutoff_ns = data.validation_end_ns!;
    return request;
  }
  const selection = data.catalog_scan_request !== undefined ? request : object(object(request.forecast, 'forecast_request').selection, 'catalog_selection');
  selection.event_end_ns = request.validation_end_ns!;
  if (clipDecisionCutoff) selection.decision_cutoff_ns = request.validation_end_ns!;
  return request;
}

export async function evaluateWasm(model: string, data: JsonObject, context: Context, scope?: JsonObject): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const request = normalizedRequest(data);
  const catalog = data.catalog_scan_request !== undefined;
  const operation = catalog ? 'scan-factor-catalog' : 'evaluate-factor';
  if (catalog && (!Array.isArray(scope?.barTypes) || !scope.barTypes.every((type) => typeof type === 'string'))) throw new Error('catalog_scope_required');
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-factor-'));
  const env = new NodeExecutionEnv({ cwd: directory });
  let stage: 'execute' | 'decode' | 'validate' = 'execute';
  try {
    await writeFile(join(directory, 'model.wasm'), compileWasm(model, 'factor.wat', operation));
    await writeFile(join(directory, 'request.json'), JSON.stringify(request));
    if (catalog) await writeFile(join(directory, 'scope.json'), jsonChunks(scope!.barTypes!));
    let stdout = ''; let stderr = '';
    const result = getOrThrow(await env.exec([
      'bash', '-c', catalog
        ? 'exec "$1" scan-factor-catalog --catalog "$2" --model "$3" --scope "$5" < "$4"'
        : 'exec "$1" evaluate-factor --summary-only --catalog "$2" --model "$3" < "$4"',
      'qz-factor', process.env.QZ_JOB_BINARY ?? 'job', data.catalog,
      join(directory, 'model.wasm'), join(directory, 'request.json'), join(directory, 'scope.json'),
    ], {
      inheritEnv: false, env: { PATH: process.env.PATH ?? '/usr/bin:/bin', TMPDIR: directory },
      onOutput: (text, _context, info) => { if (info.stream === 'stdout') stdout += text; else stderr += text; },
    }, context));
    if (result.exitCode !== 0) throw new Error(stderr.trim() || `native_factor_exit_${result.exitCode}`);
    stage = 'decode';
    const output = object(JSON.parse(stdout), 'native_result');
    stage = 'validate';
    if (canonical(output.request!) !== canonical(request)) throw new Error('native_result_request_mismatch');
    if ('points' in output) throw new Error('native_summary_required');
    return output;
  } catch (error) {
    if (error instanceof ExecutionFailure) throw error;
    throw new ExecutionFailure(error, { operation, stage });
  } finally {
    await env.cleanup(withoutAbortSignal(context));
    await rm(directory, { recursive: true, force: true });
  }
}
