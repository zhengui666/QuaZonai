//! Actual preparation and native replay from separate, explicitly synthetic fee evidence.
#![cfg(feature = "catalog-prepare")]
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;

use contracts::{
    catalogs::{FrozenMakerTakerFeesV1, RuntimeCatalogMetadataV1, MAKER_TAKER_FEES_FILE},
    execution::NativeDatasetSelectionV1,
    science::{NativeSimulationRequestV1, NativeSimulationResultV1},
    Id,
};
use nautilus_model::types::Money;
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    str::FromStr,
};

struct Fixture {
    original: tempfile::TempDir,
    inputs: tempfile::TempDir,
    request: NativeSimulationRequestV1,
    source: FrozenMakerTakerFeesV1,
    fee_file: PathBuf,
    declaration: Value,
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

impl Fixture {
    fn new() -> Self {
        let (original, request) = market::market("0.001", 20);
        let inputs = tempfile::tempdir().unwrap();
        let fee_file = inputs.path().join("independent fee policy; literal.json");
        // Detach the fixture's original independent policy before preparation:
        // the original native catalog now contains instruments and prices only.
        fs::rename(original.path().join(MAKER_TAKER_FEES_FILE), &fee_file).unwrap();
        let source = serde_json::from_slice(&fs::read(&fee_file).unwrap()).unwrap();
        let instant = |n: u64| chrono::DateTime::from_timestamp_nanos(n as i64);
        let end = request.selection.event_end_ns.get();
        let declaration = json!({
            "schema_version": 1, "registered_ref": "synthetic-independent-fees",
            "native_snapshot_ref": "synthetic-selected-partition", "storage_version": "fixture-v1",
            "provider_kind": "NAUTILUS_CATALOG", "data_kind": "BAR", "partition": "DISCOVERY",
            "event_start": instant(0), "event_end": instant(end), "available_through": instant(end),
            "origin": "FIXTURE", "pit_status": "UNVERIFIED", "revision_policy": "AS_KNOWN_THEN",
            "provenance_reference": "Synthetic preparation and native execution regression only",
            "availability_provenance": "Synthetic clock; not historical market availability",
            "universe": {
                "name": "Synthetic independent fees", "calendar_ref": "fixture-calendar", "calendar_version": "1",
                "selection_asof": instant(0), "has_historical_membership": false,
                "coverage_start": instant(0), "coverage_end": instant(end),
                "membership": request.settings.fee_rates.iter().map(|rate| json!({
                    "instrument_id": rate.instrument_id, "valid_from": instant(0),
                    "valid_until": null, "available_at": instant(0), "groups": null
                })).collect::<Vec<_>>()
            }
        });
        Self {
            original,
            inputs,
            request,
            source,
            fee_file,
            declaration,
        }
    }

    fn output(&self) -> PathBuf {
        self.inputs.path().join("prepared")
    }

    fn run(&self, explicit_fee: bool) -> Output {
        let declaration = self.inputs.path().join("declaration.json");
        let selection = self.inputs.path().join("selection.json");
        fs::write(&declaration, serde_json::to_vec(&self.declaration).unwrap()).unwrap();
        fs::write(
            &selection,
            serde_json::to_vec(&NativeDatasetSelectionV1 {
                dataset_revision_id: Id::new(),
                selection: self.request.selection.clone(),
                settlements: Vec::new(),
            })
            .unwrap(),
        )
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_catalog-prepare"));
        command
            .env_clear()
            .arg("--catalog")
            .arg(self.original.path())
            .arg("--declaration")
            .arg(declaration)
            .arg("--selection")
            .arg(selection)
            .arg("--output")
            .arg(self.output());
        if explicit_fee {
            command.arg("--fee-source").arg(&self.fee_file);
        }
        command.output().unwrap()
    }

    fn simulate(&self) -> Output {
        native::command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                self.output().join("catalog").as_os_str(),
            ],
            &self.request,
        )
    }
}

