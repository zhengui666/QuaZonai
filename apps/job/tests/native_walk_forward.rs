//! Synthetic native Parquet fixtures exercising the real upstream strategy/engine.
//! These are regression evidence, not historical venue or performance claims.
use contracts::native_strategy::NativeStrategyBacktestRequestV1;
use nautilus_model::{
    data::{BookOrder, OrderBookDelta, TradeTick},
    enums::{BookAction, OrderSide, RecordFlag},
    identifiers::InstrumentId,
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

fn walk_fixture(
    change_future: bool,
) -> (
    tempfile::TempDir,
    contracts::native_walk_forward::NativeStrategyWalkForwardRequestV1,
) {
    use contracts::native_walk_forward::*;
    use nautilus_model::data::QuoteTick;
    let dir = catalog(&snapshot(SECOND, 2 * SECOND, "1.000000", "1.000000"), &[]);
    let native =
        ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, Some(4096), None, None)
            .unwrap();
    let quotes = (0..40)
        .map(|index| {
            let mut bid = [100.0, 101.0, 102.0, 99.0, 97.0, 103.0, 105.0, 96.0][index % 8];
            if change_future && index >= 24 {
                bid += 20.0;
            }
            QuoteTick::new(
                id(),
                Price::new(bid, 2),
                Price::new(bid + 1.0, 2),
                Quantity::from("1.000000"),
                Quantity::from("1.000000"),
                (if index == 24 {
                    SECOND + 1
                } else {
                    (index as u64 + 1) * SECOND
                })
                .into(),
                ((index as u64 / 2 + 1) * 2 * SECOND).into(),
            )
        })
        .collect::<Vec<_>>();
    native.write_to_parquet(&quotes, None, None, None).unwrap();
    let mut candidate = request(41 * SECOND);
    candidate.event_end_ns = contracts::DbCounter::new(41 * SECOND).unwrap();
    candidate.selectors[0].data_type = "QuoteTick".into();
    candidate.venue.book_type = "L1_MBP".into();
    candidate.strategy = serde_json::from_value(
        json!({"kind":"EMA_CROSS","trade_size":"0.010000","fast_period":2,"slow_period":3}),
    )
    .unwrap();
    let mut second = candidate.clone();
    second.strategy = serde_json::from_value(
        json!({"kind":"EMA_CROSS","trade_size":"0.020000","fast_period":2,"slow_period":3}),
    )
    .unwrap();
    (
        dir,
        NativeStrategyWalkForwardRequestV1 {
            schema_version: contracts::SchemaV1,
            candidates: vec![candidate, second],
            split: NativeWalkForwardSplitV1 {
                n_splits: 2,
                test_size: Some(4),
                gap: 0,
                max_train_size: None,
            },
            objective: NativeWalkForwardObjectiveV1 {
                metric_source: Default::default(),
                annualization_days: None,
                native_key: "PnL (total)".into(),
                currency: Some("USDT".into()),
                direction: NativeObjectiveDirectionV1::Maximize,
            },
        },
    )
}

