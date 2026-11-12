//! Metadata and thin official statistics over snapshot-backed returns only.
use contracts::native_strategy::{
    NativeEquityPerformanceV1, NativeEquityReturnV1, NativeEquityStatisticV1,
};
use nautilus_analysis::{
    analyzer::PortfolioAnalyzer,
    statistic::PortfolioStatistic,
    statistics::{cagr::CAGR, sharpe_ratio::SharpeRatio},
};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_model::{events::PortfolioSnapshot, identifiers::AccountId};
use std::collections::BTreeSet;

pub(super) fn from_engine(engine: &BacktestEngine) -> NativeEquityPerformanceV1 {
    let ids: Vec<_> = engine
        .kernel()
        .cache
        .borrow()
        .accounts_all_owned()
        .iter()
        .map(|account| account.id())
        .collect();
    let snapshots: Vec<_> = ids
        .iter()
        .flat_map(|id| engine.kernel().portfolio.borrow().snapshots(id))
        .collect();
    analyze(&ids, &snapshots)
}
fn analyze(ids: &[AccountId], snapshots: &[PortfolioSnapshot]) -> NativeEquityPerformanceV1 {
    let mut analyzer = PortfolioAnalyzer::default();
    analyzer.set_portfolio_returns_from_snapshots(ids, snapshots);
    // Never call returns(): its upstream contract permits position fallback.
    let returns = analyzer.portfolio_returns();
    let unpriced = snapshots
        .iter()
        .filter(|snapshot| !snapshot.unpriced_instruments.is_empty())
        .count();
    let reason = if unpriced > 0 {
        Some("UNPRICED_PORTFOLIO_SNAPSHOTS")
    } else if returns.is_empty() {
        Some("OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE")
    } else {
        None
    };
    let mut statistics = Vec::new();
    for period in [252, 365] {
        let cagr = CAGR::new(Some(period));
        let sharpe = SharpeRatio::new(Some(period));
        for (native_key, value) in [
            (cagr.name(), cagr.calculate_from_returns(returns)),
            (sharpe.name(), sharpe.calculate_from_returns(returns)),
        ] {
            statistics.push(NativeEquityStatisticV1 {
                native_key,
                annualization_days: period,
                value: value.filter(|value| value.is_finite()),
            });
        }
    }
    let currencies: BTreeSet<_> = snapshots
        .iter()
        .filter(|snapshot| {
            snapshot.unpriced_instruments.is_empty() && snapshot.total_equity.len() == 1
        })
        .map(|snapshot| {
            snapshot
                .base_currency_equity
                .unwrap_or(snapshot.total_equity[0])
                .currency
                .to_string()
        })
        .collect();
    NativeEquityPerformanceV1 {
        source: "NAUTILUS_PORTFOLIO_SNAPSHOTS".into(), status: if reason.is_none() { "AVAILABLE" } else { "UNAVAILABLE" }.into(), reason: reason.map(str::to_owned),
        currency: if currencies.len() == 1 { currencies.into_iter().next() } else { None },
        returns: returns.iter().map(|(time,value)| NativeEquityReturnV1 { ts_event_ns: time.as_u64().to_string(), value: value.is_finite().then_some(*value) }).collect(), statistics,
        first_snapshot_ns: snapshots.iter().map(|snapshot| snapshot.ts_event.as_u64()).min().map(|value| value.to_string()),
        last_snapshot_ns: snapshots.iter().map(|snapshot| snapshot.ts_event.as_u64()).max().map(|value| value.to_string()),
        snapshot_count: snapshots.len().to_string(), observed_utc_days: snapshots.iter().map(|snapshot| snapshot.ts_event.as_u64()/86_400_000_000_000).collect::<BTreeSet<_>>().len().to_string(),
        unpriced_snapshot_count: unpriced.to_string(), return_days: returns.len().to_string(),
        notes: vec!["Official PortfolioAnalyzer snapshot-only daily returns; never position-return fallback. Raw canonical result is unchanged.".into(), "Official analyzer skips unpriced snapshots, uses upstream registration/day-boundary semantics and carries equity forward across missing calendar days. Observed UTC days count original timestamps, not independent priced daily observations.".into(), "252 and 365 are separately labeled upstream annualization parameters, not inferred trading frequency; Sharpe uses the upstream zero risk-free assumption. Non-finite or missing upstream statistics are null.".into(), format!("Original snapshots with carried/stale valuation inputs: {}.", snapshots.iter().filter(|snapshot| snapshot.is_stale).count())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_core::{UnixNanos, UUID4};
    use nautilus_model::{
        enums::AccountType,
        identifiers::InstrumentId,
        types::{Currency, Money},
    };
    fn snapshot(day: u64, equity: f64) -> PortfolioSnapshot {
        let time = UnixNanos::from(day * 86_400_000_000_000 + 1);
        PortfolioSnapshot::new(
            AccountId::from("SIM-001"),
            AccountType::Cash,
            Some(Currency::USD()),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![Money::new(equity, Currency::USD())],
            None,
            false,
            vec![],
            vec![],
            vec![],
            UUID4::new(),
            time,
            time,
        )
    }
    #[test]
    fn official_snapshot_statistics_are_forwarded_without_position_fallback() {
        let snapshots = vec![
            snapshot(1, 100.0),
            snapshot(2, 110.0),
            snapshot(4, 105.0),
            snapshot(5, 120.0),
        ];
        let ids = [AccountId::from("SIM-001")];
        let result = analyze(&ids, &snapshots);
        let mut official = PortfolioAnalyzer::default();
        official.set_portfolio_returns_from_snapshots(&ids, &snapshots);
        assert_eq!(result.status, "AVAILABLE");
        assert_eq!(result.snapshot_count, "4");
        assert_eq!(result.observed_utc_days, "4");
        assert_eq!(
            result.return_days,
            official.portfolio_returns().len().to_string()
        );
        assert!(
            official.portfolio_returns().len() > snapshots.len() - 1,
            "Official missing-day carry-forward is not relabeled as observed data"
        );
        for (item, (time, value)) in result.returns.iter().zip(official.portfolio_returns()) {
            assert_eq!(item.ts_event_ns, time.as_u64().to_string());
            assert_eq!(item.value, Some(*value));
        }
        for period in [252, 365] {
            let cagr = CAGR::new(Some(period));
            let sharpe = SharpeRatio::new(Some(period));
            for (key, value) in [
                (
                    cagr.name(),
                    cagr.calculate_from_returns(official.portfolio_returns()),
                ),
                (
                    sharpe.name(),
                    sharpe.calculate_from_returns(official.portfolio_returns()),
                ),
            ] {
                assert_eq!(
                    result
                        .statistics
                        .iter()
                        .find(|entry| entry.native_key == key && entry.annualization_days == period)
                        .unwrap()
                        .value,
                    value.filter(|number| number.is_finite())
                );
            }
        }
    }
    #[test]
    fn empty_and_unpriced_equity_are_explicitly_unavailable() {
        let ids = [AccountId::from("SIM-001")];
        let empty = analyze(&ids, &[]);
        assert_eq!(empty.status, "UNAVAILABLE");
        assert!(empty.returns.is_empty());
        assert_eq!(
            empty.reason.as_deref(),
            Some("OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE")
        );
        let mut snapshots = vec![snapshot(1, 100.0), snapshot(2, 110.0), snapshot(3, 120.0)];
        snapshots[1]
            .unpriced_instruments
            .push(InstrumentId::from("TEST.SIM"));
        let missing = analyze(&ids, &snapshots);
        assert_eq!(missing.unpriced_snapshot_count, "1");
        assert_eq!(missing.status, "UNAVAILABLE");
        assert_eq!(
            missing.reason.as_deref(),
            Some("UNPRICED_PORTFOLIO_SNAPSHOTS")
        );
        assert_eq!(missing.snapshot_count, "3");
    }
}
