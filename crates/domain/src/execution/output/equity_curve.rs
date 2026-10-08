//! Select recorded native equity or verified separate report-currency valuations.
//! Display sampling never calculates returns, prices or fees.
use super::{bad, simulation};
use crate::{research::invalid, DomainError};
use chrono::Datelike;
use contracts::{
    equity_curve::{EquityCurveQuery, EquityPointV1, EquityResolution, EquitySeriesV1},
    science::{NativeSimulationRequestV1, NativeSimulationResultV1},
    DbCounter, DecimalValue,
};

pub fn equity_curve_query(query: &EquityCurveQuery) -> Result<(), DomainError> {
    if matches!((query.start_ns, query.end_ns), (Some(start), Some(end)) if start > end) {
        return Err(invalid("equity_curve.range", "EQUITY_CURVE_RANGE_INVALID"));
    }
    Ok(())
}

fn count(value: u64) -> Result<DbCounter, DomainError> {
    DbCounter::new(value).map_err(|_| bad("equity_curve.timestamp"))
}

fn bucket(time: u64, resolution: EquityResolution) -> i64 {
    const DAY: u64 = 86_400_000_000_000;
    match resolution {
        EquityResolution::Day => (time / DAY) as i64,
        // The Unix epoch was Thursday; ISO weeks begin Monday.
        EquityResolution::Week => ((time / DAY + 3) / 7) as i64,
        EquityResolution::Month => {
            let date = chrono::DateTime::from_timestamp_nanos(time as i64);
            i64::from(date.year()) * 12 + i64::from(date.month0())
        }
        EquityResolution::Native | EquityResolution::Auto => time as i64,
    }
}

/// Keep first/last observations and the actual final observation in each bucket.
/// No total point ceiling; only the explicitly selected time bucket removes points.
fn indices(points: &[EquityPointV1], resolution: EquityResolution) -> Vec<usize> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut selected = vec![0];
    for index in 1..points.len() {
        if bucket(points[index - 1].timestamp_ns.get(), resolution)
            != bucket(points[index].timestamp_ns.get(), resolution)
            && selected.last() != Some(&(index - 1))
        {
            selected.push(index - 1);
        }
    }
    if selected.last() != Some(&(points.len() - 1)) {
        selected.push(points.len() - 1);
    }
    selected
}

/// Preserve both ends of every recorded unavailable interval. Dropping the
/// only null in a bucket would reconnect a chart across an unknown valuation.
pub(super) fn gap_boundaries(points: &[EquityPointV1]) -> Vec<usize> {
    points
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            (point.value.is_none()
                && (index == 0
                    || index + 1 == points.len()
                    || points[index - 1].value.is_some()
                    || points[index + 1].value.is_some()))
            .then_some(index)
        })
        .collect()
}

fn report_indices(points: &[EquityPointV1], resolution: EquityResolution) -> Vec<usize> {
    let mut selected: std::collections::BTreeSet<_> =
        indices(points, resolution).into_iter().collect();
    selected.extend(gap_boundaries(points));
    selected.into_iter().collect()
}

/// Automatic preview target, preserving real endpoints and unavailable intervals.
/// Required gap boundaries can exceed the target; a display preference must never
/// reject valid scientific evidence or reconnect an unknown valuation interval.
pub(super) fn report_preview_indices(points: &[EquityPointV1], target: usize) -> Vec<usize> {
    if points.len() <= target {
        return (0..points.len()).collect();
    }
    let mut required: std::collections::BTreeSet<_> = gap_boundaries(points).into_iter().collect();
    required.extend([0, points.len() - 1]);
    let available: Vec<_> = (0..points.len())
        .filter(|index| !required.contains(index))
        .collect();
    let remaining = target.saturating_sub(required.len()).min(available.len());
    for index in 0..remaining {
        let selected = if remaining == 1 {
            available.len() / 2
        } else {
            index * (available.len() - 1) / (remaining - 1)
        };
        required.insert(available[selected]);
    }
    required.into_iter().collect()
}

