//! Controlled fee policies exercise source binding, not historical market fees.
#[path = "../../../tests/support/execution_models.rs"]
mod models;
#[path = "../../../tests/support/catalog_metadata.rs"]
mod support;
use contracts::{SchemaV1, catalogs::*, research::DataOrigin, science::*};

fn source() -> FrozenMakerTakerFeesV1 {
    FrozenMakerTakerFeesV1 {
        schema_version: SchemaV1,
        source_reference: "Synthetic original venue schedule independent of settings".into(),
        source_version: "fixture-fee-source-1".into(),
        origin: DataOrigin::Fixture,
        venue: "SIM".into(),
        currency: "USD".into(),
        currency_precision: 2,
        available_at_ns: support::count(0),
        valid_from_ns: support::count(0),
        valid_until_ns: support::count(600_000_000_000),
        rates: vec![NativeFeeRateV1 {
            instrument_id: "EUR/USD.SIM".into(),
            maker: "-0.0002".parse().unwrap(),
            taker: "0.001".parse().unwrap(),
        }],
    }
}

fn settings() -> NativeSimulationSettingsV1 {
    NativeSimulationSettingsV1 {
        schema_version: SchemaV1,
        multi_currency_spot_cash: None,
        base_currency: "USD".into(),
        starting_capital: "1000".parse().unwrap(),
        account_kind: NativeAccountKind::Margin,
        leverage: "1".parse().unwrap(),
        fee_model: models::fee(),
        fill_model: models::fill(),
        latency_model: models::latency(1),
        snapshot_interval_ms: 1000,
        exposure_tolerance: "0.000001".parse().unwrap(),
        fee_rates: vec![NativeFeeRateV1 {
            instrument_id: "EUR/USD.SIM".into(),
            maker: "-0.0002".parse().unwrap(),
            taker: "0.001".parse().unwrap(),
        }],
    }
}

#[test]
fn source_required_even_when_settings_or_old_instrument_fields_claim_zero() {
    let mut metadata = support::metadata();
    metadata.universe.instrument_definitions[0]["CurrencyPair"]["quote_currency"] = "USD".into();
    let mut settings = settings();
    assert!(domain::catalogs::execution_fees(&metadata, &settings).is_err());
    metadata.universe.instrument_definitions[0]["CurrencyPair"]["maker_fee"] = "0".into();
    metadata.universe.instrument_definitions[0]["CurrencyPair"]["taker_fee"] = "0".into();
    assert!(domain::catalogs::execution_fees(&metadata, &settings).is_err());
    metadata.execution_fees = Some(source());
    domain::catalogs::execution_fees(&metadata, &settings).unwrap();
    settings.fee_rates[0].taker = "0".parse().unwrap();
    assert!(domain::catalogs::execution_fees(&metadata, &settings).is_err());
}

#[test]
fn frozen_source_identity_window_and_unknown_origin_fail_closed() {
    let selection = support::metadata().quality.datasets.remove(0).selection;
    let original = source();
    domain::catalogs::maker_taker_fee_selection(&original, &selection).unwrap();
    for mutate in [
        |s: &mut FrozenMakerTakerFeesV1| s.available_at_ns = support::count(60_000_000_001),
        |s: &mut FrozenMakerTakerFeesV1| s.valid_from_ns = support::count(60_000_000_001),
        |s: &mut FrozenMakerTakerFeesV1| s.valid_until_ns = selection_cutoff(),
        |s: &mut FrozenMakerTakerFeesV1| s.source_reference.clear(),
        |s: &mut FrozenMakerTakerFeesV1| s.source_version.clear(),
        |s: &mut FrozenMakerTakerFeesV1| s.venue = "OTHER".into(),
        |s: &mut FrozenMakerTakerFeesV1| s.origin = DataOrigin::LegacyUnknown,
        |s: &mut FrozenMakerTakerFeesV1| s.rates.push(s.rates[0].clone()),
    ] {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert!(domain::catalogs::maker_taker_fee_selection(&changed, &selection).is_err());
    }
}

fn selection_cutoff() -> contracts::DbCounter {
    support::count(300_000_000_000)
}
