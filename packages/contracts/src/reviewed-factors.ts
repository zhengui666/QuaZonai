import type { ReviewReport } from './review.ts';

/** Service-authorized immutable source. Research callers supply only factor IDs. */
export interface ReviewedFactorSource {
  factorId: string;
  factorVersionId: string;
  sourceResearchId: number;
  sourceTrialId: number;
  modelSource: string;
  modelSha256: string;
  report: ReviewReport;
  sourceEndNs: string;
}
export interface ReviewedFactorSources {
  schemaVersion: 1;
  fitEndNs: string;
  members: ReviewedFactorSource[];
}
