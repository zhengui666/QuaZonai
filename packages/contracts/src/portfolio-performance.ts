/** Read-only summaries of the original shared-account backtest, expressed as ratios. */
export interface PortfolioPerformanceSummary {
  sharpe: number | null;
  annualizedReturn: number | null;
  /** Original signed upstream statistic; no magnitude conversion. */
  maxDrawdown: number | null;
  annualizationDays: number | null;
  basis: {
    sharpe: 'NATIVE_ACCOUNT_DAILY_RETURNS' | 'NATIVE_SPOT_CASH_COMPLETE_DAY_RETURNS';
    maxDrawdown: 'NATIVE_ACCOUNT_DAILY_RETURNS' | 'NATIVE_SPOT_CASH_COMPLETE_DAY_RETURNS';
    annualizedReturn: 'NATIVE_CAGR' | 'NOT_PROVIDED';
    expectedDays: number | null;
    observedDays: number | null;
    partialDays: number | null;
    startNs: string | null;
    endNs: string | null;
  };
}
