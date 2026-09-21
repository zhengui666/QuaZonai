//! Explicitly synthetic Polymarket fixtures. Native types and engines remain authoritative.
#![allow(dead_code)]
use contracts::{portfolio::*, science::*, SchemaV1};
use nautilus_model::{
    data::{Bar, BarType, InstrumentClose},
    enums::{AssetClass, InstrumentCloseType},
    identifiers::{InstrumentId, Symbol},
    instruments::{BinaryOption, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use std::{path::Path, str::FromStr};

pub const IDS: [&str; 2] = ["fixture-event-101.POLYMARKET", "fixture-event-102.POLYMARKET"];
pub const STEP: u64 = 60_000_000_000;

pub fn fee_model() -> NativeModelRefV1 {
    NativeModelRefV1::NautilusPolymarket {
        schema_version: SchemaV1,
        upstream_class: NAUTILUS_POLYMARKET_FEE_CLASS.into(),
        upstream_version: NAUTILUS_EXECUTION_VERSION.into(),
        parameters: NautilusFeeParametersV1 {},
    }
}

pub fn instruments(rate: &str, expiration: u64) -> Vec<InstrumentAny> {
    IDS.iter().enumerate().map(|(index, name)| {
        let token = (101 + index).to_string();
        let binary = BinaryOption::builder()
            .instrument_id(InstrumentId::from_str(name).unwrap())
            .raw_symbol(Symbol::new(&token))
            .asset_class(AssetClass::Alternative)
            .currency(Currency::from_str("pUSD").unwrap())
            .activation_ns(0_u64.into()).expiration_ns(expiration.into())
            .price_precision(4).size_precision(6)
            .price_increment(Price::from("0.0001"))
            .size_increment(Quantity::from("0.000001"))
            .ts_event(0_u64.into()).ts_init(0_u64.into()).build().unwrap();
        let mut value = serde_json::to_value(InstrumentAny::BinaryOption(binary)).unwrap();
        value["BinaryOption"]["info"] = serde_json::json!({
            "condition_id": "fixture-event", "token_id": token,
            "fee_schedule": {"rate": rate.parse::<f64>().unwrap(), "exponent": 1,
                "rebateRate": 0.2, "takerOnly": true},
            "source_reference": "SYNTHETIC_NATIVE_REGRESSION"
        });
        serde_json::from_value(value).unwrap()
    }).collect()
}

pub fn write_catalog(root: &Path, rows: u32, rate: &str, expiration: u64, slope: bool) {
    let catalog = ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, Some(4096), None, None).unwrap();
    catalog.write_instruments(instruments(rate, expiration)).unwrap();
    for (index, id) in IDS.iter().enumerate() {
        let kind = BarType::from_str(&format!("{id}-1-MINUTE-LAST-EXTERNAL")).unwrap();
        let records = (1..=rows).map(|minute| {
            // Distinct deterministic series below one, without retrospective terminal prices.
            let price = if slope {
                let tick = if index == 0 { minute % 100 } else { (minute * 3) % 100 };
                format!("0.{:04}", 4000 + tick)
            } else { "0.4000".into() };
            let price = Price::from(price.as_str());
            Bar::new_checked(kind, price, price, price, price, Quantity::from("10000000.000000"),
                (u64::from(minute) * STEP).into(), (u64::from(minute) * STEP + 1).into()).unwrap()
        }).collect::<Vec<_>>();
        catalog.write_to_parquet(&records, None, None, None).unwrap();
    }
}

pub fn settle(root: &Path, expiration: u64, available: u64, payouts: [&str; 2]) {
    let catalog = ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, None, None, None).unwrap();
    let closes = IDS.iter().zip(payouts).map(|(id, price)| InstrumentClose::new(
        InstrumentId::from_str(id).unwrap(), Price::from(price), InstrumentCloseType::ContractExpired,
        expiration.into(), available.into())).collect::<Vec<_>>();
    catalog.write_to_parquet(&closes, None, None, None).unwrap();
}

pub fn settings(original: &mut NativeSimulationSettingsV1, rate: &str, expiration: u64) {
    original.base_currency = "pUSD".into();
    original.account_kind = NativeAccountKind::Cash;
    original.fee_model = fee_model();
    original.fee_rates = instruments(rate, expiration).iter().map(|instrument| {
        let value = serde_json::to_value(instrument).unwrap();
        NativeFeeRateV1 {
            instrument_id: value["BinaryOption"]["id"].as_str().unwrap().into(),
            maker: "0".parse().unwrap(),
            taker: domain::prediction::planning_fee(&value["BinaryOption"]).unwrap(),
        }
    }).collect();
}
