//! Synthetic native Parquet fixtures exercising the real upstream strategy/engine.
//! These are regression evidence, not historical venue or performance claims.
use contracts::native_strategy::NativeStrategyBacktestRequestV1;
use nautilus_model::{
    data::{BookOrder, OrderBookDelta, TradeTick},
    enums::{AggressorSide, BookAction, OrderSide, RecordFlag},
    identifiers::{InstrumentId, TradeId},
    instruments::{InstrumentAny, stubs::currency_pair_btcusdt},
    types::{Price, Quantity},
};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use serde_json::json;
const SECOND: u64 = 1_000_000_000;
fn id() -> InstrumentId {
    "BTCUSDT.BINANCE".parse().unwrap()
}
fn request(end: u64) -> NativeStrategyBacktestRequestV1 {
    serde_json::from_value(json!({"schema_version":1,"instrument_id":id().to_string(),
      "selectors":[{"data_type":"OrderBookDelta","identifier":id().to_string()}],
      "event_start_ns":"1000000000","event_end_ns":"10000000000","available_end_ns":end.to_string(),
      "venue":{"account_type":"CASH","oms_type":"NETTING","book_type":"L2_MBP","starting_balances":["100000 USDT","1 BTC"],"base_currency":null,"leverage":"1","maker_fee":"0","taker_fee":"0"},
      "strategy":{"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"0.010000","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":false}
    })).unwrap()
}
fn snapshot(event: u64, available: u64, bid: &str, ask: &str) -> Vec<OrderBookDelta> {
    let mut clear = OrderBookDelta::clear(id(), 0, event.into(), available.into());
    clear.order.price = Price::from("0.00");
    clear.order.size = Quantity::from("0.000000");
    vec![
        clear,
        OrderBookDelta::new(
            id(),
            BookAction::Add,
            BookOrder::new(
                OrderSide::Buy,
                Price::from("100.00"),
                Quantity::from(bid),
                0,
            ),
            RecordFlag::F_SNAPSHOT as u8,
            0,
            event.into(),
            available.into(),
        ),
        OrderBookDelta::new(
            id(),
            BookAction::Add,
            BookOrder::new(
                OrderSide::Sell,
                Price::from("101.00"),
                Quantity::from(ask),
                0,
            ),
            RecordFlag::F_SNAPSHOT as u8 | RecordFlag::F_LAST as u8,
            0,
            event.into(),
            available.into(),
        ),
    ]
}
fn catalog(books: &[OrderBookDelta], trades: &[TradeTick]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(4096),
        None,
        None,
    )
    .unwrap();
    let instrument = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
    job::catalog::currencies::write(directory.path(), std::slice::from_ref(&instrument)).unwrap();
    catalog.write_instruments(vec![instrument]).unwrap();
    catalog.write_to_parquet(books, None, None, None).unwrap();
    if !trades.is_empty() {
        catalog.write_to_parquet(trades, None, None, None).unwrap();
    }
    directory
}
#[test]
fn real_l2_strategy_trades_and_explicit_native_fees_change_fills() {
    let mut books = snapshot(SECOND, 2 * SECOND, "10.000000", "0.100000");
    books.extend(snapshot(3 * SECOND, 4 * SECOND, "10.000000", "0.100000"));
    let dir = catalog(&books, &[]);
    let free = job::native_strategy::run(dir.path(), request(5 * SECOND)).unwrap();
    assert_eq!(free.data_rows_by_type["OrderBookDelta"].get(), 6);
    assert!(
        !free.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{}",
        free.canonical_result
    );
    let mut paid_request = request(5 * SECOND);
    paid_request.venue.taker_fee = "0.001".into();
    let mut base =
        serde_json::to_value(nautilus_trading::strategy::StrategyConfig::default()).unwrap();
    base["strategy_id"] = json!("CustomBook-042");
    base["order_id_tag"] = json!("042");
    if let contracts::native_strategy::NativeCompiledStrategyV1::OrderBookImbalance {
        base: value,
        ..
    } = &mut paid_request.strategy
    {
        *value = Some(base);
    }
    let paid = job::native_strategy::run(dir.path(), paid_request).unwrap();
    assert_ne!(
        free.canonical_result["fills"],
        paid.canonical_result["fills"]
    );
    for row in paid.canonical_result["fills"].as_array().unwrap() {
        use std::str::FromStr;
        let fill = &row["event"]["Filled"];
        let quantity = rust_decimal::Decimal::from_str(fill["last_qty"].as_str().unwrap()).unwrap();
        let price = rust_decimal::Decimal::from_str(fill["last_px"].as_str().unwrap()).unwrap();
        let commission =
            nautilus_model::types::Money::from_str(fill["commission"].as_str().unwrap()).unwrap();
        assert_eq!(fill["liquidity_side"], "TAKER");
        assert_eq!(fill["strategy_id"], "CustomBook-042");
        assert_eq!(
            commission.as_decimal(),
            (quantity * price * rust_decimal::Decimal::new(1, 3))
                .round_dp(commission.currency.precision as u32)
        );
        assert!(commission.as_decimal() > rust_decimal::Decimal::ZERO);
    }
}
#[test]
fn delayed_snapshot_does_not_leak_before_availability_and_tick_selection_keeps_both_clocks() {
    let mut books = snapshot(SECOND, 2 * SECOND, "1.000000", "1.000000");
    books.extend(snapshot(SECOND + 1, 8 * SECOND, "10.000000", "0.100000"));
    let tick = |event: u64, available: u64, label: &str| {
        TradeTick::new(
            id(),
            Price::from("100.00"),
            Quantity::from("0.010000"),
            AggressorSide::Buy,
            TradeId::from(label),
            event.into(),
            available.into(),
        )
    };
    let trades = vec![
        tick(SECOND, 2 * SECOND, "one"),
        tick(SECOND + 1, 8 * SECOND, "late"),
    ];
    let dir = catalog(&books, &trades);
    let mut early = request(3 * SECOND);
    early.selectors.push(
        serde_json::from_value(json!({"data_type":"TradeTick","identifier":id().to_string()}))
            .unwrap(),
    );
    let before = job::native_strategy::run(dir.path(), early.clone()).unwrap();
    assert_eq!(before.data_rows_by_type["OrderBookDelta"].get(), 3);
    assert_eq!(before.data_rows_by_type["TradeTick"].get(), 1);
    assert!(
        before.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    early.available_end_ns = contracts::DbCounter::new(9 * SECOND).unwrap();
    let after = job::native_strategy::run(dir.path(), early).unwrap();
    assert_eq!(after.data_rows_by_type["OrderBookDelta"].get(), 6);
    assert_eq!(after.data_rows_by_type["TradeTick"].get(), 2);
    assert!(
        !after.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn incomplete_native_book_batch_is_rejected_not_silently_replayed() {
    let mut books = snapshot(SECOND, 2 * SECOND, "10.000000", "0.100000");
    books.last_mut().unwrap().flags &= !(RecordFlag::F_LAST as u8);
    let dir = catalog(&books, &[]);
    let error = job::native_strategy::run(dir.path(), request(3 * SECOND)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("NATIVE_BOOK_INCOMPLETE_EVENT_BATCH"),
        "{error:#}"
    );
}

#[test]
fn native_perpetual_mark_funding_changes_real_account_balance() {
    use nautilus_model::{
        data::{FundingRateUpdate, MarkPriceUpdate},
        instruments::{Instrument, stubs::crypto_perpetual_ethusdt},
    };
    let directory = tempfile::tempdir().unwrap();
    let native = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(4096),
        None,
        None,
    )
    .unwrap();
    let instrument = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt());
    let perp = instrument.id();
    job::catalog::currencies::write(directory.path(), std::slice::from_ref(&instrument)).unwrap();
    native.write_instruments(vec![instrument]).unwrap();
    let books: Vec<_> = snapshot(SECOND, 2 * SECOND, "10.000000", "1.000000")
        .into_iter()
        .map(|mut row| {
            row.instrument_id = perp;
            row.order.size = Quantity::new(row.order.size.as_f64(), 3);
            row
        })
        .collect();
    native.write_to_parquet(&books, None, None, None).unwrap();
    native
        .write_to_parquet(
            &[MarkPriceUpdate::new(
                perp,
                Price::from("100.00"),
                (3 * SECOND).into(),
                (3 * SECOND).into(),
            )],
            None,
            None,
            None,
        )
        .unwrap();
    native
        .write_to_parquet(
            &[FundingRateUpdate::new(
                perp,
                "0.01".parse().unwrap(),
                None,
                Some((4 * SECOND).into()),
                (4 * SECOND).into(),
                (4 * SECOND).into(),
            )],
            None,
            None,
            None,
        )
        .unwrap();
    let mut config = request(5 * SECOND);
    config.instrument_id = perp.to_string();
    config.selectors[0].identifier = perp.to_string();
    config.venue.account_type = "MARGIN".into();
    config.venue.starting_balances = vec!["100000 USDT".into()];
    config.strategy = serde_json::from_value(json!({"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"1.000","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":false})).unwrap();
    config.selectors.push(
        serde_json::from_value(
            json!({"data_type":"MarkPriceUpdate","identifier":perp.to_string()}),
        )
        .unwrap(),
    );
    let baseline = job::native_strategy::run(directory.path(), config.clone()).unwrap();
    assert!(
        !baseline.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    config.selectors.push(
        serde_json::from_value(
            json!({"data_type":"FundingRateUpdate","identifier":perp.to_string()}),
        )
        .unwrap(),
    );
    let funded = job::native_strategy::run(directory.path(), config).unwrap();
    assert_eq!(funded.data_rows_by_type["FundingRateUpdate"].get(), 1);
    assert_ne!(
        funded.canonical_result["accounts"], baseline.canonical_result["accounts"],
        "funding must reach a real open position, not just be counted"
    );
    // One long ETH * the actual 100 USDT mark * the explicit 1% settlement rate.
    assert_eq!(
        baseline.canonical_result["accounts"][0]["Margin"]["base"]["balances"]["USDT"]["total"],
        "100000.00000000 USDT"
    );
    assert_eq!(
        funded.canonical_result["accounts"][0]["Margin"]["base"]["balances"]["USDT"]["total"],
        "99999.00000000 USDT"
    );
}

#[test]
fn upstream_ema_uses_real_quotes_and_respects_future_availability() {
    use nautilus_model::data::QuoteTick;
    let dir = catalog(&snapshot(SECOND, 2 * SECOND, "1.000000", "1.000000"), &[]);
    let native =
        ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, Some(4096), None, None)
            .unwrap();
    let quotes: Vec<_> = [100.0, 101.0, 102.0, 99.0, 97.0, 103.0, 105.0, 96.0]
        .iter()
        .enumerate()
        .map(|(index, bid)| {
            QuoteTick::new(
                id(),
                Price::new(*bid, 2),
                Price::new(bid + 1.0, 2),
                Quantity::from("1.000000"),
                Quantity::from("1.000000"),
                ((index as u64 + 1) * SECOND).into(),
                ((index as u64 + 1) * SECOND + SECOND / 2).into(),
            )
        })
        .collect();
    native.write_to_parquet(&quotes, None, None, None).unwrap();
    let mut config = request(9 * SECOND);
    config.selectors[0].data_type = "QuoteTick".into();
    config.venue.book_type = "L1_MBP".into();
    config.strategy = serde_json::from_value(
        json!({"kind":"EMA_CROSS","trade_size":"0.010000","fast_period":2,"slow_period":3}),
    )
    .unwrap();
    let result = job::native_strategy::run(dir.path(), config.clone()).unwrap();
    assert_eq!(result.data_rows_by_type["QuoteTick"].get(), 8);
    assert!(
        !result.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    config.available_end_ns = contracts::DbCounter::new(3 * SECOND).unwrap();
    let prefix = job::native_strategy::run(dir.path(), config).unwrap();
    assert_eq!(prefix.data_rows_by_type["QuoteTick"].get(), 2);
    assert!(
        prefix.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[path = "support/command.rs"]
mod native_cli;
#[test]
fn actual_l2_cli_reserves_stdout_for_one_json_result() {
    let books = snapshot(SECOND, 2 * SECOND, "10.000000", "0.100000");
    let dir = catalog(&books, &[]);
    let request = request(5 * SECOND);
    let output = native_cli::command(
        &[
            "native-strategy-backtest".as_ref(),
            "--catalog".as_ref(),
            dir.path().as_os_str(),
        ],
        &request,
    );
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("summary:"),
        "upstream output must remain available on stderr"
    );
    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "invalid CLI JSON: {error}; stdout={}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
    assert_eq!(result["request"], serde_json::to_value(request).unwrap());
    assert!(!result["canonical_result"]["fills"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn sparse_funding_and_close_streams_are_legal_with_actual_market_events() {
    let books = snapshot(SECOND, 2 * SECOND, "10.000000", "0.100000");
    let dir = catalog(&books, &[]);
    let mut request = request(5 * SECOND);
    for kind in ["FundingRateUpdate", "InstrumentClose"] {
        request.selectors.push(
            serde_json::from_value(json!({"data_type":kind,"identifier":id().to_string()}))
                .unwrap(),
        );
    }
    let result = job::native_strategy::run(dir.path(), request).unwrap();
    assert_eq!(result.data_rows_by_type["FundingRateUpdate"].get(), 0);
    assert_eq!(result.data_rows_by_type["InstrumentClose"].get(), 0);
    assert!(!result.canonical_result["fills"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn actual_l2_walk_forward_cli_keeps_every_native_summary_on_stderr() {
    let books = (1..=8).flat_map(|n| snapshot(n * SECOND, n * SECOND, "10.000000", "0.100000")).collect::<Vec<_>>();
    let dir = catalog(&books, &[]);
    let request = json!({
        "schema_version":1, "candidates":[request(8 * SECOND)],
        "split":{"n_splits":2,"test_size":2,"gap":0,"max_train_size":null},
        "objective":{"native_key":"PnL (total)","currency":"USDT","direction":"MAXIMIZE"}
    });
    let output = native_cli::command(&["native-walk-forward".as_ref(), "--catalog".as_ref(), dir.path().as_os_str()], &request);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("all upstream summaries must stay outside JSON stdout");
    assert_eq!(result["folds"].as_array().unwrap().len(), 2);
    assert!(result["folds"].as_array().unwrap().iter().all(|fold| fold["trials"][0]["result"].is_object()));
    assert!(String::from_utf8_lossy(&output.stderr).contains("summary:"));
}

#[test]
fn a_completely_empty_replay_is_not_a_computed_backtest() {
    let dir = catalog(&snapshot(3 * SECOND, 3 * SECOND, "10.000000", "0.100000"), &[]);
    let error = job::native_strategy::run(dir.path(), request(2 * SECOND)).unwrap_err();
    assert!(error.to_string().contains("NATIVE_STRATEGY_EMPTY_DATA"));
}

#[test]
fn retired_custom_research_commands_are_not_executable() {
    for command in ["simulate-indicator-strategy", "simulate-indicator-strategy-batch", "forecast", "forecast-factor", "allocate", "study-portfolio", "build-observed-spot", "study-observed-spot"] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_job"))
            .args([command, "--help"]).output().unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("unrecognized subcommand"));
    }
}

#[test]
fn official_hourly_interval_settles_without_invented_next_funding_timestamp() {
    use nautilus_model::{
        data::{FundingRateUpdate, MarkPriceUpdate},
        instruments::{Instrument, stubs::crypto_perpetual_ethusdt},
    };
    let directory = tempfile::tempdir().unwrap();
    let native = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(4096),
        None,
        None,
    )
    .unwrap();
    let instrument = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt());
    let perp = instrument.id();
    job::catalog::currencies::write(directory.path(), std::slice::from_ref(&instrument)).unwrap();
    native.write_instruments(vec![instrument]).unwrap();
    let books: Vec<_> = snapshot(SECOND, 2 * SECOND, "10.000000", "1.000000")
        .into_iter()
        .map(|mut row| {
            row.instrument_id = perp;
            row.order.size = Quantity::new(row.order.size.as_f64(), 3);
            row
        })
        .collect();
    native.write_to_parquet(&books, None, None, None).unwrap();
    native
        .write_to_parquet(
            &[MarkPriceUpdate::new(
                perp,
                Price::from("100.00"),
                (3599 * SECOND).into(),
                (3599 * SECOND).into(),
            )],
            None,
            None,
            None,
        )
        .unwrap();
    native
        .write_to_parquet(
            &[FundingRateUpdate::new(
                perp,
                "0.01".parse().unwrap(),
                Some(60),
                None,
                (3600 * SECOND).into(),
                (3600 * SECOND).into(),
            )],
            None,
            None,
            None,
        )
        .unwrap();
    let mut config = request(3601 * SECOND);
    config.event_end_ns = contracts::DbCounter::new(3601 * SECOND).unwrap();
    config.instrument_id = perp.to_string();
    config.selectors[0].identifier = perp.to_string();
    config.venue.account_type = "MARGIN".into();
    config.venue.starting_balances = vec!["100000 USDT".into()];
    config.strategy = serde_json::from_value(json!({"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"1.000","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":0.0,"time_in_force":"IOC","dry_run":false})).unwrap();
    config.selectors.push(
        serde_json::from_value(
            json!({"data_type":"MarkPriceUpdate","identifier":perp.to_string()}),
        )
        .unwrap(),
    );
    let baseline = job::native_strategy::run(directory.path(), config.clone()).unwrap();
    assert!(
        !baseline.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    config.selectors.push(
        serde_json::from_value(
            json!({"data_type":"FundingRateUpdate","identifier":perp.to_string()}),
        )
        .unwrap(),
    );
    let funded = job::native_strategy::run(directory.path(), config).unwrap();
    assert_eq!(funded.data_rows_by_type["FundingRateUpdate"].get(), 1);
    assert_ne!(
        funded.canonical_result["accounts"], baseline.canonical_result["accounts"],
        "funding must reach a real open position, not just be counted"
    );
    // One long ETH * the actual 100 USDT mark * the explicit 1% settlement rate.
    assert_eq!(
        baseline.canonical_result["accounts"][0]["Margin"]["base"]["balances"]["USDT"]["total"],
        "100000.00000000 USDT"
    );
    assert_eq!(
        funded.canonical_result["accounts"][0]["Margin"]["base"]["balances"]["USDT"]["total"],
        "99999.00000000 USDT"
    );
}

#[test]
fn catalog_inspect_discovers_real_l2_files_without_modifying_catalog() {
    let books = snapshot(SECOND, 2 * SECOND, "10.000000", "0.100000");
    let dir = catalog(&books, &[]);
    let exact_time = 1_790_000_000_000_000_001_u64;
    let original = serde_json::to_value(InstrumentAny::CurrencyPair(currency_pair_btcusdt())).unwrap();
    let mut updated = original.clone();
    updated["CurrencyPair"]["ts_init"] = exact_time.into();
    updated["CurrencyPair"]["ts_event"] = (exact_time - 1).into();
    let native = ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, None, None, None).unwrap();
    native.write_instruments(vec![serde_json::from_value::<InstrumentAny>(updated.clone()).unwrap()]).unwrap();
    let files = || {
        let native =
            ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, None, None, None)
                .unwrap();
        let mut paths = native.list_parquet_files("data").unwrap();
        paths.sort();
        paths
            .into_iter()
            .map(|p| {
                let bytes = std::fs::read(dir.path().join(&p)).unwrap();
                (p, bytes)
            })
            .collect::<Vec<_>>()
    };
    let before = files();
    let result = job::catalog::inspect::query(
        dir.path(),
        contracts::catalog_coverage::NativeCatalogInspectRequestV1::default(),
    )
    .unwrap();
    assert_eq!(before, files());
    // Exercise the actual CLI dispatch as well as the projection function.
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_job"))
        .args(["catalog-inspect", "--catalog", dir.path().to_str().unwrap()])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    std::io::Write::write_all(child.stdin.take().as_mut().unwrap(), b"{}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(), serde_json::to_value(&result).unwrap());
    assert_eq!(before, files());
    assert_eq!(result.instruments.len(), 2);
    assert!(result.instruments.iter().any(|row| row.instrument == original));
    let version = result.instruments.iter().find(|row| row.instrument == updated).unwrap();
    assert_eq!(serde_json::to_value(version).unwrap()["ts_init_ns"], exact_time.to_string());
    assert_eq!(result.selectors.len(), 1);
    assert_eq!(result.selectors[0].data_type, "OrderBookDelta");
    assert_eq!(result.selectors[0].identifier, id().to_string());
    assert_eq!(
        result.selectors[0].file_intervals[0].start_ns.get(),
        2 * SECOND
    );
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(
        json["selectors"][0]["file_intervals"][0]["end_ns"],
        "2000000000"
    );
    assert_eq!(
        json["coverage_basis"],
        "native_file_intervals_not_event_completeness"
    );
    assert!(
        serde_json::from_value::<contracts::catalog_coverage::NativeCatalogInspectRequestV1>(
            json!({"path":"/other"})
        )
        .is_err()
    );
}

#[test]
fn catalog_inspect_pages_every_interval_and_rejects_changed_filters() {
    use contracts::catalog_coverage::NativeCatalogInspectRequestV1;
    let exact = 1_790_000_000_000_000_001_u64;
    let dir = catalog(&snapshot(exact - 1, exact, "1.000000", "2.000000"), &[]);
    let native = ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, None, None, None).unwrap();
    for offset in [10, 20] {
        native.write_to_parquet(&snapshot(exact + offset - 1, exact + offset, "1.000000", "2.000000"), None, None, None).unwrap();
    }
    let query = |value| job::catalog::inspect::query(dir.path(), serde_json::from_value::<NativeCatalogInspectRequestV1>(value).unwrap());
    let full = query(json!({"instrument_ids":[id().to_string()],"data_types":["OrderBookDelta"],"page_size":100})).unwrap();
    assert_eq!(full.selectors[0].file_intervals.len(), 3);
    let mut page = query(json!({"instrument_ids":[id().to_string()],"data_types":["OrderBookDelta"],"page_size":1})).unwrap();
    let first_cursor = page.next_cursor.clone().unwrap();
    let mut definitions = Vec::new();
    let mut intervals = Vec::new();
    loop {
        assert_eq!(page.page.returned_items.get(), 1);
        definitions.extend(page.instruments);
        for series in page.selectors { assert_eq!(series.identifier, id().to_string()); intervals.extend(series.file_intervals); }
        match page.next_cursor { Some(cursor) => page = query(json!({"cursor":cursor,"page_size":1})).unwrap(), None => break }
    }
    assert_eq!(serde_json::to_value(definitions).unwrap(), serde_json::to_value(full.instruments).unwrap());
    assert_eq!(serde_json::to_value(intervals).unwrap(), serde_json::to_value(&full.selectors[0].file_intervals).unwrap());
    assert_eq!(full.selectors[0].file_intervals[0].start_ns.get(), exact);
    assert!(query(json!({"cursor":first_cursor,"instrument_ids":["OTHER.SIM"]})).unwrap_err().to_string().contains("FILTER_MISMATCH"));
    let absent = query(json!({"instrument_ids":["OTHER.SIM"]})).unwrap();
    assert!(absent.instruments.is_empty() && absent.selectors.is_empty() && absent.next_cursor.is_none());
    native.write_to_parquet(&snapshot(exact+29,exact+30,"1.000000","2.000000"),None,None,None).unwrap();
    assert!(query(json!({"cursor":first_cursor})).unwrap_err().to_string().contains("CATALOG_CHANGED"));
}

#[test]
fn upstream_grid_uses_real_quotes_and_preserves_all_config_fields() {
    use nautilus_model::data::QuoteTick;
    let dir = catalog(&snapshot(SECOND, 2 * SECOND, "1.000000", "1.000000"), &[]);
    let native =
        ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, Some(4096), None, None)
            .unwrap();
    let quotes: Vec<_> = [100.0, 101.0, 102.0, 99.0, 97.0, 103.0, 105.0, 96.0]
        .iter()
        .enumerate()
        .map(|(index, bid)| {
            QuoteTick::new(
                id(),
                Price::new(*bid, 2),
                Price::new(bid + 1.0, 2),
                Quantity::from("1.000000"),
                Quantity::from("1.000000"),
                ((index as u64 + 1) * SECOND).into(),
                ((index as u64 + 1) * SECOND + SECOND / 2).into(),
            )
        })
        .collect();
    native.write_to_parquet(&quotes, None, None, None).unwrap();
    let bars: Vec<_> = ["BTCUSDT.BINANCE-1-MINUTE-LAST-EXTERNAL", "BTCUSDT.BINANCE-2-MINUTE-LAST-EXTERNAL"].into_iter().map(|kind| {
        nautilus_model::data::Bar::new(kind.parse().unwrap(), Price::from("100.00"), Price::from("101.00"), Price::from("99.00"), Price::from("100.00"), Quantity::from("1.000000"), (2 * SECOND).into(), (2 * SECOND + SECOND / 2).into())
    }).collect();
    for bar in &bars { native.write_to_parquet(std::slice::from_ref(bar), None, None, None).unwrap(); }
    let mut config = request(9 * SECOND);
    config.selectors[0].data_type = "QuoteTick".into();
    for bar in &bars { config.selectors.push(contracts::catalog_coverage::NativeCatalogSelectorV1 { data_type: "Bar".into(), identifier: bar.bar_type.to_string() }); }
    config.venue.book_type = "L1_MBP".into();
    config.strategy = serde_json::from_value(
        json!({"kind":"GRID_MARKET_MAKER","trade_size":"0.010000","num_levels":2,"grid_step_bps":10,"skew_factor":0.1,"max_position":"1.000000","requote_threshold_bps":5,"expire_time_secs":2,"on_cancel_resubmit":true}),
    )
    .unwrap();
    let result = job::native_strategy::run(dir.path(), config.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&result.request.strategy).unwrap(),
        serde_json::to_value(&config.strategy).unwrap()
    );
    assert_eq!(result.data_rows_by_type["Bar"].get(), 2);
    let mut outside = config.clone();
    outside.selectors[1].identifier = "OTHER.BINANCE-1-MINUTE-LAST-EXTERNAL".into();
    assert!(job::native_strategy::run(dir.path(), outside).unwrap_err().to_string().contains("NATIVE_STRATEGY_SINGLE_INSTRUMENT_SELECTOR"));
    assert_eq!(result.data_rows_by_type["QuoteTick"].get(), 8);
    assert!(!result.canonical_result["fills"]
        .as_array()
        .unwrap()
        .is_empty());
    config.available_end_ns = contracts::DbCounter::new(3 * SECOND).unwrap();
    let prefix = job::native_strategy::run(dir.path(), config).unwrap();
    assert_eq!(prefix.data_rows_by_type["QuoteTick"].get(), 2);
    assert_eq!(prefix.request.available_end_ns.get(), 3 * SECOND);
}

#[test]
fn cli_static_scenario_keeps_future_definition_and_market_clocks_without_claiming_history() {
    use contracts::native_strategy::{
        NativeInstrumentDefinitionV1, NativeStrategyBacktestResultV1,
    };
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let dir = tempfile::tempdir().unwrap();
    let native =
        ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, None, None, None).unwrap();
    let mut raw =
        serde_json::to_value(InstrumentAny::CurrencyPair(currency_pair_btcusdt())).unwrap();
    let definition = raw.get_mut("CurrencyPair").unwrap();
    let init = 100 * SECOND + 11;
    let event = 100 * SECOND + 7;
    definition["ts_init"] = json!(init);
    definition["ts_event"] = json!(event);
    let instrument: InstrumentAny = serde_json::from_value(raw.clone()).unwrap();
    job::catalog::currencies::write(dir.path(), std::slice::from_ref(&instrument)).unwrap();
    native.write_instruments(vec![instrument]).unwrap();
    let mut books = snapshot(SECOND, 2 * SECOND, "10.000000", "0.100000");
    books.extend(snapshot(3 * SECOND, 4 * SECOND, "10.000000", "0.100000"));
    native.write_to_parquet(&books, None, None, None).unwrap();
    assert!(
        job::native_strategy::run(dir.path(), request(5 * SECOND))
            .unwrap_err()
            .to_string()
            .contains("INSTRUMENT_UNAVAILABLE_AT_START")
    );
    let mut scenario = request(5 * SECOND);
    scenario.instrument_definition = Some(NativeInstrumentDefinitionV1::StaticScenario {
        ts_init_ns: contracts::DbCounter::new(init).unwrap(),
        ts_event_ns: contracts::DbCounter::new(event).unwrap(),
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .args([
            "native-strategy-backtest",
            "--catalog",
            dir.path().to_str().unwrap(),
        ])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&scenario).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativeStrategyBacktestResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        serde_json::to_value(&result.request).unwrap(),
        serde_json::to_value(&scenario).unwrap()
    );
    assert_eq!(result.instrument_definition.mode, "STATIC_SCENARIO");
    assert_eq!(
        result.instrument_definition.scope,
        "HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED"
    );
    assert_eq!(result.instrument_definition.ts_init_ns.get(), init);
    assert_eq!(result.instrument_definition.ts_event_ns.get(), event);
    assert!(
        !result.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // No data export, synthetic early definition or changed original timestamp was written.
    let inspected = job::catalog::inspect::query(dir.path(), Default::default()).unwrap();
    assert_eq!(inspected.instruments.len(), 1);
    assert_eq!(inspected.instruments[0].instrument, raw);
    let book = inspected
        .selectors
        .iter()
        .find(|s| s.data_type == "OrderBookDelta")
        .unwrap();
    assert_eq!(book.file_intervals[0].start_ns.get(), 2 * SECOND);
    assert_eq!(book.file_intervals[0].end_ns.get(), 4 * SECOND);
    scenario.instrument_definition = Some(NativeInstrumentDefinitionV1::StaticScenario {
        ts_init_ns: contracts::DbCounter::new(init - 1).unwrap(),
        ts_event_ns: contracts::DbCounter::new(event).unwrap(),
    });
    assert!(
        job::native_strategy::run(dir.path(), scenario)
            .unwrap_err()
            .to_string()
            .contains("SCENARIO_DEFINITION_NOT_UNIQUE_OR_MISSING")
    );
}
