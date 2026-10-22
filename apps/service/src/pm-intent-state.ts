import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { canonicalOwnerBoundary, ownerBoundaryRefines } from '../../../packages/contracts/src/pm-intent.ts';
import type { PmIntentClarification } from '../../../packages/contracts/src/pm-intent.ts';
import { canonical } from '../../research/src/documents.ts';
import { json, object, Problem, text } from './facts.ts';
import { digest, fact, gap, identity, revise } from './pm-facts.ts';
import type { PmRecord } from './pm-facts.ts';
import { assessStage } from './pm-registry.ts';

export function clarificationInput(value: unknown): PmIntentClarification {
  const input = object(value);
  if (Object.keys(input).some(key => !['requestId', 'expectedRevision', 'statement', 'boundaries', 'resolvedAmbiguities'].includes(key))) throw new Problem('intent_clarification_field_unsupported');
  text(input.requestId, 'request_id'); text(input.expectedRevision, 'expected_revision'); text(input.statement, 'statement');
  if (!Array.isArray(input.boundaries) || !Array.isArray(input.resolvedAmbiguities) || !input.resolvedAmbiguities.every(item => typeof item === 'string' && item.trim())) throw new Problem('intent_clarification_invalid');
  for (const item of input.boundaries) {
    const boundary = object(item); if (Object.keys(boundary).some(key => !['kind', 'sourceStatement', 'value'].includes(key))) throw new Problem('intent_boundary_field_unsupported');
    text(boundary.sourceStatement, 'source_statement');
  }
  return json(input) as unknown as PmIntentClarification;
}

export function mergeOwnerBoundaries(previous: readonly Pm.OwnerBoundary[], proposals: readonly Pm.OwnerBoundary[]): Pm.OwnerBoundary[] {
  const merged = previous.map(boundary => canonicalOwnerBoundary(json(boundary)));
  const seen = new Set<string>();
  for (const proposal of proposals) {
    const next = canonicalOwnerBoundary(json(proposal)); const key = `${next.kind}:${next.sourceStatement}`;
    if (seen.has(key)) throw new Problem('intent_boundary_duplicate'); seen.add(key);
    const index = merged.findIndex(item => item.kind === next.kind && item.sourceStatement === next.sourceStatement);
    if (index < 0) merged.push(next);
    else {
      if (!ownerBoundaryRefines(merged[index]!, next)) throw new Problem('owner_boundary_cannot_be_relaxed', 409);
      if (canonical(json(merged[index]!.value)) !== canonical(json(next.value))) merged[index] = next;
    }
  }
  return merged;
}

/** Original mandate URI stays stable; immutable snapshots retain each previous
 * revision. Policies have a new identity because scientific inputs pin it. */
export function reviseOwnerIntent(record: PmRecord, boundaries: Pm.OwnerBoundary[], statements: string[], evidenceId: string, atNs: string) {
  const prior = identity('mandate-policy-history', [record.mandate.id, record.mandate.revision, record.policy.id]);
  retainIntentEvidence(record, prior, json({ mandate: record.mandate, policy: record.policy }), atNs, 'qz.pm.mandate-policy-history/1');
  const oldPolicyId = record.policy.id;
  revise(record.mandate); record.mandate.ownerBoundaries = boundaries;
  record.mandate.interpretation = [record.mandate.ownerGoal, ...statements.map(statement => `Owner clarification: ${statement}`)].join('\n');
  record.policy = { ...record.policy, ...fact(identity('policy', [oldPolicyId, evidenceId, boundaries]), atNs), supersedesId: oldPolicyId,
    validDomain: { ...object(record.policy.validDomain), ownerBoundaries: json(boundaries), ownerClarificationStatements: statements } };
  record.searchFamily.methodVariantRefs.push(record.policy.id);
  const priorStrategy = record.strategies[record.latestStrategyId]!;
  const strategy = { ...priorStrategy, ...fact(identity('strategy', [priorStrategy.id, record.policy.id]), atNs), supersedesId: priorStrategy.id,
    policyRevisionId: record.policy.id, modelRefs: [], fittedEvidenceRefs: [] };
  const assessment = assessStage(strategy, 'RESEARCH', [], ['owner_intent_revision_requires_new_evaluation']);
  strategy.pipelineAssessmentId = assessment.id; record.assessments[assessment.id] = assessment;
  record.strategies[strategy.id] = strategy; record.latestStrategyId = strategy.id;
  record.basisRevision = digest([record.basisRevision, record.mandate.revision, record.policy.id]);
  if (record.currentDecisionId) { const decision = record.decisions[record.currentDecisionId]!; revise(decision); decision.status = 'SUPERSEDED'; decision.reasonCodes.push('owner_intent_revision'); record.currentDecisionId = null; }
  for (const cycle of Object.values(record.cycles)) if (!['CONCLUDED', 'FAILED', 'SUPERSEDED'].includes(cycle.state)) { revise(cycle); cycle.state = 'SUPERSEDED'; cycle.reasonCodes.push('owner_intent_revision'); }
  for (const boundary of boundaries) if (boundary.kind !== 'LEVERAGE') gap(record, 'owner_boundary_execution', 'owner_boundary_execution_unavailable', 'UNSUPPORTED', boundary.id);
  return prior;
}

export function retainIntentEvidence(record: PmRecord, id: string, artifact: JsonObject, atNs: string, schemaId = 'qz.pm.owner-intent-clarification/1') {
  record.artifacts[id] = artifact;
  record.evidence[id] = { id, sourceKind: 'DERIVED', role: 'CONTROL', runtime: { conversationId: record.conversationId, taskId: null },
    objectKey: id, contentDigest: digest(artifact), mediaType: 'application/json', sourceRevision: record.mandate.revision, eventInterval: null,
    availableAtNs: atNs, clockPrecision: 'NANOSECOND', schemaId, visibility: 'OWNER_ONLY' };
}
