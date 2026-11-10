import { runScientificStage, scientificStageResult, type ScientificStageOperation } from './scientific-stages.ts';
import type { Context } from '@earendil-works/chord';
import { defineDocFamily, type ConversationId, type JsonObject } from '@earendil-works/pi-durable';
import type { ReviewCheck, ReviewMetric, ReviewReport } from '../../../packages/contracts/src/review.ts';
import { allocationCapacityCheck, portfolioReviewVersion } from './review-allocation.ts';
import { runIndependentReview } from '../../research/src/review-runtime.ts';
import { reviewSourceChecks, validationReviewContext } from '../../research/src/review-workflow.ts';
import { readTrustedSourceChecks, trustedSourceChecks } from '../../research/src/review-source.ts';
import { canonical, Study, type ResearchInput } from '../../research/src/documents.ts';
import { checkExposure } from '../../research/plugins/native-exposure.ts';
import { normalizedRequest } from '../../research/plugins/native-wasm.ts';
import type { ResearchRuntime } from '../../research/src/research-runtime.ts';
import { assertPortfolioIndependentContract, type PortfolioTaskInput } from './native.ts';
import { nativeReviewReturnPanel, adverseReviewRequest } from './review-account-science.ts';
import { Application, counter, json, object, portfolio, Problem } from './facts.ts';

/** Persistent original scientific receipts keyed by their official Task identity.
 * Session lifetime preserves validation/selection evidence after Task retirement. */
export const PortfolioReviewScience = defineDocFamily<{ selectionBinding: JsonObject | null }, null>({
  kind: 'qz.portfolio-review-bindings', version: 1, scope: 'session', family: true,
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, initial: () => ({ selectionBinding: null }),
});
export function reservePortfolioReviewScience(input: PortfolioTaskInput, api: ResearchRuntime, context: Context, key: string, calls: number, operation: ScientificStageOperation): Promise<JsonObject> {
  return runScientificStage(api, context, key, calls, { binary: input.binary, dataDir: input.dataDir, ...operation });
}

