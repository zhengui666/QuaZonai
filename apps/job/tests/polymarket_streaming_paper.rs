//! Explicit synthetic engineering regressions over the official native engine.
//! These fixtures never establish real source/PIT/claim or account-service evidence.
#![cfg(feature = "native-paper-test")]

#[path = "support/polymarket.rs"]
mod polymarket;
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio;

use contracts::{
    portfolio::{AllocationInputV1, AllocationTargetV1},
    science::NativeTargetPointV1,
    strategy_portfolio::FreshPaperCashV1,
    DbCounter, Id, SchemaV1,
};
use job::polymarket_streaming_paper::PolymarketStreamingPaper;
use nautilus_execution::models::fee::FeeModel;
use nautilus_model::{
    data::{BarType, Data, QuoteTick, TradeTick},
    enums::{AggressorSide, LiquiditySide, OrderSide, TimeInForce},
    identifiers::{ClientOrderId, InstrumentId, StrategyId, TradeId, TraderId},
    orders::{MarketOrder, Order, OrderAny},
    types::{Price, Quantity},
};
use nautilus_polymarket::models::PolymarketFeeModel;
use serde_json::Value;
use std::{io::Write, str::FromStr};

const SECOND: u64 = 1_000_000_000;
const LATENCY: u64 = 1_000_000;

fn session() -> PolymarketStreamingPaper {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let request = portfolio::request(&input);
    let mut settings = request.execution_settings;
    settings.starting_capital = "1000".parse().unwrap();
    settings.latency_model = portfolio::execution_models::latency(LATENCY);
    polymarket::settings(&mut settings, "0.02", 60 * SECOND);
    settings.fee_rates.truncate(1);
    let mut constraints = request.mandate.constraints;
    constraints.max_cash_weight = "1".parse().unwrap();
    constraints.min_net_exposure = "0".parse().unwrap();
    let account = FreshPaperCashV1 {
        downstream_id: Id::new(),
        trader_id: "QZ-STREAM-REGRESSION-001".into(),
        account_id: "POLYMARKET-001".into(),
        base_currency: settings.base_currency.clone(),
        starting_capital: settings.starting_capital.clone(),
        execution_assumptions_id: Id::new(),
    };
    let point = NativeTargetPointV1 {
        schema_version: SchemaV1,
        asof_ns: DbCounter::new(SECOND).unwrap(),
        valid_until_ns: DbCounter::new(20 * SECOND).unwrap(),
        targets: vec![AllocationTargetV1 {
            instrument_id: polymarket::IDS[0].into(),
            weight: "0.2".parse().unwrap(),
            currency: "pUSD".into(),
        }],
        cash_weight: "0.8".parse().unwrap(),
    };
    let mut instruments = polymarket::instruments("0.02", 60 * SECOND);
    instruments.truncate(1);
    PolymarketStreamingPaper::new(
        &account,
        &settings,
        &constraints,
        point,
        instruments,
        vec![BarType::from_str(&format!("{}-1-SECOND-MID-INTERNAL", polymarket::IDS[0])).unwrap()],
    )
    .unwrap()
}

fn quote(at: u64) -> Data {
    Data::Quote(QuoteTick::new(
        InstrumentId::from(polymarket::IDS[0]),
        Price::from("0.4900"),
        Price::from("0.5100"),
        Quantity::from("1000000.000000"),
        Quantity::from("1000000.000000"),
        at.into(),
        at.into(),
    ))
}

fn trade(at: u64) -> Data {
    Data::Trade(TradeTick::new(
        InstrumentId::from(polymarket::IDS[0]),
        Price::from("0.8000"),
        Quantity::from("1000000.000000"),
        AggressorSide::Buy,
        TradeId::from("synthetic-engineering-trade"),
        at.into(),
        at.into(),
    ))
}

fn retain(name: &str, report: &Value) {
    if let Some(directory) = std::env::var_os("QZ_NATIVE_STREAM_TEST_OUTPUT_DIR") {
        let path = std::path::PathBuf::from(directory).join(format!("{name}.json"));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer_pretty(&mut file, report).unwrap();
        file.write_all(b"\n").unwrap();
        file.sync_all().unwrap();
    }
}

