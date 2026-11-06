//! Deterministic diagnostics. Inputs are evidence, never forecasts of profitability.
use anyhow::{ensure, Result};
use contracts::{factors::*, review_science::*, SchemaV1};
use std::collections::{BTreeMap, BTreeSet};

const METHOD: &str = "qz.review-science.v1";
const BOOTSTRAPS: usize = 512;

pub fn validate_source(
    request: &NativeReviewSourceRequestV1,
) -> Result<NativeReviewSourceResultV1> {
    let metadata: contracts::catalogs::RuntimeCatalogMetadataV1 =
        serde_json::from_value(request.metadata.clone())?;
    domain::catalogs::metadata(&metadata, request.observed_at)?;
    domain::execution::native_quality(&metadata.quality)?;
    Ok(NativeReviewSourceResultV1 {
        schema_version: SchemaV1,
        method_version: "qz.review-source.v1".into(),
        validated: true,
        metadata: request.metadata.clone(),
    })
}

fn metric(code: &str, value: Option<f64>, reason: &str) -> NativeReviewMetricV1 {
    NativeReviewMetricV1 {
        code: code.into(),
        value: value.filter(|v| v.is_finite()),
        status: if value.is_some_and(f64::is_finite) {
            NativeReviewMetricStatusV1::Available
        } else {
            NativeReviewMetricStatusV1::InsufficientEvidence
        },
        reason: (!value.is_some_and(f64::is_finite)).then(|| reason.into()),
    }
}
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}
fn correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() < 3 || x.len() != y.len() {
        return None;
    }
    let mx = mean(x);
    let my = mean(y);
    let mut xx = 0.0;
    let mut yy = 0.0;
    let mut xy = 0.0;
    for (x, y) in x.iter().zip(y) {
        xx += (x - mx).powi(2);
        yy += (y - my).powi(2);
        xy += (x - mx) * (y - my);
    }
    (xx > 0.0 && yy > 0.0)
        .then(|| xy / xx.sqrt() / yy.sqrt())
        .filter(|v| v.is_finite())
}
/// Reuse ranking storage across synchronized groups and bootstrap replicates.
/// Read score/label columns directly from pairs instead of copying both columns.
#[derive(Default)]
struct RankWorkspace {
    order: Vec<usize>,
    x: Vec<f64>,
    y: Vec<f64>,
}

fn ranks_by(
    len: usize,
    value: impl Fn(usize) -> f64,
    order: &mut Vec<usize>,
    ranks: &mut Vec<f64>,
) {
    order.clear();
    order.extend(0..len);
    // Every equal-value block receives one rank at the original indices, so
    // tie order is irrelevant. Unstable sorting avoids a per-call sort buffer.
    order.sort_unstable_by(|a, b| value(*a).total_cmp(&value(*b)));
    ranks.resize(len, 0.0);
    let mut index = 0;
    while index < order.len() {
        let mut end = index + 1;
        while end < order.len() && value(order[index]) == value(order[end]) {
            end += 1;
        }
        let rank = (index + end - 1) as f64 / 2.0;
        for selected in &order[index..end] {
            ranks[*selected] = rank;
        }
        index = end;
    }
}

impl RankWorkspace {
    fn statistic(&mut self, pairs: &[(f64, f64)]) -> Option<f64> {
        if pairs.len() < 3 {
            return None;
        }
        ranks_by(pairs.len(), |i| pairs[i].0, &mut self.order, &mut self.x);
        ranks_by(pairs.len(), |i| pairs[i].1, &mut self.order, &mut self.y);
        correlation(&self.x, &self.y)
    }

    fn quantile_diagnostics(&mut self, pairs: &[(f64, f64)]) -> Option<(f64, f64)> {
        if pairs.len() < 10 || pairs.iter().all(|pair| pair.0 == pairs[0].0) {
            return None;
        }
        ranks_by(pairs.len(), |i| pairs[i].0, &mut self.order, &mut self.x);
        let mut bins = [0.0; 5];
        let mut counts = [0_usize; 5];
        for (rank, pair) in self.x.iter().zip(pairs) {
            let index = ((rank * 5.0 / pairs.len() as f64).floor() as usize).min(4);
            bins[index] += pair.1;
            counts[index] += 1;
        }
        if counts.iter().any(|n| *n < 2) {
            return None;
        }
        for i in 0..5 {
            bins[i] /= counts[i] as f64;
        }
        Some((
            bins[4] - bins[0],
            bins.windows(2).filter(|p| p[1] > p[0]).count() as f64 / 4.0,
        ))
    }
}

