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
