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
    let values: Vec<_> = returns.into_iter().collect();
    let expected =
        (start_ns < end_ns && start_ns.is_multiple_of(DAY_NS) && end_ns.is_multiple_of(DAY_NS))
            .then(|| (end_ns - start_ns) / DAY_NS);
    let complete = fully_priced
        && expected.is_some_and(|days| {
            values.len() as u64 == days
                && values.iter().enumerate().all(|(index, (time, value))| {
                    *time == start_ns + index as u64 * DAY_NS && value.is_finite() && *value >= -1.0
                })
        });
    let mut rows = vec![
        metric("Calendar Expected Days", expected.map(|days| days as f64)),
        metric("Calendar Observed Return Days", Some(values.len() as f64)),
        metric(
            "Calendar Coverage Ratio",
            expected.map(|days| values.len() as f64 / days as f64),
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
        let rows = statistics(DAY_NS + 1, 2 * DAY_NS, true, [(DAY_NS, 0.02)]);
        assert_eq!(value(&rows, "Calendar Expected Days"), None);
        unavailable(&rows);
    }

    #[test]
    fn nonfinite_and_noncompoundable_returns_remain_unavailable() {
        for bad in [f64::NAN, f64::INFINITY, -1.01] {
            unavailable(&measured(&[0.02, bad]));
        }
    }
}
