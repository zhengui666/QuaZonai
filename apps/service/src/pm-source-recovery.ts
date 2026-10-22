import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { canonical } from '../../research/src/documents.ts';
import { json, object } from './facts.ts';
import { digest, gap, identity, revise } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';

const healthKey = 'source-health';

/** A failed poll invalidates usability, not the immutable scientific result.
 * Repeated polls belong to one episode; a later failure gets a new identity. */
export function markPmSourceFailure(record: PmRecord, reason: string, atNs: string) {
  const missing = gap(record, 'source_revision', reason, 'REFRESH_SOURCE');
  if (missing.state === 'RESOLVED') { revise(missing); missing.state = 'OPEN'; missing.resolutionEvidenceRefs = []; }
  const prior = record.artifacts[healthKey];
  if (prior?.state === 'FAILED') return;
  const episode = (BigInt(String(prior?.episode ?? '0')) + 1n).toString();
  const failureEventId = identity('source-failure', [record.mandate.id, episode]);
  const originalDecisionId = record.currentDecisionId;
  record.artifacts[healthKey] = { state: 'FAILED', episode, failureEventId, originalDecisionId,
    policyRevisionId: record.policy.id, strategyVersionId: record.latestStrategyId,
    datasetRevisionIds: [...record.datasets], failedAtNs: atNs };
  record.events[failureEventId] = { id: failureEventId, kind: 'EVIDENCE_INVALIDATED', sourceRevisionRef: reason,
    availableAtNs: atNs, affectedMandateIds: [record.mandate.id], supersedesEventId: null };
  if (originalDecisionId) {
    const decision = record.decisions[originalDecisionId]!; revise(decision); decision.status = 'SUPERSEDED';
    if (!decision.reasonCodes.includes('source_unavailable')) decision.reasonCodes.push('source_unavailable');
    record.currentDecisionId = null;
  }
  const cycle = record.latestCycleId ? record.cycles[record.latestCycleId] : null;
  if (cycle && !['CONCLUDED', 'FAILED', 'SUPERSEDED'].includes(cycle.state)) {
    revise(cycle); cycle.state = 'SUPERSEDED'; cycle.reasonCodes.push('source_unavailable');
  }
  record.basisRevision = digest([record.basisRevision, failureEventId]);
}

/** Only a complete original historical report is eligible for redisplay.
 * This function neither evaluates investments nor reads the independent tape. */
export function historicalSourceRecovery(record: PmRecord, revision: Pm.DatasetRevision) {
  const health = record.artifacts[healthKey];
  if (health?.state !== 'FAILED' || record.mandate.capitalSource.kind !== 'RESEARCH_SCENARIO'
    || health.policyRevisionId !== record.policy.id || health.strategyVersionId !== record.latestStrategyId
    || canonical(health.datasetRevisionIds!) !== canonical(json(record.datasets))
    || record.datasets.length !== 1 || record.datasets[0] !== revision.id
    || typeof health.originalDecisionId !== 'string' || record.currentDecisionId
    || record.artifacts[`strategy-control:${record.latestStrategyId}`]?.action === 'pause'
    || Object.values(record.retirements).some(item => item.strategyVersionId === record.latestStrategyId)) return null;
  const decision = record.decisions[health.originalDecisionId];
  if (!decision || decision.status !== 'SUPERSEDED' || decision.policyRevisionId !== record.policy.id
    || decision.strategyVersionId !== record.latestStrategyId || !['CASH', 'HOLD', 'REBALANCE'].includes(decision.action)
    || decision.accountBasis || decision.targetCommandId) return null;
  const cycle = Object.values(record.cycles).find(item => item.outputDecisionId === decision.id);
  const frozen = cycle ? record.artifacts[cycle.id] : null;
  const comparison = record.comparisons[decision.comparisonRef];
  const campaign = typeof frozen?.campaignId === 'string' ? record.campaigns[frozen.campaignId] : null;
  if (!cycle || cycle.state !== 'CONCLUDED' || record.latestCycleId !== cycle.id || cycle.accountBasis
    || decision.cycleId !== cycle.id || decision.basisRevision !== cycle.basisRevision
    || cycle.policyRevisionId !== decision.policyRevisionId || frozen?.operatingForward === true
    || !campaign || campaign.state !== 'CONCLUDED' || Object.values(record.campaigns).at(-1)?.id !== campaign.id
    || record.artifacts[`campaign-control:${campaign.id}`]?.action === 'pause'
    || canonical(json(campaign.datasetRevisionIds)) !== canonical(json(cycle.datasetRevisionIds))
    || canonical(json(cycle.datasetRevisionIds)) !== canonical(json(record.datasets))
    || !comparison || comparison.evaluationContext !== 'HISTORICAL_JOINT_REPLAY' || comparison.conclusion !== 'SELECTED'
    || comparison.cycleId !== cycle.id || comparison.basisRevision !== cycle.basisRevision || comparison.policyRevisionId !== decision.policyRevisionId
    || decision.researchStartingStateRef !== comparison.scope.startingStateRef || !decision.researchStartingStateRef
    || decision.researchStartingStateRef !== identity('research-initial-condition', [record.mandate.id, record.mandate.capitalSource.capital])
    || canonical(json(comparison.scope.capital)) !== canonical(json(record.mandate.capitalSource.capital))
    || canonical(json(comparison.scope.datasetRevisionIds)) !== canonical(json(cycle.datasetRevisionIds))) return null;
  const configured = object(frozen!.configured); const originalRevision = object(configured.revision);
  const resultEvidenceRef = decision.jointReplayEvidenceRef ?? decision.solverEvidenceRef;
  if (originalRevision.id !== revision.id || originalRevision.contentRef !== revision.contentRef
    || !frozen!.nativeSource || !resultEvidenceRef || !record.evidence[resultEvidenceRef]) return null;
  const explanation = object(decision.explanationFacts);
  if (decision.action === 'REBALANCE' && (object(explanation.economicQualification ?? {}).outcome !== 'READY'
    || typeof explanation.independentEvidenceRef !== 'string' || record.evidence[explanation.independentEvidenceRef]?.role !== 'INDEPENDENT'
    || !record.artifacts[`independent-input:${decision.id}`]?.policy
    || canonical(record.artifacts[`independent-input:${decision.id}`]!.policy!) !== canonical(json(record.policy)))) return null;
  const source = object(frozen!.nativeSource);
  if (!source.catalogPrefix) return null;
  const signature = digest({ decisionId: decision.id, policy: record.policy, cycleId: cycle.id, comparison,
    source, configured, startingStateRef: decision.researchStartingStateRef, capitalSource: record.mandate.capitalSource,
    independentEvidenceRef: explanation.independentEvidenceRef ?? null });
  return { signature, decisionId: decision.id, cycleId: cycle.id, source, catalog: String(object(configured.data).catalog) };
}

