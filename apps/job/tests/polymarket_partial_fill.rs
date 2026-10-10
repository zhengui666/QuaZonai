//! Synthetic source records through the official native replay and Paper engine.
#[path = "support/market.rs"]
mod market;
#[path = "support/polymarket.rs"]
mod polymarket;
#[cfg(feature = "native-paper-test")]
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio;

use contracts::{portfolio::AllocationTargetV1, science::*};
use nautilus_execution::models::fee::FeeModel;
use nautilus_model::{
    data::{Bar, BarType},
    enums::{LiquiditySide, OrderSide, OrderType},
    instruments::{Instrument, InstrumentAny},
    orders::{builder::OrderTestBuilder, Order, OrderAny},
    types::{Money, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use nautilus_polymarket::models::PolymarketFeeModel;
use polymarket::{IDS, STEP};
use serde_json::Value;
use std::str::FromStr;

fn fixture(volume: &str) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    let (_unused, mut request) = market::market("0", 20);
    let root = tempfile::tempdir().unwrap();
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None)
            .unwrap();
    let instruments = polymarket::instruments("0.05", 18 * STEP);
    catalog.write_instruments(instruments).unwrap();
    for id in IDS {
        let kind = BarType::from_str(&format!("{id}-1-MINUTE-LAST-EXTERNAL")).unwrap();
        let bars = (1..=17)
            .map(|minute| {
                Bar::new_checked(
                    kind,
                    Price::from("0.4000"),
                    Price::from("0.4000"),
                    Price::from("0.4000"),
                    Price::from("0.4000"),
                    Quantity::from(volume),
                    (minute * STEP).into(),
                    (minute * STEP + 1).into(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
    }
    polymarket::settings(&mut request.settings, "0.05", 18 * STEP);
    request.settings.starting_capital = "1000".parse().unwrap();
    request.selection.bar_types = IDS
        .iter()
        .map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL"))
        .collect();
    request.target_points.truncate(1);
    let point = &mut request.target_points[0];
    point.asof_ns = market::count(5 * STEP + 1);
    point.valid_until_ns = market::count(18 * STEP);
    point.cash_weight = "0.6".parse().unwrap();
    point.targets = IDS
        .iter()
        .map(|id| AllocationTargetV1 {
            instrument_id: (*id).into(),
            currency: "pUSD".into(),
            weight: "0.2".parse().unwrap(),
        })
        .collect();
    request.settlements = polymarket::settlement_groups(18 * STEP, 19 * STEP, ["1.0000", "0.0000"]);
    polymarket::settle(root.path(), 18 * STEP, 19 * STEP, ["1.0000", "0.0000"]);
    (root, request)
}

fn official_fee(fill: &Value, instrument: &InstrumentAny) -> Money {
    let qty = Quantity::from_str(fill["last_qty"].as_str().unwrap()).unwrap();
    let px = Price::from_str(fill["last_px"].as_str().unwrap()).unwrap();
    let side = if fill["order_side"] == "BUY" {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    let liquidity = if fill["liquidity_side"] == "MAKER" {
        LiquiditySide::Maker
    } else {
        LiquiditySide::Taker
    };
    let mut order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument.id())
        .side(side)
        .quantity(qty)
        .build();
    if let OrderAny::Market(ref mut value) = order {
        value.set_liquidity_side(liquidity);
    }
    PolymarketFeeModel
        .get_commission(&order, qty, px, instrument)
        .unwrap()
}

fn audit_fills(result: &Value, instruments: &[InstrumentAny]) {
    for value in result["fills"].as_array().unwrap() {
        let fill = &value["event"]["Filled"];
        let instrument = instruments
            .iter()
            .find(|i| i.id().to_string() == fill["instrument_id"].as_str().unwrap())
            .unwrap();
        let actual = Money::from_str(fill["commission"].as_str().unwrap()).unwrap();
        if fill["client_order_id"]
            .as_str()
            .unwrap()
            .starts_with("EXPIRATION-POLYMARKET-")
        {
            assert_eq!(actual, Money::zero(instrument.quote_currency()));
            assert_eq!(fill["ts_event"], (19 * STEP).to_string());
        } else {
            assert_eq!(actual, official_fee(fill, instrument), "{fill}");
        }
    }
}

#[test]
fn native_replay_accepts_sub_one_partial_fills_and_preserves_each_official_money() {
    let (root, request) = fixture("8.000000");
    let result = job::simulation::simulate(root.path(), &request).unwrap();
    let fills = result.canonical_result["fills"].as_array().unwrap();
    let first = &fills[0]["event"]["Filled"];
    let notional = Quantity::from_str(first["last_qty"].as_str().unwrap())
        .unwrap()
        .as_decimal()
        * Price::from_str(first["last_px"].as_str().unwrap())
            .unwrap()
            .as_decimal();
    assert!(notional > rust_decimal::Decimal::ZERO && notional < rust_decimal::Decimal::ONE);
    assert_eq!(first["last_qty"], "2.000000");
    audit_fills(
        &result.canonical_result,
        &polymarket::instruments("0.05", 18 * STEP),
    );
    for key in ["orders.open", "orders.inflight", "positions.open"] {
        assert_eq!(result.summary[key], "0");
    }
}

#[test]
fn native_replay_over_bound_partial_fee_cannot_publish_successful_performance() {
    let (root, request) = fixture("0.001920");
    assert!(job::simulation::simulate(root.path(), &request).is_err());
}

#[test]
fn frozen_planning_rate_and_collateral_changes_still_reject_native_replay() {
    let (root, request) = fixture("8.000000");
    let mut wrong_rate = request.clone();
    wrong_rate.settings.fee_rates[0].taker = "0.04".parse().unwrap();
    assert!(job::simulation::simulate(root.path(), &wrong_rate).is_err());
    let mut wrong_currency = request;
    wrong_currency.settings.base_currency = "USD".into();
    assert!(job::simulation::simulate(root.path(), &wrong_currency).is_err());
}

#[cfg(feature = "native-paper-test")]
mod streaming {
    use super::*;
    use contracts::{strategy_portfolio::FreshPaperCashV1, DbCounter, Id, SchemaV1};
    use job::polymarket_streaming_paper::PolymarketStreamingPaper;
    use nautilus_model::{
        data::{Data, QuoteTick},
        identifiers::InstrumentId,
    };
    const SECOND: u64 = 1_000_000_000;
    const LATENCY: u64 = 1_000_000;

    fn session() -> PolymarketStreamingPaper {
        let (_unused, request) = market::market("0", 20);
        let input = serde_json::from_str(include_str!(
            "../../../tests/contracts/allocation-input.json"
        ))
        .unwrap();
        let portfolio = portfolio::request(&input);
        let mut settings = request.settings;
        settings.starting_capital = "1000".parse().unwrap();
        settings.latency_model = portfolio::execution_models::latency(LATENCY);
        polymarket::settings(&mut settings, "0.05", 60 * SECOND);
        settings.fee_rates.truncate(1);
        let mut constraints = portfolio.mandate.constraints;
        constraints.max_cash_weight = "1".parse().unwrap();
        constraints.min_net_exposure = "0".parse().unwrap();
        let account = FreshPaperCashV1 {
            downstream_id: Id::new(),
            trader_id: "QZ-PARTIAL-FILL-001".into(),
            account_id: "POLYMARKET-001".into(),
            base_currency: settings.base_currency.clone(),
            starting_capital: settings.starting_capital.clone(),
            execution_assumptions_id: Id::new(),
        };
        let point = NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: DbCounter::new(SECOND).unwrap(),
            valid_until_ns: DbCounter::new(20 * SECOND).unwrap(),
            cash_weight: "0.996".parse().unwrap(),
            targets: vec![AllocationTargetV1 {
                instrument_id: IDS[0].into(),
                currency: "pUSD".into(),
                weight: "0.004".parse().unwrap(),
            }],
        };
        PolymarketStreamingPaper::new(
            &account,
            &settings,
            &constraints,
            point,
            vec![polymarket::instruments("0.05", 60 * SECOND).remove(0)],
            vec![BarType::from_str(&format!("{}-1-SECOND-MID-INTERNAL", IDS[0])).unwrap()],
        )
        .unwrap()
    }

    fn quote(at: u64, size: &str) -> Data {
        Data::Quote(QuoteTick::new(
            InstrumentId::from(IDS[0]),
            Price::from("0.4000"),
            Price::from("0.4000"),
            Quantity::from(size),
            Quantity::from(size),
            at.into(),
            at.into(),
        ))
    }

    #[test]
    fn actual_quotes_drive_sub_one_partial_fills_after_original_latency() {
        let mut session = session();
        for half_second in 2..=30 {
            session
                .push(quote(half_second * SECOND / 2, "2.000000"))
                .unwrap();
        }
        let report = session.finish().unwrap();
        assert_eq!(
            report["performance_status"], "NATIVE_SIMULATION_AVAILABLE",
            "{report}"
        );
        let canonical = &report["canonical_result"];
        let fills = canonical["fills"].as_array().unwrap();
        assert!(!fills.is_empty());
        let first = &fills[0]["event"]["Filled"];
        assert_eq!(first["last_qty"], "2.000000");
        let notional = Quantity::from_str(first["last_qty"].as_str().unwrap())
            .unwrap()
            .as_decimal()
            * Price::from_str(first["last_px"].as_str().unwrap())
                .unwrap()
                .as_decimal();
        assert!(notional < rust_decimal::Decimal::ONE);
        let submitted = canonical["orders"][0]["Market"]["core"]["ts_submitted"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!(first["ts_event"].as_str().unwrap().parse::<u64>().unwrap() >= submitted + LATENCY);
        audit_fills(canonical, &polymarket::instruments("0.05", 60 * SECOND));
        assert_eq!(report["native_terminal_valuation_complete"], true);
    }

    #[test]
    fn actual_quote_rounding_violation_cannot_return_available_performance() {
        let mut session = session();
        for half_second in 2..=20 {
            if session
                .push(quote(half_second * SECOND / 2, "0.000480"))
                .is_err()
            {
                break;
            }
        }
        match session.finish() {
            Err(_) => {}
            Ok(report) => assert_ne!(
                report["performance_status"], "NATIVE_SIMULATION_AVAILABLE",
                "{report}"
            ),
        }
    }

    #[test]
    fn streaming_frozen_source_fee_and_currency_mutations_remain_rejected() {
        for (field, value) in [
            ("rate", serde_json::json!(0.04)),
            ("currency", serde_json::json!("USD")),
        ] {
            let mut session = session();
            let mut payload =
                serde_json::to_value(polymarket::instruments("0.05", 60 * SECOND).remove(0))
                    .unwrap();
            if field == "currency" {
                payload["BinaryOption"][field] = value;
            } else {
                payload["BinaryOption"]["info"]["fee_schedule"][field] = value;
            }
            let changed: InstrumentAny = serde_json::from_value(payload).unwrap();
            assert!(session.observe_definition(&changed).is_err());
        }
    }
}
