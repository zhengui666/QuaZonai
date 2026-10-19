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
  const scenarios = policy?.scenarios.map(capital => ({ capital, requiredTurnover: null, maximumParticipation: null, usableCapital: null, unallocatedCash: null, status: 'UNAVAILABLE', reason,
    entryRebalanceTurnover: null, exitTurnover: null, entryRebalanceParticipation: null, exitParticipation: null })) ?? [];
  return { status: 'UNAVAILABLE', reason, scope, currency, basis: 'HISTORICAL_BAR_ESTIMATE', maximumCapital: null, minimumCapital: null, bindingInstrument: null,
    observationStartNs: null, observationEndNs: null, decisionClockNs: null, validUntilNs: null,
    assumptions: { participation: policy?.participation ?? null, maximumAgeSeconds: policy?.maximumAgeSeconds ?? null, executionWindow: 'LAST_AVAILABLE_BAR_SHARED_BUDGET', exit: 'standalone_full_exit; future_exit_depth_unknown', fees: 'execution_costs_not_estimated', depth: 'BAR_NOTIONAL_ONLY' }, scenarios,
    volumeStress: ['1', '0.5'].map(volumeMultiplier => ({ volumeMultiplier, maximumCapital: null, status: 'UNAVAILABLE', reason, scenarios })),
    economicCapacity: { status: 'UNAVAILABLE', maximumCapital: null, reason: 'required_economic_inputs_missing', missingInputs: [
      'matched_out_of_sample_returns_at_capital_scales', 'venue_calibrated_market_impact', 'matched_spread_fees_and_execution_timing', 'validated_exit_execution'] } };
}

// Preserve known scientific causes without exposing arbitrary native text. Unknown errors,
// including cancellation, retain the original exception for the owning execution boundary.
const capacityFailures = new Set([
  'capacity_scenarios_required', 'capacity_capital_invalid', 'capacity_decisions_invalid', 'capacity_target_invalid', 'capacity_target_sum_invalid',
  'CATALOG_EMPTY_SELECTION', 'CATALOG_SELECTION_INVALID', 'CATALOG_ROOT_INVALID', 'CATALOG_INSTRUMENT_MISSING', 'CATALOG_EXTRA_INSTRUMENT',
  'CATALOG_PARQUET_FOOTER_INVALID', 'CATALOG_NATIVE_TYPE_MISMATCH', 'CATALOG_VOLUME_UNDEFINED', 'CATALOG_PRICE_UNDEFINED', 'INSTRUMENT_DEFINITION_FROM_FUTURE',
  'CATALOG_TIME_INVALID', 'CATALOG_NONUNIQUE_OR_REVISED_BAR', 'CATALOG_ROW_LIMIT', 'CATALOG_PRICE_PRECISION_MISMATCH', 'CATALOG_OHLC_INVALID',
  'SIGNAL_REQUIRES_WASM_BINARY', 'SIGNAL_MODULE_INVALID', 'SIGNAL_IMPORTS_FORBIDDEN', 'SIGNAL_ABI_MISMATCH', 'SIGNAL_INSTANTIATION_REJECTED',
  'SIGNAL_EXECUTION_FAILED', 'SIGNAL_BUDGET_EXHAUSTED', 'NATIVE_DECIMAL_RANGE', 'rolling_liquidity_policy',
  'strategy_adapter_unsupported', 'strategy_forecast_mode_invalid', 'strategy_evaluation_partition_invalid', 'strategy_evidence_mode_invalid',
  'strategy_instrument_universe_invalid', 'strategy_base_instrument_not_allowed', 'strategy_instrument_not_allowed', 'strategy_instrument_invalid',
  'strategy_instrument_fee_missing', 'strategy_instrument_fee_duplicate', 'strategy_request_binding_mismatch', 'strategy_observations_missing',
  'strategy_market_observations_missing', 'strategy_instrument_bars_missing', 'strategy_target_alignment_mismatch', 'strategy_clock_invalid',
  'selected_factor_model_required', 'strategy_members_required', 'portfolio_weights_invalid', 'catalog_required',
]);
function capacityFailure(error: unknown): string | undefined {
  if (!(error instanceof Error)) return undefined;
  return error.message.split(/\s+/).find(code => capacityFailures.has(code));
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
  } catch (error) {
    const reason = capacityFailure(error);
    if (!reason) throw error;
    return unavailableCapacity(currency, reason, 'portfolio', assumptions);
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
  const directory = await mkdtemp(join(config.dataDir, 'capacity-'));
  try {
    const catalog = input.data.sealed ? replayCatalog(input.data, request) : String(input.data.catalog);
    const policy = resolveStrategyPolicy(proposal, object(input.policy.strategy));
    const targets = await forecastMembers(config.jobBinary, catalog, strategyMembers(proposal), request, policy, view.currency!, directory, context, false, policy.evaluationPartition, proposal.members !== undefined);
    const simulation = { schema_version: 1, selection: targets.selection, settings: policy.simulationSettings, target_points: targets.points };
    const report = await assessCapacity(config.jobBinary, catalog, simulation, capital, directory, context);
    return { ...report, scope: id };
  } catch (error) {
    const reason = capacityFailure(error);
    if (!reason) throw error;
    return unavailableCapacity(view.currency ?? '', reason, id, capital);
  }
  finally { await rm(directory, { recursive: true, force: true }); }
}
