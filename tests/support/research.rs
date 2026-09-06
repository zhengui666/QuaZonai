//! Native relational FIXTURE only. No network, account, data provenance or PASS claims.
#![allow(dead_code)]
use chrono::{Duration, Utc};
use contracts::{control::ProjectCreate, research::*, Id, SchemaV1};
use serde_json::json;
use sqlx::PgPool;
use store::{authority::Actor, Store};

pub async fn operator(pool: &PgPool) -> (Store, Actor) {
    let store = Store::from_pool(pool.clone());
    let cap = store
        .issue_bootstrap_capability("$argon2id$fixture-verified-in-http-tests")
        .await
        .unwrap();
    let binding = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ";
    let e = store
        .start_enrollment(cap.id, &cap.verifier, Id::new(), binding)
        .await
        .unwrap();
    let login = store
        .confirm_enrollment(
            e.id,
            binding,
            e.secret_ref,
            e.database_now.timestamp() / 30,
            false,
            None,
        )
        .await
        .unwrap();
    (store, Actor::Browser { login_id: login.id })
}
pub struct ResearchFixture {
    pub project: Id,
    pub artifact: Id,
    pub universe: Id,
    pub assumptions: Id,
    pub runtime: Id,
    pub source: Id,
    pub grant: Id,
    pub discovery: Id,
    pub validation: Id,
    pub sealed: Id,
    pub forward: Id,
}
impl ResearchFixture {
    pub fn input(&self, purpose: InputPurpose) -> InputSetCreate {
        let (dataset_revision_id, role) = match purpose {
            InputPurpose::Discovery => (self.discovery, DataPartition::Discovery),
            InputPurpose::Validation | InputPurpose::Portfolio => {
                (self.validation, DataPartition::Validation)
            }
            InputPurpose::Sealed => (self.sealed, DataPartition::Sealed),
            InputPurpose::Forward => (self.forward, DataPartition::Forward),
        };
        InputSetCreate {
            schema_version: SchemaV1,
            project_id: self.project,
            purpose,
            decision_cutoff: chrono::DateTime::from_timestamp_micros(
                (Utc::now() - Duration::seconds(1)).timestamp_micros(),
            )
            .unwrap(),
            items: vec![
                InputItemV1::Dataset {
                    dataset_revision_id,
                    role,
                },
                InputItemV1::Artifact {
                    artifact_id: self.artifact,
                    role: ArtifactInputRole::Parameters,
                },
            ],
        }
    }
    pub fn policy(&self, input: Id) -> EvaluationPolicyCreate {
        let mut request: EvaluationPolicyCreate =
            serde_json::from_str(include_str!("../contracts/research-policy.json")).unwrap();
        request.project_id = self.project;
        request.comparison_input_set_id = input;
        request.execution_assumptions_id = self.assumptions;
        request.split_policy.sealed_revision_id = self.sealed;
        request
    }
}
pub async fn setup(pool: &PgPool, store: &Store, actor: &Actor) -> ResearchFixture {
    setup_with_use(pool, store, actor, DataUse::Research).await
}
pub async fn setup_with_use(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    allowed: DataUse,
) -> ResearchFixture {
    let project = store
        .create_project(
            actor,
            &Id::new().to_string(),
            &ProjectCreate {
                schema_version: SchemaV1,
                name: "research fixture".into(),
                description: String::new(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let f = ResearchFixture {
        project,
        artifact: Id::new(),
        universe: Id::new(),
        assumptions: Id::new(),
        runtime: Id::new(),
        source: Id::new(),
        grant: Id::new(),
        discovery: Id::new(),
        validation: Id::new(),
        sealed: Id::new(),
        forward: Id::new(),
    };
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','test_fixture','1','LOCAL',$3,'1',1,'RESEARCH','FIXTURE','OPERATOR','AUDIT')")
        .bind(f.artifact.as_uuid()).bind(project.as_uuid()).bind(format!("fixture/{}",f.artifact)).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.universe_versions(id,name,membership_artifact_id,instrument_definition_artifact_id,calendar_ref,calendar_version,selection_asof,has_historical_membership,coverage_start,coverage_end) VALUES($1,'fixture',$2,$2,'fixture','1','2020-01-01',true,'2010-01-01','2021-01-01')")
        .bind(f.universe.as_uuid()).bind(f.artifact.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.execution_assumptions(id,venue_capability_ref,engine_image_ref,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref) VALUES($1,'fixture','fixture','BAR',100,'USD',$2,$3,$3,'CONSERVATIVE_ASSUMPTION','1','fixture')")
        .bind(f.assumptions.as_uuid()).bind(f.artifact.as_uuid()).bind(json!({"schema_version":1})).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'fixture','https://fixture.invalid','SYSTEM_CA','fixture-not-a-secret','{}','1',true)")
        .bind(f.runtime.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.data_sources(id,name,runtime_id,native_catalog_ref,provider_kind,enabled) VALUES($1,'fixture',$2,$3,'fixture',true)")
        .bind(f.source.as_uuid()).bind(f.runtime.as_uuid()).bind(format!("native-fixture/{}",f.source)).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.data_use_grants(id,source_id,version,license_reference,evidence_artifact_id,allowed_uses,valid_from,valid_until,authorized_by) VALUES($1,$2,1,'fixture',$3,$4,clock_timestamp()-interval '1 day',clock_timestamp()+interval '1 day','OPERATOR')")
        .bind(f.grant.as_uuid()).bind(f.source.as_uuid()).bind(f.artifact.as_uuid()).bind(allowed.code()).execute(&mut *tx).await.unwrap();
    for (id, partition) in [
        (f.discovery, "DISCOVERY"),
        (f.validation, "VALIDATION"),
        (f.sealed, "SEALED"),
        (f.forward, "FORWARD"),
    ] {
        sqlx::query("INSERT INTO app.dataset_revisions(id,source_id,data_use_grant_id,native_snapshot_ref,native_storage_version,universe_version_id,schema_version,data_kind,partition_role,event_start,event_end,available_through,row_count,timezone,quality_artifact_id,pit_status,revision_policy,origin) VALUES($1,$2,$3,$4,'1',$5,'1','BAR',$6,'2010-01-01','2020-01-01','2020-01-02',1000,'UTC',$7,'UNVERIFIED','AS_KNOWN_THEN','FIXTURE')")
            .bind(id.as_uuid()).bind(f.source.as_uuid()).bind(f.grant.as_uuid()).bind(format!("native-fixture/{id}"))
            .bind(f.universe.as_uuid()).bind(partition).bind(f.artifact.as_uuid()).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    f
}
pub async fn wait_for_query_lock(pool: &PgPool, query_prefix: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE $1)")
                .bind(format!("{query_prefix}%")).fetch_one(pool).await.unwrap();
            if waiting {return;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.expect("native PostgreSQL lock wait was not observed");
}
