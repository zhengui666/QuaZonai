import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import wabtFactory from 'wabt';
import { canonical } from '../src/documents.ts';
import { object } from '../src/trials.ts';
import { intervalTargets, simulationSelection, thresholdTargets } from '../src/strategy.ts';
import type { StrategyMember, StrategyPartition, ThresholdPolicy } from '../src/strategy.ts';
import { normalizedRequest } from './native-wasm.ts';

const commands = new Set(['evaluate-factor', 'forecast', 'forecast-factor', 'evaluate-experiment', 'simulate', 'study-portfolio', 'allocate', 'blend-targets']);
const wabt = await wabtFactory();

/** Original JSON stdin and official process cleanup/cancellation; no provider environment or deadline. */
export async function executeNative(binary: string, command: string, input: JsonObject, args: string[], directory: string, context: Context): Promise<JsonObject> {
  if (!commands.has(command)) throw new Error('native_command_unsupported');
  const inputPath = join(directory, 'request.json');
  await writeFile(inputPath, JSON.stringify(input));
  const env = new NodeExecutionEnv({ cwd: directory });
  let stdout = ''; let stderr = '';
  try {
    const result = getOrThrow(await env.exec([
      'bash', '-c', 'file="$1"; shift; exec "$@" < "$file"', 'qz-native', inputPath, binary, command, ...args,
    ], { inheritEnv: false, env: { PATH: '/usr/bin:/bin' },
      onOutput: (chunk, _ctx, info) => { if (info.stream === 'stdout') stdout += chunk; else stderr += chunk; },
    }, context));
    if (result.exitCode !== 0) throw new Error(stderr.trim() || `native_${command}_failed`);
    return object(JSON.parse(stdout), 'native_result');
  } finally { await env.cleanup(withoutAbortSignal(context)); }
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
    const module = wabt.parseWat('selected-factor.wat', member.model);
    try { module.validate(); await writeFile(modelPath, module.toBinary({}).buffer); } finally { module.destroy(); }
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
  policy: ThresholdPolicy;
  partition: StrategyPartition;
  binary: string;
}

export async function evaluatePortfolio(input: PortfolioEvaluationInput, context: Context) {
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
