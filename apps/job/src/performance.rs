//! Thin registration and aligned account-return adapters for official statistics.
use std::{collections::BTreeMap, sync::Arc};

use contracts::{
    science::{NativeStatisticGroup, NativeStatisticV1},
    spot_cash::ReportCurrencyDailyReturnsV1,
};
use nautilus_analysis::{
    analyzer::PortfolioAnalyzer,
    statistics::{
        alpha::Alpha, beta_ratio::BetaRatio, calmar_ratio::CalmarRatio,
        down_capture_ratio::DownCaptureRatio, expected_shortfall::ExpectedShortfall,
        information_ratio::InformationRatio, tracking_error::TrackingError,
        treynor_ratio::TreynorRatio, ulcer_index::UlcerIndex, up_capture_ratio::UpCaptureRatio,
        value_at_risk::ValueAtRisk,
    },
};

pub(crate) fn register_tail_risk(analyzer: &mut PortfolioAnalyzer, period: usize) {
    analyzer.register_statistic(Arc::new(CalmarRatio::new(Some(period))));
    analyzer.register_statistic(Arc::new(ValueAtRisk::new(Some(0.95))));
    analyzer.register_statistic(Arc::new(ExpectedShortfall::new(Some(0.95))));
    analyzer.register_statistic(Arc::new(UlcerIndex::new()));
}

fn benchmark_analyzer(period: usize) -> PortfolioAnalyzer {
    let mut analyzer = PortfolioAnalyzer::new();
    let period = Some(period);
    analyzer.register_statistic(Arc::new(Alpha::new(period, Some(0.0))));
    analyzer.register_statistic(Arc::new(BetaRatio::new()));
    analyzer.register_statistic(Arc::new(InformationRatio::new(period)));
    analyzer.register_statistic(Arc::new(TrackingError::new(period)));
    analyzer.register_statistic(Arc::new(TreynorRatio::new(period, Some(0.0))));
    analyzer.register_statistic(Arc::new(UpCaptureRatio::new(period)));
    analyzer.register_statistic(Arc::new(DownCaptureRatio::new(period)));
    analyzer
}

