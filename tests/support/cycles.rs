//! Native relational startup fixture, NOT an authentic market-data/OCI/Codex
//! acceptance. Formal freeze/start use the real Store transaction APIs below.
#![allow(dead_code)]
use super::{research_support, runtime_support};
#[path = "cycle_data.rs"]
mod cycle_data;
use chrono::{DateTime, Utc};
use contracts::{
    brief::*,
    codex::{CodexConnectionCreateV1, CodexProfileCreateV1, ProfileOrigin, SavedModelSettingsV1},
    control::ProjectUpdate,
    cycles::*,
    research::*,
    runs::ProjectState,
    runtime::{RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    DbCounter, Id, SchemaV1,
};
use integrations::artifacts::ArtifactStore;
use sqlx::PgPool;
use std::sync::Arc;
use store::{authority::Actor, runtime::ProbePreparation, Store};

pub struct Fixture {
    pub data: research_support::ResearchFixture,
    pub brief: BriefView,
    pub freeze: BriefFreezeV1,
    pub objects: Arc<ArtifactStore>,
    pub researcher_profile: CodexProfileChoiceV1,
    pub reviewer_profile: CodexProfileChoiceV1,
    _directory: Option<tempfile::TempDir>,
}

pub async fn setup(pool: &PgPool, store: &Store, actor: &Actor) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let objects = Arc::new(ArtifactStore::open(&directory.path().join("objects")).unwrap());
    let mut fixture = setup_with_objects(pool, store, actor, objects).await;
    fixture._directory = Some(directory);
    fixture
}

pub async fn setup_with_objects(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    objects: Arc<ArtifactStore>,
) -> Fixture {
    setup_with_policy(pool, store, actor, objects, |_| {}).await
}

pub async fn setup_with_policy(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    objects: Arc<ArtifactStore>,
    customize: impl FnOnce(&mut EvaluationPolicyCreate),
) -> Fixture {
    let mut data = research_support::setup(pool, store, actor).await;
    let capabilities = runtime_support::capabilities(Utc::now());
    let image = &capabilities.image_refs[0].image_ref;
    let assumptions = Id::new();
    sqlx::query("INSERT INTO app.execution_assumptions(id,venue_capability_ref,engine_image_ref,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref) SELECT $1,venue_capability_ref,$2,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref FROM app.execution_assumptions WHERE id=$3")
        .bind(assumptions.as_uuid()).bind(image).bind(data.assumptions.as_uuid())
        .execute(pool).await.unwrap();
    data.assumptions = assumptions;
    let revision: i64 = sqlx::query_scalar("UPDATE app.runtime_integrations SET allowed_capabilities=ARRAY['DATA_VALIDATE','ALPHA_EVALUATE'] WHERE id=$1 RETURNING revision")
        .bind(data.runtime.as_uuid()).fetch_one(pool).await.unwrap();
    let revision = revision.to_string().try_into().unwrap();
    cycle_data::register(pool, store, actor, &mut data, revision, objects.clone()).await;
    let ProbePreparation::Pending(ticket) = store
        .prepare_runtime_probe(
            actor,
            &Id::new().to_string(),
            data.runtime,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("native fixture ticket expected")
    };
    let mut observed = runtime_support::capabilities(Utc::now());
    observed
        .artifact_schemas
        .push(contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.data_quality".into(),
            version: "1".into(),
        });
    let outcome = RuntimeProbeOutcomeV1::Available {
        capabilities: Box::new(observed),
    };
    // Controlled native observation, persisted through the actual immutable
    // ArtifactStore. The separate transport suite proves actual TCP/TLS behavior.
    let publishing = objects.clone();
    store
        .complete_runtime_probe(*ticket, outcome, move |id, bytes| async move {
            tokio::task::spawn_blocking(move || publishing.put(id, &bytes))
                .await
                .map_err(|_| store::StoreError::Integrity)?
                .map_err(|_| store::StoreError::Integrity)
        })
        .await
        .unwrap();
    let cutoff = DateTime::parse_from_rfc3339("2020-01-03T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut inputs = Vec::new();
    for purpose in [
        InputPurpose::Discovery,
        InputPurpose::Validation,
        InputPurpose::Sealed,
    ] {
        let mut request = data.input(purpose);
        request.decision_cutoff = cutoff;
        inputs.push(
            store
                .create_input_set(actor, &Id::new().to_string(), &request)
                .await
                .unwrap()
                .resource
                .header
                .id,
        );
    }
    let mut policy_request = data.policy(inputs[1]);
    policy_request.require_real_data = false;
    policy_request.required_capabilities.clear();
    policy_request.split_policy.label_horizon_observations = Some(DbCounter::new(5).unwrap());
    policy_request.split_policy.purge_observations = DbCounter::new(5).unwrap();
    policy_request.selection.metric_code = "PEARSON_IC".into();
    policy_request.selection.metric_scope = "asset:0/fold:0".into();
    policy_request.selection.method_id = "ndarray-stats.pearson_correlation".into();
    policy_request.selection.method_version = "0.7.0".into();
    policy_request.selection.unit = "CORRELATION".into();
    policy_request.selection.frequency = "1-MINUTE-LAST-EXTERNAL;horizon=5".into();
    policy_request.metric_requirements = vec![contracts::evidence::MetricRequirementV1 {
        schema_version: SchemaV1,
        metric_code: "PEARSON_IC".into(),
        scope: "asset:0/fold:0".into(),
        comparator: contracts::evidence::Comparator::Ge,
        threshold_low: Some("0.1".parse().unwrap()),
        threshold_high: None,
        required: true,
        minimum_observations: DbCounter::new(2).unwrap(),
        method_allowlist: vec!["ndarray-stats.pearson_correlation".into()],
    }];
    customize(&mut policy_request);
    let policy = store
        .create_evaluation_policy(actor, &Id::new().to_string(), &policy_request)
        .await
        .unwrap()
        .resource;
    let mut request: BriefCreate =
        serde_json::from_str(include_str!("../contracts/research-brief.json")).unwrap();
    request.content.universe_version_id = data.universe;
    request.content.execution_assumptions_id = data.assumptions;
    request.content.evaluation_policy_id = policy.id;
    request.bindings = vec![
        BriefBindingV1 {
            dataset_revision_id: data.discovery,
            role: DataPartition::Discovery,
            access_policy: DataAccess::ResearchRead,
        },
        BriefBindingV1 {
            dataset_revision_id: data.validation,
            role: DataPartition::Validation,
            access_policy: DataAccess::ResearchRead,
        },
        BriefBindingV1 {
            dataset_revision_id: data.sealed,
            role: DataPartition::Sealed,
            access_policy: DataAccess::EvaluatorOnly,
        },
    ];
    let brief = store
        .create_brief(
            actor,
            &Id::new().to_string(),
            &BriefCreateIntent {
                schema_version: SchemaV1,
                project_id: data.project,
                request,
            },
        )
        .await
        .unwrap()
        .resource;
    let freeze = BriefFreezeV1 {
        schema_version: SchemaV1,
        expected_revision: brief.revision,
        execution_context: BriefExecutionContextV1 {
            schema_version: SchemaV1,
            runtime_id: data.runtime,
            runtime_revision: revision,
            discovery_input_set_id: inputs[0],
            validation_input_set_id: inputs[1],
            sealed_input_set_id: inputs[2],
        },
    };
    Fixture {
        researcher_profile: profile_choice(store, actor, "researcher").await,
        reviewer_profile: profile_choice(store, actor, "reviewer").await,
        data,
        brief,
        freeze,
        objects,
        _directory: None,
    }
}

