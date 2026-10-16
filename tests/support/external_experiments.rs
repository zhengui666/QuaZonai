//! Controlled transaction inputs, never a claim of native model or engine execution.
#![allow(dead_code)]
use super::{cycle_support, experiment_support, research_support, runtime_support};
use contracts::{
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    control::CommandResult,
    cycles::*,
    experiments::*,
    runs::RunSnapshotV1,
    science::*,
    DbCounter, Id, SchemaV1,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::{authority::Actor, Store, StoreError};

pub async fn unused_native_profiles(pool: &PgPool) -> Vec<Value> {
    // Local setup seeds these two role settings. Their presence is not a model
    // account, a selected cycle profile, or a native Agent invocation.
    let profiles: Vec<Value> =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM app.codex_profiles p ORDER BY local_role")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| p["local_role"].clone())
            .collect::<Vec<_>>(),
        vec![json!("RESEARCHER"), json!("REVIEWER")],
        "only the two migration-created local role settings may exist"
    );
    let activity: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.codex_profile_observations),\
        (SELECT count(*) FROM app.codex_account_operations),\
        (SELECT count(*) FROM app.codex_sessions),\
        (SELECT count(*) FROM app.run_missions)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        activity,
        (0, 0, 0, 0),
        "direct science must not probe/login/start a native Agent"
    );
    profiles
}

pub fn capabilities() -> contracts::runtime::RuntimeCapabilitiesV1 {
    let mut capabilities = runtime_support::portfolio_capabilities(chrono::Utc::now());
    for name in [
        "qz.wasm_model",
        "qz.model_compilation",
        "qz.experiment_evaluation",
    ] {
        if !capabilities
            .artifact_schemas
            .iter()
            .any(|schema| schema.name == name)
        {
            capabilities
                .artifact_schemas
                .push(contracts::runtime::RuntimeArtifactSchemaV1 {
                    name: name.into(),
                    version: "1".into(),
                });
        }
    }
    capabilities
}

