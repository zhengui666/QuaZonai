//! Real PG/PGMQ admission over explicitly relational historical Claim/Runtime fixtures.
//! This does not prove a production Claim, multi-day market feedback or OCI execution.
#[path = "research.rs"]
mod research;
#[path = "runtime_observation.rs"]
mod runtime_observation;
#[path = "../../crates/store/tests/support/mod.rs"]
mod support;
use chrono::{Duration, Utc};
use contracts::{
    control::*, delivery::*, forward::*, runtime::*, science::NativeReturnV1, DbCounter, Id,
    SchemaV1,
};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use store::{authority::Actor, StoreError};
pub fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}

pub struct ForwardFixture {
    pub f: support::Fixture,
    pub store: store::Store,
    pub operator: Actor,
    pub actor: Actor,
    pub runtime: Id,
    pub caps: RuntimeCapabilitiesV1,
    pub handoff: Id,
    pub objects: Arc<Mutex<BTreeMap<Id, Vec<u8>>>>,
    pub end: chrono::DateTime<Utc>,
    pub message: ForwardMessageSubmitV1,
    pub original: ForwardMessageViewV1,
    pub policy: AutomationPolicyViewV1,
}

pub async fn setup(pool: &PgPool) -> ForwardFixture {
    let f = support::fixture(pool, support::budget()).await;
    let (mandate, candidate, evaluation) = support::portfolio(pool, &f).await;
    let release = support::delivery_release_metadata(pool, &f, mandate, candidate, evaluation)
        .await
        .unwrap();
    let (store, operator) = research::operator(pool).await;
    let downstream = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'relational feedback fixture','https://example.invalid','not-a-secret','{1}','BOTH',true)").bind(downstream.as_uuid()).execute(pool).await.unwrap();
    let runtime = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'relational observation runtime','https://example.invalid','SYSTEM_CA','not-a-secret',ARRAY['FORWARD_EVALUATE'],'1',true)").bind(runtime.as_uuid()).execute(pool).await.unwrap();
    let mut caps = runtime_observation::configured_capabilities(pool, runtime).await;
    caps.artifact_schemas.push(RuntimeArtifactSchemaV1 {
        name: "qz.forward_evaluation".into(),
        version: "1".into(),
    });
    runtime_observation::publish(
        pool,
        runtime,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(caps.clone()),
        },
        Duration::seconds(60),
    )
    .await;
    sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) SELECT c.run_id,c.project_id,r.cycle_id,'relational-candidate-runtime','{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}',$2,1,'{\"schema_version\":1}',100000 FROM app.portfolio_candidates c JOIN app.runs r ON r.id=c.run_id WHERE c.id=$1")
        .bind(candidate.as_uuid()).bind(runtime.as_uuid()).execute(pool).await.unwrap();
    let input = support::approval_inputs(pool, &f, evaluation).await;
    let approval = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,clock_timestamp(),clock_timestamp()+interval '1 hour')").bind(approval.as_uuid()).bind(release.as_uuid()).bind(downstream.as_uuid()).bind(input.as_uuid()).execute(pool).await.unwrap();
    // Only this isolated test connection sees the controlled operation clock.
    // Triggers remain installed; no history is edited and no host clock is changed.
    sqlx::query("CREATE SCHEMA fixture_clock")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("CREATE FUNCTION fixture_clock.clock_timestamp() RETURNS timestamptz LANGUAGE sql AS $$ SELECT date_trunc('day',pg_catalog.clock_timestamp())-interval '3 days' $$").execute(pool).await.unwrap();
    let handoff = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL search_path=fixture_clock,pg_catalog,app")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.handoff_offers(id,release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at) VALUES($1,$2,$3,$4,'PAPER',1,'OFFERED',clock_timestamp()-interval '1 hour',clock_timestamp()+interval '1 hour')").bind(handoff.as_uuid()).bind(release.as_uuid()).bind(approval.as_uuid()).bind(downstream.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='controlled-historical-claim',claimed_at=clock_timestamp() WHERE id=$1").bind(handoff.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let start: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT claimed_at FROM app.handoff_offers WHERE id=$1")
            .bind(handoff.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(start < Utc::now() - Duration::days(2));
    let principal = store
        .create_principal(
            &operator,
            "downstream-principal",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "downstream fixture".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(f.project),
                downstream_id: Some(downstream),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let store::control::CredentialPreparation::New(ticket) = store
        .prepare_credential_issuance(
            &operator,
            "credential",
            principal,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::ForwardSubmit],
                expires_at: Utc::now() + Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new credential")
    };
    let verifier = Id::new();
    let credential = ticket
        .publish(Id::new(), verifier)
        .await
        .unwrap()
        .resource
        .id;
    let actor = Actor::Machine {
        credential_id: credential,
        verifier_ref: verifier,
        operator_grant: None,
    };
    let objects = Arc::new(Mutex::new(BTreeMap::<Id, Vec<u8>>::new()));
    let read = |id: Id, size: DbCounter| {
        let result = objects.lock().unwrap().get(&id).cloned();
        async move {
            let bytes = result.ok_or(StoreError::NotFound)?;
            assert_eq!(bytes.len() as u64, size.get());
            Ok(bytes)
        }
    };
    let publish = |v: store::lifecycle::native::NativeObjectPublication| {
        objects.lock().unwrap().insert(v.id, v.bytes);
        async { Ok(()) }
    };
    let end = start + Duration::days(2);
    let message = ForwardMessageSubmitV1 {
        schema_version: SchemaV1,
        external_message_id: "daily-original".into(),
        report: ForwardReportContentV1 {
            schema_version: SchemaV1,
            project_id: f.project,
            handoff_id: handoff,
            external_claim_id: "controlled-historical-claim".into(),
            issuer_version: "relational-fixture/1".into(),
            stream_id: "daily".into(),
            sequence: count(1),
            message_revision: 1,
            supersedes_message_id: None,
            window_start: start,
            window_end: end,
            issued_at: end,
            complete: true,
            returns_frequency: Some(ForwardReturnsFrequencyV1::UtcDay),
            returns: (1..=2)
                .map(|day| NativeReturnV1 {
                    timestamp_ns: count(
                        (start + Duration::days(day)).timestamp_nanos_opt().unwrap() as u64,
                    ),
                    value: Some(0.01),
                    reason_code: None,
                })
                .collect(),
        },
    };
    let original = store
        .submit_forward_message(&actor, &message, read, publish)
        .await
        .unwrap()
        .resource;
    assert!(store
        .enqueue_forward_evaluation(handoff, "daily", read, publish)
        .await
        .is_err());
    let requirement:contracts::evidence::MetricRequirementV1=serde_json::from_value(serde_json::json!({"schema_version":1,"metric_code":"FORWARD_DAILY_RETURN_MEAN","scope":"forward","comparator":"GE","threshold_low":"0","threshold_high":null,"required":true,"minimum_observations":"1","method_allowlist":["nautilus-analysis.ReturnsAverage"]})).unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM app.projects WHERE id=$1")
        .bind(f.project.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    let policy = store
        .authorize_automation(
            &operator,
            "policy",
            f.project,
            &AutomationAuthorizeV1 {
                schema_version: SchemaV1,
                expected_project_revision: revision.to_string().try_into().unwrap(),
                content: AutomationPolicyContentV1 {
                    mode: AutomationModeV1::AutoPaper,
                    mandate_id: mandate,
                    downstream_id: downstream,
                    required_paper_observations: 1,
                    minimum_paper_elapsed_seconds: count(86400),
                    max_feedback_age_seconds: count(i64::MAX as u64),
                    promotion_metric_requirements: vec![requirement.clone()],
                    degradation_metric_requirements: vec![requirement],
                    valid_until: Utc::now() + Duration::hours(1),
                    enabled_for_new_rebalances: true,
                    max_rebalances_per_day: 2,
                },
            },
        )
        .await
        .unwrap()
        .resource;
    ForwardFixture {
        f,
        store,
        operator,
        actor,
        runtime,
        caps,
        handoff,
        objects,
        end,
        message,
        original,
        policy,
    }
}
