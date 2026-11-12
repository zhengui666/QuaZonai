//! Exercise actual native catalog, indicators, Strategy and backtest engine.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use contracts::{SchemaV1, indicator_strategy::*};
use job::simulation::simulate_indicator_strategy;

fn request(rows: u32) -> (tempfile::TempDir, NativeIndicatorStrategyRequestV1) {
    let (directory, mut simulation) = market::market("0", rows);
    simulation.selection.bar_types.truncate(1);
    simulation.settings.fee_rates.truncate(1);
    let start = simulation.selection.event_start_ns;
    (
        directory,
        NativeIndicatorStrategyRequestV1 {
            schema_version: SchemaV1,
            selection: simulation.selection,
            settings: simulation.settings,
            signal: NativeIndicatorSignalV1::EmaCross {
                fast_period: 2,
                slow_period: 3,
            },
            decision_start_ns: start,
            long_weight: "0.5".parse().unwrap(),
            target_ttl_seconds: 3600,
            observations: Some(NativeIndicatorObservationsConfigV1 {
                atr_period: 4,
                stochastics_k_period: 5,
                stochastics_d_period: 3,
                donchian_period: 6,
            }),
        },
    )
}

#[test]
fn actor_registered_indicators_warm_and_trade_without_a_wasm_model() {
    let (directory, request) = request(20);
    let result = simulate_indicator_strategy(directory.path(), &request).unwrap();
    assert_eq!(result.simulation.consumed_target_points.get(), 18);
    assert!(result.simulation.orders.get() > 0);
    assert_eq!(result.observations.len(), 20);
    assert!(result.observations[0].atr.is_none());
    assert!(result.observations[0].donchian_upper.is_none());
    assert!(result.observations[0].vwap.is_some());
    assert!(result.observations[5].donchian_upper.is_some());
    assert!(result.observations[5].atr.unwrap() > 0.0);
    assert!(
        result
            .observations
            .iter()
            .all(|row| row.available_ns >= row.event_ns)
    );
}

#[test]
fn observation_warmup_never_delays_signal_and_future_does_not_change_prefix() {
    let (directory, mut request) = request(20);
    request.observations.as_mut().unwrap().donchian_period = 100;
    let full = simulate_indicator_strategy(directory.path(), &request).unwrap();
    assert_eq!(full.simulation.consumed_target_points.get(), 18);
    assert!(
        full.observations
            .iter()
            .all(|row| row.donchian_upper.is_none())
    );
    request.selection.event_end_ns = market::count(11 * market::INTERVAL_NS);
    request.selection.decision_cutoff_ns = request.selection.event_end_ns;
    let prefix = simulate_indicator_strategy(directory.path(), &request).unwrap();
    for (a, b) in prefix.observations.iter().zip(&full.observations) {
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }
}

#[test]
fn rsi_macd_and_insufficient_warmup_have_explicit_outcomes() {
    let (directory, mut request) = request(20);
    request.signal = NativeIndicatorSignalV1::Rsi {
        period: 3,
        entry_below: 30.0,
        exit_above: 70.0,
    };
    let rsi = simulate_indicator_strategy(directory.path(), &request).unwrap();
    assert!(rsi.simulation.consumed_target_points.get() > 0);
    assert_eq!(rsi.simulation.orders.get(), 0); // rising catalog never enters oversold
    request.signal = NativeIndicatorSignalV1::Macd {
        fast_period: 2,
        slow_period: 3,
        signal_period: 2,
    };
    assert!(
        simulate_indicator_strategy(directory.path(), &request)
            .unwrap()
            .simulation
            .consumed_target_points
            .get()
            > 0
    );
    request.signal = NativeIndicatorSignalV1::EmaCross {
        fast_period: 2,
        slow_period: 100,
    };
    assert!(
        simulate_indicator_strategy(directory.path(), &request)
            .unwrap_err()
            .to_string()
            .contains("NO_READY_DECISIONS")
    );
    request.signal = NativeIndicatorSignalV1::Rsi {
        period: 0,
        entry_below: 30.0,
        exit_above: 70.0,
    };
    assert!(
        simulate_indicator_strategy(directory.path(), &request)
            .unwrap_err()
            .to_string()
            .contains("PERIOD_INVALID")
    );
}

