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
    let parameters: NativeTaskParametersV1 = serde_json::from_slice(
        &fs::read(
            f.input
                .join("objects")
                .join(f.spec.parameters_artifact_id.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    let outputs = index
        .artifacts
        .iter()
        .map(|item| {
            (
                item.clone(),
                fs::read(f.output.join(item.storage_ref.to_string())).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    // This validator is also used at the independent Store adoption boundary.
    // Exercise it against genuine compiler/engine output, not JSON-shaped mocks.
    let calibration = match &parameters {
        NativeTaskParametersV1::EvaluateSealedAlpha {
            calibration_artifact_id: Some(id),
            ..
        } => Some(
            serde_json::from_slice(
                &fs::read(f.input.join("objects").join(id.to_string())).unwrap(),
            )
            .unwrap(),
        ),
        _ => None,
    };
    domain::execution::output_bindings(
        &parameters,
        calibration.as_ref(),
        f.spec.deadline_at - chrono::Duration::seconds(60),
        chrono::Utc::now(),
        &outputs,
    )
    .unwrap();
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
fn actual_managed_sealed_uses_original_fit_and_rejects_other_partitions() {
    use contracts::science::NativeAlphaSealedResultV1;
    for role in [
        DataPartition::Sealed,
        DataPartition::Discovery,
        DataPartition::Validation,
    ] {
        let (catalog, request, calibration, wasm) = market::sealed();
        let dataset_id = Id::new();
        let model = Id::new();
        let fitted = Id::new();
        let bytes = serde_json::to_vec(&calibration).unwrap();
        let f = fixture(
            NativeTaskParametersV1::EvaluateSealedAlpha {
                schema_version: SchemaV1,
                dataset_revision_id: dataset_id,
                model_artifact_id: model,
                calibration_artifact_id: Some(fitted),
                request: Box::new(request.clone()),
            },
            vec![
                RuntimeInputV1::Dataset {
                    revision_id: dataset_id,
                    registered_ref: "synthetic-held-out".into(),
                    storage_version: "1".into(),
                    role,
                },
                RuntimeInputV1::Artifact {
                    artifact_id: model,
                    storage_version: "1".into(),
                    byte_count: market::count(wasm.len() as u64),
                    role: ArtifactInputRole::Model,
                },
                RuntimeInputV1::Artifact {
                    artifact_id: fitted,
                    storage_version: "1".into(),
                    byte_count: market::count(bytes.len() as u64),
                    role: ArtifactInputRole::Model,
                },
            ],
        );
        fs::write(f.input.join("objects").join(model.to_string()), wasm).unwrap();
        fs::write(f.input.join("objects").join(fitted.to_string()), bytes).unwrap();
        attach_catalog(&f, dataset_id, catalog.path());
        if role != DataPartition::Sealed {
            assert!(!execute(&f));
            assert!(!f.output.join("index.json").exists());
            continue;
        }
        assert!(execute(&f));
        let report: NativeAlphaSealedResultV1 = result(&f, "qz.alpha_sealed");
        domain::execution::check_alpha_sealed(&request, Some(&calibration), &report).unwrap();
        assert_eq!(report.assets.len(), 2);
        assert!(report
            .assets
            .iter()
            .all(|asset| asset.observation_count.get() == 21));
        assert_eq!(
            report.calibration_source_report_artifact_id,
            Some(calibration.source_report_artifact_id)
        );
    }
}

fn portfolio_fixture(
    losses: bool,
    change: impl FnOnce(&mut contracts::science::NativePortfolioBuildRequestV1),
) -> Fixture {
    let (catalog, mut request, wasm) = if losses {
        market::portfolio_with_losses()
    } else {
        market::portfolio()
    };
    change(&mut request);
    let weights = serde_json::to_vec(&request.current_weights).unwrap();
    let id = Id::new();
    let mut inputs = vec![RuntimeInputV1::Dataset {
        revision_id: id,
        registered_ref: "synthetic-native-regression".into(),
        storage_version: "1".into(),
        role: DataPartition::Forward,
    }];
    for member in &request.members {
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: member.model_artifact_id,
            storage_version: "1".into(),
            byte_count: market::count(wasm.len() as u64),
            role: ArtifactInputRole::Model,
        });
    }
    inputs.push(RuntimeInputV1::Artifact {
        artifact_id: request.current_weights_artifact_id,
        storage_version: "1".into(),
        byte_count: market::count(weights.len() as u64),
        role: ArtifactInputRole::Report,
    });
    let f = fixture(
        NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: id,
            request: Box::new(request.clone()),
        },
        inputs,
    );
    fs::write(
        f.input
            .join("objects")
            .join(request.current_weights_artifact_id.to_string()),
        weights,
    )
    .unwrap();
    for member in &request.members {
        fs::write(
            f.input
                .join("objects")
                .join(member.model_artifact_id.to_string()),
            &wasm,
        )
        .unwrap();
    }
    attach_catalog(&f, id, catalog.path());
    f
}

#[test]
fn actual_managed_risk_budgeting_preserves_catalog_risk_contributions() {
    managed_risk_budget(false);
}

#[test]
fn actual_managed_cvar_risk_budget_preserves_catalog_and_tail_witness() {
    managed_risk_budget(true);
}

fn managed_risk_budget(cvar: bool) {
    use contracts::portfolio::*;
    let f = portfolio_fixture(cvar, |r| {
        r.mandate.objective = AllocationObjective::RiskBudgeting;
        r.mandate.constraints.max_ex_ante_risk = Some("1".parse().unwrap());
        let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut r.mandate.optimizer else {
            unreachable!()
        };
        if cvar {
            r.mandate.risk_measure = AllocationRisk::Cvar;
            parameters.cvar_confidence = Some("0.95".parse().unwrap());
        }
        parameters.risk_budgeting = Some(RiskBudgetSettingsV1 {
            schema_version: SchemaV1,
            risky_gross_exposure: "1".parse().unwrap(),
            assets: r
                .assets
                .iter()
                .map(|a| RiskBudgetAssetV1 {
                    instrument_id: a.instrument_id.clone(),
                    share: "0.5".parse().unwrap(),
                    sign: RiskBudgetSign::Long,
                })
                .collect(),
        });
    });
    assert!(execute(&f));
    let report: contracts::science::NativePortfolioBuildResultV1 =
        result(&f, "qz.native_portfolio");
    assert_eq!(
        report.allocation.solver_status,
        SolverStatus::Optimal,
        "{:?}",
        report.allocation
    );
    domain::portfolio::allocation_result(&report.input, &report.allocation).unwrap();
    assert_eq!(report.allocation.cvar_risk_budget_witness.is_some(), cvar);
}

#[test]
fn actual_managed_allocation_reads_original_catalog_models_and_preserves_infeasibility() {
    let f = portfolio_fixture(false, |_| {});
    assert!(execute(&f));
    let report: contracts::science::NativePortfolioBuildResultV1 =
        result(&f, "qz.native_portfolio");
    assert!(report
        .allocation
        .targets
        .as_ref()
        .is_some_and(|v| v.len() == 2));
    assert!(report.consumed_fuel.get() > 0);
    for member in &report.input.forecasts.members {
        assert_eq!(member.forecasts, vec![0.01, 0.01]);
    }
    assert_eq!(report.input.return_history.end_ns.len(), 18);
    let expected = 1.003_f64 / 1.001_f64 - 1.0;
    assert!((report.input.return_history.asset_returns[0][0] - expected).abs() < 1e-14);
    let incompatible = portfolio_fixture(false, |r| r.members[1].alpha_id = r.members[0].alpha_id);
    assert!(!execute(&incompatible));
    let bad = portfolio_fixture(false, |r| {
        r.mandate.constraints.min_cash_weight = "1".parse().unwrap();
        r.mandate.constraints.max_cash_weight = "1".parse().unwrap();
    });
    assert!(execute(&bad));
    let report: contracts::science::NativePortfolioBuildResultV1 =
        result(&bad, "qz.native_portfolio");
    assert_eq!(
        report.allocation.solver_status,
        contracts::portfolio::SolverStatus::Infeasible
    );
    assert!(report.allocation.targets.is_none());
    assert!(report.allocation.cash_weight.is_none());
}

#[test]
fn portfolio_requires_original_current_weights_and_rejects_future_expired_or_mismatched_values() {
    for case in 0..4 {
        let f = portfolio_fixture(false, |r| match case {
            0 => {
                r.current_weights.available_ns =
                    market::count(r.selection.decision_cutoff_ns.get() + 1)
            }
            1 => r.current_weights.valid_until_ns = r.selection.decision_cutoff_ns,
            2 => r.current_weights.weights[0].weight = "0.99".parse().unwrap(),
            _ => r.current_weights.base_currency = "EUR".into(),
        });
        assert!(!execute(&f), "invalid frozen weights case {case}");
    }
    let mut f = portfolio_fixture(false, |_| {});
    let id = f
        .spec
        .inputs
        .iter()
        .find_map(|i| match i {
            RuntimeInputV1::Artifact {
                artifact_id,
                role: ArtifactInputRole::Report,
                ..
            } => Some(*artifact_id),
            _ => None,
        })
        .unwrap();
    // Same declared object identity cannot omit the required original REPORT binding.
    f.spec.inputs.retain(
        |i| !matches!(i, RuntimeInputV1::Artifact { artifact_id, .. } if *artifact_id == id),
    );
    fs::write(
        f.input.join("spec.json"),
        serde_json::to_vec(&f.spec).unwrap(),
    )
    .unwrap();
    assert!(!execute(&f));
    let f = portfolio_fixture(false, |_| {});
    let id = f
        .spec
        .inputs
        .iter()
        .find_map(|i| match i {
            RuntimeInputV1::Artifact {
                artifact_id,
                role: ArtifactInputRole::Report,
                ..
            } => Some(*artifact_id),
            _ => None,
        })
        .unwrap();
    let path = f.input.join("objects").join(id.to_string());
    let raw = fs::read_to_string(&path).unwrap();
    let changed = raw.replace("USD", "EUR");
    assert_ne!(raw, changed);
    assert_eq!(raw.len(), changed.len());
    fs::write(path, changed).unwrap();
    assert!(
        !execute(&f),
        "same-length original snapshot substitution must fail"
    );
}

#[test]
fn actual_managed_alpha_validation_seals_all_folds_and_never_trains_sealed_or_discovery() {
    for role in [
        DataPartition::Validation,
        DataPartition::Discovery,
        DataPartition::Sealed,
    ] {
        let (catalog, source) = market::market("0", 25);
        let dataset_id = Id::new();
        let model = Id::new();
        let wasm = market::module("local.get 0");
        let parameters = NativeTaskParametersV1::ValidateAlpha {
            schema_version: SchemaV1,
            dataset_revision_id: dataset_id,
            model_artifact_id: model,
            request: Box::new(market::alpha_validation_request(&source)),
        };
        let mut input = dataset(dataset_id);
        if let RuntimeInputV1::Dataset {
            role: input_role, ..
        } = &mut input
        {
            *input_role = role;
        }
        let f = fixture(
            parameters.clone(),
            vec![
                input,
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
        let allowed = role == DataPartition::Validation;
        assert_eq!(
            domain::execution::task(&f.spec, &parameters).is_ok(),
            allowed
        );
        assert_eq!(execute(&f), allowed);
        if allowed {
            let report: contracts::science::NativeAlphaValidationResultV1 =
                result(&f, "qz.alpha_validation");
            assert_eq!(report.folds.len(), 6);
            assert!(report
                .folds
                .iter()
                .all(|fold| fold.source_row_count.get() == 25));
        } else {
            assert!(!f.output.join("index.json").exists());
        }
    }
}

#[test]
fn native_managed_simulation_is_a_separate_process_and_does_not_invent_daily_returns() {
    let (catalog, request) = market::market("0.001", 20);
    let id = Id::new();
    let f = fixture(
        NativeTaskParametersV1::SimulatePortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: id,
            request: Box::new(request),
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
    let mut f = portfolio_fixture(false, |_| {});
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
