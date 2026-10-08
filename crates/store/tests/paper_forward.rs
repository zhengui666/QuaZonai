//! Real PostgreSQL Forward transactions over controlled historical relational
//! provenance. This does not claim a multi-day Paper engine or market execution.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/forward_result.rs"]
mod forward_result;
#[path = "../../../tests/support/forward.rs"]
mod forward_support;
use chrono::{Duration, Utc};
use contracts::{control::*, delivery::*, Id, SchemaV1};
use forward_support::{
    research as research_support, runtime_observation::protocol_fixture as runtime_support,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use store::{authority::Actor, StoreError};

#[sqlx::test(migrations = "../../migrations")]
async fn rooted_paper_daily_returns_keep_synthetic_origin_through_measurement(pool: PgPool) {
    let f = forward_support::setup(&pool).await;
    let down = f
        .store
        .create_downstream(
            &f.operator,
            "paper-root-destination",
            &contracts::settings::DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: contracts::settings::DownstreamConfigurationV1 {
                    name: "Model account history".into(),
                    endpoint: "https://model.example".into(),
                    accepted_package_versions: vec![contracts::settings::PackageSchemaVersion::V2],
                    environments: contracts::settings::DownstreamEnvironments::Both,
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    let template=sqlx::query("SELECT r.*,c.mandate_id,c.input_set_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1")
        .bind(f.original.release_id.as_uuid()).fetch_one(&pool).await.unwrap();
    let mandate: uuid::Uuid = template.try_get("mandate_id").unwrap();
    let start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        - Duration::days(3);
    let root = Id::new();
    let bytes =
        serde_json::to_vec(&json!({"schema_version":1,"model_initial_condition":true,"root":root}))
            .unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.portfolio_current_weights','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','SYNTHETIC','OPERATOR','AUDIT')")
        .bind(root.as_uuid()).bind(f.f.project.as_uuid()).bind(root.to_string()).bind(bytes.len() as i64).execute(&pool).await.unwrap();
    f.objects.lock().unwrap().insert(root, bytes);
    // This is an explicitly historical source receipt fixture. Its controlled
    // time is inserted initially, never backfilled by disabling immutability.
    sqlx::query("INSERT INTO app.paper_initial_capital_sources(weights_artifact_id,project_id,mandate_id,downstream_id,trader_id,account_id,execution_assumptions_id,base_currency,starting_capital,created_at) SELECT $1,project_id,id,$2,'MODEL-001','BINANCE-001',execution_assumptions_id,base_currency,capital_assumption,$3 FROM app.portfolio_mandates WHERE id=$4")
        .bind(root.as_uuid()).bind(down.id.as_uuid()).bind(start).bind(mandate).execute(&pool).await.unwrap();
    let run =
        forward_support::support::candidate_run(&mut pool.acquire().await.unwrap(), &f.f).await;
    sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) SELECT $1,project_id,cycle_id,'paper-root-relational-run',normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,nextval(pg_get_serial_sequence('pgmq.q_runs','msg_id')) FROM app.run_admissions WHERE run_id=(SELECT run_id FROM app.portfolio_candidates WHERE id=$2)")
        .bind(run.as_uuid()).bind(template.try_get::<uuid::Uuid,_>("candidate_id").unwrap()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.portfolio_build_tasks(run_id,mandate_id,request,paper_initial_weights_artifact_id) VALUES($1,$2,$3,$4)")
        .bind(run.as_uuid()).bind(mandate).bind(json!({"schema_version":1,"environment":"PAPER","current_weights_source":{"kind":"PAPER_INITIAL_CAPITAL","downstream_id":down.id,"trader_id":"MODEL-001","account_id":"BINANCE-001"}})).bind(root.as_uuid()).execute(&pool).await.unwrap();
    let candidate = Id::new();
    sqlx::query("INSERT INTO app.portfolio_candidates(id,project_id,mandate_id,input_set_id,decision_asof,run_id,solver_status,evidence_status,diagnostics_artifact_id,current_weights_source,current_weights_artifact_id,paper_initial_weights_artifact_id) VALUES($1,$2,$3,$4,$5,$6,'OPTIMAL','VALID',$7,'PAPER_INITIAL_CAPITAL',$7,$7)")
        .bind(candidate.as_uuid()).bind(f.f.project.as_uuid()).bind(mandate).bind(f.f.input_set.as_uuid()).bind(start).bind(run.as_uuid()).bind(root.as_uuid()).execute(&pool).await.unwrap();
    let evaluation = Id::new();
    let mut copied: Value =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM app.evaluations e WHERE id=$1")
            .bind(template.try_get::<uuid::Uuid, _>("evaluation_id").unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    copied["id"] = json!(evaluation);
    copied["subject_candidate_id"] = json!(candidate);
    sqlx::query(
        "INSERT INTO app.evaluations SELECT (jsonb_populate_record(NULL::app.evaluations,$1)).*",
    )
    .bind(copied)
    .execute(&pool)
    .await
    .unwrap();
    let package = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PACKAGE','application/json','qz.target_package','2','LOCAL',$3,'1',1,'DELIVERY','SYNTHETIC','OPERATOR','AUDIT')")
        .bind(package.as_uuid()).bind(f.f.project.as_uuid()).bind(package.to_string()).execute(&pool).await.unwrap();
    let release = Id::new();
    sqlx::query("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment,execution_environment,paper_initial_weights_artifact_id) VALUES($1,$2,$3,'2',$4,$5,'fixture',$6,$6,$6+interval '1 hour','SYNTHETIC','PAPER',$7)")
        .bind(release.as_uuid()).bind(candidate.as_uuid()).bind(package.as_uuid()).bind(mandate).bind(evaluation.as_uuid()).bind(start).bind(root.as_uuid()).execute(&pool).await.unwrap();
    let input = forward_support::support::approval_inputs(&pool, &f.f, evaluation).await;
    for (destination, environment) in [(down.id, "LIVE"), (f.original.downstream_id, "PAPER")] {
        let rejected=sqlx::query("INSERT INTO app.approvals(release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,$3,'OPERATOR',$4,clock_timestamp(),clock_timestamp()+interval '1 hour')")
            .bind(release.as_uuid()).bind(environment).bind(destination.as_uuid()).bind(input.as_uuid()).execute(&pool).await.unwrap_err();
        forward_support::support::sqlstate(rejected, "23514");
    }
    let approval = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,$5,$5+interval '1 hour')")
        .bind(approval.as_uuid()).bind(release.as_uuid()).bind(down.id.as_uuid()).bind(input.as_uuid()).bind(start).execute(&pool).await.unwrap();
    let handoff = Id::new();
    let mut tx = pool.begin().await.unwrap();
    // Reuse the repository's isolated historical-operation clock. All transfer
    // and one-time initialization triggers run, and no host clock changes.
    sqlx::query("SET LOCAL search_path=fixture_clock,pg_catalog,app")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.handoff_offers(id,release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at) VALUES($1,$2,$3,$4,'PAPER',1,'OFFERED',clock_timestamp(),clock_timestamp()+interval '1 hour')")
        .bind(handoff.as_uuid()).bind(release.as_uuid()).bind(approval.as_uuid()).bind(down.id.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='model-history-claim',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(handoff.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    // A missing ACK must never make the same initial condition fresh again,
    // even if an old writer creates another release and bypasses Store.
    let other_release = Id::new();
    sqlx::query("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment,source_kind,execution_environment,paper_initial_weights_artifact_id) SELECT $1,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment,source_kind,execution_environment,paper_initial_weights_artifact_id FROM app.releases WHERE id=$2")
        .bind(other_release.as_uuid()).bind(release.as_uuid()).execute(&pool).await.unwrap();
    let other_approval = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) SELECT $1,$2,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until FROM app.approvals WHERE id=$3")
        .bind(other_approval.as_uuid()).bind(other_release.as_uuid()).bind(approval.as_uuid()).execute(&pool).await.unwrap();
    let other_offer = Id::new();
    sqlx::query("INSERT INTO app.handoff_offers(id,release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at) SELECT $1,$2,$3,downstream_id,environment,2,'OFFERED',offered_at,expires_at FROM app.handoff_offers WHERE id=$4")
        .bind(other_offer.as_uuid()).bind(other_release.as_uuid()).bind(other_approval.as_uuid()).bind(handoff.as_uuid()).execute(&pool).await.unwrap();
    let mut retry = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL search_path=fixture_clock,pg_catalog,app")
        .execute(&mut *retry)
        .await
        .unwrap();
    let duplicate=sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='different-initial-claim',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(other_offer.as_uuid()).execute(&mut *retry).await.unwrap_err();
    assert_eq!(
        duplicate.as_database_error().unwrap().message(),
        "PAPER_ACCOUNT_ALREADY_HAS_STATE"
    );
    retry.rollback().await.unwrap();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT state='CLAIMED' AND acknowledged_at IS NULL FROM app.handoff_offers WHERE id=$1"
    )
    .bind(handoff.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap());
    let principal = f
        .store
        .create_principal(
            &f.operator,
            "paper-report-machine",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Paper history consumer".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(f.f.project),
                downstream_id: Some(down.id),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let store::control::CredentialPreparation::New(ticket) = f
        .store
        .prepare_credential_issuance(
            &f.operator,
            "paper-report-credential",
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::ForwardSubmit, MachineScope::DownstreamClaim],
                expires_at: Utc::now() + Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("credential")
    };
    let verifier = Id::new();
    let credential = ticket.publish(Id::new(), verifier).await.unwrap().resource;
    let machine = Actor::Machine {
        credential_id: credential.id,
        verifier_ref: verifier,
        operator_grant: None,
    };
    // This explicitly historical claim had no authenticated initialization
    // claimant recorded. A new credential cannot retrospectively supply one.
    let consume = PaperInitialExecutionConsumeV1 {
        schema_version: SchemaV1,
        paper_initialization: contracts::science::PaperInitializationRefV1 {
            artifact_id: root,
            downstream_id: down.id,
            trader_id: "MODEL-001".into(),
            account_id: "BINANCE-001".into(),
        },
        release_id: release,
        external_claim_id: "model-history-claim".into(),
        owner_instance_id: Id::new(),
    };
    assert!(matches!(
        f.store
            .consume_paper_initial_execution(
                &machine,
                "unknown-historical-owner",
                handoff,
                &consume
            )
            .await,
        Err(StoreError::Forbidden)
    ));
    let retrofit =
        sqlx::query("UPDATE app.handoff_offers SET paper_claim_credential_id=$2 WHERE id=$1")
            .bind(handoff.as_uuid())
            .bind(credential.id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err();
    assert_eq!(
        retrofit.as_database_error().unwrap().message(),
        "PAPER_CLAIM_CREDENTIAL_BINDING"
    );
    let mut policy = f.policy.content.clone();
    policy.downstream_id = down.id;
    policy.mode = AutomationModeV1::AutoHandoff;
    f.store
        .authorize_automation(
            &f.operator,
            "paper-measurement-policy",
            f.f.project,
            &AutomationAuthorizeV1 {
                schema_version: SchemaV1,
                expected_project_revision: f
                    .store
                    .project(&f.operator, f.f.project)
                    .await
                    .unwrap()
                    .revision,
                content: policy,
            },
        )
        .await
        .unwrap();
    let read = |id: Id, size: contracts::DbCounter| {
        let bytes = f.objects.lock().unwrap().get(&id).cloned();
        async move {
            let bytes = bytes.ok_or(StoreError::NotFound)?;
            assert_eq!(bytes.len() as u64, size.get());
            Ok(bytes)
        }
    };
    let publish = |value: store::lifecycle::native::NativeObjectPublication| {
        f.objects.lock().unwrap().insert(value.id, value.bytes);
        async { Ok(()) }
    };
    let mut message = f.message.clone();
    message.external_message_id = "model-daily".into();
    message.report.handoff_id = handoff;
    message.report.external_claim_id = "model-history-claim".into();
    let recorded = f
        .store
        .submit_forward_message(&machine, &message, read, publish)
        .await
        .unwrap()
        .resource;
    let origin: String = sqlx::query_scalar("SELECT origin FROM app.artifacts WHERE id=$1")
        .bind(recorded.report_artifact_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origin, "SYNTHETIC");
    assert_eq!(
        f.store
            .prepare_forward_evaluation(f.f.project)
            .await
            .unwrap(),
        Some((handoff, "daily".into()))
    );
    let queued = f
        .store
        .enqueue_forward_evaluation(handoff, "daily", read, publish)
        .await
        .unwrap()
        .resource;
    let origin:(String,String)=sqlx::query_as("SELECT t.origin,a.origin FROM app.run_native_tasks t JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE t.run_id=$1")
        .bind(queued.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(origin, ("SYNTHETIC".into(), "SYNTHETIC".into()));
    let (queued_message, _, _, _) =
        forward_result::complete(&pool, &f.store, queued.id, &f.caps, &f.objects, 0.01).await;
    let evaluated = f
        .store
        .publish_scientific_result(queued.id, read, publish)
        .await
        .unwrap()
        .unwrap()
        .resource;
    let report:(String,uuid::Uuid)=sqlx::query_as("SELECT a.origin,a.id FROM app.evaluations e JOIN app.artifacts a ON a.id=e.report_artifact_id WHERE e.id=$1")
        .bind(evaluated.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(report.0, "SYNTHETIC");
    let report_id: Id = report.1.to_string().try_into().unwrap();
    let body: Value = serde_json::from_slice(&f.objects.lock().unwrap()[&report_id]).unwrap();
    assert_eq!(body["execution_environment"], "PAPER");
    assert_eq!(body["economic_origin"], "SYNTHETIC");
    assert_eq!(body["paper_initial_weights_artifact_id"], json!(root));
    f.store.observe_forward(queued.id).await.unwrap().unwrap();
    f.store.acknowledge_run(&queued_message).await.unwrap();
    assert!(matches!(
        f.store.automate_live(f.f.project, read).await,
        Err(StoreError::Invalid(
            "paper_initialization_not_live_eligible"
        ))
    ));
    let again = f
        .store
        .submit_forward_message(&machine, &message, read, |_| async {
            panic!("receipt replay republishes nothing")
        })
        .await
        .unwrap();
    assert!(again.replayed);
    assert_eq!(again.resource.id, recorded.id);
}
