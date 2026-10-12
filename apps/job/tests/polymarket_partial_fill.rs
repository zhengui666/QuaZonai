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

fn order_core(order: &Value) -> &Value {
    order
        .as_object()
        .unwrap()
        .values()
        .find_map(|payload| payload.get("core"))
        .unwrap()
}

fn is_expiration_order(core: &Value) -> bool {
    core["is_reduce_only"].as_bool().unwrap()
        && core["tags"]
            .as_array()
            .is_some_and(|tags| tags.iter().any(|tag| tag == "EXPIRATION_POLYMARKET_CLOSE"))
}

fn fill_context(value: &Value, core: &Value) -> String {
    let fill = &value["event"]["Filled"];
    let notional = Quantity::from_str(fill["last_qty"].as_str().unwrap_or(""))
        .ok()
        .zip(Price::from_str(fill["last_px"].as_str().unwrap_or("")).ok())
        .map(|(qty, px)| (qty.as_decimal() * px.as_decimal()).to_string());
    format!(
        "order_id={} ordinal={} qty={} px={} notional={} commission={} tags={} reduce_only={}",
        core["client_order_id"],
        value["order_event_ordinal"],
        fill["last_qty"],
        fill["last_px"],
        notional.as_deref().unwrap_or("unparseable"),
        fill["commission"],
        core["tags"],
        core["is_reduce_only"]
    )
}

fn order_fills<'a>(result: &'a Value, core: &Value) -> Vec<&'a Value> {
    let mut fills = result["fills"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|value| value["client_order_id"] == core["client_order_id"])
        .collect::<Vec<_>>();
    fills.sort_by_key(|value| {
        value["order_event_ordinal"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    });
    fills
}