/// Compare only the existing, explicitly requested hold/rebalance account replays.
/// This is not a market-index benchmark or a synthetic buy-and-hold series.
pub(crate) fn versus_hold(
    portfolio: &ReportCurrencyDailyReturnsV1,
    hold: &ReportCurrencyDailyReturnsV1,
) -> Vec<NativeStatisticV1> {
    let mut analyzer = benchmark_analyzer(usize::from(portfolio.annualization_days));
    let reason = if portfolio.report_currency != hold.report_currency {
        Some("BENCHMARK_CURRENCY_MISMATCH")
    } else if portfolio.annualization_days != hold.annualization_days
        || portfolio.flow_evidence.period_start_ns != hold.flow_evidence.period_start_ns
        || portfolio.flow_evidence.period_end_ns != hold.flow_evidence.period_end_ns
        || portfolio.days.len() != hold.days.len()
        || portfolio
            .days
            .iter()
            .zip(&hold.days)
            .any(|(a, b)| a.day_start_ns != b.day_start_ns || a.day_end_ns != b.day_end_ns)
    {
        Some("BENCHMARK_WINDOW_MISMATCH")
    } else if portfolio.days.is_empty()
        || portfolio
            .days
            .iter()
            .chain(&hold.days)
            .any(|day| day.reason.is_some() || !day.value.is_some_and(f64::is_finite))
    {
        Some("BENCHMARK_RETURNS_INCOMPLETE")
    } else {
        None
    };
    let mut benchmark = BTreeMap::new();
    if reason.is_none() {
        for (a, b) in portfolio.days.iter().zip(&hold.days) {
            // The native benchmark method reads primary returns. Populate only
            // these validated account days; no position-return fallback exists.
            analyzer
                .returns
                .insert(a.day_end_ns.get().into(), a.value.unwrap());
            benchmark.insert(b.day_end_ns.get().into(), b.value.unwrap());
        }
    }
    let mut output: Vec<_> = analyzer
        .get_performance_stats_returns_vs_benchmark(&benchmark)
        .into_iter()
        .map(|(key, value)| NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key: format!("{key} vs observed hold (risk-free 0%)"),
            currency: None,
            value: reason.is_none().then_some(value).filter(|v| v.is_finite()),
            reason_code: reason
                .map(str::to_owned)
                .or_else(|| (!value.is_finite()).then(|| "NATIVE_STATISTIC_UNAVAILABLE".into())),
        })
        .collect();
    output.sort_by(|a, b| a.native_key.cmp(&b.native_key));
    output
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_drawdown_preserves_account_return_semantics() {
        let observed = |values: &[f64]| -> nautilus_analysis::Returns {
            values.iter().enumerate().map(|(index, value)|
                (nautilus_core::UnixNanos::from(1_700_000_000_000_000_000 + index as u64), *value)
            ).collect()
        };
        let baseline = super::native_max_drawdown(&observed(&[-0.2, 0.25])).unwrap();
        let candidate = super::native_max_drawdown(&observed(&[-0.1, 1.0 / 9.0])).unwrap();
        assert!((baseline + 0.2).abs() < 1e-14);
        assert!((candidate - baseline - 0.1).abs() < 1e-14);
        assert_eq!(super::native_max_drawdown(&observed(&[-1.0])), Some(-1.0));
        assert_eq!(super::native_max_drawdown(&observed(&[])), None);
        assert_eq!(super::native_max_drawdown(&observed(&[f64::MAX, f64::MAX])), None);
    }

    use super::*;
    use contracts::{spot_cash::*, DbCounter, Id, SchemaV1};

    fn days(values: &[f64]) -> ReportCurrencyDailyReturnsV1 {
        let count = |n| DbCounter::new(n).unwrap();
        let day = 86_400_000_000_000;
        let id = "00000000-0000-4000-8000-000000000001".parse().unwrap();
        let binding = NativeSpotSnapshotBindingV1 {
            origin: NativeSpotSnapshotOriginV1::NativePublication,
            native_instance_id: id,
            event_id: id,
            dataset_revision_id: Id::new(),
            snapshot_sequence: count(1),
            session_id: Id::new(),
            account_id: "TEST-001".into(),
            venue: "TEST".into(),
            asof_ns: count(0),
        };
        ReportCurrencyDailyReturnsV1 {
            schema_version: SchemaV1,
            kind: ReportCurrencyReturnsKindV1::ReportCurrencyDaily,
            report_currency: "USDC".into(),
            policy: NativeSpotCashPolicyV1 {
                schema_version: SchemaV1,
                mode: NativeSpotCashModeV1::MultiCurrencyCash,
                report_currency: "USDC".into(),
                price_method: NativeSpotPriceMethodV1::ClosedBarClose,
                allowed_instrument_ids: vec![],
                returns_policy: ReportCurrencyDailyPolicyV1::FreshSimulationNoExternalFlows,
                daily_sampling: NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries,
                maximum_price_age_ns: count(day),
            },
            annualization_days: 365,
            flow_evidence: NativeSpotCashFlowEvidenceV1 {
                native_run_id: id,
                native_instance_id: id,
                session_id: binding.session_id,
                dataset_revision_id: binding.dataset_revision_id,
                account_id: binding.account_id.clone(),
                venue: binding.venue.clone(),
                period_start_ns: count(0),
                period_end_ns: count(values.len() as u64 * day),
                opening_snapshot: binding.clone(),
                closing_snapshot: binding,
                observed_snapshot_count: count(values.len() as u64 + 1),
                external_flow_count: Some(count(0)),
            },
            days: values
                .iter()
                .enumerate()
                .map(|(i, value)| ReportCurrencyDailyReturnV1 {
                    day_start_ns: count(i as u64 * day),
                    day_end_ns: count((i as u64 + 1) * day),
                    value: Some(*value),
                    reason: None,
                    opening_snapshot: None,
                    closing_snapshot: None,
                })
                .collect(),
        }
    }

    #[test]
    fn tail_statistics_use_native_account_returns() {
        let mut analyzer = PortfolioAnalyzer::new();
        register_tail_risk(&mut analyzer, 365);
        for (i, value) in [0.01, -0.03, 0.02, -0.01, 0.04].into_iter().enumerate() {
            analyzer
                .portfolio_returns
                .insert((i as u64 * 86_400_000_000_000).into(), value);
        }
        let results = analyzer.get_performance_stats_portfolio_returns();
        for stat in [
            Arc::new(CalmarRatio::new(Some(365))) as nautilus_analysis::analyzer::Statistic,
            Arc::new(ValueAtRisk::new(Some(0.95))),
            Arc::new(ExpectedShortfall::new(Some(0.95))),
            Arc::new(UlcerIndex::new()),
        ] {
            assert_eq!(
                results[&stat.name()],
                stat.calculate_from_returns(analyzer.portfolio_returns())
                    .unwrap()
            );
        }
    }

    #[test]
    fn hold_comparison_matches_native_beta_and_retains_unavailable_values() {
        let portfolio = days(&[0.02, -0.04, 0.06, -0.02]);
        let hold = days(&[0.01, -0.02, 0.03, -0.01]);
        let output = versus_hold(&portfolio, &hold);
        assert_eq!(output.len(), 7);
        let beta = output
            .iter()
            .find(|v| v.native_key.starts_with("Beta"))
            .unwrap();
        assert!((beta.value.unwrap() - 2.0).abs() < 1e-12);
        let constant = days(&[0.0; 4]);
        let unavailable = versus_hold(&portfolio, &constant);
        assert!(unavailable
            .iter()
            .find(|v| v.native_key.starts_with("Beta"))
            .unwrap()
            .value
            .is_none());
    }

    #[test]
    fn mismatched_or_incomplete_benchmarks_never_compute() {
        let portfolio = days(&[0.01, -0.02, 0.03]);
        for (reason, mut hold) in [
            ("BENCHMARK_CURRENCY_MISMATCH", days(&[0.01, -0.02, 0.03])),
            ("BENCHMARK_WINDOW_MISMATCH", days(&[0.01, -0.02, 0.03])),
            ("BENCHMARK_RETURNS_INCOMPLETE", days(&[0.01, -0.02, 0.03])),
        ] {
            match reason {
                "BENCHMARK_CURRENCY_MISMATCH" => hold.report_currency = "USD".into(),
                "BENCHMARK_WINDOW_MISMATCH" => hold.days[0].day_end_ns = DbCounter::new(1).unwrap(),
                _ => hold.days[1].value = None,
            }
            let output = versus_hold(&portfolio, &hold);
            assert_eq!(output.len(), 7);
            assert!(output
                .iter()
                .all(|s| s.value.is_none() && s.reason_code.as_deref() == Some(reason)));
        }
    }
}

