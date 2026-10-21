//! Small actual official-engine replay; this is synthetic regression data.
use contracts::{
    catalog_ensemble::*, portfolio::*, recorded_fees::*, science::*, DbCounter, SchemaV1,
};
use nautilus_model::{
    data::{Bar, BarType},
    types::{Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde_json::json;
use std::{fs, path::Path, process::Command, str::FromStr};

const MINUTE: u64 = 60_000_000_000;
fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn settings() -> NativeSimulationSettingsV1 {
    NativeSimulationSettingsV1 {
        multi_currency_spot_cash: None,
        schema_version: SchemaV1,
        base_currency: "pUSD".into(),
        starting_capital: "3000".parse().unwrap(),
        account_kind: NativeAccountKind::Margin,
        leverage: "1".parse().unwrap(),
        exposure_tolerance: "0.000001".parse().unwrap(),
        snapshot_interval_ms: 60_000,
        fee_rates: vec![],
        fee_model: NativeModelRefV1::PolymarketV2RecordedFees {
            schema_version: SchemaV1,
            native_version: NAUTILUS_EXECUTION_VERSION.into(),
            parameters: RecordedTradeFeeParametersV1 {},
        },
        fill_model: NativeModelRefV1::PolymarketV2RecordedLiquidity {
            schema_version: SchemaV1,
            native_version: NAUTILUS_EXECUTION_VERSION.into(),
            parameters: NautilusFillParametersV1 {
                prob_fill_on_limit: "1".parse().unwrap(),
                prob_slippage: "0".parse().unwrap(),
                random_seed: count(1),
            },
        },
        latency_model: NativeModelRefV1::NautilusStaticLatency {
            schema_version: SchemaV1,
            upstream_class: NAUTILUS_LATENCY_CLASS.into(),
            upstream_version: NAUTILUS_EXECUTION_VERSION.into(),
            parameters: NautilusLatencyParametersV1 {
                base_latency_ns: count(1),
                insert_latency_ns: count(1),
                update_latency_ns: count(1),
                cancel_latency_ns: count(1),
            },
        },
    }
}

fn model() -> Vec<u8> {
    wat::parse_str(r#"(module (global $flow (mut f64) (f64.const 0))
        (func (export "qz_set_feature_v2") (param $index i32) (param $value f64) (param i32 i64 i64)
            local.get $index i32.const 2 i32.eq if local.get $value global.set $flow end)
        (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) global.get $flow))"#).unwrap()
}

fn fixture(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let catalog_root = root.join("catalog");
    fs::create_dir(&catalog_root).unwrap();
    let catalog =
        ParquetDataCatalog::from_uri(catalog_root.to_str().unwrap(), None, None, None, None)
            .unwrap();
    for token in ["100", "200"] {
        let kind =
            BarType::from_str(&format!("{token}.POLYMARKET-1-MINUTE-LAST-EXTERNAL")).unwrap();
        let bars = (1..=4_u64)
            .map(|minute| {
                let value = if token == "100" {
                    0.4 + minute as f64 * 0.1
                } else {
                    0.6 - minute as f64 * 0.1
                };
                let price = Price::new(value, 6);
                Bar::new_checked(
                    kind,
                    price,
                    price,
                    price,
                    price,
                    Quantity::from("10000.000000"),
                    (minute * MINUTE).into(),
                    (minute * MINUTE).into(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
    }
    let mut parts = Vec::new();
    for (index, first) in [1_u64, 3].into_iter().enumerate() {
        let input = root.join(format!("part-{index}"));
        fs::create_dir(&input).unwrap();
        for token in ["100", "200"] {
            let records = (first..first + 2)
                .map(|minute| {
                    let buy = (token == "100") == (minute <= 2);
                    let tenths = if token == "100" {
                        4 + minute
                    } else {
                        6 - minute
                    };
                    RecordedTradeFeeV1 {
                        id: format!("original-{token}-{minute}"),
                        instrument_id: format!("{token}.POLYMARKET"),
                        ts_event_ns: count(minute * MINUTE - 1),
                        source_quantity: "10000".parse().unwrap(),
                        quantity: "10000".parse().unwrap(),
                        fee_usdc: "0.1".parse().unwrap(),
                        usdc_amount: format!("{tenths}000").parse().unwrap(),
                        price: format!("0.{tenths}").parse().unwrap(),
                        maker_direction: if buy { "SELL" } else { "BUY" }.into(),
                        taker_direction: if buy { "BUY" } else { "SELL" }.into(),
                    }
                })
                .collect();
            fs::write(
                input.join(format!("{token}.POLYMARKET.json")),
                serde_json::to_vec(&RecordedTradeFeesV1 {
                    schema_version: SchemaV1,
                    source: POLYMARKET_V2_SOURCE.into(),
                    revision: "synthetic-regression".into(),
                    currency: "pUSD".into(),
                    records,
                })
                .unwrap(),
            )
            .unwrap();
        }
        let archive = root.join(format!("part-{index}.tar.zst"));
        assert!(Command::new("tar")
            .arg("--zstd")
            .arg("-cf")
            .arg(&archive)
            .arg("-C")
            .arg(&input)
            .arg(".")
            .status()
            .unwrap()
            .success());
        parts.push(
            json!({ "source_start": format!("1970-01-01T00:{:02}:00+00:00", first - 1),
            "source_end_exclusive": format!("1970-01-01T00:{:02}:00+00:00", first + 1),
            "offline_lossless_archive": archive, "records": 4, "bars": 4 }),
        );
    }
    let manifest = root.join("original-cost-preparation.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&json!({ "parts": parts })).unwrap(),
    )
    .unwrap();
    (catalog_root, manifest)
}

fn request() -> NativeCatalogEnsembleRequestV1 {
    NativeCatalogEnsembleRequestV1 {
        schema_version: SchemaV1,
        event_start_ns: count(1),
        evaluation_start_ns: count(2 * MINUTE),
        event_end_ns: count(4 * MINUTE + 1),
        decision_cutoff_ns: count(4 * MINUTE + 1000),
        source: POLYMARKET_V2_SOURCE.into(),
        revision: "synthetic-regression".into(),
        modeled_lag_ns: count(1),
        score_max_age_ns: Some(count(MINUTE)),
        score_threshold: 0.0,
        members: vec![
            NativeCatalogEnsembleMemberV1 {
                weight: "0.6".parse().unwrap(),
                total_fuel: None,
            },
            NativeCatalogEnsembleMemberV1 {
                weight: "0.4".parse().unwrap(),
                total_fuel: None,
            },
        ],
        gross_exposure: "0.5".parse().unwrap(),
        target_ttl_ns: count(1_000_000),
        settings: settings(),
    }
}

#[test]
fn native_stream_retains_one_account_and_fees_while_dynamic_market_changes() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, manifest) = fixture(directory.path());
    let request = request();
    let result = super::replay(
        &catalog,
        &manifest,
        &request,
        &[model(), model()],
        &crate::factors::catalog_factor_scope(&catalog).unwrap(),
    )
    .unwrap();
    assert_eq!(result.catalog_series, count(2));
    assert_eq!(result.observed_markets, count(2));
    assert_eq!(result.bars, count(8));
    assert_eq!(result.source_records, count(8));
    assert_eq!(result.consumed_fuel, vec![None, None]);
    assert_eq!(result.simulation.consumed_target_points, count(3));
    assert!(result.simulation.orders.get() >= 3);
    let fills = &result
        .simulation
        .recorded_fee_replay
        .as_ref()
        .unwrap()
        .fills;
    assert!(fills.iter().all(|fill| fill.bar_end_ns.get() >= 2 * MINUTE
        && fill.fill_ns.get() > fill.bar_end_ns.get() + 1));
    assert!(fills
        .iter()
        .any(|fill| fill.instrument_id == "100.POLYMARKET" && fill.order_side == "BUY"));
    assert!(fills
        .iter()
        .any(|fill| fill.instrument_id == "100.POLYMARKET" && fill.order_side == "SELL"));
    assert!(fills
        .iter()
        .any(|fill| fill.instrument_id == "200.POLYMARKET" && fill.order_side == "BUY"));
    assert!(fills.iter().all(|fill| fill.commission.is_positive()));
}

#[test]
fn long_modeled_lag_and_pending_order_keep_original_pool_across_source_partitions() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, manifest) = fixture(directory.path());
    let mut request = request();
    request.modeled_lag_ns = count(MINUTE);
    request.decision_cutoff_ns = count(10 * MINUTE);
    request.target_ttl_ns = count(6 * MINUTE);
    if let NativeModelRefV1::NautilusStaticLatency { parameters, .. } =
        &mut request.settings.latency_model
    {
        parameters.base_latency_ns = count(MINUTE + 1);
        parameters.insert_latency_ns = count(MINUTE + 1);
    }
    let result = super::replay(
        &catalog,
        &manifest,
        &request,
        &[model(), model()],
        &crate::factors::catalog_factor_scope(&catalog).unwrap(),
    )
    .unwrap();
    assert_eq!(result.bars, count(8));
    assert_eq!(result.simulation.consumed_target_points, count(1));
    let report = result.simulation.recorded_fee_replay.unwrap();
    assert!(report.allocation_rule.contains("source BAR"));
    assert!(!report.fills.is_empty());
    assert!(report
        .fills
        .iter()
        .all(|fill| fill.bar_end_ns == count(2 * MINUTE)
            && fill.fill_ns.get() > 4 * MINUTE
            && fill.instrument_id == "100.POLYMARKET"));
}

#[test]
fn modeled_observations_after_decision_cutoff_never_reach_member_models() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, manifest) = fixture(directory.path());
    let mut request = request();
    request.modeled_lag_ns = count(2 * MINUTE);
    request.decision_cutoff_ns = count(4 * MINUTE + 1);
    let result = super::replay(
        &catalog,
        &manifest,
        &request,
        &[model(), model()],
        &crate::factors::catalog_factor_scope(&catalog).unwrap(),
    )
    .unwrap();
    assert_eq!(result.bars, count(4));
    assert_eq!(result.source_records, count(8));
    assert_eq!(result.simulation.consumed_target_points, count(1));
}

