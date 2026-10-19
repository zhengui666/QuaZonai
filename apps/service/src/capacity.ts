import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { access, mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { CapacityReport, ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { classifyExecutionError } from '../../research/src/execution-errors.ts';
import { replayCatalog, resolveStrategyPolicy, strategyMembers } from '../../research/src/strategy.ts';
import { executeNative, forecastMembers } from '../../research/plugins/native-portfolio.ts';
import type { ServiceConfig } from './config.ts';
import { json, object } from './facts.ts';
import type { PortfolioFact } from './facts.ts';
import { readFactor } from './factors.ts';

type Assumptions = NonNullable<ResearchSpecification['capital']>;
export function unavailableCapacity(currency: string, reason: string, scope = 'portfolio', policy?: Assumptions): CapacityReport {
  const scenarios = policy?.scenarios.map(capital => ({ capital, requiredTurnover: null, maximumParticipation: null, usableCapital: null, unallocatedCash: null, status: 'UNAVAILABLE', reason,
    entryRebalanceTurnover: null, exitTurnover: null, entryRebalanceParticipation: null, exitParticipation: null })) ?? [];
  return { status: 'UNAVAILABLE', reason, scope, currency, basis: 'HISTORICAL_BAR_ESTIMATE',
    ...(policy ? { requestedAssumptions: structuredClone(policy) } : {}), maximumCapital: null, minimumCapital: null, bindingInstrument: null,
    observationStartNs: null, observationEndNs: null, decisionClockNs: null, validUntilNs: null,
    assumptions: { participation: policy?.participation ?? null, maximumAgeSeconds: policy?.maximumAgeSeconds ?? null, executionWindow: 'LAST_AVAILABLE_BAR_SHARED_BUDGET', exit: 'standalone_full_exit; future_exit_depth_unknown', fees: 'execution_costs_not_estimated', depth: 'BAR_NOTIONAL_ONLY' }, scenarios,
    volumeStress: ['1', '0.5'].map(volumeMultiplier => ({ volumeMultiplier, maximumCapital: null, status: 'UNAVAILABLE', reason, scenarios })),
    economicCapacity: { status: 'UNAVAILABLE', maximumCapital: null, reason: 'required_economic_inputs_missing', missingInputs: [
      'matched_out_of_sample_returns_at_capital_scales', 'venue_calibrated_market_impact', 'matched_spread_fees_and_execution_timing', 'validated_exit_execution'] } };
}

// Known execution failures share the public cause/remedy contract. Unknown errors
// and cancellation retain the original exception for the owning execution boundary.
function capacityFailure(error: unknown, currency: string, scope: string, policy?: Assumptions): CapacityReport {
  const errorDetail = classifyExecutionError(error, 'operation_failed');
  if (!errorDetail || errorDetail.category === 'unknown' || errorDetail.category === 'cancelled') throw error;
  return { ...unavailableCapacity(currency, errorDetail.code, scope, policy), errorDetail };
}
export function capacityPolicy(evidence: JsonObject[]): Assumptions | undefined {
  const assumptions = evidence.map(source => (object(source.input).specification as unknown as ResearchSpecification | undefined)?.capital);
  return assumptions[0] && assumptions.every(value => canonical(json(value ?? null)) === canonical(json(assumptions[0]))) ? assumptions[0] : undefined;
}
export async function assessCapacity(binary: string, catalog: string, simulation: JsonObject, assumptions: Assumptions | undefined, directory: string, context: Context): Promise<CapacityReport> {
  const currency = String(object(simulation.settings).base_currency);
  if (!assumptions) return unavailableCapacity(currency, 'explicit_capacity_policy_missing_or_incompatible');
  try {
    const report = await executeNative(binary, 'assess-capacity', { schema_version: 1, simulation, policy: { schema_version: 1,
      maximum_age_seconds: assumptions.maximumAgeSeconds, participation_limit: assumptions.participation }, capitals: assumptions.scenarios }, ['--catalog', catalog], directory, context) as unknown as CapacityReport;
    return { ...report, requestedAssumptions: structuredClone(assumptions) };
  } catch (error) {
    return capacityFailure(error, currency, 'portfolio', assumptions);
  }
}
export async function portfolioCapacity(fact: PortfolioFact, config: ServiceConfig, context: Context, assumptions: Assumptions): Promise<CapacityReport> {
  const { id, currency, status } = fact.view;
  if (status !== 'COMPUTED') return unavailableCapacity(currency, 'portfolio_not_computed', id, assumptions);
  const simulation = fact.native?.simulationRequest;
  if (!simulation || typeof simulation !== 'object' || Array.isArray(simulation)) return unavailableCapacity(currency, 'portfolio_simulation_request_missing', id, assumptions);
  const catalog = fact.native?.catalog;
  if (typeof catalog !== 'string' || !catalog.trim()) return unavailableCapacity(currency, 'portfolio_catalog_missing', id, assumptions);
  try { await access(catalog); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    return unavailableCapacity(currency, 'portfolio_catalog_missing', id, assumptions);
  }
  const directory = await mkdtemp(join(config.dataDir, 'capacity-'));
  try {
    const report = await assessCapacity(config.jobBinary, catalog, simulation, assumptions, directory, context);
    return { ...report, scope: id };
  } finally { await rm(directory, { recursive: true, force: true }); }
}
export async function factorCapacity(host: ResearchHost, config: ServiceConfig, id: string, context: Context, assumptions?: Assumptions): Promise<CapacityReport> {
  const { view, study, specification } = await readFactor(host, id);
  const capital = assumptions ?? specification?.capital;
  if (!view.combinable) return unavailableCapacity(view.currency ?? '', view.reason ?? 'independent_evidence_missing', id, capital);
  if (!capital) return unavailableCapacity(view.currency ?? '', 'explicit_capacity_policy_missing', id);
  const selected = study.trials.find(trial => trial.taskId === view.trialId)!;
  const proposal = object(selected.proposal); const input = study.input!;
  const result = object(study.final!.result); const request = object(result.request);
  const directory = await mkdtemp(join(config.dataDir, 'capacity-'));
  try {
    const catalog = input.data.sealed ? replayCatalog(input.data, request) : String(input.data.catalog);
    const policy = resolveStrategyPolicy(proposal, object(input.policy.strategy));
    const targets = await forecastMembers(config.jobBinary, catalog, strategyMembers(proposal), request, policy, view.currency!, directory, context, false, policy.evaluationPartition, proposal.members !== undefined);
    const simulation = { schema_version: 1, selection: targets.selection, settings: policy.simulationSettings, target_points: targets.points };
    const report = await assessCapacity(config.jobBinary, catalog, simulation, capital, directory, context);
    return { ...report, scope: id };
  } catch (error) {
    return capacityFailure(error, view.currency ?? '', id, capital);
  }
  finally { await rm(directory, { recursive: true, force: true }); }
}