#[test]
fn observation_values_match_upstream_without_double_updating() {
    use nautilus_indicators::{
        average::vwap::VolumeWeightedAveragePrice,
        indicator::Indicator,
        momentum::stochastics::Stochastics,
        volatility::{atr::AverageTrueRange, dc::DonchianChannel},
    };
    let (directory, request) = request(20);
    let result = simulate_indicator_strategy(directory.path(), &request).unwrap();
    let data = job::catalog::load_catalog(directory.path(), &request.selection).unwrap();
    let mut atr = AverageTrueRange::new(4, None, Some(true), None);
    let mut vwap = VolumeWeightedAveragePrice::new();
    let mut stoch = Stochastics::new(5, 3);
    let mut channel = DonchianChannel::new(6);
    for (bar, observed) in data.series[0].bars.iter().zip(result.observations) {
        atr.handle_bar(bar);
        vwap.handle_bar(bar);
        stoch.handle_bar(bar);
        channel.handle_bar(bar);
        assert_eq!(observed.atr, atr.initialized().then_some(atr.value));
        assert_eq!(observed.vwap, vwap.initialized().then_some(vwap.value));
        assert_eq!(
            observed.stochastics_k,
            stoch.initialized().then_some(stoch.value_k)
        );
        assert_eq!(
            observed.stochastics_d,
            stoch.initialized().then_some(stoch.value_d)
        );
        assert_eq!(
            observed.donchian_upper,
            channel.initialized().then_some(channel.upper)
        );
        assert_eq!(
            observed.donchian_middle,
            channel.initialized().then_some(channel.middle)
        );
        assert_eq!(
            observed.donchian_lower,
            channel.initialized().then_some(channel.lower)
        );
    }
}

#[test]
fn orders_slower_than_bars_skip_unsettled_decisions_without_queuing_stale_targets() {
    let (directory, mut request) = request(20);
    request.settings.latency_model = market::execution_models::latency(150_000_000_000);
    request.target_ttl_seconds = 3600;
    let result = simulate_indicator_strategy(directory.path(), &request).unwrap();
    assert_eq!(result.decision_policy, "BATCH_LAST_SETTLED_ACCOUNT_ONLY");
    assert!(result.skipped_unsettled_decisions.get() > 0);
    assert!(result.simulation.consumed_target_points.get() > 0);
    assert!(result.simulation.consumed_target_points.get() < 18);
    assert_eq!(result.simulation.summary["orders.open"], "0");
    assert_eq!(result.simulation.summary["orders.inflight"], "0");
    assert_eq!(result.observations.len(), 20); // Indicators still receive every BAR.
    let batch = job::simulation::simulate_indicator_strategy_batch(
        directory.path(),
        &NativeIndicatorStrategyBatchRequestV1 {
            schema_version: SchemaV1,
            requests: vec![request.clone(), request],
        },
    )
    .unwrap();
    for actual in batch.results {
        assert_eq!(
            actual.simulation.canonical_result,
            result.simulation.canonical_result
        );
        assert_eq!(
            actual.skipped_unsettled_decisions,
            result.skipped_unsettled_decisions
        );
        assert_eq!(
            serde_json::to_value(actual.observations).unwrap(),
            serde_json::to_value(&result.observations).unwrap()
        );
    }
}

#[test]
fn same_availability_batch_updates_all_indicators_and_decides_once_at_batch_end() {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    let (original, request) = request(20);
    let mut market = job::catalog::load_catalog(original.path(), &request.selection).unwrap();
    let series = &mut market.series[0];
    let expected_at_batch_end = series.bars[3].close.as_f64();
    series.bars[2].ts_init = series.bars[3].ts_init; // Different events, one availability batch.
    series.bars[18].ts_init = series.bars[19].ts_init; // Final batch must not shut down after its first row.
    let directory = tempfile::tempdir().unwrap();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    catalog
        .write_instruments(vec![series.instrument.clone()])
        .unwrap();
    catalog
        .write_to_parquet(&series.bars, None, None, None)
        .unwrap();
    std::fs::copy(
        original
            .path()
            .join(contracts::catalogs::MAKER_TAKER_FEES_FILE),
        directory
            .path()
            .join(contracts::catalogs::MAKER_TAKER_FEES_FILE),
    )
    .unwrap();
    let result = simulate_indicator_strategy(directory.path(), &request).unwrap();
    assert_eq!(result.merged_same_time_bars.get(), 2);
    assert_eq!(result.simulation.consumed_target_points.get(), 16);
    assert_eq!(
        result.observations[2].available_ns,
        result.observations[3].available_ns
    );
    assert_ne!(
        result.observations[2].event_ns,
        result.observations[3].event_ns
    );
    assert!(result.observations[3].vwap.unwrap() < expected_at_batch_end);
    assert_eq!(result.observations.len(), 20);
}

