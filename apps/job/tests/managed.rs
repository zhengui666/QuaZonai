//! Actual one-job native subprocesses. All numerical inputs here are synthetic fixtures.
#[path = "support/experiment.rs"]
mod experiment_fixture;
#[path = "support/market.rs"]
mod market;
use contracts::{
    execution::*,
    research::{ArtifactInputRole, DataPartition},
    runtime_jobs::*,
    DbCounter, Id, Revision, SchemaV1,
};
use market::execution_models;
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
            cpu: Some(1),
            cpu_seconds: Some(market::count(20)),
            memory_mib: Some(512),
            wall_seconds: Some(60),
            output_bytes: Some(market::count(8 * 1024 * 1024)),
        },
        deadline_at: Some(
            chrono::DateTime::from_timestamp_micros(
                chrono::Utc::now().timestamp_micros() + 60_000_000,
            )
            .unwrap(),
        ),
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
        assert_eq!(
            String::from_utf8_lossy(&error).lines().next(),
            Some("QZ_NATIVE_JOB_FAILED")
        );
        eprintln!(
            "stage=managed execute; status={status}; stderr: {}",
            String::from_utf8_lossy(&error)
        );
    }
    status.success()
}
fn output_bytes(f: &Fixture) -> Vec<(RuntimeOutputV1, Vec<u8>)> {
    let index: NativeJobOutputIndexV1 =
        serde_json::from_slice(&fs::read(f.output.join("index.json")).unwrap()).unwrap();
    index
        .artifacts
        .into_iter()
        .map(|item| {
            let bytes = fs::read(f.output.join(item.storage_ref.to_string())).unwrap();
            (item, bytes)
        })
        .collect()
}
fn result<T: serde::de::DeserializeOwned>(f: &Fixture, schema: &str) -> T {
    let parameters: NativeTaskParametersV1 = serde_json::from_slice(
        &fs::read(
            f.input
                .join("objects")
                .join(f.spec.parameters_artifact_id.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    let outputs = output_bytes(f);
    // This validator is also used at the independent Store adoption boundary.
    // Exercise it against genuine compiler/engine output, not JSON-shaped mocks.
    domain::execution::output_bindings(
        &parameters,
        None,
        f.spec.deadline_at.unwrap() - chrono::Duration::seconds(60),
        chrono::Utc::now(),
        &outputs,
    )
    .unwrap();
    let (descriptor, bytes) = outputs
        .iter()
        .find(|(item, _)| item.schema.name == schema)
        .unwrap();
    assert_eq!(bytes.len() as u64, descriptor.byte_count.get());
    assert_eq!(descriptor.storage_version, Revision::INITIAL);
    serde_json::from_slice(bytes).unwrap()
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
    assert_eq!(report.native_version, "nautilus-persistence/0.69.0");
    assert_eq!(report.datasets[0].row_count.get(), 40);
    assert_eq!(report.datasets[0].instrument_ids.len(), 2);
    assert_eq!(report.datasets[0].dataset_revision_id, id);
    let measured = report.datasets[0].last_bar_notionals.as_ref().unwrap();
    assert_eq!(measured.len(), 2);
    for (index, observation) in measured.iter().enumerate() {
        assert_eq!(
            observation.instrument_id,
            report.datasets[0].instrument_ids[index]
        );
        assert_eq!(observation.currency, "USD");
        assert_eq!(observation.event_ns.get(), 20 * market::INTERVAL_NS);
        assert_eq!(observation.available_ns.get(), 20 * market::INTERVAL_NS + 1);
        assert_eq!(observation.traded_volume, "10000000".parse().unwrap());
        assert_eq!(
            observation.close_price,
            ["1.02", "2.02"][index].parse().unwrap()
        );
        assert_eq!(
            observation.notional_value,
            ["10200000", "20200000"][index].parse().unwrap()
        );
    }
    let value = serde_json::to_value(report).unwrap();
    assert!(value.get("pit_status").is_none());
    assert!(value.get("origin").is_none());
    assert!(value.get("qualification").is_none());
}

#[test]
fn sealed_quality_does_not_publish_last_bar_values() {
    let (catalog, request) = market::market("0.001", 20);
    let id = Id::new();
    let mut input = dataset(id);
    let RuntimeInputV1::Dataset { role, .. } = &mut input else {
        unreachable!()
    };
    *role = DataPartition::Sealed;
    let f = fixture(
        NativeTaskParametersV1::ValidateData {
            schema_version: SchemaV1,
            selections: vec![NativeDatasetSelectionV1 {
                dataset_revision_id: id,
                selection: request.selection,
            }],
        },
        vec![input],
    );
    attach_catalog(&f, id, catalog.path());
    assert!(execute(&f));
    let report: NativeDataQualityReportV1 = result(&f, "qz.data_quality");
    assert!(report.datasets[0].last_bar_notionals.is_none());
}

#[test]
fn retired_managed_forecast_cannot_publish_or_execute() {
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
    assert_eq!(
        job::managed::execute(&f.input, &f.output)
            .unwrap_err()
            .to_string(),
        "NATIVE_RESEARCH_OPERATION_RETIRED"
    );
    assert!(!execute(&f));
    assert!(!f.output.join("index.json").exists());
}

#[test]
fn retired_managed_study_cannot_publish_or_execute() {
    for (liquidity, infeasible) in [(false, false), (false, true), (true, false), (true, true)] {
        let (catalog, mut request, wasm) = if liquidity {
            market::study_liquidity("10000000", "0.4")
        } else {
            market::study()
        };
        if liquidity {
            market::calendar_schedule(&mut request);
        }
        if infeasible {
            request.mandate.constraints.max_asset_weight = "0.4".parse().unwrap();
        }
        let dataset = Id::new();
        let mut objects = request
            .members
            .iter()
            .map(|member| {
                (
                    member.model_artifact_id,
                    wasm.clone(),
                    ArtifactInputRole::Model,
                )
            })
            .collect::<Vec<_>>();
        objects.push((
            request.mandate.constraints.transaction_costs_ref,
            serde_json::to_vec(&request.execution_settings).unwrap(),
            ArtifactInputRole::Parameters,
        ));
        if let Some(policy) = &request.rolling_liquidity {
            objects.push((
                request.mandate.constraints.liquidity_ref.unwrap(),
                serde_json::to_vec(policy).unwrap(),
                ArtifactInputRole::Parameters,
            ));
        }
        if let Some(binding) = &request.calendar {
            objects.push((
                binding.artifact_id,
                serde_json::to_vec(&binding.calendar).unwrap(),
                ArtifactInputRole::Parameters,
            ));
        }
        let mut inputs = vec![RuntimeInputV1::Dataset {
            revision_id: dataset,
            registered_ref: "synthetic-native-regression".into(),
            storage_version: "1".into(),
            role: if liquidity {
                DataPartition::Validation
            } else {
                DataPartition::Discovery
            },
        }];
        inputs.extend(
            objects
                .iter()
                .map(|(id, bytes, role)| RuntimeInputV1::Artifact {
                    artifact_id: *id,
                    storage_version: "1".into(),
                    byte_count: market::count(bytes.len() as u64),
                    role: *role,
                }),
        );
        let parameters = NativeTaskParametersV1::StudyPortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            request: Box::new(request),
        };
        let f = fixture(parameters.clone(), inputs);
        for (id, bytes, _) in objects {
            fs::write(f.input.join("objects").join(id.to_string()), bytes).unwrap();
        }
        attach_catalog(&f, dataset, catalog.path());
        assert_eq!(
            job::managed::execute(&f.input, &f.output)
                .unwrap_err()
                .to_string(),
            "NATIVE_RESEARCH_OPERATION_RETIRED"
        );
        assert!(!execute(&f));
        assert!(!f.output.join("index.json").exists());
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
    let liquidity = request.bar_liquidity.as_ref().map(|binding| {
        let measurement = fixture(
            NativeTaskParametersV1::ValidateData {
                schema_version: SchemaV1,
                selections: vec![binding.source.clone()],
            },
            vec![dataset(binding.source.dataset_revision_id)],
        );
        attach_catalog(
            &measurement,
            binding.source.dataset_revision_id,
            catalog.path(),
        );
        assert!(execute(&measurement));
        let report: NativeDataQualityReportV1 = result(&measurement, "qz.data_quality");
        fs::rename(
            measurement
                .input
                .join("catalogs")
                .join(binding.source.dataset_revision_id.to_string()),
            catalog.path(),
        )
        .unwrap();
        for (asset, value) in request
            .assets
            .iter_mut()
            .zip(report.datasets[0].last_bar_notionals.as_ref().unwrap())
        {
            asset.available_notional = Some(value.notional_value.clone());
        }
        (
            binding.assumption.report_artifact_id,
            serde_json::to_vec(&report).unwrap(),
        )
    });
    let weights = serde_json::to_vec(&request.current_weights).unwrap();
    let rolling = request.rolling_liquidity.as_ref().map(|policy| {
        (
            request.mandate.constraints.liquidity_ref.unwrap(),
            serde_json::to_vec(policy).unwrap(),
        )
    });
    let costs = serde_json::to_vec(&request.execution_settings).unwrap();
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
    inputs.push(RuntimeInputV1::Artifact {
        artifact_id: request.mandate.constraints.transaction_costs_ref,
        storage_version: "1".into(),
        byte_count: market::count(costs.len() as u64),
        role: ArtifactInputRole::Parameters,
    });
    let f = fixture(
        NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: id,
            request: Box::new(request.clone()),
        },
        {
            if let Some((id, bytes)) = &liquidity {
                inputs.push(RuntimeInputV1::Artifact {
                    artifact_id: *id,
                    storage_version: "1".into(),
                    byte_count: market::count(bytes.len() as u64),
                    role: ArtifactInputRole::DataQuality,
                });
            }
            if let Some((id, bytes)) = &rolling {
                inputs.push(RuntimeInputV1::Artifact {
                    artifact_id: *id,
                    storage_version: "1".into(),
                    byte_count: market::count(bytes.len() as u64),
                    role: ArtifactInputRole::Parameters,
                });
            }
            inputs
        },
    );
    if let Some((id, bytes)) = liquidity {
        fs::write(f.input.join("objects").join(id.to_string()), bytes).unwrap();
    }
    if let Some((id, bytes)) = rolling {
        fs::write(f.input.join("objects").join(id.to_string()), bytes).unwrap();
    }
    fs::write(
        f.input.join("objects").join(
            request
                .mandate
                .constraints
                .transaction_costs_ref
                .to_string(),
        ),
        costs,
    )
    .unwrap();
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
fn retired_managed_build_cannot_publish_or_execute() {
    let f = portfolio_fixture(false, |_| {});
    assert_eq!(
        job::managed::execute(&f.input, &f.output)
            .unwrap_err()
            .to_string(),
        "NATIVE_RESEARCH_OPERATION_RETIRED"
    );
    assert!(!execute(&f));
    assert!(!f.output.join("index.json").exists());
}

#[test]
fn candidate_simulation_requires_original_targets_settings_and_causal_window() {
    for case in 0..18 {
        let (catalog, mut request) =
            market::market("0.001", if case >= 14 { 2 * 1440 + 20 } else { 20 });
        let mut source_selection = request.selection.clone();
        request.target_points.truncate(1);
        if case == 15 {
            request.target_points[0].cash_weight = "1".parse().unwrap();
            for target in &mut request.target_points[0].targets {
                target.weight = "0".parse().unwrap();
            }
        }
        request.selection.event_start_ns = request.target_points[0].asof_ns;
        let candidate = Id::new();
        let targets_id = Id::new();
        let settings_id = Id::new();
        let dataset_id = Id::new();
        let point = &request.target_points[0];
        let time = |n: DbCounter| chrono::DateTime::from_timestamp_nanos(n.get() as i64);
        let mut target = contracts::science::PortfolioTargetsV1 {
            schema_version: SchemaV1,
            candidate_id: candidate,
            base_currency: request.settings.base_currency.clone(),
            asof: time(point.asof_ns),
            valid_until: time(point.valid_until_ns),
            cash_weight: point.cash_weight.clone(),
            targets: point.targets.clone(),
        };
        let mut settings = request.settings.clone();
        let mut available = request.target_points[0].asof_ns;
        match case {
            1 => target.candidate_id = Id::new(),
            2 => target.cash_weight = "0.3".parse().unwrap(),
            3 => request.selection.event_start_ns = DbCounter::ZERO,
            4 => settings.starting_capital = "2000000".parse().unwrap(),
            9 => {
                request.selection.event_end_ns =
                    market::count(request.selection.event_end_ns.get() + 1);
                request.selection.decision_cutoff_ns = request.selection.event_end_ns;
            }
            10 => target.targets[0].weight = "0.5".parse().unwrap(),
            11 => request.target_points.push(request.target_points[0].clone()),
            12 | 13 => {
                available = market::count(available.get() + market::INTERVAL_NS);
                if case == 12 {
                    request.target_points[0].asof_ns = available;
                    request.selection.event_start_ns = available;
                }
            }
            16 => source_selection.event_start_ns = source_selection.event_end_ns,
            17 => source_selection.maximum_rows -= 1,
            _ => {}
        }
        let targets = serde_json::to_vec(&target).unwrap();
        let costs = serde_json::to_vec(&settings).unwrap();
        let mut data = dataset(dataset_id);
        if let RuntimeInputV1::Dataset { role, .. } = &mut data {
            *role = if case == 8 {
                DataPartition::Sealed
            } else {
                DataPartition::Forward
            };
        }
        let f = fixture(
            NativeTaskParametersV1::SimulateCandidate {
                schema_version: SchemaV1,
                candidate_id: candidate,
                candidate_available_ns: available,
                dataset_revision_id: dataset_id,
                target_artifact_id: targets_id,
                settings_artifact_id: settings_id,
                source_selection: source_selection.clone(),
                request: Box::new(request),
            },
            vec![
                data,
                RuntimeInputV1::Artifact {
                    artifact_id: targets_id,
                    storage_version: "1".into(),
                    byte_count: market::count(targets.len() as u64),
                    role: if case == 7 {
                        ArtifactInputRole::Model
                    } else {
                        ArtifactInputRole::Report
                    },
                },
                RuntimeInputV1::Artifact {
                    artifact_id: settings_id,
                    storage_version: "1".into(),
                    byte_count: market::count(costs.len() as u64),
                    role: ArtifactInputRole::Parameters,
                },
            ],
        );
        if case != 5 {
            fs::write(
                f.input.join("objects").join(targets_id.to_string()),
                targets,
            )
            .unwrap();
        }
        if case != 6 {
            fs::write(f.input.join("objects").join(settings_id.to_string()), costs).unwrap();
        }
        attach_catalog(&f, dataset_id, catalog.path());
        assert_eq!(
            execute(&f),
            matches!(case, 0 | 12 | 14 | 15),
            "Candidate simulation case {case}"
        );
        if matches!(case, 0 | 12 | 14 | 15) {
            let quality: contracts::execution::NativeDataQualityReportV1 =
                result(&f, "qz.data_quality");
            assert_eq!(quality.datasets.len(), 1);
            assert_eq!(quality.datasets[0].dataset_revision_id, dataset_id);
            assert_eq!(
                serde_json::to_value(&quality.datasets[0].selection).unwrap(),
                serde_json::to_value(&source_selection).unwrap()
            );
            if case == 12 {
                assert_eq!(
                    quality.datasets[0].row_count.get(),
                    40,
                    "original catalog window is not narrowed to Candidate availability"
                );
            }
            let report: contracts::science::NativeSimulationResultV1 =
                result(&f, "qz.native_simulation");
            assert_eq!(report.consumed_target_points.get(), 1);
            assert_eq!(report.orders.get() == 0, case == 15);
            assert_eq!(
                report.returns_kind,
                contracts::science::NativeReturnsKind::PortfolioDaily
            );
            if case >= 14 {
                assert_eq!(report.returns_status, contracts::evidence::MetricStatus::Ok);
                assert!(report.returns_reason.is_none());
                assert!(report.returns.len() >= 2);
                assert!(report
                    .returns
                    .iter()
                    .all(|r| r.value.is_some_and(f64::is_finite)));
                if case == 15 {
                    assert!(report.returns.iter().all(|r| r.value == Some(0.0)));
                    assert_eq!(report.positions.get(), 0);
                } else {
                    assert!(report
                        .returns
                        .iter()
                        .any(|r| r.value.is_some_and(|v| v != 0.0)));
                }
            } else {
                assert_eq!(
                    report.returns_status,
                    contracts::evidence::MetricStatus::InsufficientData
                );
                assert!(report.returns.is_empty());
            }
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
fn portfolio_sequence_binds_every_original_target_and_one_native_account() {
    use contracts::science::*;
    for case in 0..14 {
        let (catalog, mut request) = market::market("0.001", if case == 0 { 2900 } else { 20 });
        let mut selection = request.selection.clone();
        request.selection.event_start_ns = request.target_points[0].asof_ns;
        let mut sources = Vec::new();
        let mut targets = Vec::new();
        for point in &request.target_points {
            let candidate = Id::new();
            sources.push(NativePortfolioTargetSourceV1 {
                candidate_id: candidate,
                candidate_available_ns: point.asof_ns,
                target_artifact_id: Id::new(),
            });
            targets.push(PortfolioTargetsV1 {
                schema_version: SchemaV1,
                candidate_id: candidate,
                base_currency: request.settings.base_currency.clone(),
                asof: chrono::DateTime::from_timestamp_nanos(point.asof_ns.get() as i64),
                valid_until: chrono::DateTime::from_timestamp_nanos(
                    point.valid_until_ns.get() as i64
                ),
                cash_weight: point.cash_weight.clone(),
                targets: point.targets.clone(),
            });
        }
        let mut settings = request.settings.clone();
        match case {
            1 => targets[1].candidate_id = Id::new(),
            2 => targets[1].targets[0].weight = "0.1".parse().unwrap(),
            3 => sources[1].candidate_available_ns = market::instant(8),
            4 => settings.starting_capital = "2000000".parse().unwrap(),
            5 => {
                sources.pop();
                targets.pop();
                request.target_points.pop();
            }
            6 => sources[1].candidate_id = sources[0].candidate_id,
            7 => sources[1].target_artifact_id = sources[0].target_artifact_id,
            8 => {
                sources.reverse();
                targets.reverse();
                request.target_points.reverse();
            }
            9 => {
                let until = market::instant(6);
                targets[0].valid_until = chrono::DateTime::from_timestamp_nanos(until.get() as i64);
                request.target_points[0].valid_until_ns = until;
            }
            10 => selection.event_start_ns = market::instant(3),
            _ => {}
        }
        let dataset_id = Id::new();
        let settings_id = Id::new();
        let mut data = dataset(dataset_id);
        if let RuntimeInputV1::Dataset { role, .. } = &mut data {
            *role = if case == 11 {
                DataPartition::Sealed
            } else {
                DataPartition::Forward
            };
        }
        let settings = serde_json::to_vec(&settings).unwrap();
        let mut inputs = vec![
            data,
            RuntimeInputV1::Artifact {
                artifact_id: settings_id,
                storage_version: "1".into(),
                byte_count: market::count(settings.len() as u64),
                role: ArtifactInputRole::Parameters,
            },
        ];
        let mut objects = Vec::new();
        for (source, target) in sources.iter().zip(&targets) {
            let bytes = serde_json::to_vec(target).unwrap();
            inputs.push(RuntimeInputV1::Artifact {
                artifact_id: source.target_artifact_id,
                storage_version: "1".into(),
                byte_count: market::count(bytes.len() as u64),
                role: if case == 12 {
                    ArtifactInputRole::Model
                } else {
                    ArtifactInputRole::Report
                },
            });
            objects.push((source.target_artifact_id, bytes));
        }
        let f = fixture(
            NativeTaskParametersV1::SimulatePortfolioSequence {
                schema_version: SchemaV1,
                dataset_revision_id: dataset_id,
                source_selection: selection.clone(),
                sources,
                settings_artifact_id: settings_id,
                request: Box::new(request),
            },
            inputs,
        );
        fs::write(
            f.input.join("objects").join(settings_id.to_string()),
            settings,
        )
        .unwrap();
        for (index, (id, bytes)) in objects.into_iter().enumerate() {
            if case != 13 || index != 1 {
                fs::write(f.input.join("objects").join(id.to_string()), bytes).unwrap();
            }
        }
        attach_catalog(&f, dataset_id, catalog.path());
        assert_eq!(execute(&f), case == 0, "sequence source case {case}");
        if case == 0 {
            let report: NativeSimulationResultV1 = result(&f, "qz.native_simulation");
            assert_eq!(report.consumed_target_points.get(), 2);
            assert_eq!(
                report.canonical_result["accounts"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert!(report.orders.get() >= 4);
            assert_eq!(report.returns_status, contracts::evidence::MetricStatus::Ok);
            assert!(report.returns.len() >= 2);
            let quality: contracts::execution::NativeDataQualityReportV1 =
                result(&f, "qz.data_quality");
            assert_eq!(quality.datasets[0].selection, selection);
            assert_eq!(quality.datasets[0].row_count.get(), 5800);
        } else {
            assert!(!f.output.join("index.json").exists());
        }
    }
}

#[test]
fn native_managed_output_limit_fails_without_a_published_index() {
    let (catalog, request) = market::market("0.001", 20);
    let id = Id::new();
    let mut f = fixture(
        NativeTaskParametersV1::SimulatePortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: id,
            request: Box::new(request),
        },
        vec![dataset(id)],
    );
    attach_catalog(&f, id, catalog.path());
    f.spec.limits.output_bytes = Some(DbCounter::new(1).unwrap());
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

#[test]
fn retired_managed_cross_source_evaluation_cannot_publish_or_execute() {
    let (catalog, request, parts) = experiment_fixture::fixture(false);
    let dataset_id = Id::new();
    let model_id = Id::new();
    let model = experiment_fixture::feature_policy();
    let feature_ids = parts.iter().map(|_| Id::new()).collect::<Vec<_>>();
    let raw_parts = parts
        .iter()
        .map(|part| serde_json::to_vec(part).unwrap())
        .collect::<Vec<_>>();
    let mut inputs = vec![
        RuntimeInputV1::Dataset {
            revision_id: dataset_id,
            registered_ref: "synthetic-seconds-cross-source".into(),
            storage_version: "1".into(),
            role: DataPartition::Validation,
        },
        RuntimeInputV1::Artifact {
            artifact_id: model_id,
            storage_version: "1".into(),
            byte_count: market::count(model.len() as u64),
            role: ArtifactInputRole::Model,
        },
    ];
    for (&id, raw) in feature_ids.iter().zip(&raw_parts) {
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: market::count(raw.len() as u64),
            role: ArtifactInputRole::Parameters,
        });
    }
    let f = fixture(
        NativeTaskParametersV1::EvaluateExperiment {
            schema_version: SchemaV1,
            dataset_revision_id: dataset_id,
            model_artifact_id: model_id,
            feature_artifact_ids: feature_ids.clone(),
            request: Box::new(request),
        },
        inputs,
    );
    fs::write(f.input.join("objects").join(model_id.to_string()), model).unwrap();
    for (&id, raw) in feature_ids.iter().zip(raw_parts) {
        fs::write(f.input.join("objects").join(id.to_string()), raw).unwrap();
    }
    attach_catalog(&f, dataset_id, catalog.path());
    assert!(!execute(&f));
    assert!(!f.output.join("index.json").exists());
    assert_eq!(fs::read_dir(&f.output).unwrap().count(), 0);
}

#[test]
fn forward_daily_statistics_use_original_reports_and_fixed_native_methods() {
    use contracts::{forward::*, science::NativeReturnV1};
    let start = chrono::DateTime::from_timestamp(1_700_006_400, 0).unwrap();
    let end = start + chrono::Duration::days(3);
    let report = ForwardReportV1 {
        schema_version: SchemaV1,
        downstream_id: Id::new(),
        release_id: Id::new(),
        environment: ForwardEnvironmentV1::Paper,
        content: ForwardReportContentV1 {
            schema_version: SchemaV1,
            project_id: Id::new(),
            handoff_id: Id::new(),
            external_claim_id: "synthetic-claim".into(),
            issuer_version: "controlled-native/1".into(),
            stream_id: "daily-feedback".into(),
            sequence: market::count(1),
            message_revision: 1,
            supersedes_message_id: None,
            window_start: start,
            window_end: end,
            issued_at: end,
            complete: true,
            returns_frequency: Some(ForwardReturnsFrequencyV1::UtcDay),
            returns: [0.01, 0.02, -0.01]
                .into_iter()
                .enumerate()
                .map(|(i, value)| NativeReturnV1 {
                    timestamp_ns: market::count(
                        (start + chrono::Duration::days(i as i64 + 1))
                            .timestamp_nanos_opt()
                            .unwrap() as u64,
                    ),
                    value: Some(value),
                    reason_code: None,
                })
                .collect(),
        },
    };
    let message = ForwardMessageViewV1 {
        id: Id::new(),
        project_id: report.content.project_id,
        release_id: report.release_id,
        downstream_id: report.downstream_id,
        handoff_id: report.content.handoff_id,
        external_message_id: "original-daily".into(),
        stream_id: report.content.stream_id.clone(),
        sequence: market::count(1),
        message_revision: 1,
        supersedes_message_id: None,
        window_start: start,
        window_end: end,
        coverage_status: ForwardCoverageV1::Complete,
        observation_count: market::count(3),
        report_artifact_id: Id::new(),
        issued_at: end,
        received_at: end,
    };
    let mut correction = report.clone();
    correction.content.message_revision = 2;
    correction.content.supersedes_message_id = Some(message.id);
    correction.content.returns[0].value = Some(0.03);
    let mut corrected = message.clone();
    corrected.id = Id::new();
    corrected.external_message_id = "corrected-daily".into();
    corrected.report_artifact_id = Id::new();
    corrected.message_revision = 2;
    corrected.supersedes_message_id = Some(message.id);
    corrected.coverage_status = ForwardCoverageV1::Correction;
    let sources = vec![
        domain::forward::ForwardWindowSource {
            message: message.clone(),
            report: report.clone(),
        },
        domain::forward::ForwardWindowSource {
            message: corrected.clone(),
            report: correction.clone(),
        },
    ];
    let window = domain::forward::window(message.handoff_id, &message.stream_id, &sources)
        .unwrap()
        .view;
    let request = NativeForwardRequestV1 {
        window,
        sources: vec![message.clone(), corrected.clone()],
    };
    let parameters = NativeTaskParametersV1::EvaluateForward {
        schema_version: SchemaV1,
        request: Box::new(request.clone()),
    };
    let setup = |reports: &[ForwardReportV1]| {
        let bytes: Vec<_> = reports
            .iter()
            .map(|r| serde_json::to_vec(r).unwrap())
            .collect();
        let f = fixture(
            parameters.clone(),
            request
                .sources
                .iter()
                .zip(&bytes)
                .map(|(m, b)| RuntimeInputV1::Artifact {
                    artifact_id: m.report_artifact_id,
                    storage_version: "1".into(),
                    byte_count: market::count(b.len() as u64),
                    role: ArtifactInputRole::Report,
                })
                .collect(),
        );
        for (m, b) in request.sources.iter().zip(bytes) {
            fs::write(
                f.input
                    .join("objects")
                    .join(m.report_artifact_id.to_string()),
                b,
            )
            .unwrap();
        }
        f
    };
    let f = setup(&[report.clone(), correction.clone()]);
    assert!(execute(&f));
    let result: NativeForwardResultV1 = result(&f, "qz.forward_evaluation");
    assert_eq!(result.window.latest_message_ids, vec![corrected.id]);
    assert_eq!(result.window.complete_observations.get(), 3);
    assert!((result.statistics[0].value.unwrap() - 0.04 / 3.0).abs() < 1e-12);
    let evaluation = Id::new();
    let artifact = Id::new();
    let (metrics, _) =
        domain::forward::evaluation::metrics(evaluation, artifact, &request, &result).unwrap();
    assert_eq!(metrics[0].observation_count.get(), 3);
    assert_eq!(metrics[1].annualization_factor, Some(365.0));
    assert_eq!(metrics[2].source_artifact_id, artifact);
    assert_eq!(metrics[3].metric_code, "FORWARD_CAGR_365");
    assert_eq!(metrics[3].annualization_factor, Some(365.0));
    assert_eq!(metrics[3].method_id, "nautilus-analysis.CAGR");
    let annualized = ((1.03_f64 * 1.02 * 0.99).powf(365.0 / 3.0)) - 1.0;
    assert!((result.statistics[3].value.unwrap() - annualized).abs() < 1e-10);
    assert!(metrics
        .iter()
        .all(|m| m.frequency == "UTC_DAY" && m.scope == "forward"));
    let mut substituted = result.clone();
    substituted.window.latest_message_ids = vec![message.id];
    assert!(domain::forward::evaluation::binding(&request, &substituted).is_err());
    let mut wrong_method = result.clone();
    wrong_method.statistics[1].native_key = "Returns Volatility (252 days)".into();
    assert!(domain::forward::evaluation::shape(&wrong_method).is_err());
    let mut constant = correction.clone();
    for point in &mut constant.content.returns {
        point.value = Some(0.0);
    }
    let constant_job = setup(&[report.clone(), constant]);
    assert!(execute(&constant_job));
    let constant_result: NativeForwardResultV1 =
        self::result(&constant_job, "qz.forward_evaluation");
    assert_eq!(constant_result.statistics[0].value, Some(0.0));
    assert_eq!(constant_result.statistics[1].value, Some(0.0));
    assert_eq!(constant_result.statistics[2].value, None);
    assert_eq!(constant_result.statistics[3].value, Some(0.0));
    assert_eq!(
        constant_result.statistics[2].reason_code.as_deref(),
        Some("NATIVE_STATISTIC_UNAVAILABLE")
    );
    let (unavailable, _) =
        domain::forward::evaluation::metrics(evaluation, artifact, &request, &constant_result)
            .unwrap();
    assert_eq!(
        unavailable[2].status,
        contracts::evidence::MetricStatus::Failed
    );
    let mut changed = correction.clone();
    changed.content.project_id = Id::new();
    assert!(!execute(&setup(&[report.clone(), changed])));
    let mut missing = correction.clone();
    missing.content.returns.remove(1);
    assert!(!execute(&setup(&[report.clone(), missing])));
    let mut unknown = correction.clone();
    unknown.content.returns_frequency = None;
    assert!(!execute(&setup(&[report.clone(), unknown])));
    let mut partial = correction;
    partial.content.complete = false;
    assert!(!execute(&setup(&[report, partial])));
    let mut extra = f.spec.clone();
    extra.inputs.push(RuntimeInputV1::Artifact {
        artifact_id: Id::new(),
        storage_version: "1".into(),
        byte_count: market::count(10),
        role: ArtifactInputRole::Report,
    });
    assert!(domain::execution::task(&extra, &parameters).is_err());
}

#[test]
fn retired_managed_compilers_reject_before_reading_source() {
    for parameters in [
        NativeTaskParametersV1::CompileModel {
            schema_version: SchemaV1,
            code_artifact_id: Id::new(),
        },
        NativeTaskParametersV1::CompileFeatureModel {
            schema_version: SchemaV1,
            code_artifact_id: Id::new(),
        },
    ] {
        let f = fixture(parameters, vec![]);
        let error = job::managed::execute(&f.input, &f.output).unwrap_err();
        assert_eq!(error.to_string(), "NATIVE_RESEARCH_OPERATION_RETIRED");
        assert_eq!(fs::read_dir(&f.output).unwrap().count(), 0);
    }
}
