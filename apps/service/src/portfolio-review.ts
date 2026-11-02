import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { ReviewCheck, ReviewReport } from '../../../packages/contracts/src/review.ts';
import { portfolioReviewVersion } from './review-allocation.ts';
import { runIndependentReview } from '../../research/src/review-runtime.ts';
import { reviewSourceChecks, validationReviewContext } from '../../research/src/review-workflow.ts';
import type { ResearchInput } from '../../research/src/documents.ts';
import type { ResearchRuntime } from '../../research/src/research-runtime.ts';
import type { PortfolioTaskInput } from './native.ts';
import { Application, json, object, portfolio } from './facts.ts';

/** Review only original Validation facts. Consumer replay/held-out results never enter a model session. */
export async function reviewPortfolio(input: PortfolioTaskInput, api: ResearchRuntime, context: Context): Promise<ReviewReport | null> {
  if (!input.review) return null; // Already-admitted legacy tasks retain their original input.
  const sources = Array.isArray(input.evidence) ? input.evidence : [input.evidence];
  const contexts: JsonObject[] = []; const sourceChecks: ReviewCheck[][] = [];
  const refs = sources.map(source => `research:${source.researchId}:trial:${object(source.selected).taskId}:validation`);
  for (const [index, source] of sources.entries()) {
    const original = object(source.input) as ResearchInput; const selected = object(source.selected);
    contexts.push(validationReviewContext(original, selected, [selected]));
    sourceChecks.push(reviewSourceChecks(original, selected, [selected], refs[index]!));
  }
  const original = (await api.snapshot(Application, context))?.portfolios[input.id];
  if (!original) throw new Error('portfolio_not_found');
  const version = portfolioReviewVersion(portfolio(original));
  const checks = input.review.policy.requiredChecks.map(id => {
    const matches = sourceChecks.flat().filter(check => check.id === id);
    return { id, status: matches.some(check => check.status === 'FAIL') ? 'FAIL' as const
      : matches.length === sources.length && matches.every(check => check.status === 'PASS') ? 'PASS' as const : 'UNKNOWN' as const,
      reason: matches.map(check => check.reason).join('; ') || 'portfolio_check_not_available', evidenceRefs: refs };
  });
  // Component independent data is already consumed. It cannot establish a fresh portfolio validation window.
  const independent = checks.find(check => check.id === 'independent_window');
  if (independent) { independent.status = 'UNKNOWN'; independent.reason = 'fresh_portfolio_independent_window_required'; }
  const first = object(object(sources[0]!.selected).result);
  const request = first.request && typeof first.request === 'object' ? object(first.request) : {};
  const start = typeof request.train_end_ns === 'string' ? request.train_end_ns : '0';
  return runIndependentReview(api, context, {
    target: { kind: 'PORTFOLIO', id: input.id, version, sourceEvidenceRefs: refs, evaluationStartNs: start },
    policy: input.review.policy, model: input.review.model, thinkingLevel: input.review.thinkingLevel,
    checks, metrics: [], createdAt: input.review.createdAt,
    evidence: { context: { targetId: input.id, version, capital: input.capital, currency: input.currency,
      weights: input.weights ?? null, sources: contexts, evidenceRefs: refs },
      provenance: { status: 'INCOMPLETE', checks: json({ checks }).checks! },
      factor: { status: 'COMPLETE', sources: contexts },
      selection: { status: 'INCOMPLETE', reason: 'complete_comparable_portfolio_trial_matrix_required' },
      stress: { status: 'INCOMPLETE', reason: 'registered_portfolio_stress_replays_required' },
      portfolio: { status: 'INCOMPLETE', reason: 'fresh_validation_shared_account_study_and_ablation_required' } },
  });
}
