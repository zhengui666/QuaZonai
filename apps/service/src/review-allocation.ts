/** Portfolio-of-portfolios is an optional research consumer, never a delivery authority. */
import { createHash, randomUUID } from 'node:crypto';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject } from '@earendil-works/pi-durable';
import type { FastifyInstance } from 'fastify';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { researchRuntime } from '../../research/src/research-runtime.ts';
import { runIndependentReview } from '../../research/src/review-runtime.ts';
import { retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { exactWeights, resolveStrategyPolicy, strategyMembers } from '../../research/src/strategy.ts';
import { checkExposure } from '../../research/plugins/native-exposure.ts';
import { normalizedRequest } from '../../research/plugins/native-wasm.ts';
import { blendTargets, executeNative, forecastMembers } from '../../research/plugins/native-portfolio.ts';
import type { ReviewAllocationStart, ReviewAllocationView, ReviewAllocationPolicy, ReviewAllocationResult, ReviewReport } from '../../../packages/contracts/src/review.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { buildReviewAllocationRequest, createReviewPolicy, scoreReview, validateReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import type { ServiceConfig } from './config.ts';
import { scienceTarget } from './config.ts';
import { Application, counter, decimalUnits, json, nowNs, object, portfolio, Problem, text } from './facts.ts';
import type { PortfolioFact } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';

const context = BACKGROUND_CONTEXT;
interface AllocationFact { original: ReviewAllocationStart; view: ReviewAllocationView; sources: PortfolioFact[]; policy: ReviewAllocationPolicy; native: JsonObject | null }
export const ReviewAllocations = defineDoc<{ records: Record<string, JsonObject> }>({ kind: 'qz.review-allocations', version: 1, scope: 'session', initial: () => ({ records: {} }) });
const Stages = defineDoc<{ results: Record<string, JsonObject>; reservedCalls: number }>({ kind: 'qz.review-allocation-stages', version: 1, scope: 'task', initial: () => ({ results: {}, reservedCalls: 0 }) });
const fact = (value: JsonObject): AllocationFact => value as unknown as AllocationFact;
export function portfolioReviewVersion(source: PortfolioFact): string {
  return createHash('sha256').update(canonical(json({ input: source.input, evidence: source.evidence, weights: source.weights ?? null, policy: source.policy }))).digest('hex');
}
export function approvedReviewPortfolio(source: PortfolioFact): ReviewReport {
  const report = source.view.review;
  if (source.deletedAt || source.view.status !== 'COMPUTED' || !report || report.target.id !== source.view.id || report.target.kind !== 'PORTFOLIO' || report.target.version !== portfolioReviewVersion(source)) throw new Problem('review_allocation_portfolio_not_approved', 422);
  const checked = scoreReview({ id: report.id, target: report.target, policy: report.policy, metrics: report.metrics, checks: report.checks, createdAt: report.createdAt });
  if (checked.decision !== 'PASS' || checked.totalScore !== report.totalScore) throw new Problem('review_allocation_portfolio_not_approved', 422);
  return report;
}
/** Native daily returns only. No curve interpolation, arithmetic or missing-day fill in TypeScript. */
export function validateReviewAllocationWeights(result: ReviewAllocationResult, policy: ReviewAllocationPolicy): void {
  const one = 10n ** 18n;
  const weights = new Map(result.members.map(member => [member.id, decimalUnits(member.weight)]));
  if (weights.size !== result.members.length || [...weights.values()].some(weight => weight < 0n || weight > one) || [...weights.values()].reduce((a, b) => a + b, 0n) !== one) throw new Problem('review_allocation_result_weights_invalid');
  if (policy.memberCaps.length !== weights.size || policy.memberCaps.some(cap => !weights.has(cap.id) || weights.get(cap.id)! > decimalUnits(cap.maximumWeight))) throw new Problem('review_allocation_result_cap_violated');
  if (policy.clusterCaps.some(cap => cap.members.reduce((sum, id) => sum + (weights.get(id) ?? one + 1n), 0n) > decimalUnits(cap.maximumWeight))) throw new Problem('review_allocation_result_cluster_violated');
  const current = new Map(policy.currentWeights.map(member => [member.id, decimalUnits(member.weight)]));
  if (current.size !== weights.size || [...weights.keys()].some(id => !current.has(id))) throw new Problem('review_allocation_result_current_mismatch');
  const turnover = [...weights].reduce((sum, [id, weight]) => { const delta = weight - current.get(id)!; return sum + (delta < 0n ? -delta : delta); }, 0n);
  if (turnover > decimalUnits(policy.maximumTurnover)) throw new Problem('review_allocation_result_turnover_violated');
  if (!Number.isFinite(result.lambda) || result.lambda <= 0 || !Number.isFinite(result.objective) || !Number.isFinite(result.primal_residual) || result.primal_residual < 0 || result.primal_residual > 1e-8 || !Number.isFinite(result.dual_residual) || result.dual_residual < 0 || result.dual_residual > 1e-8) throw new Problem('review_allocation_result_solver_invalid');
}

export function allocationReturnPanel(simulation: JsonObject): { times: string[]; returns: number[] } {
  if (simulation.returns_kind !== 'PORTFOLIO_DAILY' || simulation.returns_status !== 'OK' || !Array.isArray(simulation.returns) || simulation.returns.length < 30) throw new Problem('review_allocation_native_daily_returns_required', 422);
  const rows = simulation.returns.map(v => object(v, 'native_return'));
  const times = rows.map(r => counter(r.timestamp_ns, 'native_return_time'));
  if (new Set(times).size !== times.length || times.some((t, i) => i > 0 && BigInt(t) <= BigInt(times[i - 1]!)) || rows.some(r => typeof r.value !== 'number' || !Number.isFinite(r.value) || r.reason_code != null)) throw new Problem('review_allocation_native_returns_incomplete', 422);
  return { times, returns: rows.map(r => r.value as number) };
}
interface AllocationTaskInput { id: string; dataDir: string; binary: NativeTarget; sources: PortfolioFact[]; policy: ReviewAllocationPolicy; maximumScientificCalls: number; model: { provider: string; modelId: string }; reviewPolicy: import('../../../packages/contracts/src/review.ts').ReviewPolicy; independentData?: JsonObject }
export const ReviewAllocationTask = defineTask<AllocationTaskInput, { phase: 'compute' }, null>({
  name: 'qz.review-allocation', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, ctx) {
    let directory: string | undefined;
    try {
      directory = await mkdtemp(join(task.input.dataDir, 'review-allocation-'));
      const reserve = async (key: string, calls: number, action: () => Promise<JsonObject>): Promise<JsonObject> => {
        const state = await runtime.snapshot(Stages, task.id, ctx);
        if (state?.results[key]) return state.results[key];
        await runtime.commit(async tx => {
          const stages = await tx.doc(Stages, task.id);
          if (stages.reservedCalls + calls > task.input.maximumScientificCalls) throw new Problem('review_allocation_scientific_budget_exhausted');
          stages.reservedCalls += calls;
          const saved = fact((await tx.doc(ReviewAllocations)).records[task.input.id]!);
          saved.view.status = 'RUNNING'; saved.view.consumedScientificCalls = stages.reservedCalls;
          (await tx.doc(ReviewAllocations)).records[task.input.id] = json(saved);
          return undefined;
        }, ctx);
        const result = await action();
        await runtime.commit(async tx => { (await tx.doc(Stages, task.id)).results[key] = result; return undefined; }, ctx);
        return result;
      };
      const result = await withNativeRemoteTracking(runtime.taskId, runtime, ctx, async () => {
        const members: { id: string; report: ReviewReport; observations: number[] }[] = [];
        const simulations: JsonObject[] = []; const targetRows: JsonObject[][] = [];
        let commonTimes: string[] | null = null; let commonSelection: JsonObject | null = null;
        let catalog: string | null = null; let settings: JsonObject | null = null;
        for (const source of task.input.sources) {
          const report = approvedReviewPortfolio(source); const rows: JsonObject[][] = [];
          const sourceWeights = source.weights ?? (source.evidence.length === 1 ? ['1'] : null);
          if (!sourceWeights) throw new Problem('review_allocation_source_weights_missing');
          exactWeights(sourceWeights);
          for (const [index, evidence] of source.evidence.entries()) {
            const input = object(evidence.input, 'study_input');
            const data = object(input.data, 'study_data');
            const proposal = object(object(evidence.selected).proposal);
            const policy = resolveStrategyPolicy(proposal, source.policy);
            const request = normalizedRequest(data); const originalCatalog = text(data.catalog, 'validation_catalog');
            if (catalog !== null && catalog !== originalCatalog) throw new Problem('review_allocation_validation_catalog_mismatch');
            catalog = originalCatalog;
            const originalSettings = { ...object(policy.simulationSettings), starting_capital: source.view.capital };
            if (settings && canonical(settings) !== canonical(originalSettings)) throw new Problem('review_allocation_account_settings_mismatch');
            settings = originalSettings;
            const models = strategyMembers(proposal);
            const projected = await reserve(`forecast:${source.view.id}:${index}`, models.length + (models.length > 1 ? 1 : 0), async () => json(await forecastMembers(task.input.binary, originalCatalog, models, request, policy, source.view.currency, directory!, ctx, false, 'VALIDATION')));
            const selection = object(projected.selection);
            if (commonSelection && canonical(selection) !== canonical(commonSelection)) throw new Problem('review_allocation_validation_interval_mismatch');
            commonSelection = selection;
            if (!Array.isArray(projected.points)) throw new Problem('review_allocation_targets_missing');
            rows.push(projected.points.map(v => object(v)));
          }
          const combined = await reserve(`source-blend:${source.view.id}`, rows.length > 1 ? 1 : 0, async () => json(await blendTargets(task.input.binary, rows, sourceWeights, directory!, ctx)));
          const points = (combined.points as unknown[]).map(v => object(v)); targetRows.push(points);
          const simulation = await reserve(`source-simulate:${source.view.id}`, 1, () => executeNative(task.input.binary, 'simulate', { schema_version: 1, selection: commonSelection!, settings: settings!, target_points: points }, ['--catalog', catalog!], directory!, ctx));
          simulations.push(simulation); const panel = allocationReturnPanel(simulation);
          if (commonTimes && canonical(commonTimes) !== canonical(panel.times)) throw new Problem('review_allocation_return_alignment_mismatch');
          commonTimes = panel.times; members.push({ id: source.view.id, report, observations: panel.returns });
        }
        const policy = { ...task.input.policy, observationTimesNs: commonTimes!, observationEvidenceRefs: members.map(m => `allocation:${task.input.id}:validation:${m.id}`) };
        const request = buildReviewAllocationRequest(members, policy);
        const allocated = await reserve('allocate', 1, () => executeNative(task.input.binary, 'review-allocate', json(request), [], directory!, ctx));
        const allocation = allocated as unknown as ReviewAllocationResult;
        if (allocation.method !== 'QZ_REVIEW_QUALITY_DIVERSIFICATION_FIXED_DIAGONAL_10_V1' || allocation.solver_version !== 'clarabel/0.11.1' || allocation.kind !== 'PORTFOLIO' || allocation.members.length !== members.length || allocation.members.some((m, i) => m.id !== members[i]!.id || m.source_report_id !== members[i]!.report.id) || !allocation.requires_new_candidate || !allocation.requires_shared_account_study) throw new Problem('review_allocation_response_mismatch');
        validateReviewAllocationWeights(allocation, task.input.policy);
        const blend = await reserve('joint-blend', 1, async () => json(await blendTargets(task.input.binary, targetRows, allocation.members.map(m => m.weight), directory!, ctx, true)));
        const joint = await reserve('joint-shared-simulate', 1, () => executeNative(task.input.binary, 'simulate', { schema_version: 1, selection: commonSelection!, settings: settings!, target_points: blend.points! }, ['--catalog', catalog!], directory!, ctx));
        let independent: JsonObject | null = null;
        if (task.input.independentData) {
          // Reserve the unread window in the original permanent exposure ledger before granting read capability.
          await checkExposure(researchRuntime(runtime), ctx, task.input.independentData, true);
          const independentRows: JsonObject[][] = []; let independentSelection: JsonObject | null = null;
          const independentCatalog = text(task.input.independentData.catalog, 'independent_catalog');
          const independentRequest = normalizedRequest(task.input.independentData);
          const labelEnd = BigInt(counter(independentRequest.validation_end_ns, 'independent_start'));
          if (labelEnd <= BigInt(counter(commonSelection!.event_end_ns, 'validation_end'))) throw new Problem('review_allocation_independent_window_not_later');
          for (const source of task.input.sources) {
            const rows: JsonObject[][] = [];
            for (const [index, evidence] of source.evidence.entries()) {
              const original = normalizedRequest(object(object(evidence.input).data));
              const a = object(original.forecast); const b = object(independentRequest.forecast);
              const originalSelection = object(a.selection); const newSelection = object(b.selection);
              if (canonical({ ...a, selection: null }) !== canonical({ ...b, selection: null }) || canonical(originalSelection.bar_types!) !== canonical(newSelection.bar_types!)) throw new Problem('review_allocation_independent_forecast_contract_mismatch');
              const proposal = object(object(evidence.selected).proposal);
              const policy = resolveStrategyPolicy(proposal, source.policy); const models = strategyMembers(proposal);
              const projected = await reserve(`independent-forecast:${source.view.id}:${index}`, models.length + (models.length > 1 ? 1 : 0), async () => json(await forecastMembers(task.input.binary, independentCatalog, models, independentRequest, policy, source.view.currency, directory!, ctx, false, 'HELDOUT')));
              const selection = object(projected.selection);
              if (independentSelection && canonical(selection) !== canonical(independentSelection)) throw new Problem('review_allocation_independent_alignment_mismatch');
              independentSelection = selection;
              rows.push((projected.points as unknown[]).map(v => object(v)));
            }
            const combined = await reserve(`independent-source-blend:${source.view.id}`, rows.length > 1 ? 1 : 0, async () => json(await blendTargets(task.input.binary, rows, source.weights ?? ['1'], directory!, ctx)));
            independentRows.push((combined.points as unknown[]).map(v => object(v)));
          }
          const combined = await reserve('independent-joint-blend', 1, async () => json(await blendTargets(task.input.binary, independentRows, allocation.members.map(m => m.weight), directory!, ctx, true)));
          const simulation = await reserve('independent-joint-shared-simulate', 1, () => executeNative(task.input.binary, 'simulate', { schema_version: 1, selection: independentSelection!, settings: settings!, target_points: combined.points! }, ['--catalog', independentCatalog], directory!, ctx));
          independent = { simulation, selection: independentSelection!, qualification: 'ONCE_ONLY_FROZEN_CONFIGURATION_EVIDENCE_NOT_A_RELEASE', source: task.input.independentData };
        }
        return { allocation, joint, simulations, blend, selection: commonSelection!, policy, independent };
      });
      const sourceRef = `allocation:${task.input.id}:shared-validation`;
      const report = await runIndependentReview(researchRuntime(runtime), ctx, {
        target: { kind: 'ALLOCATION', id: task.input.id, version: createHash('sha256').update(canonical(json(result))).digest('hex'), sourceEvidenceRefs: [sourceRef], evaluationStartNs: String(result.selection.event_start_ns) },
        policy: task.input.reviewPolicy, model: task.input.model, createdAt: new Date().toISOString(), metrics: [],
        checks: task.input.reviewPolicy.requiredChecks.map(id => {
          const sourceChecks = task.input.sources.map(source => approvedReviewPortfolio(source).checks.find(check => check.id === id));
          const independentAvailable = result.independent !== null && object(result.independent.simulation).returns_status === 'OK';
          const unresolved = id === 'execution_constraints' || id === 'independent_window' && !independentAvailable;
          return { id, status: unresolved ? 'UNKNOWN' : id === 'independent_window' ? 'PASS' : sourceChecks.some(check => check?.status === 'FAIL') ? 'FAIL'
            : sourceChecks.every(check => check?.status === 'PASS' && check.evidenceRefs.length > 0) ? 'PASS' : 'UNKNOWN',
            reason: id === 'independent_window' ? independentAvailable ? 'fresh_window_reserved_and_evaluated_once_after_weight_freeze' : 'new_configuration_requires_fresh_independent_window'
              : id === 'execution_constraints' ? 'joint_capacity_and_stress_evidence_required'
              : 'inherited_verified_source_checks_with_original_report_bindings', evidenceRefs: [sourceRef] };
        }),
        evidence: { context: { targetKind: 'ALLOCATION', memberIds: task.input.sources.map(s => s.view.id), sourceRef },
          provenance: { status: 'COMPLETE', sourceRef }, factor: { status: 'INCOMPLETE', reason: 'see_original_factor_reports' },
          selection: { status: 'INCOMPLETE', reason: 'allocation_selected_on_validation_returns' }, stress: { status: 'INCOMPLETE', reason: 'independent_stress_evidence_required' },
          portfolio: { status: 'COMPLETE', sourceRef, statistics: result.joint.statistics ?? [], allocation: json(result.allocation) } },
      });
      const weights = { level: 'PORTFOLIO' as const, status: 'PROPOSED' as const, members: result.allocation.members.map(m => ({ id: m.id, weight: m.weight })),
        sourceReportIds: result.allocation.members.map(m => m.source_report_id), candidateId: task.input.id, requiresStudy: true as const, reason: result.independent ? 'independent_test_recorded_review_gate_still_applies' : 'new_configuration_requires_fresh_independent_window' };
      report.weights = [weights];
      await runtime.commit(async tx => {
        const saved = fact((await tx.doc(ReviewAllocations)).records[task.input.id]!);
        const current = await tx.doc(Application);
        for (const source of task.input.sources) {
          const live = current.portfolios[source.view.id];
          if (!live || portfolioReviewVersion(portfolio(live)) !== portfolioReviewVersion(source) || portfolio(live).deletedAt) throw new Problem('review_allocation_source_changed');
          if (approvedReviewPortfolio(portfolio(live)).id !== approvedReviewPortfolio(source).id) throw new Problem('review_allocation_source_review_changed');
        }
        saved.native = json(result); saved.view.status = 'COMPUTED'; saved.view.weights = weights; saved.view.review = report;
        (await tx.doc(ReviewAllocations)).records[task.input.id] = json(saved);
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, ctx);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, ctx);
      if (runtime.signal.aborted) throw error;
      await runtime.commit(async tx => {
        const saved = fact((await tx.doc(ReviewAllocations)).records[task.input.id]!);
        saved.view.status = 'FAILED'; saved.view.error = error instanceof Problem ? error.message : 'review_allocation_failed';
        (await tx.doc(ReviewAllocations)).records[task.input.id] = json(saved);
        return { status: 'terminal', outcome: { status: 'failed', error: { message: saved.view.error } } };
      }, ctx);
    } finally { if (directory) await rm(directory, { recursive: true, force: true }); }
  } },
  async abort(task, runtime, ctx) { await runtime.commit(async tx => {
    const saved = fact((await tx.doc(ReviewAllocations)).records[task.input.id]!); saved.view.status = 'STOPPED';
    (await tx.doc(ReviewAllocations)).records[task.input.id] = json(saved);
    return { status: 'terminal', outcome: { status: 'aborted' } };
  }, ctx); },
});
export const reviewAllocationExtension = defineExtension({ name: 'qz.review-allocation', tasks: [ReviewAllocationTask] });

