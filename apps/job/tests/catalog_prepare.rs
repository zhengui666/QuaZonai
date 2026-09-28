//! Real Parquet preparation over explicitly synthetic sources; never a PIT certification.
#![cfg(feature = "catalog-prepare")]
#[path = "support/market.rs"]
mod market;
#[path = "support/polymarket.rs"]
mod prediction;

use contracts::{
    catalogs::RuntimeCatalogMetadataV1,
    execution::NativeDatasetSelectionV1,
    research::{DataOrigin, PitStatus},
    science::NativeBarSelectionV1,
    DbCounter, Id, SchemaV1,
};
use nautilus_model::{
    data::{Bar, InstrumentClose},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn instant(value: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_nanos(value as i64)
}
fn native(root: &Path) -> ParquetDataCatalog {
    ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, Some(16), None, None).unwrap()
}

struct Fixture {
    directory: tempfile::TempDir,
    catalog: PathBuf,
    selection: NativeDatasetSelectionV1,
    declaration: Value,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let catalog = directory.path().join("source");
        fs::create_dir(&catalog).unwrap();
        prediction::write_catalog(&catalog, 5, "0", 1000 * prediction::STEP, false);
        let selection = NativeDatasetSelectionV1 {
            dataset_revision_id: Id::new(),
            settlements: Vec::new(),
            selection: NativeBarSelectionV1 {
                schema_version: SchemaV1,
                bar_types: vec![format!("{}-1-MINUTE-LAST-EXTERNAL", prediction::IDS[0])],
                event_start_ns: count(prediction::STEP),
                event_end_ns: count(4 * prediction::STEP),
                decision_cutoff_ns: count(5 * prediction::STEP),
                maximum_rows: 10,
            },
        };
        let declaration = json!({
            "schema_version": 1, "registered_ref": "synthetic-catalog",
            "native_snapshot_ref": "synthetic-selected-partition", "storage_version": "fixture-v1",
            "provider_kind": "NAUTILUS_CATALOG", "data_kind": "BAR", "partition": "DISCOVERY",
            "event_start": instant(prediction::STEP), "event_end": instant(4 * prediction::STEP),
            "available_through": instant(5 * prediction::STEP),
            "origin": "FIXTURE", "pit_status": "UNVERIFIED", "revision_policy": "AS_KNOWN_THEN",
            "provenance_reference": "SYNTHETIC_NATIVE_REGRESSION",
            "availability_provenance": "Synthetic timestamps, not evidence of historical availability",
            "universe": {
                "name": "Synthetic fixture", "calendar_ref": "synthetic-calendar", "calendar_version": "1",
                "selection_asof": instant(0), "has_historical_membership": false,
                "coverage_start": instant(0), "coverage_end": instant(1000 * prediction::STEP),
                "membership": prediction::IDS.iter().map(|id| json!({
                    "instrument_id": id, "valid_from": instant(0), "valid_until": null,
                    "available_at": instant(0), "groups": null
                })).collect::<Vec<_>>()
            }
        });
        Self {
            directory,
            catalog,
            selection,
            declaration,
        }
    }

    fn output(&self) -> PathBuf {
        self.directory.path().join("prepared")
    }

    fn run(&self) -> Output {
        let declaration = self.directory.path().join("declaration.json");
        let selection = self.directory.path().join("selection.json");
        fs::write(&declaration, serde_json::to_vec(&self.declaration).unwrap()).unwrap();
        fs::write(&selection, serde_json::to_vec(&self.selection).unwrap()).unwrap();
        Command::new(env!("CARGO_BIN_EXE_catalog-prepare"))
            .arg("--catalog")
            .arg(&self.catalog)
            .arg("--declaration")
            .arg(declaration)
            .arg("--selection")
            .arg(selection)
            .arg("--output")
            .arg(self.output())
            .output()
            .unwrap()
    }

    fn reject(&self) {
        let result = self.run();
        assert!(!result.status.success(), "invalid source was prepared");
        assert!(!self.output().join("catalog-metadata.json").exists());
        assert_eq!(
            String::from_utf8_lossy(&result.stderr).trim(),
            "QZ_CATALOG_PREPARATION_FAILED"
        );
    }

    fn change_original(&mut self, change: impl FnOnce(&mut Value)) {
        let data = job::catalog::load_catalog(&self.catalog, &self.selection.selection).unwrap();
        let mut definition = serde_json::to_value(&data.series[0].instrument).unwrap();
        change(&mut definition);
        let instrument: InstrumentAny = serde_json::from_value(definition).unwrap();
        let root = self.directory.path().join("changed-source");
        fs::create_dir(&root).unwrap();
        let catalog = native(&root);
        catalog.write_instruments(vec![instrument]).unwrap();
        catalog
            .write_to_parquet(&data.series[0].bars, None, None, None)
            .unwrap();
        self.catalog = root;
    }
}

