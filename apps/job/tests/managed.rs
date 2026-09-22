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
                settlements: Vec::new(),
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
                settlements: Vec::new(),
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

#[test]
fn rolling_portfolio_managed_binds_original_objects_and_reports_infeasibility() {
    use contracts::science::NativePortfolioStudyResultV1;
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
        for rejected in [DataPartition::Sealed, DataPartition::Forward] {
            let mut wrong_role = f.spec.clone();
            let RuntimeInputV1::Dataset { role, .. } = &mut wrong_role.inputs[0] else {
                unreachable!()
            };
            *role = rejected;
            assert!(domain::execution::task(&wrong_role, &parameters).is_err());
        }
        let mut missing_model = f.spec.clone();
        missing_model.inputs.remove(1);
        assert!(domain::execution::task(&missing_model, &parameters).is_err());
        if liquidity {
            let NativeTaskParametersV1::StudyPortfolio { request, .. } = &parameters else {
                unreachable!()
            };
            let mut missing = f.spec.clone();
            missing.inputs.retain(|i| {
                !matches!(i, RuntimeInputV1::Artifact { artifact_id, .. }
                if Some(*artifact_id) == request.mandate.constraints.liquidity_ref)
            });
            assert!(domain::execution::task(&missing, &parameters).is_err());
            let mut missing_calendar = f.spec.clone();
            missing_calendar.inputs.retain(|i| {
                !matches!(i, RuntimeInputV1::Artifact { artifact_id, .. }
                if *artifact_id == request.calendar.as_ref().unwrap().artifact_id)
            });
            assert!(domain::execution::task(&missing_calendar, &parameters).is_err());
        }
        assert!(execute(&f));
        let report: NativePortfolioStudyResultV1 = result(&f, "qz.portfolio_study");
        assert!(report
            .frames
            .iter()
            .all(|frame| frame.bar_notionals.len() == if liquidity { 2 } else { 0 }));
        let outputs = output_bytes(&f);
        let index = outputs
            .iter()
            .position(|(a, _)| a.schema.name == contracts::portfolio_history::NAME)
            .unwrap();
        let batch = contracts::portfolio_history::read(&outputs[index].1).unwrap();
        assert_eq!(batch.num_rows(), report.frames.len() * 2);
        assert_eq!(
            batch.column(6).null_count(),
            if infeasible { batch.num_rows() } else { 0 }
        );
        assert_eq!(
            batch.column(7).null_count(),
            if infeasible { batch.num_rows() } else { 0 }
        );
        let rejects = |outputs: &[(RuntimeOutputV1, Vec<u8>)]| {
            assert!(domain::execution::output_bindings(
                &parameters,
                None,
                f.spec.deadline_at - chrono::Duration::seconds(60),
                chrono::Utc::now(),
                outputs
            )
            .is_err());
        };
        let mut missing = outputs.clone();
        missing.remove(index);
        rejects(&missing);
        let mut columns = batch.columns().to_vec();
        columns[6] = std::sync::Arc::new(
            arrow_array::Decimal128Array::from(vec![
                Some(123_456_789_012_345_678_i128);
                batch.num_rows()
            ])
            .with_precision_and_scale(38, 18)
            .unwrap(),
        );
        let changed_weights = arrow_array::RecordBatch::try_new(batch.schema(), columns).unwrap();
        let mut wrong_weights = outputs.clone();
        wrong_weights[index].1.clear();
        contracts::portfolio_history::write(&mut wrong_weights[index].1, &changed_weights).unwrap();
        wrong_weights[index].0.byte_count = market::count(wrong_weights[index].1.len() as u64);
        rejects(&wrong_weights);
        let NativeTaskParametersV1::StudyPortfolio { request, .. } = &parameters else {
            unreachable!()
        };
        let mut changed = report.clone();
        changed.frames[0].cutoff_ns = market::count(changed.frames[0].cutoff_ns.get() + 1);
        let batch = contracts::portfolio_history::batch(request, &changed).unwrap();
        let mut wrong = outputs.clone();
        wrong[index].1.clear();
        contracts::portfolio_history::write(&mut wrong[index].1, &batch).unwrap();
        wrong[index].0.byte_count = market::count(wrong[index].1.len() as u64);
        rejects(&wrong);
        let mut truncated = outputs.clone();
        truncated[index].1.pop();
        truncated[index].0.byte_count = market::count(truncated[index].1.len() as u64);
        rejects(&truncated);
        let mut wrong_media = outputs.clone();
        wrong_media[index].0.media_type = "application/json".into();
        rejects(&wrong_media);
        assert_eq!(report.frames.len(), if infeasible { 1 } else { 3 });
        assert_eq!(report.simulation.is_none(), infeasible);
        assert_eq!(report.simulation_request.is_none(), infeasible);
        if let Some(simulation) = report.simulation {
            assert_eq!(simulation.consumed_target_points.get(), 3);
            assert_eq!(
                simulation.returns_status,
                contracts::evidence::MetricStatus::Ok
            );
        }
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
fn managed_portfolio_plans_from_original_native_one_tick_references() {
    let mut previous = contracts::DecimalValue::zero();
    for probability in ["0", "0.5", "1"] {
        let f = portfolio_fixture(false, |request| {
            let contracts::portfolio::NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
                &mut request.execution_settings.fill_model
            else {
                unreachable!()
            };
            parameters.prob_slippage = probability.parse().unwrap();
        });
        assert!(execute(&f));
        let report: contracts::science::NativePortfolioBuildResultV1 =
            result(&f, "qz.native_portfolio");
        assert_eq!(report.slippage_references.is_empty(), probability == "0");
        let cost = &report.input.assets[0].transaction_cost_rate;
        if probability != "0" {
            assert!(cost.as_decimal() > previous.as_decimal());
            assert_eq!(
                report.slippage_references[0].close_price,
                "1.02".parse().unwrap()
            );
            assert_eq!(
                report.slippage_references[0].event_ns,
                report.input.forecasts.forecast_asof_ns
            );
        }
        previous = cost.clone();
    }
}

#[test]
fn portfolio_requires_unchanged_original_execution_settings() {
    for case in 0..5 {
        let mut f = portfolio_fixture(false, |request| {
            if case == 4 {
                request.execution_settings.fee_rates[0].taker = "0.01".parse().unwrap();
                request.assets[0].transaction_cost_rate = "0.01".parse().unwrap();
            }
        });
        let task: NativeTaskParametersV1 = serde_json::from_slice(
            &fs::read(
                f.input
                    .join("objects")
                    .join(f.spec.parameters_artifact_id.to_string()),
            )
            .unwrap(),
        )
        .unwrap();
        let NativeTaskParametersV1::BuildPortfolio { request, .. } = task else {
            unreachable!()
        };
        let costs_id = request.mandate.constraints.transaction_costs_ref;
        let path = f.input.join("objects").join(costs_id.to_string());
        match case {
            0 => {}
            1 => {
                let mut costs = request.execution_settings;
                let contracts::portfolio::NativeModelRefV1::NautilusDefaultFill {
                    parameters, ..
                } = &mut costs.fill_model
                else {
                    unreachable!()
                };
                parameters.random_seed = market::count(8);
                let bytes = serde_json::to_vec(&costs).unwrap();
                fs::write(&path, &bytes).unwrap();
                for input in &mut f.spec.inputs {
                    if let RuntimeInputV1::Artifact {
                        artifact_id,
                        byte_count,
                        ..
                    } = input
                    {
                        if *artifact_id == costs_id {
                            *byte_count = market::count(bytes.len() as u64);
                        }
                    }
                }
            }
            2 => {
                for input in &mut f.spec.inputs {
                    if let RuntimeInputV1::Artifact {
                        artifact_id, role, ..
                    } = input
                    {
                        if *artifact_id == costs_id {
                            *role = ArtifactInputRole::Report;
                        }
                    }
                }
            }
            3 => fs::remove_file(&path).unwrap(),
            _ => {}
        }
        fs::write(
            f.input.join("spec.json"),
            serde_json::to_vec(&f.spec).unwrap(),
        )
        .unwrap();
        assert_eq!(execute(&f), case == 0, "cost source case {case}");
        if case != 0 {
            assert!(!f.output.join("index.json").exists());
        }
    }
}

#[test]
fn managed_portfolio_uses_original_measured_liquidity_and_rejects_changed_copies() {
    for case in 0..7 {
        let mut f = portfolio_fixture(false, |request| {
            let assumption = contracts::execution_assumptions::BarLiquidityAssumptionV1 {
                schema_version: SchemaV1,
                report_artifact_id: Id::new(),
                maximum_age_seconds: 86400,
                participation_limit: "0.00001".parse().unwrap(),
            };
            request.mandate.constraints.liquidity_ref = Some(assumption.report_artifact_id);
            request.mandate.constraints.max_participation =
                Some(assumption.participation_limit.clone());
            request.bar_liquidity = Some(contracts::science::NativePortfolioLiquidityV1 {
                schema_version: SchemaV1,
                assumption,
                source: NativeDatasetSelectionV1 {
                    settlements: Vec::new(),
                    dataset_revision_id: Id::new(),
                    selection: request.selection.clone(),
                },
            });
        });
        let path = f
            .input
            .join("objects")
            .join(f.spec.parameters_artifact_id.to_string());
        let mut task: NativeTaskParametersV1 =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let NativeTaskParametersV1::BuildPortfolio { request, .. } = &mut task else {
            unreachable!()
        };
        let binding = request.bar_liquidity.as_mut().unwrap();
        match case {
            0 => {}
            1 => request.assets[0].available_notional = Some("1".parse().unwrap()),
            2 => binding.source.dataset_revision_id = Id::new(),
            3 => binding.assumption.maximum_age_seconds = 1,
            4 => {
                let report = binding.assumption.report_artifact_id;
                for input in &mut f.spec.inputs {
                    if let RuntimeInputV1::Artifact {
                        artifact_id, role, ..
                    } = input
                    {
                        if *artifact_id == report {
                            *role = ArtifactInputRole::Report;
                        }
                    }
                }
            }
            _ => {
                let report_path = f
                    .input
                    .join("objects")
                    .join(binding.assumption.report_artifact_id.to_string());
                let mut report: NativeDataQualityReportV1 =
                    serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
                if case == 5 {
                    report.datasets[0].last_bar_notionals.as_mut().unwrap()[0].notional_value =
                        "1".parse().unwrap();
                } else {
                    report.datasets[0].last_bar_notionals = None;
                }
                let bytes = serde_json::to_vec(&report).unwrap();
                fs::write(report_path, &bytes).unwrap();
                for input in &mut f.spec.inputs {
                    if let RuntimeInputV1::Artifact {
                        artifact_id,
                        byte_count,
                        ..
                    } = input
                    {
                        if *artifact_id == binding.assumption.report_artifact_id {
                            *byte_count = market::count(bytes.len() as u64);
                        }
                    }
                }
            }
        }
        let bytes = serde_json::to_vec(&task).unwrap();
        fs::write(&path, &bytes).unwrap();
        for input in &mut f.spec.inputs {
            if let RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } = input
            {
                if *artifact_id == f.spec.parameters_artifact_id {
                    *byte_count = market::count(bytes.len() as u64);
                }
            }
        }
        fs::write(
            f.input.join("spec.json"),
            serde_json::to_vec(&f.spec).unwrap(),
        )
        .unwrap();
        assert_eq!(execute(&f), case == 0, "liquidity case {case}");
        if case == 0 {
            let report: contracts::science::NativePortfolioBuildResultV1 =
                result(&f, "qz.native_portfolio");
            assert!(report.allocation.targets.is_some());
            domain::portfolio::allocation_result(&report.input, &report.allocation).unwrap();
            assert!(report
                .input
                .assets
                .iter()
                .all(|a| a.available_notional.is_some()));
        } else {
            assert!(!f.output.join("index.json").exists());
        }
    }
}

