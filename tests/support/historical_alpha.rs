//! Test-only original Alpha provenance, not a live Agent or a startup API.
//! Applied Alpha tables reference run_missions; these controlled historical facts
//! keep scientific regressions independent of removed session/Turn execution.
#![allow(dead_code)]
use super::cycle_support;
use contracts::{lifecycle::JobLimitsV1, DbCounter, Id, Revision, SchemaV1};
use sqlx::PgPool;
use store::{
    authority::Actor,
    lifecycle::{NextRuntimeAction, RunLease, RuntimeSnapshot, WorkerFence},
    Store,
};

pub async fn parent(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    cycle: Id,
) -> RunLease {
    parent_role(pool, store, actor, f, cycle, "RESEARCHER").await
}

pub async fn reviewer(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    cycle: Id,
) -> RunLease {
    parent_role(pool, store, actor, f, cycle, "INDEPENDENT_REVIEWER").await
}

async fn parent_role(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    cycle: Id,
    role: &str,
) -> RunLease {
    let run = Id::new();
    let attempt = Id::new();
    let context = &f.freeze.execution_context;
    let time: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let wall = f
        .brief
        .content
        .budget
        .max_wall_seconds
        .unwrap_or(1800)
        .min(1800);
    let deadline = time + chrono::Duration::seconds(i64::from(wall));
    let limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: Some(DbCounter::new(10).unwrap()),
        wall_seconds: Some(wall),
        memory_mib: Some(1024),
        output_bytes: Some(DbCounter::new(16_777_216).unwrap()),
    };
    let runtime: RuntimeSnapshot = sqlx::query_scalar("SELECT jsonb_build_object('schema_version',1,'endpoint',endpoint,'credential_ref',credential_ref,'tls_policy',tls_policy,'ca_certificate_ref',ca_certificate_ref,'development_http',development_http,'protocol_version',protocol_version,'allowed_capabilities',allowed_capabilities) FROM app.runtime_integrations WHERE id=$1")
        .bind(context.runtime_id.as_uuid()).fetch_one(pool).await.map(|value: serde_json::Value| serde_json::from_value(value).unwrap()).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let profiles: Option<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as("SELECT researcher_profile_id,reviewer_profile_id FROM app.cycle_startups WHERE cycle_id=$1")
        .bind(cycle.as_uuid()).fetch_optional(&mut *tx).await.unwrap();
    let (researcher, reviewer) = if let Some(profiles) = profiles {
        profiles
    } else {
        let researcher = Id::new();
        let reviewer = Id::new();
        for profile in [researcher, reviewer] {
            sqlx::query("INSERT INTO app.codex_profiles(id,name,connection_mode,profile_origin,codex_home_ref,use_default_model_settings,saved_fast_mode) VALUES($1,'historical Alpha provenance fixture','SYSTEM','MANAGED_VOLUME',$2,true,false)")
                .bind(profile.as_uuid()).bind(format!("historical-test/{profile}")).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("INSERT INTO app.cycle_startups(cycle_id,project_id,initial_run_id,researcher_profile_id,researcher_profile_revision,reviewer_profile_id,reviewer_profile_revision) SELECT $1,$2,id,$3,1,$4,1 FROM app.runs WHERE cycle_id=$1 AND kind='DATA_VALIDATE' ORDER BY queued_at LIMIT 1")
            .bind(cycle.as_uuid()).bind(f.data.project.as_uuid()).bind(researcher.as_uuid()).bind(reviewer.as_uuid()).execute(&mut *tx).await.unwrap();
        (researcher.as_uuid(), reviewer.as_uuid())
    };
    let profile = if role == "RESEARCHER" {
        researcher
    } else {
        reviewer
    };
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at,current_attempt_no,active_attempt_id) VALUES($1,$2,$3,'AGENT_RESEARCH',$4,'RUNNING',$5,$6,1,$7)")
        .bind(run.as_uuid()).bind(f.data.project.as_uuid()).bind(cycle.as_uuid()).bind(context.discovery_input_set_id.as_uuid()).bind(deadline).bind(time).bind(attempt.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,runtime_id,external_job_id,dispatch_state,runtime_state) VALUES($1,$2,1,'historical-alpha-fixture',1,$3,$4,$5,'ACKNOWLEDGED','RUNNING')")
        .bind(attempt.as_uuid()).bind(run.as_uuid()).bind(deadline).bind(context.runtime_id.as_uuid()).bind(format!("{run}/1")).execute(&mut *tx).await.unwrap();
    let msg: i64 = sqlx::query_scalar(
        "SELECT pgmq.send('runs',jsonb_build_object('schema_version',1,'run_id',$1::uuid))",
    )
    .bind(run.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    // Historical binding only. No generic admission call is used to create an Agent.
    sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) VALUES($1,$2,$3,$4,'{\"schema_version\":1,\"historical_fixture\":true}','{\"schema_version\":1,\"historical_fixture\":true}',$5,$6,$7,$8,$9)")
        .bind(run.as_uuid()).bind(f.data.project.as_uuid()).bind(cycle.as_uuid()).bind(format!("historical/{run}"))
        .bind(serde_json::to_value(&limits).unwrap()).bind(context.runtime_id.as_uuid()).bind(context.runtime_revision.get() as i64).bind(serde_json::to_value(&runtime).unwrap()).bind(msg).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_missions(run_id,project_id,cycle_id,role,profile_id,profile_revision,profile_snapshot) VALUES($1,$2,$3,$5,$4,1,jsonb_build_object('schema_version',1,'historical_fixture',true,'profile',jsonb_build_object('id',$4::uuid,'revision','1','profile_origin','MANAGED_VOLUME','connection_mode','SYSTEM')))")
        .bind(run.as_uuid()).bind(f.data.project.as_uuid()).bind(cycle.as_uuid()).bind(profile).bind(role).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    RunLease {
        run: store.get_run(actor, run).await.unwrap(),
        fence: WorkerFence {
            attempt_id: attempt,
            worker_owner_id: "historical-alpha-fixture".into(),
            owner_epoch: Revision::INITIAL,
        },
        attempt_no: 1,
        external_job_id: format!("{run}/1"),
        lease_expires_at: deadline,
        runtime,
        limits,
        action: NextRuntimeAction::Reconcile,
    }
}