pub fn portfolio_equity_curve(
    request: &NativeSimulationRequestV1,
    result: &NativeSimulationResultV1,
    query: &EquityCurveQuery,
) -> Result<EquitySeriesV1, DomainError> {
    equity_curve_query(query)?;
    simulation::binding(request, result)?;
    let canonical = &result.canonical_result;
    let start = count(simulation::native_count(
        &canonical["run"]["backtest_start_ns"],
    )?)?;
    let end = count(simulation::native_count(
        &canonical["run"]["backtest_end_ns"],
    )?)?;
    let (mut points, source_point_count) = if let Some(report) = &result.spot_cash_report {
        use contracts::{
            spot_cash::ReportCurrencyValuationOutcomeV1,
            spot_cash_report::NativeSpotCashObservationKindV1,
        };
        let mut points = std::collections::BTreeMap::new();
        let mut source_count = 0;
        // One display point per clock, selecting the last actually observed state.
        // Statistical daily boundaries are separately recorded, never derived here.
        for observation in &report.observations {
            if let NativeSpotCashObservationKindV1::Snapshot { valuation, .. } = &observation.record
            {
                source_count += 1;
                let (value, reason_code) = match &valuation.outcome {
                    ReportCurrencyValuationOutcomeV1::Complete { total, .. } => {
                        (Some(total.clone()), None)
                    }
                    ReportCurrencyValuationOutcomeV1::Unavailable { reason } => (
                        None,
                        Some(format!(
                            "REPORT_CURRENCY_VALUATION_{}",
                            serde_json::to_value(reason)
                                .map_err(|_| bad("equity_curve.reason"))?
                                .as_str()
                                .ok_or_else(|| bad("equity_curve.reason"))?
                        )),
                    ),
                };
                points.insert(
                    observation.native_clock_ns,
                    EquityPointV1 {
                        timestamp_ns: observation.native_clock_ns,
                        value,
                        reason_code,
                    },
                );
            }
        }
        (points.into_values().collect::<Vec<_>>(), source_count)
    } else {
        let source = canonical["portfolio_snapshots"]
            .as_array()
            .ok_or_else(|| bad("equity_curve.snapshots"))?;
        let mut points = Vec::with_capacity(source.len());
        for snapshot in source {
            let timestamp_ns = count(simulation::native_count(&snapshot["ts_event"])?)?;
            let value: DecimalValue = simulation::money(
                &snapshot["total_equity"][0],
                &request.settings.base_currency,
            )?
            .to_plain_string()
            .parse()
            .map_err(|_| bad("equity_curve.money_range"))?;
            points.push(EquityPointV1 {
                timestamp_ns,
                value: Some(value),
                reason_code: None,
            });
        }
        points.sort_unstable_by_key(|point| point.timestamp_ns);
        for pair in points.windows(2) {
            if pair[0].timestamp_ns == pair[1].timestamp_ns && pair[0].value != pair[1].value {
                return Err(bad("equity_curve.conflicting_timestamp"));
            }
        }
        points.dedup_by(|a, b| a.timestamp_ns == b.timestamp_ns);
        (points, source.len() as u64)
    };
    points.retain(|point| {
        query
            .start_ns
            .is_none_or(|start| point.timestamp_ns >= start)
            && query.end_ns.is_none_or(|end| point.timestamp_ns <= end)
    });
    let window_point_count = count(points.len() as u64)?;
    let resolution = if query.resolution == EquityResolution::Auto {
        EquityResolution::Native
    } else {
        query.resolution
    };
    let selected = if result.spot_cash_report.is_some() {
        report_indices(&points, resolution)
    } else {
        indices(&points, resolution)
    };
    // Preserve the existing chart contract: sampled describes only downsampling
    // within the distinct-clock query window, not duplicate-clock selection.
    let sampled = selected.len() < points.len();
    let points = selected
        .into_iter()
        .map(|index| points[index].clone())
        .collect();
    Ok(EquitySeriesV1 {
        native_version: result.native_version.clone(),
        base_currency: request.settings.base_currency.clone(),
        starting_capital: request.settings.starting_capital.clone(),
        period_start_ns: start,
        period_end_ns: end,
        source_point_count: count(source_point_count)?,
        window_point_count,
        resolution,
        sampled,
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(time: u64) -> EquityPointV1 {
        EquityPointV1 {
            timestamp_ns: count(time).unwrap(),
            value: Some("1".parse().unwrap()),
            reason_code: None,
        }
    }

    #[test]
    fn buckets_preserve_real_endpoints_and_iso_week_year_boundaries() {
        let ns = |text: &str| {
            chrono::DateTime::parse_from_rfc3339(text)
                .unwrap()
                .timestamp_nanos_opt()
                .unwrap() as u64
        };
        let points = vec![
            point(ns("2025-12-28T01:00:00Z")),
            point(ns("2025-12-28T23:00:00Z")),
            point(ns("2025-12-29T01:00:00Z")),
            point(ns("2025-12-31T23:59:59Z")),
            point(ns("2026-01-01T01:00:00Z")),
        ];
        assert_eq!(indices(&points, EquityResolution::Day), vec![0, 1, 2, 3, 4]);
        assert_eq!(indices(&points, EquityResolution::Week), vec![0, 1, 4]);
        assert_eq!(indices(&points, EquityResolution::Month), vec![0, 3, 4]);
        assert_eq!(indices(&points[..1], EquityResolution::Month), vec![0]);
        assert!(indices(&[], EquityResolution::Native).is_empty());
    }

    #[test]
    fn million_native_observations_are_all_retained_and_bucketing_is_explicit() {
        let points: Vec<_> = (0..1_000_000).map(|n| point(n * 1_000_000_000)).collect();
        assert_eq!(
            indices(&points, EquityResolution::Native),
            (0..points.len()).collect::<Vec<_>>()
        );
        let selected = indices(&points, EquityResolution::Day);
        assert!(selected.len() < 20);
        assert_eq!(selected.first(), Some(&0));
        assert_eq!(selected.last(), Some(&999_999));
        assert_eq!(
            indices(&points[..10_000], EquityResolution::Native).len(),
            10_000
        );
        assert_eq!(
            indices(&points[..10_001], EquityResolution::Native).len(),
            10_001
        );
    }

    #[test]
    fn inclusive_query_rejects_only_inverted_bounds() {
        let mut query = EquityCurveQuery {
            start_ns: Some(count(2).unwrap()),
            end_ns: Some(count(1).unwrap()),
            resolution: EquityResolution::Auto,
        };
        assert!(equity_curve_query(&query).is_err());
        query.end_ns = query.start_ns;
        assert!(equity_curve_query(&query).is_ok());
        assert!(
            serde_json::from_str::<EquityCurveQuery>(r#"{"resolution":"AUTO","unknown":1}"#)
                .is_err()
        );
    }
}

#[cfg(test)]
mod report_gap_tests {
    use super::*;
    fn observation(time: u64, value: Option<&str>) -> EquityPointV1 {
        EquityPointV1 {
            timestamp_ns: count(time).unwrap(),
            value: value.map(|v| v.parse().unwrap()),
            reason_code: value
                .is_none()
                .then(|| "REPORT_CURRENCY_VALUATION_STALE_PRICE".into()),
        }
    }
    #[test]
    fn day_downsampling_preserves_a_gap_before_same_day_recovery() {
        const DAY: u64 = 86_400_000_000_000;
        let points = vec![
            observation(DAY, Some("100")),
            observation(DAY + 5, None),
            observation(DAY + 6, Some("110")),
            observation(2 * DAY, Some("120")),
        ];
        assert_eq!(
            indices(&points, EquityResolution::Day),
            vec![0, 2, 3],
            "legacy selection is unchanged"
        );
        let selected = report_indices(&points, EquityResolution::Day);
        assert_eq!(selected, vec![0, 1, 2, 3]);
        assert!(points[selected[1]].value.is_none());
    }
    #[test]
    fn bounded_preview_retains_the_null_old_uniform_sampling_would_erase() {
        let points: Vec<_> = (0..65)
            .map(|index| observation(index, if index == 63 { None } else { Some("100") }))
            .collect();
        let selected = report_preview_indices(&points, 64);
        assert_eq!(selected.len(), 64);
        assert_eq!(selected.first(), Some(&0));
        assert_eq!(selected.last(), Some(&64));
        assert!(selected.contains(&63));
        assert!(points[63].value.is_none());
    }
    #[test]
    fn preview_target_never_rejects_or_discards_required_gap_boundaries() {
        let points: Vec<_> = (0..130)
            .map(|index| observation(index, if index % 2 == 1 { None } else { Some("100") }))
            .collect();
        let selected = report_preview_indices(&points, 64);
        let expected: Vec<_> = std::iter::once(0)
            .chain((1..points.len()).step_by(2))
            .collect();
        assert_eq!(selected, expected);
        assert!(selected.len() > 64);
    }
    #[test]
    fn full_curves_preserve_all_gaps_beyond_the_old_point_ceiling() {
        let points: Vec<_> = (0..25_000)
            .map(|index| observation(index, if index % 2 == 1 { None } else { Some("100") }))
            .collect();
        let selected = report_indices(&points, EquityResolution::Day);
        let expected: Vec<_> = std::iter::once(0)
            .chain((1..points.len()).step_by(2))
            .collect();
        assert_eq!(selected, expected);
        assert!(selected.len() > 10_000);
        assert_eq!(selected.last(), Some(&(points.len() - 1)));
    }
}