#[test]
fn performance_scope_explicitly_keeps_pre_decision_cash_across_utc_days() {
    let (directory, mut request) = request(2 * 1440 + 10);
    request.decision_start_ns = market::count(2 * 1440 * market::INTERVAL_NS + 1);
    request.long_weight = "0".parse().unwrap();
    request.observations = None;
    let result = simulate_indicator_strategy(directory.path(), &request).unwrap();
    assert_eq!(
        result.performance_scope,
        "FULL_SELECTION_INCLUDING_PRE_DECISION_CASH"
    );
    assert!(
        result
            .simulation
            .returns
            .iter()
            .any(|row| row.timestamp_ns < request.decision_start_ns)
    );
    assert_eq!(result.simulation.orders.get(), 0);
    assert!(
        result
            .simulation
            .returns
            .iter()
            .all(|row| row.value == Some(0.0))
    );
}

fn same_time_request() -> (tempfile::TempDir, NativeIndicatorStrategyRequestV1) {
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
    let (original, first) = request(40);
    let mut market = job::catalog::load_catalog(original.path(), &first.selection).unwrap();
    let series = &mut market.series[0];
    series.bars[2].ts_init = series.bars[3].ts_init;
    series.bars[38].ts_init = series.bars[39].ts_init;
    let directory = tempfile::tempdir().unwrap();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    catalog
        .write_instruments(vec![series.instrument.clone()])
        .unwrap();
    catalog
        .write_to_parquet(&series.bars, None, None, None)
        .unwrap();
    std::fs::copy(
        original
            .path()
            .join(contracts::catalogs::MAKER_TAKER_FEES_FILE),
        directory
            .path()
            .join(contracts::catalogs::MAKER_TAKER_FEES_FILE),
    )
    .unwrap();
    (directory, first)
}

#[test]
fn reset_batch_matches_independent_runs_including_native_orders_fills_balances_and_indicators() {
    let (directory, mut first) = same_time_request();
    if let contracts::portfolio::NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
        &mut first.settings.fill_model
    {
        parameters.prob_slippage = "0.5".parse().unwrap();
    } else {
        panic!("native fill model required");
    }
    let mut second = first.clone();
    second.signal = NativeIndicatorSignalV1::Rsi {
        period: 3,
        entry_below: 99.0,
        exit_above: 100.0,
    };
    second.long_weight = "0.25".parse().unwrap();
    let requests = vec![first.clone(), second, first];
    let expected: Vec<_> = requests
        .iter()
        .map(|request| simulate_indicator_strategy(directory.path(), request).unwrap())
        .collect();
    let actual = job::simulation::simulate_indicator_strategy_batch(
        directory.path(),
        &NativeIndicatorStrategyBatchRequestV1 {
            schema_version: SchemaV1,
            requests,
        },
    )
    .unwrap();
    assert_eq!(actual.reuse, "NAUTILUS_ENGINE_RESET");
    assert_eq!(actual.results.len(), expected.len());
    for (index, (actual, expected)) in actual.results.iter().zip(expected).enumerate() {
        assert_eq!(actual.merged_same_time_bars, expected.merged_same_time_bars);
        assert_eq!(actual.merged_same_time_bars.get(), 2);
        assert_eq!(
            actual.skipped_unsettled_decisions,
            expected.skipped_unsettled_decisions
        );
        // Official canonical output already removes random IDs/wall-clock noise;
        // compare the complete native execution state, not selected headline metrics.
        assert_eq!(
            actual.simulation.canonical_result, expected.simulation.canonical_result,
            "canonical run {index}"
        );
        assert_eq!(
            serde_json::to_value(&actual.simulation.statistics).unwrap(),
            serde_json::to_value(&expected.simulation.statistics).unwrap(),
            "statistics run {index}"
        );
        assert_eq!(
            serde_json::to_value(&actual.observations).unwrap(),
            serde_json::to_value(&expected.observations).unwrap(),
            "indicators run {index}"
        );
    }
}

