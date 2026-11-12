//! Completeness metadata over the original Nautilus daily account returns.
//! No custom performance metrics, new returns, forward filling or qualification thresholds.
use contracts::science::{NativeStatisticGroup, NativeStatisticV1};

const DAY_NS: u64 = 86_400_000_000_000;
fn metric(key: &str, value: Option<f64>) -> NativeStatisticV1 {
    let value = value.filter(|number| number.is_finite());
    NativeStatisticV1 {
        group: NativeStatisticGroup::Returns,
        native_key: key.into(),
        currency: None,
        value,
        reason_code: value
            .is_none()
            .then(|| "NATIVE_STATISTIC_UNAVAILABLE".into()),
    }
}

pub(crate) fn statistics(
    start_ns: u64,
    end_ns: u64,
    fully_priced: bool,
    returns: impl IntoIterator<Item = (u64, f64)>,
) -> Vec<NativeStatisticV1> {
    let first_day = start_ns.div_ceil(DAY_NS);
    let last_day = end_ns / DAY_NS;
    let expected = (start_ns < end_ns && first_day < last_day).then(|| last_day - first_day);
    let partial_days = (start_ns < end_ns)
        .then(|| end_ns.div_ceil(DAY_NS) - start_ns / DAY_NS - expected.unwrap_or(0));
    let mut values: Vec<_> = returns.into_iter().collect();
    let valid = values.iter().all(|(time, value)| {
        time.is_multiple_of(DAY_NS)
            && *time / DAY_NS >= start_ns / DAY_NS
            && *time < end_ns
            && value.is_finite()
            && *value >= -1.0
    }) && values.windows(2).all(|pair| pair[0].0 < pair[1].0);
    let observed = values.len();
    // Keep original native returns intact; only complete UTC days enter these summaries.
    values.retain(|(time, _)| first_day <= *time / DAY_NS && *time / DAY_NS < last_day);
    let complete = fully_priced
        && valid
        && expected.is_some_and(|days| {
            values.len() as u64 == days
                && values
                    .iter()
                    .enumerate()
                    .all(|(index, (time, _))| *time == (first_day + index as u64) * DAY_NS)
        });
    let mut rows = vec![
        metric("Calendar Expected Days", expected.map(|days| days as f64)),
        metric("Calendar Observed Return Days", Some(values.len() as f64)),
        metric(
            "Calendar Coverage Ratio",
            expected.map(|days| values.len() as f64 / days as f64),
        ),
        metric(
            "Calendar Partial Days",
            partial_days.map(|days| days as f64),
        ),
        metric(
            "Calendar Partial Return Days",
            valid.then_some((observed - values.len()) as f64),
        ),
    ];
    // Integrity metadata, not a QuaZonai return/dispersion indicator. A missing
    // or unpriced original native day must never silently become zero return.
    rows.push(metric(
        "Calendar Complete Priced Coverage",
        Some(if complete { 1.0 } else { 0.0 }),
    ));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(values: &[f64]) -> Vec<NativeStatisticV1> {
        statistics(
            DAY_NS,
            (values.len() as u64 + 1) * DAY_NS,
            true,
            values
                .iter()
                .enumerate()
                .map(|(i, value)| ((i as u64 + 1) * DAY_NS, *value)),
        )
    }
    fn value(rows: &[NativeStatisticV1], key: &str) -> Option<f64> {
        rows.iter().find(|row| row.native_key == key).unwrap().value
    }
    fn unavailable(rows: &[NativeStatisticV1]) {
        assert_eq!(value(rows, "Calendar Complete Priced Coverage"), Some(0.0));
    }

    #[test]
    fn reports_coverage_only_not_custom_performance() {
        let rows = measured(&[0.019, -1.0, 0.021]);
        assert_eq!(value(&rows, "Calendar Complete Priced Coverage"), Some(1.0));
        assert_eq!(rows.len(), 6);
        assert!(rows
            .iter()
            .all(|row| !row.native_key.contains("Return") || row.native_key.contains("Days")));
    }

    #[test]
    fn missing_day_is_not_imputed_as_zero_or_dropped_from_the_denominator() {
        let rows = statistics(
            DAY_NS,
            4 * DAY_NS,
            true,
            [(DAY_NS, 0.02), (3 * DAY_NS, 0.02)],
        );
        assert_eq!(value(&rows, "Calendar Expected Days"), Some(3.0));
        assert_eq!(value(&rows, "Calendar Observed Return Days"), Some(2.0));
        assert_eq!(value(&rows, "Calendar Coverage Ratio"), Some(2.0 / 3.0));
        unavailable(&rows);
        let empty = statistics(DAY_NS, 2 * DAY_NS, true, []);
        assert_eq!(value(&empty, "Calendar Coverage Ratio"), Some(0.0));
        unavailable(&empty);
    }

    #[test]
    fn unpriced_equity_cannot_qualify_through_native_forward_filled_returns() {
        let rows = statistics(
            DAY_NS,
            3 * DAY_NS,
            false,
            [(DAY_NS, 0.02), (2 * DAY_NS, 0.02)],
        );
        assert_eq!(value(&rows, "Calendar Coverage Ratio"), Some(1.0));
        unavailable(&rows);
    }

    #[test]
    fn intraday_window_does_not_become_a_calendar_day() {
        for end in [DAY_NS + 2, 2 * DAY_NS] {
            let rows = statistics(DAY_NS + 1, end, true, [(DAY_NS, 0.02)]);
            assert_eq!(value(&rows, "Calendar Expected Days"), None);
            assert_eq!(value(&rows, "Calendar Observed Return Days"), Some(0.0));
            assert_eq!(value(&rows, "Calendar Partial Days"), Some(1.0));
            assert_eq!(value(&rows, "Calendar Partial Return Days"), Some(1.0));
            unavailable(&rows);
        }
    }

    #[test]
    fn closing_bar_plus_one_nanosecond_preserves_every_complete_utc_day() {
        let rows = statistics(
            DAY_NS,
            4 * DAY_NS + 1,
            true,
            [(DAY_NS, 0.019), (2 * DAY_NS, 0.02), (3 * DAY_NS, 0.021)],
        );
        assert_eq!(value(&rows, "Calendar Expected Days"), Some(3.0));
        assert_eq!(value(&rows, "Calendar Observed Return Days"), Some(3.0));
        assert_eq!(value(&rows, "Calendar Coverage Ratio"), Some(1.0));
        assert_eq!(value(&rows, "Calendar Partial Days"), Some(1.0));
        assert_eq!(value(&rows, "Calendar Partial Return Days"), Some(0.0));
    }

    #[test]
    fn partial_endpoint_returns_do_not_change_complete_day_performance() {
        let rows = statistics(
            DAY_NS + 1,
            5 * DAY_NS + 1,
            true,
            [
                (DAY_NS, -0.8),
                (2 * DAY_NS, 0.02),
                (3 * DAY_NS, 0.0),
                (4 * DAY_NS, -0.01),
                (5 * DAY_NS, 0.9),
            ],
        );
        assert_eq!(value(&rows, "Calendar Expected Days"), Some(3.0));
        assert_eq!(value(&rows, "Calendar Observed Return Days"), Some(3.0));
        assert_eq!(value(&rows, "Calendar Coverage Ratio"), Some(1.0));
        assert_eq!(value(&rows, "Calendar Partial Days"), Some(2.0));
        assert_eq!(value(&rows, "Calendar Partial Return Days"), Some(2.0));
    }

    #[test]
    fn partial_days_cannot_fill_a_missing_complete_day() {
        let rows = statistics(
            DAY_NS + 1,
            5 * DAY_NS + 1,
            true,
            [
                (DAY_NS, 0.2),
                (2 * DAY_NS, 0.02),
                (4 * DAY_NS, 0.02),
                (5 * DAY_NS, 0.2),
            ],
        );
        assert_eq!(value(&rows, "Calendar Expected Days"), Some(3.0));
        assert_eq!(value(&rows, "Calendar Observed Return Days"), Some(2.0));
        assert_eq!(value(&rows, "Calendar Coverage Ratio"), Some(2.0 / 3.0));
        unavailable(&rows);
    }

    #[test]
    fn invalid_extra_rows_cannot_hide_in_partial_day_counts() {
        for extra in [
            (0, 0.0),
            (5 * DAY_NS, 0.0),
            (3 * DAY_NS + 1, 0.0),
            (4 * DAY_NS, f64::NAN),
        ] {
            let mut values = vec![
                (DAY_NS, 0.02),
                (2 * DAY_NS, 0.02),
                (3 * DAY_NS, 0.02),
                extra,
            ];
            values.sort_by_key(|(time, _)| *time);
            let rows = statistics(DAY_NS, 4 * DAY_NS + 1, true, values);
            assert_eq!(value(&rows, "Calendar Partial Return Days"), None);
            unavailable(&rows);
        }
    }

    #[test]
    fn nonfinite_and_noncompoundable_returns_remain_unavailable() {
        for bad in [f64::NAN, f64::INFINITY, -1.01] {
            unavailable(&measured(&[0.02, bad]));
        }
    }
}
