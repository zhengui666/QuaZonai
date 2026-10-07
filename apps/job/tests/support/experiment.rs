//! Controlled second-frequency, irregular synthetic source/target fixture.
#![allow(dead_code)]
use contracts::{research::DataPartition, science::*, DbCounter, Id, SchemaV1};
use nautilus_model::{
    data::{Bar, BarType},
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, Equity, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use std::str::FromStr;

use super::execution_models;
pub const SECOND: u64 = 1_000_000_000;
/// Same controlled policy as feature_policy(), for the actual native Rust
/// CompileFeatureModel boundary in Worker/OCI integration tests.
pub const SOURCE: &str = r#"
static mut VALUE: f64 = 0.0;
#[no_mangle]
pub extern "C" fn qz_set_feature_v2(index: i32, value: f64, mask: i32, _event_ns: i64, _available_ns: i64) {
    if index == 0 { unsafe { VALUE = if mask == 0 { value } else { 0.05 }; } }
}
#[no_mangle]
pub extern "C" fn qz_predict_v2(_decision_ns: i64, _event_ns: i64, _target_index: i32, _ordinal: i32, _feature_count: i32) -> f64 {
    unsafe { VALUE }
}
"#;
pub fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
pub fn event(ordinal: usize) -> u64 {
    (ordinal as u64 + 1 + if ordinal >= 20 { 3 } else { 0 }) * SECOND
}

pub fn fixture(
    future_prices: bool,
) -> (
    tempfile::TempDir,
    NativeExperimentEvaluationRequestV1,
    Vec<FeatureObservationsV1>,
) {
    market_fixture(future_prices, FixtureMarket::Equity, 1)
}

#[derive(Clone, Copy)]
pub enum FixtureMarket {
    Equity,
    CryptoCurrencyPair,
    CryptoUsdtCurrencyPair,
}

pub fn market_fixture(
    future_prices: bool,
    market: FixtureMarket,
    availability_delay_ns: u64,
) -> (
    tempfile::TempDir,
    NativeExperimentEvaluationRequestV1,
    Vec<FeatureObservationsV1>,
) {
    let root = tempfile::tempdir().unwrap();
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    let id = InstrumentId::from(match market {
        FixtureMarket::Equity => "TARGET.SIM",
        FixtureMarket::CryptoCurrencyPair => "BTC/USD.SIM",
        FixtureMarket::CryptoUsdtCurrencyPair => "BTC/USDT.SIM",
    });
    let quote = match market {
        FixtureMarket::Equity | FixtureMarket::CryptoCurrencyPair => Currency::USD(),
        FixtureMarket::CryptoUsdtCurrencyPair => Currency::USDT(),
    };
    let instrument = match market {
        FixtureMarket::Equity => InstrumentAny::Equity(
            Equity::builder()
                .instrument_id(id)
                .raw_symbol(Symbol::from("TARGET"))
                .currency(Currency::USD())
                .price_precision(2)
                .price_increment(Price::from("0.01"))
                .lot_size(Quantity::from("1"))
                .maker_fee(rust_decimal::Decimal::ZERO)
                .taker_fee(rust_decimal::Decimal::from_str("0.001").unwrap())
                .margin_init(rust_decimal::Decimal::ONE)
                .margin_maint(rust_decimal::Decimal::ONE)
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        ),
        FixtureMarket::CryptoCurrencyPair | FixtureMarket::CryptoUsdtCurrencyPair => {
            InstrumentAny::CurrencyPair(
                CurrencyPair::builder()
                    .instrument_id(id)
                    .raw_symbol(Symbol::from(format!("BTC/{quote}").as_str()))
                    .base_currency(Currency::from_str("BTC").unwrap())
                    .quote_currency(quote)
                    .price_precision(2)
                    .size_precision(3)
                    .price_increment(Price::from("0.01"))
                    .size_increment(Quantity::from("0.001"))
                    .maker_fee(rust_decimal::Decimal::ZERO)
                    .taker_fee(rust_decimal::Decimal::from_str("0.001").unwrap())
                    .margin_init(rust_decimal::Decimal::ONE)
                    .margin_maint(rust_decimal::Decimal::ONE)
                    .ts_event(0_u64.into())
                    .ts_init(0_u64.into())
                    .build()
                    .unwrap(),
            )
        }
    };
    catalog.write_instruments(vec![instrument]).unwrap();
    let bar_type = BarType::from_str(&format!("{id}-1-SECOND-LAST-EXTERNAL")).unwrap();
    let bars = (0..40)
        .map(|ordinal| {
            let price = (match market {
                FixtureMarket::Equity => 100.0,
                FixtureMarket::CryptoCurrencyPair | FixtureMarket::CryptoUsdtCurrencyPair => {
                    20_000.0
                }
            }) + ordinal as f64
                * if future_prices && ordinal >= 20 {
                    0.7
                } else {
                    0.1
                };
            Bar::new_checked(
                bar_type,
                Price::from(format!("{price:.2}").as_str()),
                Price::from(format!("{:.2}", price + 0.01).as_str()),
                Price::from(format!("{:.2}", price - 0.01).as_str()),
                Price::from(format!("{price:.2}").as_str()),
                Quantity::from(match market {
                    FixtureMarket::Equity => "1000000",
                    FixtureMarket::CryptoCurrencyPair | FixtureMarket::CryptoUsdtCurrencyPair => {
                        "1000000.000"
                    }
                }),
                event(ordinal).into(),
                (event(ordinal) + availability_delay_ns).into(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    let feature_schema = vec![
        FeatureDefinitionV1 {
            feature_key: "independent_measurement".into(),
            source_ref: "frozen-source-A-v1".into(),
            source_key: "SERIES-X".into(),
            availability: FeatureAvailabilityV1::Observed,
            max_age_ns: Some(count(2 * SECOND + 1)),
        },
        FeatureDefinitionV1 {
            feature_key: "lagged_measurement".into(),
            source_ref: "frozen-source-B-v1".into(),
            source_key: "SERIES-Y".into(),
            availability: FeatureAvailabilityV1::ModeledLag {
                lag_ns: count(2 * SECOND),
            },
            max_age_ns: Some(count(10 * SECOND)),
        },
        FeatureDefinitionV1 {
            feature_key: "later_measurement".into(),
            source_ref: "frozen-source-C-v1".into(),
            source_key: "SERIES-Z".into(),
            availability: FeatureAvailabilityV1::Observed,
            max_age_ns: None,
        },
    ];
    let row = |feature_index: u16,
               time: u64,
               available: Option<u64>,
               sequence: u64,
               value: Option<f64>| FeatureObservationV1 {
        feature_index,
        event_ns: count(time * SECOND),
        observed_available_ns: available.map(|time| count(time * SECOND + 1)),
        sequence: count(sequence),
        value,
        missing_reason: value.is_none().then(|| "SOURCE_GAP".into()),
    };
    let observations = vec![
        row(0, 10, Some(12), 1, Some(0.2)),
        row(0, 10, Some(12), 2, Some(0.25)),
        row(0, 11, Some(13), 3, Some(0.0)),
        row(0, 13, Some(16), 4, Some(0.6)),
        row(0, 16, Some(16), 5, None),
        row(0, 30, Some(30), 6, Some(0.4)),
        row(1, 10, None, 1, Some(0.4)),
        row(2, 30, Some(30), 1, Some(0.55)),
    ];
    let parts = vec![FeatureObservationsV1 {
        schema_version: SchemaV1,
        partition: DataPartition::Validation,
        feature_schema: feature_schema.clone(),
        observations,
    }];
    let request = NativeExperimentEvaluationRequestV1 {
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec![bar_type.to_string()],
            event_start_ns: count(0),
            event_end_ns: count(event(39) + SECOND),
            decision_cutoff_ns: count(event(39) + availability_delay_ns.max(SECOND)),
            maximum_rows: 100,
        },
        instrument_id: id.to_string(),
        feature_schema,
        split_policy: contracts::research::SplitPolicyV1 {
            schema_version: SchemaV1,
            kind: contracts::research::SplitKind::WalkForward,
            train_size: count(8),
            test_size: count(3),
            step_size: Some(count(3)),
            group_count: None,
            test_group_count: None,
            purge_observations: count(2),
            embargo_observations: count(1),
            label_horizon_observations: Some(count(2)),
            interval_validation_required: true,
            sealed_revision_id: Id::new(),
        },
        label_horizon_observations: 2,
        total_fuel: count(10_000_000),
        target_ttl_ns: count(10 * SECOND),
        decision_output: ExperimentDecisionOutputV1::TargetWeight,
        settings: NativeSimulationSettingsV1 {
            multi_currency_spot_cash: None,
            schema_version: SchemaV1,
            base_currency: quote.to_string(),
            // Native whole-share / .001-coin flooring can leave almost one
            // lot. The largest fixture lots cost 127.31 / 20.02731 quote
            // units; a 200-unit initial budget remains ample after native fees.
            starting_capital: "1000000".parse().unwrap(),
            account_kind: match market {
                FixtureMarket::Equity => NativeAccountKind::Cash,
                FixtureMarket::CryptoCurrencyPair | FixtureMarket::CryptoUsdtCurrencyPair => {
                    NativeAccountKind::Margin
                }
            },
            leverage: "1".parse().unwrap(),
            fee_model: execution_models::fee(),
            fill_model: execution_models::fill(),
            latency_model: execution_models::latency(1_000_000),
            snapshot_interval_ms: 1000,
            exposure_tolerance: "0.0002".parse().unwrap(),
            fee_rates: vec![NativeFeeRateV1 {
                instrument_id: id.to_string(),
                maker: "0".parse().unwrap(),
                taker: "0.001".parse().unwrap(),
            }],
        },
    };
    (root, request, parts)
}

pub fn module(prefix: &str, setter: &str, body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        "(module {prefix}
        (func (export \"qz_set_feature_v2\") (param i32 f64 i32 i64 i64) {setter})
        (func (export \"qz_predict_v2\") (param i64 i64 i32 i32 i32) (result f64) {body}))"
    ))
    .unwrap()
}

pub fn feature_policy() -> Vec<u8> {
    module("(global $value (mut f64) (f64.const 0))", "local.get 0 i32.eqz if local.get 2 i32.eqz if local.get 1 global.set $value else f64.const 0.05 global.set $value end end", "global.get $value")
}
