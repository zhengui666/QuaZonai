import type { CriterionView, FactorView, PortfolioProgress } from './index.ts';

/** Owner list projections. Large evidence, code and execution identities stay in detail reads. */
export interface ListPage<T> {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
  facets: Record<string, { value: string; label: string }[]>;
}
export type ListCriterion = Pick<CriterionView, 'status' | 'metric' | 'value' | 'target' | 'partition'>;
export interface ResearchListItem {
  id: number; title: string; status: string; trials: number; model: string;
  datasetId: string | null; datasetName: string | null;
  scope: 'single-factor' | 'ensemble' | null;
  modality: 'BAR' | 'TRADE_TICK' | null;
  criterion: ListCriterion | null;
}
export interface FactorListItem {
  id: string; name: string; researchId: number; researchName: string; trialId: number; memberIndex: number;
  datasetId: string | null; datasetName: string | null;
  status: string; evidence: FactorView['evidence']; combinable: boolean;
  nativeModality: 'BAR' | 'TRADE_TICK' | null;
  reviewDecision: string | null; independentIc: number | null;
}
export interface ListPerformance {
  sharpe: number | null; annualizedReturn: number | null; maxDrawdown: number | null;
  annualizationDays: number | null;
}
export interface PortfolioListItem {
  id: string; name: string; status: string; datasetId: string; datasetName: string | null;
  memberCount: number; memberType: 'single-factor' | 'ensemble';
  capital: string; currency: string; createdAt: string;
  reviewDecision: string | null; progress: PortfolioProgress | null;
  performance: ListPerformance;
}
export interface TickReplayListItem {
  id: string; name: string; status: string; datasetId: string; datasetName: string | null;
  memberCount: number; memberType: 'single-factor' | 'ensemble';
  scientificQualification: 'UNQUALIFIED_EXPLORATORY';
  createdAt: string | null; progress: PortfolioProgress | null;
}
export interface AllocationListItem {
  id: string; name: string; status: string; memberCount: number;
  reviewDecision: string | null; createdAt: string;
}
