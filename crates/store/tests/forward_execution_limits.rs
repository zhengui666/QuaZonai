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

// Exercise the actual business replay on both surviving prepared-statement
// sessions across the application's real historical migration cutover.
#[sqlx::test(migrations = false)]
async fn forward_submit_replays_on_both_warmed_backends_after_real_upgrade(pool: PgPool) {
    use sqlx::{PgConnection, postgres::PgPoolOptions};
    use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};
    type Replay = Result<contracts::control::CommandResult<contracts::forward::ForwardMessageViewV1>, StoreError>;

    async fn pid(connection: &mut PgConnection) -> i32 {
        sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(connection).await.unwrap()
    }
    async fn handoff_prepared_names(connection: &mut PgConnection) -> Vec<String> {
        // Identify the actual business statement by its relation/lock features,
        // without copying or exposing that statement as a test-only constant.
        // Exclude this metadata statement, which contains the same literals.
        sqlx::query_scalar("SELECT name FROM pg_prepared_statements WHERE statement LIKE '%FROM app.handoff_offers h JOIN app.releases r%' AND statement LIKE '%JOIN app.portfolio_candidates c%' AND statement LIKE '%FOR SHARE OF h%' AND statement NOT LIKE '%pg_prepared_statements%' ORDER BY name")
            .fetch_all(connection).await.unwrap()
    }
    async fn persisted(connection: &mut PgConnection, fixture: &forward_support::ForwardFixture) -> Value {
        sqlx::query_scalar("SELECT jsonb_build_object('message',to_jsonb(m),'receipt',(SELECT to_jsonb(c) FROM app.command_receipts c WHERE c.principal_scope=$3 AND c.operation='FORWARD_SUBMIT' AND c.idempotency_key=$2 AND c.resource_id=m.id),'artifact',(SELECT to_jsonb(a) FROM app.artifacts a WHERE a.id=m.report_artifact_id),'message_count',(SELECT count(*) FROM app.forward_messages),'receipt_count',(SELECT count(*) FROM app.command_receipts),'artifact_count',(SELECT count(*) FROM app.artifacts)) FROM app.forward_messages m WHERE m.id=$1")
            .bind(fixture.original.id.as_uuid()).bind(&fixture.message.external_message_id)
            .bind(format!("DOWNSTREAM:{}", fixture.original.downstream_id))
            .fetch_one(connection).await.unwrap()
    }
    async fn replay(store: &store::Store, fixture: &forward_support::ForwardFixture, published: &AtomicUsize) -> Replay {
        store.submit_forward_message(&fixture.actor, &fixture.message, |id: Id, size: DbCounter| {
            let bytes = fixture.objects.lock().unwrap().get(&id).cloned();
            async move {
                let bytes = bytes.ok_or(StoreError::NotFound)?;
                if bytes.len() as u64 != size.get() { return Err(StoreError::Integrity); }
                Ok(bytes)
            }
        }, |_| {
            published.fetch_add(1, Ordering::SeqCst);
            async { Err(StoreError::Invalid("replay_must_not_publish")) }
        }).await
    }
    fn sqlstate(result: &Replay) -> Option<String> {
        match result {
            Err(StoreError::Database(error)) => error.as_database_error()
                .and_then(|error| error.code()).map(|code| code.into_owned()),
            _ => None,
        }
    }
    fn matches_receipt(result: &Replay, fixture: &forward_support::ForwardFixture) -> bool {
        result.as_ref().is_ok_and(|result| result.replayed
            && serde_json::to_value(&result.resource).unwrap() == serde_json::to_value(&fixture.original).unwrap())
    }

    sqlx::migrate!("../../migrations").run_to(202610060103, &pool).await.unwrap();
    let fixture = forward_support::setup(&pool).await;
    let historical_version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(historical_version, 202610060103);
    let original_objects = fixture.objects.lock().unwrap().clone();
    let original_receipt = {
        let mut connection = pool.acquire().await.unwrap();
        persisted(&mut connection, &fixture).await
    };
    assert!(!original_receipt["receipt"].is_null(), "the original message receipt must predate the upgrade");
    let options = pool.connect_options().as_ref().clone();
    // The setup pool is no longer used: all application operations below must
    // run on exactly A/B/C, followed by the one new spare D after C is retired.
    pool.close().await;
    let connected = Arc::new(Mutex::new(Vec::<i32>::new()));
    let tracked = Arc::clone(&connected);
    let controlled = PgPoolOptions::new().max_connections(3).min_connections(0)
        .idle_timeout(None).max_lifetime(None)
        .after_connect(move |connection, _| {
            let tracked = Arc::clone(&tracked);
            Box::pin(async move {
                let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                    .fetch_one(connection).await?;
                tracked.lock().expect("backend evidence mutex").push(backend);
                Ok(())
            })
        }).connect_lazy_with(options);
    let store = store::Store::from_pool(controlled.clone());
    let published = AtomicUsize::new(0);
    let mut a = controlled.acquire().await.unwrap();
    let mut b = controlled.acquire().await.unwrap();
    let mut c = controlled.acquire().await.unwrap();
    let pid_a = pid(&mut a).await;
    let pid_b = pid(&mut b).await;
    let pid_c = pid(&mut c).await;
    assert_ne!(pid_a, pid_b); assert_ne!(pid_a, pid_c); assert_ne!(pid_b, pid_c);
    assert_eq!(*connected.lock().unwrap(), vec![pid_a, pid_b, pid_c]);

    // B/C occupy the other slots, so the real Store transaction must take A.
    drop(a);
    let warm_a = replay(&store, &fixture, &published).await;
    a = controlled.acquire().await.unwrap();
    let warm_pid_a = pid(&mut a).await;
    // A/C occupy the other slots, so the next real transaction must take B.
    drop(b);
    let warm_b = replay(&store, &fixture, &published).await;
    b = controlled.acquire().await.unwrap();
    let warm_pid_b = pid(&mut b).await;
    let prepared_a = handoff_prepared_names(&mut a).await;
    let prepared_b = handoff_prepared_names(&mut b).await;
    eprintln!("PR190_BUSINESS_REPLAY {}", json!({"phase":"historical_103_warmed",
        "backend_a":pid_a,"backend_b":pid_b,"backend_c":pid_c,
        "returned_backend_a":warm_pid_a,"returned_backend_b":warm_pid_b,
        "replayed_a":matches_receipt(&warm_a,&fixture),"replayed_b":matches_receipt(&warm_b,&fixture),
        "sqlstate_a":sqlstate(&warm_a),"sqlstate_b":sqlstate(&warm_b),
        "prepared_names_a":prepared_a,"prepared_names_b":prepared_b,
        "publisher_calls":published.load(Ordering::SeqCst)}));
    assert_eq!(warm_pid_a, pid_a); assert_eq!(warm_pid_b, pid_b);
    assert!(matches_receipt(&warm_a, &fixture)); assert!(matches_receipt(&warm_b, &fixture));
    assert_eq!(prepared_a.len(), 1); assert_eq!(prepared_b.len(), 1);
    assert_eq!(published.load(Ordering::SeqCst), 0);
    assert_eq!(persisted(&mut a, &fixture).await, original_receipt);

    // A/B remain checked out and idle, with no open transaction. Only C is
    // returned for Store::migrate; the native API detaches and closes C.
    drop(c);
    let migrated = store.migrate().await;
    let after_migration = connected.lock().unwrap().clone();
    eprintln!("PR190_BUSINESS_REPLAY {}", json!({"phase":"real_store_migration",
        "backend_c":pid_c,"connected_backend_pids":after_migration,"migration_succeeded":migrated.is_ok()}));
    migrated.unwrap();
    assert_eq!(after_migration, vec![pid_a, pid_b, pid_c]);
    let mut d = controlled.acquire().await.unwrap();
    let pid_d = pid(&mut d).await;
    assert!(![pid_a, pid_b, pid_c].contains(&pid_d));
    let latest_version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
        .fetch_one(&mut *d).await.unwrap();
    assert_eq!(latest_version, sqlx::migrate!("../../migrations").iter().last().unwrap().version);
    assert_eq!(*connected.lock().unwrap(), vec![pid_a, pid_b, pid_c, pid_d]);
    eprintln!("PR190_BUSINESS_REPLAY {}", json!({"phase":"post_upgrade_spare",
        "backend_d":pid_d,"latest_version":latest_version}));

    let mut outcomes = Vec::new();
    // Keep B/D occupied for A, and A/D occupied for B. Gather both backends'
    // results in both rounds before asserting successful business replay.
    for round in 1..=2 {
        drop(a);
        let result_a = replay(&store, &fixture, &published).await;
        a = controlled.acquire().await.unwrap();
        let actual_a = pid(&mut a).await;
        eprintln!("PR190_BUSINESS_REPLAY {}", json!({"phase":"post_upgrade_replay",
            "side":"a","round":round,"expected_backend":pid_a,"returned_backend":actual_a,
            "replayed":matches_receipt(&result_a,&fixture),"sqlstate":sqlstate(&result_a)}));
        outcomes.push(("a", round, pid_a, actual_a, result_a));
        drop(b);
        let result_b = replay(&store, &fixture, &published).await;
        b = controlled.acquire().await.unwrap();
        let actual_b = pid(&mut b).await;
        eprintln!("PR190_BUSINESS_REPLAY {}", json!({"phase":"post_upgrade_replay",
            "side":"b","round":round,"expected_backend":pid_b,"returned_backend":actual_b,
            "replayed":matches_receipt(&result_b,&fixture),"sqlstate":sqlstate(&result_b)}));
        outcomes.push(("b", round, pid_b, actual_b, result_b));
    }
    let final_prepared_a = handoff_prepared_names(&mut a).await;
    let final_prepared_b = handoff_prepared_names(&mut b).await;
    let final_backends = connected.lock().unwrap().clone();
    let final_receipt = persisted(&mut a, &fixture).await;
    let objects_unchanged = original_objects == *fixture.objects.lock().unwrap();
    let publisher_calls = published.load(Ordering::SeqCst);
    eprintln!("PR190_BUSINESS_REPLAY {}", json!({"phase":"final_invariants",
        "connected_backend_pids":final_backends,"prepared_names_a":final_prepared_a,
        "prepared_names_b":final_prepared_b,"publisher_calls":publisher_calls,
        "objects_unchanged":objects_unchanged,"receipt_unchanged":final_receipt==original_receipt}));
    // Release every held slot before the final assertions, including RED.
    drop(a); drop(b); drop(d);
    controlled.close().await;
    assert_eq!(final_backends, vec![pid_a, pid_b, pid_c, pid_d], "no extra or replaced native connection is allowed");
    assert_eq!(final_prepared_a, prepared_a, "A's business prepared statement must not be cleared/replaced");
    assert_eq!(final_prepared_b, prepared_b, "B's business prepared statement must not be cleared/replaced");
    assert_eq!(publisher_calls, 0); assert!(objects_unchanged); assert_eq!(final_receipt, original_receipt);
    assert_eq!(outcomes.len(), 4);
    for (side, round, expected, actual, result) in outcomes {
        assert_eq!(actual, expected, "{side} round {round}: application connection was replaced");
        let result = result.unwrap_or_else(|error| panic!("{side} round {round}: actual Store replay failed: {error}"));
        assert!(result.replayed, "{side} round {round}: receipt must replay");
        assert_eq!(serde_json::to_value(&result.resource).unwrap(), serde_json::to_value(&fixture.original).unwrap(),
            "{side} round {round}: original receipt resource changed");
    }
}
