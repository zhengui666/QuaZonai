import type { Context } from '@earendil-works/chord';
import type { JsonObject, Tx } from '@earendil-works/pi-durable';
import type { ResearchTrial } from '../../../packages/contracts/src/pm.ts';
import { CampaignResearchDoc, fact, nowNs } from './pm-campaign.ts';
import { selectionObservations } from './pm-metrics.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import { canonical } from './documents.ts';

export async function admitCampaignTrial(tx: Tx, api: Pick<ResearchRuntime, 'conversationId' | 'taskId'>, proposal: JsonObject, rejected = false): Promise<void> {
  const doc = await tx.doc(CampaignResearchDoc, api.conversationId);
  const binding = doc.binding;
  if (!binding || doc.trials.some(trial => trial.runtime.taskId === api.taskId)) return;
  const id = `${binding.campaignId}:trial:${api.taskId}`;
  const at = nowNs();
  const hypothesis = doc.hypotheses.find(item => item.id === proposal.hypothesisId);
  if (!hypothesis || hypothesis.state === 'RETIRED' || hypothesis.state === 'REFUTED') throw new Error('registered_hypothesis_required');
  const parents = proposal.parentTrialIds ?? [];
  if (!Array.isArray(parents) || !parents.every(parent => typeof parent === 'string' && doc.trials.some(trial => trial.id === parent && trial.state !== 'REJECTED'))) throw new Error('trial_lineage_invalid');
  const purpose = proposal.purpose ?? (Array.isArray(proposal.members) ? 'JOINT_REPLAY' : 'FEASIBILITY');
  if (!['FEASIBILITY', 'CALIBRATION', 'ENSEMBLE_FIT', 'JOINT_REPLAY', 'CAPITAL_SCALE', 'ABLATION', 'ROBUSTNESS'].includes(String(purpose))) throw new Error('trial_purpose_invalid');
  const trial: ResearchTrial = { ...fact(id, at), campaignId: binding.campaignId, hypothesisId: hypothesis.id,
    parentTrialIds: parents as string[], runtime: { conversationId: api.conversationId, taskId: api.taskId }, state: rejected ? 'REJECTED' : 'RUNNING',
    purpose: purpose as ResearchTrial['purpose'], proposalRef: `${id}:proposal`, datasetRevisionIds: binding.datasetRevisionIds,
    protocolRef: binding.protocolRef, methodVersion: 'qz.native.v1', countedAttempt: !rejected, resourceUsageRef: `${id}:usage`, evidenceRefs: [],
    errorCode: rejected ? 'trial_budget_exhausted' : null };
  doc.trials.push(trial);
  if (rejected) return;
  hypothesis.state = 'TESTING';
  const movement = { ...fact(`${id}:trial-consumption`, at), budgetId: binding.budget.id, campaignId: binding.campaignId,
    hypothesisId: hypothesis.id, trialId: id, kind: 'CONSUME' as const, resource: 'TRIAL' as const, amount: '1', currency: null,
    fromReservationId: null, toReservationId: null, measurementEvidenceRef: `${id}:usage:TRIAL`, reasonCode: 'scientific_attempt_admitted' };
  doc.budgetMovements.push(movement);
  doc.resourceMeasurements.push({ id: `${id}:usage:TRIAL`, trialId: id, resource: 'TRIAL', status: 'AVAILABLE', value: '1', reasonCodes: [] });
  for (const resource of ['MODEL_COST', 'DATA_BYTES'] as const) doc.resourceMeasurements.push({ id: `${id}:usage:${resource}`, trialId: id, resource, status: 'UNAVAILABLE', value: null, reasonCodes: ['meter_not_available'] });
  doc.allocations.push({ ...fact(`${id}:allocation`, at), campaignId: binding.campaignId, budgetId: binding.budget.id,
    consideredHypothesisIds: doc.hypotheses.filter(item => item.state !== 'RETIRED' && item.state !== 'REFUTED').map(item => item.id),
    selectedHypothesisIds: [hypothesis.id], nextExperimentPurpose: trial.purpose, expectedInformationEvidenceRef: null,
    resourceEstimateRef: `${id}:usage`, incumbentComparisonRef: doc.comparisons.at(-1)?.id ?? null,
    reasonCodes: ['registered_hypothesis_incremental_question', 'original_trial_budget_available'], action: 'EXPERIMENT' });
}