fn audit_fills(result: &Value, instruments: &[InstrumentAny]) {
    for value in result["fills"].as_array().unwrap() {
        let fill = &value["event"]["Filled"];
        let core = result["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(order_core)
            .find(|core| core["client_order_id"] == value["client_order_id"])
            .unwrap_or_else(|| panic!("unmatched fill={value}; orders={}", result["orders"]));
        let context = fill_context(value, core);
        eprintln!("canonical fill: {context}");
        assert_eq!(
            fill["client_order_id"], core["client_order_id"],
            "{context}"
        );
        assert_eq!(fill["instrument_id"], core["instrument_id"], "{context}");
        let instrument = instruments
            .iter()
            .find(|i| i.id().to_string() == fill["instrument_id"].as_str().unwrap())
            .unwrap();
        let actual = Money::from_str(fill["commission"].as_str().unwrap()).unwrap();
        if is_expiration_order(core) {
            assert_eq!(
                actual,
                Money::zero(instrument.quote_currency()),
                "{context}"
            );
            assert_eq!(fill["ts_event"], (19 * STEP).to_string(), "{context}");
            let index = IDS
                .iter()
                .position(|id| *id == instrument.id().to_string())
                .unwrap();
            assert_eq!(fill["last_px"], ["1.0000", "0.0000"][index], "{context}");
        } else {
            assert_eq!(actual, official_fee(fill, instrument), "{context}");
        }
    }
}

#[test]
fn native_replay_accepts_sub_one_partial_fills_and_preserves_each_official_money() {
    let (root, request) = fixture("8.000000");
    let result = job::simulation::simulate(root.path(), &request).unwrap();
    let canonical = &result.canonical_result;
    audit_fills(canonical, &polymarket::instruments("0.05", 18 * STEP));
    let orders = canonical["orders"].as_array().unwrap();
    for id in IDS {
        let buys = orders
            .iter()
            .map(order_core)
            .filter(|core| {
                core["instrument_id"] == id
                    && core["side"] == "BUY"
                    && core["is_reduce_only"] == false
            })
            .collect::<Vec<_>>();
        assert_eq!(buys.len(), 1, "instrument={id}; orders={orders:?}");
        let core = buys[0];
        let fills = order_fills(canonical, core);
        assert!(fills.len() >= 2, "order={core}; fills={fills:?}");
        let first = &fills[0]["event"]["Filled"];
        let context = fill_context(fills[0], core);
        let qty = Quantity::from_str(first["last_qty"].as_str().unwrap()).unwrap();
        let px = Price::from_str(first["last_px"].as_str().unwrap()).unwrap();
        let notional = qty.as_decimal() * px.as_decimal();
        assert_eq!(first["last_qty"], "2.000000", "{context}");
        assert_eq!(first["last_px"], "0.4000", "{context}");
        assert!(
            notional > rust_decimal::Decimal::ZERO && notional < rust_decimal::Decimal::ONE,
            "{context}"
        );
        let order_qty = Quantity::from_str(core["quantity"].as_str().unwrap()).unwrap();
        assert!(qty < order_qty, "{context}; order_qty={order_qty}");
        let total = fills
            .iter()
            .fold(rust_decimal::Decimal::ZERO, |sum, value| {
                sum + Quantity::from_str(value["event"]["Filled"]["last_qty"].as_str().unwrap())
                    .unwrap()
                    .as_decimal()
            });
        assert_eq!(total, order_qty.as_decimal(), "{context}; fills={fills:?}");
        let settlements = orders
            .iter()
            .map(order_core)
            .filter(|core| core["instrument_id"] == id && is_expiration_order(core))
            .collect::<Vec<_>>();
        assert_eq!(settlements.len(), 1, "instrument={id}; orders={orders:?}");
        let settlement_fills = order_fills(canonical, settlements[0]);
        assert_eq!(
            settlement_fills.len(),
            1,
            "settlement={}; fills={settlement_fills:?}",
            settlements[0]
        );
    }
    for key in ["orders.open", "orders.inflight", "positions.open"] {
        assert_eq!(result.summary[key], "0");
    }
}

#[test]
fn native_replay_over_bound_partial_fee_cannot_publish_successful_performance() {
    let (root, request) = fixture("0.001920");
    let error = job::simulation::simulate(root.path(), &request).unwrap_err();
    assert_eq!(error.to_string(), "NATIVE_TARGET_REPLAY_FAILED");
    assert_eq!(
        format!("{error:#}"),
        "NATIVE_TARGET_REPLAY_FAILED: POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND"
    );
    audit_fee_failure(&error);
}

fn audit_fee_failure(error: &anyhow::Error) {
    let failure = error
        .downcast_ref::<job::simulation::FillFeeFailure>()
        .unwrap();
    assert_eq!(failure.reason, "POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND");
    assert_eq!(failure.quantity, Quantity::from("0.000480"));
    assert_eq!(failure.price, Price::from("0.4000"));
    assert!(IDS
        .iter()
        .any(|id| *id == failure.instrument_id.to_string()));
    assert!(!failure.client_order_id.as_str().is_empty());
    let instrument = polymarket::instruments("0.05", 18 * STEP)
        .into_iter()
        .find(|instrument| instrument.id() == failure.instrument_id)
        .unwrap();
    let mut order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(failure.instrument_id)
        .side(OrderSide::Buy)
        .quantity(failure.quantity)
        .build();
    if let OrderAny::Market(ref mut value) = order {
        value.set_liquidity_side(LiquiditySide::Taker);
    }
    let native = PolymarketFeeModel
        .get_commission(&order, failure.quantity, failure.price, &instrument)
        .unwrap();
    assert_eq!(
        native.as_decimal(),
        rust_decimal::Decimal::from_str("0.00001").unwrap()
    );
    let payload = serde_json::to_value(instrument).unwrap();
    let bound = bigdecimal::BigDecimal::from_str(&failure.quantity.as_decimal().to_string())
        .unwrap()
        * bigdecimal::BigDecimal::from_str(&failure.price.as_decimal().to_string()).unwrap()
        * domain::prediction::planning_fee(&payload["BinaryOption"])
            .unwrap()
            .as_decimal();
    assert_eq!(
        bound,
        bigdecimal::BigDecimal::from_str("0.00000960096").unwrap()
    );
    eprintln!(
        "native fee failure={error:#}; inputs={failure:?}; official_money={native}; bound={bound}"
    );
}

#[test]
fn frozen_planning_rate_and_collateral_changes_still_reject_native_replay() {
    let (root, request) = fixture("8.000000");
    let mut wrong_rate = request.clone();
    wrong_rate.settings.fee_rates[0].taker = "0.04".parse().unwrap();
    let error = job::simulation::simulate(root.path(), &wrong_rate).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "POLYMARKET_PLANNING_FEE_SOURCE_MISMATCH"
    );
    eprintln!("frozen replay rate error={error:#}");
    let mut wrong_currency = request;
    wrong_currency.settings.base_currency = "USD".into();
    let error = job::simulation::simulate(root.path(), &wrong_currency).unwrap_err();
    assert_eq!(format!("{error:#}"), "SIMULATION_MARKET_UNSUPPORTED");
    eprintln!("frozen replay currency error={error:#}");
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
        let mut failure = None;
        for half_second in 2..=20 {
            if let Err(error) = session.push(quote(half_second * SECOND / 2, "0.000480")) {
                failure = Some(error);
                break;
            }
        }
        let failure = failure.expect("the real quote fill must expose its fee failure");
        assert_eq!(
            format!("{failure:#}"),
            "POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND"
        );
        audit_fee_failure(&failure);
        assert_fee_unavailable(&session.finish().unwrap());
    }

    fn assert_fee_unavailable(report: &Value) {
        assert_eq!(report["performance_status"], "UNAVAILABLE", "{report}");
        assert_eq!(
            report["reason_code"], "POLYMARKET_FILL_FEE_EXCEEDS_PLANNING_BOUND",
            "{report}"
        );
        let failure = &report["native_fill_fee_failure"];
        assert_eq!(failure["reason"], report["reason_code"]);
        assert_eq!(failure["last_qty"], "0.000480");
        assert_eq!(failure["last_px"], "0.4000");
        assert_eq!(failure["instrument_id"], IDS[0]);
        assert!(!failure["client_order_id"].as_str().unwrap().is_empty());
        eprintln!(
            "stream fee failure: status={}, reason={}, native_inputs={failure}",
            report["performance_status"], report["reason_code"]
        );
    }

    #[test]
    fn latency_deferred_fee_failure_only_at_native_end_is_unavailable() {
        let mut session = session();
        for half_second in 2..=4 {
            session
                .push(quote(half_second * SECOND / 2, "0.000480"))
                .unwrap();
        }
        let report = session.finish().unwrap();
        // All three source batches completed; only end() advances the deferred arrival.
        assert_eq!(report["completed_timestamp_batches"], 3);
        assert_eq!(
            report["native_ended_at_ns"],
            (2 * SECOND + LATENCY).to_string()
        );
        assert_fee_unavailable(&report);
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
            let error = session.observe_definition(&changed).unwrap_err();
            let expected = if field == "currency" {
                domain::DomainError::Invalid("polymarket_native_contract").to_string()
            } else {
                "PAPER_STREAM_DEFINITION_CHANGED_UNSUPPORTED".into()
            };
            assert_eq!(format!("{error:#}"), expected);
            let report = session.finish().unwrap();
            assert_eq!(report["performance_status"], "UNAVAILABLE");
            assert_eq!(report["reason_code"], expected);
            eprintln!(
                "stream frozen {field}: error={error:#}, status={}, reason={}",
                report["performance_status"], report["reason_code"]
            );
        }
    }
}