fn mean_present(values: &[Option<f64>]) -> Option<f64> {
    let count = values.iter().flatten().count();
    (count != 0).then(|| values.iter().flatten().sum::<f64>() / count as f64)
}
fn next_random(state: &mut u64) -> usize {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as usize
}
fn quantile(values: &mut [f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[((values.len() - 1) as f64 * q).floor() as usize])
}

/// Complete synchronized event groups, preserving cross-asset dependence in bootstrap.
pub fn factor_diagnostics(
    points: &[NativeFactorPointV1],
    partition: NativeFactorPartitionV1,
    requested_block: usize,
    scope: NativeReviewFactorScopeV1,
) -> Result<NativeFactorDiagnosticsV1> {
    let selected = points.iter().filter(|p| p.partition == partition);
    let selected_count = selected.clone().count();
    let included = selected.filter(|p| p.exclusion_reason.is_none());
    let included_count = included.clone().count();
    let mut output = NativeFactorDiagnosticsV1 {
        method_version: METHOD.into(),
        scope,
        observations: included_count.try_into()?,
        metrics: vec![metric(
            "coverage",
            (selected_count != 0).then(|| included_count as f64 / selected_count as f64),
            "EMPTY_PARTITION",
        )],
        limitations: vec![
            "Factor labels are forward price returns, not executable strategy PnL.".into(),
        ],
    };
    let mut groups: BTreeMap<u64, Vec<(f64, f64)>> = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for point in included.clone() {
        if !keys.insert((&point.observation.instrument_id, point.observation.event_ns)) {
            let mut unavailable = metric("rank_ic", None, "REVIEW_DUPLICATE_FACTOR_POINT");
            unavailable.status = NativeReviewMetricStatusV1::InvalidInput;
            output.metrics.push(unavailable);
            return Ok(output);
        }
        let (Some(score), Some(label)) =
            (point.observation.forecast, point.observation.label_return)
        else {
            anyhow::bail!("REVIEW_INCLUDED_FACTOR_VALUE_MISSING");
        };
        ensure!(
            score.is_finite() && label.is_finite(),
            "REVIEW_FACTOR_NONFINITE"
        );
        groups
            .entry(point.observation.event_ns.get())
            .or_default()
            .push((score, label));
    }
    // Duplicate detection is complete; the tree is not needed during ranking/bootstrap.
    drop(keys);
    let times: Vec<_> = groups.keys().copied().collect();
    let groups: Vec<_> = groups.into_values().collect();
    let maximum_overlap = included
        .filter_map(|p| {
            p.label_event_ns.map(|end| {
                let left = times.partition_point(|t| *t < p.observation.event_ns.get());
                let right = times.partition_point(|t| *t <= end.get());
                right.saturating_sub(left)
            })
        })
        .max()
        .unwrap_or(1);
    drop(times);
    let block = requested_block.max(maximum_overlap).max(1);
    let mut workspace = RankWorkspace::default();
    let mut pairs = Vec::new();
    // Keep one entry per event group so missing cross-sectional statistics retain
    // their original slice positions. Pooling does not compute these statistics.
    let cross: Vec<_> = if scope == NativeReviewFactorScopeV1::CrossSectional {
        groups.iter().map(|g| workspace.statistic(g)).collect()
    } else {
        Vec::new()
    };
    let (observed, quantiles) = match scope {
        NativeReviewFactorScopeV1::PooledTimeSeries => {
            pairs.extend(groups.iter().flatten().copied());
            (
                workspace.statistic(&pairs),
                workspace.quantile_diagnostics(&pairs),
            )
        }
        NativeReviewFactorScopeV1::CrossSectional => {
            let values: Vec<_> = groups
                .iter()
                .filter_map(|g| workspace.quantile_diagnostics(g))
                .collect();
            (
                mean_present(&cross),
                (!values.is_empty()).then(|| {
                    (
                        values.iter().map(|v| v.0).sum::<f64>() / values.len() as f64,
                        values.iter().map(|v| v.1).sum::<f64>() / values.len() as f64,
                    )
                }),
            )
        }
    };
    output.metrics.push(metric(
        "rank_ic",
        observed,
        "CONSTANT_OR_INSUFFICIENT_FACTOR_PAIRS",
    ));
    output.metrics.push(metric(
        "quantile_spread",
        quantiles.map(|q| q.0),
        "QUANTILE_GROUPS_INSUFFICIENT",
    ));
    output.metrics.push(metric(
        "quantile_monotonicity",
        quantiles.map(|q| q.1),
        "QUANTILE_GROUPS_INSUFFICIENT",
    ));
    output.metrics.push(metric(
        "bootstrap_blocks",
        Some((groups.len() / block) as f64),
        "",
    ));
    let can_bootstrap = groups.len() / block >= 30
        && (scope != NativeReviewFactorScopeV1::CrossSectional
            || cross.iter().all(Option::is_some));
    let mut samples = Vec::new();
    let mut seed = 0x6a09e667f3bcc909_u64;
    if can_bootstrap {
        samples.reserve(BOOTSTRAPS);
        let mut indices = Vec::with_capacity(groups.len());
        for _ in 0..BOOTSTRAPS {
            indices.clear();
            while indices.len() < groups.len() {
                let start = next_random(&mut seed) % (groups.len() - block + 1);
                for index in start..start + block {
                    if indices.len() < groups.len() {
                        indices.push(index);
                    }
                }
            }
            let estimate = match scope {
                NativeReviewFactorScopeV1::PooledTimeSeries => {
                    pairs.clear();
                    pairs.extend(indices.iter().flat_map(|i| groups[*i].iter().copied()));
                    workspace.statistic(&pairs)
                }
                NativeReviewFactorScopeV1::CrossSectional => Some(
                    indices.iter().map(|i| cross[*i].unwrap()).sum::<f64>() / indices.len() as f64,
                ),
            };
            if let Some(v) = estimate {
                samples.push(v);
            }
        }
    }
    let valid = samples.len() == BOOTSTRAPS;
    output.metrics.push(metric(
        "rank_ic_lower",
        valid.then(|| quantile(&mut samples, 0.025)).flatten(),
        "BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS",
    ));
    output.metrics.push(metric(
        "rank_ic_upper",
        valid.then(|| quantile(&mut samples, 0.975)).flatten(),
        "BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS",
    ));
    let mut slice_results = Vec::new();
    for slice in 0..4 {
        let range = groups.len() * slice / 4..groups.len() * (slice + 1) / 4;
        let value = match scope {
            NativeReviewFactorScopeV1::PooledTimeSeries => {
                pairs.clear();
                pairs.extend(groups[range].iter().flatten().copied());
                workspace.statistic(&pairs)
            }
            NativeReviewFactorScopeV1::CrossSectional => mean_present(&cross[range]),
        };
        if let Some(value) = value {
            slice_results.push(value);
        }
    }
    output.metrics.push(metric(
        "positive_slice_fraction",
        (slice_results.len() == 4)
            .then(|| slice_results.iter().filter(|v| **v > 0.0).count() as f64 / 4.0),
        "INCOMPLETE_TIME_SLICES",
    ));
    if scope == NativeReviewFactorScopeV1::PooledTimeSeries {
        output.limitations.push("Pooled time-series RankIC is not cross-sectional IC; instrument scale differences may affect pooling.".into());
    }
    output.limitations.push(format!("Moving-block bootstrap: 512 fixed-seed replicates, 95% percentile interval, block={block} synchronized event groups, at least actual label overlap; not a future-profit probability."));
    Ok(output)
}

