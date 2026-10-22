import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { BudgetMovement, CandidateComparison, ComparisonScope, DecisionPolicyRevision, ExposureRecord, Hypothesis, MetricDefinition, MetricObservation, OwnerBoundary, ResearchBudget, ResearchTrial, ResearchWorkAllocation, StrategyDefinition, VersionedFact } from '../../../packages/contracts/src/pm.ts';
import { validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';

/** DTOs contain only JSON; the mapped type retains their fields for Durable. */
export type JsonFact<T> = T extends readonly (infer V)[] ? JsonFact<V>[] : T extends object ? { [K in keyof T]: JsonFact<T[K]> } : T;
export type ResearchCampaignBinding = {
  campaignId: string;
  mandateRevisionId: string;
  policy: JsonFact<DecisionPolicyRevision>;
  budget: JsonFact<ResearchBudget>;
  scopes: JsonFact<ComparisonScope>[];
  definitions: JsonFact<MetricDefinition>[];
  frontierDefinitionIds: string[];
  datasetRevisionIds: string[];
  protocolRef: string;
  intakeRequired?: boolean;
  independentConsumer?: 'RESEARCH' | 'PM_PORTFOLIO';
};
export type StrategyIntake = {
  strategy: JsonFact<StrategyDefinition>; ownerBoundaries: JsonFact<OwnerBoundary>[]; sourceGoal: string;
  ownerBoundaryStatements: { kind: OwnerBoundary['kind']; sourceStatement: string }[];
  support: 'CONDITIONAL' | 'UNSUPPORTED' | 'NEEDS_CLARIFICATION'; missingCapabilities: string[]; ambiguities: string[]; taskId: number;
};
export type ResourceMeasurement = {
  id: string; trialId: string; resource: BudgetMovement['resource'];
  status: 'AVAILABLE' | 'UNAVAILABLE'; value: string | null; reasonCodes: string[];
};
export type CampaignResearchState = {
  binding: ResearchCampaignBinding | null;
  state: 'SEARCHING' | 'CLOSING' | 'EVALUATING_INDEPENDENT' | 'CONCLUDED';
  hypotheses: JsonFact<Hypothesis>[];
  trials: JsonFact<ResearchTrial>[];
  budgetMovements: JsonFact<BudgetMovement>[];
  resourceMeasurements: ResourceMeasurement[];
  allocations: JsonFact<ResearchWorkAllocation>[];
  comparisons: JsonFact<CandidateComparison>[];
  observations: Record<string, JsonFact<MetricObservation>[]>;
  exposureRecords: JsonFact<ExposureRecord>[];
  closureTaskId: TaskId<JsonObject> | null;
  previousClosures?: { taskId: TaskId<JsonObject>; reason: string | null }[];
  closingReason: string | null;
  intake: StrategyIntake | null;
  orchestration: { policyVersion: 'qz.research.resources/1'; maximumModelTurns: number; generationTaskIds: number[] } | null;
};
/** Research-owned facts; the service owns Mandate/Campaign lifecycle records. */
export const CampaignResearchDoc = defineDoc<CampaignResearchState>({
  kind: 'qz.research-campaign', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  initial: () => ({ binding: null, state: 'SEARCHING', hypotheses: [], trials: [], budgetMovements: [], resourceMeasurements: [], allocations: [], comparisons: [], observations: {}, exposureRecords: [], closureTaskId: null, closingReason: null, orchestration: null, intake: null }),
});
export function fact(id: string, nowNs: string): VersionedFact {
  return { schemaVersion: 1, id, revision: '1', supersedesId: null, createdAtNs: nowNs, producerVersion: 'qz.research.pm.v1' };
}
export function nowNs(): string { return (BigInt(Date.now()) * 1_000_000n).toString(); }

export function researchCampaignBinding(value: unknown): ResearchCampaignBinding {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('campaign_binding_required');
  const binding = value as ResearchCampaignBinding;
  if (binding.independentConsumer !== undefined && !['RESEARCH', 'PM_PORTFOLIO'].includes(binding.independentConsumer)) throw new Error('campaign_independent_consumer_invalid');
  for (const key of ['campaignId', 'mandateRevisionId', 'protocolRef'] as const) if (typeof binding[key] !== 'string' || !binding[key].trim()) throw new Error(`campaign_${key}_required`);
  const policy = validatePmEntity('DecisionPolicyRevision', binding.policy);
  const budget = validatePmEntity('ResearchBudget', binding.budget);
  if (policy.mandateRevisionId !== binding.mandateRevisionId) throw new Error('campaign_mandate_mismatch');
  if (!Array.isArray(binding.scopes) || binding.scopes.length !== 1) throw new Error('campaign_single_registered_scope_required');
  const scopes = binding.scopes.map(scope => validatePmEntity('ComparisonScope', scope));
  if (!['SELECTION', 'INNER_VALIDATION'].includes(scopes[0]!.role)) throw new Error('campaign_selection_scope_required');
  if (!Array.isArray(binding.definitions) || !binding.definitions.length) throw new Error('campaign_metric_definitions_required');
  const definitions = binding.definitions.map(definition => validatePmEntity('MetricDefinition', definition));
  if (new Set(definitions.map(item => item.id)).size !== definitions.length) throw new Error('campaign_duplicate_metric_definition');
  if (!Array.isArray(binding.frontierDefinitionIds) || !binding.frontierDefinitionIds.every(id => definitions.some(item => item.id === id))) throw new Error('campaign_frontier_definition_missing');
  if (!Array.isArray(binding.datasetRevisionIds) || !binding.datasetRevisionIds.length || !binding.datasetRevisionIds.every(id => typeof id === 'string' && id.length > 0)) throw new Error('campaign_dataset_revisions_required');
  if (JSON.stringify([...binding.datasetRevisionIds].sort()) !== JSON.stringify([...scopes[0]!.datasetRevisionIds].sort())) throw new Error('campaign_scope_dataset_mismatch');
  // Measured per-attempt wall seconds are evidence, not a cross-restart hard
  // timer. Unsupported hard caps must never silently become admission hints.
  if (budget.nativeComputeSeconds !== null || budget.modelSpend !== null || budget.dataBytes !== null) throw new Error('campaign_resource_meter_unavailable');
  return structuredClone({ ...binding, policy, budget, scopes, definitions });
}
