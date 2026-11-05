import { createHash } from 'node:crypto';
import type { ConversationId, JsonObject, Tx } from '@earendil-works/pi-durable';
import type { ReviewedFactorSource, ReviewedFactorSources } from '../../../packages/contracts/src/reviewed-factors.ts';
import type { ReviewReport } from '../../../packages/contracts/src/review.ts';
import { scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import { Study } from '../../research/src/documents.ts';
import { reviewTargetVersion } from '../../research/src/review-identity.ts';
import { ReviewLedger } from '../../research/src/review-documents.ts';
import { compileWasm, iterationRequest } from '../../research/plugins/native-wasm.ts';
import { strategyMembers } from '../../research/src/strategy.ts';
import { ReviewSourceEvidence, reviewSourceDataDigest } from '../../research/src/review-source.ts';
import type { CapturedReviewSource } from '../../research/src/review-source.ts';
import type { ServiceConfig } from './config.ts';
import { assertCurrentReviewSource } from './review-source.ts';
import { factorReference } from './factors.ts';
import { counter, object, Problem } from './facts.ts';

/** Pure verification of source snapshots obtained inside the admission transaction. */
export function authorizeReviewedFactor(factorId: string, study: { input: JsonObject | null; deletedAt?: string;
  deletedFactors?: Record<string, string>; trials: JsonObject[] }, records: Record<string, JsonObject>, fitEndNs: string): ReviewedFactorSource {
  const reference = factorReference(factorId);
  if (!study.input || study.deletedAt || study.deletedFactors?.[factorId]) throw new Problem('reviewed_factor_not_found', 404);
  const trial = study.trials.find(item => item.taskId === reference.trialId);
  if (!trial || trial.status !== 'COMPUTED') throw new Problem('reviewed_factor_not_computed', 422);
  const members = strategyMembers(object(trial.proposal));
  // A group review is not transferable to one constituent factor.
  if (reference.memberIndex !== 0 || members.length !== 1) throw new Problem('reviewed_factor_single_source_required', 422);
  const recordsForTrial = Object.values(records).filter(record => record.trialId === reference.trialId && record.status === 'COMPLETED');
  if (recordsForTrial.length !== 1) throw new Problem('reviewed_factor_pass_report_required', 422);
  const record = recordsForTrial[0]!;
  const report = record.report as unknown as ReviewReport;
  const expectedRef = `research:${reference.researchId}:trial:${reference.trialId}:validation`;
  if (!report || report.target?.kind !== 'FACTOR' || report.target.id !== `research:${reference.researchId}:trial:${reference.trialId}`
    || report.target.version !== record.version || report.policyId !== record.policyId
    || !Array.isArray(report.target.sourceEvidenceRefs) || !report.target.sourceEvidenceRefs.includes(expectedRef)) throw new Problem('reviewed_factor_report_binding_invalid', 422);
  const originalVersion = reviewTargetVersion(study.input, trial, study.trials, report.policy);
  if (originalVersion !== report.target.version) throw new Problem('reviewed_factor_report_original_mismatch', 422);
  const refs = [...report.target.sourceEvidenceRefs, ...report.metrics.map(metric => metric.evidenceRef)];
  if (refs.some(ref => /heldout|sealed|independent/i.test(ref))) throw new Problem('reviewed_factor_independent_evidence_forbidden', 422);
  const snapshot = JSON.parse(JSON.stringify(report)) as ReviewReport;
  const scored = scoreReview(snapshot);
  if (report.decision !== 'PASS' || scored.decision !== 'PASS' || scored.totalScore !== report.totalScore
    || scored.qualityCoefficient !== report.qualityCoefficient || scored.qualityCoefficient <= 0) throw new Problem('reviewed_factor_pass_report_required', 422);
  const result = object(trial.result);
  const request = object(result.registered_request ?? result.request);
  const data = object(study.input.data);
  const sourceEndNs = counter(request.validation_end_ns ?? data.validation_end_ns, 'reviewed_factor_validation_end');
  if (BigInt(sourceEndNs) <= BigInt(report.target.evaluationStartNs) || BigInt(sourceEndNs) >= BigInt(fitEndNs)) throw new Problem('reviewed_factor_evidence_not_before_training_cutoff', 422);
  const modelSource = members[0]!.model;
  const modelSha256 = createHash('sha256').update(compileWasm(modelSource)).digest('hex');
  return { factorId, factorVersionId: `factor:${reference.researchId}:${reference.trialId}`, sourceResearchId: reference.researchId,
    sourceTrialId: reference.trialId, modelSource, modelSha256, report: snapshot, sourceEndNs };
}

/** Owner service admission only. The same transaction freezes sources and the new
 * research request, so deletion or a different report cannot race acceptance. */
export async function freezeReviewedFactors(tx: Tx, ids: unknown, data: JsonObject, config?: ServiceConfig): Promise<ReviewedFactorSources | null> {
  if (ids === undefined) return null;
  if (!Array.isArray(ids) || ids.length < 2 || ids.some(id => typeof id !== 'string')
    || new Set(ids).size !== ids.length) throw new Problem('reviewed_factor_ids_invalid');
  if (data.catalog_ensemble_request !== undefined || data.catalog_scan_request !== undefined) throw new Problem('reviewed_factor_full_prediction_panel_required', 422);
  const request = iterationRequest(data, true);
  const fitEndNs = counter(request.train_end_ns, 'reviewed_factor_training_cutoff');
  const members: ReviewedFactorSource[] = [];
  for (const id of ids as string[]) {
    const reference = factorReference(id);
    const study = await tx.doc(Study, reference.researchId as ConversationId);
    const ledger = await tx.doc(ReviewLedger, reference.researchId as ConversationId);
    const authorized = authorizeReviewedFactor(id, study, ledger.records, fitEndNs);
    if (config) {
      const saved = (await tx.doc(ReviewSourceEvidence, reference.researchId as ConversationId)).captured;
      if (!saved) throw new Problem('reviewed_factor_current_source_required', 409);
      const captured = JSON.parse(JSON.stringify(saved)) as CapturedReviewSource;
      if (captured.partition !== 'VALIDATION' || captured.inputDataDigest !== reviewSourceDataDigest(object(study.input!.data))
        || !authorized.report.target.sourceEvidenceRefs.includes(`source:${captured.datasetId}:${captured.metadataDigest}`)) throw new Problem('reviewed_factor_current_source_binding_invalid', 409);
      const configured = config.datasets.find(dataset => dataset.ready && dataset.id === captured.datasetId);
      if (!configured) throw new Problem('reviewed_factor_current_source_unavailable', 409);
      await assertCurrentReviewSource(configured, captured);
    }
    members.push(authorized);
  }
  if (new Set(members.map(member => member.factorVersionId)).size !== members.length) throw new Problem('reviewed_factor_duplicate_version');
  return { schemaVersion: 1, fitEndNs, members };
}