#[test]
fn original_quotes_drive_one_native_account_with_nonzero_fees() {
    let mut session = session();
    for half_second in 2..=20 {
        session.push(quote(half_second * SECOND / 2)).unwrap();
    }
    let report = session.finish().unwrap();
    retain("quotes-complete", &report);
    assert_eq!(
        report["performance_status"], "NATIVE_SIMULATION_AVAILABLE",
        "{report}"
    );
    assert_eq!(report["native_account_id"], "POLYMARKET-001");
    assert_eq!(report["native_terminal_valuation_complete"], true);
    assert_eq!(report["consumed_original_target_points"], 1);
    let fills = report["canonical_result"]["fills"].as_array().unwrap();
    assert_eq!(fills.len(), 1, "{report}");
    let fill = &fills[0]["event"]["Filled"];
    assert_eq!(fill["last_px"], "0.5100");
    assert_eq!(fill["last_qty"], "400.000000");
    assert_eq!(fill["liquidity_side"], "TAKER");
    let quantity = Quantity::from_str(fill["last_qty"].as_str().unwrap()).unwrap();
    let price = Price::from_str(fill["last_px"].as_str().unwrap()).unwrap();
    // Independent invocation of the unchanged official fee model; no copied fee formula.
    let mut fee_order = MarketOrder::new_checked(
        TraderId::from("QZ-FEE-REGRESSION-001"),
        StrategyId::from("QZ-FEE-001"),
        InstrumentId::from(polymarket::IDS[0]),
        ClientOrderId::from("synthetic-fee-order"),
        OrderSide::Buy,
        quantity,
        TimeInForce::Gtc,
        nautilus_core::UUID4::new(),
        SECOND.into(),
        false,
        false,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    fee_order.set_liquidity_side(LiquiditySide::Taker);
    let expected_fee = PolymarketFeeModel
        .get_commission(
            &OrderAny::Market(fee_order),
            quantity,
            price,
            &polymarket::instruments("0.02", 60 * SECOND)[0],
        )
        .unwrap();
    assert!(expected_fee.as_decimal() > rust_decimal::Decimal::ZERO);
    assert_eq!(fill["commission"], expected_fee.to_string());
    let submitted = report["canonical_result"]["orders"][0]["Market"]["core"]["ts_submitted"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let filled = fill["ts_event"].as_str().unwrap().parse::<u64>().unwrap();
    assert!(filled >= submitted + LATENCY && filled <= 10 * SECOND);
    assert_eq!(
        report["canonical_result"]["positions"][0]["quantity"],
        "400.000000"
    );
    convert_original_snapshots(&report);
}

fn convert_original_snapshots(report: &Value) {
    use contracts::{account_observation::*, forward::ForwardEnvironmentV1};
    use std::{
        fs::File,
        process::{Command, Stdio},
    };
    let directory = tempfile::tempdir().unwrap();
    let binding = NativeAccountBindingV1 {
        schema_version: SchemaV1,
        project_id: Id::new(),
        environment: ForwardEnvironmentV1::Paper,
        native_trader_id: report["native_trader_id"].as_str().unwrap().into(),
        native_session_id: report["native_session_id"].as_str().unwrap().into(),
        native_account_id: report["native_account_id"].as_str().unwrap().into(),
        native_version: NATIVE_ACCOUNT_VERSION.into(),
    };
    let binding_path = directory.path().join("engineering-binding.json");
    serde_json::to_writer(File::create(&binding_path).unwrap(), &binding).unwrap();
    let snapshots = report["original_portfolio_snapshots"].as_array().unwrap();
    let input_path = directory.path().join("original-native-snapshots.ndjson");
    let mut input = File::create(&input_path).unwrap();
    for value in snapshots {
        serde_json::to_writer(&mut input, value).unwrap();
        input.write_all(b"\n").unwrap();
    }
    drop(input);
    let output_path = directory.path().join("observations.ndjson");
    let result = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("native-account-observation")
        .arg("--binding")
        .arg(&binding_path)
        .args([
            "--last-sequence",
            "0",
            "--dropped-events",
            "0",
            "--stream",
            "--output",
        ])
        .arg(&output_path)
        .stdin(File::open(&input_path).unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = std::fs::read_to_string(&output_path).unwrap();
    let observations = output
        .lines()
        .map(|line| serde_json::from_str::<AccountObservationSubmitV1>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(observations.len(), snapshots.len());
    for (index, (observation, original)) in observations.iter().zip(snapshots).enumerate() {
        let native = serde_json::from_value(original.clone()).unwrap();
        let expected = job::account_observer::project_snapshot(&native).unwrap();
        assert_eq!(observation.sequence.get(), index as u64 + 1);
        assert_eq!(observation.dropped_events.get(), 0);
        assert_eq!(observation.connection, AccountConnectionV1::Unknown);
        assert_eq!(
            serde_json::to_value(&observation.snapshot).unwrap(),
            serde_json::to_value(Some(expected)).unwrap()
        );
        let converted = observation.snapshot.as_ref().unwrap();
        assert_eq!(converted.event_id, original["event_id"].as_str().unwrap());
        assert_eq!(
            converted.ts_event.get(),
            original["ts_event"].as_u64().unwrap()
        );
        assert_eq!(
            converted.ts_init.get(),
            original["ts_init"].as_u64().unwrap()
        );
        assert_eq!(
            converted.account_id,
            original["account_id"].as_str().unwrap()
        );
        assert_eq!(
            converted.base_currency.as_deref(),
            original["base_currency"].as_str()
        );
        assert_eq!(converted.balances.len(), native.balances.len());
        for (converted, original) in converted.balances.iter().zip(&native.balances) {
            for (amount, money) in [
                (&converted.total, &original.total),
                (&converted.free, &original.free),
                (&converted.locked, &original.locked),
            ] {
                assert_eq!(amount.currency, money.currency.to_string());
                assert_eq!(
                    amount
                        .amount
                        .as_decimal()
                        .to_string()
                        .parse::<rust_decimal::Decimal>()
                        .unwrap(),
                    money.as_decimal()
                );
            }
        }
    }
    retain(
        "converted-original-snapshots",
        &serde_json::to_value(observations).unwrap(),
    );
}

#[test]
fn trade_only_tail_cannot_supply_execution_prices_or_extend_quote_coverage() {
    let mut session = session();
    for half_second in 2..=4 {
        session.push(quote(half_second * SECOND / 2)).unwrap();
    }
    session.push(trade(2 * SECOND + 10 * LATENCY)).unwrap();
    let report = session.finish().unwrap();
    retain("trade-only-tail", &report);
    assert_eq!(report["performance_status"], "UNAVAILABLE", "{report}");
    assert_eq!(
        report["reason_code"],
        "PAPER_STREAM_NATIVE_EXECUTION_BEYOND_SOURCE"
    );
    assert_eq!(
        report["source_covered_through_ns"],
        (2 * SECOND).to_string()
    );
    assert!(
        report["canonical_result"]["fills"]
            .as_array()
            .unwrap()
            .iter()
            .all(|fill| fill["event"]["Filled"]["last_px"] != "0.8000"),
        "{report}"
    );
}

#[test]
fn eof_with_a_latency_deferred_order_is_unavailable() {
    let mut session = session();
    for half_second in 2..=4 {
        session.push(quote(half_second * SECOND / 2)).unwrap();
    }
    let report = session.finish().unwrap();
    retain("quote-eof", &report);
    assert_eq!(report["performance_status"], "UNAVAILABLE", "{report}");
    assert_eq!(
        report["reason_code"], "PAPER_STREAM_NATIVE_EXECUTION_BEYOND_SOURCE",
        "{report}"
    );
    assert_eq!(
        report["source_covered_through_ns"],
        (2 * SECOND).to_string()
    );
    assert_eq!(
        report["native_ended_at_ns"],
        (2 * SECOND + LATENCY).to_string()
    );
}
