import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { canonical } from '../../research/src/documents.ts';
import { ExecutionFailure } from '../../research/src/execution-errors.ts';
import { exactWeights, replayCatalog, resolvePortfolioPolicy, resolveStrategyPolicy, strategyMembers } from '../../research/src/strategy.ts';
import { blendTargets, evaluatePortfolio, executeNative, forecastMembers } from '../../research/plugins/native-portfolio.ts';
import { normalizedRequest } from '../../research/plugins/native-wasm.ts';
import type { StrategyPolicy } from './config.ts';
import { json, object, Problem } from './facts.ts';
import { nativeCurve } from './projections.ts';
import type { CapacityReport, Metric, ResearchSpecification, PortfolioProgress, EquityPoint } from '../../../packages/contracts/src/index.ts';
import { assessCapacity, capacityPolicy, unavailableCapacity } from './capacity.ts';
import { withoutExecutionQuotas } from './science-binding.ts';

export interface PortfolioTaskInput {
  review?: { policy: import('../../../packages/contracts/src/review.ts').ReviewPolicy; model: { provider: string; modelId: string }; thinkingLevel?: import('../../../packages/contracts/src/index.ts').ThinkingLevel; createdAt: string; independentData?: JsonObject; independentSource?: import('../../research/src/review-source.ts').CapturedReviewSource | null; maximumScientificCalls?: number; stressMethod?: 'QZ_REVIEW_ONE_BAR_NATIVE_SLIPPAGE_V1' };
  id: string; dataDir: string; catalog: string; binary: NativeTarget;
  // Original v1 single-evidence inputs keep their kernel, whole interval and default weight 1.
  capital: string; currency: string; evidence: JsonObject | JsonObject[]; policy: StrategyPolicy; weights?: string[]; capacityPolicy?: NonNullable<ResearchSpecification['capital']>;
}

export async function nativeJob(binary: NativeTarget, command: string, input: JsonObject, args: string[], directory: string, context: Context): Promise<JsonObject> {
  try { return await executeNative(binary, command, input, args, directory, context); }
  catch (error) { throw error instanceof ExecutionFailure ? error : new Problem(error instanceof Error ? error.message : 'native_job_failed'); }
}

/** New independent clocks cannot change the already reviewed forecasting method. */
export function assertPortfolioIndependentContract(input: PortfolioTaskInput, data: JsonObject): void {
  const independent = normalizedRequest(data);
  const { selection: independentSelection, ...independentMethod } = object(independent.forecast, 'independent_forecast');
  for (const source of Array.isArray(input.evidence) ? input.evidence : [input.evidence]) {
    const request = object(object(object(source.selected).result).request, 'reviewed_validation_request');
    const { selection, ...method } = object(request.forecast, 'reviewed_forecast');
    if (canonical(withoutExecutionQuotas(method)) !== canonical(withoutExecutionQuotas(independentMethod))
      || canonical(object(selection).bar_types!) !== canonical(object(independentSelection).bar_types!)) throw new Problem('portfolio_independent_forecast_contract_changed', 422);
  }
  const currency = object(json(input.policy).simulationSettings).base_currency;
  if (currency !== input.currency || data.currency !== undefined && data.currency !== input.currency) throw new Problem('portfolio_independent_currency_mismatch', 422);
}

