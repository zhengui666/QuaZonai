import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { assessPmExecutionQualification, validatePmExecutionEvidence } from '../../../packages/contracts/src/pm-forward.ts';
import type { PmExecutionLiquidity } from '../../../packages/contracts/src/pm-forward.ts';
import { addDecimal, compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { object } from './facts.ts';
import { pmArtifactDigest } from './pm-evidence.ts';
import { fact, identity } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';
import { assessPmForwardFeasibility } from './pm-forward-feasibility.ts';

/** Combines actual original proofs; it does not confer scientific qualification
 * from a source-provided readiness flag. Missing proof stays missing. */
export function qualifyPmForward(input: { record: PmRecord; decision: Pm.PortfolioDecision; seedDecision: Pm.PortfolioDecision;
  native: JsonObject; capture: JsonObject; dataset: Pm.DatasetRevision; sourceManifest: JsonObject | null;
  forwardEvidenceRef: string; assessedAtNs: string }): { assessment: Pm.PipelineAssessment; validUntilNs: string; artifact: JsonObject } {
  const { record, decision, seedDecision, native, capture, dataset, forwardEvidenceRef, assessedAtNs } = input;
  const snapshot = object(capture.snapshot); const execution = snapshot.execution ? object(snapshot.execution) : null;
  const target = object(native.target); const request = object(native.request); const mandate = object(request.mandate);
  const ttl = (BigInt(String(object(mandate.rebalance_schedule).target_ttl_seconds)) * 1_000_000_000n).toString();
  const nativeObservation = object(snapshot.nativeObservation); const binding = object(nativeObservation.binding);
  const trades = (native.trade_estimates as JsonObject[]).map(value => object(value));
  const roundedTargets = decision.assetTargets.map(row => {
    const trade = trades.find(value => value.instrument_id === row.instrumentId)!;
    return { ...row, targetQuantity: String(trade.post_rounding_quantity), targetNotional: { amount: String(trade.post_rounding_notional), currency: row.targetNotional.currency },
      netTradeNotional: { amount: String(trade.rounded_trade_notional), currency: row.targetNotional.currency } };
  });
  const build = object(native.build); const nativeInput = object(build.input);
  const executionAssessment = assessPmExecutionQualification({ proof: execution?.pmEvidence ?? null, proofEvidenceRef: decision.accountBasis!.positionsEvidenceRef,
    accountBasis: decision.accountBasis!, nativeAccountId: String(binding.native_account_id), mode: object(capture.observation).mode as 'paper' | 'live',
    decisionAtNs: String(target.asof_ns), evaluatedAtNs: assessedAtNs, nativeTargetValidUntilNs: String(target.valid_until_ns), frozenTargetTtlNs: ttl,
    liquidityValidUntilNs: typeof execution?.validUntilNs === 'string' ? execution.validUntilNs : String(target.asof_ns),
    liquidity: (execution?.liquidity ?? []) as unknown as PmExecutionLiquidity[], targets: roundedTargets,
    positions: trades.map(row => ({ instrumentId: String(row.instrument_id), currentNotional: { amount: String(row.current_notional), currency: decision.accountBasis!.currency } })),
    modeledCostRates: (nativeInput.assets as JsonObject[]).map(row => ({ instrumentId: String(row.instrument_id), rate: String(row.transaction_cost_rate) })) });
  const missing: string[] = []; const failed: string[] = [];
  const constraintFeasibility = assessPmForwardFeasibility(native);
  if (constraintFeasibility.proposed.status === 'UNKNOWN') missing.push('current_constraint_measurement_not_supported');
  else if (constraintFeasibility.proposed.status !== 'FEASIBLE') failed.push('current_rounded_proposal_violates_original_constraints');
  const proof = execution?.pmEvidence ? validatePmExecutionEvidence(execution.pmEvidence) : null;
  const definitions = (native.instrument_definitions as JsonObject[]).map(value => object(value));
  if (proof) for (const row of proof.instruments) {
    const definition = definitions.find(value => Object.values(value).some(body => body && typeof body === 'object' && !Array.isArray(body) && body.id === row.instrumentId));
    if (!definition || row.rulesRef !== `sha256:${pmArtifactDigest(definition)}`) failed.push(`execution_native_instrument_rule_identity_mismatch:${row.instrumentId}`);
  }
  const market = object(object(native.valuation).source_evidence ?? {});
  // The native adapter checks every selected bar against original official
  // response rows and maps ts_init to that response's actual receipt. This is
  // current knowledge, never a claim of historical first-public availability.
  if (!input.sourceManifest || !dataset.contentRef || !market.capture_id || market.method !== 'HYPERLIQUID_PUBLIC_REST_CANDLE_SNAPSHOT'
    || typeof market.last_received_ns !== 'string' || typeof market.selected_row_count !== 'string'
    || BigInt(String(market.selected_row_count ?? '0')) <= 0n) missing.push('current_original_market_receipt_coverage_missing');
  else if (BigInt(market.last_received_ns) > BigInt(String(target.asof_ns))) failed.push('current_market_receipts_after_cutoff');
  if (object(capture.observation).mode === 'live') {
    const independentProtocol = object(object(record.policy.validDomain).qualificationProtocol);
    if (independentProtocol.maximumPromotion !== 'TARGET_OPERATION' || independentProtocol.liveTargetAuthorization !== true) missing.push('original_method_independent_protocol_authorizes_shadow_only');
    if (dataset.qualityStatus !== 'AVAILABLE') missing.push('live_method_historical_availability_not_qualified');
  }
  const caps = object(object(capture.capitalCapabilities ?? {}).document ?? {});
  if (object(caps.automaticTargets ?? {}).supported !== true || object(caps.automaticTargets ?? {}).revisionBound !== true) missing.push('atomic_revision_bound_target_transport_missing');
  if (object(caps.automaticTargets ?? {}).snapshotBound !== true) missing.push('atomic_snapshot_bound_target_transport_missing');
  if (record.mandate.operatingAuthority !== 'AUTHORIZED_TARGETS') missing.push('owner_target_authority_not_granted');
  if (record.mandate.unresolvedIntent.length || Object.values(record.gaps).some(gap => gap.state !== 'RESOLVED' && gap.permittedRemedy === 'CLARIFY_INTENT')) missing.push('owner_intent_unresolved');
  if (object(seedDecision.explanationFacts).economicQualification == null || object(object(seedDecision.explanationFacts).economicQualification).outcome !== 'READY') missing.push('original_independent_method_not_qualified');
  const roundedInvested = roundedTargets.reduce((sum, row) => addDecimal(sum, row.targetNotional.amount), '0');
  if (!decision.capital || compareDecimal(roundedInvested, decision.capital.committedCapital.amount) > 0) failed.push('rounded_inventory_exceeds_frozen_deployment_fraction');
  const maxAge = BigInt(String(object(mandate.rebalance_schedule).max_input_age_seconds)) * 1_000_000_000n;
  const maximumPriceAge = BigInt(String(object(object(request.execution_settings).multi_currency_spot_cash).maximum_price_age_ns));
  const markAge = maximumPriceAge < maxAge ? maximumPriceAge : maxAge;
  const deadlines = [{ sourceKind: 'ORIGINAL_NATIVE_TARGET_TTL', sourceEvidenceRef: forwardEvidenceRef, validUntilNs: String(target.valid_until_ns) },
    { sourceKind: 'ORIGINAL_ACCOUNT_MAX_INPUT_AGE', sourceEvidenceRef: decision.accountBasis!.positionsEvidenceRef, validUntilNs: (BigInt(decision.accountBasis!.observedAtNs) + maxAge).toString() },
    { sourceKind: 'ORIGINAL_FORECAST_MAX_INPUT_AGE', sourceEvidenceRef: forwardEvidenceRef, validUntilNs: (BigInt(String(object(nativeInput.forecasts).forecast_asof_ns)) + maxAge).toString() },
    ...trades.map(row => ({ sourceKind: `ORIGINAL_MARK_PRICE_AND_INPUT_AGE:${row.instrument_id}`, sourceEvidenceRef: forwardEvidenceRef, validUntilNs: (BigInt(String(row.price_event_ns)) + markAge).toString() })),
    ...(executionAssessment.validUntilNs ? [{ sourceKind: 'ORIGINAL_EXECUTION_SOURCE_DEADLINES', sourceEvidenceRef: decision.accountBasis!.positionsEvidenceRef, validUntilNs: executionAssessment.validUntilNs }] : [])];
  const validUntilNs = deadlines.map(value => value.validUntilNs).reduce((a, b) => BigInt(a) < BigInt(b) ? a : b);
  if (BigInt(validUntilNs) <= BigInt(assessedAtNs)) failed.push('current_input_original_freshness_deadline_expired');
  const reasonCodes = [...executionAssessment.reasonCodes, ...missing, ...failed];
  const outcome: Pm.PipelineAssessment['outcome'] = failed.length || executionAssessment.outcome === 'NOT_READY' ? 'NOT_READY'
    : missing.length || executionAssessment.outcome !== 'READY' ? 'NEEDS_EVIDENCE' : 'READY';
  const id = identity('forward-qualification', decision.id);
  const artifact: JsonObject = { schemaVersion: 1, scope: 'CURRENT_TARGET_DELIVERY_ONLY', decisionId: decision.id,
    basisRevision: decision.basisRevision, assessmentOutcome: outcome, execution: executionAssessment as unknown as JsonObject,
    roundedExecutionTargets: roundedTargets as unknown as JsonObject[], constraintFeasibility: constraintFeasibility as unknown as JsonObject, deadlines, validUntilNs, marketReceiptEvidence: market,
    datasetRevision: dataset.id, datasetContentRef: dataset.contentRef, nativeEvidenceRef: forwardEvidenceRef,
    independentMethodEvidenceRef: object(seedDecision.explanationFacts).independentEvidenceRef!,
    limitations: ['MODELED_CURRENT_EXPECTATIONS_ARE_NOT_REALIZED_OR_INDEPENDENTLY_TESTED_CURRENT_WEIGHTS', 'SOURCE_BOUNDS_ARE_CONDITIONAL_NOT_GUARANTEED_FILLS'] };
  const paper = object(capture.observation).mode === 'paper';
  const assessment: Pm.PipelineAssessment = { ...fact(id, assessedAtNs), strategyVersionId: decision.strategyVersionId,
    stage: paper ? 'PAPER' : 'TARGET_OPERATION', outcome, evaluatedAtNs: assessedAtNs,
    requirementsVersion: paper ? 'qz.pm.paper-shadow-source-target-qualification/1' : 'qz.pm.live-source-target-qualification/1',
    evidenceRefs: [id, forwardEvidenceRef, decision.accountBasis!.positionsEvidenceRef, String(object(seedDecision.explanationFacts).independentEvidenceRef)],
    reasonCodes: [...reasonCodes, paper ? 'PAPER_SHADOW_OBSERVATION_NOT_LIVE_METHOD_QUALIFICATION' : 'LIVE_METHOD_PROTOCOL_REQUIRED'],
    nextPermittedActions: outcome === 'READY' ? [paper ? 'DELIVER_REVISION_BOUND_PAPER_SHADOW_TARGET' : 'DELIVER_REVISION_BOUND_TARGET'] : ['COLLECT_MISSING_OR_REFRESHED_ORIGINAL_SOURCE_PROOFS'] };
  // Current source bounds do not constitute a historical capital replay,
  // a validated capital interval, or an economic capacity upper bound.
  return { assessment, validUntilNs, artifact };
}
