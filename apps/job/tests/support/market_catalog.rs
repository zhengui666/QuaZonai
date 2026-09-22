//! Shared native synthetic Parquet construction, never REAL data.
//! Callers supply their existing explicit execution-model fixtures.
use contracts::{
    portfolio::{AllocationTargetV1, NativeModelRefV1},
    science::*,
    DbCounter, SchemaV1,
};
use nautilus_model::{
    data::{Bar, BarType},
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, Equity, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use rust_decimal::Decimal;
use std::str::FromStr;

pub const INTERVAL_NS: u64 = 60_000_000_000;
pub fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
pub fn instant(n: u64) -> DbCounter {
    count(n * INTERVAL_NS + 1)
}

pub fn market_direction(
    fee: &str,
    rows_per_asset: u32,
    direction: f64,
    volume: &str,
    equities: bool,
    models: [NativeModelRefV1; 3],
) -> (tempfile::TempDir, NativeSimulationRequestV1) {
    let [fill_model, fee_model, latency_model] = models;
    let directory = tempfile::tempdir().unwrap();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    let usd = Currency::from_str("USD").unwrap();
    let mut types = Vec::new();
    let mut rates = Vec::new();
    let mut targets = Vec::new();
    let names = if equities {
        ["AAA.SIM", "BBB.SIM"]
    } else {
        ["EUR/USD.SIM", "GBP/USD.SIM"]
    };
    for (name, base, multiplier) in [(names[0], "EUR", 1.0), (names[1], "GBP", 2.0)] {
        let id = InstrumentId::from_str(name).unwrap();
        let instrument = if equities {
            InstrumentAny::Equity(
                Equity::builder()
                    .instrument_id(id)
                    .raw_symbol(Symbol::new_checked(name.split('.').next().unwrap()).unwrap())
                    .currency(usd)
                    .price_precision(5)
                    .price_increment(Price::from("0.00001"))
                    .lot_size(Quantity::from("1"))
                    .maker_fee(Decimal::ZERO)
                    .taker_fee(Decimal::from_str(fee).unwrap())
                    .margin_init(Decimal::ONE)
                    .margin_maint(Decimal::ONE)
                    .ts_event(0_u64.into())
                    .ts_init(0_u64.into())
                    .build()
                    .unwrap(),
            )
        } else {
            InstrumentAny::CurrencyPair(
                CurrencyPair::builder()
                    .instrument_id(id)
                    .raw_symbol(Symbol::new_checked(name.split('.').next().unwrap()).unwrap())
                    .base_currency(Currency::from_str(base).unwrap())
                    .quote_currency(usd)
                    .price_precision(5)
                    .size_precision(0)
                    .price_increment(Price::from("0.00001"))
                    .size_increment(Quantity::from("1"))
                    .maker_fee(Decimal::ZERO)
                    .taker_fee(Decimal::from_str(fee).unwrap())
                    .margin_init(Decimal::ONE)
                    .margin_maint(Decimal::ONE)
                    .ts_event(0_u64.into())
                    .ts_init(0_u64.into())
                    .build()
                    .unwrap(),
            )
        };
        catalog.write_instruments(vec![instrument]).unwrap();
        let kind = BarType::from_str(&format!("{name}-1-MINUTE-LAST-EXTERNAL")).unwrap();
        types.push(kind.to_string());
        let bars = (1..=rows_per_asset)
            .map(|i| {
                let price = multiplier + direction * f64::from(i) * 0.001;
                Bar::new_checked(
                    kind,
                    Price::from(format!("{price:.5}").as_str()),
                    Price::from(format!("{:.5}", price + 0.0002).as_str()),
                    Price::from(format!("{:.5}", price - 0.0002).as_str()),
                    Price::from(format!("{price:.5}").as_str()),
                    Quantity::from(volume),
                    (u64::from(i) * INTERVAL_NS).into(),
                    (u64::from(i) * INTERVAL_NS + 1).into(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
        rates.push(NativeFeeRateV1 {
            instrument_id: name.into(),
            maker: "0".parse().unwrap(),
            taker: fee.parse().unwrap(),
        });
        targets.push(AllocationTargetV1 {
            instrument_id: name.into(),
            currency: "USD".into(),
            weight: "0.4".parse().unwrap(),
        });
    }
    let end = count((u64::from(rows_per_asset) + 1) * INTERVAL_NS);
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: types,
        event_start_ns: DbCounter::ZERO,
        event_end_ns: end,
        decision_cutoff_ns: end,
        maximum_rows: rows_per_asset * 2,
    };
    let mut later = targets.clone();
    later[0].weight = "0.2".parse().unwrap();
    later[1].weight = "0.6".parse().unwrap();
    let request = NativeSimulationRequestV1 {
        settlements: Vec::new(),
        schema_version: SchemaV1,
        selection,
        settings: NativeSimulationSettingsV1 {
            schema_version: SchemaV1,
            base_currency: "USD".into(),
            starting_capital: "1000000".parse().unwrap(),
            account_kind: NativeAccountKind::Margin,
            leverage: "1".parse().unwrap(),
            fill_model,
            fee_model,
            latency_model,
            snapshot_interval_ms: 60_000,
            exposure_tolerance: "0.00001".parse().unwrap(),
            fee_rates: rates,
        },
        target_points: vec![
            NativeTargetPointV1 {
                schema_version: SchemaV1,
                asof_ns: instant(2),
                valid_until_ns: end,
                targets,
                cash_weight: "0.2".parse().unwrap(),
            },
            NativeTargetPointV1 {
                schema_version: SchemaV1,
                asof_ns: instant(7),
                valid_until_ns: end,
                targets: later,
                cash_weight: "0.2".parse().unwrap(),
            },
        ],
    };
    (directory, request)
}
