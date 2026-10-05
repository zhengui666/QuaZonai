//! Actual Store/PGMQ/object transactions using controlled producer observations.
//! Native WASM/account replay is covered separately in job/strategy_composition.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
use cycle_support::execution_models;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "../../../tests/support/external_experiments.rs"]
mod external_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "support/recorded_feature.rs"]
mod recorded_fixture;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
#[path = "../../../tests/support/strategy_composition.rs"]
mod strategy_fixture;
use contracts::{
    control::ListQuery,
    execution::NativeTaskParametersV1,
    lifecycle::RunCancelV1,
    research::{DataPartition, InputItemV1, InputPurpose, InputSetCreate},
    runtime_jobs::*,
    science::*,
    strategy_portfolio::*,
    DbCounter, Id, Revision, SchemaV1,
};
use sqlx::PgPool;
use store::{
    authority::Actor,
    lifecycle::{ClaimResult, RunMessage},
    Store, StoreError,
};

struct Fixture {
    store: Store,
    actor: Actor,
    data: cycle_support::Fixture,
    build: StrategyPortfolioBuildV1,
    source: NativeExperimentEvaluationResultV1,
}

async fn setup(pool: &PgPool) -> Fixture {
    setup_with_delivery(pool, false).await
}

async fn setup_for_delivery(pool: &PgPool) -> Fixture {
    setup_with_delivery(pool, true).await
}

async fn setup_with_delivery(pool: &PgPool, delivery: bool) -> Fixture {
    setup_with_sources(pool, delivery, false).await
}

