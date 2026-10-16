import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import wabtFactory from 'wabt';
import { canonical } from '../../research/src/documents.ts';
import type { ThresholdPolicy } from './config.ts';
import { counter, decimal, decimalUnits, json, object, Problem, text } from './facts.ts';
import { nativeCurve } from './projections.ts';
import type { Metric } from '../../../packages/contracts/src/index.ts';

export interface PortfolioTaskInput {
  id: string; dataDir: string; catalog: string; binary: string;
  // The original v1 single-evidence input remains replayable with its original kernel and default weight 1.
  capital: string; currency: string; evidence: JsonObject | JsonObject[]; policy: ThresholdPolicy; weights?: string[];
}
const commands = new Set(['evaluate-factor', 'forecast', 'evaluate-experiment', 'simulate', 'study-portfolio', 'allocate', 'blend-targets']);
const wabt = await wabtFactory();

/** Official process ownership and cancellation, original JSON stdin, no provider credentials. */
export async function nativeJob(binary: string, command: string, input: JsonObject, args: string[], directory: string, context: Context): Promise<JsonObject> {
  if (!commands.has(command)) throw new Problem('native_command_unsupported');
  const inputPath = join(directory, 'request.json');
  await writeFile(inputPath, JSON.stringify(input));
  const env = new NodeExecutionEnv({ cwd: directory });
  let stdout = '';
  try {
    const result = getOrThrow(await env.exec([
      'bash', '-c', 'file="$1"; shift; exec "$@" < "$file"', 'qz-native', inputPath, binary, command, ...args,
    ], { inheritEnv: false, env: { PATH: '/usr/bin:/bin' },
      onOutput: (chunk, _ctx, info) => { if (info.stream === 'stdout') stdout += chunk; },
    }, context));
    if (result.exitCode !== 0) throw new Problem(`native_${command}_failed`);
    return object(JSON.parse(stdout), 'native_result');
  } finally { await env.cleanup(withoutAbortSignal(context)); }
}

function thresholdTargets(forecasts: JsonObject, policy: ThresholdPolicy, currency: string): JsonObject[] {
  if (policy.kind !== 'factor-threshold') throw new Problem('strategy_adapter_unsupported');
  if (!Number.isFinite(policy.threshold)) throw new Problem('strategy_threshold_invalid');
  const weight = decimal(policy.investedWeight, 'policy_weight'); const units = decimalUnits(weight);
  if (units <= 0n || units > 10n ** 18n) throw new Problem('strategy_weight_invalid');
  const ttl = BigInt(counter(policy.targetTtlNs, 'target_ttl')); if (ttl <= 0n) throw new Problem('target_ttl_invalid');
  if (policy.simulationSettings.base_currency !== currency) throw new Problem('strategy_currency_mismatch');
  if (!Array.isArray(forecasts.points)) throw new Problem('native_forecast_points_missing');
  const available = forecasts.points.map((point) => object(point, 'forecast_point')).filter((point) => point.instrument_id === policy.instrumentId && typeof point.forecast === 'number');
  if (!available.length) throw new Problem('strategy_observations_missing');
  available.sort((a, b) => {
    for (const clock of ['available_ns', 'event_ns']) {
      const left = BigInt(counter(a[clock], clock)); const right = BigInt(counter(b[clock], clock));
      if (left !== right) return left < right ? -1 : 1;
    }
    return 0;
  });
  const points: JsonObject[] = [];
  for (const point of available) {
    const asof = counter(point.available_ns, 'available');
    if (BigInt(asof) < BigInt(counter(point.event_ns, 'event'))) throw new Problem('strategy_clock_invalid');
    const invested = (point.forecast as number) > policy.threshold;
    const cashUnits = 10n ** 18n - (invested ? units : 0n);
    const cash = `${cashUnits / 10n ** 18n}.${(cashUnits % 10n ** 18n).toString().padStart(18, '0')}`;
    const target: JsonObject = { schema_version: 1, asof_ns: asof, valid_until_ns: (BigInt(asof) + ttl).toString(),
      targets: [{ instrument_id: policy.instrumentId, currency, weight: invested ? weight : '0' }], cash_weight: cash };
    // A batch has one decision clock: retain the last original event at that clock, never resample missing decisions.
    if (points.at(-1)?.asof_ns === asof) points[points.length - 1] = target; else points.push(target);
  }
  return points;
}

