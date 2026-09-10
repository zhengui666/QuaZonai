//! Native relational startup fixture, NOT an authentic market-data/OCI/Codex
//! acceptance. Formal freeze/start use the real Store transaction APIs below.
#![allow(dead_code)]
use super::{research_support, runtime_support};
use chrono::{DateTime, Utc};
use contracts::{
    brief::*,
    control::ProjectUpdate,
    cycles::*,
    research::*,
    runs::ProjectState,
    runtime::{RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    DbCounter, Id, SchemaV1,
};
use serde_json::json;
use sqlx::PgPool;
use store::{authority::Actor, runtime::ProbePreparation, Store};

pub struct Fixture {
    pub data: research_support::ResearchFixture,
    pub brief: BriefView,
    pub freeze: BriefFreezeV1,
}

pub async fn setup(pool: &PgPool, store: &Store, actor: &Actor) -> Fixture {
    let mut data = research_support::setup(pool, store, actor).await;
    let capabilities = runtime_support::capabilities(Utc::now());
    let image = &capabilities.image_refs[0].image_ref;
    let assumptions = Id::new();
    sqlx::query("INSERT INTO app.execution_assumptions(id,venue_capability_ref,engine_image_ref,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref) SELECT $1,venue_capability_ref,$2,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref FROM app.execution_assumptions WHERE id=$3")
        .bind(assumptions.as_uuid()).bind(image).bind(data.assumptions.as_uuid())
        .execute(pool).await.unwrap();
    data.assumptions = assumptions;
    for (field, start, end) in [
        (&mut data.discovery, "2010-01-01", "2015-01-01"),
        (&mut data.validation, "2015-01-01", "2018-01-01"),
        (&mut data.sealed, "2018-01-01", "2020-01-01"),
    ] {
        let original = *field;
        *field = Id::new();
        sqlx::query("INSERT INTO app.dataset_revisions(id,source_id,data_use_grant_id,native_snapshot_ref,native_storage_version,universe_version_id,schema_version,data_kind,partition_role,event_start,event_end,available_through,row_count,timezone,quality_artifact_id,pit_status,revision_policy,origin) SELECT $1,source_id,data_use_grant_id,$2,native_storage_version,universe_version_id,schema_version,data_kind,partition_role,$3::timestamptz,$4::timestamptz,available_through,row_count,timezone,quality_artifact_id,pit_status,revision_policy,origin FROM app.dataset_revisions WHERE id=$5")
            .bind(field.as_uuid()).bind(format!("cycle-fixture/{field}"))
            .bind(start).bind(end).bind(original.as_uuid()).execute(pool).await.unwrap();
    }
    let revision: i64 = sqlx::query_scalar("UPDATE app.runtime_integrations SET allowed_capabilities=ARRAY['DATA_VALIDATE','ALPHA_EVALUATE','AGENT_RESEARCH'] WHERE id=$1 RETURNING revision")
        .bind(data.runtime.as_uuid()).fetch_one(pool).await.unwrap();
    let revision = revision.to_string().try_into().unwrap();
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
    let outcome = RuntimeProbeOutcomeV1::Available {
        capabilities: Box::new(runtime_support::capabilities(Utc::now())),
    };
    let count = serde_json::to_vec(&json!({"schema_version":1,"result":outcome}))
        .unwrap()
        .len();
    // This trusted adapter outcome is explicitly synthetic test setup. The
    // independent runtime_http test exercises actual TLS and ArtifactStore bytes.
    store
        .complete_runtime_probe(
            *ticket,
            outcome,
            Id::new(),
            DbCounter::new(count as u64).unwrap(),
        )
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
        data,
        brief,
        freeze,
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
        },
    }
}
