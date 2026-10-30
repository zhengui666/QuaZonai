//! Shared controlled native spot source; never production evidence.
#![allow(dead_code)]
#[path = "../../../../tests/support/execution_models.rs"]
mod models;
use contracts::{
    DbCounter, Id, SchemaV1,
    portfolio::{AllocationTargetV1, NativeModelRefV1},
    science::*,
    spot_cash::*,
    spot_fees::*,
};
use nautilus_model::{
    data::{Bar, BarType},
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use rust_decimal::Decimal;
use std::str::FromStr;
pub const MINUTE: u64 = 60_000_000_000;
pub const START: u64 = 86_400_000_000_000;
pub fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
pub fn money(s: &str) -> contracts::DecimalValue {
    s.parse().unwrap()
}
pub fn fixture(
    rows: u32,
) -> (
    tempfile::TempDir,
    NativeSimulationRequestV1,
    Vec<job::spot_cash_capture::ClosedBarSourceRow>,
) {
    fixture_with_delay(rows, 0)
}
pub fn fixture_with_delay(
    rows: u32,
    delay: u64,
) -> (
    tempfile::TempDir,
    NativeSimulationRequestV1,
    Vec<job::spot_cash_capture::ClosedBarSourceRow>,
) {
    let dir = tempfile::tempdir().unwrap();
    let id = InstrumentId::from_str("BTC-SPOT.HYPERLIQUID").unwrap();
    let base = Currency::from_str("BTC").unwrap();
    let quote = Currency::from_str("USDC").unwrap();
    let instrument = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(id)
            .raw_symbol(Symbol::new("BTC"))
            .base_currency(base)
            .quote_currency(quote)
            .price_precision(2)
            .size_precision(4)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.0001"))
            // Venue charges come from the separate original FrozenSpotFeeSchedule.
            .margin_init(Decimal::ONE)
            .margin_maint(Decimal::ONE)
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    let kind = BarType::from_str("BTC-SPOT.HYPERLIQUID-1-MINUTE-LAST-EXTERNAL").unwrap();
    let bars: Vec<_> = (0..rows)
        .map(|i| {
            let t = START + u64::from(i) * MINUTE;
            Bar::new_checked(
                kind,
                Price::from("100.00"),
                Price::from("100.00"),
                Price::from("100.00"),
                Price::from("100.00"),
                Quantity::from("100.0000"),
                t.into(),
                (t + delay).into(),
            )
            .unwrap()
        })
        .collect();
    let sources = bars
        .iter()
        .enumerate()
        .map(|(i, bar)| job::spot_cash_capture::ClosedBarSourceRow {
            event_time: NativeSpotBarEventTimeV1::CloseExclusive,
            source_row_key: format!("CONTROLLED_CLOSED_BAR:{i}"),
            bar_open_ns: count(bar.ts_event.as_u64() - MINUTE),
            bar_close_ns: count(bar.ts_event.as_u64()),
            native: *bar,
        })
        .collect();
    let catalog =
        ParquetDataCatalog::from_uri(dir.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    catalog.write_instruments(vec![instrument]).unwrap();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    let end = START + u64::from(rows) * MINUTE;
    let rules = [SpotFeeOrderSideV1::Buy, SpotFeeOrderSideV1::Sell]
        .into_iter()
        .flat_map(|side| {
            [SpotFeeLiquidityV1::Maker, SpotFeeLiquidityV1::Taker]
                .into_iter()
                .map(move |liquidity| {
                    let is_buy = side == SpotFeeOrderSideV1::Buy;
                    let currency = if is_buy { base } else { quote };
                    FrozenSpotFeeRuleV1 {
                        instrument_id: id.to_string(),
                        order_side: side,
                        liquidity,
                        rate: money(if liquidity == SpotFeeLiquidityV1::Maker {
                            "0.0004"
                        } else {
                            "0.0007"
                        }),
                        basis: if is_buy {
                            SpotFeeBasisV1::BaseQuantity
                        } else {
                            SpotFeeBasisV1::QuoteNotional
                        },
                        fee_currency: if is_buy {
                            SpotFeeCurrencyV1::Base
                        } else {
                            SpotFeeCurrencyV1::Quote
                        },
                        currency_code: currency.to_string(),
                        currency_precision: currency.precision,
                        rounding: SpotFeeRoundingV1::HalfEvenCurrencyPrecision,
                    }
                })
        })
        .collect();
    let schedule = FrozenSpotFeeScheduleV1 {
        schema_version: SchemaV1,
        source: FrozenSpotFeeSourceV1 {
            status: SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability,
            source_ref: "CONTROLLED_PUBLIC_RATE_SCENARIO".into(),
            source_uri: job::spot_fees::HYPERLIQUID_PUBLIC_FEE_SOURCE.into(),
            observed_at_ns: count(START),
            assumptions:
                "Controlled rows; applicability, tier, fee-token choices and rounding unverified"
                    .into(),
        },
        valid_from_ns: count(START),
        valid_until_ns: count(end),
        rules,
    };
    let settings = NativeSimulationSettingsV1 {
        schema_version: SchemaV1,
        base_currency: "USDC".into(),
        starting_capital: money("1000"),
        account_kind: NativeAccountKind::Cash,
        leverage: money("1"),
        multi_currency_spot_cash: Some(NativeSpotCashPolicyV1 {
            schema_version: SchemaV1,
            mode: NativeSpotCashModeV1::MultiCurrencyCash,
            report_currency: "USDC".into(),
            price_method: NativeSpotPriceMethodV1::ClosedBarClose,
            allowed_instrument_ids: vec![id.to_string()],
            returns_policy: ReportCurrencyDailyPolicyV1::FreshSimulationNoExternalFlows,
            daily_sampling: NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries,
            maximum_price_age_ns: count(2 * MINUTE),
        }),
        fee_model: NativeModelRefV1::FrozenSpotFeeScenario {
            schema_version: SchemaV1,
            native_version: "0.67.0".into(),
            parameters: FrozenSpotFeeScenarioParametersV1 {
                acceptance: SpotCashFeeAcceptanceV1::PublicRateScenarioUnverifiedApplicability,
                schedule,
            },
        },
        fill_model: models::fill(),
        latency_model: models::latency(1),
        snapshot_interval_ms: 60_000,
        exposure_tolerance: money("0.001"),
        fee_rates: vec![NativeFeeRateV1 {
            instrument_id: id.to_string(),
            maker: money("0.0004"),
            taker: money("0.0007"),
        }],
    };
    let point = |time, weight: &str, cash: &str| NativeTargetPointV1 {
        schema_version: SchemaV1,
        rebalance_instrument_ids: None,
        asof_ns: count(time),
        valid_until_ns: count(time + 2 * MINUTE),
        targets: vec![AllocationTargetV1 {
            instrument_id: id.to_string(),
            currency: "USDC".into(),
            weight: money(weight),
        }],
        cash_weight: money(cash),
    };
    let request = NativeSimulationRequestV1 {
        schema_version: SchemaV1,
        settlements: vec![],
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec![kind.to_string()],
            event_start_ns: count(START),
            event_end_ns: count(end),
            decision_cutoff_ns: count(end),
            maximum_rows: rows,
        },
        settings,
        target_points: vec![
            point(START, "0.5", "0.5"),
            point(START + 2 * MINUTE, "0", "1"),
        ],
    };
    (dir, request, sources)
}
#[cfg(feature = "hyperliquid-offline")]
pub fn source_fixture() -> (tempfile::TempDir, NativeSimulationRequestV1) {
    source_fixture_with_rows(6, false)
}

#[cfg(feature = "hyperliquid-offline")]
pub fn source_fixture_with_rows(
    rows: u32,
    trend: bool,
) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    source_fixture_with_prices(rows, |index| {
        format!(
            "{:.2}",
            100.0 + if trend { index as f64 * 0.01 } else { 0.0 }
        )
    })
}

