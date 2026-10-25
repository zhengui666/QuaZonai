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
import type { CapacityReport, Metric, ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import { assessCapacity, capacityPolicy, unavailableCapacity } from './capacity.ts';

export interface PortfolioTaskInput {
  id: string; dataDir: string; catalog: string; binary: NativeTarget;
  // Original v1 single-evidence inputs keep their kernel, whole interval and default weight 1.
  capital: string; currency: string; evidence: JsonObject | JsonObject[]; policy: StrategyPolicy; weights?: string[]; capacityPolicy?: NonNullable<ResearchSpecification['capital']>;
}

export async function nativeJob(binary: NativeTarget, command: string, input: JsonObject, args: string[], directory: string, context: Context): Promise<JsonObject> {
  try { return await executeNative(binary, command, input, args, directory, context); }
  catch (error) { throw error instanceof ExecutionFailure ? error : new Problem(error instanceof Error ? error.message : 'native_job_failed'); }
}

/** Original ensembles → native exact member blending → optional study blending → ONE shared account. */
export async function computePortfolio(input: PortfolioTaskInput, context: Context, scope?: JsonObject) {
  const evidence = Array.isArray(input.evidence) ? input.evidence : [input.evidence];
  const weights = input.weights ?? (evidence.length === 1 ? ['1'] : null);
  if (!weights || !evidence.length || weights.length !== evidence.length) throw new Problem('portfolio_weights_required');
  const directory = await mkdtemp(join(input.dataDir, 'native-'));
  try {
    if (input.policy.kind === 'catalog-ensemble') {
      if (evidence.length !== 1 || evidence[0]!.factorId !== undefined) throw new Problem('catalog_ensemble_single_selection_required');
      exactWeights(weights);
      const source = evidence[0]!;
      const studyInput = object(source.input, 'study_input');
      const data = object(studyInput.data, 'study_data');
      const proposal = object(object(source.selected, 'selected_trial').proposal, 'selected_proposal');
      const originalPolicy = object(object(studyInput.policy, 'study_policy').strategy, 'strategy_policy');
      if (canonical(originalPolicy) !== canonical(json(input.policy))) throw new Problem('strategy_policy_mismatch');
      const final = object(object(source.final, 'independent_final').result, 'independent_result');
      const request = object(final.registered_request ?? final.request, 'independent_request');
      if (replayCatalog(data, request) !== input.catalog) throw new Problem('strategy_request_binding_mismatch');
      const binding = data.sealed ? object(data.sealed, 'heldout_data') : data;
      const admitted = normalizedRequest(binding);
      const settings = object(admitted.settings, 'simulation_settings');
      if (settings.base_currency !== input.currency) throw new Problem('strategy_currency_mismatch');
      const policy = resolvePortfolioPolicy(proposal, originalPolicy);
      const result = await evaluatePortfolio({ data: { ...binding, catalog: input.catalog,
        catalog_ensemble_request: { ...admitted, settings: { ...settings, starting_capital: input.capital } } },
        members: strategyMembers(proposal), policy, partition: 'HELDOUT', binary: input.binary }, context, scope);
      const simulation = object(result.evidence.simulation, 'native_simulation');
      const native: JsonObject = { ...result.evidence, summary: result.summary };
      return { native, target: null, curve: nativeCurve(simulation, input.currency), metrics: simulationMetrics(simulation),
        capacity: unavailableCapacity(input.currency, 'catalog_ensemble_capacity_unavailable', 'portfolio', input.capacityPolicy ?? capacityPolicy(evidence)) };
    }
    const sources: JsonObject[] = []; const memberRows: JsonObject[][] = [];
    let selection: JsonObject | undefined;
    for (const source of evidence) {
      const studyInput = object(source.input, 'study_input');
      const trial = object(source.selected, 'selected_trial');
      const proposal = object(trial.proposal, 'selected_proposal');
      const result = object(object(source.final, 'independent_final').result, 'independent_result');
      const request = structuredClone(object(result.request, 'independent_request'));
      const originalPolicy = (studyInput.policy as JsonObject | undefined)?.strategy;
      // Retain legacy consumer semantics only without original instrument/history bindings.
      const bound = Array.isArray(input.evidence) || proposal.instrumentId !== undefined || proposal.allocations !== undefined || input.policy.instrumentIds !== undefined
        || (originalPolicy as JsonObject | undefined)?.instrumentIds !== undefined || input.policy.evidenceMode !== undefined
        || (originalPolicy as JsonObject | undefined)?.evidenceMode !== undefined;
      if (bound && canonical(originalPolicy ?? null) !== canonical(json(input.policy))) throw new Problem('strategy_policy_mismatch');
      const policy = resolveStrategyPolicy(proposal, bound ? object(originalPolicy, 'strategy_policy') : json(input.policy));
      const members = strategyMembers(proposal);
      // Forecast keeps the complete original prefix even when only HELDOUT is traded.
      const targets = await forecastMembers(input.binary, input.catalog, members, request, policy, input.currency, directory, context, false, policy.evaluationPartition, proposal.members !== undefined);
      if (selection && canonical(selection) !== canonical(targets.selection)) throw new Problem('strategy_selection_mismatch');
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
    const simulation = await nativeJob(input.binary, 'simulate', simulationRequest, ['--catalog', input.catalog], directory, context);
    const curve = nativeCurve(simulation, input.currency);
    const native: JsonObject = { catalog: input.catalog, sources, rows: combined.rows, weights, blend: combined.blend, simulationRequest, simulation };
    if (sources.length === 1) native.forecasts = sources[0]!.forecasts!;
    const capacity: CapacityReport = await assessCapacity(input.binary, input.catalog, simulationRequest, input.capacityPolicy ?? capacityPolicy(evidence), directory, context);
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
