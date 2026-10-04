//! Real native import over explicitly synthetic source responses; never market/PIT evidence.
#![cfg(feature = "catalog-prepare")]
use contracts::{science::NativeBarSelectionV1, DbCounter, SchemaV1};
use nautilus_model::{
    data::{Bar, Data},
    instruments::InstrumentAny,
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    str::FromStr,
};

#[path = "support/candle_acquisition.rs"]
mod acquisition;
use acquisition::{file, hash, write_json, OBSERVED, RECEIVED, VALUES};

struct Fixture {
    directory: tempfile::TempDir,
    source: PathBuf,
    definitions: PathBuf,
    output: PathBuf,
    manifest: Value,
    instrument: InstrumentAny,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let acquisition::Acquisition {
            source,
            definitions,
            manifest,
            instrument,
        } = acquisition::write_acquisition(directory.path());
        let output = directory.path().join("imported");
        Self {
            directory,
            source,
            definitions,
            output,
            manifest,
            instrument,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_catalog-prepare"));
        command
            .arg("ingest-candles")
            .arg("--acquisition")
            .arg(self.source.join("acquisition.json"))
            .arg("--instruments")
            .arg(&self.definitions)
            .arg("--output")
            .arg(&self.output);
        command
    }
    fn run(&self) -> Output {
        self.command().output().unwrap()
    }
    fn save_manifest(&self) {
        write_json(&self.source.join("acquisition.json"), &self.manifest);
    }
    fn reject(&self) {
        let result = self.run();
        assert!(!result.status.success(), "invalid acquisition was imported");
        assert!(!self.output.join("import-report.json").exists());
        assert_eq!(
            String::from_utf8_lossy(&result.stderr).trim(),
            "QZ_CATALOG_PREPARATION_FAILED"
        );
    }
    fn native_rows(&self) -> Vec<Bar> {
        let mut native = ParquetDataCatalog::from_uri(
            self.output.join("catalog").to_str().unwrap(),
            None,
            Some(2),
            None,
            None,
        )
        .unwrap();
        native
            .query::<Bar>(
                Some(vec!["BTC-USD.COINBASE-1-MINUTE-LAST-EXTERNAL".into()]),
                None,
                None,
                None,
                None,
                true,
            )
            .unwrap()
            .map(|row| match row.unwrap() {
                Data::Bar(bar) => bar,
                _ => panic!("unexpected record"),
            })
            .collect()
    }
    fn change_raw(&mut self, raw: &[u8]) {
        fs::write(self.source.join("raw/0000.json"), raw).unwrap();
        self.manifest["responses"][0]["file"] = file("raw/0000.json", raw);
        self.manifest["raw_response_bytes"] = raw.len().into();
        self.save_manifest();
    }
    fn change_definitions(&self, change: impl FnOnce(&mut Value)) {
        let mut definitions = serde_json::to_value([&self.instrument]).unwrap();
        change(&mut definitions[0]["CurrencyPair"]);
        write_json(&self.definitions, &definitions);
    }
}

#[test]
fn native_roundtrip_preserves_exact_prices_volume_definitions_and_batch_clock() {
    let f = Fixture::new();
    let result = f.run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["bars"], 3);
    assert_eq!(report["historical_availability"], "UNVERIFIED");
    assert_eq!(report["research_qualified"], false);
    assert_eq!(report["native_readback_verified"], true);
    assert_eq!(
        report["acquisition_sha256"],
        hash(&fs::read(f.source.join("acquisition.json")).unwrap())
    );
    let bars = f.native_rows();
    assert_eq!(bars.len(), 3);
    for (index, bar) in bars.iter().enumerate() {
        assert_eq!(bar.ts_event.as_u64(), (index as u64 + 1) * 60_000_000_000);
        assert_eq!(bar.ts_init.as_u64(), RECEIVED);
        assert_eq!(bar.open, Price::from(VALUES[0]));
        assert_eq!(bar.volume, Quantity::from(VALUES[4]));
    }
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec![bars[0].bar_type.to_string()],
        event_start_ns: DbCounter::ZERO,
        event_end_ns: DbCounter::new(240_000_000_000).unwrap(),
        decision_cutoff_ns: DbCounter::new(RECEIVED).unwrap(),
        maximum_rows: 3,
    };
    let selected = job::catalog::load_catalog(&f.output.join("catalog"), &selection).unwrap();
    assert_eq!(selected.series[0].bars, bars);
    assert_eq!(
        serde_json::to_value(&selected.series[0].instrument).unwrap(),
        serde_json::to_value(&f.instrument).unwrap()
    );
}

#[test]
fn source_raw_derived_hashes_and_lineage_cannot_be_changed() {
    for case in 0..5 {
        let mut f = Fixture::new();
        match case {
            0 => fs::write(f.source.join("raw/0000.json"), b"[]").unwrap(),
            1 => {
                f.manifest["responses"][0]["request"]["url"] = "https://other.example".into();
                f.save_manifest();
            }
            2 => {
                f.manifest["created_at"] = "2024-01-01T23:59:59Z".into();
                f.save_manifest();
            }
            3 => {
                f.manifest["admission"]["research_qualified"] = true.into();
                f.save_manifest();
            }
            _ => {
                let path = f.source.join("records.jsonl");
                let changed = fs::read_to_string(&path)
                    .unwrap()
                    .replace("42000.01", "42000.02");
                fs::write(&path, changed.as_bytes()).unwrap();
                f.manifest["records"] = file("records.jsonl", changed.as_bytes());
                f.save_manifest();
            }
        }
        f.reject();
    }
}