fn validate_returns(
    series: &[&NativeReviewReturnSeriesV1],
    timestamps: &[contracts::DbCounter],
    frequency: &str,
    block: u32,
) -> Result<()> {
    ensure!(
        !frequency.trim().is_empty() && block > 0,
        "REVIEW_RETURN_POLICY_INVALID"
    );
    ensure!(
        timestamps.windows(2).all(|p| p[0] < p[1]),
        "REVIEW_RETURN_TIME_INVALID"
    );
    let mut ids = BTreeSet::new();
    for row in series {
        ensure!(
            !row.id.trim().is_empty()
                && ids.insert(&row.id)
                && row.returns.len() == timestamps.len()
                && row.returns.iter().all(|v| v.is_finite() && *v >= -1.0),
            "REVIEW_RETURN_SERIES_INVALID"
        );
    }
    Ok(())
}
fn sharpe(values: &[f64]) -> Option<f64> {
    if values.len() < 3 {
        return None;
    }
    let m = mean(values);
    let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    (variance > 0.0)
        .then(|| m / variance.sqrt())
        .filter(|v| v.is_finite())
}
// Standard normal CDF, Abramowitz-Stegun 26.2.17; absolute approximation error < 7.5e-8.
fn normal_cdf(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.2316419 * z);
    let p = 0.3989422804014327
        * (-z * z / 2.0).exp()
        * t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    if x >= 0.0 {
        1.0 - p
    } else {
        p
    }
}
fn inverse_normal(p: f64) -> f64 {
    let mut lo = -12.0;
    let mut hi = 12.0;
    for _ in 0..80 {
        let mid = (lo + hi) / 2.0;
        if normal_cdf(mid) < p {
            lo = mid
        } else {
            hi = mid
        }
    }
    (lo + hi) / 2.0
}
fn interval_mean(values: &[f64], block: usize) -> Option<(f64, f64)> {
    if values.len() / block < 30 {
        return None;
    }
    let mut samples = Vec::with_capacity(BOOTSTRAPS);
    let mut seed = 0xbb67ae8584caa73b_u64;
    for _ in 0..BOOTSTRAPS {
        let mut total = 0.0;
        let mut n = 0;
        while n < values.len() {
            let start = next_random(&mut seed) % (values.len() - block + 1);
            for v in &values[start..start + block] {
                if n < values.len() {
                    total += v;
                    n += 1;
                }
            }
        }
        samples.push(total / values.len() as f64);
    }
    Some((
        quantile(&mut samples, 0.025)?,
        quantile(&mut samples, 0.975)?,
    ))
}
fn max_drawdown(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut nav = 1.0_f64;
    let mut peak = 1.0_f64;
    let mut loss = 0.0_f64;
    for r in values {
        nav *= 1.0 + r;
        if !nav.is_finite() {
            return None;
        }
        peak = peak.max(nav);
        loss = loss.max(1.0 - nav / peak);
    }
    Some(loss)
}

