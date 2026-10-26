import { boundScienceTarget } from './science-binding.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { addDecimal, compareDecimal, multiplyDecimal, subtractDecimal, validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';
import { canonical } from '../../research/src/documents.ts';
import { existingDataset, type ServiceConfig } from './config.ts';
import { json, nowNs, object, Problem } from './facts.ts';
import { fact, identity } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';
import type { PmDecisionResult } from './pm-native.ts';
import type { PmScienceCheckpoint } from './pm-science.ts';
import { pmArtifactDigest } from './pm-evidence.ts';
import { buildPmForward } from './pm-forward-native.ts';
import { validatePmOperatingRule } from './pm-operating-rule.ts';
import { qualifyPmForward } from './pm-forward-qualification.ts';
import { assessPmForwardFeasibility, selectPmForwardProposal } from './pm-forward-feasibility.ts';
import { readSourceRevision } from './pm-source.ts';

/** The incumbent is a complete independently evaluated method under the exact
 * current policy. A newer unqualified research candidate cannot replace it. */
export function forwardSeedDecision(record: PmRecord): Pm.PortfolioDecision | null {
  if (record.mandate.capitalSource.kind !== 'ACCOUNT' || !object(record.policy.validDomain).operatingRule) return null;
  try { validatePmOperatingRule(object(record.policy.validDomain).operatingRule); }
  catch { return null; } // Older methods remain historical evidence, not a faulting monitor or an upgraded operating seed.
  return Object.values(record.decisions).reverse().find(decision => {
    const explanation = object(decision.explanationFacts);
    const control = record.artifacts[`strategy-control:${decision.strategyVersionId}`];
    const resumedObservation = control?.action === 'resume' && decision.reasonCodes.includes('owner_strategy_paused');
    if (decision.status === 'REVOKED' && !resumedObservation || control?.action === 'pause'
      || decision.policyRevisionId !== record.policy.id || !decision.capital || !decision.ensembleRevisionId
      || decision.action !== 'REBALANCE' || typeof explanation.forwardSeedEvidenceRef !== 'string'
      || object(explanation.economicQualification ?? {}).outcome !== 'READY'
      || typeof explanation.independentEvidenceRef !== 'string') return false;
    const frozen = record.artifacts[`independent-input:${decision.id}`];
    if (!frozen || canonical(object(frozen.policy)) !== canonical(json(record.policy))) return false;
    const independent = record.evidence[explanation.independentEvidenceRef];
    return !!independent && independent.role === 'INDEPENDENT'
      && !Object.values(record.retirements).some(item => item.strategyVersionId === decision.strategyVersionId);
  }) ?? null;
}

export interface PmForwardResult extends PmDecisionResult { assessments: Record<string, Pm.PipelineAssessment> }
type ArtifactLoader = (reference: Pm.EvidenceRef, stored: JsonObject) => Promise<JsonObject>;

