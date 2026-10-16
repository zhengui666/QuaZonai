//! Synthetic definition histories; timestamps do not certify source availability.
#[path = "../../../tests/support/execution_models.rs"]
mod models;
#[path = "../../../tests/support/catalog_metadata.rs"]
mod support;

use contracts::{science::*, SchemaV1};
use domain::catalogs::{instrument_version_at, instrument_versions};
use serde_json::{json, Value};

fn versions() -> Vec<Value> {
    let mut original = support::metadata()
        .universe
        .instrument_definitions
        .remove(0);
    let payload = &mut original["CurrencyPair"];
    payload["quote_currency"] = "USD".into();
    payload["maker_fee"] = "0.001".into();
    payload["taker_fee"] = "0.002".into();
    payload["price_precision"] = 5.into();
    payload["size_increment"] = "1".into();
    let mut update = original.clone();
    update["CurrencyPair"]["price_increment"] = "0.00010".into();
    update["CurrencyPair"]["min_price"] = "0.00100".into();
    update["CurrencyPair"]["max_price"] = "100.00000".into();
    update["CurrencyPair"]["ts_event"] = 149_000_000_000_u64.into();
    update["CurrencyPair"]["ts_init"] = 150_000_000_000_u64.into();
    vec![original, update]
}

#[test]
fn chains_preserve_originals_and_resolve_only_already_available_versions() {
    let originals = versions();
    let chains = instrument_versions(&originals).unwrap();
    let chain = &chains["EUR/USD.SIM"];
    assert_eq!(chain, &originals.iter().collect::<Vec<_>>());
    assert_eq!(
        instrument_version_at(chain, 149_999_999_999).unwrap().1["price_increment"],
        "0.00001"
    );
    assert_eq!(
        instrument_version_at(chain, 150_000_000_000).unwrap().1["price_increment"],
        "0.00010"
    );
    for (field, value) in [
        ("quote_currency", json!("EUR")),
        ("taker_fee", json!("0.003")),
        ("price_precision", json!(4)),
        ("size_increment", json!("10")),
        ("expiration_ns", json!(500)),
        ("info", json!({"revised":true})),
        ("ts_event", json!(151_000_000_000_u64)),
        ("ts_init", json!(0)),
        ("price_increment", json!("0")),
        ("min_price", json!("101")),
    ] {
        let mut changed = originals.clone();
        changed[1]["CurrencyPair"][field] = value;
        assert!(instrument_versions(&changed).is_err(), "field {field}");
    }
    let mut reversed = originals.clone();
    reversed.reverse();
    assert!(instrument_versions(&reversed).is_err());
    assert!(instrument_versions(&vec![originals[0].clone(); 257]).is_err());
    let mut unchanged = originals[0].clone();
    unchanged["CurrencyPair"]["ts_init"] = 1.into();
    assert!(instrument_versions(&[originals[0].clone(), unchanged]).is_err());
    let mut missing = originals;
    missing[0]["CurrencyPair"]
        .as_object_mut()
        .unwrap()
        .remove("ts_init");
    assert!(instrument_versions(&missing).is_err());
}

#[test]
fn metadata_keeps_the_pre_boundary_baseline_and_the_update_at_start() {
    let mut metadata = support::metadata();
    let boundary = metadata.quality.datasets[0].selection.event_start_ns.get();
    let mut history = versions();
    history[1]["CurrencyPair"]["ts_event"] = (boundary - 1).into();
    history[1]["CurrencyPair"]["ts_init"] = boundary.into();
    metadata.universe.instrument_definitions = history.clone();
    domain::catalogs::metadata(&metadata, support::instant(600)).unwrap();
    let mut older = history[0].clone();
    older["CurrencyPair"]["price_increment"] = "0.00005".into();
    older["CurrencyPair"]["ts_event"] = (boundary - 1).into();
    older["CurrencyPair"]["ts_init"] = (boundary - 1).into();
    metadata.universe.instrument_definitions.insert(1, older);
    assert!(domain::catalogs::metadata(&metadata, support::instant(600)).is_err());
    // Without an earlier original, the first definition at start remains a baseline.
    metadata.universe.instrument_definitions = vec![history.remove(1)];
    domain::catalogs::metadata(&metadata, support::instant(600)).unwrap();
}

#[test]
fn metadata_fees_and_slippage_share_one_chain_without_promoting_evidence() {
    let mut metadata = support::metadata();
    metadata.universe.instrument_definitions = versions();
    domain::catalogs::metadata(&metadata, support::instant(600)).unwrap();
    assert_eq!(metadata.origin, contracts::research::DataOrigin::Fixture);
    assert_eq!(
        metadata.pit_status,
        contracts::research::PitStatus::Unverified
    );
    let settings = NativeSimulationSettingsV1 {
        multi_currency_spot_cash: None,
        schema_version: SchemaV1,
        base_currency: "USD".into(),
        starting_capital: "1000".parse().unwrap(),
        account_kind: NativeAccountKind::Margin,
        leverage: "1".parse().unwrap(),
        fill_model: models::fill(),
        fee_model: models::fee(),
        latency_model: models::latency(1),
        snapshot_interval_ms: 1000,
        exposure_tolerance: "0.000001".parse().unwrap(),
        fee_rates: vec![NativeFeeRateV1 {
            instrument_id: "EUR/USD.SIM".into(),
            maker: "0.001".parse().unwrap(),
            taker: "0.002".parse().unwrap(),
        }],
    };
    domain::catalogs::execution_fees(&metadata, &settings).unwrap();
    for (available, tick) in [(140_000_000_000, "0.00001"), (160_000_000_000, "0.00010")] {
        let mut reference = NativePortfolioSlippageReferenceV1 {
            instrument_id: "EUR/USD.SIM".into(),
            currency: "USD".into(),
            event_ns: support::count(120_000_000_000),
            available_ns: support::count(available),
            close_price: "1".parse().unwrap(),
            price_increment: tick.parse().unwrap(),
        };
        domain::catalogs::portfolio_slippage_sources(&metadata, &[reference.clone()], available)
            .unwrap();
        if available < 150_000_000_000 {
            assert!(
                domain::catalogs::portfolio_slippage_sources(
                    &metadata,
                    &[reference.clone()],
                    300_000_000_000
                )
                .is_err(),
                "old BAR availability cannot justify an obsolete tick at a later decision"
            );
        }
        reference.price_increment = "0.1".parse().unwrap();
        assert!(
            domain::catalogs::portfolio_slippage_sources(&metadata, &[reference], available)
                .is_err()
        );
    }
    let mut future = metadata.clone();
    future.universe.instrument_definitions[1]["CurrencyPair"]["ts_init"] =
        301_000_000_000_u64.into();
    assert!(domain::catalogs::metadata(&future, support::instant(600)).is_err());
    let mut late_original = metadata;
    late_original.universe.instrument_definitions.remove(1);
    late_original.universe.instrument_definitions[0]["CurrencyPair"]["ts_init"] =
        70_000_000_000_u64.into();
    domain::catalogs::metadata(&late_original, support::instant(600)).unwrap();
    domain::catalogs::execution_fees(&late_original, &settings).unwrap();
    let ids = vec!["EUR/USD.SIM".into()];
    assert!(domain::prediction::target_window(
        &late_original.universe.instrument_definitions,
        &ids,
        69_000_000_000,
        100_000_000_000
    )
    .is_err());
    domain::prediction::target_window(
        &late_original.universe.instrument_definitions,
        &ids,
        70_000_000_000,
        100_000_000_000,
    )
    .unwrap();
}