async fn profile_choice(store: &Store, actor: &Actor, role: &str) -> CodexProfileChoiceV1 {
    let profile = store
        .create_codex_profile(
            actor,
            &Id::new().to_string(),
            &CodexProfileCreateV1 {
                schema_version: SchemaV1,
                name: format!("{role} fixture"),
                home_binding: format!("native-{}", Id::new()),
                profile_origin: ProfileOrigin::ManagedVolume,
                connection: CodexConnectionCreateV1::System {},
                model_settings: SavedModelSettingsV1 {
                    schema_version: SchemaV1,
                    use_default_model_settings: true,
                    saved_model: None,
                    saved_reasoning_effort: None,
                    saved_fast_mode: false,
                },
            },
            |binding| async move {
                // Explicit binding fixture only; native dedicated-home proof is separate.
                domain::codex::settings::home_binding(&binding.home_binding)?;
                Ok(())
            },
        )
        .await
        .unwrap()
        .resource;
    CodexProfileChoiceV1 {
        profile_id: profile.id,
        expected_revision: profile.revision,
    }
}

impl Fixture {
    pub async fn read(&self, id: Id, size: DbCounter) -> Result<Vec<u8>, store::StoreError> {
        let objects = self.objects.clone();
        tokio::task::spawn_blocking(move || objects.read(id, size))
            .await
            .map_err(|_| store::StoreError::Integrity)?
            .map_err(|_| store::StoreError::Integrity)
    }
    pub async fn start(
        &self,
        store: &Store,
        actor: &Actor,
        key: &str,
        request: &CycleStartIntent,
    ) -> Result<contracts::control::CommandResult<CycleStartedV1>, store::StoreError> {
        let reading = self.objects.clone();
        let publishing = self.objects.clone();
        store
            .start_cycle(
                actor,
                key,
                request,
                move |id, size| {
                    let objects = reading.clone();
                    async move {
                        tokio::task::spawn_blocking(move || objects.read(id, size))
                            .await
                            .map_err(|_| store::StoreError::Integrity)?
                            .map_err(|_| store::StoreError::Integrity)
                    }
                },
                move |object| async move {
                    tokio::task::spawn_blocking(move || publishing.put(object.id, &object.bytes))
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(|_| store::StoreError::Integrity)
                },
            )
            .await
    }
}

pub async fn start_request(store: &Store, actor: &Actor, fixture: &Fixture) -> CycleStartIntent {
    let current = store.project(actor, fixture.data.project).await.unwrap();
    let active = store
        .update_project(
            actor,
            &Id::new().to_string(),
            current.id,
            &ProjectUpdate {
                schema_version: SchemaV1,
                expected_revision: current.revision,
                name: current.name,
                description: current.description,
                state: ProjectState::Active,
            },
        )
        .await
        .unwrap()
        .resource;
    CycleStartIntent {
        schema_version: SchemaV1,
        project_id: active.id,
        request: CycleStartV1 {
            schema_version: SchemaV1,
            brief_id: fixture.brief.id,
            expected_revision: active.revision,
            researcher_profile: fixture.researcher_profile,
            reviewer_profile: fixture.reviewer_profile,
        },
    }
}
