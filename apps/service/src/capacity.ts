import { scienceTarget } from './config.ts';
import { withNativeStep } from '../../research/src/native-execution.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
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
  const catalog = reason === 'catalog_ensemble_capacity_unavailable';
  const scenarios = policy?.scenarios.map(capital => ({ capital, requiredTurnover: null, maximumParticipation: null, usableCapital: null, unallocatedCash: null, status: 'UNAVAILABLE', reason,
    entryRebalanceTurnover: null, exitTurnover: null, entryRebalanceParticipation: null, exitParticipation: null })) ?? [];
  return { status: 'UNAVAILABLE', reason, scope, currency, basis: catalog ? 'NOT_ASSESSED' : 'HISTORICAL_BAR_ESTIMATE',
    ...(policy ? { requestedAssumptions: structuredClone(policy) } : {}), maximumCapital: null, minimumCapital: null, bindingInstrument: null,
    observationStartNs: null, observationEndNs: null, decisionClockNs: null, validUntilNs: null,
    assumptions: { participation: policy?.participation ?? null, maximumAgeSeconds: policy?.maximumAgeSeconds ?? null, executionWindow: catalog ? 'NOT_ASSESSED' : 'LAST_AVAILABLE_BAR_SHARED_BUDGET', exit: 'standalone_full_exit; future_exit_depth_unknown', fees: 'execution_costs_not_estimated', depth: catalog ? 'NOT_ASSESSED' : 'BAR_NOTIONAL_ONLY' }, scenarios,
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
export async function assessCapacity(binary: NativeTarget, catalog: string, simulation: JsonObject, assumptions: Assumptions | undefined, directory: string, context: Context): Promise<CapacityReport> {
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
/** Compatibility helpers use the same captured plan as the managed HTTP Task. */
export async function portfolioCapacity(fact: PortfolioFact, config: ServiceConfig, context: Context, assumptions: Assumptions): Promise<CapacityReport> {
  return computeCapacityPlan(await capacityPlan(undefined, 'PORTFOLIO', fact.view.id, assumptions, fact), scienceTarget(config), config.dataDir, context);
}
export async function factorCapacity(host: ResearchHost, config: ServiceConfig, id: string, context: Context, assumptions?: Assumptions): Promise<CapacityReport> {
  return computeCapacityPlan(await capacityPlan(host, 'FACTOR', id, assumptions), scienceTarget(config), config.dataDir, context);
}

/** Captured scientific facts for one admitted capacity operation. */
export type CapacityPlan = {
  scope: string; currency: string; assumptions?: Assumptions; unavailable?: CapacityReport;
  catalog?: string; simulation?: JsonObject;
  forecast?: { members: ReturnType<typeof strategyMembers>; request: JsonObject; policy: ReturnType<typeof resolveStrategyPolicy>; ensemble: boolean };
};
export async function capacityPlan(host: ResearchHost | undefined, kind: 'FACTOR' | 'PORTFOLIO', id: string, assumptions?: Assumptions, fact?: PortfolioFact): Promise<CapacityPlan> {
  if (kind === 'PORTFOLIO') {
    if (!fact || fact.view.id !== id) throw new Error('portfolio_not_found');
    const { currency, status } = fact.view;
    const plan: CapacityPlan = { scope: id, currency, assumptions };
    const unavailable = (reason: string) => ({ ...plan, unavailable: unavailableCapacity(currency, reason, id, assumptions) });
    if (status !== 'COMPUTED') return unavailable('portfolio_not_computed');
    if (fact.policy.kind === 'catalog-ensemble') return unavailable('catalog_ensemble_capacity_unavailable');
    const simulation = fact.native?.simulationRequest;
    if (!simulation || typeof simulation !== 'object' || Array.isArray(simulation)) return unavailable('portfolio_simulation_request_missing');
    if (typeof fact.native?.catalog !== 'string' || !fact.native.catalog.trim()) return unavailable('portfolio_catalog_missing');
    return { ...plan, catalog: fact.native.catalog, simulation: structuredClone(simulation) };
  }
  if (!host) throw new Error('capacity_host_required');
  const { view, study, specification } = await readFactor(host, id);
  const capital = assumptions ?? specification?.capital;
  const plan: CapacityPlan = { scope: id, currency: view.currency ?? '', assumptions: capital };
  const unavailable = (reason: string) => ({ ...plan, unavailable: unavailableCapacity(plan.currency, reason, id, capital) });
  if (study.input?.data.catalog_ensemble_request !== undefined) return unavailable('catalog_ensemble_capacity_unavailable');
  if (!view.combinable) return unavailable(view.reason ?? 'independent_evidence_missing');
  if (!capital) return unavailable('explicit_capacity_policy_missing');
  const selected = study.trials.find(trial => trial.taskId === view.trialId)!;
  const proposal = object(selected.proposal); const input = study.input!;
  const request = object(object(study.final!.result).request);
  const policy = resolveStrategyPolicy(proposal, object(input.policy.strategy));
  return structuredClone({ ...plan, catalog: input.data.sealed ? replayCatalog(input.data, request) : String(input.data.catalog),
    forecast: { members: strategyMembers(proposal), request, policy, ensemble: proposal.members !== undefined } });
}
export async function computeCapacityPlan(plan: CapacityPlan, target: NativeTarget, dataDir: string, context: Context): Promise<CapacityReport> {
  if (plan.unavailable) return structuredClone(plan.unavailable);
  if (!plan.forecast) {
    try { await access(plan.catalog!); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; return unavailableCapacity(plan.currency, 'portfolio_catalog_missing', plan.scope, plan.assumptions); }
  }
  const directory = await mkdtemp(join(dataDir, 'capacity-'));
  try {
    let simulation = plan.simulation;
    if (plan.forecast) {
      const { members, request, policy, ensemble } = plan.forecast;
      const targets = await withNativeStep('forecast-members', () => forecastMembers(target, plan.catalog!, members, request, policy, plan.currency, directory, context, false, policy.evaluationPartition, ensemble));
      simulation = { schema_version: 1, selection: targets.selection, settings: policy.simulationSettings, target_points: targets.points };
    }
    const report = await withNativeStep('assess-capacity', () => assessCapacity(target, plan.catalog!, simulation!, plan.assumptions, directory, context));
    return { ...report, scope: plan.scope };
  } catch (error) { return capacityFailure(error, plan.currency, plan.scope, plan.assumptions); }
  finally { await rm(directory, { recursive: true, force: true }); }
}
