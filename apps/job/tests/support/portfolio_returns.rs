//! Empty UTC-day return observations retain the fixed native statistic semantics.
use contracts::{
    evidence::MetricStatus,
    science::{NativeReturnsKind, NativeSimulationResultV1, NativeStatisticGroup},
};
use nautilus_analysis::{analyzer::PortfolioAnalyzer, statistics::max_drawdown::MaxDrawdown};
use std::sync::Arc;

pub fn assert_daily_returns_unavailable(result: &NativeSimulationResultV1) {
    assert_eq!(result.returns_kind, NativeReturnsKind::PortfolioDaily);
    assert_eq!(result.returns_status, MetricStatus::InsufficientData);
    assert_eq!(
        result.returns_reason.as_deref(),
        Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE")
    );
    assert!(result.returns.is_empty());
    // These non-prediction fixtures use native default annualization. Empty
    // MaxDrawdown is 0; other statistics may be NaN. Neither creates daily data.
    let mut native = PortfolioAnalyzer::default();
    native.register_statistic(Arc::new(MaxDrawdown::new()));
    let expected = native.get_performance_stats_portfolio_returns();
    let observed = result
        .statistics
        .iter()
        .filter(|statistic| {
            statistic.group == NativeStatisticGroup::Returns
                && !statistic.native_key.starts_with("Calendar ")
        })
        .collect::<Vec<_>>();
    assert_eq!(observed.len(), expected.len());
    for statistic in observed {
        let value = expected[&statistic.native_key];
        assert_eq!(statistic.value, value.is_finite().then_some(value));
        assert_eq!(
            statistic.reason_code.as_deref(),
            (!value.is_finite()).then_some("NATIVE_STATISTIC_UNAVAILABLE")
        );
    }
    // Supplementary QZ account summaries do not replace the exact native set.
    let calendar = result
        .statistics
        .iter()
        .filter(|statistic| statistic.native_key.starts_with("Calendar "))
        .collect::<Vec<_>>();
    assert_eq!(calendar.len(), 9);
    assert_eq!(
        calendar
            .iter()
            .find(|row| row.native_key == "Calendar Observed Return Days")
            .unwrap()
            .value,
        Some(0.0)
    );
    for key in [
        "Calendar Geometric Daily Return",
        "Calendar Minimum Daily Return",
        "Calendar Maximum Daily Return",
        "Calendar Maximum Absolute Deviation",
        "Calendar Negative Days",
        "Calendar Zero Days",
    ] {
        let row = calendar.iter().find(|row| row.native_key == key).unwrap();
        assert!(row.value.is_none(), "{key}");
        assert!(row.reason_code.is_some());
    }
}