#[test]
fn compact_account_preserves_existing_snapshot_and_tolerance_constraints() {
    let directory = tempfile::tempdir().unwrap();
    for (snapshot, tolerance) in [(0, "0.000001"), (86_400_001, "0.000001"), (60_000, "0.002")] {
        let mut request = request();
        request.settings.snapshot_interval_ms = snapshot;
        request.settings.exposure_tolerance = tolerance.parse().unwrap();
        let error = super::replay(
            directory.path(),
            &directory.path().join("not-opened.json"),
            &request,
            &[model(), model()],
            &[],
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("CATALOG_ENSEMBLE_CONDITIONAL_ACCOUNT"));
    }
}

fn omit_source_market(root: &Path, part: usize) {
    let input = root.join(format!("part-{part}"));
    fs::remove_file(input.join("200.POLYMARKET.json")).unwrap();
    let archive = root.join(format!("part-{part}.tar.zst"));
    assert!(Command::new("tar")
        .arg("--zstd")
        .arg("-cf")
        .arg(archive)
        .arg("-C")
        .arg(input)
        .arg(".")
        .status()
        .unwrap()
        .success());
    let manifest = root.join("original-cost-preparation.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["parts"][part]["records"] = json!(2);
    value["parts"][part]["bars"] = json!(2);
    fs::write(manifest, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[test]
fn entire_catalog_market_missing_from_self_consistent_source_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, manifest) = fixture(directory.path());
    omit_source_market(directory.path(), 0);
    omit_source_market(directory.path(), 1);
    let error = super::replay(
        &catalog,
        &manifest,
        &request(),
        &[model(), model()],
        &crate::factors::catalog_factor_scope(&catalog).unwrap(),
    )
    .err()
    .expect("an omitted market was accepted");
    assert!(error
        .to_string()
        .contains("CATALOG_ENSEMBLE_SOURCE_SCOPE_INCOMPLETE"));
}

#[test]
fn one_market_missing_only_one_source_period_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, manifest) = fixture(directory.path());
    omit_source_market(directory.path(), 1);
    let error = super::replay(
        &catalog,
        &manifest,
        &request(),
        &[model(), model()],
        &crate::factors::catalog_factor_scope(&catalog).unwrap(),
    )
    .err()
    .expect("a missing market period was accepted");
    assert!(error
        .to_string()
        .contains("CATALOG_ENSEMBLE_SOURCE_SCOPE_INCOMPLETE"));
}