pub async fn upload(
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    kind: ResearchArtifactKind,
    content: String,
) -> Id {
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

pub async fn setup(
    pool: &PgPool,
) -> (
    Store,
    Actor,
    cycle_support::Fixture,
    ExperimentView,
    ExperimentEvaluateIntent,
) {
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let (store, actor, mut f, experiment, request) = setup_with_objects(pool, objects).await;
    f.directory = Some(directory);
    (store, actor, f, experiment, request)
}

pub async fn setup_with_objects(
    pool: &PgPool,
    objects: std::sync::Arc<integrations::artifacts::ArtifactStore>,
) -> (
    Store,
    Actor,
    cycle_support::Fixture,
    ExperimentView,
    ExperimentEvaluateIntent,
) {
    setup_with_objects_and_grant(pool, objects, contracts::research::DataUse::Research, false).await
}

pub async fn setup_for_paper(
    pool: &PgPool,
    permanent_grant: bool,
) -> (
    Store,
    Actor,
    cycle_support::Fixture,
    ExperimentView,
    ExperimentEvaluateIntent,
) {
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let (store, actor, mut fixture, experiment, request) = setup_with_objects_and_grant(
        pool,
        objects,
        contracts::research::DataUse::ResearchAndPaper,
        permanent_grant,
    )
    .await;
    fixture.directory = Some(directory);
    (store, actor, fixture, experiment, request)
}

async fn setup_with_objects_and_grant(
    pool: &PgPool,
    objects: std::sync::Arc<integrations::artifacts::ArtifactStore>,
    allowed_uses: contracts::research::DataUse,
    permanent_grant: bool,
) -> (
    Store,
    Actor,
    cycle_support::Fixture,
    ExperimentView,
    ExperimentEvaluateIntent,
) {
    setup_with_features(pool, objects, allowed_uses, permanent_grant, None).await
}

/// The descriptor and buffers are controlled fixtures supplied by the test.
/// Production registration still computes and checks the original buffer digest.
pub async fn setup_recorded(
    pool: &PgPool,
    allowed_uses: contracts::research::DataUse,
    descriptor: contracts::catalogs::RecordedFeatureInputsV1,
    parts: Vec<(String, String)>,
) -> (
    Store,
    Actor,
    cycle_support::Fixture,
    ExperimentView,
    ExperimentEvaluateIntent,
) {
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let (store, actor, mut fixture, experiment, request) = setup_with_features(
        pool,
        objects,
        allowed_uses,
        false,
        Some((descriptor, parts)),
    )
    .await;
    fixture.directory = Some(directory);
    (store, actor, fixture, experiment, request)
}

async fn setup_with_features(
    pool: &PgPool,
    objects: std::sync::Arc<integrations::artifacts::ArtifactStore>,
    allowed_uses: contracts::research::DataUse,
    permanent_grant: bool,
    recorded: Option<(
        contracts::catalogs::RecordedFeatureInputsV1,
        Vec<(String, String)>,
    )>,
) -> (
    Store,
    Actor,
    cycle_support::Fixture,
    ExperimentView,
    ExperimentEvaluateIntent,
) {
    let native_profiles = unused_native_profiles(pool).await;
    let (store, actor) = research_support::operator(pool).await;
    let f = if let Some((descriptor, _)) = &recorded {
        cycle_support::setup_external_with_features(
            pool,
            &store,
            &actor,
            objects,
            allowed_uses,
            permanent_grant,
            descriptor.clone(),
            |policy| {
                policy.split_policy.step_size = Some(policy.split_policy.test_size);
            },
        )
        .await
    } else {
        cycle_support::setup_external_with_grant(
            pool,
            &store,
            &actor,
            objects,
            allowed_uses,
            permanent_grant,
            |policy| {
                policy.split_policy.step_size = Some(policy.split_policy.test_size);
            },
        )
        .await
    };
    assert_eq!(
        unused_native_profiles(pool).await,
        native_profiles,
        "external setup must not configure the migration-created native profiles"
    );
    let capabilities = capabilities();
    experiment_support::probe_capabilities(&store, &actor, &f, capabilities).await;
    let cycle = start(&store, &actor, &f).await;
    let schema = vec![FeatureDefinitionV1 {
        feature_key: "signal".into(),
        source_ref: "fixture:immutable-part".into(),
        source_key: "source-market".into(),
        availability: FeatureAvailabilityV1::Observed,
        max_age_ns: Some(DbCounter::new(60_000_000_000).unwrap()),
    }];
    let mut ids = Vec::new();
    if let Some((_, parts)) = recorded {
        for (part_key, content) in parts {
            let registered = store
                .register_recorded_feature(
                    &actor,
                    &Id::new().to_string(),
                    &contracts::data::RecordedFeatureRegisterV1 {
                        schema_version: SchemaV1,
                        project_id: f.data.project,
                        dataset_revision_id: f.data.validation,
                        feature_part_key: part_key,
                        content,
                    },
                    |id, size| f.read(id, size),
                    |object| {
                        let objects = f.objects.clone();
                        async move {
                            objects
                                .put(object.id, &object.bytes)
                                .map_err(|_| StoreError::Integrity)
                        }
                    },
                )
                .await
                .unwrap()
                .resource;
            ids.push(registered.artifact_id);
        }
    } else {
        for sequence in 0..2 {
            ids.push(
                upload(
                    &store,
                    &actor,
                    &f,
                    ResearchArtifactKind::Parameters,
                    serde_json::to_string(&FeatureObservationsV1 {
                        schema_version: SchemaV1,
                        partition: contracts::research::DataPartition::Validation,
                        feature_schema: schema.clone(),
                        observations: vec![FeatureObservationV1 {
                            feature_index: 0,
                            event_ns: DbCounter::new(1_577_923_200_000_000_000 + sequence).unwrap(),
                            observed_available_ns: Some(
                                DbCounter::new(1_577_923_200_000_000_000 + sequence).unwrap(),
                            ),
                            sequence: DbCounter::new(sequence).unwrap(),
                            value: Some(0.5),
                            missing_reason: None,
                        }],
                    })
                    .unwrap(),
                )
                .await,
            );
        }
    }
    let settings: Value = sqlx::query_scalar(
        "SELECT settings FROM app.execution_assumption_sources WHERE assumptions_id=$1",
    )
    .bind(f.brief.content.execution_assumptions_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let parameters=upload(&store,&actor,&f,ResearchArtifactKind::Parameters,json!({"schema_version":1,"dataset_revision_id":f.data.validation,"feature_artifact_ids":ids,"instrument_id":"EUR/USD.SIM","feature_schema":schema,"label_horizon_observations":5,"total_fuel":"1000000","target_ttl_ns":"60000000000","decision_output":"TARGET_WEIGHT","settings":settings}).to_string()).await;
    let code = upload(
        &store,
        &actor,
        &f,
        ResearchArtifactKind::Code,
        "// controlled compiler input; execution is separately tested".into(),
    )
    .await;
    let report = upload(
        &store,
        &actor,
        &f,
        ResearchArtifactKind::Report,
        "{\"schema_version\":1,\"hypothesis\":\"controlled research\"}".into(),
    )
    .await;
    let family: uuid::Uuid =
        sqlx::query_scalar("SELECT family_id FROM app.evaluation_policies WHERE id=$1")
            .bind(f.brief.content.evaluation_policy_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let experiment = store
        .propose_experiment(
            &actor,
            "external-propose",
            &ExperimentProposalV1 {
                schema_version: SchemaV1,
                cycle_id: cycle.id,
                family_id: family.to_string().try_into().unwrap(),
                parent_experiment_id: None,
                hypothesis: "Independent rerun of a frozen target policy".into(),
                expected_failure_modes: "Execution costs may remove the edge".into(),
                proposal_artifact_id: report,
                parameter_artifact_id: parameters,
                code_artifact_id: Some(code),
            },
        )
        .await
        .unwrap()
        .resource;
    let mut evaluation_limits = experiment_support::limits();
    evaluation_limits.experiments = 0;
    let request = ExperimentEvaluateIntent {
        schema_version: SchemaV1,
        experiment_id: experiment.id,
        request: ExperimentEvaluateV1 {
            schema_version: SchemaV1,
            expected_revision: experiment.revision,
            compile_limits: experiment_support::limits(),
            evaluation_limits,
        },
    };
    (store, actor, f, experiment, request)
}

/// Freeze and start only; callers supply the actual catalog and Runtime observation.
pub async fn start(store: &Store, actor: &Actor, f: &cycle_support::Fixture) -> CycleViewV1 {
    store
        .freeze_brief(
            actor,
            "external-freeze",
            f.brief.id,
            &f.freeze,
            |id, size| f.read(id, size),
        )
        .await
        .unwrap();
    let legacy = cycle_support::start_request(store, actor, f).await;
    let request = ExternalCycleStartIntent {
        schema_version: SchemaV1,
        project_id: f.data.project,
        request: ExternalCycleStartV1 {
            schema_version: SchemaV1,
            brief_id: f.brief.id,
            expected_revision: legacy.request.expected_revision,
        },
    };
    let cycle = store
        .start_external_cycle(actor, "external-cycle", &request, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    let replay = store
        .start_external_cycle(actor, "external-cycle", &request, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(cycle.resource.id, replay.resource.id);
    assert!(cycle.resource.initial_run_id.is_none());
    assert!(cycle.resource.researcher_profile.is_none());
    assert!(cycle.resource.reviewer_profile.is_none());
    cycle.resource
}

pub async fn evaluate(
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    request: &ExperimentEvaluateIntent,
) -> Result<CommandResult<RunSnapshotV1>, StoreError> {
    store
        .evaluate_experiment(
            actor,
            "external-evaluate",
            request,
            |id, size| f.read(id, size),
            |object| async move {
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}
pub async fn advance(store: &Store, f: &cycle_support::Fixture, run: Id) -> Result<(), StoreError> {
    store
        .advance_external_experiment(
            run,
            |id, size| f.read(id, size),
            |object| async move {
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}