#[test]
fn partition_readback_preserves_actual_rows_originals_and_unverified_status() {
    let f = Fixture::new();
    let result = f.run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(metadata.row_count.get(), 3);
    assert_eq!(metadata.origin, DataOrigin::Fixture);
    assert_eq!(metadata.pit_status, PitStatus::Unverified);
    assert!(!metadata.universe.has_historical_membership);
    assert_eq!(metadata.universe.instrument_definitions.len(), 1);
    assert!(metadata.quality.datasets[0].last_bar_notionals.is_some());
    domain::catalogs::metadata(&metadata, chrono::Utc::now()).unwrap();
    let mut catalog = native(&f.output().join("catalog"));
    let all_bars = catalog
        .query::<Bar>(None, None, None, None, None, true)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        all_bars.len(),
        3,
        "unselected rows must not be in the physical partition"
    );
    let instruments = catalog.instruments(None, None, None).unwrap();
    assert_eq!(instruments.len(), 1);
    assert_eq!(instruments[0].id().to_string(), prediction::IDS[0]);
    let source = job::catalog::load_catalog(&f.catalog, &f.selection.selection).unwrap();
    let copied =
        job::catalog::load_catalog(&f.output().join("catalog"), &f.selection.selection).unwrap();
    assert_eq!(source.series[0].bars, copied.series[0].bars);
    assert_eq!(
        serde_json::to_value(&source.series[0].instrument).unwrap(),
        serde_json::to_value(&copied.series[0].instrument).unwrap()
    );
    let published = fs::read(f.output().join("catalog-metadata.json")).unwrap();
    assert!(
        !f.run().status.success(),
        "existing output cannot be overwritten"
    );
    assert_eq!(
        published,
        fs::read(f.output().join("catalog-metadata.json")).unwrap()
    );
}

#[test]
fn original_future_definition_and_missing_historical_fee_are_rejected() {
    for mutation in 0..3 {
        let mut f = Fixture::new();
        f.change_original(|value| match mutation {
            0 => value["BinaryOption"]["ts_init"] = (2 * prediction::STEP).into(),
            1 => {
                value["BinaryOption"]["info"]
                    .as_object_mut()
                    .unwrap()
                    .remove("fee_schedule");
            }
            _ => value["BinaryOption"]["info"]["fee_schedule"]["exponent"] = 2.into(),
        });
        f.reject();
    }
}

#[test]
fn incomplete_declarations_cannot_acquire_fabricated_provenance_or_measurements() {
    for key in [
        "availability_provenance",
        "origin",
        "pit_status",
        "revision_policy",
    ] {
        let mut f = Fixture::new();
        f.declaration.as_object_mut().unwrap().remove(key);
        f.reject();
    }
    let mut f = Fixture::new();
    f.declaration["row_count"] = "999".into();
    f.reject();
    let mut f = Fixture::new();
    f.declaration["universe"]["instrument_definitions"] = json!([]);
    f.reject();
}

#[test]
fn settlement_siblings_and_original_closes_survive_without_leaking_sealed_values() {
    for sealed in [false, true] {
        let mut f = Fixture::new();
        let at = 3 * prediction::STEP;
        let available = 4 * prediction::STEP;
        prediction::settle(&f.catalog, at, available, ["1.0000", "0.0000"]);
        f.selection.settlements =
            prediction::settlement_groups(at, available, ["1.0000", "0.0000"]);
        if sealed {
            f.declaration["partition"] = "SEALED".into();
        }
        let result = f.run();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(metadata.universe.instrument_definitions.len(), 2);
        assert_eq!(metadata.quality.datasets[0].settlements.is_empty(), sealed);
        assert_eq!(
            metadata.quality.datasets[0].last_bar_notionals.is_none(),
            sealed
        );
        let mut catalog = native(&f.output().join("catalog"));
        assert_eq!(catalog.instruments(None, None, None).unwrap().len(), 2);
        assert_eq!(
            catalog
                .query::<InstrumentClose>(None, None, None, None, None, true)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            catalog
                .query::<Bar>(None, None, None, None, None, true)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .len(),
            3
        );
    }
}

#[test]
fn missing_original_settlement_sibling_is_not_synthesized() {
    let mut f = Fixture::new();
    f.change_original(|_| {});
    let at = 3 * prediction::STEP;
    let available = 4 * prediction::STEP;
    prediction::settle(&f.catalog, at, available, ["1.0000", "0.0000"]);
    f.selection.settlements = prediction::settlement_groups(at, available, ["1.0000", "0.0000"]);
    f.reject();
}

#[test]
fn generic_currency_pair_uses_the_same_preparation_without_prediction_metadata() {
    let mut f = Fixture::new();
    let (source, request) = market::market("0.001", 5);
    f.catalog = source.path().to_owned();
    f.selection.selection.bar_types = request.selection.bar_types[..1].to_vec();
    let original = job::catalog::load_catalog(&f.catalog, &f.selection.selection).unwrap();
    let mut member = f.declaration["universe"]["membership"][0].clone();
    member["instrument_id"] = original.series[0].instrument.id().to_string().into();
    f.declaration["universe"]["membership"] = json!([member]);
    let result = f.run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(metadata.row_count.get(), 3);
    assert!(metadata.universe.instrument_definitions[0]
        .get("CurrencyPair")
        .is_some());
}

