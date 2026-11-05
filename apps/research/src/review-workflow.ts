import { withoutExecutionQuotas } from '../../../packages/contracts/src/execution-quotas.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { ReviewCheck, ReviewMetric, ReviewPolicy, ReviewReport, ReviewTargetKind } from '../../../packages/contracts/src/review.ts';
import { createReviewPolicy, validateReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import { canonical, Study } from './documents.ts';
import { reviewTargetVersion } from './review-identity.ts';
import type { ResearchInput } from './documents.ts';
import { modelBriefInput, modelFeedback } from './feedback.ts';
import { factorCohortSelectionEvidence } from './review-factor-account.ts';
import type { ReviewNativePlan } from './review-native-task.ts';
import { sourceReviewEvidence } from './review-source.ts';
import { ReviewLedger, latestTrialReview } from './review-documents.ts';
import { runIndependentReview } from './review-runtime.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { portfolioCohortSelectionEvidence } from './review-plans.ts';
import { nativeFactorReviewMetrics } from './review-science-adapter.ts';
import { checkExposure } from '../plugins/native-exposure.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
import { compileWasm } from '../plugins/native-wasm.ts';

const json = (value: unknown): JsonObject => JSON.parse(JSON.stringify(value)) as JsonObject;
function object(value: unknown): JsonObject | null { return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as JsonObject : null; }

/** Call only for an actual new admission. Request replay keeps its original input identity. */
export function freezeReviewPolicies(input: ResearchInput, nowNs = (BigInt(Date.now()) * 1_000_000n).toString()): ResearchInput {
  const frozen = structuredClone(input);
  if (input.policy.reviewPolicy !== undefined) validateReviewPolicy(input.policy.reviewPolicy);
  else if (input.policy.reviewPolicies === undefined) {
    frozen.policy.reviewPolicies = json({ FACTOR: createReviewPolicy('FACTOR', `review:factor:${nowNs}`, nowNs),
      PORTFOLIO: createReviewPolicy('PORTFOLIO', `review:portfolio:${nowNs}`, nowNs) });
  }
  if (frozen.policy.reviewPolicies !== undefined) {
    const policies = object(frozen.policy.reviewPolicies);
    if (!policies) throw new Error('review_policies_invalid');
    for (const kind of ['FACTOR', 'PORTFOLIO'] as const) {
      const policy = validateReviewPolicy(policies[kind]);
      if (policy.targetKind !== kind) throw new Error('review_policy_target_mismatch');
    }
  }
  return frozen;
}

function policyFor(input: ResearchInput, kind: ReviewTargetKind): ReviewPolicy | null {
  const policy = input.policy.reviewPolicy ?? object(input.policy.reviewPolicies)?.[kind];
  if (policy === undefined) return null;
  const resolved = validateReviewPolicy(JSON.parse(JSON.stringify(policy)));
  if (resolved.targetKind !== kind) throw new Error('review_policy_target_mismatch');
  return resolved;
}

/** Never expose independent results in the reviewer context, even if malformed input contains both partitions. */
export function validationReviewContext(input: ResearchInput, trial: JsonObject, trials: readonly JsonObject[]): JsonObject {
  const result = object(trial.result);
  const { data, policy } = modelBriefInput(input);
  const feedback = result ? modelFeedback(result) : null;
  const partitions = Array.isArray(feedback?.partitions) ? feedback.partitions.filter(value => object(value)?.partition === 'VALIDATION') : [];
  return json({ goal: input.goal, specification: input.specification ?? null,
    data: { source: data.source ?? null, revision: data.revision ?? null, modality: object(data.nativeInput)?.modality ?? 'BAR' }, policy,
    candidate: { trialId: trial.taskId, proposal: trial.proposal, status: trial.status,
      feedback: feedback ? { schema_version: feedback.schema_version ?? null, request: feedback.request ?? null, partitions,
        portfolio: object(feedback.portfolio)?.partition === 'VALIDATION' ? feedback.portfolio : null } : null },
    ledger: trials.map(item => ({ trialId: item.taskId, status: item.status, error: item.error ?? null, proposal: item.proposal })),
    limitations: ['Validation-only review. No independent result is available to this agent.',
      'AST differences do not establish independent economic exposures. Unsupported checks remain missing.'] });
}

/** Source checks are host computations; the model cannot assert PASS for them. */
export function reviewSourceChecks(input: ResearchInput, trial: JsonObject, trials: readonly JsonObject[], evidenceRef: string): ReviewCheck[] {
  const check = (id: string, status: ReviewCheck['status'], reason: string): ReviewCheck => ({ id, status, reason, evidenceRefs: [evidenceRef] });
  const proposal = object(trial.proposal); const result = object(trial.result);
  let implementation: ReviewCheck = check('implementation', 'UNKNOWN', 'implementation_not_audited');
  const models = typeof proposal?.model === 'string' ? [proposal.model] : Array.isArray(proposal?.members)
    ? proposal.members.flatMap(member => typeof object(member)?.model === 'string' ? [object(member)!.model as string] : []) : [];
  if (models.length) {
    try {
      const modules = models.map(model => new WebAssembly.Module(Uint8Array.from(compileWasm(model))));
      implementation = modules.some(module => WebAssembly.Module.imports(module).length)
        ? check('implementation', 'FAIL', 'external_wasm_imports') : check('implementation', 'PASS', 'original_models_compile_without_host_imports');
    } catch { implementation = check('implementation', 'FAIL', 'original_model_compilation_failed'); }
  }
  const raw = object(result?.request);
  let causalPrefix = false;
  if (raw && result) {
    try { const portfolio = Array.isArray(proposal?.members); const account = object(result.portfolio);
      const validAccount = account?.partition === 'VALIDATION' && object(account.provenance)?.account === 'ONE_NATIVE_SHARED_ACCOUNT';
      const partitions = result.partitions;
      causalPrefix = canonical(withoutExecutionQuotas(raw)) === canonical(withoutExecutionQuotas(iterationRequest(input.data, portfolio)))
      && (portfolio && validAccount && partitions === undefined || Array.isArray(partitions) && (!portfolio || validAccount) && partitions.every(row => { const partition = object(row); if (['TRAIN', 'VALIDATION'].includes(String(partition?.partition))) return true; const counts = object(object(partition?.metric)?.counts); return partition?.partition === 'HELDOUT' && counts !== null && Object.values(counts).every(value => value === 0 || value === '0') && object(object(partition?.metric)?.pearson_ic)?.value === null && (partition?.diagnostics === undefined || object(partition.diagnostics)?.observations === 0);  })); }
    catch { /* A missing or unsupported original request is not proof of causality. */ }
  }
  const hasFinalData = Array.isArray(result?.partitions) && result.partitions.some(row => { const partition = object(row); const counts = object(object(partition?.metric)?.counts); return partition?.partition === 'HELDOUT' && counts !== null && Object.values(counts).some(value => typeof value === 'string' && /^[0-9]+$/.test(value) ? BigInt(value) > 0n : typeof value === 'number' && value > 0); });
  const sharedAccount = object(result?.portfolio);
  const simulated = sharedAccount?.partition === 'VALIDATION' && object(sharedAccount.provenance)?.account === 'ONE_NATIVE_SHARED_ACCOUNT';
  return [
    check('provenance', 'UNKNOWN', 'historical_source_license_and_pit_require_independent_evidence'),
    implementation,
    check('trial_ledger', trials.some(item => item.status === 'STARTED') ? 'UNKNOWN' : 'PASS', 'complete_original_attempt_ledger_retained'),
    check('reproducibility', result && raw && trial.status === 'COMPUTED' ? 'PASS' : 'UNKNOWN', result && raw ? 'original_native_request_model_and_result_bound' : 'original_native_request_missing'),
    check('no_leakage', hasFinalData ? 'FAIL' : causalPrefix ? 'PASS' : 'UNKNOWN', hasFinalData ? 'heldout_observations_present_in_validation_source' : causalPrefix ? 'original_native_request_clipped_to_validation_prefix_no_heldout_partition' : 'original_causal_prefix_not_verified'),
    check('independent_window', object(input.data.sealed) ? 'PASS' : 'UNKNOWN', object(input.data.sealed) ? 'configured_unread_independent_partition' : 'unread_independent_partition_missing'),
    check('execution_constraints', simulated && !input.specification?.capital ? 'PASS' : 'UNKNOWN', simulated && !input.specification?.capital ? 'native_shared_account_completed_under_frozen_simulation_constraints' : 'registered_execution_and_capacity_evidence_missing'),
  ];
}

/** All direct/automatic finalizers share this fence. A failed review never consumes Sealed. */
export async function ensureTrialReview(api: ResearchRuntime, context: Context, trialId: number, options: { deferRefusalConclusion?: boolean } = {}): Promise<ReviewReport | null> {
  const frozen = await api.commit(async tx => {
    const study = await tx.doc(Study, api.conversationId);
    if (!study.input) throw new Error('research_input_missing');
    const trial = study.trials.find(item => item.taskId === trialId);
    if (trial?.status !== 'COMPUTED') throw new Error('selected_trial_not_computed');
    const kind: ReviewTargetKind = Array.isArray(object(trial.proposal)?.members) ? 'PORTFOLIO' : 'FACTOR';
    const policy = policyFor(study.input as ResearchInput, kind);
    if (!policy) return null; // Historical input retains its already-frozen semantics.
    const ledger = await tx.doc(ReviewLedger, api.conversationId);
    const previous = latestTrialReview(ledger, trialId);
    if (previous) return { previous: json(previous) as unknown as ReviewReport };
    if (study.trials.some(item => item.status === 'STARTED')) throw new Error('review_pending_trials');
    const version = reviewTargetVersion(study.input, trial, study.trials, policy);
    if (study.reviewSelection && (study.reviewSelection.trialId !== trialId || study.reviewSelection.version !== version)) throw new Error('review_selection_already_frozen');
    study.reviewSelection = { trialId, version };
    const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
    if (campaign.state === 'SEARCHING') { campaign.state = 'CLOSING'; campaign.closingReason = 'independent_review'; }
    const evidenceRef = `research:${api.conversationId}:trial:${trialId}:validation`;
    const raw = object(object(trial.result)?.request);
    const start = raw?.train_end_ns ?? object(study.input.data.catalog_ensemble_request)?.evaluation_start_ns ?? study.input.data.train_end_ns;
    const evaluationStartNs = typeof start === 'string' && /^\d+$/.test(start) ? start : '0';
    const binding = { trialId, version, policyId: policy.id, status: 'STARTED', report: null, createdAt: new Date().toISOString() };
    ledger.records[version] ??= binding;
    return { input: json(study.input) as ResearchInput, trial: json(trial), trials: JSON.parse(JSON.stringify(study.trials)) as JsonObject[],
      kind, policy, version, evidenceRef, evaluationStartNs, createdAt: ledger.records[version]!.createdAt as string };
  }, context);
  if (!frozen) return null;
  if ('previous' in frozen) return frozen.previous!;
  const sourceContext = validationReviewContext(frozen.input, frozen.trial, frozen.trials);
  const sourceEvidence = await sourceReviewEvidence(api, context, frozen.input.data);
  const checks = reviewSourceChecks(frozen.input, frozen.trial, frozen.trials, frozen.evidenceRef);
  for (const check of sourceEvidence.checks) { const index = checks.findIndex(row => row.id === check.id); if (index >= 0) { if (checks[index]!.status !== 'FAIL' && (check.id !== 'no_leakage' || checks[index]!.status === 'PASS' || check.status !== 'PASS')) checks[index] = check; } else checks.push(check); }
  const independentWindow = checks.find(check => check.id === 'independent_window')!;
  if (object(frozen.input.data.sealed)) {
    const independentSource = await sourceReviewEvidence(api, context, object(frozen.input.data.sealed)!, 'HELDOUT');
    const authorized = ['provenance', 'no_leakage'].every(id => independentSource.checks.find(check => check.id === id)?.status === 'PASS');
    if (!authorized) {
      independentWindow.status = independentSource.checks.some(check => check.status === 'FAIL') ? 'FAIL' : 'UNKNOWN';
      independentWindow.reason = 'independent_partition_source_quality_or_authorization_missing';
      independentWindow.evidenceRefs.push(...independentSource.evidenceRefs);
    } else {
      try { await checkExposure(api, context, object(frozen.input.data.sealed)!, true, undefined, undefined, false); independentWindow.status = 'PASS'; independentWindow.reason = 'own_sealed_source_authorized_and_original_exposure_ledger_confirms_unread_window'; independentWindow.evidenceRefs.push(...independentSource.evidenceRefs); }
      catch (error) { independentWindow.status = 'UNKNOWN'; independentWindow.reason = error instanceof Error && error.message === 'heldout_already_exposed' ? 'independent_window_already_exposed' : 'independent_window_cannot_be_verified'; }
    }
  }
  const componentDiagnostics = nativeFactorReviewMetrics(frozen.trial.result, frozen.evidenceRef);
  const metrics: ReviewMetric[] = frozen.kind === 'FACTOR' ? [...componentDiagnostics] : [];
  const evidence: JsonObject = { context: { ...sourceContext, evidenceRefs: [frozen.evidenceRef] },
    provenance: { status: checks.find(check => check.id === 'provenance')?.status === 'PASS' ? 'COMPLETE' : 'INCOMPLETE', source: sourceEvidence.summary, checks: json({ checks }).checks!, reason: checks.find(check => check.id === 'provenance')?.status === 'PASS' ? null : 'historical_provenance_requires_independent_evidence' },
    factor: { status: componentDiagnostics.length ? 'COMPLETE' : 'INCOMPLETE', reason: componentDiagnostics.length ? null : 'native_factor_diagnostics_not_available', metrics: JSON.parse(JSON.stringify(componentDiagnostics)), evidence: sourceContext.candidate! },
    selection: { status: 'INCOMPLETE', reason: 'comparable_net_return_matrix_required_for_dsr_pbo', ledger: sourceContext.ledger! },
    stress: { status: 'INCOMPLETE', reason: 'registered_stress_replays_required' },
    portfolio: { status: 'INCOMPLETE', reason: 'matched_shared_account_ablation_required' } };
  let selectionPlans: ReviewNativePlan[] = [];
  if (object(frozen.input.policy.strategy) && (frozen.kind === 'PORTFOLIO' || typeof object(frozen.trial.proposal)?.model === 'string')) {
    try {
      const reviewedCohort = frozen.kind === 'FACTOR'
        ? await factorCohortSelectionEvidence(api, context, frozen.input, frozen.trial, frozen.trials, frozen.evidenceRef)
        : await portfolioCohortSelectionEvidence(api, context, frozen.input, frozen.trial, frozen.trials, frozen.evidenceRef);
      selectionPlans = reviewedCohort.plans;
      evidence.selection = { status: 'INCOMPLETE', reason: selectionPlans.length ? 'native_selection_analysis_pending' : 'reference_cohort_net_return_evidence_missing', cohort: reviewedCohort.cohort };
      const account = reviewedCohort.selectedAccount;
      if (account && Array.isArray(account.metrics)) {
        for (const metric of account.metrics as unknown as ReviewMetric[]) {
          // Account robustness/coverage cannot overwrite the factor's own predictive diagnostics.
          if (!metrics.some(original => original.key === metric.key)) metrics.push(metric);
        }
        evidence.stress = { status: 'COMPLETE', evidenceRef: `${frozen.evidenceRef}#reference-strategy`, evidence: account.evidence! };
        evidence.portfolio = { status: 'COMPLETE', evidenceRef: `${frozen.evidenceRef}#reference-strategy`, evidence: account.evidence! };
        const execution = checks.find(check => check.id === 'execution_constraints')!;
        if (!frozen.input.specification?.capital) { execution.status = 'PASS'; execution.reason = 'registered_reference_strategy_shared_account_cash_baseline_and_adverse_replay_completed'; execution.evidenceRefs.push(`${frozen.evidenceRef}#reference-strategy`); }
      } else { evidence.stress = { status: 'INCOMPLETE', reason: 'registered_reference_strategy_replay_failed_or_insufficient' }; }
    } catch { evidence.stress = { status: 'INCOMPLETE', reason: 'registered_reference_strategy_evidence_unavailable_or_unsupported' }; }
  }
  const sciencePlans = selectionPlans;
  const report = await runIndependentReview(api, context, {
    target: { kind: frozen.kind, id: `research:${api.conversationId}:trial:${trialId}`, version: frozen.version,
      sourceEvidenceRefs: [...new Set([frozen.evidenceRef, ...metrics.map(metric => metric.evidenceRef), ...componentDiagnostics.map(metric => metric.evidenceRef), ...sciencePlans.map(plan => plan.evidenceRef), ...sourceEvidence.evidenceRefs, ...checks.flatMap(check => check.evidenceRefs)])], evaluationStartNs: frozen.evaluationStartNs },
    policy: frozen.policy, model: frozen.input.model, thinkingLevel: frozen.input.thinkingLevel,
    evidence, checks, metrics, sciencePlans, createdAt: frozen.createdAt,
  });
  await api.commit(async tx => {
    const ledger = await tx.doc(ReviewLedger, api.conversationId); const record = ledger.records[frozen.version]!;
    record.report = json(report); record.status = 'COMPLETED';
    const study = await tx.doc(Study, api.conversationId);
    if (!options.deferRefusalConclusion && report.decision !== 'PASS' && (!study.final || study.final.status === 'STOPPED' && study.final.trialId === trialId)) {
      study.final = { taskId: api.taskId, trialId, status: 'STOPPED', reason: 'independent_review_not_passed',
        result: null, reviewReportId: report.id, conclusion: report.decision, comparedTrials: study.trials.map(item => item.taskId!), scientificClaim: 'REVIEW_ONLY_NOT_INDEPENDENT_QUALIFICATION' };
      (await tx.doc(CampaignResearchDoc, api.conversationId)).state = 'CONCLUDED';
    }
  }, context);
  return report;
}

export async function requireTrialReview(api: ResearchRuntime, context: Context, trialId: number): Promise<boolean> {
  const report = await ensureTrialReview(api, context, trialId);
  return report === null || report.decision === 'PASS';
}