#[test]
fn complete_source_scope_counts_boundary_groups_and_ignores_outside_window_series() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, manifest) = fixture(directory.path());
    let native =
        ParquetDataCatalog::from_uri(catalog.to_str().unwrap(), None, None, None, None).unwrap();
    let kind = BarType::from_str("300.POLYMARKET-1-MINUTE-LAST-EXTERNAL").unwrap();
    let price = Price::from("0.500000");
    native
        .write_to_parquet(
            &[Bar::new_checked(
                kind,
                price,
                price,
                price,
                price,
                Quantity::from("1.000000"),
                (6 * MINUTE).into(),
                (6 * MINUTE).into(),
            )
            .unwrap()],
            None,
            None,
            None,
        )
        .unwrap();
    let mut request = request();
    request.event_end_ns = count(3 * MINUTE);
    request.decision_cutoff_ns = count(3 * MINUTE);
    let result = super::replay(
        &catalog,
        &manifest,
        &request,
        &[model(), model()],
        &crate::factors::catalog_factor_scope(&catalog).unwrap(),
    )
    .unwrap();
    assert_eq!(result.catalog_series, count(3));
    assert_eq!(result.observed_markets, count(2));
    assert_eq!(result.bars, count(4));
}

#[test]
fn caller_scope_cannot_shorten_the_actual_catalog() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, manifest) = fixture(directory.path());
    let mut scope = crate::factors::catalog_factor_scope(&catalog).unwrap();
    scope.pop();
    let error = super::replay(&catalog, &manifest, &request(), &[model(), model()], &scope)
        .err()
        .expect("a shortened scope was accepted");
    assert!(error.to_string().contains("CATALOG_ENSEMBLE_SCOPE_CHANGED"));
}