#[test]
fn oversized_declaration_and_future_membership_are_rejected_before_publication() {
    let mut f = Fixture::new();
    f.declaration["availability_provenance"] = "x".repeat(1024 * 1024).into();
    f.reject();
    let mut f = Fixture::new();
    f.declaration["universe"]["membership"][0]["available_at"] =
        json!(instant(6 * prediction::STEP));
    f.reject();
}

fn versioned_fixture() -> Fixture {
    let mut f = Fixture::new();
    let data = job::catalog::load_catalog(&f.catalog, &f.selection.selection).unwrap();
    let root = f.directory.path().join("versioned-source");
    fs::create_dir(&root).unwrap();
    let catalog = native(&root);
    let mut versions = Vec::new();
    for instrument in prediction::instruments("0", 1000 * prediction::STEP) {
        for (time, tick) in [
            (0, "0.0001"),
            (prediction::STEP / 2, "0.0010"),
            (2 * prediction::STEP - 1000, "0.0100"),
            (4 * prediction::STEP + 10, "0.1000"),
            (6 * prediction::STEP, "0.0001"),
        ] {
            let mut value = serde_json::to_value(&instrument).unwrap();
            value["BinaryOption"]["ts_event"] = time.into();
            value["BinaryOption"]["ts_init"] = time.into();
            value["BinaryOption"]["price_increment"] = tick.into();
            versions.push(serde_json::from_value(value).unwrap());
        }
    }
    catalog.write_instruments(versions).unwrap();
    catalog
        .write_to_parquet(&data.series[0].bars, None, None, None)
        .unwrap();
    f.catalog = root;
    prediction::settle(
        &f.catalog,
        3 * prediction::STEP,
        4 * prediction::STEP,
        ["1.0000", "0.0000"],
    );
    f.selection.settlements = prediction::settlement_groups(
        3 * prediction::STEP,
        4 * prediction::STEP,
        ["1.0000", "0.0000"],
    );
    f
}

#[test]
fn physical_partition_keeps_causal_versions_and_siblings_without_future_definitions() {
    for sealed in [false, true] {
        let mut f = versioned_fixture();
        if sealed {
            f.declaration["partition"] = "SEALED".into();
        }
        let result = f.run();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            metadata.row_count.get(),
            3,
            "definition versions are not BAR rows"
        );
        assert_eq!(metadata.quality.datasets[0].instrument_ids.len(), 1);
        assert_eq!(metadata.universe.instrument_definitions.len(), 6);
        assert_eq!(metadata.origin, DataOrigin::Fixture);
        assert_eq!(metadata.pit_status, PitStatus::Unverified);
        assert_eq!(metadata.quality.datasets[0].settlements.is_empty(), sealed);
        assert_eq!(
            metadata.quality.datasets[0].last_bar_notionals.is_none(),
            sealed
        );
        let output = native(&f.output().join("catalog"));
        let published = output.instruments(None, None, None).unwrap();
        assert_eq!(published.len(), 6);
        for id in prediction::IDS {
            let times = published
                .iter()
                .filter(|v| v.id().to_string() == id)
                .map(|v| v.ts_init().as_u64())
                .collect::<Vec<_>>();
            assert_eq!(
                times,
                vec![
                    prediction::STEP / 2,
                    2 * prediction::STEP - 1000,
                    4 * prediction::STEP + 10
                ]
            );
        }
        let readback =
            job::catalog::load_catalog(&f.output().join("catalog"), &f.selection.selection)
                .unwrap();
        assert_eq!(readback.series[0].instrument_updates.len(), 2);
        assert_eq!(readback.series[0].bars.len(), 3);
        assert!(
            readback.series[0].instrument_updates[1].ts_event().as_u64()
                > f.selection.selection.event_end_ns.get()
        );
    }
}

#[test]
fn preparation_retains_an_original_update_received_exactly_at_selection_start() {
    let mut f = versioned_fixture();
    // Declaration instants are microsecond-aligned; original native times stay exact.
    let boundary = 2 * prediction::STEP - 1000;
    f.selection.selection.event_start_ns = count(boundary);
    f.declaration["event_start"] = json!(instant(boundary));
    let result = f.run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(metadata.row_count.get(), 2);
    assert_eq!(metadata.universe.instrument_definitions.len(), 6);
    let readback =
        job::catalog::load_catalog(&f.output().join("catalog"), &f.selection.selection).unwrap();
    assert_eq!(
        readback.series[0].instrument.ts_init().as_u64(),
        prediction::STEP / 2
    );
    assert_eq!(readback.series[0].instrument_updates.len(), 2);
    assert_eq!(
        readback.series[0].instrument_updates[0].ts_init().as_u64(),
        boundary
    );
}

#[test]
fn metadata_availability_must_cover_original_updates_after_the_last_bar() {
    let mut f = versioned_fixture();
    f.declaration["available_through"] = json!(instant(4 * prediction::STEP));
    f.reject();
}
