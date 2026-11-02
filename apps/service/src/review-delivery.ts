import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { ReviewSourceEvidence } from '../../research/src/review-source.ts';
import type { CapturedReviewSource } from '../../research/src/review-source.ts';
import type { DatasetConfig, ServiceConfig } from './config.ts';
import { existingDataset } from './config.ts';
import { assertCurrentReviewSource } from './review-source.ts';
import type { PortfolioFact } from './facts.ts';
import { json, object, Problem } from './facts.ts';

export interface ReviewDeliverySource { configured: DatasetConfig; captured: CapturedReviewSource }
/** Frozen paths identify original sources; current bytes/grants are rechecked before every new POST. */
export async function assertReviewDeliverySources(bindings: readonly ReviewDeliverySource[]): Promise<void> {
  for (const binding of bindings) {
    try { await assertCurrentReviewSource(binding.configured, binding.captured); }
    catch { throw new Problem('review_source_delivery_authorization_unavailable', 409); }
  }
}
export async function portfolioDeliverySources(host: ResearchHost, config: ServiceConfig, fact: PortfolioFact): Promise<ReviewDeliverySource[]> {
  if (!fact.view.review) return []; // Original pre-review portfolios retain original source contracts.
  if (fact.view.review.decision !== 'PASS') throw new Problem('portfolio_review_not_passed', 422);
  const bindings: ReviewDeliverySource[] = [];
  const add = (captured: CapturedReviewSource) => {
    const configured = existingDataset(config, captured.datasetId);
    if (!configured?.ready) throw new Problem('review_source_current_configuration_missing', 409);
    const { id, name, assetClass, currency, source, start, end, ready, data, revisionManifest, reviewSourceEvidence } = configured;
    const safe = json({ id, name, assetClass, currency, source, start, end, ready, data, revisionManifest, reviewSourceEvidence }) as unknown as DatasetConfig;
    if (!bindings.some(binding => binding.captured.inputDataDigest === captured.inputDataDigest && binding.captured.licenseDigest === captured.licenseDigest)) bindings.push({ configured: safe, captured: JSON.parse(JSON.stringify(captured)) as CapturedReviewSource });
  };
  for (const id of fact.view.researchIds) {
    const evidence = await host.harness.snapshot(ReviewSourceEvidence, id as ConversationId, BACKGROUND_CONTEXT);
    if (!evidence?.captured) throw new Problem('review_source_current_authorization_missing', 409);
    add(evidence.captured as unknown as CapturedReviewSource);
  }
  if (fact.view.taskId !== null) {
    const original = await host.harness.commit(tx => tx.task(fact.view.taskId as TaskId), BACKGROUND_CONTEXT);
    const input = original?.input && typeof original.input === 'object' && !Array.isArray(original.input) ? object(original.input) : null;
    const review = input?.review && typeof input.review === 'object' && !Array.isArray(input.review) ? object(input.review) : null;
    if (!review?.independentSource) throw new Problem('review_source_independent_authorization_missing', 409);
    add(review.independentSource as unknown as CapturedReviewSource);
  }
  await assertReviewDeliverySources(bindings);
  return bindings;
}