/** Original selected models → explicit policy rows → native exact blending → ONE shared-account simulation. */
export async function computePortfolio(input: PortfolioTaskInput, context: Context) {
  const evidence = Array.isArray(input.evidence) ? input.evidence : [input.evidence];
  const weights = input.weights ?? (evidence.length === 1 ? ['1'] : null);
  if (!weights || !evidence.length || weights.length !== evidence.length) throw new Problem('portfolio_weights_required');
  const directory = await mkdtemp(join(input.dataDir, 'native-'));
  try {
    const sources: JsonObject[] = []; const memberRows: JsonObject[][] = [];
    let selection: JsonObject | undefined;
    for (const [index, source] of evidence.entries()) {
      const studyInput = object(source.input, 'study_input');
      const trial = object(source.selected, 'selected_trial');
      const proposal = object(trial.proposal, 'selected_proposal');
      const result = object(object(source.final, 'independent_final').result, 'independent_result');
      const forecastRequest = structuredClone(object(object(result.request, 'independent_request').forecast, 'forecast_request'));
      const current = object(forecastRequest.selection, 'selection');
      if (selection && canonical(selection) !== canonical(current)) throw new Problem('strategy_selection_mismatch');
      selection = current;
      if (Array.isArray(input.evidence) && canonical(object(studyInput.policy).strategy ?? null) !== canonical(json(input.policy))) throw new Problem('strategy_policy_mismatch');
      const model = text(proposal.model, 'selected_factor_model');
      const modelPath = join(directory, `model-${index}.wasm`);
      const module = wabt.parseWat('selected-factor.wat', model);
      try { module.validate(); await writeFile(modelPath, module.toBinary({}).buffer); } finally { module.destroy(); }
      // Every replay uses this admission's immutable catalog, model, native request, policy and kernel path.
      const forecasts = await nativeJob(input.binary, 'forecast', forecastRequest, ['--catalog', input.catalog, '--model', modelPath], directory, context);
      const points = thresholdTargets(forecasts, input.policy, input.currency);
      memberRows.push(points);
      sources.push({ researchId: source.researchId ?? null, trialId: trial.taskId ?? null, model: studyInput.model ?? null,
        policy: json(input.policy), forecastRequest, forecasts, target_points: points });
    }
    if (memberRows.some((row) => row.length !== memberRows[0]!.length)) throw new Problem('strategy_target_alignment_mismatch');
    const rows = memberRows[0]!.map((_, index) => memberRows.map((row) => row[index]!));
    // Each native row is one original decision across members; no filling/resampling or TS numerical blending.
    const blend = memberRows.length > 1 ? await nativeJob(input.binary, 'blend-targets', { schema_version: 1, weights, rows }, [], directory, context) : null;
    const points = blend ? blend.target_points : memberRows[0]!;
    if (!Array.isArray(points) || !points.length) throw new Problem('native_blended_targets_missing');
    const simulationRequest: JsonObject = { schema_version: 1, selection: selection!,
      settings: { ...input.policy.simulationSettings, starting_capital: input.capital }, target_points: points };
    const simulation = await nativeJob(input.binary, 'simulate', simulationRequest, ['--catalog', input.catalog], directory, context);
    const curve = nativeCurve(simulation, input.currency);
    const statistics: Metric[] = Array.isArray(simulation.statistics) ? simulation.statistics.map((item) => {
      const statistic = object(item, 'native_statistic'); const key = String(statistic.native_key);
      return { key, label: key, value: typeof statistic.value === 'number' ? statistic.value : null };
    }) : [];
    const native: JsonObject = { sources, rows, weights, blend, simulationRequest, simulation };
    if (sources.length === 1) native.forecasts = sources[0]!.forecasts!;
    return { native, target: object(points.at(-1), 'native_target'), curve, metrics: statistics };
  } finally { await rm(directory, { recursive: true, force: true }); }
}
