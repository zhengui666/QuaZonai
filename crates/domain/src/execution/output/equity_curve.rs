//! Select existing observations only. Pricing, returns and fees stay in Nautilus.
use super::{bad, simulation};
use crate::{research::invalid, DomainError};
use chrono::Datelike;
use contracts::{
    equity_curve::{
        EquityCurveQuery, EquityPointV1, EquityResolution, EquitySeriesV1, MAX_EQUITY_POINTS,
    },
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
/// Returning None signals too many points, never a silently truncated series.
fn indices(points: &[EquityPointV1], resolution: EquityResolution) -> Option<Vec<usize>> {
    if points.is_empty() {
        return Some(Vec::new());
    }
    let mut selected = vec![0];
    for index in 1..points.len() {
        if bucket(points[index - 1].timestamp_ns.get(), resolution)
            != bucket(points[index].timestamp_ns.get(), resolution)
            && selected.last() != Some(&(index - 1))
        {
            selected.push(index - 1);
            if selected.len() > MAX_EQUITY_POINTS {
                return None;
            }
        }
    }
    if selected.last() != Some(&(points.len() - 1)) {
        selected.push(points.len() - 1);
    }
    (selected.len() <= MAX_EQUITY_POINTS).then_some(selected)
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
    points.retain(|point| {
        query
            .start_ns
            .is_none_or(|start| point.timestamp_ns >= start)
            && query.end_ns.is_none_or(|end| point.timestamp_ns <= end)
    });
    let window_point_count = count(points.len() as u64)?;
    let choices: &[EquityResolution] = if query.resolution == EquityResolution::Auto {
        &[
            EquityResolution::Native,
            EquityResolution::Day,
            EquityResolution::Week,
            EquityResolution::Month,
        ]
    } else {
        std::slice::from_ref(&query.resolution)
    };
    let (resolution, selected) = choices
        .iter()
        .find_map(|&resolution| indices(&points, resolution).map(|selected| (resolution, selected)))
        .ok_or_else(|| invalid("equity_curve.resolution", "EQUITY_CURVE_TOO_MANY_POINTS"))?;
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
        source_point_count: count(source.len() as u64)?,
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
        assert_eq!(
            indices(&points, EquityResolution::Day).unwrap(),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(
            indices(&points, EquityResolution::Week).unwrap(),
            vec![0, 1, 4]
        );
        assert_eq!(
            indices(&points, EquityResolution::Month).unwrap(),
            vec![0, 3, 4]
        );
        assert_eq!(
            indices(&points[..1], EquityResolution::Month).unwrap(),
            vec![0]
        );
        assert!(indices(&[], EquityResolution::Native).unwrap().is_empty());
    }

    #[test]
    fn million_observations_are_bounded_without_losing_the_end() {
        let points: Vec<_> = (0..1_000_000).map(|n| point(n * 1_000_000_000)).collect();
        assert!(indices(&points, EquityResolution::Native).is_none());
        let selected = indices(&points, EquityResolution::Day).unwrap();
        assert!(selected.len() < 20);
        assert_eq!(selected.first(), Some(&0));
        assert_eq!(selected.last(), Some(&999_999));
        assert!(indices(&points[..10_000], EquityResolution::Native).is_some());
        assert!(indices(&points[..10_001], EquityResolution::Native).is_none());
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