export async function computePmForward(record: PmRecord, cycle: Pm.DecisionCycle, config: ServiceConfig,
  context: Context, checkpoint: PmScienceCheckpoint, load: ArtifactLoader): Promise<PmForwardResult> {
  const frozen = record.artifacts[cycle.id]!;
  const configured = object(frozen.configured); const data = object(configured.data);
  const dataset = configured.revision as unknown as Pm.DatasetRevision;
  const seedDecision = typeof frozen.forwardSeedDecisionId === 'string' ? record.decisions[frozen.forwardSeedDecisionId] : null;
  const comparison: Pm.OperatingComparison = { ...fact(identity('forward-comparison', cycle.id)), cycleId: cycle.id, basisRevision: cycle.basisRevision,
    policyRevisionId: record.policy.id, incumbentDecisionId: seedDecision?.id ?? null, evaluationContext: 'FORWARD_MODEL_ESTIMATE',
    scope: { id: identity('forward-scope', cycle.id), allowedUniverseRef: record.mandate.allowedUniverseRef, datasetRevisionIds: cycle.datasetRevisionIds,
      interval: { startNs: cycle.asOfNs, endExclusiveNs: (BigInt(cycle.asOfNs) + 1n).toString() }, role: 'OBSERVED', calendarId: 'POINT_IN_TIME_DECISION_CUTOFF',
      labelDefinitionId: null, accountingPolicyRef: 'qz.observed-spot-cutoff-price-inventory/1', costModelRef: record.policy.id,
      startingStateRef: cycle.accountBasis?.positionsEvidenceRef ?? identity('missing-account', cycle.id), capital: null },
    candidates: [], pairedDifferenceEvidenceRefs: [], selectedCandidateId: null, conclusion: 'NEEDS_EVIDENCE', reasonCodes: [] };
  const decision: Pm.PortfolioDecision = { ...fact(identity('forward-decision', cycle.id)), cycleId: cycle.id, mandateRevisionId: record.mandate.id,
    strategyVersionId: seedDecision?.strategyVersionId ?? cycle.strategyVersionId, ensembleRevisionId: seedDecision?.ensembleRevisionId ?? null,
    strategyAllocations: [], policyRevisionId: record.policy.id, basisRevision: cycle.basisRevision, incumbentDecisionId: seedDecision?.id ?? null,
    accountBasis: cycle.accountBasis, researchStartingStateRef: null, action: 'NEEDS_EVIDENCE', status: 'CANDIDATE', assetTargets: [], capital: null,
    comparisonRef: comparison.id, solverEvidenceRef: null, jointReplayEvidenceRef: null, reasonCodes: [],
    explanationFacts: { summary: 'Current account inference requires an independently evaluated frozen method and fresh original sources.', drivers: [] },
    validUntilNs: null, nextInvalidationDependencies: cycle.dependencyRevisionRefs, targetCommandId: null };
  const result: PmForwardResult = { decision, comparison, capacities: {}, ensembles: {}, evidence: {}, artifacts: {}, assessments: {},
    trials: {}, budgetMovements: {}, scienceCompleted: false, trialSettlements: [], gaps: [] };
  const unavailable = (reason: string, capability = 'current_frozen_method', remedy: Pm.EvidenceGap['permittedRemedy'] = 'COLLECT_AUTHORIZED_DATA') => {
    decision.reasonCodes.push(reason); comparison.reasonCodes.push(reason); result.gaps.push({ capability, reason, remedy });
    decision.explanationFacts = { ...object(decision.explanationFacts), drivers: [...decision.reasonCodes] }; return result;
  };
  if (!seedDecision || forwardSeedDecision(record)?.id !== seedDecision.id) return unavailable('independently_qualified_current_policy_seed_missing');
  if (frozen.operatingSlotAlreadyAdmitted === true) return unavailable('registered_operating_slot_already_admitted', 'operating_cadence', 'REFRESH_SOURCE');
  if (!cycle.accountBasis) return unavailable('current_authoritative_account_missing', 'current_account', 'REFRESH_SOURCE');
  if (dataset.revisionKind === 'RETRACTION' || Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && item.requiredCapability === 'source_revision')) {
    return unavailable('current_source_revision_invalidated', 'source_revision', 'REFRESH_SOURCE');
  }
  if (Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && item.permittedRemedy === 'CLARIFY_INTENT')) return unavailable('current_owner_intent_unresolved', 'owner_intent', 'CLARIFY_INTENT');
  const seedRef = String(object(seedDecision.explanationFacts).forwardSeedEvidenceRef);
  const seed = await load(record.evidence[seedRef]!, record.artifacts[seedRef]!);
  if (seed.policyRevisionId !== record.policy.id || canonical(seed.operatingRule!) !== canonical(object(record.policy.validDomain).operatingRule!)
    || !seed.allocationMethod || canonical(seed.allocationMethod) !== canonical(object(record.policy.validDomain).jointAllocationMethod!)) return unavailable('current_method_policy_mismatch');
  const study = object(seed.study); const mandate = object(study.mandate); const schedule = object(mandate.rebalance_schedule);
  const ttl = BigInt(String(schedule.target_ttl_seconds)) * 1_000_000_000n;
  const anchor = BigInt(String(study.evaluation_start_ns));
  if (ttl <= 0n || BigInt(cycle.asOfNs) < anchor) return unavailable('current_method_clock_invalid');
  const slot = ((BigInt(cycle.asOfNs) - anchor) / ttl).toString();
  const previous = Object.values(record.decisions).find(item => object(item.explanationFacts).forwardSeedDecisionId === seedDecision.id
    && object(item.explanationFacts).operatingSlot === slot && item.id !== decision.id && item.solverEvidenceRef !== null);
  if (previous) return unavailable('registered_operating_slot_already_computed', 'operating_cadence', 'REFRESH_SOURCE');
  const raw = object(data.request, 'current_factor_dataset_request');
  const selection = { ...object(object(raw.forecast).selection), event_end_ns: cycle.asOfNs, decision_cutoff_ns: cycle.asOfNs };
  if (BigInt(dataset.availableThroughNs) > BigInt(cycle.availableCutoffNs)) return unavailable('current_data_not_available_asof', 'point_in_time_data', 'REFRESH_SOURCE');
  const accountRef = record.evidence[cycle.accountBasis.positionsEvidenceRef]!;
  const capture = { basis: cycle.accountBasis, evidence: accountRef, artifact: record.artifacts[accountRef.objectKey]! };
  const trustedConfig = existingDataset(config, dataset.datasetId);
  const sourceBefore = trustedConfig ? await readSourceRevision(trustedConfig) : null;
  if (sourceBefore && sourceBefore.contentDigest !== dataset.contentRef) return unavailable('current_source_revision_changed_before_inference', 'source_revision', 'REFRESH_SOURCE');
  const artifact = await buildPmForward({ seed, capture, datasetRevision: { id: dataset.id, contentRef: dataset.contentRef }, selection,
    catalog: String(data.catalog), binary: boundScienceTarget(config, data), dataDir: config.dataDir, basisRevision: cycle.basisRevision }, context, checkpoint);
  const sourceAfter = trustedConfig ? await readSourceRevision(trustedConfig) : null;
  if (sourceBefore?.contentDigest !== sourceAfter?.contentDigest) throw new Problem('current_source_revision_changed_during_inference');
  const native = object(artifact.result); const build = object(native.build); const valuation = object(native.valuation);
  const evidenceRef = identity('forward-native', cycle.id); const atNs = nowNs();
  const evidence = (id: string, payload: JsonObject, role: Pm.EvidenceRole = 'CONTROL') => {
    result.artifacts[id] = payload; result.evidence[id] = { id, sourceKind: 'NATIVE_RESULT', role, runtime: cycle.runtime, objectKey: id,
      contentDigest: pmArtifactDigest(payload), mediaType: 'application/json', sourceRevision: cycle.basisRevision, eventInterval: null,
      availableAtNs: atNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.pm.current-frozen-method/1', visibility: 'OWNER_ONLY' }; return id;
  };
  evidence(evidenceRef, artifact); decision.solverEvidenceRef = evidenceRef;
  const capital: Pm.Money = { currency: cycle.accountBasis.currency, amount: String(valuation.total) }; comparison.scope.capital = capital;
  decision.accountBasis = { ...cycle.accountBasis, valuation: { origin: 'DERIVED', timeBasis: 'DECISION_CUTOFF', reportValue: capital,
    valuationEvidenceRef: evidenceRef, priceEvidenceRef: evidenceRef, methodRef: 'qz.native.observed-spot-current-valuation/1',
    asOfNs: cycle.asOfNs, availableAtNs: String(valuation.available_ns) } };
  decision.explanationFacts = { summary: 'A single application of the frozen method compares current holdings with a current optimized target using the same modeled return, risk and cost basis.',
    drivers: [evidenceRef, seedRef], forwardEvidenceRef: evidenceRef, forwardSeedDecisionId: seedDecision.id,
    operatingSlot: slot, nextOperatingAtNs: (anchor + (BigInt(slot) + 1n) * ttl).toString(), independentMethodEvidenceRef: object(seedDecision.explanationFacts).independentEvidenceRef!,
    currentWeightsIndependentlyEvaluated: false };
  const modeled = object(native.rounded_modeled_comparison ?? { hold: object(native.modeled_comparison).hold, proposed: null, limitations: ['ROUNDED_MODEL_PROPOSAL_UNAVAILABLE'] }); const outcome = (value: unknown): Pm.ModeledOperatingOutcome => {
    const row = object(value); return { grossExpectedReturn: Number(row.gross_expected_return), linearTurnoverCost: Number(row.linear_turnover_cost),
      netExpectedReturn: Number(row.net_expected_return), variance: Number(row.variance), objectiveValue: Number(row.objective_value) };
  };
  const actualInput = object(build.input); const forecasts = object(actualInput.forecasts); const returns = object(actualInput.return_history);
  const usedAvailability = [String(valuation.available_ns), capture.basis.availableAtNs,
    ...(forecasts.members as JsonObject[]).map(value => String(object(value).available_ns)), ...(returns.available_ns as string[])];
  const availableAtNs = usedAvailability.reduce((a, b) => BigInt(a) > BigInt(b) ? a : b);
  comparison.modeledComparison = { method: 'MATCHED_ROUNDED_LINEAR_RETURN_VARIANCE_V1', basis: 'FORWARD_MODEL_ESTIMATE', forecastUnit: 'RETURN_PER_HORIZON',
    methodPolicyRef: record.policy.id, horizonRef: record.ensembles[seedDecision.ensembleRevisionId!]!.labelDefinitionId,
    riskAversion: String(object(object(mandate.optimizer).parameters).risk_aversion), nativeEvidenceRef: evidenceRef, startingStateRef: accountRef.id,
    asOfNs: cycle.asOfNs, availableAtNs, hold: outcome(modeled.hold), proposed: modeled.proposed ? outcome(modeled.proposed) : null,
    targetBlockReason: typeof native.target_block_reason === 'string' ? native.target_block_reason : null, limitations: (modeled.limitations ?? []) as string[] };
  const held = identity('forward-hold', cycle.id), proposed = identity('forward-proposed', cycle.id);
  comparison.candidates = [{ id: held, kind: 'HOLD_CURRENT', configurationEvidenceRef: evidenceRef, status: 'AVAILABLE', transitionCost: [], netOutcome: [], downside: [], capacityEstimateIds: [], reasonCodes: ['MODELED_HOLD_SAME_CURRENT_INVENTORY'] },
    { id: proposed, kind: 'REBALANCE', configurationEvidenceRef: evidenceRef, status: native.target ? 'AVAILABLE' : 'UNAVAILABLE', transitionCost: [], netOutcome: [], downside: [], capacityEstimateIds: [], reasonCodes: ['MODELED_FROZEN_METHOD_NOT_HISTORICAL_OWNER_METRICS'] }];
  const feasibility = assessPmForwardFeasibility(native);
  const feasibilityRef = evidence(identity('forward-feasibility', cycle.id), json(feasibility));
  result.evidence[feasibilityRef]!.sourceKind = 'DERIVED';
  decision.explanationFacts = { ...object(decision.explanationFacts), currentConstraintEvidenceRef: feasibilityRef };
  for (const [index, measured] of [feasibility.hold, feasibility.proposed].entries()) {
    const candidate = comparison.candidates[index]!;
    candidate.feasibility = { status: measured.status, evidenceRefs: [feasibilityRef], reasonCodes: measured.reasonCodes };
    if (measured.status !== 'FEASIBLE') { candidate.status = 'UNAVAILABLE'; candidate.reasonCodes.push(...measured.reasonCodes); }
  }
  // Filled by the registered method's exact comparison helper below. The
  // historical owner metric selector is intentionally never called here.
  const proposedOutcome = comparison.modeledComparison.proposed;
  const objectiveDelta = proposedOutcome ? comparison.modeledComparison.hold.objectiveValue - proposedOutcome.objectiveValue : null;
  const tie = object(object(object(seed.operatingRule).decisionRule).numericalTie);
  const numericalTie = Number(tie.absoluteTolerance) + Number(tie.relativeTolerance)
    * Math.max(Math.abs(comparison.modeledComparison.hold.objectiveValue), Math.abs(proposedOutcome?.objectiveValue ?? 0));
  if (!native.target || objectiveDelta === null) {
    return unavailable(typeof native.target_block_reason === 'string' ? native.target_block_reason : 'current_modeled_proposal_unavailable', 'current_native_proposal', 'REFRESH_SOURCE');
  }
  const operatingChoice = selectPmForwardProposal(feasibility, comparison.modeledComparison.hold.objectiveValue,
    proposedOutcome?.objectiveValue ?? null, numericalTie);
  if (operatingChoice === 'NEEDS_EVIDENCE') {
    return unavailable('current_hard_constraint_restoration_unproven', 'current_constraint_measurement', 'COLLECT_AUTHORIZED_DATA');
  }
  const restoringConstraints = feasibility.hold.status === 'INFEASIBLE';
  if (operatingChoice === 'HOLD') {
    decision.action = 'HOLD'; comparison.selectedCandidateId = held; comparison.conclusion = 'SELECTED';
    decision.reasonCodes = [native.target_block_reason ? String(native.target_block_reason) : 'registered_method_no_material_modeled_improvement'];
    validatePmEntity('OperatingComparison', comparison); validatePmEntity('PortfolioDecision', decision); return result;
  }
  const target = object(native.target); const trades = (native.trade_estimates as JsonObject[]).map(value => object(value));
  decision.assetTargets = (target.targets as JsonObject[]).map(row => {
    const trade = trades.find(item => item.instrument_id === row.instrument_id); if (!trade) throw new Problem('forward_actual_trade_basis_missing');
    return { instrumentId: String(row.instrument_id), navWeight: String(row.weight), targetQuantity: null,
      targetNotional: { amount: String(trade.target_notional), currency: capital.currency }, currentNotional: { amount: String(trade.current_notional), currency: capital.currency },
      netTradeNotional: { amount: String(trade.net_trade_notional), currency: capital.currency }, quantityRuleRef: evidenceRef, riskContribution: null,
      executionEstimate: { resultingQuantity: String(trade.post_rounding_quantity), resultingNotional: { amount: String(trade.post_rounding_notional), currency: capital.currency },
        netTradeQuantity: String(trade.rounded_trade_quantity), netTradeNotional: { amount: String(trade.rounded_trade_notional), currency: capital.currency }, evidenceRef } };
  });
  const committed = multiplyDecimal(capital.amount, subtractDecimal('1', String(object(mandate.constraints).min_cash_weight)));
  const invested = decision.assetTargets.reduce((sum, row) => addDecimal(sum, row.targetNotional.amount), '0');
  if (compareDecimal(invested, committed) > 0) throw new Problem('forward_allocation_exceeds_frozen_capital_fraction');
  decision.capital = { equity: capital, reserved: { amount: '0', currency: capital.currency }, ownerLiquidityReserve: { amount: '0', currency: capital.currency },
    deployableBudget: capital, committedCapital: { amount: committed, currency: capital.currency }, uncommittedCash: { amount: subtractDecimal(capital.amount, committed), currency: capital.currency },
    projectedTradingCash: { amount: subtractDecimal(committed, invested), currency: capital.currency }, capacityEstimateIds: [], sizingEvidenceRef: evidenceRef };
  decision.strategyAllocations = [{ strategyVersionId: decision.strategyVersionId, ensembleRevisionId: decision.ensembleRevisionId,
    committedCapital: decision.capital.committedCapital, capitalFraction: '1', allocationEvidenceRef: evidenceRef, riskContributionEvidenceRef: null, nettingGroupId: cycle.id }];
  const targetRef = evidence(identity('forward-target', cycle.id), target);
  decision.explanationFacts = { ...object(decision.explanationFacts), nativeTargetEvidenceRef: targetRef };
  decision.action = 'REBALANCE'; comparison.selectedCandidateId = proposed; comparison.conclusion = 'SELECTED';
  decision.validUntilNs = String(target.valid_until_ns);
  const selectionReason = restoringConstraints ? 'current_original_hard_constraint_restoration' : 'current_frozen_method_modeled_improvement';
  decision.reasonCodes = [selectionReason, 'current_execution_qualification_required'];
  const qualified = qualifyPmForward({ record, decision, seedDecision, native, capture: capture.artifact, dataset,
    sourceManifest: sourceBefore?.manifest ?? null, forwardEvidenceRef: evidenceRef, assessedAtNs: atNs });
  evidence(qualified.assessment.id, qualified.artifact);
  result.evidence[qualified.assessment.id]!.sourceKind = 'DERIVED';
  result.assessments[qualified.assessment.id] = qualified.assessment;
  const executionEstimateRef = evidence(identity('forward-rounded-execution', cycle.id), { tradeEstimates: native.trade_estimates!, modeledComparison: native.rounded_modeled_comparison! });
  decision.explanationFacts = { ...object(decision.explanationFacts), executionQualificationEvidenceRef: qualified.assessment.id,
    executionEstimateEvidenceRef: executionEstimateRef, targetQualification: qualified.assessment.outcome };
  if (qualified.assessment.outcome === 'READY') {
    decision.validUntilNs = qualified.validUntilNs;
    decision.reasonCodes = [selectionReason, 'current_paper_source_target_qualified'];
  } else for (const reason of qualified.assessment.reasonCodes.filter(reason => !['original_execution_source_bounds_cover_current_target', 'PAPER_SHADOW_OBSERVATION_NOT_LIVE_METHOD_QUALIFICATION', 'LIVE_METHOD_PROTOCOL_REQUIRED'].includes(reason))) {
    result.gaps.push({ capability: 'current_target_qualification', reason, remedy: 'COLLECT_AUTHORIZED_DATA' });
  }
  validatePmEntity('OperatingComparison', comparison); validatePmEntity('PortfolioDecision', decision);
  return result;
}
