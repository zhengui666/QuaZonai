//! Genuine v2-schema Parquet -> existing native dispatcher -> preparation -> v3 handoff.
//! Every record/definition is explicitly synthetic; no public-source or PIT qualification.
#![cfg(all(feature = "catalog-prepare", feature = "polymarket-history", unix))]
#[path = "support/polymarket.rs"]
mod prediction;

use contracts::factors::NativeFactorEvaluationRequestV1;
use nautilus_model::data::Bar;
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use parquet::{
    data_type::{ByteArrayType, DoubleType, Int64Type},
    file::writer::SerializedFileWriter,
    schema::parser::parse_message_type,
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

const START: u64 = 1_788_220_800;
const NS: u64 = 1_000_000_000;
const COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REPOSITORY: &str = "TimeSeventeen/Polymarket-v2";
const FILE: &str = "OrderFilled/2026_09_01.parquet";

fn instant(seconds: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_nanos((seconds * NS) as i64)
}
fn write_parquet(path: &Path, damaged: bool) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let fields = [
        ("id", "BINARY"),
        ("order_hash", "BINARY"),
        ("maker", "BINARY"),
        ("taker", "BINARY"),
        ("timestamp", "INT64"),
        ("maker_direction", "BINARY"),
        ("taker_direction", "BINARY"),
        ("token_asset_id", "BINARY"),
        ("token_amount", "DOUBLE"),
        ("usdc_amount", "DOUBLE"),
        ("price", "DOUBLE"),
        ("fee_usdc", "DOUBLE"),
        ("builder", "BINARY"),
        ("metadata", "BINARY"),
    ];
    let declaration = fields
        .iter()
        .map(|(name, kind)| {
            format!(
                "REQUIRED {kind} {name}{};",
                if *kind == "BINARY" { " (UTF8)" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let schema =
        Arc::new(parse_message_type(&format!("message source {{ {declaration} }}")).unwrap());
    let mut writer =
        SerializedFileWriter::new(fs::File::create(path).unwrap(), schema, Default::default())
            .unwrap();
    // A deliberate missing minute, one identical duplicate and one exchange summary.
    let mut rows = (0..30_u32)
        .filter(|minute| *minute != 7)
        .map(|minute| (minute, false, "101"))
        .collect::<Vec<_>>();
    rows.push((0, false, "101"));
    rows.push((0, true, "101"));
    rows.push((0, false, "102"));
    let mut group = writer.next_row_group().unwrap();
    for (name, kind) in fields {
        let mut column = group.next_column().unwrap().unwrap();
        match kind {
            "INT64" => {
                let values = rows
                    .iter()
                    .map(|(minute, _, _)| (START + u64::from(*minute) * 60 + 10) as i64)
                    .collect::<Vec<_>>();
                column
                    .typed::<Int64Type>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            "DOUBLE" => {
                let values = rows
                    .iter()
                    .map(|(minute, _, _)| match name {
                        "token_amount" if damaged && *minute == 0 => 1.0000000000000002,
                        "token_amount" => 2.0,
                        "usdc_amount" => f64::from(80 + minute) / 100.0,
                        "price" => 0.12345, // Derived price is intentionally not the authoritative cash/share ratio.
                        "fee_usdc" if damaged => 0.12345600000000001,
                        "fee_usdc" => 0.0,
                        _ => unreachable!(),
                    })
                    .collect::<Vec<_>>();
                column
                    .typed::<DoubleType>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            "BINARY" => {
                let values = rows
                    .iter()
                    .map(|(minute, summary, token)| {
                        let value = match name {
                            "id" => format!(
                                "137_{}_{}",
                                90_000_000 + minute,
                                if *summary { 2 } else { 1 }
                            ),
                            "token_asset_id" => token.to_string(),
                            "taker" if *summary => {
                                "0xe111180000d2663c0091e4f400237545b87b996b".into()
                            }
                            "taker" => "0x2222222222222222222222222222222222222222".into(),
                            "maker" => "0x1111111111111111111111111111111111111111".into(),
                            "order_hash" => format!("0x{}", "a".repeat(64)),
                            "maker_direction" => "SELL".into(),
                            "taker_direction" => "BUY".into(),
                            "builder" => format!("0x{}", "0".repeat(40)),
                            "metadata" => format!("0x{}", "0".repeat(64)),
                            _ => unreachable!(),
                        };
                        value.as_str().into()
                    })
                    .collect::<Vec<_>>();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            _ => unreachable!(),
        }
        column.close().unwrap();
    }
    group.close().unwrap();
    writer.close().unwrap();
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    repository: PathBuf,
    plan: PathBuf,
    instruments: PathBuf,
}
impl Fixture {
    fn new(damaged: bool) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("qz-v2-native-")
            .tempdir()
            .unwrap();
        let root = directory.path().canonicalize().unwrap();
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let cached = root
            .join("cache/datasets")
            .join(REPOSITORY)
            .join(COMMIT)
            .join("files")
            .join(FILE);
        write_parquet(&cached, damaged);
        let size = fs::metadata(&cached).unwrap().len();
        let plan = root.join("original-plan.json");
        fs::write(&plan, serde_json::to_vec(&json!({
            "schema": "qz.hf_dataset_plan/1", "repository": REPOSITORY, "revision": COMMIT,
            "requested_revision": "FIXTURE: synthetic local files, not actual Hub contents", "license": "FIXTURE",
            "partition_index": null, "request": {"includes": [FILE], "markets": [], "start_date": null, "end_date": null},
            "selection_bounds": "[start_date,end_date)", "download_granularity": "FILE_PARTITION", "coverage": "NOT_ASSERTED",
            "max_bytes": null, "total_bytes": size, "files": [{"path": FILE, "size": size, "format": "parquet",
                "url": format!("https://huggingface.co/datasets/{REPOSITORY}/resolve/{COMMIT}/{FILE}")}],
            "polymarket_v2": {"schema": "qz.polymarket_v2_request/1", "start_date": "2026-09-01", "end_date": "2026-09-02", "tokens": ["101"]}
        })).unwrap()).unwrap();
        let instruments = root.join("original-instruments.json");
        let mut definitions =
            serde_json::to_value(vec![
                prediction::instruments("0", (START + 3600) * NS)[0].clone()
            ])
            .unwrap();
        // Valid omitted Option fields must survive native normalization and export.
        definitions[0]["BinaryOption"]
            .as_object_mut()
            .unwrap()
            .remove("description");
        fs::write(&instruments, serde_json::to_vec(&definitions).unwrap()).unwrap();
        fs::create_dir(root.join("scripts")).unwrap();
        for operation in ["polymarket-history", "catalog-prepare"] {
            let shim = root.join("scripts").join(operation);
            fs::write(
                &shim,
                format!("#!/bin/sh\nexec \"$QZ_TEST_SOURCE_TOOLS\" {operation} \"$@\"\n"),
            )
            .unwrap();
            fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self {
            _directory: directory,
            root,
            repository,
            plan,
            instruments,
        }
    }
    fn source(&self, operation: &str) -> Command {
        let mut command = Command::new("python3");
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("QZ_TEST_SOURCE_TOOLS", env!("CARGO_BIN_EXE_source-tools"))
            .current_dir(&self.repository)
            .arg("-B")
            .arg(self.repository.join("runtimes/data/source_plugins.py"))
            .args([operation, "polymarket-v2-hf"]);
        command
    }
    fn frozen(&self) -> PathBuf {
        accepted(
            self.source("freeze")
                .arg("--plan")
                .arg(&self.plan)
                .arg("--cache-dir")
                .arg(self.root.join("cache"))
                .arg("--output")
                .arg(self.root.join("request")),
        );
        self.root.join("request/selection.json")
    }
    fn convert(&self) -> Command {
        let selection = self.frozen();
        let mut command = self.source("convert");
        command
            .arg("--selection")
            .arg(selection)
            .arg("--instruments")
            .arg(&self.instruments)
            .args([
                "--bar-seconds",
                "60",
                "--start-seconds",
                &START.to_string(),
                "--end-seconds",
                &(START + 1800).to_string(),
            ])
            .arg("--native-bin")
            .arg(self.root.join("scripts/polymarket-history"))
            .arg("--output")
            .arg(self.root.join("native"));
        command
    }
    fn prepare(&self, sealed: bool) -> (PathBuf, Value) {
        let end = if sealed {
            (START + 1800) * NS + 1000
        } else {
            (START + 1200) * NS
        };
        let selected = json!({"dataset_revision_id": contracts::Id::new(), "settlements": [],
            "selection": {"schema_version": 1, "bar_types": [format!("{}-1-MINUTE-LAST-EXTERNAL", prediction::IDS[0])],
                "event_start_ns": ((START + 60) * NS).to_string(), "event_end_ns": end.to_string(),
                "decision_cutoff_ns": ((START + 1900) * NS).to_string(), "maximum_rows": 100}});
        let name = if sealed { "sealed" } else { "discovery" };
        let declaration = self.root.join(format!("{name}-declaration.json"));
        let native_selection = self.root.join(format!("{name}-selection.json"));
        let output = self.root.join(name);
        let parsed = serde_json::from_value(selected.clone()).unwrap();
        job::catalog::measure_catalog(&self.root.join("native/catalog"), &parsed, !sealed)
            .expect("native fixture selection must be measurable before preparation");
        fs::write(&native_selection, serde_json::to_vec(&selected).unwrap()).unwrap();
        fs::write(&declaration, serde_json::to_vec(&json!({
            "schema_version": 1, "registered_ref": format!("fixture:v2:{name}"), "native_snapshot_ref": "fixture:v2-native",
            "storage_version": "fixture-v1", "provider_kind": "NAUTILUS_CATALOG", "data_kind": "BAR",
            "partition": if sealed { "SEALED" } else { "DISCOVERY" },
            "event_start": instant(START + 60), "event_end": chrono::DateTime::from_timestamp_nanos(end as i64),
            "available_through": instant(START + 1900), "origin": "FIXTURE", "pit_status": "UNVERIFIED", "revision_policy": "UNKNOWN",
            "provenance_reference": "SYNTHETIC_NATIVE_REGRESSION: not actual HF source files",
            "availability_provenance": "Synthetic event-time proxies; no historical availability claim",
            "universe": {"name": "Synthetic v2 fixture", "calendar_ref": "fixture-calendar", "calendar_version": "1",
                "selection_asof": instant(0), "has_historical_membership": false, "coverage_start": instant(0),
                "coverage_end": instant(START + 3600), "membership": [{"instrument_id": prediction::IDS[0],
                    "valid_from": instant(0), "valid_until": null, "available_at": instant(0), "groups": null}]}
        })).unwrap()).unwrap();
        let result = accepted(
            self.source("prepare")
                .arg("--native-output")
                .arg(self.root.join("native"))
                .arg("--declaration")
                .arg(declaration)
                .arg("--selection")
                .arg(native_selection)
                .arg("--native-bin")
                .arg(self.root.join("scripts/catalog-prepare"))
                .arg("--output")
                .arg(&output),
        );
        assert_eq!(result["source_preset"], "polymarket-v2-hf");
        assert_eq!(result["admission"]["research_qualified"], false);
        (output.join("preparation.json"), selected)
    }
}
fn accepted(command: &mut Command) -> Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn public_v2_schema_runs_through_real_dispatch_preparation_export_and_factor_evaluation() {
    let fixture = Fixture::new(false);
    let converted = accepted(&mut fixture.convert());
    assert_eq!(converted["native_report"]["trades"], 29);
    assert_eq!(converted["native_report"]["bars"], 29);
    let evidence: Value = serde_json::from_slice(
        &fs::read(fixture.root.join("native/source-evidence.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence["source_metadata"]["quality"]["duplicate_rows"], 1);
    assert_eq!(
        evidence["source_metadata"]["quality"]["exchange_summaries_excluded"],
        1
    );
    let (discovery, _) = fixture.prepare(false);
    let (sealed, selected) = fixture.prepare(true);
    let mut catalog = ParquetDataCatalog::from_uri(
        fixture.root.join("sealed/catalog").to_str().unwrap(),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let bars = catalog
        .query::<Bar>(None, None, None, None, None, true)
        .unwrap()
        .map(|r| r.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bars.len(), 29);
    assert!(!bars.iter().any(|row| matches!(row, nautilus_model::data::Data::Bar(bar) if bar.ts_event.as_u64() == (START + 480) * NS)));
    let request = json!({"schema_version": 1, "forecast": {"schema_version": 1, "selection": selected["selection"],
        "parameters": {"schema_version": 1, "fast_period": 1, "slow_period": 2, "label_horizon_observations": 1, "total_fuel": "1000000"}},
        "train_end_ns": ((START + 600) * NS).to_string(), "validation_end_ns": ((START + 1200) * NS).to_string()});
    let request_path = fixture.root.join("original-factor.json");
    fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let dataset_path = fixture.root.join("dataset.json");
    let exported = accepted(
        fixture
            .source("export-dataset")
            .arg("--prepared")
            .arg(discovery)
            .arg("--request")
            .arg(&request_path)
            .arg("--sealed-prepared")
            .arg(sealed)
            .arg("--sealed-request")
            .arg(&request_path)
            .args([
                "--id",
                "native-v2-fixture",
                "--name",
                "Synthetic native integration; UNVERIFIED",
            ])
            .arg("--output")
            .arg(&dataset_path),
    );
    assert_eq!(exported["registered_in_service"], false);
    let dataset: Value = serde_json::from_slice(&fs::read(dataset_path).unwrap()).unwrap();
    assert_eq!(dataset["ready"], false);
    assert_eq!(dataset["data"]["request"], request);
    assert!(dataset["source"].as_str().unwrap().contains("FIXTURE"));
    let mut iterative: NativeFactorEvaluationRequestV1 =
        serde_json::from_value(dataset["data"]["request"].clone()).unwrap();
    iterative.forecast.selection.event_end_ns = iterative.validation_end_ns;
    let model = wat::parse_str("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) local.get 0))").unwrap();
    let iteration = job::factors::evaluate_factor(
        Path::new(dataset["data"]["catalog"].as_str().unwrap()),
        &iterative,
        &model,
    )
    .unwrap();
    assert!(iteration
        .points
        .iter()
        .all(|point| point.partition != contracts::factors::NativeFactorPartitionV1::Heldout));
    let full: NativeFactorEvaluationRequestV1 =
        serde_json::from_value(dataset["data"]["sealed"]["request"].clone()).unwrap();
    let independent = job::factors::evaluate_factor(
        Path::new(dataset["data"]["sealed"]["catalog"].as_str().unwrap()),
        &full,
        &model,
    )
    .unwrap();
    assert!(independent
        .points
        .iter()
        .any(|point| point.partition == contracts::factors::NativeFactorPartitionV1::Heldout));
}

#[test]
fn recorded_cost_preparation_joins_original_native_quantities_and_never_zero_fills() {
    use nautilus_model::{
        data::{BarType, TradeTick},
        enums::AggressorSide,
        identifiers::{InstrumentId, TradeId},
        types::{Price, Quantity},
    };
    let fixture = Fixture::new(true);
    let source_root = fixture
        .root
        .join("cache/datasets")
        .join(REPOSITORY)
        .join(COMMIT);
    let plan: Value = serde_json::from_slice(&fs::read(&fixture.plan).unwrap()).unwrap();
    let mut file = plan["files"][0].clone();
    file["local_path"] = json!(source_root.join("files").join(FILE));
    file["cached"] = true.into();
    file["resumed_bytes"] = 0.into();
    file["validation"] = "PARQUET_ENVELOPE".into();
    let source_selection = fixture.root.join("source-selection.json");
    fs::write(&source_selection, serde_json::to_vec(&json!({
        "schema":"qz.hf_selection/1", "plan":plan, "cache_root":source_root,
        "files":[file], "retrieved_at":"2026-09-01T01:00:00Z", "downloaded_bytes":0,"cached_files":1
    })).unwrap()).unwrap();
    let native_selection = fixture.root.join("cost-bar-selection.json");
    fs::write(&native_selection, serde_json::to_vec(&json!({
        "schema_version":1,"bar_types":["101.POLYMARKET-1-MINUTE-LAST-EXTERNAL"],
        "event_start_ns":((START+60)*NS).to_string(),"event_end_ns":((START+120)*NS).to_string(),
        "decision_cutoff_ns":((START+120)*NS).to_string(),"maximum_rows":1
    })).unwrap()).unwrap();
    let make_native = |name: &str, event_id: &str, volume: &str, available: u64| {
        let path = fixture.root.join(name);
        fs::create_dir(&path).unwrap();
        let catalog =
            ParquetDataCatalog::from_uri(path.to_str().unwrap(), None, Some(4096), None, None)
                .unwrap();
        let id: InstrumentId = "101.POLYMARKET".parse().unwrap();
        let kind: BarType = "101.POLYMARKET-1-MINUTE-LAST-EXTERNAL".parse().unwrap();
        let price = Price::from("0.800000000");
        catalog
            .write_to_parquet(
                &[Bar::new(
                    kind,
                    price,
                    price,
                    price,
                    price,
                    Quantity::from(volume),
                    ((START + 60) * NS).into(),
                    ((START + 60) * NS).into(),
                )],
                None,
                None,
                None,
            )
            .unwrap();
        catalog
            .write_to_parquet(
                &[TradeTick::new(
                    id,
                    price,
                    Quantity::from("1.000000"),
                    AggressorSide::NoAggressor,
                    TradeId::new(event_id),
                    ((START + 10) * NS).into(),
                    (available * NS).into(),
                )],
                None,
                None,
                None,
            )
            .unwrap();
        path
    };
    let command = |catalog: &Path, output: &Path| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_source-tools"));
        cmd.args(["polymarket-history", "recorded-fees"])
            .arg("--selection")
            .arg(&source_selection)
            .arg("--catalog")
            .arg(catalog)
            .arg("--bar-selection")
            .arg(&native_selection)
            .arg("--output")
            .arg(output);
        cmd
    };
    let catalog = make_native("cost-native", "137_90000000_1", "1.000000", START + 10);
    let output = fixture.root.join("cost-output");
    let report = accepted(&mut command(&catalog, &output));
    assert_eq!(report["records"], 1); // Identical source duplicates do not double charge.
    let path = output.join("101.POLYMARKET.json");
    let original = fs::read(&path).unwrap();
    let costs: Value = serde_json::from_slice(&original).unwrap();
    let record = &costs["records"][0];
    assert_eq!(record["source_quantity"], "1.0000000000000002");
    assert_eq!(record["quantity"], "1");
    assert_eq!(record["fee_usdc"], "0.12345600000000001");
    assert_eq!(record["usdc_amount"], "0.8");
    assert_eq!(record["price"], "0.12345");
    assert_eq!(record["maker_direction"], "SELL");
    assert_eq!(costs["currency"], "pUSD");
    let overwrite = command(&catalog, &output).output().unwrap();
    assert!(!overwrite.status.success());
    assert_eq!(fs::read(path).unwrap(), original);
    #[cfg(unix)]
    {
        let output = fixture.root.join("interrupted-output");
        let cmd = command(&catalog, &output);
        let result = Command::new("sh")
            .args([
                "-c",
                "ulimit -c 0; ulimit -f 0; exec \"$@\"",
                "fee-write-failure",
            ])
            .arg(cmd.get_program())
            .args(cmd.get_args())
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            !output.exists(),
            "a failed write exposed a canonical cost directory"
        );
        assert_eq!(accepted(&mut command(&catalog, &output))["records"], 1);
    }
    for (name, id, volume, available, reason) in [
        (
            "missing",
            "137_99999999_1",
            "1.000000",
            START + 10,
            "RECORDED_FEE_SOURCE_RECORD_MISSING",
        ),
        (
            "volume",
            "137_90000000_1",
            "2.000000",
            START + 10,
            "RECORDED_FEE_BAR_QUANTITY_MISMATCH",
        ),
        (
            "reversed-clock",
            "137_90000000_1",
            "1.000000",
            START + 9,
            "RECORDED_FEE_NATIVE_CLOCK_MISMATCH",
        ),
        (
            "late-clock",
            "137_90000000_1",
            "1.000000",
            START + 61,
            "RECORDED_FEE_NATIVE_CLOCK_MISMATCH",
        ),
    ] {
        let catalog = make_native(name, id, volume, available);
        let output = fixture.root.join(format!("{name}-output"));
        let result = command(&catalog, &output).output().unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(reason),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!output.exists());
    }
}

#[test]
fn normalized_amount_damage_stays_a_native_failure_without_success_publication() {
    let fixture = Fixture::new(true);
    let result = fixture.convert().output().unwrap();
    assert!(!result.status.success());
    assert!(!fixture.root.join("native/import-report.json").exists());
    assert!(!fixture.root.join("native/catalog").exists());
}
