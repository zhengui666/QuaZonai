import { createHash } from 'node:crypto';
import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { canonical } from '../../research/src/documents.ts';
import { json, nowNs, Problem } from './facts.ts';

/** Business facts share the official Durable store; task state stays with Durable. */
export const PmDocument = defineDoc<JsonObject>({
  kind: 'qz.pm', version: 1, scope: 'session',
  initial: () => json({ mandates: {}, datasets: {}, commands: {} }),
});
export interface PmCommandReceipt { original: JsonObject; mandateId: string; resourceId: string }
export interface PmRecord {
  /** Owner deletion hides the workspace; original facts and receipts remain. */
  deletedAt?: string;
  input: Pm.PmMandateStart;
  conversationId: number;
  mandate: Pm.MandateRevision;
  policy: Pm.DecisionPolicyRevision;
  budget: Pm.ResearchBudget;
  definition: Pm.StrategyDefinition;
  strategies: Record<string, Pm.StrategyVersion>;
  assessments: Record<string, Pm.PipelineAssessment>;
  datasets: string[];
  events: Record<string, Pm.BusinessEvent>;
  campaigns: Record<string, Pm.ResearchCampaign>;
  cycles: Record<string, Pm.DecisionCycle>;
  decisions: Record<string, Pm.PortfolioDecision>;
  comparisons: Record<string, Pm.OperatingComparison>;
  capacities: Record<string, Pm.CapacityEstimate>;
  ensembles: Record<string, Pm.EnsembleRevision>;
  evidence: Record<string, Pm.EvidenceRef>;
  artifacts: Record<string, JsonObject>;
  outcomes: Record<string, Pm.ObservedOutcome>;
  attributions: Record<string, Pm.OutcomeAttribution>;
  retirements: Record<string, Pm.RetirementDisposition>;
  commands: Record<string, Pm.TargetCommand>;
  gaps: Record<string, Pm.EvidenceGap>;
  currentDecisionId: string | null;
  latestCycleId: string | null;
  latestStrategyId: string;
  basisRevision: string;
  monitorTaskId: number | null;
  derivedTrials: Record<string, Pm.ResearchTrial>;
  budgetMovements: Record<string, Pm.BudgetMovement>;
  searchFamily: Pm.SearchFamily;
}
export interface PmState {
  mandates: Record<string, PmRecord>;
  datasets: Record<string, { revision: Pm.DatasetRevision; data: JsonObject; policy: JsonObject; strategy: JsonObject | null; sourceManifest?: JsonObject }>;
  commands: Record<string, PmCommandReceipt>;
}
export const pmState = (value: unknown): PmState => (value ?? { mandates: {}, datasets: {}, commands: {} }) as PmState;
export const pmRecord = (state: PmState, id: string): PmRecord => {
  const value = state.mandates[id]; if (!value) throw new Problem('mandate_not_found', 404); return value;
};
export function assertPmNotDeleted(record: PmRecord) {
  if (record.deletedAt) throw new Problem('mandate_deleted', 409);
}
export function visiblePmRecord(state: PmState, id: string): PmRecord {
  const record = pmRecord(state, id);
  if (record.deletedAt) throw new Problem('mandate_not_found', 404);
  return record;
}
export function digest(value: unknown): string {
  return createHash('sha256').update(canonical(json(value))).digest('hex');
}
export function identity(kind: string, value: unknown): string { return `${kind}:${digest(value)}`; }
export function fact(id: string, atNs = nowNs()): Pm.VersionedFact {
  return { schemaVersion: 1, id, revision: '0', supersedesId: null, createdAtNs: atNs, producerVersion: 'qz-pm/1' };
}
export function revise<T extends Pm.VersionedFact>(value: T): T { value.revision = (BigInt(value.revision) + 1n).toString(); return value; }
export function gap(record: PmRecord, capability: string, reasonCode: string, remedy: Pm.EvidenceGap['permittedRemedy'], entityId = record.mandate.id) {
  const id = identity('gap', [entityId, capability, reasonCode]);
  record.gaps[id] ??= { ...fact(id), affectedEntityId: entityId, requiredCapability: capability, reasonCode,
    permittedRemedy: remedy, dataAuthorizationRef: null, state: remedy === 'UNSUPPORTED' ? 'UNRESOLVABLE' : 'OPEN', resolutionEvidenceRefs: [] };
  return record.gaps[id]!;
}
export function currentStrategy(record: PmRecord) { return record.strategies[record.latestStrategyId]!; }
export function ensureRevision(record: PmRecord, expected: unknown) {
  if (typeof expected !== 'string' || expected !== record.mandate.revision) throw new Problem('mandate_revision_conflict', 409);
}