/** Original ensembles → native exact member blending → optional study blending → ONE shared account. */
export async function computePortfolio(input: PortfolioTaskInput, context: Context, scope?: JsonObject, report?: (progress: PortfolioProgress, curve?: EquityPoint[]) => Promise<void>, options?: { partition: 'VALIDATION' | 'HELDOUT'; data?: JsonObject }) {
  const partition = options?.partition;
  if (options?.data && partition === 'HELDOUT') assertPortfolioIndependentContract(input, options.data);
  const update = async (phase: string, completed?: number, total?: number, curve?: EquityPoint[]) => report?.({ phase, updatedAt: new Date().toISOString(), ...(completed === undefined ? {} : { completed }), ...(total === undefined ? {} : { total }) }, curve);
  const evidence = Array.isArray(input.evidence) ? input.evidence : [input.evidence];
  const weights = input.weights ?? (evidence.length === 1 ? ['1'] : null);
  if (!weights || !evidence.length || weights.length !== evidence.length) throw new Problem('portfolio_weights_required');
  await update('FORECASTING', 0, evidence.length, []);
  const directory = await mkdtemp(join(input.dataDir, 'native-'));
  try {
    if (input.policy.kind === 'catalog-ensemble') {
      if (evidence.length !== 1 || evidence[0]!.factorId !== undefined) throw new Problem('catalog_ensemble_single_selection_required');
      exactWeights(weights);
      const source = evidence[0]!;
      const studyInput = object(source.input, 'study_input');
      const data = options?.data ?? object(studyInput.data, 'study_data');
      const proposal = object(object(source.selected, 'selected_trial').proposal, 'selected_proposal');
      const originalPolicy = object(object(studyInput.policy, 'study_policy').strategy, 'strategy_policy');
      if (canonical(originalPolicy) !== canonical(json(input.policy))) throw new Problem('strategy_policy_mismatch');
      const final = partition === 'VALIDATION' ? object(object(source.selected, 'selected_trial').result, 'validation_result') : object(object(source.final, 'independent_final').result, 'independent_result');
      const request = object(final.registered_request ?? final.request, 'independent_request');
      const catalog = partition === 'VALIDATION' || options?.data ? String(data.catalog) : input.catalog;
      if (!options?.data && partition !== 'VALIDATION' && replayCatalog(data, request) !== input.catalog) throw new Problem('strategy_request_binding_mismatch');
      const binding = partition === 'VALIDATION' || options?.data ? data : data.sealed ? object(data.sealed, 'heldout_data') : data;
      const admitted = normalizedRequest(binding);
      const settings = object(admitted.settings, 'simulation_settings');
      if (settings.base_currency !== input.currency) throw new Problem('strategy_currency_mismatch');
      const policy = resolvePortfolioPolicy(proposal, originalPolicy);
      await update('SIMULATING');
      const result = await evaluatePortfolio({ data: { ...binding, catalog,
        catalog_ensemble_request: { ...admitted, settings: { ...settings, starting_capital: input.capital } } },
        members: strategyMembers(proposal), policy, partition: partition ?? 'HELDOUT', binary: input.binary }, context, scope);
      const simulation = object(result.evidence.simulation, 'native_simulation');
      const curve = nativeCurve(simulation, input.currency);
      await update('CAPACITY', undefined, undefined, curve);
      const native: JsonObject = { ...result.evidence, summary: result.summary };
      return { native, target: null, curve, metrics: simulationMetrics(simulation),
        capacity: unavailableCapacity(input.currency, 'catalog_ensemble_capacity_unavailable', 'portfolio', input.capacityPolicy ?? capacityPolicy(evidence)) };
    }
    const sources: JsonObject[] = []; const memberRows: JsonObject[][] = [];
    let selection: JsonObject | undefined; let originalCatalog: string | undefined;
    for (const [sourceIndex, source] of evidence.entries()) {
      const studyInput = object(source.input, 'study_input');
      const trial = object(source.selected, 'selected_trial');
      const proposal = object(trial.proposal, 'selected_proposal');
      const result = partition === 'VALIDATION' ? object(trial.result, 'validation_result') : object(object(source.final, 'independent_final').result, 'independent_result');
      const request = options?.data ? normalizedRequest(options.data) : structuredClone(object(result.request, partition === 'VALIDATION' ? 'validation_request' : 'independent_request'));
      const data = options?.data ?? (partition === 'VALIDATION' ? object(studyInput.data, 'study_data') : {});
      const sourceCatalog = partition === 'VALIDATION' || options?.data ? String(data.catalog) : input.catalog;
      if (!sourceCatalog || originalCatalog && originalCatalog !== sourceCatalog) throw new Problem('portfolio_catalog_mismatch');
      originalCatalog = sourceCatalog;
      if (partition === 'VALIDATION') {
        const sourceSelection = object(object(request.forecast, 'forecast_request').selection, 'validation_selection');
        if (typeof request.validation_end_ns !== 'string' || typeof sourceSelection.event_end_ns !== 'string' || BigInt(sourceSelection.event_end_ns) > BigInt(request.validation_end_ns)) throw new Problem('portfolio_validation_request_not_clipped');
      }
      const originalPolicy = (studyInput.policy as JsonObject | undefined)?.strategy;
      // Retain legacy consumer semantics only without original instrument/history bindings.
      const bound = Array.isArray(input.evidence) || proposal.instrumentId !== undefined || proposal.allocations !== undefined || input.policy.instrumentIds !== undefined
        || (originalPolicy as JsonObject | undefined)?.instrumentIds !== undefined || input.policy.evidenceMode !== undefined
        || (originalPolicy as JsonObject | undefined)?.evidenceMode !== undefined;
      if (bound && canonical(originalPolicy ?? null) !== canonical(json(input.policy))) throw new Problem('strategy_policy_mismatch');
      const policy = resolveStrategyPolicy(proposal, bound ? object(originalPolicy, 'strategy_policy') : json(input.policy));
      const members = strategyMembers(proposal);
      // Forecast keeps the complete original prefix even when only HELDOUT is traded.
      const targets = await forecastMembers(input.binary, sourceCatalog, members, request, policy, input.currency, directory, context, false, partition ?? policy.evaluationPartition, proposal.members !== undefined);
      await update('FORECASTING', sourceIndex + 1, evidence.length);
      if (selection && canonical(withoutExecutionQuotas(selection)) !== canonical(withoutExecutionQuotas(targets.selection))) throw new Problem('strategy_selection_mismatch');
      selection = targets.selection;
      memberRows.push(targets.points);
      const first = targets.components[0]!;
      sources.push({ researchId: source.researchId ?? null, trialId: trial.taskId ?? null, model: studyInput.model ?? null,
        policy: json(policy), members: members.map((member) => ({ ...member })), components: targets.components,
        forecastRequest: first.forecastRequest!, forecasts: first.forecasts!, target_points: targets.points,
        rows: targets.rows, weights: targets.weights, blend: targets.blend });
    }
    const combined = await blendTargets(input.binary, memberRows, weights, directory, context);
    const simulationRequest: JsonObject = { schema_version: 1, selection: selection!,
      settings: { ...object(object(sources[0]!.policy).simulationSettings), starting_capital: input.capital }, target_points: combined.points };
    await update('SIMULATING');
    const simulation = await nativeJob(input.binary, 'simulate', simulationRequest, ['--catalog', originalCatalog!], directory, context);
    const curve = nativeCurve(simulation, input.currency);
    const native: JsonObject = { catalog: originalCatalog!, sources, rows: combined.rows, weights, blend: combined.blend, simulationRequest, simulation };
    if (sources.length === 1) native.forecasts = sources[0]!.forecasts!;
    await update('CAPACITY', undefined, undefined, curve);
    const capacity: CapacityReport = await assessCapacity(input.binary, originalCatalog!, simulationRequest, input.capacityPolicy ?? capacityPolicy(evidence), directory, context);
    return { native, target: object(combined.points.at(-1), 'native_target'), curve, metrics: simulationMetrics(simulation), capacity };
  } catch (error) { throw error instanceof Problem || error instanceof ExecutionFailure ? error : new Problem(error instanceof Error ? error.message : 'portfolio_failed'); }
  finally { await rm(directory, { recursive: true, force: true }); }
}

function simulationMetrics(simulation: JsonObject): Metric[] {
  return Array.isArray(simulation.statistics) ? simulation.statistics.map(item => {
    const statistic = object(item, 'native_statistic'); const key = String(statistic.native_key);
    return { key, label: key, value: typeof statistic.value === 'number' ? statistic.value : null };
  }) : [];
}