#[test]
fn reset_batch_keeps_frozen_fee_expiry_failure_and_never_publishes_a_successful_prefix() {
    // Preserve the original failing replay exactly: 40 BARs, repeated ts_init,
    // 150-second insert latency, and the unchanged original fee source window.
    // Its standalone run already fails before any reset is possible.
    let (directory, mut first) = same_time_request();
    first.settings.latency_model = market::execution_models::latency(150_000_000_000);
    first.target_ttl_seconds = 3600;
    if let contracts::portfolio::NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
        &mut first.settings.fill_model
    {
        parameters.prob_slippage = "0.5".parse().unwrap();
    }
    let mut second = first.clone();
    second.signal = NativeIndicatorSignalV1::Rsi {
        period: 3,
        entry_below: 99.0,
        exit_above: 100.0,
    };
    second.long_weight = "0.25".parse().unwrap();
    let cases = [first.clone(), second, first.clone()];
    let failing = cases
        .iter()
        .find(|request| simulate_indicator_strategy(directory.path(), request).is_err())
        .expect("the unchanged long-latency replay must expose its frozen fee expiry");
    let standalone = simulate_indicator_strategy(directory.path(), failing).unwrap_err();
    assert!(format!("{standalone:#}").contains("MAKER_TAKER_FEE_FILL_OUTSIDE_SOURCE_WINDOW"));
    // A real successful first case forces the failing case to exercise reset.
    let mut prefix = first;
    prefix.long_weight = "0".parse().unwrap();
    assert!(simulate_indicator_strategy(directory.path(), &prefix).is_ok());
    let batch = NativeIndicatorStrategyBatchRequestV1 {
        schema_version: SchemaV1,
        requests: vec![prefix, failing.clone()],
    };
    let error =
        job::simulation::simulate_indicator_strategy_batch(directory.path(), &batch).unwrap_err();
    assert_eq!(format!("{error:#}"), format!("{standalone:#}"));
    let cli = native::command(
        &[
            "simulate-indicator-strategy-batch".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
        ],
        &batch,
    );
    assert!(!cli.status.success());
    assert!(
        cli.stdout.is_empty(),
        "fee failure must not publish its successful prefix"
    );
}

#[test]
fn batch_rejects_changed_selection_or_settings_before_loading_data() {
    let (_directory, first) = request(10);
    let mut different = first.clone();
    different.settings.starting_capital = "1".parse().unwrap();
    let error = job::simulation::simulate_indicator_strategy_batch(
        std::path::Path::new("/nonexistent"),
        &NativeIndicatorStrategyBatchRequestV1 {
            schema_version: SchemaV1,
            requests: vec![first.clone(), different],
        },
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "INDICATOR_BATCH_REQUIRES_SAME_SELECTION_AND_SETTINGS"
    );
    let mut different = first.clone();
    different.selection.event_end_ns =
        contracts::DbCounter::new(first.selection.event_end_ns.get() + 1).unwrap();
    let error = job::simulation::simulate_indicator_strategy_batch(
        std::path::Path::new("/nonexistent"),
        &NativeIndicatorStrategyBatchRequestV1 {
            schema_version: SchemaV1,
            requests: vec![first, different],
        },
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "INDICATOR_BATCH_REQUIRES_SAME_SELECTION_AND_SETTINGS"
    );
}

#[test]
fn batch_cli_does_not_publish_a_successful_prefix_when_a_later_case_fails() {
    let (directory, first) = request(12);
    let mut invalid = first.clone();
    invalid.signal = NativeIndicatorSignalV1::EmaCross {
        fast_period: 0,
        slow_period: 3,
    };
    let result = native::command(
        &[
            "simulate-indicator-strategy-batch".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
        ],
        &NativeIndicatorStrategyBatchRequestV1 {
            schema_version: SchemaV1,
            requests: vec![first, invalid],
        },
    );
    assert!(!result.status.success());
    assert!(
        result.stdout.is_empty(),
        "a failed batch cannot publish its completed prefix"
    );
}