export function registerReviewAllocationRoutes(app: FastifyInstance, host: ResearchHost, config: ServiceConfig): void {
  app.get('/api/v3/review-allocations', async () => Object.values((await host.harness.snapshot(ReviewAllocations, context))?.records ?? {}).map(v => fact(v).view));
  app.get<{ Params: { id: string } }>('/api/v3/review-allocations/:id', async request => {
    const found = (await host.harness.snapshot(ReviewAllocations, context))?.records[request.params.id];
    if (!found) throw new Problem('review_allocation_not_found', 404); return fact(found).view;
  });
  app.post('/api/v3/review-allocations', async (request, reply) => {
    const input = object(request.body) as unknown as ReviewAllocationStart;
    const allowed = ['requestId', 'name', 'portfolioIds', 'memberCaps', 'clusterCaps', 'currentWeights', 'maximumTurnover', 'maximumScientificCalls', 'reviewPolicy', 'independentDatasetId'];
    if (Object.keys(input).some(key => !allowed.includes(key))) throw new Problem('review_allocation_unknown_input');
    text(input.requestId, 'request_id'); text(input.name, 'name');
    if (!Array.isArray(input.portfolioIds) || input.portfolioIds.length < 2 || input.portfolioIds.length > 64 || new Set(input.portfolioIds).size !== input.portfolioIds.length || input.portfolioIds.some(id => typeof id !== 'string' || !id.trim())) throw new Problem('review_allocation_members_required');
    if (!Number.isSafeInteger(input.maximumScientificCalls) || input.maximumScientificCalls < 1 || input.maximumScientificCalls > 4096 || !Array.isArray(input.memberCaps) || !Array.isArray(input.clusterCaps) || !Array.isArray(input.currentWeights) || typeof input.maximumTurnover !== 'string') throw new Problem('review_allocation_constraints_required');
    const id = randomUUID(); const createdAt = new Date().toISOString(); const frozenAtNs = nowNs();
    const saved = await host.harness.commit(async tx => {
      const document = await tx.doc(ReviewAllocations);
      const existing = Object.values(document.records).map(fact).find(f => f.original.requestId === input.requestId);
      if (existing) { if (canonical(json(existing.original)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409); return existing.view; }
      const application = await tx.doc(Application); assertBusinessAdmission(application);
      const sources = input.portfolioIds.map(id => { const found = application.portfolios[id]; if (!found) throw new Problem('portfolio_not_found', 404); const source = portfolio(found); approvedReviewPortfolio(source); return structuredClone(source); });
      if (sources.some(s => s.policy.kind === 'catalog-ensemble')) throw new Problem('review_allocation_catalog_ensemble_unsupported', 422);
      if (sources.some(s => s.view.currency !== sources[0]!.view.currency || s.view.capital !== sources[0]!.view.capital || s.view.datasetId !== sources[0]!.view.datasetId)) throw new Problem('review_allocation_common_account_basis_required', 422);
      if (!config.defaultModel) throw new Problem('review_model_not_configured', 422);
      const policy: ReviewAllocationPolicy = { id: `allocation-policy:${id}`, frozenAtNs, kind: 'PORTFOLIO', observationTimesNs: [], observationEvidenceRefs: [], memberCaps: input.memberCaps, clusterCaps: input.clusterCaps, currentWeights: input.currentWeights, maximumTurnover: input.maximumTurnover };
      const independentDataset = input.independentDatasetId === undefined ? undefined : config.datasets.find(dataset => dataset.id === input.independentDatasetId);
      if (input.independentDatasetId !== undefined && (!independentDataset || independentDataset.currency !== sources[0]!.view.currency || independentDataset.nativeModality === 'TRADE_TICK')) throw new Problem('review_allocation_independent_dataset_invalid');
      const reviewPolicy = input.reviewPolicy ? validateReviewPolicy(input.reviewPolicy) : createReviewPolicy('ALLOCATION', `review-policy:${id}`, frozenAtNs);
      if (reviewPolicy.targetKind !== 'ALLOCATION') throw new Problem('review_policy_target_mismatch');
      const taskId = await tx.createTask(ReviewAllocationTask, { id, dataDir: config.dataDir, binary: scienceTarget(config), sources, policy, maximumScientificCalls: input.maximumScientificCalls, model: config.defaultModel, reviewPolicy, ...(independentDataset ? { independentData: json(independentDataset.data.sealed ? object(independentDataset.data.sealed) : independentDataset.data) } : {}) },
        { conversationId: sources[0]!.view.researchIds[0] as ConversationId, ownership: { kind: 'conversation' } });
      const view: ReviewAllocationView = { id, requestId: input.requestId, name: input.name, status: 'QUEUED', portfolioIds: input.portfolioIds, taskId, consumedScientificCalls: 0, weights: null, review: null, error: null, createdAt };
      document.records[id] = json({ original: input, view, sources, policy, native: null } satisfies AllocationFact); return view;
    }, context);
    return reply.code(202).send(saved);
  });
}