/// Explicit price benchmark. Only exact closed-price boundaries are accepted;
/// never invent prices, fill missing days, convert currencies, or simulate fees.
pub(crate) fn versus_catalog_price(
    root: &std::path::Path,
    selection: &contracts::science::NativeBarSelectionV1,
    evaluation: &contracts::science::NativeBarSelectionV1,
    currency: &str,
    period: usize,
    days: &[(u64, u64, Option<f64>)],
) -> Vec<NativeStatisticV1> {
    use anyhow::{ensure, Result};
    use nautilus_model::instruments::Instrument;
    let mut analyzer = benchmark_analyzer(period);
    let mut benchmark = BTreeMap::new();
    let result = (|| -> Result<()> {
        ensure!(
            selection.bar_types.len() == 1,
            "BENCHMARK_SINGLE_SERIES_REQUIRED"
        );
        const DAY: u64 = 86_400_000_000_000;
        let expected_start = evaluation
            .event_start_ns
            .get()
            .div_ceil(DAY)
            .saturating_mul(DAY);
        let expected_end = evaluation.event_end_ns.get() / DAY * DAY;
        ensure!(
            !days.is_empty()
                && expected_start < expected_end
                && days.first().is_some_and(|d| d.0 == expected_start)
                && days.last().is_some_and(|d| d.1 == expected_end)
                && days.windows(2).all(|pair| pair[0].1 == pair[1].0)
                && days
                    .iter()
                    .all(
                        |(start, end, value)| end.checked_sub(*start) == Some(86_400_000_000_000)
                            && start % 86_400_000_000_000 == 0
                            && value.is_some_and(f64::is_finite)
                    ),
            "BENCHMARK_RETURNS_INCOMPLETE"
        );
        // A reusable policy may declare a later benchmark cutoff than this
        // validation/holdout run. Never read observations unavailable to this run.
        let mut bounded_selection = selection.clone();
        bounded_selection.decision_cutoff_ns = selection
            .decision_cutoff_ns
            .min(evaluation.decision_cutoff_ns);
        bounded_selection.event_end_ns = selection
            .event_end_ns
            .min(bounded_selection.decision_cutoff_ns);
        let market = crate::catalog::load_catalog(root, &bounded_selection)
            .map_err(|_| anyhow::anyhow!("BENCHMARK_CATALOG_UNAVAILABLE"))?;
        ensure!(market.series.len() == 1, "BENCHMARK_SINGLE_SERIES_REQUIRED");
        let series = &market.series[0];
        ensure!(
            series.instrument.quote_currency().to_string() == currency,
            "BENCHMARK_CURRENCY_MISMATCH"
        );
        // Catalog INTERNAL bars carry a known right-open convention. EXTERNAL
        // timestamp semantics are not declared, so do not guess their close.
        let mut prices = BTreeMap::new();
        for bar in &series.bars {
            let close = crate::catalog::right_open_bar_close_ns(bar)
                .map_err(|_| anyhow::anyhow!("BENCHMARK_CLOSE_CONVENTION_UNAVAILABLE"))?;
            if close % 86_400_000_000_000 == 0 {
                let price = bar.close.as_f64();
                ensure!(price.is_finite() && price > 0.0, "BENCHMARK_PRICE_INVALID");
                ensure!(
                    prices.insert(close, price).is_none(),
                    "BENCHMARK_DUPLICATE_BOUNDARY"
                );
            }
        }
        for &(start, end, value) in days {
            let opening = prices
                .get(&start)
                .ok_or_else(|| anyhow::anyhow!("BENCHMARK_WINDOW_MISMATCH"))?;
            let closing = prices
                .get(&end)
                .ok_or_else(|| anyhow::anyhow!("BENCHMARK_WINDOW_MISMATCH"))?;
            let price_return = closing / opening - 1.0;
            ensure!(price_return.is_finite(), "BENCHMARK_PRICE_INVALID");
            analyzer.returns.insert(end.into(), value.unwrap());
            benchmark.insert(end.into(), price_return);
        }
        Ok(())
    })();
    let reason = result.err().map(|e| e.to_string());
    if reason.is_some() {
        analyzer.returns.clear();
        benchmark.clear();
    }
    let label = selection
        .bar_types
        .first()
        .map(String::as_str)
        .unwrap_or("unspecified");
    let mut output: Vec<_> = analyzer
        .get_performance_stats_returns_vs_benchmark(&benchmark)
        .into_iter()
        .map(|(key, value)| NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key: format!(
                "{key} vs {label} price return (fees/funding excluded; risk-free 0%)"
            ),
            currency: None,
            value: reason.is_none().then_some(value).filter(|v| v.is_finite()),
            reason_code: reason
                .clone()
                .or_else(|| (!value.is_finite()).then(|| "NATIVE_STATISTIC_UNAVAILABLE".into())),
        })
        .collect();
    output.sort_by(|a, b| a.native_key.cmp(&b.native_key));
    output
}