/** Commit one recovery audit. Original evidence availability/expiry is retained;
 * the new time belongs only to this source verification receipt. */
export function recoverPmSource(record: PmRecord, revision: Pm.DatasetRevision, sourceEvidenceRef: string,
  atNs: string, taskId: number, verified: { signature: string; prefix: JsonObject } | null) {
  const health = record.artifacts[healthKey];
  if (health?.state !== 'FAILED') return null;
  const id = identity('source-recovery', [health.failureEventId, revision.id]);
  const event: Pm.BusinessEvent = { id: identity('event', id), kind: 'SOURCE_RECOVERED', sourceRevisionRef: id,
    availableAtNs: atNs, affectedMandateIds: [record.mandate.id], supersedesEventId: String(health.failureEventId) };
  const candidate = historicalSourceRecovery(record, revision);
  const restored = candidate && verified?.signature === candidate.signature ? candidate : null;
  // Changed data has its own DATA_REVISION admission below the recovery audit.
  // It must not claim an operating slot against the previous dataset/seed.
  const sameRevision = record.datasets.length === 1 && record.datasets[0] === revision.id
    && canonical(health.datasetRevisionIds!) === canonical(json(record.datasets));
  const forward = record.mandate.capitalSource.kind === 'ACCOUNT' && sameRevision && revision.revisionKind !== 'RETRACTION';
  const artifact: JsonObject = { schemaVersion: 1, failureEventId: health.failureEventId!, episode: health.episode!,
    datasetRevisionId: revision.id, contentRef: revision.contentRef, originalSourceEvidenceRef: sourceEvidenceRef,
    originalSourceAvailableAtNs: record.evidence[sourceEvidenceRef]!.availableAtNs, verifiedAtNs: atNs,
    outcome: !sameRevision ? 'NEW_DATA_REVISION_REQUIRES_OWN_CAMPAIGN' : restored ? 'HISTORICAL_REPORT_RESTORED' : forward ? 'OPERATING_REEVALUATION_REQUESTED' : 'NEEDS_EVIDENCE',
    originalDecisionId: health.originalDecisionId ?? null, restoredDecisionId: restored?.decisionId ?? null,
    historicalReportOnly: !forward, prefixVerification: restored ? verified!.prefix : null };
  record.artifacts[id] = artifact;
  record.evidence[id] = { id, sourceKind: 'DERIVED', role: 'CONTROL', runtime: { conversationId: record.conversationId, taskId },
    objectKey: id, contentDigest: digest(artifact), mediaType: 'application/json', sourceRevision: String(health.failureEventId),
    eventInterval: null, availableAtNs: atNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.pm.source-recovery/1', visibility: 'OWNER_ONLY' };
  record.artifacts[healthKey] = { ...json(health), state: 'HEALTHY', recoveryEventId: event.id, recoveryEvidenceRef: id };
  if (restored) {
    const decision = record.decisions[restored.decisionId]!; revise(decision); decision.status = 'CURRENT';
    decision.reasonCodes = decision.reasonCodes.filter(reason => reason !== 'source_unavailable');
    record.currentDecisionId = decision.id; record.basisRevision = record.cycles[restored.cycleId]!.basisRevision;
    for (const missing of Object.values(record.gaps)) if (missing.requiredCapability === 'source_recovery_evidence' && missing.state !== 'RESOLVED') {
      revise(missing); missing.state = 'RESOLVED'; missing.resolutionEvidenceRefs = [id];
    }
  } else if (!forward && sameRevision) {
    const missing = gap(record, 'source_recovery_evidence', 'same_source_recovered_without_reusable_complete_historical_result', 'COLLECT_AUTHORIZED_DATA');
    if (missing.state === 'RESOLVED') { revise(missing); missing.state = 'OPEN'; missing.resolutionEvidenceRefs = []; }
  }
  if (!forward) record.events[event.id] = event;
  return { event, forward };
}

export function recordOperatingRecoveryWait(record: PmRecord, event: Pm.BusinessEvent, reason: string) {
  if (event.kind !== 'SOURCE_RECOVERED') return;
  record.events[event.id] = event;
  const artifact = record.artifacts[event.sourceRevisionRef]; const evidence = record.evidence[event.sourceRevisionRef];
  if (artifact && evidence) { artifact.outcome = 'WAITING_EVIDENCE'; artifact.reasonCode = reason; evidence.contentDigest = digest(artifact); }
  gap(record, 'source_recovery_operating', reason, 'COLLECT_AUTHORIZED_DATA', event.id);
}