pub fn analyze(request: &NativeReviewScienceRequestV1) -> Result<NativeReviewScienceResultV1> {
    ensure!(
        request
            .source_digest
            .strip_prefix("sha256:")
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())),
        "REVIEW_SOURCE_DIGEST_INVALID"
    );
    let mut out = NativeReviewScienceResultV1 {
        schema_version: SchemaV1,
        source_digest: request.source_digest.clone(),
        method_version: METHOD.into(),
        scope: String::new(),
        observations: 0,
        metrics: Vec::new(),
        limitations: Vec::new(),
    };
    match &request.analysis {
        NativeReviewAnalysisV1::Factor {
            evidence,
            partition,
            scope,
            block_length,
        } => {
            ensure!(*block_length > 0, "REVIEW_BLOCK_LENGTH_INVALID");
            domain::factors::evaluation_request(&evidence.request)?;
            let partition = match partition {
                NativeReviewPartitionV1::Validation => NativeFactorPartitionV1::Validation,
                NativeReviewPartitionV1::Heldout => NativeFactorPartitionV1::Heldout,
            };
            let d =
                factor_diagnostics(&evidence.points, partition, *block_length as usize, *scope)?;
            out.scope = match scope {
                NativeReviewFactorScopeV1::PooledTimeSeries => "POOLED_TIME_SERIES",
                NativeReviewFactorScopeV1::CrossSectional => "CROSS_SECTIONAL",
            }
            .into();
            out.observations = d.observations;
            out.metrics = d.metrics;
            out.limitations = d.limitations;
        }
        NativeReviewAnalysisV1::Selection {
            total_trials,
            ledger_complete,
            selected_trial_id,
            trials,
            timestamps_ns,
            frequency,
            block_length,
        } => {
            out.scope = "SELECTION_COHORT".into();
            validate_returns(
                &trials.iter().collect::<Vec<_>>(),
                timestamps_ns,
                frequency,
                *block_length,
            )?;
            ensure!(
                *total_trials as usize >= trials.len(),
                "REVIEW_TRIAL_DENOMINATOR_INVALID"
            );
            out.observations = timestamps_ns.len().try_into()?;
            let selected = trials
                .iter()
                .position(|s| s.id == *selected_trial_id)
                .ok_or_else(|| anyhow::anyhow!("REVIEW_SELECTED_TRIAL_MISSING"))?;
            let reason = if !ledger_complete {
                "INCOMPLETE_TRIAL_LEDGER"
            } else {
                "DSR_REQUIRES_COMPARABLE_SHARPE_DISTRIBUTION"
            };
            let sharpes: Vec<_> = trials.iter().filter_map(|s| sharpe(&s.returns)).collect();
            let dsr = if *ledger_complete
                && *block_length == 1
                && *total_trials >= 1
                && *total_trials as usize == trials.len()
                && sharpes.len() == trials.len()
                && !trials.is_empty()
                && timestamps_ns.len() / (*block_length as usize) >= 30
            {
                let s = &trials[selected].returns;
                let sr = sharpes[selected];
                let m = mean(s);
                let sd = (s.iter().map(|v| (v - m).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
                let skew = s.iter().map(|v| ((v - m) / sd).powi(3)).sum::<f64>() / s.len() as f64;
                let kurt = s.iter().map(|v| ((v - m) / sd).powi(4)).sum::<f64>() / s.len() as f64;
                let variance = if sharpes.len() == 1 {
                    0.0
                } else {
                    sharpes
                        .iter()
                        .map(|v| (v - mean(&sharpes)).powi(2))
                        .sum::<f64>()
                        / (sharpes.len() - 1) as f64
                };
                let n = *total_trials as f64;
                let gamma = 0.5772156649015329;
                let benchmark = if *total_trials == 1 {
                    0.0
                } else {
                    variance.sqrt()
                        * ((1.0 - gamma) * inverse_normal(1.0 - 1.0 / n)
                            + gamma * inverse_normal(1.0 - 1.0 / (n * std::f64::consts::E)))
                };
                let denominator = 1.0 - skew * sr + (kurt - 1.0) / 4.0 * sr * sr;
                (denominator > 0.0).then(|| {
                    normal_cdf((sr - benchmark) * ((s.len() - 1) as f64 / denominator).sqrt())
                })
            } else {
                None
            };
            out.metrics.push(metric("dsr", dsr, reason));
            if *block_length > 1 && *ledger_complete {
                let metric = out.metrics.last_mut().expect("DSR metric");
                metric.status = NativeReviewMetricStatusV1::Unsupported;
                metric.reason = Some("DSR_SERIAL_DEPENDENCE_UNSUPPORTED".into());
            }
            let pbo = if *ledger_complete
                && *total_trials as usize == trials.len()
                && trials.len() >= 2
                && timestamps_ns.len() >= 32
                && timestamps_ns.len() / 8 >= *block_length as usize
            {
                pbo(trials)
            } else {
                None
            };
            out.metrics.push(metric(
                "pbo",
                pbo,
                if !ledger_complete {
                    "INCOMPLETE_TRIAL_LEDGER"
                } else {
                    "PBO_REQUIRES_FULL_COMPARABLE_RETURN_MATRIX"
                },
            ));
            out.limitations.push("DSR uses all recorded attempts as conservative independent-trial count, per-observation unannualized Sharpe and empirical skew/kurtosis. It assumes independent return observations; declared dependence (block_length > 1) is unsupported rather than inflating sample size. Missing/failed return rows are never zero-filled.".into());
            if *total_trials == 1 {
                out.limitations.push("Explicit complete single-trial ledger: zero selection-inflation benchmark, the probabilistic Sharpe special case. Missing ledgers never imply a single trial.".into());
            }
            out.limitations.push("PBO uses 8 contiguous blocks and all 70 symmetric 4/4 splits; it diagnoses the supplied search cohort, not an individual factor or future profitability. CSCV does not replace chronological held-out evaluation.".into());
        }
        NativeReviewAnalysisV1::Portfolio {
            candidate,
            baseline,
            timestamps_ns,
            frequency,
            block_length,
        } => {
            validate_returns(
                &[candidate, baseline],
                // Both are one original shared-account scope, never factor labels.
                timestamps_ns,
                frequency,
                *block_length,
            )?;
            out.scope = "SHARED_ACCOUNT".into();
            out.observations = timestamps_ns.len().try_into()?;
            let delta: Vec<_> = candidate
                .returns
                .iter()
                .zip(&baseline.returns)
                .map(|(a, b)| a - b)
                .collect();
            let bounds = interval_mean(&delta, *block_length as usize);
            let account_bounds = interval_mean(&candidate.returns, *block_length as usize);
            let positive_slices = if candidate.returns.len() / (*block_length as usize) >= 30 {
                Some(
                    (0..4)
                        .filter(|slice| {
                            mean(
                                &candidate.returns[candidate.returns.len() * slice / 4
                                    ..candidate.returns.len() * (slice + 1) / 4],
                            ) > 0.0
                        })
                        .count() as f64
                        / 4.0,
                )
            } else {
                None
            };
            let daily_complete = frequency == "DAY"
                && timestamps_ns.len() >= 2
                && timestamps_ns
                    .windows(2)
                    .all(|pair| pair[1].get() - pair[0].get() == 86_400_000_000_000);
            out.metrics = vec![
                metric(
                    "candidate_net_return_mean",
                    (!candidate.returns.is_empty()).then(|| mean(&candidate.returns)),
                    "EMPTY_RETURNS",
                ),
                metric(
                    "candidate_net_return_lower",
                    account_bounds.map(|v| v.0),
                    "BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS",
                ),
                metric(
                    "positive_slice_fraction",
                    positive_slices,
                    "BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS",
                ),
                metric(
                    "return_coverage",
                    daily_complete.then_some(1.0),
                    "COMPLETE_NATIVE_DAILY_GRID_REQUIRED",
                ),
                metric(
                    "net_return_delta",
                    (!delta.is_empty()).then(|| mean(&delta)),
                    "EMPTY_RETURNS",
                ),
                metric(
                    "net_return_delta_lower",
                    bounds.map(|x| x.0),
                    "BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS",
                ),
                metric(
                    "net_return_delta_upper",
                    bounds.map(|x| x.1),
                    "BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS",
                ),
                metric(
                    "return_correlation",
                    correlation(&candidate.returns, &baseline.returns),
                    "CONSTANT_OR_INSUFFICIENT_RETURNS",
                ),
                metric(
                    "drawdown_reduction",
                    max_drawdown(&baseline.returns)
                        .zip(max_drawdown(&candidate.returns))
                        .map(|(b, a)| b - a),
                    "NONFINITE_EQUITY",
                ),
                metric(
                    "sharpe_delta",
                    sharpe(&candidate.returns)
                        .zip(sharpe(&baseline.returns))
                        .map(|(a, b)| a - b),
                    "CONSTANT_OR_INSUFFICIENT_RETURNS",
                ),
            ];
            out.limitations.push("Paired differences require original shared-account net returns with matching currency, frequency, interval, costs and initial state, verified by the caller. No curves are summed and no missing returns are imputed.".into());
        }
    }
    Ok(out)
}
fn pbo(trials: &[NativeReviewReturnSeriesV1]) -> Option<f64> {
    let n = trials[0].returns.len();
    let mut failures = 0;
    let mut splits = 0;
    for mask in 0_u16..256 {
        if mask.count_ones() != 4 {
            continue;
        }
        let mut train = vec![Vec::new(); trials.len()];
        let mut test = vec![Vec::new(); trials.len()];
        for block in 0..8 {
            for (row, series) in trials.iter().enumerate() {
                let target = if mask & (1 << block) != 0 {
                    &mut train[row]
                } else {
                    &mut test[row]
                };
                target.extend_from_slice(&series.returns[n * block / 8..n * (block + 1) / 8]);
            }
        }
        let train: Vec<_> = train.iter().map(|v| sharpe(v)).collect::<Option<_>>()?;
        let test: Vec<_> = test.iter().map(|v| sharpe(v)).collect::<Option<_>>()?;
        let best = train.iter().copied().max_by(f64::total_cmp)?;
        let winners: Vec<_> = train
            .iter()
            .enumerate()
            .filter(|(_, v)| **v == best)
            .map(|(i, _)| i)
            .collect();
        if winners.len() != 1 {
            return None;
        }
        let winner = winners[0];
        let below = test.iter().filter(|v| **v < test[winner]).count() as f64;
        let equal = test.iter().filter(|v| **v == test[winner]).count() as f64;
        let rank = (below + (equal + 1.0) / 2.0) / (test.len() + 1) as f64;
        if rank <= 0.5 {
            failures += 1;
        }
        splits += 1;
    }
    (splits == 70).then(|| failures as f64 / splits as f64)
}
