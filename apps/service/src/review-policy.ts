import { randomUUID } from 'node:crypto';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc } from '@earendil-works/pi-durable';
import type { DocumentReader } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { FastifyInstance } from 'fastify';
import type { ReviewPolicy, ReviewReferenceObservation } from '../../../packages/contracts/src/review.ts';
import { createReviewPolicy, deriveReviewPolicyFromReference, validateReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { counter, json, nowNs, object, Problem, text } from './facts.ts';
import { ReviewAllocations } from './review-allocation.ts';

const context = BACKGROUND_CONTEXT;
export const ReviewPolicies = defineDoc<{ records: Record<string, JsonObject> }>({ kind: 'qz.review-policies', version: 1, scope: 'session', initial: () => ({ records: {} }) });
const Policies = ReviewPolicies;
/** Calibrated anchors must be the immutable output of the retained-reference consumer. */
export async function resolveRegisteredReviewPolicy(reader: DocumentReader, value: unknown): Promise<ReviewPolicy> {
  const policy = validateReviewPolicy(value);
  if (policy.calibration === null) return policy;
  const registry = await reader.snapshot(Policies, context);
  const found = Object.values(registry?.records ?? {}).find(record => object(record.policy).id === policy.id);
  if (!found || canonical(found.policy!) !== canonical(json(policy))) throw new Problem('review_policy_not_registered', 422);
  return JSON.parse(JSON.stringify(found.policy)) as ReviewPolicy;
}
/** Reference metrics are resolved from original Validation reports, never accepted as request fields. */
export function registerReviewPolicyRoutes(app: FastifyInstance, host: ResearchHost): void {
  app.get('/api/v3/review-policies', async () => Object.values((await host.harness.snapshot(Policies, context))?.records ?? {}).map(record => record.policy));
  app.post('/api/v3/review-policies', async (request, reply) => {
    const input = object(request.body);
    if (Object.keys(input).some(key => !['requestId', 'kind', 'researchIds', 'allocationIds', 'evaluationStartNs'].includes(key))) throw new Problem('review_policy_unknown_input');
    text(input.requestId, 'request_id');
    if (input.kind !== 'FACTOR' && input.kind !== 'PORTFOLIO' && input.kind !== 'ALLOCATION') throw new Problem('review_policy_kind_invalid');
    const evaluationStartNs = counter(input.evaluationStartNs, 'evaluation_start');
    const ids = input.kind === 'ALLOCATION' ? input.allocationIds : input.researchIds;
    if (!Array.isArray(ids) || ids.length < 30 || new Set(ids).size !== ids.length
      || ids.some(id => input.kind === 'ALLOCATION' ? typeof id !== 'string' || !id : !Number.isSafeInteger(id) || Number(id) < 0)
      || (input.kind === 'ALLOCATION' ? input.researchIds !== undefined : input.allocationIds !== undefined)) throw new Problem('review_reference_roots_required');
    const prior = (await host.harness.snapshot(Policies, context))?.records[String(input.requestId)];
    if (prior) {
      if (canonical(prior.input!) !== canonical(input)) throw new Problem('request_id_conflict', 409);
      return prior.policy;
    }
    const records: ReviewReferenceObservation[] = [];
    // Include the original result regardless of the candidate's score/decision. Never retain winners only.
    if (input.kind === 'ALLOCATION') {
      const allocations = await host.harness.snapshot(ReviewAllocations, context);
      for (const id of ids as string[]) {
        const saved = allocations?.records[id];
        if (!saved) throw new Problem('review_allocation_not_found', 404);
        const view = object(saved.view); const report = view.review as unknown as import('../../../packages/contracts/src/review.ts').ReviewReport;
        const native = object(saved.native, 'review_reference_native');
        if (!report || report.target.kind !== 'ALLOCATION' || report.target.id !== id) throw new Problem('review_reference_report_missing', 422);
        const endNs = counter(object(native.selection).event_end_ns, 'validation_end');
        const sources = Array.isArray(saved.sources) ? saved.sources.map(source => String(object(object(source).view).id)).sort() : [];
        if (!sources.length || report.metrics.some(metric => /heldout|sealed|independent/i.test(metric.evidenceRef))) throw new Problem('review_reference_independent_evidence_forbidden', 422);
        records.push({ rootId: `allocation-sources:${sources.join('|')}`, endNs, evidenceRefs: report.target.sourceEvidenceRefs, metrics: report.metrics });
      }
    }
    for (const researchId of input.kind === 'ALLOCATION' ? [] : ids as number[]) {
      const state = await host.state(researchId);
      if (!state.research?.input || state.research.deletedAt) throw new Problem('research_not_found', 404);
      const candidates = Object.values(state.reviews?.records ?? {}).filter(item => item.report && typeof item.report === 'object' && !Array.isArray(item.report));
      const record = candidates.find(item => object(item.report).target && object(object(item.report).target).kind === input.kind);
      if (!record) throw new Problem('review_reference_report_missing', 422);
      const report = record.report as unknown as import('../../../packages/contracts/src/review.ts').ReviewReport;
      const trial = state.research.trials.find(item => item.taskId === record.trialId);
      if (!trial?.result) throw new Problem('review_reference_trial_missing', 422);
      const result = object(trial.result); const nativeRequest = object(result.registered_request ?? result.request);
      const endNs = counter(nativeRequest.validation_end_ns ?? state.research.input.data.validation_end_ns, 'validation_end');
      if (report.metrics.some(metric => /heldout|sealed|independent/i.test(metric.evidenceRef))) throw new Problem('review_reference_independent_evidence_forbidden', 422);
      records.push({ rootId: `research:${researchId}`, endNs, evidenceRefs: report.target.sourceEvidenceRefs, metrics: report.metrics });
    }
    let policy: ReviewPolicy;
    try { policy = deriveReviewPolicyFromReference(createReviewPolicy(input.kind, `review-policy:${randomUUID()}`, nowNs()), records, evaluationStartNs); }
    catch (error) {
      if (error instanceof Error && /^review_reference_/.test(error.message)) throw new Problem(error.message, 422);
      throw error;
    }
    const saved = await host.harness.commit(async tx => {
      const document = await tx.doc(Policies); const old = document.records[String(input.requestId)];
      if (old) { if (canonical(old.input!) !== canonical(input)) throw new Problem('request_id_conflict', 409); return old.policy as unknown as ReviewPolicy; }
      document.records[String(input.requestId)] = json({ input, policy }); return policy;
    }, context);
    return reply.code(201).send(saved);
  });
}