#[test]
fn malformed_duplicate_and_lossy_candles_are_rejected_without_a_final_report() {
    for raw in [
        b"not json".as_slice(),
        b"[[0,1,3,2,2,1],[0,1,3,2,2,1]]",
        b"[[0,42001,41999,42000,42000,1]]",
        b"[[0,1,3,2,2,0.000000000000000000000000000001]]",
    ] {
        let mut f = Fixture::new();
        f.change_raw(raw);
        f.reject();
        assert_eq!(fs::read(f.source.join("raw/0000.json")).unwrap(), raw);
    }
}

#[test]
fn original_symbol_currency_precision_and_definition_time_are_enforced() {
    for case in 0..9 {
        let f = Fixture::new();
        f.change_definitions(|definition| match case {
            0 => definition["raw_symbol"] = "ETH-USD".into(),
            1 => definition["id"] = "BTC-USD.OTHER".into(),
            2 => {
                definition["price_precision"] = 1.into();
                definition["price_increment"] = "0.1".into();
            }
            3 => {
                definition["size_precision"] = 6.into();
                definition["size_increment"] = "0.000001".into();
            }
            4 => {
                definition["ts_init"] = (RECEIVED + 1).into();
            }
            5 => {
                definition["multiplier"] = "2".into();
            }
            6 => {
                definition["base_currency"] =
                    serde_json::to_value(Currency::from_str("ETH").unwrap()).unwrap();
            }
            7 => {
                definition["quote_currency"] =
                    serde_json::to_value(Currency::from_str("EUR").unwrap()).unwrap();
            }
            _ => {
                definition["ts_event"] = 1_u64.into();
            }
        });
        f.reject();
    }
}

#[test]
fn current_definitions_keep_their_actual_late_clock_without_becoming_pit() {
    let f = Fixture::new();
    f.change_definitions(|definition| {
        definition["ts_event"] = (RECEIVED - 1_000_000_000).into();
        definition["ts_init"] = (RECEIVED - 1_000_000_000).into();
    });
    let result = f.run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: Value =
        serde_json::from_slice(&fs::read(f.output.join("source-evidence.json")).unwrap()).unwrap();
    assert_eq!(
        evidence["instrument_definitions"][0]["CurrencyPair"]["ts_init"],
        RECEIVED - 1_000_000_000
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["historical_availability"], "UNVERIFIED");
    assert_eq!(report["registered_in_quazonai"], false);
}

#[test]
fn output_is_new_never_overwrites_or_modifies_original_inputs() {
    let f = Fixture::new();
    let original = fs::read(f.source.join("raw/0000.json")).unwrap();
    fs::create_dir(&f.output).unwrap();
    fs::write(f.output.join("user.txt"), b"keep").unwrap();
    f.reject();
    assert_eq!(fs::read(f.output.join("user.txt")).unwrap(), b"keep");
    assert_eq!(fs::read(f.source.join("raw/0000.json")).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn linked_source_files_are_rejected() {
    let f = Fixture::new();
    let raw = f.source.join("raw/0000.json");
    let original = f.directory.path().join("original.json");
    fs::rename(&raw, &original).unwrap();
    std::os::unix::fs::symlink(original, raw).unwrap();
    f.reject();
}

#[test]
fn registry_invokes_the_real_native_converter_and_validates_its_publication() {
    let f = Fixture::new();
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtimes/data/source_plugins.py");
    let result = Command::new("python3")
        .arg("-B")
        .arg(script)
        .args(["convert", "coinbase-candles", "--native-bin"])
        .arg(env!("CARGO_BIN_EXE_catalog-prepare"))
        .arg("--acquisition")
        .arg(f.source.join("acquisition.json"))
        .arg("--instruments")
        .arg(&f.definitions)
        .arg("--output")
        .arg(&f.output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["status"], "NATIVE_ARTIFACTS_VALIDATED");
    assert_eq!(report["native_report"]["bars"], 3);
    assert_eq!(report["admission"]["research_qualified"], false);
    assert_eq!(f.native_rows().len(), 3);
}

#[test]
fn imported_native_bars_use_the_unchanged_partition_preparation_and_quality_path() {
    let f = Fixture::new();
    let imported = f.run();
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let selection = acquisition::selection();
    let declaration = acquisition::declaration();
    let selection_path = f.directory.path().join("selection.json");
    let declaration_path = f.directory.path().join("declaration.json");
    write_json(&selection_path, &selection);
    write_json(&declaration_path, &declaration);
    let output = f.directory.path().join("prepared");
    let result = Command::new(env!("CARGO_BIN_EXE_catalog-prepare"))
        .arg("--catalog")
        .arg(f.output.join("catalog"))
        .arg("--declaration")
        .arg(declaration_path)
        .arg("--selection")
        .arg(selection_path)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(metadata["row_count"], "3");
    assert_eq!(metadata["origin"], "FIXTURE");
    assert_eq!(metadata["pit_status"], "UNVERIFIED");
    assert_eq!(metadata["available_through"], OBSERVED);
    assert!(output.join("catalog-metadata.json").is_file());
}

#[cfg(feature = "polymarket-history")]
#[test]
fn installed_dispatch_preserves_candle_artifacts() {
    let f = Fixture::new();
    let original = f.command();
    let result = Command::new(env!("CARGO_BIN_EXE_source-tools"))
        .arg("catalog-prepare")
        .args(original.get_args())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["bars"], 3);
    assert_eq!(report["native_readback_verified"], true);
    assert_eq!(report["historical_availability"], "UNVERIFIED");
    assert_eq!(report["research_qualified"], false);
    assert_eq!(f.native_rows().len(), 3);
}
