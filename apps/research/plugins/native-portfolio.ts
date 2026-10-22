import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { canonical } from '../src/documents.ts';
import { object } from '../src/trials.ts';
import { intervalTargets, nativeDecimal, simulationSelection, thresholdTargets } from '../src/strategy.ts';
import { JsonReader, jsonChunks } from '../src/native-json.ts';
import type { StrategyMember, StrategyPartition, StrategyPolicy, ThresholdPolicy } from '../src/strategy.ts';
import { compileWasm, iterationRequest, normalizedRequest } from './native-wasm.ts';
import { ExecutionFailure } from '../src/execution-errors.ts';

const commands = new Set(['evaluate-factor', 'forecast', 'forecast-factor', 'evaluate-experiment', 'simulate', 'replay-catalog-ensemble', 'study-portfolio', 'allocate', 'blend-targets', 'assess-capacity', 'size-account', 'validate-capital-exit', 'fit-ensemble', 'apply-ensemble', 'capital-curve', 'freeze-ensemble-calibrations', 'compare-observed-spot', 'study-observed-spot', 'value-observed-spot', 'catalog-prefix-digest']);

/** Original JSON stdin and official process cleanup/cancellation; no provider environment or deadline. */
export async function executeNative(binary: string, command: string, input: JsonObject, args: string[], directory: string, context: Context): Promise<JsonObject> {
  if (!commands.has(command)) throw new Error('native_command_unsupported');
  const inputPath = join(directory, 'request.json');
  await writeFile(inputPath, jsonChunks(input));
  const env = new NodeExecutionEnv({ cwd: directory });
  const stdout = new JsonReader(); let parseError: unknown; let stderr = '';
  let stage: 'execute' | 'decode' = 'execute';
  try {
    const result = getOrThrow(await env.exec([
      'bash', '-c', 'file="$1"; shift; exec "$@" < "$file"', 'qz-native', inputPath, binary, command, ...args,
    ], { inheritEnv: false, env: { PATH: '/usr/bin:/bin', TMPDIR: directory },
      onOutput: (chunk, _ctx, info) => {
        if (info.stream === 'stderr') stderr += chunk;
        else if (parseError === undefined) { try { stdout.write(chunk); } catch (error) { parseError = error; } }
      },
    }, context));
    if (result.exitCode !== 0) throw new Error(stderr.trim() || `native_${command}_failed`);
    stage = 'decode';
    if (parseError !== undefined) throw parseError;
    return object(stdout.finish(), 'native_result');
  } catch (error) { throw new ExecutionFailure(error, { operation: command, stage }); }
  finally { await env.cleanup(withoutAbortSignal(context)); }
}

export async function blendTargets(binary: string, memberRows: JsonObject[][], weights: string[], directory: string, context: Context, force = false) {
  if (!memberRows.length || memberRows.some((row) => row.length !== memberRows[0]!.length)) throw new Error('strategy_target_alignment_mismatch');
  const rows = memberRows[0]!.map((_, index) => memberRows.map((row) => row[index]!));
  // Native validates exact weights, clocks, assets and currency. Never fill missing member decisions.
  const blend = memberRows.length > 1 || force ? await executeNative(binary, 'blend-targets', { schema_version: 1, weights, rows }, [], directory, context) : null;
  const points = blend ? blend.target_points : memberRows[0]!;
  if (!Array.isArray(points) || !points.length) throw new Error('native_blended_targets_missing');
  return { rows, weights, blend, points: points.map((point) => object(point, 'native_target')) };
}

/** One compiled original WAT per member, reused by diagnostics and causal-prefix forecasting. */
export async function forecastMembers(binary: string, catalog: string, members: StrategyMember[], request: JsonObject, policy: ThresholdPolicy, currency: string,
  directory: string, context: Context, diagnostics = false, partition?: StrategyPartition, forceBlend = false) {
  const components: JsonObject[] = []; const memberRows: JsonObject[][] = [];
  const selection = simulationSelection(request, partition, policy);
  // The registered request remains authoritative. This explicit multi-market
  // projection computes only the registered choices, retaining their full causal prefix.
  const computationRequest = policy.kind === 'cross-market-threshold'
    ? { ...request, forecast: { ...object(request.forecast, 'forecast_request'), selection: simulationSelection(request, undefined, policy) } } : request;
  for (const [index, member] of members.entries()) {
    const modelPath = join(directory, `model-${index}.wasm`);
    await writeFile(modelPath, compileWasm(member.model, 'selected-factor.wat', 'portfolio-evaluation'));
    const args = ['--catalog', catalog, '--model', modelPath];
    const summary = diagnostics ? await executeNative(binary, 'evaluate-factor', computationRequest, ['--summary-only', ...args], directory, context) : null;
    if (summary && (canonical(summary.request!) !== canonical(computationRequest) || 'points' in summary)) throw new Error('native_summary_request_mismatch');
    const forecastRequest = object(computationRequest.forecast, 'forecast_request');
    const command = policy.forecastMode === 'price-volume' ? 'forecast-factor' : 'forecast';
    const forecasts = await executeNative(binary, command, forecastRequest, args, directory, context);
    const original = thresholdTargets(forecasts, policy, currency, partition ? selection : undefined);
    const points = partition && policy.kind !== 'cross-market-threshold' ? intervalTargets(original, selection) : original;
    memberRows.push(points);
    components.push({ index, member: { ...member }, command, forecastRequest, forecasts, target_points: points, summary });
  }
  const blended = await blendTargets(binary, memberRows, members.map((member) => member.weight), directory, context, diagnostics || forceBlend);
  return { components, selection, ...blended };
}

