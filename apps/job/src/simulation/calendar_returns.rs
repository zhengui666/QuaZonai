//! Supplementary QZ summaries over the original Nautilus daily account returns.
//! No new returns, forward filling, alternative Sharpe or qualification thresholds.
use contracts::science::{NativeStatisticGroup, NativeStatisticV1};

const DAY_NS: u64 = 86_400_000_000_000;
const PERFORMANCE_KEYS: [&str; 6] = [
    "Calendar Geometric Daily Return",
    "Calendar Minimum Daily Return",
    "Calendar Maximum Daily Return",
    "Calendar Maximum Absolute Deviation",
    "Calendar Negative Days",
    "Calendar Zero Days",
];

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

pub(super) fn statistics(
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
    let performance = complete.then(|| {
        let geometric = if values.iter().any(|(_, value)| *value == -1.0) {
            -1.0
        } else {
            (values.iter().map(|(_, value)| value.ln_1p()).sum::<f64>() / values.len() as f64)
                .exp_m1()
        };
        let minimum = values
            .iter()
            .map(|(_, value)| *value)
            .fold(f64::INFINITY, f64::min);
        let maximum = values
            .iter()
            .map(|(_, value)| *value)
            .fold(f64::NEG_INFINITY, f64::max);
        let deviation = values
            .iter()
            .map(|(_, value)| (value - geometric).abs())
            .fold(0.0, f64::max);
        [
            geometric,
            minimum,
            maximum,
            deviation,
            values.iter().filter(|(_, value)| *value < 0.0).count() as f64,
            values.iter().filter(|(_, value)| *value == 0.0).count() as f64,
        ]
    });
    for (index, key) in PERFORMANCE_KEYS.iter().enumerate() {
        rows.push(metric(key, performance.map(|numbers| numbers[index])));
    }
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
        for key in PERFORMANCE_KEYS {
            let row = rows.iter().find(|row| row.native_key == key).unwrap();
            assert!(row.value.is_none(), "{key}");
            assert_eq!(
                row.reason_code.as_deref(),
                Some("NATIVE_STATISTIC_UNAVAILABLE")
            );
        }
    }

    #[test]
    fn geometric_mean_and_absolute_daily_dispersion_use_every_native_day() {
        let rows = measured(&[0.019, 0.02, 0.021]);
        let expected = (1.019_f64 * 1.02 * 1.021).powf(1.0 / 3.0) - 1.0;
        let geometric = value(&rows, PERFORMANCE_KEYS[0]).unwrap();
        assert!((geometric - expected).abs() < 1e-14);
        assert_eq!(value(&rows, PERFORMANCE_KEYS[1]), Some(0.019));
        assert_eq!(value(&rows, PERFORMANCE_KEYS[2]), Some(0.021));
        assert!((value(&rows, PERFORMANCE_KEYS[3]).unwrap() - (0.021 - geometric)).abs() < 1e-14);
        assert_eq!(value(&rows, "Calendar Coverage Ratio"), Some(1.0));
        assert_eq!(value(&rows, "Calendar Expected Days"), Some(3.0));
        assert_eq!(value(&rows, "Calendar Negative Days"), Some(0.0));
        assert!(!rows.iter().any(|row| row.native_key.contains("Sharpe")));
    }

    #[test]
    fn one_good_day_cannot_hide_zero_days_or_large_dispersion() {
        let rows = measured(&[0.0, 0.06, 0.0]);
        assert_eq!(value(&rows, "Calendar Minimum Daily Return"), Some(0.0));
        assert_eq!(value(&rows, "Calendar Zero Days"), Some(2.0));
        assert!(value(&rows, "Calendar Maximum Absolute Deviation").unwrap() > 0.04);
        let cash = measured(&[0.0, 0.0, 0.0]);
        assert_eq!(value(&cash, "Calendar Geometric Daily Return"), Some(0.0));
        assert_eq!(
            value(&cash, "Calendar Maximum Absolute Deviation"),
            Some(0.0)
        );
        assert_eq!(value(&cash, "Calendar Zero Days"), Some(3.0));
    }

    #[test]
    fn losses_and_total_loss_remain_real_not_missing_or_clamped() {
        let rows = measured(&[0.02, -1.0, 0.03]);
        assert_eq!(value(&rows, "Calendar Geometric Daily Return"), Some(-1.0));
        assert_eq!(value(&rows, "Calendar Minimum Daily Return"), Some(-1.0));
        assert_eq!(value(&rows, "Calendar Negative Days"), Some(1.0));
        assert_eq!(value(&rows, "Calendar Zero Days"), Some(0.0));
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
        let expected = (1.019_f64 * 1.02 * 1.021).powf(1.0 / 3.0) - 1.0;
        assert!(
            (value(&rows, "Calendar Geometric Daily Return").unwrap() - expected).abs() < 1e-14
        );
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
        assert_eq!(value(&rows, "Calendar Minimum Daily Return"), Some(-0.01));
        assert_eq!(value(&rows, "Calendar Maximum Daily Return"), Some(0.02));
        assert_eq!(value(&rows, "Calendar Negative Days"), Some(1.0));
        assert_eq!(value(&rows, "Calendar Zero Days"), Some(1.0));
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