#[cfg(test)]
mod price_tests {
    use super::*;
    use contracts::{science::NativeBarSelectionV1, DbCounter, SchemaV1};
    use nautilus_model::{
        data::{Bar, BarType},
        identifiers::{InstrumentId, Symbol},
        instruments::{CurrencyPair, InstrumentAny},
        types::{Currency, Price, Quantity},
    };
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;

    #[test]
    fn catalog_price_benchmark_reads_real_parquet_and_rejects_currency_and_date_gaps() {
        const DAY: u64 = 86_400_000_000_000;
        let dir = tempfile::tempdir().unwrap();
        let catalog =
            ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, Some(16), None, None)
                .unwrap();
        let instrument = CurrencyPair::builder()
            .instrument_id(InstrumentId::from("BTC/USDC.TEST"))
            .raw_symbol(Symbol::new("BTC/USDC"))
            .base_currency(Currency::BTC())
            .quote_currency(Currency::USDC())
            .price_precision(2)
            .size_precision(2)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.01"))
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap();
        catalog
            .write_instruments(vec![InstrumentAny::CurrencyPair(instrument.clone())])
            .unwrap();
        let bar_type: BarType = "BTC/USDC.TEST-1-DAY-LAST-INTERNAL".parse().unwrap();
        let bars = [100.0, 110.0, 99.0, 118.8]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let price = Price::new(p, 2);
                Bar::new(
                    bar_type,
                    price,
                    price,
                    price,
                    price,
                    Quantity::from("1.00"),
                    (i as u64 * DAY).into(),
                    ((i as u64 + 1) * DAY).into(),
                )
            })
            .collect::<Vec<_>>();
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
        let selection = NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec![bar_type.to_string()],
            event_start_ns: DbCounter::ZERO,
            event_end_ns: DbCounter::new(4 * DAY).unwrap(),
            decision_cutoff_ns: DbCounter::new(4 * DAY + 1).unwrap(),
            maximum_rows: 0,
        };
        let mut evaluation = selection.clone();
        evaluation.event_start_ns = DbCounter::new(DAY).unwrap();
        let days = [
            (DAY, 2 * DAY, Some(0.2)),
            (2 * DAY, 3 * DAY, Some(-0.2)),
            (3 * DAY, 4 * DAY, Some(0.4)),
        ];
        let output = versus_catalog_price(dir.path(), &selection, &evaluation, "USDC", 365, &days);
        assert_eq!(output.len(), 7);
        let beta = output
            .iter()
            .find(|s| s.native_key.starts_with("Beta"))
            .unwrap();
        assert!((beta.value.unwrap_or_else(|| panic!("{output:?}")) - 2.0).abs() < 1e-10);
        assert!(
            versus_catalog_price(dir.path(), &selection, &evaluation, "USD", 365, &days)
                .iter()
                .all(|s| s.value.is_none()
                    && s.reason_code.as_deref() == Some("BENCHMARK_CURRENCY_MISMATCH"))
        );
        assert!(versus_catalog_price(
            dir.path(),
            &selection,
            &evaluation,
            "USDC",
            365,
            &[(0, DAY, Some(0.1))]
        )
        .iter()
        .all(|s| s.value.is_none()
            && s.reason_code.as_deref() == Some("BENCHMARK_RETURNS_INCOMPLETE")));
        // Missing leading/trailing days (e.g. a truncated native snapshot ring),
        // an interior zero-equity omission, duplicates and explicit unknown days
        // must not become a shorter, apparently complete comparison sample.
        for incomplete in [
            days[1..].to_vec(),
            days[..2].to_vec(),
            vec![days[0], days[2]],
            vec![days[0], days[0], days[1], days[2]],
            vec![days[0], (2 * DAY, 3 * DAY, None), days[2]],
        ] {
            let output = versus_catalog_price(
                dir.path(),
                &selection,
                &evaluation,
                "USDC",
                365,
                &incomplete,
            );
            assert!(output.iter().all(|s| s.value.is_none()
                && s.reason_code.as_deref() == Some("BENCHMARK_RETURNS_INCOMPLETE")));
        }
        let zero_days = days.map(|(start, end, _)| (start, end, Some(0.0)));
        let zero =
            versus_catalog_price(dir.path(), &selection, &evaluation, "USDC", 365, &zero_days);
        assert_eq!(
            zero.iter()
                .find(|s| s.native_key.starts_with("Beta"))
                .unwrap()
                .value,
            Some(0.0)
        );

        // Same historical event close, but its observation is only published
        // after this evaluation cutoff. A wider reusable policy must not leak it.
        let late_dir = tempfile::tempdir().unwrap();
        let late_catalog = ParquetDataCatalog::from_uri(
            late_dir.path().to_str().unwrap(),
            None,
            Some(16),
            None,
            None,
        )
        .unwrap();
        late_catalog
            .write_instruments(vec![InstrumentAny::CurrencyPair(instrument)])
            .unwrap();
        let mut late_bars = bars.clone();
        late_bars.last_mut().unwrap().ts_init = (6 * DAY).into();
        late_catalog
            .write_to_parquet(&late_bars, None, None, None)
            .unwrap();
        let mut wider_policy = selection.clone();
        wider_policy.decision_cutoff_ns = DbCounter::new(7 * DAY).unwrap();
        let unavailable = versus_catalog_price(
            late_dir.path(),
            &wider_policy,
            &evaluation,
            "USDC",
            365,
            &days,
        );
        assert!(unavailable
            .iter()
            .all(|s| s.value.is_none()
                && s.reason_code.as_deref() == Some("BENCHMARK_WINDOW_MISMATCH")));
        // Prove the record exists and becomes eligible only in a later as-of run.
        evaluation.decision_cutoff_ns = wider_policy.decision_cutoff_ns;
        let later = versus_catalog_price(
            late_dir.path(),
            &wider_policy,
            &evaluation,
            "USDC",
            365,
            &days,
        );
        assert!(later
            .iter()
            .find(|s| s.native_key.starts_with("Beta"))
            .unwrap()
            .value
            .is_some());
    }
}

#[cfg(test)]
pub(crate) fn native_max_drawdown(returns: &nautilus_analysis::Returns) -> Option<f64> {
    use nautilus_analysis::statistic::PortfolioStatistic;
    if returns.is_empty() {
        return None;
    }
    // Native MaxDrawdown has no overflow/missing-evidence guard. Keep this input
    // validity check outside the statistic, without calculating drawdown again.
    returns.values().try_fold(1.0_f64, |nav, value| {
        let next = nav * (1.0 + value);
        next.is_finite().then_some(next)
    })?;
    nautilus_analysis::statistics::max_drawdown::MaxDrawdown::new()
        .calculate_from_returns(returns)
        .filter(|value| value.is_finite())
}
