//! Controlled native protocol fixtures, not rustc/Wasmi execution or market evidence.
#![allow(dead_code)]
use super::{cycle_support, mission_support, runtime_support};
use contracts::{
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    experiments::ExperimentProposalV1,
    lifecycle::JobLimitsV1,
    runtime::{RuntimeArtifactSchemaV1, RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use store::{
    authority::Actor,
    lifecycle::{ClaimResult, RunLease},
    runtime::ProbePreparation,
    Store, StoreError,
};

pub fn limits() -> JobLimitsV1 {
    JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 1,
        cpu_seconds: DbCounter::new(10).unwrap(),
        wall_seconds: 60,
        memory_mib: 1024,
        output_bytes: DbCounter::new(1024 * 1024).unwrap(),
    }
}

pub async fn trial_usage(pool: &PgPool, lease: &RunLease) -> (i64, i64) {
    sqlx::query_as(
        "SELECT reserved_experiments,used_experiments FROM app.research_cycles WHERE id=$1",
    )
    .bind(lease.run.cycle_id.unwrap().as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn result_turn(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
) -> Result<bool, StoreError> {
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    store
        .prepare_mission_result_turn(
            lease.run.id,
            &lease.fence,
            move |id, size| {
                let objects = reading.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            move |object| async move {
                writing
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

pub async fn start(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let objects = f.objects.clone();
    store
        .start_experiment_compilation(
            lease.run.id,
            &lease.fence,
            experiment,
            &limits(),
            move |object| async move {
                objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

pub async fn forecast(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    let mut allocation = limits();
    allocation.experiments = 0;
    store
        .start_experiment_forecast(
            lease.run.id,
            &lease.fence,
            experiment,
            &allocation,
            move |id, size| {
                let objects = reading.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            move |object| async move {
                writing
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

pub async fn validation(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let mut allocation = limits();
    allocation.experiments = 0;
    store
        .start_experiment_validation(
            lease.run.id,
            &lease.fence,
            experiment,
            &allocation,
            |id, size| f.read(id, size),
            |object| async move {
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

async fn upload(
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    kind: ResearchArtifactKind,
) -> Id {
    let mut content = match kind {
        ResearchArtifactKind::Code => "#![no_std]\n#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }\n#[no_mangle] pub extern \"C\" fn predict(c:f64,p:f64,_f:f64,_s:f64,_v:f64,_o:f64,_h:f64,_l:f64)->f64 { c-p }".into(),
        ResearchArtifactKind::Parameters => serde_json::json!({"schema_version":1,"dataset_revision_id":f.data.discovery,"parameters":{"schema_version":1,"fast_period":2,"slow_period":5,"label_horizon_observations":5,"total_fuel":"1000000"}}).to_string(),
        _ => "{\"schema_version\":1,\"summary\":\"Controlled compilation input\"}".into(),
    };
    // Valid JSON whitespace also lets negative reads retain the original length;
    // those checks must reach semantic validation, not just the byte-count guard.
    if kind == ResearchArtifactKind::Parameters {
        content.push_str(&" ".repeat(128));
    }
    let upload = store
        .prepare_artifact_upload(
            actor,
            &Id::new().to_string(),
            &ArtifactCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                kind,
                content: content.clone(),
            },
        )
        .await
        .unwrap();
    f.objects.put(upload.id(), content.as_bytes()).unwrap();
    upload.publish().await.unwrap().resource.id
}

pub async fn setup(pool: &PgPool) -> (Store, Actor, cycle_support::Fixture, RunLease, Id) {
    let (store, actor, f, cycle, preparation) = mission_support::setup(pool).await;
    mission_support::complete(pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(lease)) = store
        .claim_mission(&message, "compiler-parent", 120)
        .await
        .unwrap()
    else {
        panic!("Mission lease required")
    };
    let experiment = propose(pool, &store, &actor, &f, cycle).await;
    (store, actor, f, *lease, experiment)
}
pub fn capabilities(
    now: chrono::DateTime<chrono::Utc>,
) -> contracts::runtime::RuntimeCapabilitiesV1 {
    let mut capabilities = runtime_support::capabilities(now);
    for name in [
        "qz.data_quality",
        "qz.wasm_model",
        "qz.model_compilation",
        "qz.native_forecast",
    ] {
        capabilities.artifact_schemas.push(RuntimeArtifactSchemaV1 {
            name: name.into(),
            version: "1".into(),
        });
    }
    capabilities
}

pub async fn propose(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    cycle: Id,
) -> Id {
    probe(store, actor, f).await;
    let family: uuid::Uuid =
        sqlx::query_scalar("SELECT family_id FROM app.evaluation_policies WHERE id=$1")
            .bind(f.brief.content.evaluation_policy_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    store
        .propose_experiment(
            actor,
            "native-compilation",
            &ExperimentProposalV1 {
                schema_version: SchemaV1,
                cycle_id: cycle,
                family_id: Id::try_from(family.to_string()).unwrap(),
                parent_experiment_id: None,
                hypothesis: "Compile the exact proposed code".into(),
                expected_failure_modes: "ABI or compiler rejection is not a scientific result"
                    .into(),
                proposal_artifact_id: upload(store, actor, f, ResearchArtifactKind::Report).await,
                parameter_artifact_id: upload(store, actor, f, ResearchArtifactKind::Parameters)
                    .await,
                code_artifact_id: Some(upload(store, actor, f, ResearchArtifactKind::Code).await),
            },
        )
        .await
        .unwrap()
        .resource
        .id
}

pub async fn probe(store: &Store, actor: &Actor, f: &cycle_support::Fixture) {
    probe_capabilities(store, actor, f, capabilities(chrono::Utc::now())).await;
}

pub async fn probe_capabilities(
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    capabilities: contracts::runtime::RuntimeCapabilitiesV1,
) {
    let revision = f.freeze.execution_context.runtime_revision;
    let ProbePreparation::Pending(ticket) = store
        .prepare_runtime_probe(
            actor,
            &Id::new().to_string(),
            f.data.runtime,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("probe required")
    };
    let objects = f.objects.clone();
    store.complete_runtime_probe(*ticket, RuntimeProbeOutcomeV1::Available { capabilities: Box::new(capabilities) },
        move |id, bytes| async move { objects.put(id, &bytes).map_err(|_| StoreError::Integrity) }).await.unwrap();
}

pub async fn complete_compilation(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    run: Id,
) -> Id {
    complete_native(pool, store, f, run, Observation::Compilation, None).await
}

pub async fn complete_forecast(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    run: Id,
) -> Id {
    complete_native(pool, store, f, run, Observation::Forecast, None).await
}

pub async fn complete_validation(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    run: Id,
    rows: usize,
    ic: f64,
) -> Id {
    complete_native(
        pool,
        store,
        f,
        run,
        Observation::Validation { rows, ic },
        None,
    )
    .await
}

pub async fn complete_sealed(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    lease: RunLease,
) -> Id {
    complete_native(
        pool,
        store,
        f,
        lease.run.id,
        Observation::Sealed,
        Some(lease),
    )
    .await
}

enum Observation {
    Compilation,
    Forecast,
    Validation { rows: usize, ic: f64 },
    Sealed,
}

async fn complete_native(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    run: Id,
    observation: Observation,
    claimed: Option<RunLease>,
) -> Id {
    use contracts::{execution::NativeModelCompilationV1, runtime_jobs::*, Revision};
    let lease = if let Some(lease) = claimed {
        lease
    } else {
        let message = store
            .read_native_run_messages(1, 100)
            .await
            .unwrap()
            .into_iter()
            .find(|m| m.run_id == run)
            .unwrap();
        let Some(ClaimResult::Leased(lease)) = store
            .claim_native_run(&message, "compile-result", 60)
            .await
            .unwrap()
        else {
            panic!("compiler lease required");
        };
        *lease
    };
    let job = store.native_job(run, &lease.fence).await.unwrap();
    assert!(store.begin_run_dispatch(run, &lease.fence).await.unwrap());
    let compilation = matches!(observation, Observation::Compilation);
    let model_ref = Id::new();
    let payloads = if compilation {
        let code = job
            .spec
            .inputs
            .iter()
            .find_map(|input| match input {
                RuntimeInputV1::Artifact {
                    artifact_id,
                    role: contracts::research::ArtifactInputRole::Code,
                    ..
                } => Some(*artifact_id),
                _ => None,
            })
            .unwrap();
        let wasm = b"\0asm\x01\0\0\0".to_vec();
        let report = serde_json::to_vec(&NativeModelCompilationV1 {
            schema_version: SchemaV1,
            code_artifact_id: code,
            model_storage_ref: model_ref,
            rustc_version: "rustc 1.98.1 (controlled observation)".into(),
            target: "wasm32-unknown-unknown".into(),
            abi: "predict(f64,f64,f64,f64,f64,f64,f64,f64)->f64".into(),
            module_bytes: DbCounter::new(wasm.len() as u64).unwrap(),
        })
        .unwrap();
        vec![wasm, report]
    } else if let Observation::Validation { rows, ic } = observation {
        let size = job
            .spec
            .inputs
            .iter()
            .find_map(|input| match input {
                RuntimeInputV1::Artifact {
                    artifact_id,
                    byte_count,
                    ..
                } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
                _ => None,
            })
            .unwrap();
        let contracts::execution::NativeTaskParametersV1::ValidateAlpha { request, .. } =
            serde_json::from_slice(
                &f.objects
                    .read(job.spec.parameters_artifact_id, size)
                    .unwrap(),
            )
            .unwrap()
        else {
            panic!("validation task required")
        };
        vec![serde_json::to_vec(&validation_report(&request, rows, ic)).unwrap()]
    } else {
        use contracts::{execution::NativeTaskParametersV1, science::*};
        let size = job
            .spec
            .inputs
            .iter()
            .find_map(|input| match input {
                RuntimeInputV1::Artifact {
                    artifact_id,
                    byte_count,
                    ..
                } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
                _ => None,
            })
            .unwrap();
        let task: NativeTaskParametersV1 = serde_json::from_slice(
            &f.objects
                .read(job.spec.parameters_artifact_id, size)
                .unwrap(),
        )
        .unwrap();
        let (request, calibration) = match task {
            NativeTaskParametersV1::EvaluateAlpha { request, .. } => (request, None),
            NativeTaskParametersV1::EvaluateSealedAlpha {
                request,
                calibration_artifact_id,
                ..
            } => {
                let id = calibration_artifact_id.unwrap();
                let size = job
                    .spec
                    .inputs
                    .iter()
                    .find_map(|i| match i {
                        RuntimeInputV1::Artifact {
                            artifact_id,
                            byte_count,
                            ..
                        } if *artifact_id == id => Some(*byte_count),
                        _ => None,
                    })
                    .unwrap();
                let model: NativeFrozenCalibrationV1 =
                    serde_json::from_slice(&f.objects.read(id, size).unwrap()).unwrap();
                (request.forecast, Some(model))
            }
            _ => panic!("forecast task required"),
        };
        let rows = if matches!(observation, Observation::Sealed) {
            1000
        } else {
            40
        };
        let mut points = Vec::new();
        for kind in &request.selection.bar_types {
            let instrument_id = kind.rsplitn(5, '-').nth(4).unwrap();
            for ordinal in 0..rows {
                let predicted = ordinal + 1 >= request.parameters.slow_period;
                let labelled =
                    predicted && ordinal + request.parameters.label_horizon_observations < rows;
                let time =
                    request.selection.event_start_ns.get() + u64::from(ordinal) * 60_000_000_000;
                points.push(NativeForecastPointV1 {
                    instrument_id: instrument_id.into(),
                    ordinal,
                    event_ns: DbCounter::new(time).unwrap(),
                    available_ns: DbCounter::new(time + 1).unwrap(),
                    forecast: predicted.then_some(f64::from(ordinal) / 1000.0),
                    forecast_reason: (!predicted).then_some(ForecastMissingReason::IndicatorWarmup),
                    label_return: labelled.then_some(if calibration.is_some() {
                        f64::from(ordinal) / 2000.0
                    } else {
                        0.001
                    }),
                    label_available_ns: labelled.then(|| {
                        DbCounter::new(
                            time + u64::from(request.parameters.label_horizon_observations)
                                * 60_000_000_000
                                + 1,
                        )
                        .unwrap()
                    }),
                    label_reason: if labelled {
                        None
                    } else {
                        Some(if predicted {
                            ForecastMissingReason::LabelNotComplete
                        } else {
                            ForecastMissingReason::IndicatorWarmup
                        })
                    },
                });
            }
        }
        let forecast = NativeForecastResultV1 {
            schema_version: SchemaV1,
            native_versions: std::collections::BTreeMap::from([
                ("nautilus-indicators".into(), "0.63.0".into()),
                ("nautilus-persistence".into(), "0.63.0".into()),
                ("wasmi".into(), "2.0.0".into()),
            ]),
            consumed_fuel: DbCounter::new(u64::from(rows)).unwrap(),
            points,
        };
        let bytes = if let Some(model) = calibration {
            use contracts::evidence::MetricStatus;
            // Controlled protocol values, not execution of the scientific engine.
            let report = NativeAlphaSealedResultV1 {
                schema_version: SchemaV1,
                expected_returns: forecast.points.iter().map(|p| p.forecast).collect(),
                calibration_source_report_artifact_id: Some(model.source_report_artifact_id),
                calibration_fit_end_available_ns: Some(model.fit_end_available_ns),
                native_versions: std::collections::BTreeMap::from([
                    ("ndarray".into(), "0.17.1".into()),
                    ("ndarray-stats".into(), "0.7.0".into()),
                ]),
                assets: request
                    .selection
                    .bar_types
                    .iter()
                    .map(|bar_type| {
                        let instrument_id = bar_type.rsplitn(5, '-').nth(4).unwrap().to_string();
                        NativeSealedAssetMetricsV1 {
                            observation_count: DbCounter::new(
                                forecast
                                    .points
                                    .iter()
                                    .filter(|p| {
                                        p.instrument_id == instrument_id && p.label_return.is_some()
                                    })
                                    .count() as u64,
                            )
                            .unwrap(),
                            instrument_id,
                            bar_type: bar_type.clone(),
                            metrics: vec![
                                NativeValidationMetricV1 {
                                    kind: NativeAlphaMetricKind::PearsonIc,
                                    value: Some(0.15),
                                    status: MetricStatus::Ok,
                                    reason_code: None,
                                },
                                NativeValidationMetricV1 {
                                    kind: NativeAlphaMetricKind::ReturnRmse,
                                    value: Some(0.02),
                                    status: MetricStatus::Ok,
                                    reason_code: None,
                                },
                            ],
                        }
                    })
                    .collect(),
                forecast,
            };
            serde_json::to_vec(&report).unwrap()
        } else {
            serde_json::to_vec(&forecast).unwrap()
        };
        vec![bytes]
    };
    let outputs = job
        .spec
        .requested_output_schemas
        .iter()
        .zip(payloads)
        .map(|(schema, bytes)| {
            let contract = native_output_contract(&schema.name, &schema.version).unwrap();
            let output = RuntimeOutputV1 {
                kind: contract.kind,
                schema: schema.clone(),
                storage_ref: if !compilation || contract.kind == RuntimeOutputKind::Model {
                    model_ref
                } else {
                    Id::new()
                },
                storage_version: Revision::INITIAL,
                byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
                media_type: contract.media_type.into(),
            };
            (output, bytes)
        })
        .collect::<Vec<_>>();
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id,
        input_set_id: job.run.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: runtime_support::capabilities(now).engine_versions,
        started_at: Some(job.submitted_not_before),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::new(
                (now - job.submitted_not_before).num_milliseconds().max(0) as u64,
            )
            .unwrap(),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: DbCounter::new(outputs.iter().map(|(o, _)| o.byte_count.get()).sum())
                .unwrap(),
        },
        artifacts: outputs.iter().map(|(o, _)| o.clone()).collect(),
        error: None,
    };
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    let result = store
        .publish_native_result(
            run,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            store::lifecycle::native::NativePayloads::Verified(outputs),
            move |id, size| {
                let reading = reading.clone();
                async move { reading.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            move |batch| async move {
                for object in batch {
                    writing
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)?;
                }
                Ok(())
            },
        )
        .await
        .unwrap();
    assert_eq!(result.resource.state, contracts::runs::RunState::Succeeded);
    let id:uuid::Uuid=sqlx::query_scalar("SELECT artifact_id FROM app.run_native_outputs WHERE attempt_id=$1 AND remote_storage_ref=$2").bind(lease.fence.attempt_id.as_uuid()).bind(model_ref.as_uuid()).fetch_one(pool).await.unwrap();
    Id::try_from(id.to_string()).unwrap()
}

/// Controlled producer observations for real PG publication tests. Only split
/// membership uses the native adapter; scores, calibration and metrics are NOT executed science.
fn validation_report(
    request: &contracts::science::NativeAlphaValidationRequestV1,
    rows: usize,
    ic: f64,
) -> contracts::science::NativeAlphaValidationResultV1 {
    use contracts::{evidence::MetricStatus, science::*};
    let count = |n| DbCounter::new(n).unwrap();
    let warmup = request.forecast.parameters.slow_period as usize - 1;
    let horizon = request.forecast.parameters.label_horizon_observations as usize;
    let native = domain::execution::validation::validation_folds(
        &request.split_policy,
        rows - warmup - horizon,
    )
    .unwrap();
    let mut folds = Vec::new();
    let mut unique = std::collections::BTreeSet::new();
    for (asset, bar_type) in request.forecast.selection.bar_types.iter().enumerate() {
        let instrument = bar_type.rsplitn(5, '-').nth(4).unwrap();
        for (index, fold) in native.iter().enumerate() {
            let mut points = Vec::new();
            for ordinal in fold.test.iter().map(|n| n + warmup) {
                unique.insert((asset, ordinal));
                let time = request.forecast.selection.event_start_ns.get()
                    + ordinal as u64 * 60_000_000_000;
                let prediction = ordinal as f64 / 1000.0;
                points.push(NativeValidationPointV1 {
                    observation: NativeForecastPointV1 {
                        instrument_id: instrument.into(),
                        ordinal: ordinal as u32,
                        event_ns: count(time),
                        available_ns: count(time + 1),
                        forecast: Some(prediction),
                        forecast_reason: None,
                        label_return: Some(prediction),
                        label_available_ns: Some(count(time + horizon as u64 * 60_000_000_000 + 1)),
                        label_reason: None,
                    },
                    expected_return: Some(prediction),
                });
            }
            folds.push(NativeValidationFoldV1 {
                instrument_id: instrument.into(),
                bar_type: bar_type.clone(),
                source_row_count: count(rows as u64),
                fold_index: index as u16,
                training_ordinals: fold.train.iter().map(|n| (n + warmup) as u32).collect(),
                training_end_available_ns: count(
                    request.forecast.selection.event_start_ns.get()
                        + (fold.train[fold.train.len() - 1] + warmup + horizon) as u64
                            * 60_000_000_000
                        + 1,
                ),
                test_points: points,
                calibration: Some(NativeCalibrationV1 {
                    status: MetricStatus::Ok,
                    reason_code: None,
                    intercept: Some(0.0),
                    slope: Some(1.0),
                    training_observations: count(fold.train.len() as u64),
                }),
                metrics: vec![
                    NativeValidationMetricV1 {
                        kind: NativeAlphaMetricKind::PearsonIc,
                        value: Some(ic),
                        status: MetricStatus::Ok,
                        reason_code: None,
                    },
                    NativeValidationMetricV1 {
                        kind: NativeAlphaMetricKind::ReturnRmse,
                        value: Some(0.0),
                        status: MetricStatus::Ok,
                        reason_code: None,
                    },
                ],
            });
        }
    }
    NativeAlphaValidationResultV1 {
        schema_version: SchemaV1,
        native_versions: std::collections::BTreeMap::from([
            ("nautilus-indicators".into(), "0.63.0".into()),
            ("nautilus-persistence".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
            ("solow-cv".into(), "0.7.3".into()),
            ("linregress".into(), "0.5.4".into()),
            ("ndarray-stats".into(), "0.7.0".into()),
        ]),
        consumed_fuel: count(1000),
        unique_test_observations: count(unique.len() as u64),
        folds,
    }
}