export interface PortfolioEvaluationInput {
  data: JsonObject;
  members: StrategyMember[];
  policy: StrategyPolicy;
  partition: StrategyPartition;
  binary: string;
}

export async function evaluatePortfolio(input: PortfolioEvaluationInput, context: Context, scope?: JsonObject) {
  if (input.policy.kind === 'catalog-ensemble') return evaluateCatalogEnsemble(input, context, scope);
  if (typeof input.data.catalog !== 'string') throw new Error('catalog_required');
  const request = normalizedRequest(input.data);
  const currency = input.policy.simulationSettings.base_currency;
  if (typeof currency !== 'string') throw new Error('strategy_currency_required');
  if (typeof input.policy.simulationSettings.starting_capital !== 'string') throw new Error('strategy_starting_capital_required');
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-strategy-'));
  try {
    const targets = await forecastMembers(input.binary, input.data.catalog, input.members, request, input.policy, currency, directory, context, true, input.partition);
    const simulationRequest: JsonObject = { schema_version: 1, selection: targets.selection, settings: input.policy.simulationSettings, target_points: targets.points };
    const simulation = await executeNative(input.binary, 'simulate', simulationRequest, ['--catalog', input.data.catalog], directory, context);
    const first = object(targets.components[0]!.summary, 'component_summary');
    const summary: JsonObject = { schema_version: 1, request, partitions: first.partitions!,
      partitions_component: { index: 0, meaning: 'COMPONENT_FACTOR_DIAGNOSTICS_NOT_ACCOUNT_METRICS' },
      components: targets.components.map((component) => ({ index: component.index!, weight: object(component.member, 'member').weight!, summary: component.summary! })),
      portfolio: { partition: input.partition, native_version: simulation.native_version ?? null,
        ...(input.policy.kind === 'cross-market-threshold' ? { strategy_kind: input.policy.kind } : {}),
        ...(input.policy.evidenceMode ? { evidenceMode: input.policy.evidenceMode } : {}),
        statistics: simulation.statistics ?? [], summary: simulation.summary ?? {},
        iterations: simulation.iterations ?? null, events: simulation.events ?? null, orders: simulation.orders ?? null,
        positions: simulation.positions ?? null, consumed_target_points: simulation.consumed_target_points ?? null,
        returns_kind: simulation.returns_kind ?? null, returns_status: simulation.returns_status ?? null, returns_reason: simulation.returns_reason ?? null,
        provenance: { command: 'simulate', account: 'ONE_NATIVE_SHARED_ACCOUNT', catalog: input.data.catalog, selection: targets.selection } } };
    // Raw forecasts/labels, native return arrays and the original account curve stay in owner-only task facts.
    const evidence: JsonObject = { catalog: input.data.catalog, request, policy: JSON.parse(JSON.stringify(input.policy)), members: input.members.map((member) => ({ ...member })),
      components: targets.components, rows: targets.rows, weights: targets.weights, blend: targets.blend, simulationRequest, simulation };
    return { summary, evidence };
  } finally { await rm(directory, { recursive: true, force: true }); }
}