/// Controlled historical parent terminal for selection tests, not a new engine outcome.
pub async fn terminal(pool: &PgPool, lease: &RunLease) {
    sqlx::query("UPDATE app.run_attempts SET dispatch_state='TERMINAL',runtime_state='CANCELLED' WHERE id=$1").bind(lease.fence.attempt_id.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("UPDATE app.runs SET state='CANCELLED',finished_at=clock_timestamp() WHERE id=$1")
        .bind(lease.run.id.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.run_terminal_receipts(run_id,attempt_id,terminal_state,observation,result_snapshot) SELECT id,active_attempt_id,state,'{\"schema_version\":1,\"source\":\"CONTROLLED_HISTORICAL_TEST\"}',to_jsonb(r)||jsonb_build_object('schema_version',1,'revision',revision::text,'last_event_seq',last_event_seq::text) FROM app.runs r WHERE id=$1")
        .bind(lease.run.id.as_uuid()).execute(pool).await.unwrap();
}

/// Saved original independent-review metadata only, not a model call or science result.
pub async fn review(
    pool: &PgPool,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Id {
    let profile: uuid::Uuid =
        sqlx::query_scalar("SELECT profile_id FROM app.run_missions WHERE run_id=$1")
            .bind(lease.run.id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO app.codex_sessions(project_id,cycle_id,run_id,role,profile_id,profile_revision,thread_id,codex_version,protocol_schema_version,requested_settings,native_history_ref) VALUES($1,$2,$3,'INDEPENDENT_REVIEWER',$4,1,$5,'controlled-historical','v2',jsonb_build_object('schema_version',1,'profile_id',$4::uuid,'profile_revision','1','profile_origin','MANAGED_VOLUME','connection_mode','SYSTEM'),'historical-fixture') ON CONFLICT(run_id) DO NOTHING")
        .bind(f.data.project.as_uuid()).bind(lease.run.cycle_id.map(Id::as_uuid)).bind(lease.run.id.as_uuid()).bind(profile).bind(format!("historical/{}",lease.run.id)).execute(pool).await.unwrap();
    let session: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.codex_sessions WHERE run_id=$1")
            .bind(lease.run.id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let reservation = Id::new();
    let request = Id::new();
    let summary = Id::new();
    let native = format!("historical/{reservation}");
    for (id, kind, schema) in [
        (request, "PARAMETERS", "qz.mission_turn"),
        (summary, "REPORT", "qz.mission_summary"),
    ] {
        let bytes = serde_json::to_vec(&serde_json::json!({"schema_version":1,"historical_fixture":true,"text":"Controlled original independent assessment; not native inference"})).unwrap();
        f.objects.put(id, &bytes).unwrap();
        sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,$5,'application/json',$6,'1','LOCAL',$7,'1',$8,'EVALUATOR_ONLY','SYNTHETIC','RUNTIME','REFERENCED')")
            .bind(id.as_uuid()).bind(f.data.project.as_uuid()).bind(lease.run.id.as_uuid()).bind(lease.fence.attempt_id.as_uuid()).bind(kind).bind(schema).bind(id.to_string()).bind(bytes.len() as i64).execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO app.model_turn_reservations(id,project_id,cycle_id,run_id,session_id,attempt_id,command_key,turn_kind,reserved_tokens,request_artifact_id,deadline_at,owner_epoch,profile_revision,ordinal) SELECT $1,$2,$3,$4,$5,$6,$7,'RESEARCH',100,$8,$9,1,1,coalesce(max(ordinal),0)+1 FROM app.model_turn_reservations WHERE session_id=$5")
        .bind(reservation.as_uuid()).bind(f.data.project.as_uuid()).bind(lease.run.cycle_id.map(Id::as_uuid)).bind(lease.run.id.as_uuid()).bind(session).bind(lease.fence.attempt_id.as_uuid()).bind(format!("mission/review/{experiment}")).bind(request.as_uuid()).bind(lease.run.deadline_at).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.mission_review_turns(reservation_id,run_id,cycle_id,experiment_id,alpha_version_id,validation_evaluation_id) SELECT $1,$2,cycle_id,experiment_id,review_alpha_version_id,evaluation_id FROM app.cycle_selection_trials WHERE cycle_id=$3 AND experiment_id=$4")
        .bind(reservation.as_uuid()).bind(lease.run.id.as_uuid()).bind(lease.run.cycle_id.map(Id::as_uuid)).bind(experiment.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.model_turn_dispatches(reservation_id,owner_epoch,rpc_request_id) VALUES($1,1,$2)").bind(reservation.as_uuid()).bind(reservation.to_string()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.model_turn_bindings(reservation_id,session_id,native_turn_id) VALUES($1,$2,$3)").bind(reservation.as_uuid()).bind(session).bind(&native).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.model_turn_terminals(reservation_id,native_turn_id,outcome,reason_code,observed_at) VALUES($1,$2,'SUCCEEDED','CONTROLLED_HISTORICAL_TEST',clock_timestamp())").bind(reservation.as_uuid()).bind(&native).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.model_turn_receipts(reservation_id,outcome,actual_tokens,usage_source,reason_code) VALUES($1,'SUCCEEDED',12,'NATIVE_REPORT','CONTROLLED_HISTORICAL_TEST')").bind(reservation.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.model_turn_summaries(reservation_id,artifact_id,native_item_id) VALUES($1,$2,$3)").bind(reservation.as_uuid()).bind(summary.as_uuid()).bind(&native).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.mission_reviews(reservation_id,summary_artifact_id,decision,reasons) VALUES($1,$2,'PASS','[\"Controlled original assessment fixture; not inference\"]')").bind(reservation.as_uuid()).bind(summary.as_uuid()).execute(pool).await.unwrap();
    reservation
}