#[test]
fn genuine_native_training_selects_past_only_and_test_is_independent_cold_start() {
    let (dir, request) = walk_fixture(false);
    let result = job::native_walk_forward::run(dir.path(), request).unwrap();
    assert_eq!(result.folds.len(), 2);
    assert_eq!(
        result.performance_scope,
        "INDEPENDENT_COLD_START_TEST_WINDOW_NATIVE_STATISTICS"
    );
    for fold in &result.folds {
        assert_eq!(
            fold.status,
            "COMPLETED",
            "{}",
            serde_json::to_string(fold).unwrap()
        );
        assert!(fold.train_end_ns < fold.test_start_ns);
        let selected = fold.selected_candidate_index.unwrap();
        let best = fold.trials[selected].metric_value.unwrap();
        assert!(
            fold.trials
                .iter()
                .all(|trial| trial.metric_value.unwrap() <= best)
        );
        for trial in &fold.trials {
            let run = trial.result.as_ref().unwrap();
            assert!(run.request.available_end_ns < fold.test_start_ns);
            assert!(!run.canonical_result["fills"].as_array().unwrap().is_empty());
        }
        let test = fold.test_result.as_ref().unwrap();
        assert_eq!(test.request.available_start_ns, Some(fold.test_start_ns));
        assert_eq!(test.data_rows_by_type["QuoteTick"].get(), 8); // four complete two-row availability batches
        let independent = job::native_strategy::run(dir.path(), test.request.clone()).unwrap();
        assert_eq!(test.canonical_result, independent.canonical_result);
        assert!(
            !test.canonical_result["fills"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    let (changed, changed_request) = walk_fixture(true);
    let changed_result = job::native_walk_forward::run(changed.path(), changed_request).unwrap();
    // Future prices affect test, never the first fold's training or chosen candidate.
    assert_eq!(
        result.folds[0].selected_candidate_index,
        changed_result.folds[0].selected_candidate_index
    );
    for (left, right) in result.folds[0]
        .trials
        .iter()
        .zip(&changed_result.folds[0].trials)
    {
        assert_eq!(left.metric_value, right.metric_value);
        assert_eq!(
            left.result.as_ref().unwrap().canonical_result,
            right.result.as_ref().unwrap().canonical_result
        );
    }
    assert_ne!(
        result.folds[0]
            .test_result
            .as_ref()
            .unwrap()
            .canonical_result,
        changed_result.folds[0]
            .test_result
            .as_ref()
            .unwrap()
            .canonical_result
    );
}

#[test]
fn minimize_selects_lower_native_value_and_missing_metric_never_fabricates_test_success() {
    let (dir, mut request) = walk_fixture(false);
    request.objective.direction =
        contracts::native_walk_forward::NativeObjectiveDirectionV1::Minimize;
    let result = job::native_walk_forward::run(dir.path(), request.clone()).unwrap();
    for fold in result.folds {
        let best = fold.trials[fold.selected_candidate_index.unwrap()]
            .metric_value
            .unwrap();
        assert_ne!(
            fold.trials[0].metric_value, fold.trials[1].metric_value,
            "fixture must distinguish actual native candidate outcomes"
        );
        assert!(fold.trials.iter().all(|v| v.metric_value.unwrap() >= best));
    }
    request.objective.native_key = "not an upstream statistic".into();
    let result = job::native_walk_forward::run(dir.path(), request).unwrap();
    assert!(result.folds.iter().all(|fold| {
        fold.status == "NOT_EVALUATED"
            && fold.test_result.is_none()
            && fold.selected_candidate_index.is_none()
            && fold
                .trials
                .iter()
                .all(|v| v.metric_value.is_none() && v.error.is_some() && v.result.is_some())
    }));
}

#[test]
fn cli_emits_real_walk_forward_evidence_as_one_json_document() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let (dir, request) = walk_fixture(false);
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .args([
            "native-walk-forward",
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
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: contracts::native_walk_forward::NativeStrategyWalkForwardResultV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        serde_json::to_value(result.request).unwrap(),
        serde_json::to_value(request).unwrap()
    );
    assert_eq!(result.folds.len(), 2);
    assert!(result.folds.iter().all(|fold| {
        fold.status == "COMPLETED"
            && !fold.test_result.as_ref().unwrap().canonical_result["fills"]
                .as_array()
                .unwrap()
                .is_empty()
    }));
}

#[test]
fn actual_walk_forward_preserves_static_scenario_in_every_training_and_test_result() {
    use contracts::native_strategy::NativeInstrumentDefinitionV1;
    let (dir, mut request) = walk_fixture(false);
    let native =
        ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, None, None, None).unwrap();
    let mut raw =
        serde_json::to_value(InstrumentAny::CurrencyPair(currency_pair_btcusdt())).unwrap();
    raw["CurrencyPair"]["ts_init"] = json!(100 * SECOND + 11);
    raw["CurrencyPair"]["ts_event"] = json!(100 * SECOND + 7);
    native
        .write_instruments(vec![serde_json::from_value(raw).unwrap()])
        .unwrap();
    for candidate in &mut request.candidates {
        candidate.instrument_definition = Some(NativeInstrumentDefinitionV1::StaticScenario {
            ts_init_ns: contracts::DbCounter::new(100 * SECOND + 11).unwrap(),
            ts_event_ns: contracts::DbCounter::new(100 * SECOND + 7).unwrap(),
        });
    }
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .args([
            "native-walk-forward",
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
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: contracts::native_walk_forward::NativeStrategyWalkForwardResultV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result
            .notes
            .iter()
            .any(|note| note.contains("HYPOTHETICAL_STATIC_RULES"))
    );
    for fold in result.folds {
        assert_eq!(fold.status, "COMPLETED");
        for result in fold
            .trials
            .iter()
            .map(|trial| trial.result.as_ref().unwrap())
            .chain(std::iter::once(fold.test_result.as_ref().unwrap()))
        {
            assert_eq!(result.instrument_definition.mode, "STATIC_SCENARIO");
            assert_eq!(
                result.instrument_definition.ts_init_ns.get(),
                100 * SECOND + 11
            );
            assert_eq!(
                result.instrument_definition.scope,
                "HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED"
            );
            assert!(matches!(
                result.request.instrument_definition,
                Some(NativeInstrumentDefinitionV1::StaticScenario { .. })
            ));
            assert!(
                !result.canonical_result["fills"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