async function evaluateCatalogEnsemble(input: PortfolioEvaluationInput, context: Context, scope?: JsonObject) {
  if (typeof input.data.catalog !== 'string') throw new Error('catalog_required');
  if (typeof input.data.catalog_partitions !== 'string' || !input.data.catalog_partitions.trim()) throw new Error('catalog_partitions_required');
  if (input.data.catalog_ensemble_request === undefined) throw new Error('catalog_ensemble_request_required');
  if (input.members.length < 2) throw new Error('strategy_members_required');
  const request = input.partition === 'VALIDATION' ? iterationRequest(input.data, true) : normalizedRequest(input.data);
  if (input.partition === 'HELDOUT') request.evaluation_start_ns = input.data.validation_end_ns!;
  if (!Array.isArray(request.members)) throw new Error('strategy_members_required');
  const template = request.members.map(value => object(value, 'member_template'));
  const metered = template.some(member => member.total_fuel !== undefined && member.total_fuel !== null);
  if (metered && template.length !== input.members.length) throw new Error('catalog_ensemble_member_count_mismatch');
  const nativeRequest = { ...request, members: input.members.map((member, index) => ({ weight: member.weight,
    ...(metered && template[index]!.total_fuel !== undefined && template[index]!.total_fuel !== null ? { total_fuel: template[index]!.total_fuel! } : {}) })) };
  if (!Array.isArray(scope?.barTypes) || !scope.barTypes.length || !scope.barTypes.every(type => typeof type === 'string')) throw new Error('catalog_scope_required');
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-catalog-ensemble-'));
  try {
    const scopePath = join(directory, 'scope.json');
    await writeFile(scopePath, jsonChunks(scope.barTypes));
    const args = ['--catalog', input.data.catalog, '--partitions', input.data.catalog_partitions, '--scope', scopePath];
    for (const [index, member] of input.members.entries()) {
      const path = join(directory, `model-${index}.wasm`);
      await writeFile(path, compileWasm(member.model, 'catalog-ensemble.wat', 'replay-catalog-ensemble'));
      args.push('--model', path);
    }
    const result = await executeNative(input.binary, 'replay-catalog-ensemble', nativeRequest, args, directory, context);
    if (result.source !== request.source || result.revision !== request.revision) throw new Error('native_catalog_source_mismatch');
    const simulation = object(result.simulation, 'native_simulation');
    const summary: JsonObject = { schema_version: 1, request: nativeRequest, registered_request: request,
      ...(input.data.catalog_scope ? { catalog_scope: input.data.catalog_scope } : {}), partitions: [], portfolio: {
      partition: input.partition, strategy_kind: 'catalog-ensemble', native_version: simulation.native_version ?? null,
      ...(input.policy.evidenceMode ? { evidenceMode: input.policy.evidenceMode } : {}),
      source: result.source!, revision: result.revision!, observed_markets: result.observed_markets ?? null,
      catalog_series: result.catalog_series ?? null,
      bars: result.bars ?? null, source_records: result.source_records ?? null, consumed_fuel: result.consumed_fuel ?? null,
      assumptions: result.assumptions ?? [], statistics: simulation.statistics ?? [], summary: simulation.summary ?? {},
      recorded_fee_totals: recordedFeeTotals(simulation),
      iterations: simulation.iterations ?? null, events: simulation.events ?? null, orders: simulation.orders ?? null,
      positions: simulation.positions ?? null, consumed_target_points: simulation.consumed_target_points ?? null,
      returns_kind: simulation.returns_kind ?? null, returns_status: simulation.returns_status ?? null, returns_reason: simulation.returns_reason ?? null,
      ...(input.partition === 'HELDOUT' && Array.isArray(simulation.returns) ? { daily_returns: simulation.returns } : {}),
      provenance: { command: 'replay-catalog-ensemble', account: 'ONE_NATIVE_SHARED_ACCOUNT', selection: nativeRequest } } };
    const evidence: JsonObject = { catalog: input.data.catalog, catalog_partitions: input.data.catalog_partitions,
      request: nativeRequest, registered_request: request, policy: JSON.parse(JSON.stringify(input.policy)), members: input.members.map(member => ({ ...member })), nativeRequest, simulation, result };
    return { summary, evidence };
  } finally { await rm(directory, { recursive: true, force: true }); }
}

function exactTotal(value: bigint, places: number): string {
  const digits = (value < 0n ? -value : value).toString().padStart(places + 1, '0');
  const fraction = digits.slice(-places).replace(/0+$/, '');
  return `${value < 0n ? '-' : ''}${digits.slice(0, -places)}${fraction ? `.${fraction}` : ''}`;
}

function recordedFeeTotals(simulation: JsonObject): JsonObject | null {
  if (simulation.recorded_fee_replay == null) return null;
  const report = object(simulation.recorded_fee_replay, 'recorded_fee_replay');
  if (typeof report.currency !== 'string' || !Array.isArray(report.fills)) throw new Error('native_recorded_fee_replay_invalid');
  let fills = 0n; let commission = 0n; let notional = 0n;
  for (const value of report.fills) {
    const fill = object(value, 'recorded_fee_fill');
    fills++; commission += nativeDecimal(fill.commission);
    notional += nativeDecimal(fill.fill_quantity) * nativeDecimal(fill.fill_price);
  }
  return { currency: report.currency, fills: fills.toString(), commission_total: exactTotal(commission, 18), gross_fill_notional: exactTotal(notional, 36) };
}