async fn setup_with_sources(pool: &PgPool, delivery: bool, recorded: bool) -> Fixture {
    let (store, actor, data, experiment, evaluate) = if recorded {
        let (descriptor, parts) = recorded_fixture::parts();
        external_support::setup_recorded(
            pool,
            if delivery {
                contracts::research::DataUse::ResearchAndPaper
            } else {
                contracts::research::DataUse::Research
            },
            descriptor,
            parts,
        )
        .await
    } else if delivery {
        external_support::setup_for_paper(pool, true).await
    } else {
        external_support::setup(pool).await
    };
    let compile = external_support::evaluate(&store, &actor, &data, &evaluate)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_compilation(pool, &store, &data, compile).await;
    external_support::advance(&store, &data, compile)
        .await
        .unwrap();
    let evaluation = store
        .experiment(&actor, experiment.id)
        .await
        .unwrap()
        .run_id
        .unwrap();
    experiment_support::complete_external_evaluation(pool, &store, &data, evaluation, None).await;
    external_support::advance(&store, &data, evaluation)
        .await
        .unwrap();
    let original = store.experiment(&actor, experiment.id).await.unwrap();
    let source = store
        .experiment_evaluation(&actor, experiment.id, |id, size| data.read(id, size))
        .await
        .unwrap();
    let alpha = store
        .adopt_experiment_alpha(
            &actor,
            "strategy-alpha",
            &StrategyAlphaAdoptIntentV1 {
                schema_version: SchemaV1,
                experiment_id: experiment.id,
                request: StrategyAlphaAdoptV1 {
                    schema_version: SchemaV1,
                    expected_revision: original.revision,
                    name: "Controlled target policy".into(),
                    source_fold_index: 0,
                },
            },
            |id, size| data.read(id, size),
        )
        .await
        .unwrap()
        .resource;
    let mut cap = external_support::capabilities();
    cap.engine_versions
        .insert("strategy-composition".into(), "1".into());
    cap.artifact_schemas
        .push(contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.strategy_portfolio".into(),
            version: "1".into(),
        });
    experiment_support::probe_capabilities(&store, &actor, &data, cap).await;
    let mut content = strategy_fixture::mandate(&source.request.settings);
    content.universe_version_id = data.data.universe;
    content.execution_assumptions_id = data.brief.content.execution_assumptions_id;
    let cost: uuid::Uuid = sqlx::query_scalar(
        "SELECT fee_schedule_artifact_id FROM app.execution_assumptions WHERE id=$1",
    )
    .bind(content.execution_assumptions_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    content.constraints.transaction_costs_ref = cost.to_string().try_into().unwrap();
    content.target_ttl_seconds = 60;
    let mandate = store
        .create_strategy_mandate(
            &actor,
            "strategy-mandate",
            &StrategyMandateCreateV1 {
                schema_version: SchemaV1,
                project_id: data.data.project,
                runtime_id: data.data.runtime,
                expected_runtime_revision: data.freeze.execution_context.runtime_revision,
                content,
            },
        )
        .await
        .unwrap()
        .resource;
    let input = store
        .create_input_set(
            &actor,
            "strategy-historical-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: data.data.project,
                purpose: InputPurpose::Validation,
                decision_cutoff: chrono::DateTime::from_timestamp_nanos(
                    source.request.selection.decision_cutoff_ns.get() as i64,
                ),
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: data.data.validation,
                    role: DataPartition::Validation,
                }],
            },
        )
        .await
        .unwrap()
        .resource;
    let mut limits = experiment_support::limits();
    limits.experiments = 0;
    let build = StrategyPortfolioBuildV1 {
        schema_version: SchemaV1,
        source_kind: StrategyPortfolioSourceV1::StrategyAlpha,
        cycle_id: experiment.cycle_id,
        mandate_id: mandate.id,
        input_set_id: input.header.id,
        runtime_id: data.data.runtime,
        expected_runtime_revision: data.freeze.execution_context.runtime_revision,
        members: vec![StrategyMemberSelectionV1 {
            alpha_version_id: alpha.id,
            ensemble_weight: "1".parse().unwrap(),
        }],
        purpose: StrategyPortfolioPurposeV1::HistoricalReplay {},
        limits,
    };
    Fixture {
        store,
        actor,
        data,
        build,
        source,
    }
}
async fn start(
    f: &Fixture,
    key: &str,
) -> contracts::control::CommandResult<contracts::runs::RunSnapshotV1> {
    f.store
        .start_strategy_portfolio_build(
            &f.actor,
            key,
            &f.build,
            |id, size| f.data.read(id, size),
            |object| async move {
                f.data
                    .objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap()
}
async fn message(pool: &PgPool, id: Id) -> RunMessage {
    RunMessage {
        message_id: sqlx::query_scalar(
            "SELECT initial_queue_message_id FROM app.run_admissions WHERE run_id=$1",
        )
        .bind(id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap(),
        run_id: id,
        read_count: 1,
    }
}
async fn accept(pool: &PgPool, f: &Fixture, run: Id) -> Id {
    accept_with(pool, f, run, |_| {}).await
}
async fn accept_with(
    pool: &PgPool,
    f: &Fixture,
    run: Id,
    alter: impl FnOnce(&mut NativeStrategyCompositionResultV1),
) -> Id {
    let message = message(pool, run).await;
    let Some(ClaimResult::Leased(lease)) = f
        .store
        .claim_native_run(&message, "strategy-fixture", 60)
        .await
        .unwrap()
    else {
        panic!("native lease")
    };
    let job = f.store.native_job(run, &lease.fence).await.unwrap();
    assert!(f.store.begin_run_dispatch(run, &lease.fence).await.unwrap());
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|i| match i {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &f.data
            .read(job.spec.parameters_artifact_id, size)
            .await
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::ComposeStrategyTargets { request, .. } = task else {
        panic!("strategy task")
    };
    let fold = &f.source.folds[0];
    let mut simulation_request = fold.simulation_request.clone();
    simulation_request.settings = request.settings.clone();
    simulation_request.target_points = fold
        .simulation_request
        .target_points
        .iter()
        .map(|point| {
            domain::execution::strategy::blend(
                &request.members,
                &[point],
                request.mandate.target_ttl_seconds,
            )
            .unwrap()
        })
        .collect();
    let outcome = match &request.purpose {
        StrategyPortfolioPurposeV1::HistoricalReplay {} => {
            StrategyCompositionOutcomeV1::HistoricalReplay {
                simulation_request: Box::new(simulation_request),
                simulation: Box::new(fold.simulation.clone()),
            }
        }
        StrategyPortfolioPurposeV1::CurrentDecision { account_start, .. } => {
            StrategyCompositionOutcomeV1::CurrentDecision {
                account_start: Box::new(account_start.clone()),
                target: NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    asof_ns: request.selection.decision_cutoff_ns,
                    valid_until_ns: DbCounter::new(
                        request.selection.decision_cutoff_ns.get()
                            + u64::from(request.mandate.target_ttl_seconds) * 1_000_000_000,
                    )
                    .unwrap(),
                    targets: vec![contracts::portfolio::AllocationTargetV1 {
                        instrument_id: request.members[0].policy.instrument_id.clone(),
                        currency: request.mandate.base_currency.clone(),
                        weight: "0.25".parse().unwrap(),
                    }],
                    cash_weight: "0.75".parse().unwrap(),
                },
                predictions_per_member: std::collections::BTreeMap::from([(
                    request.members[0].alpha_version_id,
                    DbCounter::new(fold.decisions.len() as u64 + 1).unwrap(),
                )]),
            }
        }
    };
    let current = matches!(
        request.purpose,
        StrategyPortfolioPurposeV1::CurrentDecision { .. }
    );
    let mut report = NativeStrategyCompositionResultV1 {
        schema_version: SchemaV1,
        request: *request,
        native_versions: std::collections::BTreeMap::from([
            ("nautilus-backtest".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
            ("strategy-composition".into(), "1".into()),
        ]),
        consumed_fuel: DbCounter::new(if current { 1000 } else { 0 }).unwrap(),
        outcome,
    };
    alter(&mut report);
    domain::execution::strategy_composition_result(&report.request, &report).unwrap();
    let bytes = serde_json::to_vec(&report).unwrap();
    let output = RuntimeOutputV1 {
        kind: RuntimeOutputKind::Report,
        schema: job.spec.requested_output_schemas[0].clone(),
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: DbCounter::new(bytes.len() as u64).unwrap(),
        media_type: "application/json".into(),
    };
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut engines = external_support::capabilities().engine_versions;
    engines.insert("strategy-composition".into(), "1".into());
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id.clone(),
        input_set_id: job.run.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: engines,
        started_at: Some(job.submitted_not_before),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::new(
                (now - job.submitted_not_before).num_milliseconds().max(0) as u64,
            )
            .unwrap(),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: output.byte_count,
        },
        artifacts: vec![output.clone()],
        error: None,
    };
    let writing = f.data.objects.clone();
    f.store
        .publish_native_result(
            run,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            store::lifecycle::native::NativePayloads::Verified(vec![(output, bytes)]),
            |id, size| f.data.read(id, size),
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
    lease.fence.attempt_id
}
async fn publish(f: &Fixture, run: Id) -> contracts::control::CommandResult<Id> {
    f.store
        .publish_scientific_result(
            run,
            |id, size| f.data.read(id, size),
            |object| async move {
                f.data
                    .objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap()
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn public_upload_alpha_build_native_acceptance_historical_publication_readback_and_retry(
    pool: PgPool,
) {
    let f = setup(&pool).await;
    let use_scope: String =
        sqlx::query_scalar("SELECT allowed_uses FROM app.data_use_grants WHERE id=$1")
            .bind(f.data.data.grant.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(use_scope, "RESEARCH");
    assert_eq!(
        f.store
            .input_set(&f.actor, f.build.input_set_id)
            .await
            .unwrap()
            .header
            .purpose,
        InputPurpose::Validation,
        "historical research does not require Paper preparation authority"
    );
    let first = start(&f, "strategy-build").await;
    let replay = start(&f, "strategy-build").await;
    assert!(replay.replayed);
    assert_eq!(first.resource.id, replay.resource.id);
    assert_eq!(
        first.resource.kind,
        contracts::runs::RunKind::PortfolioBuild
    );
    let msg = message(&pool, first.resource.id).await;
    let attempt = accept(&pool, &f, first.resource.id).await;
    assert!(matches!(
        f.store.acknowledge_run(&msg).await,
        Err(StoreError::Conflict)
    ));
    let published = publish(&f, first.resource.id).await;
    let replay = publish(&f, first.resource.id).await;
    assert!(replay.replayed);
    assert_eq!(published.resource, replay.resource);
    let PortfolioCandidateEnvelopeV2::Strategy(candidate) = f
        .store
        .candidate_envelope(&f.actor, published.resource)
        .await
        .unwrap()
    else {
        panic!("strategy projection")
    };
    let summary = f
        .store
        .strategy_portfolio_summary(&f.actor, candidate.id, |id, size| f.data.read(id, size))
        .await
        .unwrap();
    assert_eq!(summary.candidate_id, candidate.id);
    assert_eq!(summary.accepted_attempt_id, attempt);
    assert_eq!(summary.report_artifact_id, candidate.report_artifact_id);
    let StrategyPortfolioSummaryOutcomeV1::HistoricalReplay { simulation } = summary.outcome else {
        panic!("historical summary");
    };
    assert!(!simulation.equity_preview.points.is_empty());
    assert!(simulation.equity_preview.points.len() <= 64);
    assert_eq!(
        simulation.base_currency,
        f.source.request.settings.base_currency
    );
    assert_eq!(candidate.accepted_attempt_id, attempt);
    assert_eq!(candidate.members.len(), 1);
    assert_eq!(candidate.members[0].ensemble_weight, "1".parse().unwrap());
    assert!(candidate.decision_asof < chrono::Utc::now());
    assert!(matches!(
        candidate.purpose,
        StrategyPortfolioPurposeV1::HistoricalReplay {}
    ));
    let page = f
        .store
        .candidates_envelope(
            &f.actor,
            f.data.data.project,
            &ListQuery {
                cursor: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(f
        .store
        .candidates(
            &f.actor,
            f.data.data.project,
            &ListQuery {
                cursor: None,
                limit: 100
            }
        )
        .await
        .unwrap()
        .items
        .is_empty());
    let wire = serde_json::to_value(&candidate).unwrap();
    assert!(wire.get("solver_status").is_none());
    assert!(wire.get("qualification_refs").is_none());
    let release_valid: bool =
        sqlx::query_scalar("SELECT app.strategy_candidate_producer_valid($1,$2,$3,$4)")
            .bind(candidate.id.as_uuid())
            .bind(candidate.run_id.as_uuid())
            .bind(candidate.accepted_attempt_id.as_uuid())
            .bind(candidate.report_artifact_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !release_valid,
        "historical report can never be a current delivery source"
    );
    f.store.acknowledge_run(&msg).await.unwrap();
    f.store.acknowledge_run(&msg).await.unwrap();
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_candidates),(SELECT count(*) FROM app.qualifications),(SELECT count(*) FROM app.run_missions)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_strategy_run_acknowledges_without_fabricating_target_candidate(pool: PgPool) {
    let f = setup(&pool).await;
    let run = start(&f, "strategy-cancel").await.resource;
    f.store
        .cancel_run(
            &f.actor,
            "cancel-strategy",
            run.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    assert!(f
        .store
        .publish_scientific_result(
            run.id,
            |_, _| async { Err(StoreError::Integrity) },
            |_| async { Err(StoreError::Integrity) }
        )
        .await
        .unwrap()
        .is_none());
    let msg = message(&pool, run.id).await;
    f.store.acknowledge_run(&msg).await.unwrap();
    f.store.acknowledge_run(&msg).await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.portfolio_candidates WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

async fn current_inputs(pool: &PgPool, f: &mut Fixture, age_seconds: i64) {
    current_inputs_with_source(pool, f, age_seconds, false).await
}

async fn current_inputs_with_source(
    pool: &PgPool,
    f: &mut Fixture,
    age_seconds: i64,
    recorded: bool,
) {
    use contracts::{
        artifacts::ResearchArtifactKind,
        catalogs::RuntimeCatalogMetadataV1,
        data::{DataGrantCreate, DatasetRegister},
        research::DataUse,
    };
    let (metadata_id,size):(uuid::Uuid,i64)=sqlx::query_as("SELECT e.native_metadata_artifact_id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1").bind(f.data.data.validation.as_uuid()).fetch_one(pool).await.unwrap();
    let mut metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(
        &f.data
            .read(
                metadata_id.to_string().try_into().unwrap(),
                DbCounter::new(size as u64).unwrap(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let available = now - chrono::Duration::seconds(age_seconds);
    let available_ns = available.timestamp_nanos_opt().unwrap() as u64;
    let feature_content = serde_json::to_string(&FeatureObservationsV1 {
        schema_version: SchemaV1,
        partition: DataPartition::Forward,
        feature_schema: f.source.request.feature_schema.clone(),
        observations: vec![FeatureObservationV1 {
            feature_index: 0,
            event_ns: DbCounter::new(available_ns - 1000).unwrap(),
            observed_available_ns: Some(DbCounter::new(available_ns).unwrap()),
            sequence: DbCounter::new(100_000).unwrap(),
            value: Some(0.25),
            missing_reason: None,
        }],
    })
    .unwrap();
    metadata.recorded_feature_inputs = if recorded {
        Some(recorded_fixture::descriptor(&[(
            "current-feature".into(),
            feature_content.clone(),
        )]))
    } else {
        None
    };
    metadata.native_snapshot_ref = format!("controlled-current/{}", Id::new());
    metadata.storage_version = Id::new().to_string();
    metadata.partition = DataPartition::Forward;
    metadata.event_end = available;
    metadata.available_through = available;
    metadata.quality.checked_at = now;
    let quality = &mut metadata.quality.datasets[0];
    quality.last_event_ns = DbCounter::new(available_ns - 1000).unwrap();
    quality.available_through_ns = DbCounter::new(available_ns).unwrap();
    quality.selection.event_end_ns = DbCounter::new(available_ns).unwrap();
    quality.selection.decision_cutoff_ns = DbCounter::new(available_ns).unwrap();
    let proof: uuid::Uuid =
        sqlx::query_scalar("SELECT evidence_artifact_id FROM app.data_use_grants WHERE id=$1")
            .bind(f.data.data.grant.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let grant = f
        .store
        .create_data_grant(
            &f.actor,
            "current-data-grant",
            &DataGrantCreate {
                schema_version: SchemaV1,
                source_id: f.data.data.source,
                license_reference: "Controlled extension for Paper transaction tests".into(),
                evidence_artifact_id: proof.to_string().try_into().unwrap(),
                allowed_uses: DataUse::ResearchAndPaper,
                valid_from: now - chrono::Duration::hours(1),
                valid_until: Some(now + chrono::Duration::hours(1)),
            },
        )
        .await
        .unwrap()
        .resource;
    let source_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM app.data_sources WHERE id=$1")
            .bind(f.data.data.source.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let store::data_registration::RegistrationPreparation::Execute(ticket) = f
        .store
        .prepare_dataset_registration(
            &f.actor,
            "current-catalog",
            &DatasetRegister {
                schema_version: SchemaV1,
                source_id: f.data.data.source,
                grant_id: grant.id,
                expected_source_revision: source_revision.to_string().try_into().unwrap(),
                expected_runtime_revision: f.build.expected_runtime_revision,
                native_storage_version: metadata.storage_version.clone(),
                existing_universe_version_id: Some(f.data.data.universe),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new current catalog")
    };
    let objects = f.data.objects.clone();
    let dataset = f
        .store
        .complete_dataset_registration(
            *ticket,
            serde_json::to_vec(&metadata).unwrap(),
            |id, size| f.data.read(id, size),
            move |batch| async move {
                for object in batch {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)?;
                }
                Ok(())
            },
        )
        .await
        .unwrap()
        .resource;
    let input = f
        .store
        .create_input_set(
            &f.actor,
            "current-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.data.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: available,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset.id,
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap()
        .resource;
    let new = if recorded {
        let objects = f.data.objects.clone();
        f.store
            .register_recorded_feature(
                &f.actor,
                "current-recorded-feature",
                &contracts::data::RecordedFeatureRegisterV1 {
                    schema_version: SchemaV1,
                    project_id: f.data.data.project,
                    dataset_revision_id: dataset.id,
                    feature_part_key: "current-feature".into(),
                    content: feature_content.clone(),
                },
                |id, size| f.data.read(id, size),
                move |object| async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                },
            )
            .await
            .unwrap()
            .resource
            .artifact_id
    } else {
        external_support::upload(
            &f.store,
            &f.actor,
            &f.data,
            ResearchArtifactKind::Parameters,
            feature_content,
        )
        .await
    };
    let MandateViewEnvelopeV2::Strategy(old) = f
        .store
        .mandate_envelope(&f.actor, f.build.mandate_id)
        .await
        .unwrap()
    else {
        panic!("strategy mandate")
    };
    let mut content = old.content;
    content.target_ttl_seconds = 10;
    let mandate = f
        .store
        .create_strategy_mandate(
            &f.actor,
            "current-mandate",
            &StrategyMandateCreateV1 {
                schema_version: SchemaV1,
                project_id: f.data.data.project,
                runtime_id: f.data.data.runtime,
                expected_runtime_revision: f.build.expected_runtime_revision,
                content,
            },
        )
        .await
        .unwrap()
        .resource;
    let downstream = f
        .store
        .create_downstream(
            &f.actor,
            "current-downstream",
            &contracts::settings::DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: contracts::settings::DownstreamConfigurationV1 {
                    name: "Controlled fresh Paper".into(),
                    endpoint: "https://paper.example".into(),
                    accepted_package_versions: vec![contracts::settings::PackageSchemaVersion::V2],
                    environments: contracts::settings::DownstreamEnvironments::Paper,
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource
        .id;
    f.build.input_set_id = input.header.id;
    f.build.mandate_id = mandate.id;
    f.build.purpose = StrategyPortfolioPurposeV1::CurrentDecision {
        account_start: FreshPaperCashV1 {
            downstream_id: downstream,
            trader_id: "TRADER-001".into(),
            account_id: "SIM-001".into(),
            base_currency: mandate.content.base_currency.clone(),
            starting_capital: mandate.content.capital_assumption.clone(),
            execution_assumptions_id: mandate.content.execution_assumptions_id,
        },
        member_inputs: vec![StrategyCurrentInputsV1 {
            alpha_version_id: f.build.members[0].alpha_version_id,
            feature_artifact_ids: vec![new],
        }],
    };
}

#[sqlx::test(migrations = "../../migrations")]
async fn current_native_result_keeps_provenance_and_expired_facts_but_cannot_release(pool: PgPool) {
    let mut f = setup(&pool).await;
    current_inputs(&pool, &mut f, 15).await;
    let run = start(&f, "expired-current-build").await.resource.id;
    accept(&pool, &f, run).await;
    let candidate = publish(&f, run).await.resource;
    let PortfolioCandidateEnvelopeV2::Strategy(view) = f
        .store
        .candidate_envelope(&f.actor, candidate)
        .await
        .unwrap()
    else {
        panic!("current strategy")
    };
    assert!(matches!(
        view.purpose,
        StrategyPortfolioPurposeV1::CurrentDecision { .. }
    ));
    assert_eq!(
        view.input_provenance.market_data_origin,
        contracts::research::DataOrigin::Real
    );
    assert!(view
        .input_provenance
        .feature_artifact_origins
        .values()
        .all(|v| *v == contracts::research::DataOrigin::Synthetic));
    let summary = f
        .store
        .strategy_portfolio_summary(&f.actor, candidate, |id, size| f.data.read(id, size))
        .await
        .unwrap();
    assert!(matches!(summary.outcome,
        StrategyPortfolioSummaryOutcomeV1::CurrentDecision { reason_code }
        if reason_code == "CURRENT_DECISION_TARGET_ONLY_NO_HISTORICAL_SIMULATION"));
    let expiry: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT min(valid_until) FROM app.candidate_targets WHERE candidate_id=$1",
    )
    .bind(candidate.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(expiry < chrono::Utc::now());
    let error = f
        .store
        .create_strategy_release(
            &f.actor,
            "expired-release",
            &StrategyReleaseCreateV1 {
                schema_version: SchemaV1,
                source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
                candidate_id: candidate,
            },
            |id, size| f.data.read(id, size),
            |_| async { panic!("expired target cannot publish a package") },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, StoreError::Invalid("strategy_current_expired")),
        "{error:?}"
    );
    let msg = message(&pool, run).await;
    f.store.acknowledge_run(&msg).await.unwrap();
    f.store.acknowledge_run(&msg).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn current_publication_rejects_prefix_only_prediction_claim(pool: PgPool) {
    let mut f = setup(&pool).await;
    current_inputs(&pool, &mut f, 1).await;
    let run = start(&f, "bad-current-build").await.resource.id;
    accept_with(&pool, &f, run, |report| {
        let StrategyCompositionOutcomeV1::CurrentDecision {
            predictions_per_member,
            ..
        } = &mut report.outcome
        else {
            unreachable!()
        };
        *predictions_per_member.values_mut().next().unwrap() = DbCounter::new(1).unwrap();
    })
    .await;
    assert!(matches!(
        f.store
            .publish_scientific_result(
                run,
                |id, size| f.data.read(id, size),
                |_| async { panic!("invalid continuation cannot publish targets") }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    assert!(matches!(
        f.store.acknowledge_run(&message(&pool, run).await).await,
        Err(StoreError::Conflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn current_publication_is_native_producer_bound_and_ack_replays(pool: PgPool) {
    let mut f = setup(&pool).await;
    current_inputs(&pool, &mut f, 1).await;
    let run = start(&f, "current-build").await.resource.id;
    let attempt = accept(&pool, &f, run).await;
    let candidate = publish(&f, run).await.resource;
    let PortfolioCandidateEnvelopeV2::Strategy(view) = f
        .store
        .candidate_envelope(&f.actor, candidate)
        .await
        .unwrap()
    else {
        panic!("strategy branch")
    };
    assert_eq!(view.run_id, run);
    assert_eq!(view.accepted_attempt_id, attempt);
    assert!(matches!(
        view.purpose,
        StrategyPortfolioPurposeV1::CurrentDecision { .. }
    ));
    let valid: bool =
        sqlx::query_scalar("SELECT app.strategy_candidate_producer_valid($1,$2,$3,$4)")
            .bind(candidate.as_uuid())
            .bind(run.as_uuid())
            .bind(attempt.as_uuid())
            .bind(view.report_artifact_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(valid);
    let substituted: bool =
        sqlx::query_scalar("SELECT app.strategy_candidate_producer_valid($1,$2,$3,$4)")
            .bind(candidate.as_uuid())
            .bind(run.as_uuid())
            .bind(Id::new().as_uuid())
            .bind(view.report_artifact_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!substituted);
    let msg = message(&pool, run).await;
    f.store.acknowledge_run(&msg).await.unwrap();
    f.store.acknowledge_run(&msg).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_native_strategy_run_acknowledges_without_fabricating_candidate(pool: PgPool) {
    let f = setup(&pool).await;
    let run = start(&f, "strategy-failed").await.resource.id;
    let msg = message(&pool, run).await;
    let Some(ClaimResult::Leased(lease)) = f
        .store
        .claim_native_run(&msg, "strategy-failed-fixture", 60)
        .await
        .unwrap()
    else {
        panic!("native lease")
    };
    let job = f.store.native_job(run, &lease.fence).await.unwrap();
    assert!(f.store.begin_run_dispatch(run, &lease.fence).await.unwrap());
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut engines = external_support::capabilities().engine_versions;
    engines.insert("strategy-composition".into(), "1".into());
    let manifest = ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: job.spec.attempt_no,
        external_job_id: job.spec.external_job_id,
        input_set_id: job.run.input_set_id,
        state: RuntimeResultState::Failed,
        engine_versions: engines,
        started_at: Some(job.submitted_not_before),
        finished_at: now,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: DbCounter::new(
                (now - job.submitted_not_before).num_milliseconds().max(0) as u64,
            )
            .unwrap(),
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: DbCounter::ZERO,
        },
        artifacts: vec![],
        error: Some(domain::runtime_jobs::error(
            RuntimeFailureCode::NativeJobFailed,
        )),
    };
    let objects = f.data.objects.clone();
    f.store
        .publish_native_result(
            run,
            &lease.fence,
            serde_json::to_vec(&manifest).unwrap(),
            store::lifecycle::native::NativePayloads::Verified(vec![]),
            |id, size| f.data.read(id, size),
            move |batch| async move {
                for object in batch {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)?;
                }
                Ok(())
            },
        )
        .await
        .unwrap();
    assert!(f
        .store
        .publish_scientific_result(
            run,
            |_, _| async { Err(StoreError::Integrity) },
            |_| async { Err(StoreError::Integrity) }
        )
        .await
        .unwrap()
        .is_none());
    f.store.acknowledge_run(&msg).await.unwrap();
    f.store.acknowledge_run(&msg).await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.portfolio_candidates WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[path = "support/strategy_delivery_checks.rs"]
mod strategy_delivery_checks;

async fn summary_reader(
    f: &Fixture,
    project: Id,
    scopes: Vec<contracts::control::MachineScope>,
) -> Actor {
    use contracts::control::*;
    let principal = f
        .store
        .create_principal(
            &f.actor,
            &Id::new().to_string(),
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Summary reader".into(),
                kind: AssignablePrincipalKind::Cli,
                project_id: Some(project),
                downstream_id: None,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let store::control::CredentialPreparation::New(ticket) = f
        .store
        .prepare_credential_issuance(
            &f.actor,
            &Id::new().to_string(),
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: scopes,
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new credential");
    };
    let verifier = Id::new();
    let credential = ticket.publish(Id::new(), verifier).await.unwrap().resource;
    Actor::Machine {
        credential_id: credential.id,
        verifier_ref: verifier,
        operator_grant: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn summary_stays_project_scoped_and_rejects_missing_or_corrupt_report(pool: PgPool) {
    use contracts::control::{MachineScope, ProjectCreate};
    let f = setup(&pool).await;
    let run = start(&f, "summary-build").await.resource.id;
    // Reaching this callback would turn the expected authorization/missing
    // result into Integrity and fail the assertions below.
    let never_read = |_, _| std::future::ready(Err::<Vec<u8>, _>(StoreError::Integrity));
    assert!(matches!(
        f.store
            .strategy_portfolio_summary(&f.actor, run, never_read)
            .await,
        Err(StoreError::NotFound)
    ));
    accept(&pool, &f, run).await;
    let candidate = publish(&f, run).await.resource;
    let PortfolioCandidateEnvelopeV2::Strategy(view) = f
        .store
        .candidate_envelope(&f.actor, candidate)
        .await
        .unwrap()
    else {
        panic!("strategy candidate");
    };
    let reader = summary_reader(&f, f.data.data.project, vec![MachineScope::ResearchRead]).await;
    let summary = f
        .store
        .strategy_portfolio_summary(&reader, candidate, |id, size| f.data.read(id, size))
        .await
        .unwrap();
    assert_eq!(summary.run_id, run);
    assert!(matches!(
        f.store
            .artifact_content(&reader, view.report_artifact_id)
            .await,
        Err(StoreError::NotFound)
    ));
    let denied = summary_reader(&f, f.data.data.project, vec![MachineScope::RunRead]).await;
    assert!(matches!(
        f.store
            .strategy_portfolio_summary(&denied, candidate, never_read)
            .await,
        Err(StoreError::Forbidden)
    ));
    let other = f
        .store
        .create_project(
            &f.actor,
            "summary-other",
            &ProjectCreate {
                schema_version: SchemaV1,
                name: "Other project".into(),
                description: String::new(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let foreign = summary_reader(&f, other, vec![MachineScope::ResearchRead]).await;
    assert!(matches!(
        f.store
            .strategy_portfolio_summary(&foreign, candidate, never_read)
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        f.store
            .strategy_portfolio_summary(&reader, candidate, |_, _| async { Ok(Vec::new()) })
            .await,
        Err(StoreError::Integrity)
    ));
    let data = &f.data;
    assert!(matches!(
        f.store
            .strategy_portfolio_summary(&reader, candidate, |id, size| async move {
                let mut bytes = data.read(id, size).await?;
                bytes[0] = b'!';
                Ok(bytes)
            })
            .await,
        Err(StoreError::Integrity)
    ));
    // No read failure changes the accepted source or creates another computation.
    assert_eq!(
        f.store
            .strategy_portfolio_summary(&reader, candidate, |id, size| f.data.read(id, size))
            .await
            .unwrap()
            .report_artifact_id,
        view.report_artifact_id
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn registered_policy_and_current_features_keep_separate_datasets_in_actual_provenance(
    pool: PgPool,
) {
    let mut f = setup_with_sources(&pool, true, true).await;
    current_inputs_with_source(&pool, &mut f, 1, true).await;
    let run = start(&f, "recorded-current-build").await.resource.id;
    let (parameter, size): (uuid::Uuid, i64) = sqlx::query_as("SELECT a.id,a.byte_count FROM app.run_native_tasks t JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE t.run_id=$1")
        .bind(run.as_uuid()).fetch_one(&pool).await.unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &f.data
            .read(
                parameter.to_string().try_into().unwrap(),
                DbCounter::new(size as u64).unwrap(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::ComposeStrategyTargets {
        request,
        dataset_revision_id,
        ..
    } = task
    else {
        panic!("composition task required");
    };
    let sources = request
        .input_provenance
        .feature_source_bindings
        .as_ref()
        .unwrap();
    let policy = &request.members[0].policy;
    assert_eq!(sources.len(), 3);
    assert_eq!(policy.dataset_revision_id, f.data.data.validation);
    assert_ne!(dataset_revision_id, policy.dataset_revision_id);
    for artifact in &policy.feature_artifact_ids {
        assert_eq!(
            sources[artifact].dataset_revision_id,
            policy.dataset_revision_id
        );
        assert_eq!(sources[artifact].data_use_grant_id, f.data.data.grant);
    }
    let StrategyPortfolioPurposeV1::CurrentDecision { member_inputs, .. } = &f.build.purpose else {
        unreachable!()
    };
    let current = member_inputs[0].feature_artifact_ids[0];
    assert_eq!(sources[&current].dataset_revision_id, dataset_revision_id);
    assert_ne!(sources[&current].data_use_grant_id, f.data.data.grant);
    assert_eq!(
        sources[&current].origin,
        request.input_provenance.market_data_origin
    );
    domain::data::recorded_feature_provenance(&request.input_provenance).unwrap();
    // Both kinds of registered inputs survive the actual native spec and transfer gate.
    let Some(ClaimResult::Leased(lease)) = f
        .store
        .claim_native_run(&message(&pool, run).await, "recorded-current-worker", 60)
        .await
        .unwrap()
    else {
        panic!("native lease required");
    };
    let job = f.store.native_job(run, &lease.fence).await.unwrap();
    for artifact in sources.keys() {
        assert!(job.spec.inputs.iter().any(
            |i| matches!(i, RuntimeInputV1::Artifact { artifact_id, .. } if artifact_id == artifact)
        ));
        f.store
            .native_input(run, &lease.fence, *artifact)
            .await
            .unwrap();
    }
    // The current Dataset grant is active, while the historical feature grant
    // is withdrawn after spec freezing. Every new transfer/send must stop.
    f.store
        .revoke_data_grant(
            &f.actor,
            "withdraw-old-paired-features",
            f.data.data.grant,
            &contracts::data::DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: None,
                reason_code: "CONTROLLED_WITHDRAWAL".into(),
                reason: "Historical feature initialization permission withdrawn".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        f.store
            .get_dataset_revision(&f.actor, dataset_revision_id)
            .await
            .unwrap()
            .license_state,
        contracts::data::DataLicenseState::Active
    );
    assert!(f
        .store
        .native_input(run, &lease.fence, current)
        .await
        .is_err());
    assert!(f
        .store
        .native_input(run, &lease.fence, policy.feature_artifact_ids[0])
        .await
        .is_err());
    assert!(f.store.native_job(run, &lease.fence).await.is_err());
    assert!(f.store.begin_run_dispatch(run, &lease.fence).await.is_err());
    let dispatch: String =
        sqlx::query_scalar("SELECT dispatch_state FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dispatch, "NOT_SENT");
}

#[sqlx::test(migrations = "../../migrations")]
async fn research_only_historical_feature_grant_cannot_be_reused_for_current_decision(
    pool: PgPool,
) {
    let mut f = setup_with_sources(&pool, false, true).await;
    current_inputs_with_source(&pool, &mut f, 1, true).await;
    let result = f
        .store
        .start_strategy_portfolio_build(
            &f.actor,
            "recorded-current-without-paper",
            &f.build,
            |id, size| f.data.read(id, size),
            |_| async { panic!("wrong-purpose features cannot publish a task") },
        )
        .await;
    let Err(StoreError::Domain(domain::DomainError::Fields(fields))) = result else {
        panic!("the historical feature grant must authorize the current use");
    };
    assert!(fields
        .iter()
        .any(|f| f.code == "DATA_USE_PURPOSE_NOT_AUTHORIZED"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_feature_authority_is_rechecked_after_last_read_before_current_build_commit(
    pool: PgPool,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut f = setup_with_sources(&pool, true, true).await;
    current_inputs_with_source(&pool, &mut f, 1, true).await;
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let effective = now + chrono::Duration::seconds(5);
    f.store
        .revoke_data_grant(
            &f.actor,
            "future-old-feature-revocation",
            f.data.data.grant,
            &contracts::data::DataGrantRevoke {
                schema_version: SchemaV1,
                effective_at: Some(effective),
                reason_code: "CONTROLLED_WITHDRAWAL".into(),
                reason: "Original feature authorization ends during the final bounded object read"
                    .into(),
            },
        )
        .await
        .unwrap();
    let published = AtomicBool::new(false);
    let waited = AtomicBool::new(false);
    let last_feature = *f.source.feature_artifact_ids.last().unwrap();
    let fixture = &f;
    let clock = &pool;
    let published_ref = &published;
    let waited_ref = &waited;
    let result = f
        .store
        .start_strategy_portfolio_build(
            &f.actor,
            "feature-expires-during-final-read",
            &f.build,
            |id, size| async move {
                if id == last_feature && published_ref.load(Ordering::SeqCst) {
                    tokio::time::timeout(std::time::Duration::from_secs(8), async {
                        loop {
                            let now: chrono::DateTime<chrono::Utc> =
                                sqlx::query_scalar("SELECT clock_timestamp()")
                                    .fetch_one(clock)
                                    .await
                                    .unwrap();
                            if now >= effective {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .expect("future revocation did not become effective");
                    waited_ref.store(true, Ordering::SeqCst);
                }
                fixture.data.read(id, size).await
            },
            |object| {
                published.store(true, Ordering::SeqCst);
                let objects = f.data.objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await;
    assert!(published.load(Ordering::SeqCst));
    assert!(
        waited.load(Ordering::SeqCst),
        "authority must cross its boundary during the second object read pass"
    );
    let Err(StoreError::Domain(domain::DomainError::Fields(fields))) = result else {
        panic!("expired original feature permission cannot commit admission");
    };
    assert!(fields
        .iter()
        .any(|field| field.code == "DATA_USE_NOT_AUTHORIZED"));
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs WHERE project_id=$1 AND kind='PORTFOLIO_BUILD'),(SELECT count(*) FROM app.command_receipts WHERE operation='PORTFOLIO_BUILD' AND idempotency_key='feature-expires-during-final-read')")
        .bind(f.data.data.project.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
}