#[cfg(feature = "hyperliquid-offline")]
pub fn source_fixture_with_prices(
    rows: u32,
    price: impl Fn(u64) -> String,
) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    use contracts::spot_cash_source::*;
    use nautilus_hyperliquid::websocket::{messages::CandleData, parse::parse_ws_candle};
    let (original, mut request, _) = fixture(rows);
    let market = job::catalog::load_catalog(original.path(), &request.selection).unwrap();
    let mut instrument = market.series[0].instrument.clone();
    let InstrumentAny::CurrencyPair(pair) = &mut instrument else {
        unreachable!()
    };
    pair.raw_symbol = Symbol::new("@142");
    let bar_type = market.series[0].bar_type;
    let mut bundle = FrozenSpotCandleSourceV1 {
        schema_version: SchemaV1,
        capture_id: Id::new(),
        method: SpotCashSourceMethodV1::HyperliquidPublicRestCandleSnapshot,
        network: SpotCashSourceNetworkV1::Mainnet,
        native_version: "0.67.0".into(),
        historical_availability: SpotCashHistoricalAvailabilityV1::Unverified,
        instrument_definition: serde_json::to_value(&instrument).unwrap(),
        instrument_request_started_ns: count(1),
        instrument_received_ns: count(2),
        bar_type: bar_type.to_string(),
        responses: vec![],
        selected_rows: vec![],
    };
    let mut bars = Vec::new();
    for index in 0..u64::from(rows) {
        let open = START + index * MINUTE;
        let close = open + MINUTE;
        let received = close + 500_000_000;
        let price = price(index);
        let raw = serde_json::json!({"t":open/1_000_000,"T":close/1_000_000-1,"s":"@142","i":"1m","o":price,"h":price,"l":price,"c":price,"v":"100.0000","n":1});
        let candle: CandleData = serde_json::from_value(raw.clone()).unwrap();
        bars.push(parse_ws_candle(&candle, &instrument, &bar_type, received.into()).unwrap());
        bundle.responses.push(FrozenSpotCandleResponseV1 {
            sequence: count(index + 1),
            coin: "@142".into(),
            interval: "1m".into(),
            requested_start_ms: count(open / 1_000_000),
            requested_end_ms: count(close / 1_000_000 - 1),
            request_started_ns: count(close + 100_000_000),
            received_ns: count(received),
            response: serde_json::json!([raw]),
        });
        bundle.selected_rows.push(FrozenSpotCandleRowV1 {
            response_sequence: count(index + 1),
            source_row_index: DbCounter::ZERO,
        });
    }
    for target in &mut request.target_points {
        target.asof_ns = count(target.asof_ns.get() + MINUTE + 500_000_000);
        target.valid_until_ns = count(target.valid_until_ns.get() + MINUTE + 500_000_000);
    }
    request.selection.decision_cutoff_ns = count(START + u64::from(rows) * MINUTE + 500_000_000);
    // The usable price is the source close, not native ts_event (the open).
    request
        .settings
        .multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .maximum_price_age_ns = count(1_000_000_000);
    let NativeModelRefV1::FrozenSpotFeeScenario { parameters, .. } =
        &mut request.settings.fee_model
    else {
        unreachable!()
    };
    parameters.schedule.valid_until_ns = count(request.selection.decision_cutoff_ns.get() + MINUTE);
    let root = tempfile::tempdir().unwrap();
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    catalog.write_instruments(vec![instrument]).unwrap();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    std::fs::write(
        root.path().join(SPOT_CANDLE_SOURCE_FILE),
        serde_json::to_vec(&bundle).unwrap(),
    )
    .unwrap();
    (root, request)
}
