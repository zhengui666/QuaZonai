//! Real-PG admission/trigger regressions with declared relational source fixtures.
//! No execution or qualified scientific result is inferred from these fixtures.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/forward_legacy_receipt.rs"]
mod forward_legacy_receipt;
#[path = "../../../tests/support/forward.rs"]
mod forward_support;
use contracts::{DbCounter, Id, SchemaV1, lifecycle::JobLimitsV1};
use forward_support::{
    research as research_support, runtime_observation::protocol_fixture as runtime_support,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use store::{StoreError, lifecycle::ClaimResult};
fn limits(cpu: Option<u64>, wall: Option<u32>, memory: u32, output: Option<u64>) -> JobLimitsV1 {
    JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: cpu.map(|n| DbCounter::new(n).unwrap()),
        wall_seconds: wall,
        memory_mib: Some(memory),
        output_bytes: output.map(|n| DbCounter::new(n).unwrap()),
    }
}
async fn queue(
    f: &forward_support::ForwardFixture,
) -> contracts::control::CommandResult<contracts::runs::RunSnapshotV1> {
    f.store
        .enqueue_forward_evaluation(
            f.handoff,
            "daily",
            |id, size| {
                let bytes = f
                    .objects
                    .lock()
                    .unwrap()
                    .get(&id)
                    .cloned()
                    .ok_or(StoreError::NotFound);
                async move {
                    let bytes = bytes?;
                    assert_eq!(bytes.len() as u64, size.get());
                    Ok(bytes)
                }
            },
            |object| {
                f.objects.lock().unwrap().insert(object.id, object.bytes);
                async { Ok(()) }
            },
        )
        .await
        .unwrap()
}
#[sqlx::test(migrations = "../../migrations")]
async fn database_derivation_matches_rust_for_null_finite_and_short_wall(pool: PgPool) {
    for original in [
        limits(None, None, 4096, None),
        limits(Some(300), Some(3600), 4096, Some(67108864)),
        limits(Some(30), Some(1), 128, Some(512)),
        limits(None, Some(5), 256, None),
    ] {
        let sql: Value = sqlx::query_scalar("SELECT app.forward_execution_limits($1)")
            .bind(serde_json::to_value(&original).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            sql,
            serde_json::to_value(domain::execution_limits::forward_evaluation(&original).unwrap())
                .unwrap()
        );
    }
    for malformed in [
        json!({"schema_version":1}),
        json!({"schema_version":1,"experiments":0,"cpu_seconds":"1","wall_seconds":null,"memory_mib":512,"output_bytes":null}),
        json!({"schema_version":1,"experiments":0,"cpu_seconds":null,"wall_seconds":0,"memory_mib":512,"output_bytes":null}),
    ] {
        assert!(
            sqlx::query_scalar::<_, Value>("SELECT app.forward_execution_limits($1)")
                .bind(malformed)
                .fetch_one(&pool)
                .await
                .is_err()
        );
    }
}
#[sqlx::test(migrations = "../../migrations")]
async fn absent_candidate_caps_reach_original_native_job_and_replay_without_reading(pool: PgPool) {
    let f =
        forward_support::setup_with_candidate_limits(&pool, limits(None, None, 1024, None)).await;
    let queued = queue(&f).await;
    assert_eq!(queued.resource.deadline_at, None);
    let parallel: Value = sqlx::query_scalar(
        "SELECT normalized_request->'max_parallel_runs' FROM app.run_admissions WHERE run_id=$1",
    )
    .bind(queued.resource.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(parallel, Value::Null);
    let original: Value =
        sqlx::query_scalar("SELECT limits FROM app.run_admissions WHERE run_id=$1")
            .bind(queued.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        original,
        serde_json::to_value(limits(None, None, 1024, None)).unwrap()
    );
    let message = f
        .store
        .read_run_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == queued.resource.id)
        .unwrap();
    let ClaimResult::Leased(lease) = f
        .store
        .claim_run(&message, "inherited-forward", 60)
        .await
        .unwrap()
    else {
        panic!("native lease")
    };
    let job = f
        .store
        .native_job(queued.resource.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.limits.cpu, None);
    assert_eq!(job.spec.limits.memory_mib, Some(1024));
    assert_eq!(job.spec.limits.cpu_seconds, None);
    assert_eq!(job.spec.limits.wall_seconds, None);
    assert_eq!(job.spec.limits.output_bytes, None);
    assert_eq!(job.spec.deadline_at, None);
    let replay = f
        .store
        .enqueue_forward_evaluation(
            f.handoff,
            "daily",
            |_, _| async { panic!("replay cannot reread protected bytes") },
            |_| async { panic!("replay cannot publish new parameters") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, queued.resource);
    assert!(
        sqlx::query("UPDATE app.run_admissions SET limits=$2 WHERE run_id=$1")
            .bind(queued.resource.id.as_uuid())
            .bind(serde_json::to_value(limits(Some(30), Some(60), 512, Some(1048576))).unwrap())
            .execute(&pool)
            .await
            .is_err()
    );
}
async fn reject_duplicate_admission(
    pool: &PgPool,
    run: Id,
    requested: Value,
    parallel: u32,
    runtime: Option<Id>,
) {
    // BEFORE INSERT must reject the forged tuple/provenance before the existing
    // run_id unique constraint. Require the guard's own error, not any SQL error.
    let error=sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) SELECT run_id,project_id,cycle_id,command_key,jsonb_set(normalized_request,'{max_parallel_runs}',to_jsonb($3::integer)),initial_snapshot,$2,COALESCE($4,runtime_id),runtime_revision,runtime_snapshot,initial_queue_message_id FROM app.run_admissions WHERE run_id=$1")
        .bind(run.as_uuid()).bind(requested).bind(parallel as i32).bind(runtime.map(Id::as_uuid)).execute(pool).await.unwrap_err();
    let error = error.as_database_error().unwrap();
    assert_eq!(error.code().as_deref(), Some("23514"));
    assert_eq!(
        error.message(),
        "forward admission requires its exact inherited protected input"
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn finite_source_cannot_be_bypassed_by_null_old_tuple_or_wrong_runtime(pool: PgPool) {
    let f = forward_support::setup_with_candidate_limits(
        &pool,
        limits(Some(8), Some(4), 128, Some(512)),
    )
    .await;
    let queued = queue(&f).await;
    let expected = limits(Some(8), Some(4), 128, Some(512));
    let actual: Value = sqlx::query_scalar("SELECT limits FROM app.run_admissions WHERE run_id=$1")
        .bind(queued.resource.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actual, serde_json::to_value(&expected).unwrap());
    let cpu: i64 = sqlx::query_scalar("SELECT cpu FROM app.run_native_tasks WHERE run_id=$1")
        .bind(queued.resource.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(cpu, 2);
    for forged in [
        limits(None, None, 128, None),
        limits(Some(30), Some(60), 512, Some(1048576)),
    ] {
        reject_duplicate_admission(
            &pool,
            queued.resource.id,
            serde_json::to_value(&forged).unwrap(),
            2,
            None,
        )
        .await;
        let request = store::lifecycle::StandaloneRunSubmission {
            project_id: f.f.project,
            input_set_id: queued.resource.input_set_id,
            runtime_id: f.runtime,
            runtime_revision: contracts::Revision::INITIAL,
            kind: contracts::runs::RunKind::ForwardEvaluate,
            limits: forged,
            max_parallel_runs: Some(2),
        };
        assert!(matches!(
            f.store
                .enqueue_standalone_run(&Id::new().to_string(), &request)
                .await,
            Err(StoreError::Invalid("forward_inherited_execution_limits"))
        ));
    }
    reject_duplicate_admission(&pool, queued.resource.id, actual.clone(), 0, None).await;
    reject_duplicate_admission(&pool, queued.resource.id, actual, 2, Some(Id::new())).await;
    let replay = queue(&f).await;
    assert!(replay.replayed);
    assert_eq!(replay.resource, queued.resource);
}
#[sqlx::test(migrations = "../../migrations")]
async fn finite_inherited_tuple_and_snapshot_remain_immutable_on_replay(pool: PgPool) {
    let f = forward_support::setup(&pool).await;
    let queued = queue(&f).await;
    let before: (Value, Value) =
        sqlx::query_as("SELECT limits,initial_snapshot FROM app.run_admissions WHERE run_id=$1")
            .bind(queued.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        before.0,
        serde_json::to_value(limits(Some(7200), Some(3600), 4096, Some(67108864))).unwrap()
    );
    let replay = queue(&f).await;
    assert!(replay.replayed);
    assert_eq!(replay.resource, queued.resource);
    let after: (Value, Value) =
        sqlx::query_as("SELECT limits,initial_snapshot FROM app.run_admissions WHERE run_id=$1")
            .bind(queued.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
}

#[sqlx::test(migrations = false)]
async fn migration_103_to_104_preserves_existing_receipt_and_rejects_new_legacy_bypass(
    pool: PgPool,
) {
    // Execute the actual historical migrations. The receipt must exist before
    // 104; a receipt first created under 104 cannot establish upgrade replay.
    sqlx::migrate!("../../migrations")
        .run_to(202610060103, &pool)
        .await
        .unwrap();
    let version: i64 =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version, 202610060103);
    let old = forward_support::setup(&pool).await;
    let queued = forward_legacy_receipt::insert(&pool, &old).await;
    let original_limits: Value =
        sqlx::query_scalar("SELECT limits FROM app.run_admissions WHERE run_id=$1")
            .bind(queued.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        original_limits,
        serde_json::to_value(limits(Some(30), Some(60), 512, Some(1048576))).unwrap()
    );

    async fn stored_receipt(pool: &PgPool, run: Id) -> Value {
        sqlx::query_scalar(
            "SELECT jsonb_build_object(
                'run',to_jsonb(r),'admission',to_jsonb(a),'task',to_jsonb(t),
                'input',to_jsonb(f),'parameters',to_jsonb(p),
                'queue',(SELECT jsonb_agg(to_jsonb(q) ORDER BY q.msg_id) FROM pgmq.q_runs q),
                'artifact_count',(SELECT count(*) FROM app.artifacts),
                'admission_count',(SELECT count(*) FROM app.run_admissions))
             FROM app.runs r JOIN app.run_admissions a ON a.run_id=r.id
             JOIN app.run_native_tasks t ON t.run_id=r.id
             JOIN app.forward_evaluation_inputs f ON f.input_set_id=r.input_set_id
             JOIN app.artifacts p ON p.id=t.parameters_artifact_id WHERE r.id=$1",
        )
        .bind(run.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap()
    }
    let before = stored_receipt(&pool, queued.resource.id).await;
    let original_objects = old.objects.lock().unwrap().clone();
    // Exercise the application's native migration transaction and lock handling.
    old.store.migrate().await.unwrap();
    let version: i64 =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await
            .unwrap();
    // The application migrates through 104 and all later additive migrations.
    // Keep the exact 103 starting point without freezing the current head here.
    let migrations = sqlx::migrate!("../../migrations");
    assert_eq!(version, migrations.iter().last().unwrap().version);
    let applied_104: bool =
        sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version=202610060104")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        applied_104,
        "the specific Forward protection must be applied"
    );
    assert_eq!(before, stored_receipt(&pool, queued.resource.id).await);
    let replay = old
        .store
        .enqueue_forward_evaluation(
            old.handoff,
            "daily",
            |_, _| async { panic!("upgraded replay cannot reread protected bytes") },
            |_| async { panic!("upgraded replay cannot publish new parameters") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, queued.resource);
    assert_eq!(before, stored_receipt(&pool, queued.resource.id).await);
    assert_eq!(original_objects, *old.objects.lock().unwrap());

    // Keeping historical fixed receipts must not grant that tuple to a new
    // source whose immutable limits are tighter after the upgrade.
    let new = forward_support::setup_with_candidate_limits(
        &pool,
        limits(Some(8), Some(4), 128, Some(512)),
    )
    .await;
    let distinct_source_ids: bool = sqlx::query_scalar(
        "SELECT count(*)=2 AND count(DISTINCT a.initial_queue_message_id)=2
         FROM app.run_admissions a JOIN app.runs r ON r.id=a.run_id
         WHERE a.project_id IN ($1,$2) AND r.kind='PORTFOLIO_BUILD'",
    )
    .bind(old.f.project.as_uuid())
    .bind(new.f.project.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        distinct_source_ids,
        "source fixtures must own distinct native queue identities"
    );
    let new_run = queue(&new).await;
    reject_duplicate_admission(&pool, new_run.resource.id, original_limits, 2, None).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn forward_parallel_choice_preserves_normalized_identity_without_fixed_two(pool: PgPool) {
    let f = forward_support::setup(&pool).await;
    let queued = queue(&f).await;
    for parallel in [Value::Null, json!(1), json!(3), json!(u32::MAX)] {
        // A valid alternate explicit ceiling reaches the immutable run's unique
        // constraint; the BEFORE guard must no longer require the value two.
        let error = sqlx::query("INSERT INTO app.run_admissions SELECT (jsonb_populate_record(NULL::app.run_admissions, to_jsonb(a) || jsonb_build_object('normalized_request', jsonb_set(a.normalized_request,'{max_parallel_runs}',$2)))).* FROM app.run_admissions a WHERE a.run_id=$1")
            .bind(queued.resource.id.as_uuid()).bind(parallel).execute(&pool).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(), Some("23505"));
    }
    for malformed in [
        json!(0),
        json!(-1),
        json!("3"),
        json!(u64::from(u32::MAX) + 1),
    ] {
        let error = sqlx::query("INSERT INTO app.run_admissions SELECT (jsonb_populate_record(NULL::app.run_admissions, to_jsonb(a) || jsonb_build_object('normalized_request', jsonb_set(a.normalized_request,'{max_parallel_runs}',$2)))).* FROM app.run_admissions a WHERE a.run_id=$1")
            .bind(queued.resource.id.as_uuid()).bind(malformed).execute(&pool).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(), Some("23514"));
    }
    for field in ["project_id", "input_set_id", "runtime_id"] {
        let error = sqlx::query("INSERT INTO app.run_admissions SELECT (jsonb_populate_record(NULL::app.run_admissions, to_jsonb(a) || jsonb_build_object('normalized_request', jsonb_set(a.normalized_request,ARRAY[$2::text],to_jsonb($3::text))))).* FROM app.run_admissions a WHERE a.run_id=$1")
            .bind(queued.resource.id.as_uuid()).bind(field).bind(Id::new().to_string()).execute(&pool).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(), Some("23514"));
    }
}
