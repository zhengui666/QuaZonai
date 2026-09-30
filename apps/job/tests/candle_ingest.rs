//! Real native import over explicitly synthetic source responses; never market/PIT evidence.
#![cfg(feature = "catalog-prepare")]
use contracts::{science::NativeBarSelectionV1, DbCounter, SchemaV1};
use nautilus_model::{
    data::{Bar, Data},
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    str::FromStr,
};

const OBSERVED: &str = "2024-01-02T00:00:01Z";
const RECEIVED: u64 = 1_704_153_601_000_000_000;
const VALUES: [&str; 5] = ["42000.01", "42001.02", "41999.00", "42000.99", "0.10000001"];
const URL: &str = "https://api.exchange.coinbase.com/products/BTC-USD/candles?start=1970-01-01T00%3A00%3A00Z&end=1970-01-01T00%3A03%3A00Z&granularity=60";

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file(path: &str, bytes: &[u8]) -> Value {
    json!({"path":path,"size":bytes.len(),"sha256":hash(bytes)})
}
fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

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
        let source = directory.path().join("acquired");
        fs::create_dir_all(source.join("raw")).unwrap();
        let instrument = InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from_str("BTC-USD.COINBASE").unwrap())
                .raw_symbol(Symbol::new("BTC-USD"))
                .base_currency(Currency::from_str("BTC").unwrap())
                .quote_currency(Currency::from_str("USD").unwrap())
                .price_precision(2)
                .size_precision(8)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.00000001"))
                .maker_fee(Decimal::from_str("0.004").unwrap())
                .taker_fee(Decimal::from_str("0.006").unwrap())
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        );
        let definitions = directory.path().join("instruments.json");
        fs::write(
            &definitions,
            serde_json::to_vec(&vec![instrument.clone()]).unwrap(),
        )
        .unwrap();
        let terms = b"Synthetic provider terms for deterministic native tests only.";
        fs::write(source.join("source-terms.bin"), terms).unwrap();
        let rows = (0..3).map(|index| json!({
            "selection_time_seconds":index*60,"event_time_seconds":(index+1)*60,
            "open":VALUES[0],"high":VALUES[1],"low":VALUES[2],"close":VALUES[3],"volume":VALUES[4],
            "instrument":"BTC-USD","kind":"OHLCV_CANDLE","interval_seconds":60,
            "observed_at":OBSERVED,"historical_available_at":null,
            "source_response":"raw/0000.json","source_row_index":index
        })).collect::<Vec<_>>();
        let records = rows
            .iter()
            .map(|row| {
                serde_json::to_string(
                    &row.as_object()
                        .unwrap()
                        .iter()
                        .collect::<std::collections::BTreeMap<_, _>>(),
                )
                .unwrap()
                    + "\n"
            })
            .collect::<String>()
            .into_bytes();
        fs::write(source.join("records.jsonl"), &records).unwrap();
        let raw = format!(
            "[{}]",
            (0..3)
                .map(|index| format!(
                    "[{},{},{},{},{},{}]",
                    index * 60,
                    VALUES[2],
                    VALUES[1],
                    VALUES[0],
                    VALUES[3],
                    VALUES[4]
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
        .into_bytes();
        fs::write(source.join("raw/0000.json"), &raw).unwrap();
        // Read the unchanged Python descriptor so this fixture exercises the real
        // frozen acquisition contract rather than a parallel test-only schema.
        let script = "import json,providers; print(json.dumps(providers.PROVIDERS['coinbase-candles'].descriptor))";
        let response = Command::new("python3")
            .args(["-B", "-c", script])
            .env(
                "PYTHONPATH",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtimes/data"),
            )
            .output()
            .unwrap();
        assert!(response.status.success());
        let provider: Value = serde_json::from_slice(&response.stdout).unwrap();
        let request = json!({"url":URL,"start_seconds":0,"end_seconds":180,"method":"GET"});
        let manifest = json!({
            "schema":"qz.public_acquisition/1", "provider":provider,
            "selection":{"instrument":"BTC-USD","start_seconds":0,"end_seconds":180,"interval_seconds":60},
            "selection_bounds":"[start_seconds,end_seconds)",
            "limits":{"max_response_bytes":4194304,"max_response_total_bytes":33554432,
                "max_requests":128,"max_records":100000,"max_output_bytes":134217728},
            "requests":[request], "admission":{"coverage":"UNPROVEN","historical_availability":"UNVERIFIED",
                "research_qualified":false,"registered_in_quazonai":false,"permission_status":"REQUIRES_INDEPENDENT_REVIEW"},
            "created_at":"2024-01-02T00:00:02Z",
            "source_terms":{"file":file("source-terms.bin",terms),"reference":"https://www.coinbase.com/legal/market_data",
                "evidence_status":"OPERATOR_SUPPLIED_NOT_INDEPENDENTLY_VERIFIED"},
            "responses":[{"request":request,"observation":{"status":200,"headers":{"Content-Type":"application/json"},
                "request_started_at":"2024-01-02T00:00:00Z","retrieved_at":OBSERVED},
                "file":file("raw/0000.json",&raw),"counts":{"source_rows":3,"selected_rows":3,"outside_request_window":0}}],
            "records":file("records.jsonl",&records),"record_count":3,"raw_response_bytes":raw.len(),"observation_status":"OBSERVED"
        });
        write_json(&source.join("acquisition.json"), &manifest);
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
    let selection = json!({"dataset_revision_id":contracts::Id::new(),"settlements":[],"selection":{
        "schema_version":1,"bar_types":["BTC-USD.COINBASE-1-MINUTE-LAST-EXTERNAL"],
        "event_start_ns":"60000000000","event_end_ns":"240000000000",
        "decision_cutoff_ns":RECEIVED.to_string(),"maximum_rows":3}});
    let declaration = json!({"schema_version":1,"registered_ref":"synthetic-candle-import",
        "native_snapshot_ref":"synthetic-candle-import-discovery","storage_version":"fixture-v1",
        "provider_kind":"NAUTILUS_CATALOG","data_kind":"BAR","partition":"DISCOVERY",
        "event_start":"1970-01-01T00:01:00Z","event_end":"1970-01-01T00:04:00Z",
        "available_through":OBSERVED,"origin":"FIXTURE","pit_status":"UNVERIFIED","revision_policy":"UNKNOWN",
        "provenance_reference":"SYNTHETIC_NATIVE_CANDLE_IMPORT_TEST",
        "availability_provenance":"Synthetic REST batch observed late; no historical publication evidence",
        "universe":{"name":"Synthetic BTC-USD fixture","calendar_ref":"synthetic-utc","calendar_version":"1",
            "selection_asof":"1970-01-01T00:00:00Z","has_historical_membership":false,
            "coverage_start":"1970-01-01T00:00:00Z","coverage_end":"2024-01-03T00:00:00Z",
            "membership":[{"instrument_id":"BTC-USD.COINBASE","valid_from":"1970-01-01T00:00:00Z",
                "valid_until":null,"available_at":"1970-01-01T00:00:00Z","groups":null}]}});
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