/** Only original Validation inputs enter Review; component independent results stay private. */
export async function reviewPortfolio(input: PortfolioTaskInput, api: ResearchRuntime, context: Context): Promise<ReviewReport | null> {
  if (!input.review) return null;
  const sources = Array.isArray(input.evidence) ? input.evidence : [input.evidence];
  const contexts: JsonObject[] = []; const sourceChecks: ReviewCheck[][] = [];
  const refs = sources.map(source => `research:${source.researchId}:trial:${object(source.selected).taskId}:validation`);
  for (const [index, source] of sources.entries()) {
    const original = object(source.input) as ResearchInput; const selected = object(source.selected);
    const study = await api.snapshot(Study, source.researchId as ConversationId, context); const trials = study?.trials ?? [selected];
    contexts.push(validationReviewContext(original, selected, trials));
    const checks = reviewSourceChecks(original, selected, trials, refs[index]!);
    for (const check of await readTrustedSourceChecks(api, source.researchId as ConversationId, original.data, context)) {
      const index = checks.findIndex(item => item.id === check.id); if (index >= 0) checks[index] = check; else checks.push(check);
    }
    sourceChecks.push(checks);
  }
  for (const reference of sourceChecks.flat().flatMap(check => check.evidenceRefs)) if (!refs.includes(reference)) refs.push(reference);
  const application = await api.snapshot(Application, context); const saved = application?.portfolios[input.id];
  if (!saved) throw new Problem('portfolio_not_found'); const current = portfolio(saved); const version = portfolioReviewVersion(current);
  const checks: ReviewCheck[] = input.review.policy.requiredChecks.map(id => {
    const matches = sourceChecks.flat().filter(check => check.id === id);
    return { id, status: matches.some(check => check.status === 'FAIL') ? 'FAIL' : matches.length === sources.length && matches.every(check => check.status === 'PASS') ? 'PASS' : 'UNKNOWN', reason: matches.map(check => check.reason).join('; ') || 'portfolio_check_not_available', evidenceRefs: refs };
  });
  const nativeRef = `portfolio:${input.id}:${version}:validation`; refs.push(nativeRef);
  const request = object(object(object(sources[0]!.selected).result).request ?? {});
  const start = typeof request.train_end_ns === 'string' ? request.train_end_ns : '0';
  const setCheck = (id: string, status: ReviewCheck['status'], reason: string) => { const check = checks.find(item => item.id === id); if (check) { check.status = status; check.reason = reason; check.evidenceRefs = [nativeRef]; } };
  setCheck('independent_window', 'UNKNOWN', 'fresh_portfolio_independent_window_required');
  let metrics: ReviewMetric[] = []; let scientific: JsonObject = { status: 'INCOMPLETE', reason: 'validation_shared_account_replay_not_completed' };
  const reserve = (key: string, calls: number, operation: ScientificStageOperation) => reservePortfolioReviewScience(input, api, context, key, calls, operation);
  try {
    if (input.review.stressMethod !== 'QZ_REVIEW_ONE_BAR_NATIVE_SLIPPAGE_V1') throw new Problem('portfolio_review_stress_method_not_frozen');
    for (const source of sources) {
      const original = object(source.input) as ResearchInput; const raw = object(object(source.selected).result).request;
      const { sealed: _sealed, ...validationData } = original.data;
      await checkExposure(api, context, { ...validationData, request: raw! }, false);
    }
    const forecastCalls = sources.reduce((sum, source) => { const proposal = object(object(source.selected).proposal); const members = Array.isArray(proposal.members) ? proposal.members.length : 1; return sum + members + (members > 1 ? 1 : 0); }, 0);
    const built = await reserve('candidate', forecastCalls + 3, { kind: 'portfolio', input: input, partition: 'VALIDATION' });
    const native = object(built.native); const simulationRequest = object(native.simulationRequest, 'portfolio_review_explicit_shared_request_required');
    const candidate = object(native.simulation); const catalog = String(native.catalog);
    const cashRequest = structuredClone(simulationRequest);
    cashRequest.target_points = (cashRequest.target_points as JsonObject[]).map(point => ({ ...point, cash_weight: '1', targets: (point.targets as JsonObject[]).map(target => ({ ...target, weight: '0' })) }));
    const cash = await reserve('cash', 1, { kind: 'native', command: 'simulate', input: cashRequest, args: ['--catalog', catalog] });
    const stressRequest = adverseReviewRequest(simulationRequest);
    const stress = await reserve('stress', 1, { kind: 'native', command: 'simulate', input: stressRequest, args: ['--catalog', catalog] });
    const diagnostics = await reserve('diagnostics', 2, { kind: 'account-metrics', candidate, baseline: cash, stress, evidenceRef: nativeRef });
    metrics = diagnostics.metrics as unknown as ReviewMetric[];
    scientific = { status: 'COMPLETE', candidate: native, cash, stress, stressRequest, diagnostics: diagnostics.evidence!, capacity: built.capacity!, method: 'QZ_PORTFOLIO_REVIEW_VALIDATION_CASH_SLIPPAGE_BAR_DELAY_V1', unsupported: ['Venue fee multipliers are unsupported: source fee rates remain original.', 'BAR liquidity capacity is conditional historical evidence, not order-book depth or economic capacity.'] };
    const capacityCheck = allocationCapacityCheck(object(built.capacity), input.capital, stress, nativeRef);
    setCheck('execution_constraints', capacityCheck.status, capacityCheck.reason);
    if (input.review.independentData) {
      assertPortfolioIndependentContract(input, input.review.independentData);
      const independent = normalizedRequest(input.review.independentData);
      if (BigInt(counter(independent.validation_end_ns, 'independent_start')) < BigInt(counter(object(simulationRequest.selection).event_end_ns, 'validation_end'))) throw new Problem('portfolio_independent_window_not_later');
      await checkExposure(api, context, input.review.independentData, true, undefined, undefined, false);
      if (input.review.independentSource) {
        const source = input.review.independentSource;
        const validated = await reserve('independent-source', 1, { kind: 'native', command: 'review-source', input: { schema_version: 1, metadata: source.metadata, observed_at: source.receivedAt }, args: [] });
        const sourceChecks = trustedSourceChecks(source, validated, input.review.independentData, (BigInt(Date.now())*1_000_000n).toString());
        if (sourceChecks.length >= 2 && sourceChecks.every(check => check.status === 'PASS')) setCheck('independent_window', 'PASS', 'fresh_registered_window_and_its_own_source_authority_verified_without_reading_values');
        else if (sourceChecks.some(check => check.status === 'FAIL')) setCheck('provenance', 'FAIL', sourceChecks.map(check => check.reason).join(';'));
      }
    }
    // Other configurations, including failures, remain in the denominator.
    const peers = Object.values(application!.portfolios).map(portfolio).filter(peer => peer.input.datasetId === current.input.datasetId && peer.view.capital === input.capital && peer.view.currency === input.currency);
    const panel = nativeReviewReturnPanel(candidate); const cohort: JsonObject[] = [{ id: input.id, returns: panel.returns }]; let complete = true;
    for (const peer of peers) if (peer.view.id !== input.id) {
      const old = peer.view.taskId === null ? null : await scientificStageResult(api, context, peer.view.taskId, 'candidate');
      if (!old) { complete = false; continue; }
      const source = object(old.native); const original = object(source.simulationRequest);
      if (canonical((original.selection!)) !== canonical((simulationRequest.selection!)) || canonical(original.settings!) !== canonical(simulationRequest.settings!)) { complete = false; continue; }
      const returns = nativeReviewReturnPanel(object(source.simulation)); if (canonical(returns.times) !== canonical(panel.times)) { complete = false; continue; }
      cohort.push({ id: peer.view.id, returns: returns.returns });
    }
    // Persist the actual comparison input before giving the numerical task read
    // capability. A restart cannot add newly admitted peers to the same analysis.
    const binding = await api.commit(async tx => {
      const state = await tx.doc(PortfolioReviewScience, String(api.taskId), null);
      state.selectionBinding ??= { source: { cohort, simulationRequest, candidate }, analysis: { kind: 'SELECTION', total_trials: peers.length, ledger_complete: complete, selected_trial_id: input.id, trials: cohort, timestamps_ns: panel.times, frequency: 'DAY', block_length: 1 } };
      return JSON.parse(JSON.stringify(state.selectionBinding)) as JsonObject;
    }, context);
    const selected = await reserve('selection', 1, { kind: 'review-science', source: object(binding.source), analysis: object(binding.analysis), evidenceRef: nativeRef });
    metrics.push(...selected.metrics as unknown as ReviewMetric[]); scientific.selection = selected.native!;
  } catch (error) {
    if (context.abortSignal?.aborted) throw error;
    scientific = { ...scientific, status: 'INCOMPLETE', reason: error instanceof Error ? error.message : 'portfolio_review_science_failed' };
  }
  return runIndependentReview(api, context, {
    target: { kind: 'PORTFOLIO', id: input.id, version, sourceEvidenceRefs: refs, evaluationStartNs: start }, policy: input.review.policy, model: input.review.model, thinkingLevel: input.review.thinkingLevel,
    checks, metrics, createdAt: input.review.createdAt,
    evidence: { context: { targetId: input.id, version, capital: input.capital, currency: input.currency, weights: input.weights ?? null, sources: contexts, evidenceRefs: refs }, provenance: { status: checks.find(check => check.id === 'provenance')?.status ?? 'UNKNOWN', checks: json({ checks }).checks! }, factor: { status: 'COMPLETE', sources: contexts }, selection: scientific, stress: scientific, portfolio: scientific },
  });
}
