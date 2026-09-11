//! Actual one-job native subprocesses. All numerical inputs here are synthetic fixtures.
#[path = "support/market.rs"]
mod market;
use contracts::{
    execution::*,
    research::{ArtifactInputRole, DataPartition},
    runtime_jobs::*,
    DbCounter, Id, Revision, SchemaV1,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

struct Fixture {
    root: tempfile::TempDir,
    input: PathBuf,
    output: PathBuf,
    spec: JobSpecV1,
}
fn fixture(parameters: NativeTaskParametersV1, mut inputs: Vec<RuntimeInputV1>) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    fs::create_dir_all(input.join("objects")).unwrap();
    fs::create_dir(input.join("catalogs")).unwrap();
    fs::create_dir(&output).unwrap();
    let id = Id::new();
    let raw = serde_json::to_vec(&parameters).unwrap();
    fs::write(input.join("objects").join(id.to_string()), &raw).unwrap();
    inputs.push(RuntimeInputV1::Artifact {
        artifact_id: id,
        storage_version: "1".into(),
        byte_count: market::count(raw.len() as u64),
        role: ArtifactInputRole::Parameters,
    });
    let run_id = Id::new();
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run_id, 1).unwrap(),
        job_kind: parameters.job_kind(),
        image_ref: format!("example.invalid/native-fixture@sha256:{}", "a".repeat(64)),
        input_set_id: Id::new(),
        inputs,
        parameters_artifact_id: id,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: market::count(20),
            memory_mib: 512,
            wall_seconds: 60,
            output_bytes: market::count(8 * 1024 * 1024),
        },
        deadline_at: chrono::DateTime::from_timestamp_micros(
            chrono::Utc::now().timestamp_micros() + 60_000_000,
        )
        .unwrap(),
        requested_output_schemas: parameters.output_schemas(),
    };
    fs::write(input.join("spec.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
    Fixture {
        root,
        input,
        output,
        spec,
    }
}
fn dataset(id: Id) -> RuntimeInputV1 {
    RuntimeInputV1::Dataset {
        revision_id: id,
        registered_ref: "synthetic-native-regression".into(),
        storage_version: "1".into(),
        role: DataPartition::Discovery,
    }
}
fn attach_catalog(f: &Fixture, id: Id, source: &Path) {
    fs::rename(source, f.input.join("catalogs").join(id.to_string())).unwrap();
}
fn execute(f: &Fixture) -> bool {
    let stdout = fs::File::create(f.root.path().join("stdout")).unwrap();
    let stderr = fs::File::create(f.root.path().join("stderr")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("execute")
        .arg("--input-root")
        .arg(&f.input)
        .arg("--output-root")
        .arg(&f.output)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native managed fixture exceeded its subprocess deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(fs::read(f.root.path().join("stdout")).unwrap().is_empty());
    let error = fs::read(f.root.path().join("stderr")).unwrap();
    if status.success() {
        assert!(error.is_empty());
    } else {
        assert_eq!(error, b"QZ_NATIVE_JOB_FAILED\n");
    }
    status.success()
}
fn result<T: serde::de::DeserializeOwned>(f: &Fixture, schema: &str) -> T {
    let index: NativeJobOutputIndexV1 =
        serde_json::from_slice(&fs::read(f.output.join("index.json")).unwrap()).unwrap();
    let descriptor = index
        .artifacts
        .iter()
        .find(|item| item.schema.name == schema)
        .unwrap();
    let bytes = fs::read(f.output.join(descriptor.storage_ref.to_string())).unwrap();
    assert_eq!(bytes.len() as u64, descriptor.byte_count.get());
    assert_eq!(descriptor.storage_version, Revision::INITIAL);
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn actual_managed_catalog_validation_publishes_native_counts_and_no_pit_claim() {
    let (catalog, request) = market::market("0.001", 20);
    let id = Id::new();
    let f = fixture(
        NativeTaskParametersV1::ValidateData {
            schema_version: SchemaV1,
            selections: vec![NativeDatasetSelectionV1 {
                dataset_revision_id: id,
                selection: request.selection,
            }],
        },
        vec![dataset(id)],
    );
    attach_catalog(&f, id, catalog.path());
    assert!(execute(&f));
    let report: NativeDataQualityReportV1 = result(&f, "qz.data_quality");
    assert_eq!(report.native_version, "nautilus-persistence/0.63.0");
    assert_eq!(report.datasets[0].row_count.get(), 40);
    assert_eq!(report.datasets[0].instrument_ids.len(), 2);
    assert_eq!(report.datasets[0].dataset_revision_id, id);
    let value = serde_json::to_value(report).unwrap();
    assert!(value.get("pit_status").is_none());
    assert!(value.get("origin").is_none());
    assert!(value.get("qualification").is_none());
}

#[test]
fn actual_managed_forecast_reads_only_the_bound_model_and_retains_null_warmup() {
    let (catalog, request) = market::market("0.001", 20);
    let dataset_id = Id::new();
    let model = Id::new();
    let wasm = market::module("local.get 0");
    let f = fixture(
        NativeTaskParametersV1::EvaluateAlpha {
            schema_version: SchemaV1,
            dataset_revision_id: dataset_id,
            model_artifact_id: model,
            request: market::forecast_request(&request),
        },
        vec![
            dataset(dataset_id),
            RuntimeInputV1::Artifact {
                artifact_id: model,
                storage_version: "1".into(),
                byte_count: market::count(wasm.len() as u64),
                role: ArtifactInputRole::Model,
            },
        ],
    );
    fs::write(f.input.join("objects").join(model.to_string()), wasm).unwrap();
    attach_catalog(&f, dataset_id, catalog.path());
    assert!(execute(&f));
    let report: contracts::science::NativeForecastResultV1 = result(&f, "qz.native_forecast");
    let value = serde_json::to_value(report).unwrap();
    assert!(value.to_string().contains("INDICATOR_WARMUP"));
    assert!(value.to_string().contains("LABEL_NOT_COMPLETE"));
}

#[test]
fn actual_managed_allocation_preserves_real_solver_result_and_infeasibility_without_fallback() {
    let request: contracts::portfolio::AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let f = fixture(
        NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            request: Box::new(request.clone()),
        },
        vec![],
    );
    assert!(execute(&f));
    let report: contracts::portfolio::AllocationResultV1 = result(&f, "qz.native_allocation");
    let value = serde_json::to_value(&report).unwrap();
    assert!(!value["targets"].as_array().unwrap().is_empty());
    let mut impossible = request;
    impossible.constraints.min_cash_weight = "1".parse().unwrap();
    impossible.constraints.max_cash_weight = "1".parse().unwrap();
    impossible.constraints.min_net_exposure = "1".parse().unwrap();
    impossible.constraints.max_net_exposure = "1".parse().unwrap();
    let bad = fixture(
        NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            request: Box::new(impossible),
        },
        vec![],
    );
    assert!(execute(&bad));
    let report: contracts::portfolio::AllocationResultV1 = result(&bad, "qz.native_allocation");
    let value = serde_json::to_value(report).unwrap();
    assert_eq!(value["solver_status"], "INFEASIBLE");
    assert!(value["targets"].is_null());
    assert!(value["cash_weight"].is_null());
}

#[test]
fn native_managed_simulation_is_a_separate_process_and_does_not_invent_daily_returns() {
    let (catalog, request) = market::market("0.001", 20);
    let id = Id::new();
    let f = fixture(
        NativeTaskParametersV1::SimulatePortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: id,
            request,
        },
        vec![dataset(id)],
    );
    attach_catalog(&f, id, catalog.path());
    assert!(execute(&f));
    let report: contracts::science::NativeSimulationResultV1 = result(&f, "qz.native_simulation");
    let value = serde_json::to_value(report).unwrap();
    assert_eq!(value["returns_kind"], "PORTFOLIO_DAILY");
    assert_eq!(value["returns_status"], "INSUFFICIENT_DATA");
    assert!(value["returns"].as_array().unwrap().is_empty());
}

#[test]
fn native_managed_output_limit_fails_without_a_published_index() {
    let request = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut f = fixture(
        NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            request: Box::new(request),
        },
        vec![],
    );
    f.spec.limits.output_bytes = DbCounter::new(1).unwrap();
    fs::write(
        f.input.join("spec.json"),
        serde_json::to_vec(&f.spec).unwrap(),
    )
    .unwrap();
    assert!(!execute(&f));
    assert!(!f.output.join("index.json").exists());
}

#[test]
fn native_compiler_rejects_a_dataset_mount_before_starting_a_compiler() {
    let code = Id::new();
    let forbidden = Id::new();
    let f = fixture(
        NativeTaskParametersV1::CompileModel {
            schema_version: SchemaV1,
            code_artifact_id: code,
        },
        vec![
            RuntimeInputV1::Artifact {
                artifact_id: code,
                storage_version: "1".into(),
                byte_count: market::count(1),
                role: ArtifactInputRole::Code,
            },
            dataset(forbidden),
        ],
    );
    fs::write(f.input.join("objects").join(code.to_string()), b"x").unwrap();
    assert!(!execute(&f));
    assert!(fs::read_dir(&f.output).unwrap().next().is_none());
}