export async function settleCampaignTrial(tx: Tx, api: Pick<ResearchRuntime, 'conversationId' | 'taskId'>, result: JsonObject | null, errorCode: string | null, elapsedSeconds: string | null): Promise<void> {
    const doc = await tx.doc(CampaignResearchDoc, api.conversationId); const binding = doc.binding;
    const trial = doc.trials.find(item => item.runtime.taskId === api.taskId);
    if (!binding || !trial || ['COMPUTED', 'FAILED', 'REJECTED'].includes(trial.state)) return;
    const at = nowNs();
    trial.state = result ? 'COMPUTED' : 'FAILED'; trial.errorCode = errorCode;
    const measurementId = `${trial.id}:usage:NATIVE_SECONDS`;
    doc.resourceMeasurements.push({ id: measurementId, trialId: trial.id, resource: 'NATIVE_SECONDS', status: elapsedSeconds === null ? 'UNAVAILABLE' : 'AVAILABLE', value: elapsedSeconds, reasonCodes: [elapsedSeconds === null ? 'interrupted_compute_time_unavailable' : 'scientific_compute_elapsed_wall_seconds'] });
    if (elapsedSeconds !== null) doc.budgetMovements.push({ ...fact(`${trial.id}:native-consumption`, at), budgetId: binding.budget.id, campaignId: binding.campaignId,
      hypothesisId: trial.hypothesisId, trialId: trial.id, kind: 'CONSUME', resource: 'NATIVE_SECONDS', amount: elapsedSeconds, currency: null,
      fromReservationId: null, toReservationId: null, measurementEvidenceRef: measurementId, reasonCode: 'measured_scientific_compute' });
    if (result) {
      const evidenceRef = `${trial.id}:validation`;
      trial.evidenceRefs.push(evidenceRef);
      doc.observations[trial.id] = selectionObservations(result, binding, evidenceRef, at);
      doc.exposureRecords.push({ ...fact(`${trial.id}:model-exposure`, at), evidenceRef, roleAtExposure: trial.purpose === 'ENSEMBLE_FIT' || trial.purpose === 'CALIBRATION' ? 'TRAINING' : 'SELECTION',
        campaignId: binding.campaignId, policyRevisionId: binding.policy.id, exposedAtNs: at, consumer: 'RESEARCH_AGENT',
        allowedUse: trial.purpose === 'ENSEMBLE_FIT' || trial.purpose === 'CALIBRATION' ? 'FIT' : 'SELECT', resultingEvidenceRefs: [evidenceRef] });
    }
}

export async function registerHypothesis(api: ResearchRuntime, context: Context, value: {
  economicFamilyId: string; parentHypothesisId?: string; mechanism: string; prediction: string; falsification: string;
  requiredDataCapabilities: string[]; incrementalQuestion: string;
}) {
  return api.commit(async tx => {
    const doc = await tx.doc(CampaignResearchDoc, api.conversationId);
    if (!doc.binding) throw new Error('campaign_binding_required');
    const id = `${doc.binding.campaignId}:hypothesis:${api.taskId}`;
    const previous = doc.hypotheses.find(item => item.id === id);
    const content = { ...value, parentHypothesisId: value.parentHypothesisId ?? null };
    if (previous) {
      const { economicFamilyId, parentHypothesisId, mechanism, prediction, falsification, requiredDataCapabilities, incrementalQuestion } = previous;
      if (canonical({ economicFamilyId, parentHypothesisId, mechanism, prediction, falsification, requiredDataCapabilities, incrementalQuestion }) !== canonical(content)) throw new Error('hypothesis_identity_conflict');
      return JSON.parse(JSON.stringify(previous)) as typeof previous;
    }
    if (doc.state !== 'SEARCHING') throw new Error('research_search_closed');
    if (content.parentHypothesisId && !doc.hypotheses.some(item => item.id === content.parentHypothesisId)) throw new Error('hypothesis_parent_missing');
    for (const key of ['economicFamilyId', 'mechanism', 'prediction', 'falsification', 'incrementalQuestion'] as const) if (!content[key].trim()) throw new Error(`hypothesis_${key}_required`);
    const hypothesis = { ...fact(id, nowNs()), campaignId: doc.binding.campaignId, ...content, state: 'PROPOSED' as const, evidenceRefs: [] };
    doc.hypotheses.push(hypothesis); return hypothesis;
  }, context);
}
