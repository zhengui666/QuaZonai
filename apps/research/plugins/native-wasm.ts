import type { JsonObject } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import wabtFactory from 'wabt';
import { object } from '../src/trials.ts';
import { canonical } from '../src/documents.ts';

const wabt = await wabtFactory();

function normalizedRequest(data: JsonObject): JsonObject {
  const request = structuredClone(object(data.request, 'native_request'));
  const parameters = object(object(request.forecast, 'forecast').parameters, 'parameters');
  // Rust serializes None by omitting total_fuel, for both absent and null input.
  if (parameters.total_fuel === null) delete parameters.total_fuel;
  return request;
}

export function iterationRequest(data: JsonObject): JsonObject {
  const request = normalizedRequest(data);
  const forecast = object(request.forecast, 'forecast_request');
  const selection = object(forecast.selection, 'catalog_selection');
  if (BigInt(selection.event_end_ns as string) < BigInt(request.validation_end_ns as string)) throw new Error('native_partition_boundaries');
  selection.event_end_ns = request.validation_end_ns!;
  return request;
}

export async function evaluateWasm(model: string, data: JsonObject, context: Context): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const request = normalizedRequest(data);
  const directory = await mkdtemp(join(tmpdir(), 'qz-factor-'));
  const env = new NodeExecutionEnv({ cwd: directory });
  try {
    const module = wabt.parseWat('factor.wat', model);
    let binary: Uint8Array;
    try { module.validate(); binary = module.toBinary({}).buffer; } finally { module.destroy(); }
    await writeFile(join(directory, 'model.wasm'), binary);
    await writeFile(join(directory, 'request.json'), JSON.stringify(request));
    let stdout = ''; let stderr = '';
    const result = getOrThrow(await env.exec([
      'bash', '-c', 'exec "$1" evaluate-factor --catalog "$2" --model "$3" < "$4"',
      'qz-factor', process.env.QZ_JOB_BINARY ?? 'job', data.catalog,
      join(directory, 'model.wasm'), join(directory, 'request.json'),
    ], {
      inheritEnv: false, env: { PATH: process.env.PATH ?? '/usr/bin:/bin' },
      onOutput: (text, _context, info) => { if (info.stream === 'stdout') stdout += text; else stderr += text; },
    }, context));
    if (result.exitCode !== 0) throw new Error(stderr.trim() || `native_factor_exit_${result.exitCode}`);
    const output = object(JSON.parse(stdout), 'native_result');
    if (canonical(output.request!) !== canonical(request)) throw new Error('native_result_request_mismatch');
    const { points: _points, ...summary } = output;
    return summary;
  } finally {
    await env.cleanup(withoutAbortSignal(context));
    await rm(directory, { recursive: true, force: true });
  }
}