#[test]
fn managed_build_measures_rolling_policy_and_rejects_unbound_or_expired_inputs() {
    use contracts::science::{NativePortfolioBuildResultV1, NativeRollingBarLiquidityPolicyV1};
    for case in 0..6 {
        let mut f = portfolio_fixture(false, |request| {
            let policy = NativeRollingBarLiquidityPolicyV1 {
                schema_version: SchemaV1,
                maximum_age_seconds: if case == 3 { 1 } else { 3600 },
                participation_limit: "0.00001".parse().unwrap(),
            };
            request.mandate.constraints.liquidity_ref = Some(Id::new());
            request.mandate.constraints.max_participation =
                Some(policy.participation_limit.clone());
            request.rolling_liquidity = Some(policy);
        });
        let path = f
            .input
            .join("objects")
            .join(f.spec.parameters_artifact_id.to_string());
        let mut task: NativeTaskParametersV1 =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let NativeTaskParametersV1::BuildPortfolio { request, .. } = &mut task else {
            unreachable!()
        };
        let id = request.mandate.constraints.liquidity_ref.unwrap();
        let policy_path = f.input.join("objects").join(id.to_string());
        match case {
            1 => fs::remove_file(&policy_path).unwrap(),
            2 => {
                for input in &mut f.spec.inputs {
                    if let RuntimeInputV1::Artifact {
                        artifact_id, role, ..
                    } = input
                    {
                        if *artifact_id == id {
                            *role = ArtifactInputRole::DataQuality;
                        }
                    }
                }
            }
            4 => request.assets[0].available_notional = Some("1".parse().unwrap()),
            5 => {
                let mut original = request.rolling_liquidity.clone().unwrap();
                original.participation_limit = "0.00002".parse().unwrap();
                fs::write(&policy_path, serde_json::to_vec(&original).unwrap()).unwrap();
            }
            _ => {}
        }
        if case == 4 {
            assert!(domain::execution::portfolio_build_request(request).is_err());
        }
        fs::write(&path, serde_json::to_vec(&task).unwrap()).unwrap();
        fs::write(
            f.input.join("spec.json"),
            serde_json::to_vec(&f.spec).unwrap(),
        )
        .unwrap();
        assert_eq!(execute(&f), case == 0, "rolling Build case {case}");
        if case != 0 {
            assert!(!f.output.join("index.json").exists());
            continue;
        }
        let report: NativePortfolioBuildResultV1 = result(&f, "qz.native_portfolio");
        assert_eq!(report.bar_notionals.len(), 2);
        let NativeTaskParametersV1::BuildPortfolio { request, .. } = &task else {
            unreachable!()
        };
        for (asset, value) in report.input.assets.iter().zip(&report.bar_notionals) {
            assert_eq!(
                asset.available_notional.as_ref(),
                Some(&value.notional_value)
            );
            assert!(value.notional_value.is_positive());
        }
        for changed_field in 0..4 {
            let mut changed = report.clone();
            match changed_field {
                0 => changed.bar_notionals.clear(),
                1 => changed.bar_notionals[0].notional_value = "0".parse().unwrap(),
                2 => changed.bar_notionals[0].currency = "EUR".into(),
                _ => changed.bar_notionals[0].event_ns = DbCounter::ZERO,
            }
            assert!(domain::execution::portfolio_build_result(request, &changed).is_err());
        }
    }
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
fn original_native_allocation_enters_one_shared_account_without_future_build_rows() {
    use contracts::science::*;
    let unsupported = portfolio_fixture(false, |request| {
        request.execution_settings.account_kind = NativeAccountKind::Cash
    });
    assert!(!execute(&unsupported));
    assert!(!unsupported.output.join("index.json").exists());
    let cutoff = market::instant(10);
    let build = portfolio_fixture(false, |request| {
        // This explicit capital keeps one native lot below the original exposure tolerance.
        request.mandate.capital_assumption = "10000000".parse().unwrap();
        request.execution_settings.starting_capital = request.mandate.capital_assumption.clone();
        request.selection.event_end_ns = cutoff;
        request.selection.decision_cutoff_ns = cutoff;
        request.current_weights.asof_ns = cutoff;
        request.current_weights.available_ns = cutoff;
        request.current_weights.valid_until_ns = market::count(cutoff.get() + 1);
        // The declared initial hypothesis matches the native account's all-cash start.
        request.current_weights.cash_weight = "1".parse().unwrap();
        for (asset, weight) in request
            .assets
            .iter_mut()
            .zip(&mut request.current_weights.weights)
        {
            asset.current_weight = "0".parse().unwrap();
            weight.weight = "0".parse().unwrap();
        }
    });
    assert!(execute(&build));
    let report: NativePortfolioBuildResultV1 = result(&build, "qz.native_portfolio");
    assert_eq!(report.input.forecasts.members.len(), 2);
    assert_ne!(
        report.input.forecasts.members[0].alpha_id,
        report.input.forecasts.members[1].alpha_id
    );
    assert!(report
        .input
        .return_history
        .available_ns
        .iter()
        .all(|time| *time <= cutoff));
    assert!(report
        .input
        .forecasts
        .members
        .iter()
        .all(|member| member.available_ns <= cutoff));
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &fs::read(
            build
                .input
                .join("objects")
                .join(build.spec.parameters_artifact_id.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::BuildPortfolio {
        dataset_revision_id,
        request,
        ..
    } = task
    else {
        unreachable!()
    };
    let until = market::count(
        cutoff.get()
            + u64::from(request.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000,
    );
    let simulation = NativeSimulationRequestV1 {
        settlements: Vec::new(),
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            event_start_ns: cutoff,
            event_end_ns: until,
            decision_cutoff_ns: until,
            ..request.selection.clone()
        },
        settings: request.execution_settings.clone(),
        target_points: vec![NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: cutoff,
            valid_until_ns: until,
            targets: report.allocation.targets.unwrap(),
            cash_weight: report.allocation.cash_weight.unwrap(),
        }],
    };
    let mut unsupported = simulation.clone();
    unsupported.settings.account_kind = NativeAccountKind::Cash;
    let catalog = build
        .input
        .join("catalogs")
        .join(dataset_revision_id.to_string());
    let error = job::simulation::simulate(&catalog, &unsupported).unwrap_err();
    assert!(error
        .to_string()
        .contains("execution_assumption_account_instrument"));
    let f = fixture(
        NativeTaskParametersV1::SimulatePortfolio {
            schema_version: SchemaV1,
            dataset_revision_id,
            request: Box::new(simulation),
        },
        vec![dataset(dataset_revision_id)],
    );
    attach_catalog(
        &f,
        dataset_revision_id,
        &build
            .input
            .join("catalogs")
            .join(dataset_revision_id.to_string()),
    );
    assert!(execute(&f));
    let outcome: NativeSimulationResultV1 = result(&f, "qz.native_simulation");
    assert_eq!(outcome.consumed_target_points.get(), 1);
    assert!(outcome.orders.get() > 0);
    assert_eq!(
        outcome.canonical_result["accounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        outcome.returns_status,
        contracts::evidence::MetricStatus::InsufficientData
    );
    assert!(
        outcome.returns.is_empty(),
        "intraday execution is not daily scientific PASS"
    );
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