#[test]
fn explicit_original_fee_file_binds_new_catalog_and_enables_actual_native_replay() {
    let f = Fixture::new();
    let original = files(f.original.path());
    let original_fees = fs::read(&f.fee_file).unwrap();
    let result = f.run(true);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(metadata.execution_fees.as_ref(), Some(&f.source));
    domain::catalogs::metadata(&metadata, chrono::Utc::now()).unwrap();
    let copied: FrozenMakerTakerFeesV1 = serde_json::from_slice(
        &fs::read(f.output().join("catalog").join(MAKER_TAKER_FEES_FILE)).unwrap(),
    )
    .unwrap();
    assert_eq!(copied, f.source);
    assert_eq!(files(f.original.path()), original);
    assert_eq!(fs::read(&f.fee_file).unwrap(), original_fees);
    let replay = f.simulate();
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let result: NativeSimulationResultV1 = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(result.native_version, "0.67.0");
    assert!(result.orders.get() >= 4);
    assert_eq!(
        result.summary.get("orders.open").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        result.summary.get("orders.inflight").map(String::as_str),
        Some("0")
    );
    let mut nonzero_fees = 0;
    for value in result.canonical_result["fills"].as_array().unwrap() {
        let fill = &value["event"]["Filled"];
        assert_eq!(fill["liquidity_side"], "TAKER");
        let quantity = Decimal::from_str(fill["last_qty"].as_str().unwrap()).unwrap();
        let price = Decimal::from_str(fill["last_px"].as_str().unwrap()).unwrap();
        // Independently declared fixture policy: 0.1% of each fill's USD
        // notional, rounded to cents half-even by the official 0.67 model.
        let expected = (quantity * price * Decimal::from_str("0.001").unwrap())
            .round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven);
        let actual = Money::from_str(fill["commission"].as_str().unwrap()).unwrap();
        assert_eq!(actual.currency.code.as_str(), "USD");
        assert_eq!(actual.as_decimal(), expected);
        nonzero_fees += usize::from(expected > Decimal::ZERO);
    }
    assert!(nonzero_fees >= 4);
    assert_eq!(files(f.original.path()), original);
    assert_eq!(fs::read(&f.fee_file).unwrap(), original_fees);
}

#[test]
fn data_only_preparation_does_not_manufacture_request_fees() {
    let f = Fixture::new();
    let result = f.run(false);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert!(metadata.execution_fees.is_none());
    assert!(!f
        .output()
        .join("catalog")
        .join(MAKER_TAKER_FEES_FILE)
        .exists());
    let replay = f.simulate();
    assert!(!replay.status.success());
    assert!(String::from_utf8_lossy(&replay.stderr).contains("MAKER_TAKER_FEE_SOURCE_REQUIRED"));
}

#[test]
fn invalid_scope_window_or_conflicting_declaration_cannot_publish_fee_binding() {
    for mutation in 0..5 {
        let mut f = Fixture::new();
        match mutation {
            0 => f.source.available_at_ns = market::count(1),
            1 => f.source.valid_until_ns = f.request.selection.decision_cutoff_ns,
            2 => f.source.currency_precision = 3,
            3 => {
                f.source.rates.pop();
            }
            _ => {
                let mut declared = f.source.clone();
                declared.rates[0].taker = "0.002".parse().unwrap();
                f.declaration["execution_fees"] = serde_json::to_value(declared).unwrap();
            }
        }
        fs::write(&f.fee_file, serde_json::to_vec(&f.source).unwrap()).unwrap();
        let original = files(f.original.path());
        let original_fees = fs::read(&f.fee_file).unwrap();
        let result = f.run(true);
        assert!(
            !result.status.success(),
            "mutation {mutation} unexpectedly prepared"
        );
        assert!(!f.output().join("catalog-metadata.json").exists());
        assert_eq!(files(f.original.path()), original);
        assert_eq!(fs::read(&f.fee_file).unwrap(), original_fees);
    }
}

#[test]
fn existing_catalog_source_is_preserved_and_conflicting_explicit_source_is_rejected() {
    for conflict in [false, true] {
        let mut f = Fixture::new();
        fs::copy(&f.fee_file, f.original.path().join(MAKER_TAKER_FEES_FILE)).unwrap();
        if conflict {
            f.source.rates[0].taker = "0.002".parse().unwrap();
            fs::write(&f.fee_file, serde_json::to_vec(&f.source).unwrap()).unwrap();
        }
        let original = files(f.original.path());
        let result = f.run(conflict);
        assert_eq!(
            result.status.success(),
            !conflict,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(files(f.original.path()), original);
        assert_eq!(f.output().join("catalog-metadata.json").exists(), !conflict);
        if !conflict {
            let metadata: RuntimeCatalogMetadataV1 =
                serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(metadata.execution_fees, Some(f.source));
        }
    }
}
