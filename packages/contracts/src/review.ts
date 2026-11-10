/** Immutable, evidence-bound quantitative review. Scores are policy outputs, never return forecasts. */
export type ReviewTargetKind = 'FACTOR' | 'PORTFOLIO' | 'ALLOCATION';
export type ReviewDecision = 'PASS' | 'REJECT' | 'INCONCLUSIVE';
export type ReviewDimensionId = 'DATA' | 'STATISTICS' | 'ROBUSTNESS' | 'EXECUTION' | 'INCREMENTAL' | 'MECHANISM';
export interface ReviewTarget {
  kind: ReviewTargetKind; id: string; version: string; sourceEvidenceRefs: string[]; evaluationStartNs: string;
}
export interface ReviewMetric {
  key: string; value: number | null; status: 'AVAILABLE' | 'UNAVAILABLE' | 'UNSUPPORTED' | 'INSUFFICIENT' | 'INVALID';
  observationCount: number; method: string; scope: string; unit: string; frequency: string; evidenceRef: string; reason?: string;
}
export interface ReviewMetricRule {
  key: string; dimension: ReviewDimensionId; direction: 'HIGHER' | 'LOWER'; low: number; high: number;
  minimumObservations: number; required: boolean; methods: string[]; scope: string; unit: string; frequency: string;
  /** Absolute admission requirement; reference percentiles may never alter this gate. */
  minimumTarget?: { value: number; inclusive: boolean };
  normalization?: 'FIXED_BOUNDS_V1' | 'REFERENCE_P10_P90_V1';
}
export interface ReviewCheck {
  id: string; status: 'PASS' | 'FAIL' | 'UNKNOWN'; reason: string; evidenceRefs: string[];
}
export interface ReviewPolicy {
  schemaVersion: 1; id: string; targetKind: ReviewTargetKind; frozenAtNs: string;
  /** Derived only from a frozen reference cohort, never the current candidate or sealed results. */
  calibration: { rootCount: number; endNs: string; evidenceRefs: string[] } | null;
  requiredChecks: string[]; rejectChecks: string[]; metricRules: ReviewMetricRule[];
  dimensionWeights: Record<ReviewDimensionId, number>;
  passScore: number; observeScore: number; dimensionFloor: number;
}
export interface ReviewDimension {
  id: ReviewDimensionId; label: string; score: number | null; weight: number;
  status: 'COMPLETE' | 'INCOMPLETE'; reasons: string[];
}
export interface ReviewAnalysis {
  kind: string; status: 'COMPLETE' | 'INCOMPLETE' | 'FAILED'; reason: string | null;
  evidenceRef: string | null; metrics: string[];
}
export interface ReviewWeights {
  level: 'FACTOR' | 'ASSET' | 'PORTFOLIO'; status: 'PROPOSED' | 'UNAVAILABLE';
  members: { id: string; weight: string }[]; sourceReportIds: string[];
  /** A weight change is a new candidate, never approval of an existing version. */
  candidateId: string | null; requiresStudy: true; reason: string | null;
}
export interface ReviewReport {
  schemaVersion: 1; id: string; target: ReviewTarget; policyId: string; policy: ReviewPolicy;
  decision: ReviewDecision; totalScore: number | null; dimensions: ReviewDimension[];
  qualityCoefficient: number; reasonCodes: string[]; hardFailures: string[]; missingEvidence: string[];
  metrics: ReviewMetric[]; checks: ReviewCheck[]; analyses: ReviewAnalysis[];
  weights: ReviewWeights[]; narrative: string | null; createdAt: string;
}
export interface ReviewScoringInput {
  id: string; target: ReviewTarget; policy: ReviewPolicy; metrics: ReviewMetric[]; checks: ReviewCheck[];
  analyses?: ReviewAnalysis[]; narrative?: string | null; createdAt: string;
}
export interface ReviewAllocationMember {
  id: string; report: ReviewReport; observations: number[];
}
export interface ReviewAllocationPolicy {
  id: string; frozenAtNs: string; kind: 'FACTOR' | 'PORTFOLIO';
  observationTimesNs: string[]; observationEvidenceRefs: string[];
  memberCaps: { id: string; maximumWeight: string }[];
  clusterCaps: { id: string; members: string[]; maximumWeight: string }[];
  currentWeights: { id: string; weight: string }[]; maximumTurnover: string;
}
export interface ReviewAllocationRequest {
  schema_version: 1; kind: 'FACTOR' | 'PORTFOLIO';
  members: { id: string; source_report_id: string; score: number; observations: number[] }[];
  member_caps: { id: string; maximum_weight: string }[];
  cluster_caps: { id: string; members: string[]; maximum_weight: string }[];
  current_weights: { id: string; weight: string }[]; maximum_turnover: string;
}
export interface ReviewAllocationResult {
  schema_version: 1; method: string; kind: 'FACTOR' | 'PORTFOLIO'; solver_version: string;
  members: { id: string; source_report_id: string; quality_coefficient: number; prior: number; weight: string }[];
  covariance: number[][]; lambda: number; objective: number; primal_residual: number; dual_residual: number;
  requires_new_candidate: true; requires_shared_account_study: true;
}
export interface ReviewReferenceObservation {
  rootId: string; endNs: string; evidenceRefs: string[]; metrics: ReviewMetric[];
}
export interface ReviewAllocationStart {
  requestId: string; name: string; portfolioIds: string[];
  memberCaps: ReviewAllocationPolicy['memberCaps']; clusterCaps: ReviewAllocationPolicy['clusterCaps'];
  currentWeights: ReviewAllocationPolicy['currentWeights']; maximumTurnover: string;
  reviewPolicy?: ReviewPolicy;
  /** Explicit untouched dataset for a once-only test of the frozen configuration. */
  independentDatasetId?: string;
}
export interface ReviewAllocationView {
  id: string; requestId: string; name: string; status: 'QUEUED' | 'RUNNING' | 'COMPUTED' | 'FAILED' | 'STOPPED';
  portfolioIds: string[]; taskId: number | null; consumedScientificCalls: number;
  weights: ReviewWeights | null; review: ReviewReport | null; error: string | null; createdAt: string;
}
