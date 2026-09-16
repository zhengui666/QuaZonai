//! Relational proposal fixture, never a production seed or scientific acceptance.
#![allow(dead_code)]
use super::{brief_support, research_support};
use contracts::{
    artifacts::ResearchArtifactKind, experiments::ExperimentProposalV1, research::InputPurpose, Id,
    SchemaV1,
};
use sqlx::PgPool;
use store::{authority::Actor, Store};

pub struct Fixture {
    pub data: research_support::ResearchFixture,
    pub brief: Id,
    pub cycle: Id,
    pub family: Id,
    pub request: ExperimentProposalV1,
}

pub async fn artifact(pool: &PgPool, project: Id, kind: ResearchArtifactKind, access: &str) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,$5,'1','LOCAL',$6,'1',20,$7,'FIXTURE','OPERATOR','AUDIT')")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(kind.code()).bind(kind.media_type()).bind(kind.schema_name())
        .bind(format!("relational-fixture/{id}")).bind(access).execute(pool).await.unwrap();
    id
}

pub async fn setup(pool: &PgPool, store: &Store, actor: &Actor, maximum: u32) -> Fixture {
    let data = research_support::setup(pool, store, actor).await;
    let mut draft = brief_support::request(store, actor, &data).await;
    draft.request.content.budget.max_experiments = maximum;
    draft.request.content.budget.max_parallel_runs = 1;
    draft.request.content.stop_rule.stop_on_qualified_count = 1;
    let brief = store
        .create_brief(actor, &Id::new().to_string(), &draft)
        .await
        .unwrap()
        .resource;
    // Only the fixture establishes an active project and running parent. This is
    // not an HTTP freeze path or evidence of readiness/PIT/qualification.
    sqlx::query("UPDATE app.projects SET state='ACTIVE' WHERE id=$1")
        .bind(data.project.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE app.research_briefs SET state='FROZEN',frozen_at=clock_timestamp(),revision=revision+1 WHERE id=$1")
        .bind(brief.id.as_uuid()).execute(pool).await.unwrap();
    let family: uuid::Uuid =
        sqlx::query_scalar("SELECT family_id FROM app.evaluation_policies WHERE id=$1")
            .bind(brief.content.evaluation_policy_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let family = Id::try_from(family.to_string()).unwrap();
    let cycle = Id::new();
    sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot) VALUES($1,$2,$3,1,'OPERATOR','RUNNING',$4)")
        .bind(cycle.as_uuid()).bind(data.project.as_uuid()).bind(brief.id.as_uuid())
        .bind(serde_json::to_value(&brief.content.budget).unwrap()).execute(pool).await.unwrap();
    let request = ExperimentProposalV1 {
        schema_version: SchemaV1,
        cycle_id: cycle,
        family_id: family,
        parent_experiment_id: None,
        hypothesis: "Explicit test question".into(),
        expected_failure_modes: "Costs may remove the apparent edge".into(),
        proposal_artifact_id: artifact(
            pool,
            data.project,
            ResearchArtifactKind::Report,
            "RESEARCH",
        )
        .await,
        parameter_artifact_id: artifact(
            pool,
            data.project,
            ResearchArtifactKind::Parameters,
            "RESEARCH",
        )
        .await,
        code_artifact_id: Some(
            artifact(pool, data.project, ResearchArtifactKind::Code, "RESEARCH").await,
        ),
    };
    Fixture {
        data,
        brief: brief.id,
        cycle,
        family,
        request,
    }
}

pub async fn mission(
    pool: &PgPool,
    store: &Store,
    operator: &Actor,
    fixture: &Fixture,
) -> (Id, Id) {
    let input = store
        .create_input_set(
            operator,
            &Id::new().to_string(),
            &fixture.data.input(InputPurpose::Discovery),
        )
        .await
        .unwrap()
        .resource;
    let run = Id::new();
    let attempt = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at,current_attempt_no,active_attempt_id) VALUES($1,$2,$3,'AGENT_RESEARCH',$4,'RUNNING',clock_timestamp()+interval '2 hours',clock_timestamp(),1,$5)")
        .bind(run.as_uuid()).bind(fixture.data.project.as_uuid()).bind(fixture.cycle.as_uuid()).bind(input.header.id.as_uuid()).bind(attempt.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,1,'proposal-fixture',1,clock_timestamp()+interval '2 hours','ACKNOWLEDGED','RUNNING')")
        .bind(attempt.as_uuid()).bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    (run, attempt)
}

pub async fn machine(
    pool: &PgPool,
    project: Id,
    run: Option<Id>,
    kind: &str,
    scopes: &[&str],
) -> Actor {
    let principal = Id::new();
    let credential = Id::new();
    let verifier = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,run_id,enabled,credential_epoch) VALUES($1,'proposal-fixture',$2,$3,$4,true,1)")
        .bind(principal.as_uuid()).bind(kind).bind(project.as_uuid()).bind(run.map(Id::as_uuid)).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,$3,$4,1,$5,clock_timestamp(),clock_timestamp()+interval '1 hour',$6)")
        .bind(credential.as_uuid()).bind(principal.as_uuid()).bind(Id::new().to_string()).bind(verifier.to_string())
        .bind(scopes).bind(if kind == "MISSION" { "MISSION_SERVICE" } else { "OPERATOR" }).execute(pool).await.unwrap();
    Actor::Machine {
        credential_id: credential,
        verifier_ref: verifier,
        operator_grant: None,
    }
}
