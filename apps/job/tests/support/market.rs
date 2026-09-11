//! Native synthetic catalog fixture. Never REAL data or a production initialization path.
#![allow(dead_code)]
use contracts::{portfolio::AllocationTargetV1, science::*, DbCounter, SchemaV1};
use nautilus_model::{
    data::{Bar, BarType},
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, InstrumentAny},
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

pub fn market(fee: &str, rows_per_asset: u32) -> (tempfile::TempDir, NativeSimulationRequestV1) {
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
    for (name, base, multiplier) in [("EUR/USD.SIM", "EUR", 1.0), ("GBP/USD.SIM", "GBP", 2.0)] {
        let id = InstrumentId::from_str(name).unwrap();
        let instrument = CurrencyPair::builder()
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
            .unwrap();
        catalog
            .write_instruments(vec![InstrumentAny::CurrencyPair(instrument)])
            .unwrap();
        let kind = BarType::from_str(&format!("{name}-1-MINUTE-LAST-EXTERNAL")).unwrap();
        types.push(kind.to_string());
        let bars = (1..=rows_per_asset)
            .map(|i| {
                let price = multiplier + f64::from(i) * 0.001;
                Bar::new_checked(
                    kind,
                    Price::from(format!("{price:.5}").as_str()),
                    Price::from(format!("{:.5}", price + 0.0002).as_str()),
                    Price::from(format!("{:.5}", price - 0.0002).as_str()),
                    Price::from(format!("{price:.5}").as_str()),
                    Quantity::from("10000000"),
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
        schema_version: SchemaV1,
        selection,
        settings: NativeSimulationSettingsV1 {
            schema_version: SchemaV1,
            base_currency: "USD".into(),
            starting_capital: "1000000".parse().unwrap(),
            account_kind: NativeAccountKind::Margin,
            leverage: "1".parse().unwrap(),
            insert_latency_ns: count(1_000_000),
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

pub fn forecast_request(simulation: &NativeSimulationRequestV1) -> NativeForecastRequestV1 {
    NativeForecastRequestV1 {
        schema_version: SchemaV1,
        selection: simulation.selection.clone(),
        parameters: NativeForecastParametersV1 {
            schema_version: SchemaV1,
            fast_period: 2,
            slow_period: 3,
            label_horizon_observations: 2,
            total_fuel: count(100_000_000),
        },
    }
}
pub fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) {body}))")).unwrap()
}
