import { boundScienceTarget } from './science-binding.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { compareOperatingCandidates, projectNativeValidationMetrics } from '../../../packages/contracts/src/decision.ts';
import { addDecimal, compareDecimal, multiplyDecimal, subtractDecimal, validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';
import type { ServiceConfig } from './config.ts';
import { json, nowNs, object, Problem } from './facts.ts';
import { fact, identity } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';
import { pmMetricDefinitions, pmDiagnosticDefinitions } from './pm-registry.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { canonical } from '../../research/src/documents.ts';
import { computePmAccountScience, computePmScience } from './pm-science.ts';
import type { PmAccountScienceResult, PmScienceResult } from './pm-science.ts';
import type { CapturedAccount } from './pm-account.ts';
import { pmArtifactDigest } from './pm-evidence.ts';
import { assertPmDataAvailable } from './pm-datasets.ts';
import type { PmScienceCheckpoint } from './pm-science.ts';

export interface PmDecisionResult {
  decision: Pm.PortfolioDecision;
  comparison: Pm.OperatingComparison;
  capacities: Record<string, Pm.CapacityEstimate>;
  ensembles: Record<string, Pm.EnsembleRevision>;
  evidence: Record<string, Pm.EvidenceRef>;
  artifacts: Record<string, JsonObject>;
  trials: Record<string, Pm.ResearchTrial>;
  budgetMovements: Record<string, Pm.BudgetMovement>;
  scienceCompleted: boolean;
  trialSettlements: { state: Pm.ResearchTrial['state']; countedAttempt: boolean; evidenceRefs: string[]; errorCode: string | null }[];
  gaps: { capability: string; reason: string; remedy: Pm.EvidenceGap['permittedRemedy'] }[];
}

/** No metric, capital bound, simulated holding or target is fabricated on failure. */
export async function computePmDecision(record: PmRecord, cycle: Pm.DecisionCycle, config: ServiceConfig, context: Context, checkpoint?: PmScienceCheckpoint): Promise<PmDecisionResult> {
  const frozen = record.artifacts[cycle.id]!;
  const registered = object(frozen.configured, 'registered_dataset'); const data = object(registered.data, 'native_dataset');
  const datasetRevision = registered.revision as unknown as Pm.DatasetRevision;
  const request = object(data.catalog_ensemble_request ?? data.catalog_scan_request ?? data.request, 'native_request');
  const interval = { startNs: String(data.train_end_ns ?? request.train_end_ns), endExclusiveNs: String(data.validation_end_ns ?? request.validation_end_ns) };
  const isAccount = record.mandate.capitalSource.kind === 'ACCOUNT';
  let capital = record.mandate.capitalSource.kind === 'RESEARCH_SCENARIO' ? record.mandate.capitalSource.capital : cycle.accountBasis?.equity ?? null;
  const scope: Pm.ComparisonScope = { id: identity('operating-scope', cycle.id), allowedUniverseRef: record.mandate.allowedUniverseRef, datasetRevisionIds: cycle.datasetRevisionIds,
    interval, role: 'SELECTION', calendarId: 'UTC_COMPLETE_DAYS', labelDefinitionId: null, accountingPolicyRef: 'qz.single-currency-shared-account/1',
    costModelRef: identity('cost', registered.strategy), startingStateRef: identity('research-initial-condition', [record.mandate.id, capital]), capital };
  const comparison: Pm.OperatingComparison = { ...fact(identity('operating-comparison', cycle.id)), cycleId: cycle.id, basisRevision: cycle.basisRevision,
    policyRevisionId: record.policy.id, scope, incumbentDecisionId: typeof frozen.incumbentDecisionId === 'string' ? frozen.incumbentDecisionId : null,
    evaluationContext: 'HISTORICAL_JOINT_REPLAY', candidates: [], pairedDifferenceEvidenceRefs: [], selectedCandidateId: null, conclusion: 'NEEDS_EVIDENCE', reasonCodes: [] };
  const decision: Pm.PortfolioDecision = { ...fact(identity('decision', cycle.id)), cycleId: cycle.id, mandateRevisionId: record.mandate.id,
    strategyVersionId: cycle.strategyVersionId, ensembleRevisionId: null, strategyAllocations: [], policyRevisionId: record.policy.id, basisRevision: cycle.basisRevision,
    incumbentDecisionId: comparison.incumbentDecisionId, accountBasis: cycle.accountBasis, researchStartingStateRef: cycle.accountBasis ? null : scope.startingStateRef,
    action: 'NEEDS_EVIDENCE', status: 'CANDIDATE', assetTargets: [], capital: null, comparisonRef: comparison.id, solverEvidenceRef: null, jointReplayEvidenceRef: null,
    reasonCodes: [], explanationFacts: { summary: 'Awaiting comparable scientific evidence.', drivers: [] }, validUntilNs: null,
    nextInvalidationDependencies: cycle.dependencyRevisionRefs, targetCommandId: null };
  const result: PmDecisionResult = { decision, comparison, capacities: {}, ensembles: {}, evidence: {}, artifacts: {}, trials: {}, budgetMovements: {}, gaps: [], scienceCompleted: false, trialSettlements: [] };
  const unavailable = (reason: string, capability = 'joint_portfolio_evidence', remedy: Pm.EvidenceGap['permittedRemedy'] = 'RECOMPUTE') => {
    result.comparison.reasonCodes.push(reason); decision.reasonCodes.push(reason); result.gaps.push({ capability, reason, remedy });
    decision.explanationFacts = { ...object(decision.explanationFacts), summary: 'No new investment allocation is supported by the captured evidence.', drivers: [...decision.reasonCodes] }; return result;
  };
  if (Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && item.reasonCode === 'pm_account_consumer_not_requested')) { const missing = unavailable('pm_account_consumer_not_requested', 'owner_account_decision_intent', 'CLARIFY_INTENT'); decision.explanationFacts = { summary: 'Independent factor research is available through the research workflow; this mandate requires explicit account or capital decision intent.', drivers: ['pm_account_consumer_not_requested'], researchEntry: '/research' }; return missing; }
  if (Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && item.requiredCapability === 'source_revision')) return unavailable('source_revision_verification_required', 'source_revision', 'REFRESH_SOURCE');
  assertPmDataAvailable(data, cycle.availableCutoffNs);
  if (BigInt(datasetRevision.availableThroughNs) > BigInt(cycle.availableCutoffNs)) return unavailable('dataset_native_availability_after_decision_cutoff', 'point_in_time_data', 'REFRESH_SOURCE');
  if (isAccount && !cycle.accountBasis) return unavailable('authoritative_account_basis_missing', 'matched_account_starting_state', 'REFRESH_SOURCE');
  if (Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && ['owner_boundary_normalization_required', 'strategy_intake_needs_clarification', 'strategy_archetype_unsupported'].includes(item.reasonCode))) return unavailable('owner_intent_requires_clarification', 'owner_boundary_normalization', 'CLARIFY_INTENT');
  if (!capital && !isAccount) return unavailable('capital_basis_unavailable', 'capital_basis', 'REFRESH_SOURCE');
  if (frozen.budgetUnavailable) return unavailable('shared_trial_budget_exhausted', 'shared_research_budget', 'CLARIFY_INTENT');
  if (!frozen.nativeSource) return unavailable('selected_fitted_joint_replay_missing');
  const source = object(frozen.nativeSource, 'fitted_native_source');
  const sourceRequest = object(source.request, 'fitted_source_request');
  const studyInput = object(object(frozen.research, 'frozen_research').input, 'frozen_research_input');
  if (canonical(object(studyInput.data, 'frozen_research_data')) !== canonical(data) || canonical(sourceRequest) !== canonical(iterationRequest(data, true))) return unavailable('fitted_evidence_dataset_revision_mismatch');
  const fit = object(source.fit, 'frozen_fit');
  if (!Array.isArray(fit.members) || !fit.members.length) return unavailable('fitted_factor_members_missing');
  const ensembleId = identity('ensemble', [cycle.strategyVersionId, fit]);
  const fitEvidenceId = identity('evidence', [ensembleId, 'fit']);
  const evidence = (id: string, artifact: JsonObject, role: Pm.EvidenceRole): string => {
    const atNs = nowNs();
    result.artifacts[id] = artifact;
    result.evidence[id] = { id, sourceKind: 'NATIVE_RESULT', role, runtime: cycle.runtime, objectKey: id,
      contentDigest: pmArtifactDigest(artifact), mediaType: 'application/json', sourceRevision: cycle.basisRevision, eventInterval: interval,
      availableAtNs: atNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.pm.native-evidence/1', visibility: isAccount || role === 'INDEPENDENT' || role === 'OBSERVED' ? 'OWNER_ONLY' : 'RESEARCH_VISIBLE' };
    return id;
  };
  const unsupportedBoundary = (reason: string, capability: string) => {
    const ref = evidence(identity('boundary-capability', cycle.id), json({ phase: 'BEFORE_NATIVE_SCIENCE', reason,
      ownerBoundaries: record.mandate.ownerBoundaries, originalSettings: object(source.simulationRequest).settings }), 'CONTROL');
    // No scientific configuration has executed at this capability boundary.
    // Release only this cycle's admitted but unexecuted downstream slots.
    result.trialSettlements = Array.from({ length: Number(frozen.maximumTrials ?? 0) }, () => ({ state: 'REJECTED' as const, countedAttempt: false, evidenceRefs: [ref], errorCode: reason }));
    return unavailable(reason, capability, 'UNSUPPORTED');
  };
  for (const boundary of record.mandate.ownerBoundaries) {
    if (boundary.kind !== 'LEVERAGE') return unsupportedBoundary('owner_boundary_execution_unavailable', 'owner_boundary_execution');
    const value = object(boundary.value); const settings = object(object(source.simulationRequest).settings);
    if (value.borrowingAllowed === false && (settings.account_kind !== 'CASH' || typeof settings.leverage !== 'string' || compareDecimal(settings.leverage, '1') > 0)) {
      return unsupportedBoundary('no_borrow_boundary_requires_native_cash_account', 'native_no_borrow_account');
    }
  }
  evidence(fitEvidenceId, fit, 'TRAINING');
  result.evidence[fitEvidenceId]!.eventInterval = { startNs: String(object(object(sourceRequest.forecast).selection).event_start_ns), endExclusiveNs: String(fit.fit_end_ns) };
  const ensemble: Pm.EnsembleRevision = { ...fact(ensembleId), strategyVersionId: cycle.strategyVersionId, policyRevisionId: record.policy.id,
    members: fit.members.map(value => { const row = object(value); return { factorVersionId: String(row.factor_version_id), calibrationModelId: null,
      weight: String(row.weight), economicFamilyId: record.definition.id, contributionEvidenceRef: null }; }), predictionUnit: 'RETURN_PER_HORIZON',
    labelDefinitionId: identity('label', object(fit.forecast_contract).parameters), fitProtocolRef: record.policy.trainingProtocolRef, fitCutoffNs: String(fit.fit_end_ns),
    trainingEvidenceRefs: [fitEvidenceId], jointValidationEvidenceRefs: [], qualification: 'CANDIDATE', rejectionReasonCodes: [] };
  result.ensembles[ensembleId] = ensemble; decision.ensembleRevisionId = ensembleId;
  const common = { source, currency: capital?.currency ?? cycle.accountBasis!.currency, catalog: String(data.catalog), dataDir: config.dataDir,
    binary: boundScienceTarget(config, data), maximumTrials: Number(frozen.maximumTrials ?? 0), cycleId: cycle.id,
    ownerBoundaries: record.mandate.ownerBoundaries,
    allocationMethod: object(object(record.policy.validDomain).jointAllocationMethod, 'joint_allocation_method') };
  let accountScience: PmAccountScienceResult | null = null;
  let researchScience: PmScienceResult | null = null;
  if (isAccount) {
    const basis = cycle.accountBasis!; const ref = record.evidence[basis.positionsEvidenceRef];
    if (!ref || !record.artifacts[ref.objectKey]) return unavailable('captured_account_original_evidence_missing', 'matched_account_starting_state', 'REFRESH_SOURCE');
    const capture: CapturedAccount = { basis, evidence: ref, artifact: record.artifacts[ref.objectKey]! };
    accountScience = await computePmAccountScience({ ...common, capture, datasetRevision: { id: datasetRevision.id, contentRef: datasetRevision.contentRef } }, context, checkpoint);
    capital = accountScience.capital;
    scope.capital = capital; scope.accountingPolicyRef = 'qz.observed-multi-currency-spot-account/1'; scope.startingStateRef = ref.id;
  } else researchScience = await computePmScience({ ...common, capital: capital! }, context, checkpoint);
  if (!capital) throw new Problem('native_account_valuation_missing');
  let valuationRef: string | null = null;
  if (accountScience) {
    // Derived account value is an observed-state fact independent of whether
    // the frozen preference resolves the investment comparison.
    valuationRef = evidence(identity('account-valuation', cycle.id), accountScience.valuation as unknown as JsonObject, 'OBSERVED');
    result.evidence[valuationRef]!.eventInterval = null; // A point-in-time inventory valuation, not a period return.
    decision.accountBasis = { ...cycle.accountBasis!, valuation: { origin: 'DERIVED', reportValue: capital, valuationEvidenceRef: valuationRef,
      methodRef: 'qz.native.observed-spot-pit-valuation/1', asOfNs: accountScience.valuation.asOfNs, availableAtNs: accountScience.valuation.availableAtNs } };
    decision.explanationFacts = { ...object(decision.explanationFacts), valuationEvidenceRef: valuationRef };
  }
  const science = accountScience ?? researchScience!;
  const firstHold = accountScience?.points[0]?.hold;
  const baseline = accountScience ? firstHold ? { kind: 'HOLD' as const, simulationRequest: firstHold.simulationRequest, simulation: firstHold.result, status: firstHold.status, reason: firstHold.reason }
    : null : { kind: 'CASH' as const, simulationRequest: researchScience!.cash.simulationRequest, simulation: researchScience!.cash.simulation, status: 'COMPLETED' as const, reason: null };
  if (!baseline) throw new Problem('matched_account_baseline_unavailable');
  result.scienceCompleted = true;
  evidence(identity('science-inputs', cycle.id), science.evidence, 'SELECTION');
  const scientificBindings = Array.isArray(science.evidence.bindings) ? science.evidence.bindings.map(item => object(item)) : [];
  const scientificObjects = object(science.evidence.objects, 'original_native_object_bindings');
  for (const member of ensemble.members) {
    const matches = scientificBindings.filter(binding => binding.factorVersionId === member.factorVersionId);
    if (matches.length !== 1 || typeof matches[0]!.calibrationArtifactId !== 'string' || typeof matches[0]!.reportArtifactId !== 'string') throw new Problem('native_calibration_binding_missing_or_ambiguous');
    const binding = matches[0]!;
    member.calibrationModelId = String(binding.calibrationArtifactId);
    for (const id of [String(binding.calibrationArtifactId), String(binding.reportArtifactId)]) {
      evidence(id, object(scientificObjects[id], 'original_native_training_object'), 'TRAINING');
      result.evidence[id]!.eventInterval = { ...result.evidence[fitEvidenceId]!.eventInterval! };
      ensemble.trainingEvidenceRefs.push(id);
    }
  }
  const executedSelection = object(baseline.simulationRequest.selection, 'comparison_selection');
  interval.startNs = String(executedSelection.event_start_ns); interval.endExclusiveNs = String(executedSelection.event_end_ns);
  if (BigInt(interval.startNs) < BigInt(String(data.train_end_ns ?? request.train_end_ns)) || BigInt(interval.endExclusiveNs) > BigInt(String(data.validation_end_ns ?? request.validation_end_ns))) throw new Problem('native_replay_outside_registered_validation');
  for (const point of science.points) if (point.simulationRequest) {
    const selection = object(point.simulationRequest.selection, 'point_selection');
    if (selection.event_start_ns !== interval.startNs || selection.event_end_ns !== interval.endExclusiveNs) throw new Problem('native_operating_comparison_scope_mismatch');
  }
  const capacityId = identity('capacity', cycle.id);
  const capacity: Pm.CapacityEstimate = { ...fact(capacityId), strategyVersionId: cycle.strategyVersionId, ensembleRevisionId: ensembleId,
    basisRevision: cycle.basisRevision, level: 'CONDITIONAL_FEASIBILITY', status: 'INSUFFICIENT', currency: capital.currency, observedInterval: interval,
    asOfNs: cycle.asOfNs, validUntilNs: null, minimumTestedCapital: null, maximumTestedCapital: null, supportedCapitalUpperBound: null, feasibleRegions: [], curve: [],
    liquidityModelRef: null, impactModelRef: null, executionModelRef: identity('execution-model', source.policy), exitModelRef: null, evidenceRefs: [],
    assumptions: science.limitations, missingEvidence: ['economic_impact_and_exit_capacity_unverified', 'historical_replay_does_not_establish_live_fill_capacity'] };
  result.capacities[capacityId] = capacity;
  const pointResults = new Map<string, typeof science.points[number]>();
  const definitions = [...pmMetricDefinitions(), ...pmDiagnosticDefinitions()];
  const requiredMetricsAvailable = (metrics: Pm.MetricObservation[]) => record.policy.selectionRule.objectives.every(objective => metrics.filter(metric => metric.definition.id === objective.metricDefinitionId && metric.status === 'AVAILABLE').length === 1);
  const metricsFor = (simulation: JsonObject, evidenceId: string) => {
    const report = isAccount ? object(simulation.spot_cash_report, 'native_spot_account_report') : simulation;
    if (isAccount && object(report.daily_returns).report_currency !== capital!.currency) throw new Problem('native_spot_report_currency_mismatch');
    return projectNativeValidationMetrics({ portfolio: { partition: 'VALIDATION', statistics: report.statistics ?? [], returns_status: isAccount ? null : simulation.returns_status ?? null, recorded_fee_totals: simulation.recorded_fee_totals ?? null } }, definitions,
      { role: 'SELECTION', interval, asOfNs: cycle.asOfNs, availableAtNs: nowNs(), evidenceRefs: [evidenceId], methodRef: record.policy.id, currency: capital!.currency, calendarId: scope.calendarId });
  };
  for (const [index, point] of science.points.entries()) {
    const id = identity('operating-configuration', [cycle.id, index]); pointResults.set(id, point);
    const replayEvidenceId = evidence(identity('replay', id), point as unknown as JsonObject, 'SELECTION');
    result.trialSettlements.push({ state: point.status === 'COMPLETED' ? 'COMPUTED' : 'FAILED', countedAttempt: true, evidenceRefs: [replayEvidenceId], errorCode: point.reason });
    if (accountScience) {
      const hold = accountScience.points[index]!.hold;
      const holdRef = evidence(identity('hold-replay', [cycle.id, index]), hold as unknown as JsonObject, 'SELECTION');
      result.trialSettlements.push({ state: hold.status === 'COMPLETED' ? 'COMPUTED' : 'FAILED', countedAttempt: true, evidenceRefs: [holdRef], errorCode: hold.reason });
      const holdSelection = object(hold.simulationRequest.selection);
      if (holdSelection.event_start_ns !== interval.startNs || holdSelection.event_end_ns !== interval.endExclusiveNs) throw new Problem('native_account_hold_scope_mismatch');
      comparison.pairedDifferenceEvidenceRefs.push(replayEvidenceId, holdRef);
    }
    const metrics = point.simulation ? metricsFor(point.simulation, replayEvidenceId) : [];
    const frames = Array.isArray(point.allocation?.frames) ? point.allocation.frames.map(frame => object(object(frame).allocation)) : [];
    const provenInfeasible = point.status === 'FAILED' && frames.some(frame => frame.solver_status === 'INFEASIBLE') && frames.every(frame => ['OPTIMAL', 'ACCEPTABLE_INACCURATE', 'INFEASIBLE'].includes(String(frame.solver_status)));
    const available = point.status === 'COMPLETED' && (!accountScience || accountScience.points[index]!.hold.status === 'COMPLETED') && metrics.length > 0 && requiredMetricsAvailable(metrics);
    capacity.evidenceRefs.push(replayEvidenceId);
    capacity.curve.push({ capital: { amount: point.capital, currency: capital.currency }, status: available ? 'AVAILABLE' : 'UNAVAILABLE', replayEvidenceRef: point.simulation ? replayEvidenceId : null,
      strategyVersionId: cycle.strategyVersionId, portfolioDecisionRef: null, netPerformance: metrics, entryDemand: null, exitDemand: null, estimatedCost: null, fillRatio: null,
      bindingConstraints: [], reasonCodes: available ? ['historical_configuration_replay_only'] : [point.reason ?? 'native_account_metric_unavailable'] });
    if (available) capacity.feasibleRegions.push({ lower: { amount: point.capital, currency: capital.currency }, upper: { amount: point.capital, currency: capital.currency },
      kind: 'TESTED_POINT', configurationEvidenceRef: replayEvidenceId, evidenceRef: replayEvidenceId });
    comparison.candidates.push({ id, kind: 'REBALANCE', configurationEvidenceRef: replayEvidenceId, status: available ? 'AVAILABLE' : 'UNAVAILABLE', transitionCost: [],
      ...(provenInfeasible ? { feasibility: { status: 'INFEASIBLE' as const, evidenceRefs: [replayEvidenceId], reasonCodes: ['native_registered_schedule_constraint_infeasible'] } } : {}),
      netOutcome: metrics.filter(metric => metric.definition.direction === 'MAXIMIZE'), downside: metrics.filter(metric => metric.definition.direction !== 'MAXIMIZE'),
      capacityEstimateIds: [capacityId], reasonCodes: available ? [isAccount ? 'same_observed_spot_balances_and_native_derived_nav' : 'same_fresh_account_start_and_owner_nav'] : [point.reason ?? 'native_account_metric_unavailable'] });
  }
  if (capacity.curve.length) { const sorted = capacity.curve.map(item => item.capital).sort((a, b) => compareDecimal(a.amount, b.amount)); capacity.minimumTestedCapital = sorted[0]!; capacity.maximumTestedCapital = sorted.at(-1)!; }
  if (capacity.feasibleRegions.length) capacity.status = 'AVAILABLE';
  const cashId = identity('operating-configuration', [cycle.id, 'cash']);
  const cashEvidenceId = evidence(identity('replay', cashId), baseline as unknown as JsonObject, 'SELECTION');
  const reserved = Number(frozen.maximumTrials ?? 0);
  while (result.trialSettlements.length < reserved - (isAccount ? 0 : 1)) result.trialSettlements.push({ state: 'REJECTED', countedAttempt: false, evidenceRefs: [], errorCode: 'planned_configuration_not_executed' });
  if (!isAccount) result.trialSettlements.push({ state: 'COMPUTED', countedAttempt: true, evidenceRefs: [cashEvidenceId], errorCode: null });
  const cashMetrics = baseline.simulation ? metricsFor(baseline.simulation, cashEvidenceId) : [];
  const cashAvailable = cashMetrics.length > 0 && requiredMetricsAvailable(cashMetrics);
  comparison.candidates.push({ id: cashId, kind: baseline.kind === 'HOLD' ? 'HOLD_CURRENT' : 'CASH', configurationEvidenceRef: cashEvidenceId, status: cashAvailable ? 'AVAILABLE' : 'UNAVAILABLE', transitionCost: [],
    netOutcome: cashMetrics.filter(metric => metric.definition.direction === 'MAXIMIZE'), downside: cashMetrics.filter(metric => metric.definition.direction !== 'MAXIMIZE'), capacityEstimateIds: [],
    reasonCodes: cashAvailable ? [isAccount ? 'same_observed_spot_balances_no_active_orders' : 'same_fresh_account_cash_baseline'] : [baseline.reason ?? 'matched_account_baseline_metric_unavailable'] });
  const compared = compareOperatingCandidates({ comparison, selection: record.policy.selectionRule, definitions, asOfNs: nowNs(), expectedBasis: { cycleId: cycle.id, basisRevision: cycle.basisRevision } });
  result.comparison = compared.comparison;
  result.artifacts[identity('paired-comparison', cycle.id)] = json(compared.pairwise);
  if (!result.comparison.selectedCandidateId) return unavailable(result.comparison.conclusion === 'UNRESOLVED_TRADEOFF' ? 'investment_tradeoff_unresolved' : 'comparable_native_metrics_missing', 'selection_evidence', 'CLARIFY_INTENT');
  const selectedId = result.comparison.selectedCandidateId;
  const selected = selectedId === cashId ? null : pointResults.get(selectedId)!;
  const selectedEvidenceId = comparison.candidates.find(item => item.id === selectedId)!.configurationEvidenceRef;
  if (accountScience) {
    if (!selected) {
      decision.action = 'HOLD'; decision.ensembleRevisionId = null; decision.jointReplayEvidenceRef = selectedEvidenceId;
      decision.reasonCodes = ['matched_observed_state_hold_preferred', 'historical_counterfactual_not_current_portfolio_instruction', 'no_new_capital_allocation', 'target_not_offered'];
      decision.explanationFacts = { ...object(decision.explanationFacts), summary: 'Keeping the original observed balances is preferred under the frozen business preference.', drivers: [selectedEvidenceId, valuationRef!, ...decision.reasonCodes] };
      validatePmEntity('PortfolioDecision', decision); validatePmEntity('CapacityEstimate', capacity);
      return result;
    }
  }
  const committed = selected?.capital ?? '0'; const uncommitted = subtractDecimal(capital.amount, committed);
  decision.action = selected ? 'REBALANCE' : baseline.kind; decision.jointReplayEvidenceRef = selectedEvidenceId;
  decision.solverEvidenceRef = selected?.allocation ? selectedEvidenceId : null;
  const points = selected?.simulationRequest?.target_points;
  const last = Array.isArray(points) && points.length ? object(points.at(-1)) : null;
  if (last && Array.isArray(last.targets)) decision.assetTargets = last.targets.map(item => {
    const target = object(item); const targetNotional = multiplyDecimal(String(target.weight), capital.amount);
    return { instrumentId: String(target.instrument_id), navWeight: String(target.weight), targetQuantity: null,
      targetNotional: { amount: targetNotional, currency: capital.currency }, currentNotional: null,
      netTradeNotional: null, quantityRuleRef: datasetRevision.instrumentRulesRef, riskContribution: null };
  });
  const allocated = decision.assetTargets.map(item => item.targetNotional.amount).reduce(addDecimal, '0');
  const tradingCash = subtractDecimal(committed, allocated);
  if (compareDecimal(tradingCash, '0') < 0) throw new Problem('native_target_exceeds_committed_capital');
  decision.capital = { equity: capital, reserved: { amount: '0', currency: capital.currency }, ownerLiquidityReserve: { amount: '0', currency: capital.currency },
    deployableBudget: capital, committedCapital: { amount: committed, currency: capital.currency }, uncommittedCash: { amount: uncommitted, currency: capital.currency },
    projectedTradingCash: { amount: tradingCash, currency: capital.currency }, capacityEstimateIds: selected ? [capacityId] : [], sizingEvidenceRef: selectedEvidenceId };
  if (compareDecimal(committed, '0') > 0) decision.strategyAllocations = [{ strategyVersionId: cycle.strategyVersionId, ensembleRevisionId: ensembleId,
    committedCapital: { amount: committed, currency: capital.currency }, capitalFraction: '1', allocationEvidenceRef: selectedEvidenceId, riskContributionEvidenceRef: null, nettingGroupId: cycle.id }];
  decision.reasonCodes = [isAccount ? 'historical_observed_account_counterfactual' : 'historical_research_allocation', 'initial_nav_normalized_research_targets_not_observed_positions', 'matched_owner_nav_comparison', 'new_portfolio_requires_independent_evaluation', 'target_not_offered'];
  decision.explanationFacts = { ...object(decision.explanationFacts), summary: selected ? 'Selected a native optimized allocation using the frozen business preference and a matched starting-state baseline.' : 'The matched starting-state baseline is preferred under the frozen business preference.',
    drivers: [result.comparison.id, selectedEvidenceId, ...decision.reasonCodes] };
  ensemble.jointValidationEvidenceRefs = selected ? [selectedEvidenceId] : []; ensemble.qualification = selected ? 'SELECTED' : 'CANDIDATE';
  if (!selected) { decision.ensembleRevisionId = null; decision.reasonCodes = decision.reasonCodes.filter(reason => reason !== 'new_portfolio_requires_independent_evaluation'); decision.reasonCodes.push('baseline_does_not_qualify_alpha_strategy'); }
  if (selected) result.gaps.push({ capability: 'independent_portfolio_evaluation', reason: 'new_optimized_portfolio_not_independently_evaluated', remedy: 'RECOMPUTE' });
  if (selected && object(record.policy.validDomain).operatingRule) {
    const studies = Array.isArray(science.evidence.studies) ? science.evidence.studies.map(value => object(value)) : [];
    const study = studies.find(value => value.capital === selected.capital);
    if (!study?.request) throw new Problem('selected_operating_method_request_missing');
    const seedRef = identity('forward-seed', decision.id);
    evidence(seedRef, { source, study: object(study.request), objects: scientificObjects, bindings: scientificBindings,
      nativeImplementation: science.evidence.nativeImplementation!,
      allocationMethod: object(record.policy.validDomain).jointAllocationMethod!,
      selectedCapital: committed, originalNav: capital.amount, currency: capital.currency,
      operatingRule: object(record.policy.validDomain).operatingRule!, policyRevisionId: record.policy.id,
      strategyVersionId: cycle.strategyVersionId, ensembleRevisionId: ensembleId, selectionEvidenceRef: selectedEvidenceId }, 'CONTROL');
    decision.explanationFacts = { ...object(decision.explanationFacts), forwardSeedEvidenceRef: seedRef,
      forwardAnchorNs: object(study.request).evaluation_start_ns!, forwardTtlSeconds: object(object(object(study.request).mandate).rebalance_schedule).target_ttl_seconds! };
  }
  // Historical decisions never receive a renewed TTL or pretend to offer a current target.
  validatePmEntity('PortfolioDecision', decision); validatePmEntity('CapacityEstimate', capacity);
  return result;
}
