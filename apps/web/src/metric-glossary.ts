import i18n from './i18n';
/** Display-only definitions. Values, units and scientific identities remain server-owned. */
export interface MetricDefinition { label: string; explanation: string }
const definition = (label: string, explanation: string): MetricDefinition => ({ label, explanation });
const translate = (value: MetricDefinition): MetricDefinition => ({ label: i18n.t(value.label, { ns: 'shared' }), explanation: i18n.t(value.explanation, { ns: 'shared' }) });
const glossary: Record<string, MetricDefinition> = {
  'tick.conditional_annualized_return_365': definition("Conditional annualized return (365 days)", "The conditional annualized return reported by the native tick economic replay for this partition and market state. The original fractional value is preserved; the browser does not recalculate it. It establishes neither future profitability nor scientific qualification."),
  pearson_ic: definition("Pearson information coefficient (IC)", "Pearson correlation between factor scores and their future labels, ranging from \u22121 to 1 with no units. This is not a return. Constant scores, constant labels or too few samples make it unavailable."),
  rank_ic: definition("Signed rank information coefficient (RankIC)", "Rank correlation between factor scores and future labels, ranging from \u22121 to 1. Its sign is preserved rather than taking the absolute value. The original analysis specifies whether the scope is cross-sectional or pooled time series; these are not interchangeable."),
  rank_ic_lower: definition("RankIC interval lower bound", "The lower confidence bound for RankIC under the original analysis method. Block resampling preserves simultaneous event groups and overlapping labels. It is unavailable with insufficient samples and is not a probability of future profit."),
  rank_ic_upper: definition("RankIC interval upper bound", "The upper confidence bound for RankIC under the original analysis method. Interpret it alongside the lower bound, analysis scope, block length and sample count."),
  coverage: definition("Valid factor observation coverage", "The fraction of original observations in the evaluation partition with valid factor observations. This measures record coverage, not historical availability or sample independence."),
  independent_blocks: definition("Complete time blocks", "The number of complete simultaneous-event blocks at the analysis block length. The length covers at least the observed label overlap; the count alone does not establish independence between blocks."),
  bootstrap_blocks: definition("Resampling time blocks", "The number of complete simultaneous-event blocks at the resampling block length. The length covers at least the observed label overlap. This is not a count of independent samples and does not establish independence between blocks."),
  positive_slice_fraction: definition("Positive time-slice fraction", "The fraction of declared time slices whose directional statistic is above zero. This is neither the fraction of profitable trades nor a probability of future profit."),
  quantile_spread: definition("Factor quantile label-return spread", "The difference in future price-label returns between the highest and lowest factor quantiles, retaining the original label horizon. Without fills and execution costs, it is not a tradable portfolio's net return."),
  quantile_monotonicity: definition("Factor quantile monotonicity", "Rank correlation between quantile order and each quantile's mean future label return. It describes whether label returns vary consistently with the factor direction, not causality or a probability of profit."),
  dsr: definition("Deflated Sharpe ratio (DSR)", "Uses the number of searches, candidate Sharpe distribution, return skewness and kurtosis to assess whether the observed Sharpe exceeds a selection-bias benchmark. It depends on complete trial information and statistical assumptions; it is not a probability of future profit."),
  pbo: definition("Backtest overfitting diagnostic (PBO)", "Uses CSCV on comparable candidate-return matrices to measure how often the in-sample winner underperforms out of sample. It diagnoses the search process, not the future failure probability of an individual factor, and does not replace forward-in-time validation."),
  net_return_delta: definition("Incremental net return", "Mean difference in after-cost returns between the candidate portfolio and the frozen baseline at matched times. Currency, window, fees and shared-account definitions must agree; it is not derived by summing member equity curves."),
  net_return_delta_lower: definition("Incremental net-return interval lower bound", "The lower block-resampling bound for the mean paired net-return difference between candidate and baseline. The original return period is preserved without reannualization."),
  net_return_delta_upper: definition("Incremental net-return interval upper bound", "The upper block-resampling bound for the mean paired net-return difference between candidate and baseline. Interpret it alongside the lower bound and native sample count."),
  return_correlation: definition("Return correlation with baseline", "Pearson correlation between aligned candidate and baseline net returns. Low correlation alone establishes neither incremental returns nor tradability."),
  drawdown_reduction: definition("Drawdown improvement over baseline", "Baseline maximum drawdown magnitude minus the candidate's maximum drawdown magnitude. Positive values indicate a smaller candidate drawdown. The native fractional value is preserved."),
  sharpe_delta: definition("Sharpe improvement over baseline", "The difference in candidate and baseline Sharpe ratios for same-frequency, nonannualized net returns. It is not a forecast and cannot be compared directly with differently annualized statistics."),
  'account.net_return_lower': definition("After-cost return interval lower bound", "The lower confidence bound for the native shared-account net-return statistic. See the original fields for period and method; the browser does not infer fees or reannualize it."),
  'execution.stressed_net_return': definition("Stressed net return", "Native net return under a fixed cost, latency or liquidity stress scenario. The scenario scope and execution mapping require supporting evidence; factor labels alone are not a substitute."),
  'mechanism.evidence_fraction': definition("Mechanism evidence coverage", "The fraction of mechanism items on the fixed review checklist supported by sources. This measures evidence completeness only and cannot offset data leakage or missing statistical evidence."),
  'counts.observations': definition("Total observations", "The number of original observations received for this partition, including observations later excluded. This is not a count of independent trades."),
  'counts.included': definition("Included observations", "The observations that passed native alignment and completeness checks and were used in this partition's metrics. The count does not imply sample independence."),
  'counts.excluded': definition("Excluded observations", "The observations excluded from this partition's metrics. See warmup, label and partition counts for the reasons. Missing data is not filled with zero."),
  'counts.indicator_warmup': definition("Warmup observations", "The number of observations excluded because the required indicator history was not yet available."),
  'counts.incomplete_labels': definition("Incomplete labels", "The number of observations excluded because the future data needed to construct labels was incomplete."),
  'counts.cross_boundary_labels': definition("Cross-boundary labels", "The number of observations excluded because the label window crossed the original registered partition boundary."),
  'counts.prediction_outside_partition': definition("Predictions outside partition", "The number of observations whose prediction clock falls outside this partition. They are not included in its metrics."),
  'PnL (total)': definition("Total PnL", "Total PnL in the native account report, including any unrealized PnL that report includes. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  'PnL% (total)': definition("Total PnL percentage", "Native total account PnL as a percentage of the initial balance. The original value is preserved and is not an annualized return."),
  'Max Winner': definition("Largest win", "The largest realized winning amount. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  'Avg Winner': definition("Average win", "The mean realized winning amount, excluding losses. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  'Min Winner': definition("Smallest win", "The smallest realized winning amount. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  'Max Loser': definition("Largest loss", "The largest realized loss, preserving the native negative sign. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  'Avg Loser': definition("Average loss", "The mean realized losing amount, preserving the native negative sign. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  'Min Loser': definition("Smallest loss", "The realized loss closest to zero, preserving the native negative sign. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  Expectancy: definition("Expectancy per trade", "Native average win multiplied by the winning fraction, plus average loss multiplied by the losing fraction. This is not an expected future return. Amounts use the native reporting currency; no currency is inferred if the API omits it. Historical results do not establish future returns."),
  'Win Rate': definition("Win rate", "The native fraction of valid PnL records that are realized wins. It does not measure the size of gains or losses. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Average (Return)': definition("Average return", "The arithmetic mean of native account returns, not a compounded return. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Average Win (Return)': definition("Average positive return", "The arithmetic mean of positive native returns. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Average Loss (Return)': definition("Average negative return", "The arithmetic mean of negative native returns, retaining the negative sign. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Returns Skewness': definition("Return skewness", "The asymmetry of the native return distribution, with no units. It may be unavailable when samples are insufficient or volatility is zero."),
  'Returns Kurtosis': definition("Return kurtosis", "The excess kurtosis of native returns relative to a normal distribution, with no units. It is sensitive to extreme observations and is not a probability of loss."),
  'Profit Factor': definition("Profit factor", "The sum of native positive returns divided by the absolute sum of negative returns, with no units. It may be unavailable when there are no losses and does not establish a factor's causal validity."),
  'Risk Return Ratio': definition("Risk-return ratio", "The native mean daily return divided by its standard deviation, with no units and no annualization. This is not an additionally annualized Sharpe ratio."),
  'Long Ratio': definition("Long-position fraction", "The native fraction of position records opened with a buy, not a capital weight. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Tail Ratio': definition("Tail ratio", "The 95th percentile of native returns divided by the absolute 5th percentile, with no units. It may be unavailable with too few samples or a zero denominator."),
  'Max Drawdown': definition("Native maximum drawdown", "The deepest decline of the native compounded-return curve from a previous peak; negative values indicate drawdown. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them. This remains distinct from the display projection of the equity curve below."),
  'Calendar Expected Days': definition("Expected calendar days", "The number of complete UTC days in the native registered window. Incomplete days at the start and end are counted separately."),
  'Calendar Observed Return Days': definition("Observed calendar return days", "The number of complete UTC days for which the native account actually supplied daily returns. Missing days are not filled in."),
  'Calendar Partial Days': definition("Partial-day windows", "The number of incomplete UTC day segments at the start and end of the registered window. They are excluded from complete-day performance statistics."),
  'Calendar Partial Return Days': definition("Partial-day return records", "Native account return records for incomplete UTC days at the window boundaries. They remain in the original results."),
  'Calendar Coverage Ratio': definition("Calendar return coverage", "Observed return days divided by expected days. This measures record coverage only. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Calendar Geometric Daily Return': definition("Calendar geometric daily return", "The geometric mean of complete native daily returns, not an annualized return. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them. Unavailable when coverage is incomplete."),
  'Calendar Minimum Daily Return': definition("Minimum calendar daily return", "The lowest return in the complete native daily-return series. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Calendar Maximum Daily Return': definition("Maximum calendar daily return", "The highest return in the complete native daily-return series. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."),
  'Calendar Maximum Absolute Deviation': definition("Maximum calendar absolute deviation", "The largest absolute difference between a daily return and the geometric mean daily return. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them. This is not a confidence interval."),
  'Calendar Negative Days': definition("Negative calendar days", "The number of complete calendar days with negative returns. Unavailable if days are missing; missing days are not treated as zero-return days."),
  'Calendar Zero Days': definition("Zero-return calendar days", "The number of complete calendar days with returns exactly equal to zero. Missing days are not included."),
  capital: definition("Starting capital", "The initial capital explicitly supplied to this native shared-account backtest, in reporting currency. Exact amounts are preserved; separate starting capital from individual members is not summed."),
  weight: definition("Weight", "The capital or target-exposure fraction in the original policy. It has no units, and 1 means 100%. Original precision is preserved; factor scores are not automatically weights."),
  equity: definition("Account equity", "Total equity in an authoritative account snapshot or native shared account, in account currency. It may include unrealized PnL and is not withdrawable cash. Missing values remain unknown."),
  cash: definition("Cash", "The cash amount in an authoritative account snapshot, in account currency. Not all of it is necessarily tradable or withdrawable; native reservations and other constraints still apply."),
  endingEquity: definition("Ending equity", "The final native account equity in the full window, in reporting currency. No points are filled in and no rounding is applied. Unavailable when the time or ending value is missing."),
  cumulativeReturn: definition("Cumulative return", "Equity's percentage change from this run's starting capital. This is a display projection of native equity, not annualized or adjusted for external cash flows, and does not replace native return statistics."),
  drawdown: definition("Drawdown", "Equity's percentage change from its previous peak, including starting capital. Negative values indicate decline. Missing equity points or invalid timestamps are not filled in for calculation."),
  snapshotMaxDrawdown: definition("Maximum snapshot-equity decline", "The deepest percentage decline of equity snapshots from their previous peak, including starting capital. Negative values indicate decline. This display projection is not adjusted for external flows and differs from native daily-return drawdown."),
  records: definition("Recorded points", "The number of native account equity records with parseable times. Original clocks within the same millisecond remain separate records. This is not an independent sample count."),
  quantity: definition("Quantity", "The position quantity reported by the authoritative account. Units come from the original instrument definition. Precision is preserved; no contract multiplier is guessed from the instrument name."),
  maximumReductionQuantity: definition("Maximum quantity", "The maximum quantity allowed by the native bounded-reduction plan, in the instrument's defined units. It is not a quantity guaranteed to fill."),
  minimumSellPrice: definition("Minimum sell price", "The lowest sell price allowed by this bounded reduction, in the instrument's price currency. It is neither the current market price nor a fill guarantee."),
  cancellationsAndLegs: definition("Opening-order cancellations / reduction legs", "The number of exposure-increasing orders the native plan intends to cancel and the number of explicit reduction legs. These counts do not imply completed cancellations or reductions."),
  positionValue: definition("Position value", "The authoritative account's valuation of this position, in account currency. Missing valuations are not inferred from quantity or the latest price."),
  pnl: definition("PnL", "The position PnL reported by the authoritative account, in account currency. Original values and signs are preserved; missing PnL is not inferred."),
  reserved: definition("Total account reservation", "The account-wide reservation total reported by the native owner, in account currency. Pausing or cancelling does not release it automatically. Unknown amounts are not treated as zero."),
  exitReserved: definition("Retained reservation", "The amount the native owner retains for this capital plan, in account currency. It is not the account-wide total; pausing or cancelling a plan does not imply its reservation was released."),
  remainingManagedCapital: definition("Remaining tradable capital", "Remaining managed capital reported by the native capital-exit preview under its funding and execution policies, in account currency. It is not a recommendation for one strategy or evidence of a completed withdrawal."),
  cashBuffer: definition("Explicit cash buffer", "Capital the user explicitly keeps in cash rather than allocating to strategies, in account currency. It is deducted separately from account-wide reservations."),
  budget: definition("Equity available after reservations", "Account equity less total account reservations and the explicit cash buffer, in account currency. It is neither filled capital nor a guaranteed withdrawable amount."),
  strategyCapital: definition("Effective strategy capital", "Capital allocated within budget, joint capacity, liquidity, fee and order-increment constraints, in account currency. Suggested targets do not imply actual fills."),
  residualCash: definition("Cash / residual after fees", "Capital left unallocated after this calculation, in account currency. Native values are preserved; this is not necessarily a withdrawable balance."),
  scale: definition("Budget scale", "The factor scaling this run's suggested member capital relative to its original capital basis. It has no units and scales only the selected pool without reselecting factors."),
  requestedAmount: definition("Requested amount", "The amount explicitly requested for reservation and external withdrawal, in account currency. A request confirms neither a native reservation nor a completed transfer."),
  releasedCash: definition("Released cash", "Cash the native owner reports as released from positions, in account currency. Withdrawability evidence and external reconciliation are still required."),
  withdrawable: definition("Verified withdrawable amount", "The amount confirmed by the native owner from withdrawability evidence, in account currency. Simulated evidence does not prove live withdrawability; unknown values are not filled with zero."),
  reconciledWithdrawal: definition("Reconciled external withdrawal", "The cumulative external cash movement reconciled downstream, in account currency. QZ itself does not transfer funds."),
  executionCost: definition("Estimated economic loss", "The native capital-exit plan's estimated economic loss, in account currency. For each fill, reference buy price times quantity minus actual net sale proceeds after fees is floored at zero, then summed. Favorable prices offset only that fill's fees, not prior losses. Actual fees are shown separately and are not added again."),
  unrealizedPnl: definition("Existing unrealized PnL (not withdrawal cost)", "Unrealized PnL on existing positions reported separately by the native plan, in account currency. It is not an execution cost caused by this withdrawal."),
  economicCapacity: definition("Economic capacity", "Profitable capacity requires matching out-of-sample net returns at the capital scale, market-calibrated impact, spreads and fees, execution timing and exit evidence. Its unit is reporting currency. A historical BAR participation limit is not a substitute; missing inputs leave it unavailable."),
  decisionNs: definition("Decision clock", "The Unix-nanosecond clock of the native constraining decision. A readable UTC time and the full nanosecond integer are both retained; the full counter is not converted to a JavaScript floating-point number."),
  barEventNs: definition("BAR event clock", "The event time of the original BAR used by the constraint. UTC and the full Unix-nanosecond value are retained. This is not the time the data became available to the decision."),
  barAvailableNs: definition("BAR availability clock", "The time the original BAR became available for decisions. It must not be later than the decision clock; availability is not invented from retrieval times or future data."),
  maximumCapital: definition("Historical executable ceiling", "The tightest capital ceiling imposed by historical BARs under the original execution mapping, volume participation, entry/rebalancing and independent full-exit assumptions, in reporting currency. It is not an optimal profitable allocation, future depth or a return promise."),
  minimumCapital: definition("Minimum practical capital", "The minimum usable capital under native minimum-order, quantity and monetary precision rules, in reporting currency. Smaller amounts may not produce valid orders."),
  participation: definition("Volume participation", "The explicit maximum fraction of historical BAR notional volume allowed for the strategy. It has no units; 0.01 means 1%. It is not a measured fill probability."),
  maximumAgeSeconds: definition("Maximum liquidity-data age", "The maximum allowed interval between the decision clock and the available BAR used, in seconds. Missing or stale observations are not replaced with later data."),
  scenarioCapital: definition("Capital scenario", "An exact trial capital amount used for native capacity calculation, in reporting currency. It is not invested capital, and scenarios are not a probability distribution."),
  requiredTurnover: definition("Peak BAR demand", "Peak notional demand for a scenario on one instrument and BAR, in reporting currency. It is the greater of cumulative entry/rebalancing demand and independent full-exit demand, not one order or full-period turnover."),
  maximumParticipation: definition("Maximum participation", "The ratio of required notional demand to observed volume for the scenario's binding instrument and BAR. It has no units; 0.01 means 1%. It is not a market-impact estimate."),
  usableCapital: definition("Usable capital", "Capital this scenario can allocate under historical capacity and native minimum-order constraints, in reporting currency. It is neither economic capacity nor capital already deployed downstream."),
  unallocatedCash: definition("Unallocated cash", "Capital left after subtracting allocatable capital from the scenario, in reporting currency. Native decimal values are preserved."),
  volumeMultiplier: definition("Volume multiplier", "An explicit multiplier applied to observed BAR volume, with no units. A value of 1 is the reference scenario and 0.5 assumes half the volume. It is not a forecast or confidence level."),
  entryRebalanceTurnover: definition("Entry / rebalancing demand", "Notional demand from cumulative absolute net target changes charged to the same instrument and BAR budget, in reporting currency. Reusing a BAR does not replenish its liquidity."),
  exitTurnover: definition("Full-exit demand", "Peak notional demand in an independent full-exit scenario, in reporting currency. It is an alternative liquidation constraint, not assumed to occur simultaneously with strategy rebalancing."),
  entryRebalanceParticipation: definition("Entry / rebalancing participation", "Entry and rebalancing notional demand divided by the corresponding BAR's observed notional volume. It has no units; 0.01 means 1%."),
  exitParticipation: definition("Exit participation", "Independent full-exit notional demand divided by the corresponding BAR's observed notional volume. It has no units and is not a probability of successful exit."),
  decisionCount: definition("Decisions", "The number of original decisions in the capacity request, not the number whose assessment completed."),
  assessedDecisionCount: definition("Assessed decisions", "The number of original decisions actually assessed by the capacity calculation. Failure reports may cover only part of the request."),
  requiredObservations: definition("Required observations", "The total liquidity observations required for instrument/decision pairs with demand. Missing observations are not filled in."),
  coveredObservations: definition("Covered observations", "The number of instrument/decision pairs with qualifying BAR observations. Interpret it alongside the required count."),
  uniqueBars: definition("Distinct BARs", "The number of BARs distinguished by original instrument, event and availability clocks. Reusing a BAR does not reset its liquidity budget."),
  reusedBarObservations: definition("Repeated BAR uses", "The number of later observations that reuse an existing BAR. Their demand accumulates against the same budget."),
  observedNotional: definition("Observed notional volume", "Historical BAR notional volume used by the constraint, in reporting currency. It does not represent order-book depth at the time or future executable volume."),
  requiredWeight: definition("Constraint demand weight", "The cumulative absolute net target-change or exit weight required by this BAR constraint. It has no units and is used in the native capital-ceiling calculation, not as a factor score."),
  inputTokens: definition("Input tokens", "Input-token count reported by the official model provider. Missing usage is not zero and may include provider-defined cached usage."),
  outputTokens: definition("Output tokens", "Output-token count reported by the official model provider. Missing usage is not zero."),
  cost: definition("Model cost", "The cost recorded in the official provider usage report. Original amounts are displayed; neither currency nor missing costs are inferred."),
  activeTasks: definition("Active tasks", "The active-task count recorded by official Durable. This differs from the number of studies or running scientific processes."),
  trials: definition("Trials", "Trials recorded in the original research ledger, including failures, rejections and cancellations. This is not a count of successful results."),
  members: definition("Members", "The number of atomic factors or historical research members bound to the portfolio. They share one native account simulation; their separate equity curves are not summed."),
};
const annualized: Record<string, [string, string]> = {
  "Sharpe Ratio": ["Sharpe ratio", "Native mean daily return divided by its standard deviation, multiplied by the square root of the specified annualization period. The native assumption is a zero risk-free rate. It has no units and is unavailable with zero volatility or insufficient samples."],
  "Sortino Ratio": ["Sortino ratio", "Native mean daily return relative to downside volatility, using the specified annualization period. It has no units and is unavailable with zero downside volatility or insufficient samples."],
  "Returns Volatility": ["Return volatility", "The standard deviation of native daily returns, annualized over the specified period. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."],
  "Information Ratio": ["Information ratio", "Native mean active return relative to the benchmark divided by tracking error, annualized over the specified period, with no units. A matching native benchmark is required; the frontend does not reconstruct it."],
  "Tracking Error": ["Tracking error", "The standard deviation of native active returns relative to the benchmark, annualized over the specified period. Values retain the native fractional unit: 0.01 means 1%. The browser does not reannualize them."],
  "Alpha": ["Alpha", "The excess-return parameter from the native regression against the benchmark, reported for the native period. A matching benchmark is required; the frontend does not construct one."],
  "Treynor Ratio": ["Treynor ratio", "The native return-to-systematic-risk beta ratio for the specified period, with no units. Unavailable with zero beta or insufficient benchmark data."],
  "Up Capture Ratio": ["Up capture ratio", "The native strategy-to-benchmark return ratio during rising benchmark periods, reported for the specified period. The native ratio unit is preserved without conversion."],
  "Down Capture Ratio": ["Down capture ratio", "The native strategy-to-benchmark return ratio during falling benchmark periods, reported for the specified period. The native ratio unit is preserved without conversion."],
};
const unknown = definition("Unrecognized metric", "The service returned a metric not yet in this glossary. Its value and original units are preserved, but its meaning is unconfirmed. Check its technical name under Original fields; do not infer returns or units.");
export function metricDefinition(key: string): MetricDefinition {
  const normalized = key.replace(/^(?:TRAIN|VALIDATION|HELDOUT|DISCOVERY|SEALED|FORWARD)\./, '').replace(/^portfolio\./, '');
  if (normalized === 'ic') return translate(glossary.pearson_ic!);
  if (glossary[normalized]) return translate(glossary[normalized]!);
  const reviewAliases: Record<string, string> = { 'data.coverage': 'coverage', 'factor.rank_ic': 'rank_ic', 'factor.rank_ic_lower': 'rank_ic_lower', 'factor.rank_ic_upper': 'rank_ic_upper', 'selection.dsr': 'dsr', 'selection.pbo': 'pbo', 'robustness.positive_fraction': 'positive_slice_fraction', 'incremental.net_return_delta_lower': 'net_return_delta_lower' };
  if (reviewAliases[normalized]) return translate(glossary[reviewAliases[normalized]!]!);
  const period = /^(.*?) \((\d+) days\)$/.exec(normalized);
  if (period && annualized[period[1]!]) {
    const [label, explanation] = annualized[period[1]!]!;
    return definition(i18n.t('{{label}} ({{days}}-day annualization)', { ns: 'shared', label: i18n.t(label, { ns: 'shared' }), days: period[2] }), `${i18n.t(explanation, { ns: 'shared' })}${i18n.t(' The annualization period is the {{days}} days specified by the native field; it is not changed based on the market name.', { ns: 'shared', days: period[2] })}`);
  }
  const extra: Record<string, MetricDefinition> = {
    Beta: definition("Beta", "The regression sensitivity of native strategy returns to benchmark returns, with no units. Unavailable without a matching benchmark."),
    'Ulcer Index': definition("Ulcer index", "The square root of the mean squared drawdown of the native return curve, describing sustained downside. The native numerical convention is retained; it is not a probability of loss."),
  };
  if (extra[normalized]) return translate(extra[normalized]!);
  const threshold = /^Omega Ratio \(threshold ([-\d.]+)\)$/.exec(normalized);
  if (threshold) return definition(i18n.t('Omega ratio (threshold {{threshold}})', { ns: 'shared', threshold: threshold[1] }), i18n.t('The ratio of returns above to returns below the native threshold, with no units. The browser does not change the threshold.', { ns: 'shared' }));
  const risk = /^(Value at Risk|Expected Shortfall) \(confidence ([\d.]+)\)$/.exec(normalized);
  if (risk) return definition(i18n.t('{{label}} (confidence {{confidence}})', { ns: 'shared', label: i18n.t(risk[1] === 'Value at Risk' ? 'Value at risk' : 'Expected shortfall', { ns: 'shared' }), confidence: risk[2] }), i18n.t('Tail-risk statistics from the native historical return distribution at the specified confidence level, preserving native return units. This is neither a capacity confidence interval nor a worst-loss guarantee.', { ns: 'shared' }));
  return translate(unknown);
}

/** Existing facts use Chinese labels; aliases keep their explanations in the same glossary. */
const aliases: Record<string, string> = {
  '总权益': 'equity', '权益': 'equity', '账户权益': 'equity', '现金': 'cash', '本金': 'capital', '权重': 'weight', '目标权重': 'weight', '成员': 'members',
  '上限': 'maximumCapital', '历史可执行上限': 'maximumCapital', '条件容量上限': 'maximumCapital', '最低实用本金': 'minimumCapital', '约束资金上限': 'maximumCapital',
  '所用成交量参与率': 'participation', '成交量参与率': 'participation', '流动性数据最大时差': 'maximumAgeSeconds', '资本情景': 'scenarioCapital',
  '账户总预留': 'reserved', '保留预留': 'exitReserved', '显式现金缓冲': 'cashBuffer', '扣预留后的可用权益': 'budget', '有效策略资金': 'strategyCapital', '交易预算': 'strategyCapital', '本次建议资金': 'strategyCapital', '剩余可交易资金': 'remainingManagedCapital',
  '现金 / 费用后剩余': 'residualCash', '预算缩放': 'scale', '请求金额': 'requestedAmount', '已释放现金': 'releasedCash', '有证据的可提取额': 'withdrawable', '已对账外部提取': 'reconciledWithdrawal', '预计执行成本': 'executionCost', '预计经济损失': 'executionCost', '既有未实现盈亏（非提取成本）': 'unrealizedPnl',
  '取消开启订单 / 减仓腿': 'cancellationsAndLegs', '输入 Token': 'inputTokens', '输出 Token': 'outputTokens', '费用': 'cost', '活动任务': 'activeTasks',
};
// Historical display aliases remain recognizable without changing their metric identity.
const factLabels: Record<string, string> = {
  "总权益": "Total equity",
  "权益": "Equity",
  "账户权益": "Account equity",
  "现金": "Cash",
  "本金": "Capital",
  "权重": "Weight",
  "目标权重": "Target weight",
  "成员": "Members",
  "上限": "Upper limit",
  "历史可执行上限": "Historical executable ceiling",
  "条件容量上限": "Conditional capacity ceiling",
  "最低实用本金": "Minimum practical capital",
  "约束资金上限": "Constrained capital ceiling",
  "所用成交量参与率": "Applied volume participation",
  "成交量参与率": "Volume participation",
  "流动性数据最大时差": "Maximum liquidity-data age",
  "资本情景": "Capital scenario",
  "账户总预留": "Total account reservations",
  "保留预留": "Retained reservation",
  "显式现金缓冲": "Explicit cash buffer",
  "扣预留后的可用权益": "Equity available after reservations",
  "有效策略资金": "Effective strategy capital",
  "交易预算": "Trading budget",
  "本次建议资金": "Capital proposed for this plan",
  "剩余可交易资金": "Remaining tradable capital",
  "现金 / 费用后剩余": "Cash remaining after fees",
  "预算缩放": "Budget scaling",
  "请求金额": "Requested amount",
  "已释放现金": "Released cash",
  "有证据的可提取额": "Verified withdrawable amount",
  "已对账外部提取": "Reconciled external withdrawal",
  "预计执行成本": "Estimated execution cost",
  "预计经济损失": "Estimated economic loss",
  "既有未实现盈亏（非提取成本）": "Existing unrealized PnL (not a withdrawal cost)",
  "取消开启订单 / 减仓腿": "Opening-order cancellations / reduction legs",
  "输入 Token": "Input tokens",
  "输出 Token": "Output tokens",
  "费用": "Model cost",
  "活动任务": "Active tasks"
};
export function factDisplayLabel(label: string): string {
  return factLabels[label] ? i18n.t(factLabels[label], { ns: 'shared' }) : label;
}
export function factMetricKey(label: string): string | undefined {
  if (aliases[label]) return aliases[label];
  const historical = Object.entries(factLabels).find(([, key]) => key === label || i18n.t(key, { ns: 'shared' }) === label)?.[0];
  if (historical) return aliases[historical];
  return Object.entries(glossary).find(([, item]) => item.label === label || i18n.t(item.label, { ns: 'shared' }) === label)?.[0];
}
export function partitionLabel(value: string): string { return i18n.t(({ TRAIN: 'Training', VALIDATION: 'Validation', HELDOUT: 'Held-out', DISCOVERY: 'Discovery', SEALED: 'Sealed', FORWARD: 'Forward' } as Record<string, string>)[value] ?? 'Unrecognized partition', { ns: 'shared' }); }
