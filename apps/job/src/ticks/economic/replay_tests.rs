//! Controlled local replay regressions, never historical market or performance evidence.
use super::*;
use nautilus_model::{
    data::TradeTick,
    enums::{AggressorSide, CurrencyType},
    identifiers::{InstrumentId, Symbol, TradeId},
    instruments::{CryptoPerpetual, CurrencyPair},
    types::{Price, Quantity},
};
use serde_json::json;
use std::path::PathBuf;

const SECOND: u64 = 1_000_000_000;
const BRIDGE: &str = "USDT0-USDC-SPOT.HYPERLIQUID";
const SPOT: &str = "BTC-USDC-SPOT.HYPERLIQUID";
const LATE: &str = "ETH-USD-PERP.HYPERLIQUID";

fn definitions() -> Vec<InstrumentAny> {
    let usdt0 = Currency::try_from_str("USDT0").unwrap_or_else(|| {
        let currency = Currency::new("USDT0", 8, 0, "USDT0", CurrencyType::Crypto);
        Currency::register(currency, false).unwrap();
        currency
    });
    let pair = |id: &str, base: Currency, precision, increment: &str| {
        InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from(id))
                .raw_symbol(Symbol::new("UNIT-ONLY"))
                .base_currency(base)
                .quote_currency(Currency::USDC())
                .price_precision(precision)
                .size_precision(4)
                .price_increment(Price::from(increment))
                .size_increment(Quantity::from("0.0001"))
                .min_notional(Money::from("10 USDC"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        )
    };
    vec![
        pair(BRIDGE, usdt0, 3, "0.001"),
        pair(SPOT, Currency::BTC(), 2, "0.01"),
        InstrumentAny::CryptoPerpetual(
            CryptoPerpetual::builder()
                .instrument_id(InstrumentId::from(LATE))
                .raw_symbol(Symbol::new("UNIT-ONLY-LATE"))
                .base_currency(Currency::ETH())
                .quote_currency(Currency::USD())
                .settlement_currency(Currency::USDC())
                .is_inverse(false)
                .price_precision(2)
                .size_precision(4)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.0001"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        ),
    ]
}

fn source(directory: &Path, after_positive: i8) -> (PathBuf, NativeTickEconomicRequestV1, Vec<u8>) {
    let instruments = definitions();
    let mut request = super::tests::request();
    request.research.input.instrument_ids = vec![BRIDGE.into(), SPOT.into(), LATE.into()];
    request.research.input.event_end_ns = counter(3 * DAY).unwrap();
    request.research.input.decision_cutoff_ns = request.research.input.event_end_ns;
    request.research.train_end_ns = counter(DAY).unwrap();
    request.research.validation_end_ns = counter(2 * DAY).unwrap();
    request.scenario.participation_fraction = "1".parse().unwrap();
    request.scenario.maximum_asset_weight = "1".parse().unwrap();
    request.scenario.spot_taker_fee_bps = "0".parse().unwrap();
    request.scenario.perp_taker_fee_bps = "0".parse().unwrap();
    request.scenario.extra_cost_bps = "0".parse().unwrap();
    request.scenario.decision_interval_ns = counter(10 * SECOND).unwrap();
    request.scenario.base_latency_ns = counter(SECOND).unwrap();
    request.scenario.jitter_min_ns = counter(0).unwrap();
    request.scenario.jitter_max_ns = counter(0).unwrap();
    request.fx_instrument_ids = vec![BRIDGE.into()];
    let mut rows = Vec::new();
    let mut append = |id: &str, event, price: &str, side| {
        rows.push(TradeTick::new(
            InstrumentId::from(id),
            Price::from(price),
            Quantity::from("10000.0000"),
            side,
            TradeId::from(format!("UNIT-{}", rows.len()).as_str()),
            event.into(),
            event.into(),
        ));
    };
    for event in [
        10 * SECOND,
        20 * SECOND,
        30 * SECOND,
        40 * SECOND,
        50 * SECOND,
        60 * SECOND,
        DAY + 10 * SECOND,
        2 * DAY + 10 * SECOND,
    ] {
        append(BRIDGE, event, "1.003", AggressorSide::Buy);
        append(BRIDGE, event, "1.003", AggressorSide::Sell);
        // Bridge grids precede sparse spot events: replacing pending targets at every
        // intervening grid would postpone each attempt forever (lag is one second).
        let event = event + SECOND / 2;
        let price = if event < DAY { "100.00" } else { "200.00" };
        if event < 40 * SECOND || after_positive > 0 {
            append(SPOT, event, price, AggressorSide::Buy);
        } else if after_positive < 0 {
            append(SPOT, event, price, AggressorSide::Sell);
        } else {
            append(SPOT, event, price, AggressorSide::Buy);
            append(SPOT, event, price, AggressorSide::Sell);
        }
    }
    // Present in the full source, absent from the clipped research view.
    append(LATE, 2 * DAY + SECOND, "100.00", AggressorSide::Buy);
    rows.sort_by_key(|row| (row.ts_event, row.instrument_id));
    let output = directory.join("source");
    let mut builder = cohort_cache::Builder::new_flow(&output, 100, 100, 1_000_000).unwrap();
    for row in rows {
        builder.push(row).unwrap();
    }
    let manifest = builder.seal("fixed-source", 0, 3 * DAY).unwrap();
    builder
        .publish(
            manifest,
            json!({"status":"IMPORTED","availability":"EVENT_TIME_PROXY_UNVERIFIED",
                "source_revision":"fixed-source","event_start_ns":"0","event_end_ns":(3 * DAY).to_string(),
                "missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0,
                "instrument_source":{"currencies":crate::catalog::currencies::definitions(&instruments),"instruments":instruments}}),
        )
        .unwrap();
    let catalog = output.join("catalog");
    // A real test-source event after the late instrument first appears; no invented prior rates.
    let rate =
        json!({"instrument_id":LATE,"event_ns":(2 * DAY + 2 * SECOND).to_string(),"rate":"0"});
    std::fs::write(catalog.join("funding-rates.jsonl"), format!("{rate}\n")).unwrap();
    std::fs::write(
        catalog.join("funding-rates.manifest.json"),
        serde_json::to_vec(&json!({"source_revision":"fixed-source","start_ns":"0",
            "end_ns":(3 * DAY).to_string(),"instruments":[LATE],"rows":1,"missing_instruments":[]}))
        .unwrap(),
    )
    .unwrap();
    let prepared = directory.join("prepared");
    cohort_cache::prepare_views(&catalog, &request.research, &prepared, 1_000_000).unwrap();
    (prepared, request, crate::ticks::tests::model(7))
}

#[test]
fn clipped_zero_cohort_asset_keeps_identity_and_unstarted_collateral_without_fake_events() {
    let directory = tempfile::tempdir().unwrap();
    let (prepared, mut request, wasm) = source(directory.path(), 0);
    request.research.clip_end(counter(2 * DAY).unwrap());
    let result = replay(&prepared.join("research/catalog"), &request, &wasm).unwrap();
    assert_eq!(result.coverage.instrument_ids.len(), 3);
    let late = result
        .assets
        .iter()
        .find(|asset| asset.instrument_id == LATE)
        .unwrap();
    assert_eq!(late.cohorts.get(), 0);
    assert_eq!(late.decisions.get(), 0);
    assert_eq!(late.execution_attempts.get(), 0);
    assert_eq!(result.days.len(), 2);
    assert_eq!(
        decimal(result.initial_usdc_after_cost.as_ref().unwrap()).unwrap(),
        Decimal::from(3009)
    );
    assert!(result.days.iter().all(|day| {
        day.equity_usdt
            .as_ref()
            .is_some_and(|equity| decimal(equity).unwrap() == Decimal::from(3000))
    }));
    assert_eq!(result.native_fills.get(), 2);
    assert_eq!(decimal(&result.funding_usdc).unwrap(), Decimal::ZERO);
}

#[test]
fn negative_or_zero_score_schedules_flat_then_native_future_fill_removes_later_price_exposure() {
    for score in [-1, 0] {
        let directory = tempfile::tempdir().unwrap();
        let (prepared, request, wasm) = source(directory.path(), score);
        let result = replay(&prepared.join("independent/catalog"), &request, &wasm).unwrap();
        assert_eq!(result.assets.len(), 3);
        assert_eq!(result.days.len(), 3);
        assert_eq!(
            result.native_fills.get(),
            2,
            "one delayed native buy and one delayed native close"
        );
        assert_eq!(result.native_rejections.get(), 0);
        if result.target_asof_ns.is_none() {
            assert!(result.targets.is_empty());
            assert!(result.target_cash_weight.is_none());
            assert!(result
                .limitations
                .iter()
                .any(|reason| reason
                    == "TARGET_SNAPSHOT_UNAVAILABLE_NO_COMPLETE_COMMON_DECISION_CLOCK"));
        }
        assert_eq!(decimal(&result.funding_usdc).unwrap(), Decimal::ZERO);
        assert_eq!(
            decimal(&result.initial_conversion_cost_usdc).unwrap(),
            Decimal::ZERO
        );
        assert!(result
            .targets
            .iter()
            .all(|target| decimal(&target.weight).unwrap().is_zero()));
        assert!(
            result.days.iter().all(|day| {
                day.equity_usdt
                    .as_ref()
                    .is_some_and(|equity| decimal(equity).unwrap() == Decimal::from(3000))
            }),
            "the price doubled only after the score had flattened at a later cohort"
        );
    }
}
