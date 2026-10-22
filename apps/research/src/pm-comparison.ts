import type { JsonObject, Tx } from '@earendil-works/pi-durable';
import { compareResearchCandidates } from '../../../packages/contracts/src/decision.ts';
import { CampaignResearchDoc, fact, nowNs } from './pm-campaign.ts';
import { Study } from './documents.ts';
import type { ResearchRuntime } from './research-runtime.ts';

export async function compareCampaign(tx: Tx, api: Pick<ResearchRuntime, 'conversationId' | 'taskId'>) {
  const doc = await tx.doc(CampaignResearchDoc, api.conversationId); const binding = doc.binding;
  if (!binding) return null;
  if (doc.trials.some(trial => trial.state === 'RUNNING' || trial.state === 'ADMITTED')) throw new Error('research_trials_unsettled');
  const at = nowNs();
  const accountSelection = binding.policy.selectionRule.objectives.some(objective => objective.scope === 'ACCOUNT');
  const candidateTrialIds = doc.trials.filter(trial => accountSelection
    ? trial.purpose === 'JOINT_REPLAY' || trial.purpose === 'CAPITAL_SCALE'
    : trial.purpose !== 'ENSEMBLE_FIT' && trial.purpose !== 'CALIBRATION').map(trial => trial.id);
  const comparisonInput = { policy: binding.policy, scopes: binding.scopes, trials: doc.trials,
    candidateTrialIds,
    metricsByTrial: doc.observations, scopeByTrial: Object.fromEntries(doc.trials.map(trial => [trial.id, binding.scopes[0]!.id])),
    definitions: binding.definitions, frontierDefinitionIds: binding.frontierDefinitionIds, asOfNs: at };
  const compared = compareResearchCandidates(JSON.parse(JSON.stringify(comparisonInput)) as typeof comparisonInput);
  const id = `${binding.campaignId}:comparison:${api.taskId}`;
  const previous = doc.comparisons.find(item => item.id === id);
  if (previous) return JSON.parse(JSON.stringify(previous)) as typeof previous;
  const { pairwise: _pairwise, ...result } = compared;
  const comparison = { ...fact(id, at), campaignId: binding.campaignId, policyRevisionId: binding.policy.id, ...result, evidenceRefs: doc.trials.flatMap(trial => trial.evidenceRefs) };
  doc.comparisons.push(comparison);
  for (const trial of doc.trials.filter(trial => candidateTrialIds.includes(trial.id))) for (const evidenceRef of trial.evidenceRefs) doc.exposureRecords.push({
    ...fact(`${id}:exposure:${trial.id}`, at), evidenceRef, roleAtExposure: 'SELECTION', campaignId: binding.campaignId,
    policyRevisionId: binding.policy.id, exposedAtNs: at, consumer: 'SELECTION_METHOD', allowedUse: 'SELECT', resultingEvidenceRefs: [id],
  });
  return comparison;
}

/** Freeze every disclosed candidate before selection consumes independent data. */
export async function freezeSelection(tx: Tx, api: Pick<ResearchRuntime, 'conversationId' | 'taskId'>, trialId: number): Promise<JsonObject> {
    const study = await tx.doc(Study, api.conversationId);
    if (study.final && (study.final.taskId !== api.taskId || study.final.trialId !== trialId)) throw new Error('research_already_final');
    if (study.final?.comparison) return JSON.parse(JSON.stringify(study.final.comparison)) as JsonObject;
    if (!study.trials.some(trial => trial.taskId === trialId && trial.status === 'COMPUTED')) throw new Error('selected_trial_not_computed');
    const comparison = await compareCampaign(tx, api);
    const doc = await tx.doc(CampaignResearchDoc, api.conversationId);
    if (comparison && (comparison.recommendationStatus !== 'SELECTED'
      || doc.trials.find(trial => trial.id === comparison.recommendedTrialId)?.runtime.taskId !== trialId)) throw new Error('selected_trial_not_supported_by_comparison');
    if (study.trials.some(trial => trial.status === 'STARTED')) throw new Error('research_trials_unsettled');
    return JSON.parse(JSON.stringify({ comparisonId: comparison?.id ?? null, basis: study.trials.map(trial => ({ taskId: trial.taskId!, status: trial.status!,
      proposal: trial.proposal!, validation: trial.result ?? null, error: trial.error ?? null })), frozenAtNs: nowNs() })) as JsonObject;
}

/** Legacy inputs retain their original metric declaration; never infer preference. */
export function legacySelection(input: { specification?: { evaluation: { metric: string } }; policy: JsonObject }, trials: JsonObject[]): number | null {
  const key = input.specification?.evaluation.metric ?? (input.policy.selection_metric === 'validation.pearson_ic' ? 'pearson_ic' : null);
  if (!key) return null;
  const scored = trials.flatMap(trial => {
    if (trial.status !== 'COMPUTED' || !trial.result || typeof trial.result !== 'object' || Array.isArray(trial.result)) return [];
    const result = trial.result; let sources: JsonObject[] = [];
    if (key === 'pearson_ic' && result.portfolio === undefined && Array.isArray(result.partitions)) {
      sources = result.partitions.flatMap(value => {
        if (!value || typeof value !== 'object' || Array.isArray(value) || value.partition !== 'VALIDATION') return [];
        const metric = value.metric as JsonObject | undefined; const ic = metric?.pearson_ic;
        return ic && typeof ic === 'object' && !Array.isArray(ic) ? [ic] : [];
      });
    } else {
      const portfolio = result.portfolio as JsonObject | undefined;
      if (portfolio?.partition === 'VALIDATION' && Array.isArray(portfolio.statistics)) sources = portfolio.statistics.filter((value): value is JsonObject => !!value && typeof value === 'object' && !Array.isArray(value) && value.native_key === key);
    }
    const metric = sources.length === 1 ? sources[0] : undefined;
    return metric && (metric.status === undefined || metric.status === 'AVAILABLE') && typeof metric.value === 'number' && Number.isFinite(metric.value)
      ? [{ taskId: trial.taskId as number, value: metric.value }] : [];
  }).sort((a, b) => b.value - a.value);
  return scored.length && (scored.length === 1 || scored[0]!.value > scored[1]!.value) ? scored[0]!.taskId : null;
}
