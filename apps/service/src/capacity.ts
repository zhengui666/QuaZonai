import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { CapacityReport, ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { replayCatalog, resolveStrategyPolicy, strategyMembers } from '../../research/src/strategy.ts';
import { executeNative, forecastMembers } from '../../research/plugins/native-portfolio.ts';
import type { ServiceConfig } from './config.ts';
import { json, object } from './facts.ts';
import { readFactor } from './factors.ts';

type Assumptions = NonNullable<ResearchSpecification['capital']>;
export function unavailableCapacity(currency: string, reason: string, scope = 'portfolio', policy?: Assumptions): CapacityReport {
  return { status: 'UNAVAILABLE', reason, scope, currency, basis: 'HISTORICAL_BAR_ESTIMATE', maximumCapital: null, minimumCapital: null, bindingInstrument: null,
    observationStartNs: null, observationEndNs: null, decisionClockNs: null, validUntilNs: null,
    assumptions: { participation: policy?.participation ?? null, maximumAgeSeconds: policy?.maximumAgeSeconds ?? null, executionWindow: 'ONE_LAST_AVAILABLE_BAR', exit: 'future_exit_depth_unknown', fees: 'original_native_simulation_settings', depth: 'BAR_NOTIONAL_ONLY' }, scenarios: policy?.scenarios.map(capital => ({ capital, requiredTurnover: null, maximumParticipation: null, usableCapital: null, unallocatedCash: null, status: 'UNAVAILABLE', reason })) ?? [] };
}
export function capacityPolicy(evidence: JsonObject[]): Assumptions | undefined {
  const assumptions = evidence.map(source => (object(source.input).specification as unknown as ResearchSpecification | undefined)?.capital);
  return assumptions[0] && assumptions.every(value => canonical(json(value ?? null)) === canonical(json(assumptions[0]))) ? assumptions[0] : undefined;
}
export async function assessCapacity(binary: string, catalog: string, simulation: JsonObject, assumptions: Assumptions | undefined, directory: string, context: Context): Promise<CapacityReport> {
  const currency = String(object(simulation.settings).base_currency);
  if (!assumptions) return unavailableCapacity(currency, 'explicit_capacity_policy_missing_or_incompatible');
  try {
    return await executeNative(binary, 'assess-capacity', { schema_version: 1, simulation, policy: { schema_version: 1,
      maximum_age_seconds: assumptions.maximumAgeSeconds, participation_limit: assumptions.participation }, capitals: assumptions.scenarios }, ['--catalog', catalog], directory, context) as unknown as CapacityReport;
  } catch {
    return unavailableCapacity(currency, 'native_capacity_unavailable', 'portfolio', assumptions);
  }
}
export async function factorCapacity(host: ResearchHost, config: ServiceConfig, id: string, context: Context, assumptions?: Assumptions): Promise<CapacityReport> {
  const { view, study, specification } = await readFactor(host, id);
  const capital = assumptions ?? specification?.capital;
  if (!view.combinable) return unavailableCapacity(view.currency ?? '', view.reason ?? 'independent_evidence_missing', id, capital);
  if (!capital) return unavailableCapacity(view.currency ?? '', 'explicit_capacity_policy_missing', id);
  const selected = study.trials.find(trial => trial.taskId === view.trialId)!;
  const proposal = object(selected.proposal); const input = study.input!;
  const result = object(study.final!.result); const request = object(result.request);
  const catalog = input.data.sealed ? replayCatalog(input.data, request) : String(input.data.catalog);
  const directory = await mkdtemp(join(config.dataDir, 'capacity-'));
  try {
    const policy = resolveStrategyPolicy(proposal, object(input.policy.strategy));
    const targets = await forecastMembers(config.jobBinary, catalog, strategyMembers(proposal), request, policy, view.currency!, directory, context, false, policy.evaluationPartition, proposal.members !== undefined);
    const simulation = { schema_version: 1, selection: targets.selection, settings: policy.simulationSettings, target_points: targets.points };
    const report = await assessCapacity(config.jobBinary, catalog, simulation, capital, directory, context);
    return { ...report, scope: id };
  } catch { return unavailableCapacity(view.currency ?? '', 'factor_execution_mapping_or_data_unavailable', id, capital); }
  finally { await rm(directory, { recursive: true, force: true }); }
}
