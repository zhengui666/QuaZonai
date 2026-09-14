//! Real PG/PGMQ admission over explicitly relational historical Claim/Runtime fixtures.
//! This does not prove a production Claim, multi-day market feedback or OCI execution.
#[path = "../../../tests/support/research.rs"]
mod research;
#[path = "../../../tests/support/runtime_observation.rs"]
mod runtime_observation;
mod support;
use chrono::{Duration, Utc};
use contracts::{
    control::*, delivery::*, forward::*, research::*, runtime::*, science::NativeReturnV1,
    DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use store::{authority::Actor, StoreError};
fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
#[sqlx::test(migrations = "../../migrations")]
async fn original_forward_inputs_queue_once_and_revalidate_before_dispatch(pool: PgPool) {
    let f = support::fixture(&pool, support::budget()).await;
    let (mandate, candidate, evaluation) = support::portfolio(&pool, &f).await;
    let release = support::delivery_release_metadata(&pool, &f, mandate, candidate, evaluation)
        .await
        .unwrap();
    let (store, operator) = research::operator(&pool).await;
    let downstream = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'relational feedback fixture','https://example.invalid','not-a-secret','{1}','BOTH',true)").bind(downstream.as_uuid()).execute(&pool).await.unwrap();
    let runtime = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'relational observation runtime','https://example.invalid','SYSTEM_CA','not-a-secret',ARRAY['FORWARD_EVALUATE'],'1',true)").bind(runtime.as_uuid()).execute(&pool).await.unwrap();
    let mut caps = runtime_observation::configured_capabilities(&pool, runtime).await;
    caps.artifact_schemas.push(RuntimeArtifactSchemaV1 {
        name: "qz.forward_evaluation".into(),
        version: "1".into(),
    });
    runtime_observation::publish(
        &pool,
        runtime,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(caps),
        },
        Duration::seconds(60),
    )
    .await;
    sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) SELECT c.run_id,c.project_id,r.cycle_id,'relational-candidate-runtime','{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}',$2,1,'{\"schema_version\":1}',100000 FROM app.portfolio_candidates c JOIN app.runs r ON r.id=c.run_id WHERE c.id=$1")
        .bind(candidate.as_uuid()).bind(runtime.as_uuid()).execute(&pool).await.unwrap();
    let input = support::approval_inputs(&pool, &f, evaluation).await;
    let approval = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,clock_timestamp(),clock_timestamp()+interval '1 hour')").bind(approval.as_uuid()).bind(release.as_uuid()).bind(downstream.as_uuid()).bind(input.as_uuid()).execute(&pool).await.unwrap();
    // Only this isolated test connection sees the controlled operation clock.
    // Triggers remain installed; no history is edited and no host clock is changed.
    sqlx::query("CREATE SCHEMA fixture_clock")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE FUNCTION fixture_clock.clock_timestamp() RETURNS timestamptz LANGUAGE sql AS $$ SELECT date_trunc('day',pg_catalog.clock_timestamp())-interval '3 days' $$").execute(&pool).await.unwrap();
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
            .fetch_one(&pool)
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
    let mut message = ForwardMessageSubmitV1 {
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
        .fetch_one(&pool)
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
    let (left, right) = tokio::join!(
        store.enqueue_forward_evaluation(handoff, "daily", read, publish),
        store.enqueue_forward_evaluation(handoff, "daily", read, publish)
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.resource.id, right.resource.id);
    assert_ne!(left.replayed, right.replayed);
    assert!(left.resource.cycle_id.is_none());
    let original_input = left.resource.input_set_id;
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(left.resource.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued, 1);
    let params: uuid::Uuid = sqlx::query_scalar(
        "SELECT parameters_artifact_id FROM app.run_native_tasks WHERE run_id=$1",
    )
    .bind(left.resource.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let parameter: Id = params.to_string().try_into().unwrap();
    let definition: contracts::execution::NativeTaskParametersV1 =
        serde_json::from_slice(&objects.lock().unwrap()[&parameter]).unwrap();
    assert_eq!(
        definition.job_kind(),
        contracts::runs::RunKind::ForwardEvaluate
    );
    assert!(store
        .create_input_set(
            &operator,
            "forbidden-copy",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: end,
                items: vec![InputItemV1::Artifact {
                    artifact_id: original.report_artifact_id,
                    role: ArtifactInputRole::Report
                }]
            }
        )
        .await
        .is_err());
    let messages = store.read_run_messages(60, 100).await.unwrap();
    let queued = messages
        .iter()
        .find(|m| m.run_id == left.resource.id)
        .unwrap();
    let store::lifecycle::ClaimResult::Leased(lease) = store
        .claim_run(queued, "forward-fixture", 60)
        .await
        .unwrap()
    else {
        panic!("original native lease")
    };
    let job = store
        .native_job(left.resource.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.input_set_id, original_input);
    domain::execution::task(&job.spec, &definition).unwrap();
    message.external_message_id = "daily-correction".into();
    message.report.message_revision = 2;
    message.report.supersedes_message_id = Some(original.id);
    message.report.returns[0].value = Some(0.02);
    store
        .submit_forward_message(&actor, &message, read, publish)
        .await
        .unwrap();
    assert!(store
        .native_job(left.resource.id, &lease.fence)
        .await
        .is_err());
    // Inject a failure after input/Run/queue creation to check the whole transaction.
    sqlx::query("CREATE FUNCTION app.fail_forward_bind() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled binding failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_forward_bind BEFORE INSERT ON app.run_native_tasks FOR EACH ROW EXECUTE FUNCTION app.fail_forward_bind()").execute(&pool).await.unwrap();
    let before: Vec<_> = objects.lock().unwrap().keys().copied().collect();
    assert!(store
        .enqueue_forward_evaluation(handoff, "daily", read, publish)
        .await
        .is_err());
    let allocated: Vec<_> = objects
        .lock()
        .unwrap()
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect();
    assert_eq!(allocated.len(), 1);
    for id in allocated {
        assert!(store
            .discard_unpublished_forward_artifact(f.project, id, |id| {
                objects.lock().unwrap().remove(&id);
                async { Ok(()) }
            })
            .await
            .unwrap());
    }
    assert_eq!(objects.lock().unwrap().len(), before.len());
    let frozen: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.forward_evaluation_inputs WHERE project_id=$1",
    )
    .bind(f.project.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(frozen, 1);
    sqlx::query("DROP TRIGGER fail_forward_bind ON app.run_native_tasks")
        .execute(&pool)
        .await
        .unwrap();
    let second = store
        .enqueue_forward_evaluation(handoff, "daily", read, publish)
        .await
        .unwrap();
    assert_ne!(second.resource.id, left.resource.id);
    store
        .revoke_automation(
            &operator,
            "revoke",
            policy.id,
            &PolicyRevokeV1 {
                schema_version: SchemaV1,
                expected_latest_revocation_id: None,
                effective_at: None,
                reason: "controlled revoke".into(),
            },
        )
        .await
        .unwrap();
    // Historical replay keeps the original queued identity, never a fresh authorization.
    assert_eq!(
        store
            .enqueue_forward_evaluation(
                handoff,
                "daily",
                |_, _| async { panic!("replay must not reread private reports") },
                |_| async { panic!("replay must not republish parameters") }
            )
            .await
            .unwrap()
            .resource
            .id,
        second.resource.id
    );
    let messages = store.read_run_messages(60, 100).await.unwrap();
    let message = messages
        .iter()
        .find(|m| m.run_id == second.resource.id)
        .unwrap();
    let store::lifecycle::ClaimResult::Leased(lease) = store
        .claim_run(message, "revoked-forward", 60)
        .await
        .unwrap()
    else {
        panic!("durable existing Run")
    };
    assert!(store
        .native_job(second.resource.id, &lease.fence)
        .await
        .is_err());
    let generic = store::lifecycle::StandaloneRunSubmission {
        project_id: f.project,
        input_set_id: f.input_set,
        runtime_id: runtime,
        runtime_revision: contracts::Revision::INITIAL,
        kind: contracts::runs::RunKind::ForwardEvaluate,
        max_parallel_runs: 2,
        limits: contracts::lifecycle::JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: count(30),
            wall_seconds: 60,
            memory_mib: 512,
            output_bytes: count(1048576),
        },
    };
    assert!(matches!(
        store
            .enqueue_standalone_run("unregistered-forward", &generic)
            .await,
        Err(StoreError::Invalid("forward_native_input_required"))
    ));
}
