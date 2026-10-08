//! Real PostgreSQL/PGMQ upgrade from 080004, preserving original quota receipts.
//! Controlled historical relational sources test admission, replay and immutable
//! JobSpecs. They do not claim OCI execution, live market data or qualification.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/forward.rs"]
mod forward_support;
use contracts::{
    DbCounter, Id, SchemaV1, control::CommandResult, execution::NativeTaskParametersV1, forward::*,
    lifecycle::JobLimitsV1, research::ArtifactInputRole, runs::RunSnapshotV1,
    runtime_jobs::RuntimeInputV1,
};
use forward_support::{
    research as research_support, runtime_observation::protocol_fixture as runtime_support,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use store::{
    Store, StoreError,
    lifecycle::{ClaimResult, RunLease},
};

const CUTOVER: i64 = 202610080005;
const TASK_GUARD: &str = "native task definition must bind a new authorized Run";
const SPEC_GUARD: &str = "native JobSpec must bind the current unsent Attempt";

fn limits(
    cpu: Option<u64>,
    wall: Option<u32>,
    memory: Option<u32>,
    output: Option<u64>,
) -> JobLimitsV1 {
    serde_json::from_value(json!({
        "schema_version":1,"experiments":0,"cpu_seconds":cpu.map(|n| n.to_string()),
        "wall_seconds":wall,"memory_mib":memory,"output_bytes":output.map(|n| n.to_string())
    }))
    .unwrap()
}

fn rejected(error: sqlx::Error, code: &str, message: Option<&str>) {
    let database = error
        .as_database_error()
        .expect("PostgreSQL rejection, not client failure");
    assert_eq!(database.code().as_deref(), Some(code), "{error:?}");
    if let Some(message) = message {
        assert_eq!(database.message(), message);
    }
}

async fn upgrade_after_shutdown(pool: PgPool) -> PgPool {
    // Actual deployment stops API/Worker, migrates, and uses fresh connections.
    // Keep SQLx's test database and connection limits, including the normal cache.
    let options = pool.options().clone();
    let connect = pool.connect_options().as_ref().clone();
    pool.close().await;
    let migration_pool = options.clone().connect_with(connect.clone()).await.unwrap();
    Store::from_pool(migration_pool.clone())
        .migrate()
        .await
        .unwrap();
    migration_pool.close().await;
    let pool = options.connect_with(connect).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT success FROM _sqlx_migrations WHERE version=$1")
            .bind(CUTOVER)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    pool
}

async fn snapshot(pool: &PgPool) -> Value {
    let mut result = serde_json::Map::new();
    for table in [
        "artifacts",
        "input_sets",
        "input_set_items",
        "runs",
        "run_admissions",
        "run_attempts",
        "run_native_tasks",
        "run_native_attempts",
        "forward_evaluation_inputs",
        "command_receipts",
        "run_events",
        "handoff_offers",
        "handoff_transfers",
    ] {
        let query = format!(
            "SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM app.{table} t"
        );
        let value: Value = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .fetch_one(pool)
            .await
            .unwrap();
        result.insert(table.into(), value);
    }
    result.insert("queue".into(), sqlx::query_scalar("SELECT coalesce(jsonb_agg(to_jsonb(q) ORDER BY msg_id),'[]'::jsonb) FROM pgmq.q_runs q").fetch_one(pool).await.unwrap());
    Value::Object(result)
}

async fn authority_schema(pool: &PgPool) -> Value {
    // Capture actual function definitions as well as installed triggers and FKs.
    // Only the three intended quota functions may change during this upgrade.
    sqlx::query_scalar("SELECT jsonb_build_object(
      'triggers',(SELECT jsonb_agg(jsonb_build_array(c.relname,t.tgname,pg_get_triggerdef(t.oid),t.tgenabled) ORDER BY c.relname,t.tgname)
        FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='app' AND NOT t.tgisinternal),
      'foreign_keys',(SELECT jsonb_agg(jsonb_build_array(c.relname,k.conname,pg_get_constraintdef(k.oid),k.convalidated) ORDER BY c.relname,k.conname)
        FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='app' AND k.contype='f'),
      'other_functions',(SELECT jsonb_agg(pg_get_functiondef(p.oid) ORDER BY p.proname,p.oid)
        FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='app' AND p.prokind='f'
          AND p.proname NOT IN ('forward_execution_limits','guard_native_task_binding','guard_native_attempt_spec')))")
        .fetch_one(pool).await.unwrap()
}

async fn queue(f: &forward_support::ForwardFixture) -> CommandResult<RunSnapshotV1> {
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

async fn claim(f: &forward_support::ForwardFixture, run: Id) -> RunLease {
    let message = f
        .store
        .read_run_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == run)
        .unwrap();
    let ClaimResult::Leased(lease) = f
        .store
        .claim_run(&message, "optional-quota-upgrade", 300)
        .await
        .unwrap()
    else {
        panic!("native Run lease")
    };
    *lease
}

async fn replay(f: &forward_support::ForwardFixture) -> CommandResult<RunSnapshotV1> {
    f.store
        .enqueue_forward_evaluation(
            f.handoff,
            "daily",
            |_, _| async { panic!("replay must not reread frozen input bytes") },
            |_| async { panic!("replay must not publish a second parameters artifact") },
        )
        .await
        .unwrap()
}

// Exact pre-080005 writer shape: inherited budget, explicit null CPU seconds,
// and the then-mandatory numeric CPU=1 task. This inserts through original live
// guards under 080004; it never disables guards or updates immutable old rows.
async fn historical_forward(
    pool: &PgPool,
    f: &forward_support::ForwardFixture,
) -> CommandResult<RunSnapshotV1> {
    let version: i64 =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        version, 202610080004,
        "historical fixture must run under the actual original schema"
    );
    let window = f
        .store
        .forward_window(
            &f.operator,
            f.handoff,
            &ForwardWindowQueryV1 {
                stream_id: "daily".into(),
            },
            |id, size| {
                let bytes = f.objects.lock().unwrap().get(&id).cloned().unwrap();
                assert_eq!(bytes.len() as u64, size.get());
                async move { Ok(bytes) }
            },
        )
        .await
        .unwrap();
    // This fixture has one original message; assert membership before building
    // exactly the native request the historical producer persisted.
    let sources: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.forward_messages WHERE handoff_id=$1 AND stream_id='daily' ORDER BY sequence,message_revision")
        .bind(f.handoff.as_uuid()).fetch_all(pool).await.unwrap();
    assert_eq!(sources, vec![f.original.id.as_uuid()]);
    let end = window.window_end.unwrap();
    let request = NativeForwardRequestV1 {
        window,
        sources: vec![f.original.clone()],
    };
    domain::forward::evaluation::request(&request).unwrap();
    let parameters = NativeTaskParametersV1::EvaluateForward {
        schema_version: SchemaV1,
        request: Box::new(request.clone()),
    };
    let output_schemas = parameters.output_schemas();
    let parameter = Id::new();
    let bytes = serde_json::to_vec(&parameters).unwrap();
    let size = i64::try_from(bytes.len()).unwrap();
    let input = Id::new();
    let run = Id::new();
    let fixed: Value = sqlx::query_scalar("SELECT app.forward_execution_limits(a.limits) FROM app.handoff_offers h JOIN app.releases released ON released.id=h.release_id JOIN app.portfolio_candidates candidate ON candidate.id=released.candidate_id JOIN app.run_admissions a ON a.run_id=candidate.run_id WHERE h.id=$1")
        .bind(f.handoff.as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(
        fixed,
        serde_json::to_value(limits(None, None, Some(512), None)).unwrap()
    );
    let mut tx = pool.begin().await.unwrap();
    let observed: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let policy_until = f.policy.content.valid_until;
    let until = i64::try_from(f.policy.content.max_feedback_age_seconds.get())
        .ok()
        .and_then(chrono::Duration::try_seconds)
        .and_then(|age| end.checked_add_signed(age))
        .unwrap_or(policy_until)
        .min(policy_until);
    assert!(until > observed);
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','REAL','OPERATOR','AUDIT')")
        .bind(parameter.as_uuid()).bind(f.f.project.as_uuid()).bind(parameter.to_string()).bind(size).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,'FORWARD',$3)")
        .bind(input.as_uuid()).bind(f.f.project.as_uuid()).bind(end).execute(&mut *tx).await.unwrap();
    let mut bindings = Vec::new();
    for (ordinal, source) in request.sources.iter().enumerate() {
        sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) VALUES($1,$2,'REPORT',$3)")
            .bind(input.as_uuid()).bind(source.report_artifact_id.as_uuid()).bind(i32::try_from(ordinal).unwrap()).execute(&mut *tx).await.unwrap();
        let artifact =
            sqlx::query("SELECT storage_version,byte_count FROM app.artifacts WHERE id=$1")
                .bind(source.report_artifact_id.as_uuid())
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        bindings.push(RuntimeInputV1::Artifact {
            artifact_id: source.report_artifact_id,
            storage_version: artifact.get("storage_version"),
            byte_count: DbCounter::new(
                u64::try_from(artifact.get::<i64, _>("byte_count")).unwrap(),
            )
            .unwrap(),
            role: ArtifactInputRole::Report,
        });
    }
    sqlx::query("UPDATE app.input_sets SET frozen_at=clock_timestamp() WHERE id=$1")
        .bind(input.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.forward_evaluation_inputs(input_set_id,project_id,handoff_id,policy_id,runtime_id,parameters_artifact_id,request,valid_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(input.as_uuid()).bind(f.f.project.as_uuid()).bind(f.handoff.as_uuid()).bind(f.policy.id.as_uuid()).bind(f.runtime.as_uuid()).bind(parameter.as_uuid()).bind(serde_json::to_value(&parameters).unwrap()).bind(until).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at) VALUES($1,$2,NULL,'FORWARD_EVALUATE',$3,'QUEUED',NULL,$4)")
        .bind(run.as_uuid()).bind(f.f.project.as_uuid()).bind(input.as_uuid()).bind(observed).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_events(run_id,seq,attempt_id,event_type,schema_version,payload,occurred_at) VALUES($1,1,NULL,'run.created',1,'{\"schema_version\":1,\"state\":\"QUEUED\",\"reason\":\"ADMITTED\"}',clock_timestamp())")
        .bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    let snapshot: Value = sqlx::query_scalar("SELECT jsonb_build_object('schema_version',1,'id',id,'project_id',project_id,'cycle_id',cycle_id,'kind',kind,'input_set_id',input_set_id,'state',state,'current_attempt_no',current_attempt_no,'active_attempt_id',active_attempt_id,'last_event_seq',last_event_seq::text,'deadline_at',deadline_at,'cancellation_requested_at',cancellation_requested_at,'terminal_reason_code',terminal_reason_code,'queued_at',queued_at,'started_at',started_at,'finished_at',finished_at,'revision',revision::text) FROM app.runs WHERE id=$1")
        .bind(run.as_uuid()).fetch_one(&mut *tx).await.unwrap();
    let resource: RunSnapshotV1 = serde_json::from_value(snapshot.clone()).unwrap();
    let queue: i64 = sqlx::query_scalar("SELECT pgmq.send('runs',$1)")
        .bind(json!({"schema_version":1,"run_id":run}))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let runtime = sqlx::query("SELECT revision,last_capability_snapshot_artifact_id,jsonb_build_object('schema_version',1,'endpoint',endpoint,'credential_ref',credential_ref,'tls_policy',tls_policy,'ca_certificate_ref',ca_certificate_ref,'development_http',development_http,'protocol_version',protocol_version,'allowed_capabilities',allowed_capabilities) AS snapshot FROM app.runtime_integrations WHERE id=$1")
        .bind(f.runtime.as_uuid()).fetch_one(&mut *tx).await.unwrap();
    let revision: i64 = runtime.get("revision");
    let normalized = json!({"schema_version":1,"project_id":f.f.project,"input_set_id":input,"runtime_id":f.runtime,"runtime_revision":revision.to_string(),"kind":"FORWARD_EVALUATE","limits":fixed,"max_parallel_runs":null});
    sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) VALUES($1,$2,NULL,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(run.as_uuid()).bind(f.f.project.as_uuid()).bind(format!("forward/{input}")).bind(normalized).bind(snapshot).bind(fixed).bind(f.runtime.as_uuid()).bind(revision).bind(runtime.get::<Value,_>("snapshot")).bind(queue).execute(&mut *tx).await.unwrap();
    bindings.push(RuntimeInputV1::Artifact {
        artifact_id: parameter,
        storage_version: "1".into(),
        byte_count: DbCounter::new(u64::try_from(size).unwrap()).unwrap(),
        role: ArtifactInputRole::Parameters,
    });
    let image = &f
        .caps
        .image_refs
        .iter()
        .find(|image| image.job_kind == contracts::runs::RunKind::ForwardEvaluate)
        .unwrap()
        .image_ref;
    sqlx::query("INSERT INTO app.run_native_tasks(run_id,parameters_artifact_id,input_bindings,image_ref,cpu,capability_snapshot_artifact_id,output_schemas,origin,access_class) VALUES($1,$2,$3,$4,1,$5,$6,'REAL','EVALUATOR_ONLY')")
        .bind(run.as_uuid()).bind(parameter.as_uuid()).bind(serde_json::to_value(bindings).unwrap()).bind(image).bind(runtime.get::<uuid::Uuid,_>("last_capability_snapshot_artifact_id")).bind(serde_json::to_value(output_schemas).unwrap()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    f.objects.lock().unwrap().insert(parameter, bytes);
    CommandResult {
        schema_version: SchemaV1,
        replayed: false,
        resource,
    }
}

#[sqlx::test(migrations = false)]
async fn upgrade_080005_preserves_historical_numeric_cpu_receipt_and_exact_launch_json(
    pool: PgPool,
) {
    forward_support::support::migrate_before(&pool, CUTOVER).await;
    let mut launched =
        forward_support::setup_with_candidate_limits(&pool, limits(None, None, Some(512), None))
            .await;
    let mut pending =
        forward_support::setup_with_candidate_limits(&pool, limits(None, None, Some(512), None))
            .await;
    let old = historical_forward(&pool, &launched).await;
    let lease = claim(&launched, old.resource.id).await;
    let job = launched
        .store
        .native_job(old.resource.id, &lease.fence)
        .await
        .unwrap();
    let original_json = serde_json::to_vec(&job.spec).unwrap();
    let original: Value = serde_json::from_slice(&original_json).unwrap();
    assert_eq!(original["limits"]["cpu"], json!(1));
    assert_eq!(original["limits"]["memory_mib"], json!(512));
    assert_eq!(original["limits"]["cpu_seconds"], Value::Null);
    let stored_json: String = sqlx::query_scalar(
        "SELECT spec_json::text FROM app.run_native_attempts WHERE attempt_id=$1",
    )
    .bind(lease.fence.attempt_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let not_launched = historical_forward(&pool, &pending).await;
    let before = snapshot(&pool).await;
    let guards = authority_schema(&pool).await;
    let launched_objects = launched.objects.lock().unwrap().clone();
    let pending_objects = pending.objects.lock().unwrap().clone();
    let pool = upgrade_after_shutdown(pool).await;
    launched.store = Store::from_pool(pool.clone());
    pending.store = Store::from_pool(pool.clone());
    assert_eq!(
        snapshot(&pool).await,
        before,
        "migration may not rewrite historical rows"
    );
    assert_eq!(
        authority_schema(&pool).await,
        guards,
        "all other authority guards remain exact"
    );
    for (fixture, result) in [(&launched, &old), (&pending, &not_launched)] {
        let replayed = replay(fixture).await;
        assert!(replayed.replayed);
        assert_eq!(replayed.resource, result.resource);
    }
    let restored = launched
        .store
        .native_job(old.resource.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&restored.spec).unwrap(),
        original_json,
        "launch JSON stays numeric and byte-for-byte identical"
    );
    let stored_after: String = sqlx::query_scalar(
        "SELECT spec_json::text FROM app.run_native_attempts WHERE attempt_id=$1",
    )
    .bind(lease.fence.attempt_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_after, stored_json);
    assert_eq!(
        snapshot(&pool).await,
        before,
        "replay must not enqueue or allocate again"
    );
    assert_eq!(*launched.objects.lock().unwrap(), launched_objects);
    assert_eq!(*pending.objects.lock().unwrap(), pending_objects);

    // Old queued tasks without a persisted spec must also retain numeric CPU=1
    // on first launch after upgrade. A current-budget recomputation would lose it.
    let pending_lease = claim(&pending, not_launched.resource.id).await;
    let pending_job = pending
        .store
        .native_job(not_launched.resource.id, &pending_lease.fence)
        .await
        .unwrap();
    let pending_spec = serde_json::to_value(&pending_job.spec).unwrap();
    assert_eq!(pending_spec["limits"]["cpu"], json!(1));
    assert_eq!(pending_spec["limits"]["cpu_seconds"], Value::Null);
    assert_eq!(pending_spec["limits"]["memory_mib"], json!(512));
    assert!(
        pending
            .store
            .begin_run_dispatch(not_launched.resource.id, &pending_lease.fence)
            .await
            .unwrap()
    );
    assert!(
        !pending
            .store
            .begin_run_dispatch(not_launched.resource.id, &pending_lease.fence)
            .await
            .unwrap()
    );
    let reconciling = pending
        .store
        .native_job(not_launched.resource.id, &pending_lease.fence)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&reconciling.spec).unwrap(),
        serde_json::to_vec(&pending_job.spec).unwrap()
    );
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_native_attempts WHERE run_id=$1)")
        .bind(not_launched.resource.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1));
}

async fn duplicate_task(pool: &PgPool, run: Id, changes: Value) -> sqlx::Error {
    sqlx::query("INSERT INTO app.run_native_tasks SELECT (jsonb_populate_record(NULL::app.run_native_tasks,to_jsonb(t)||$2)).* FROM app.run_native_tasks t WHERE run_id=$1")
        .bind(run.as_uuid()).bind(changes).execute(pool).await.unwrap_err()
}

async fn duplicate_spec(pool: &PgPool, attempt: Id, run: Id, spec: &Value) -> sqlx::Error {
    sqlx::query("INSERT INTO app.run_native_attempts(attempt_id,run_id,spec_json) VALUES($1,$2,$3)")
        .bind(attempt.as_uuid())
        .bind(run.as_uuid())
        .bind(spec)
        .execute(pool)
        .await
        .unwrap_err()
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_optional_quotas_bind_exact_json_and_preserve_native_authority_guards(pool: PgPool) {
    for (source, expected_cpu) in [
        (limits(None, None, None, None), Value::Null),
        (
            limits(Some(7201), Some(3600), Some(128), Some(512)),
            json!(3),
        ),
        (limits(None, Some(3600), Some(256), None), Value::Null),
        (limits(Some(3601), Some(3600), None, None), json!(2)),
    ] {
        let expected_limits = serde_json::to_value(&source).unwrap();
        let mut f = forward_support::setup_with_candidate_limits(&pool, source).await;
        if expected_cpu == json!(3) {
            // This case needs three cores to test the nontrivial ceil result.
            // Append a local protocol observation; the shared two-core fixture
            // and all production capability checks remain unchanged.
            f.caps.max_cpu = 3;
            forward_support::runtime_observation::publish(
                &pool,
                f.runtime,
                contracts::runtime::RuntimeProbeOutcomeV1::Available {
                    capabilities: Box::new(f.caps.clone()),
                },
                chrono::Duration::seconds(60),
            )
            .await;
        }
        let queued = queue(&f).await;
        let run = queued.resource.id;
        let (cpu, admission): (Option<i64>, Value) = sqlx::query_as("SELECT t.cpu,a.limits FROM app.run_native_tasks t JOIN app.run_admissions a ON a.run_id=t.run_id WHERE t.run_id=$1")
            .bind(run.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(serde_json::to_value(cpu).unwrap(), expected_cpu);
        assert_eq!(admission, expected_limits);
        // A valid tuple reaches uniqueness. Wrong CPU nullness or a legacy core
        // must be rejected by the binding guard before the uniqueness conflict.
        rejected(
            duplicate_task(&pool, run, json!({"cpu":expected_cpu})).await,
            "23505",
            None,
        );
        let wrong_cpu = if expected_cpu.is_null() {
            json!(1)
        } else {
            Value::Null
        };
        for changes in [
            json!({"cpu":wrong_cpu}),
            json!({"cpu":1}),
            json!({"cpu":0}),
            json!({"cpu":-1}),
            json!({"cpu":u64::from(u32::MAX)+1}),
            json!({"access_class":"RESEARCH"}),
            json!({"origin":"SYNTHETIC"}),
            json!({"output_schemas":[{"name":"qz.other","version":"1"}]}),
            json!({"parameters_artifact_id":Id::new()}),
            json!({"capability_snapshot_artifact_id":Id::new()}),
        ] {
            rejected(
                duplicate_task(&pool, run, changes).await,
                "23514",
                Some(TASK_GUARD),
            );
        }
        let lease = claim(&f, run).await;
        let job = f.store.native_job(run, &lease.fence).await.unwrap();
        let spec = serde_json::to_value(&job.spec).unwrap();
        assert_eq!(spec["limits"]["cpu"], expected_cpu);
        for field in ["cpu_seconds", "memory_mib", "wall_seconds", "output_bytes"] {
            assert_eq!(spec["limits"][field], expected_limits[field]);
        }
        rejected(
            duplicate_spec(&pool, lease.fence.attempt_id, run, &spec).await,
            "23505",
            None,
        );
        // Equal JSONB numeric values can retain a decimal representation that
        // Rust's u32 decoder rejects. Bind raw JSON text so the exponent spelling
        // reaches PostgreSQL instead of being normalized by serde_json first.
        for field in ["cpu", "memory_mib"] {
            let Some(value) = spec["limits"][field].as_u64() else {
                continue;
            };
            for wire in [format!("{value}.0"), format!("{value}.0e0")] {
                let malformed: Value = sqlx::query_scalar(
                    "SELECT jsonb_set($1::jsonb,ARRAY['limits',$2::text],$3::text::jsonb)",
                )
                .bind(&spec)
                .bind(field)
                .bind(&wire)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert!(malformed["limits"][field].is_f64());
                assert!(
                    serde_json::from_value::<contracts::runtime_jobs::JobSpecV1>(malformed.clone())
                        .is_err()
                );
                rejected(
                    duplicate_spec(&pool, lease.fence.attempt_id, run, &malformed).await,
                    "23514",
                    Some(SPEC_GUARD),
                );
            }
            // JSONB may erase an exponent with no retained fractional scale.
            // Its resulting integer remains safe and must keep the original
            // uniqueness outcome rather than being rejected as a quota change.
            let canonical: Value = sqlx::query_scalar(
                "SELECT jsonb_set($1::jsonb,ARRAY['limits',$2::text],$3::text::jsonb)",
            )
            .bind(&spec)
            .bind(field)
            .bind(format!("{value}e0"))
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(canonical, spec);
            rejected(
                duplicate_spec(&pool, lease.fence.attempt_id, run, &canonical).await,
                "23505",
                None,
            );
        }
        // Explicit null is meaningful. Neither an absent key nor another JSON
        // type may masquerade as an absent/finite CPU or memory quota.
        for field in [
            "cpu",
            "cpu_seconds",
            "memory_mib",
            "wall_seconds",
            "output_bytes",
        ] {
            let mut missing = spec.clone();
            missing["limits"].as_object_mut().unwrap().remove(field);
            rejected(
                duplicate_spec(&pool, lease.fence.attempt_id, run, &missing).await,
                "23514",
                Some(SPEC_GUARD),
            );
        }
        for field in ["cpu", "memory_mib"] {
            for value in [
                json!(0),
                json!(-1),
                json!("1"),
                json!("null"),
                json!(false),
                json!([]),
                json!({}),
                json!(u64::from(u32::MAX) + 1),
            ] {
                let mut malformed = spec.clone();
                malformed["limits"][field] = value;
                rejected(
                    duplicate_spec(&pool, lease.fence.attempt_id, run, &malformed).await,
                    "23514",
                    Some(SPEC_GUARD),
                );
            }
            let mut mismatch = spec.clone();
            mismatch["limits"][field] = if spec["limits"][field].is_null() {
                json!(1)
            } else {
                Value::Null
            };
            rejected(
                duplicate_spec(&pool, lease.fence.attempt_id, run, &mismatch).await,
                "23514",
                Some(SPEC_GUARD),
            );
        }
        for (field, value) in [
            ("run_id", json!(Id::new())),
            ("input_set_id", json!(Id::new())),
            ("parameters_artifact_id", json!(Id::new())),
            ("image_ref", json!("other-image")),
            ("job_kind", json!("PORTFOLIO_BUILD")),
            ("external_job_id", json!("other-job")),
            ("owner_epoch", json!("99999")),
            ("attempt_no", json!(99999)),
            ("inputs", json!([])),
            ("requested_output_schemas", json!([])),
        ] {
            let mut forged = spec.clone();
            forged[field] = value;
            rejected(
                duplicate_spec(&pool, lease.fence.attempt_id, run, &forged).await,
                "23514",
                Some(SPEC_GUARD),
            );
        }
        let mut missing_deadline = spec.clone();
        missing_deadline
            .as_object_mut()
            .unwrap()
            .remove("deadline_at");
        rejected(
            duplicate_spec(&pool, lease.fence.attempt_id, run, &missing_deadline).await,
            "23514",
            Some(SPEC_GUARD),
        );
        for query in [
            "UPDATE app.run_native_tasks SET cpu=1 WHERE run_id=$1",
            "DELETE FROM app.run_native_tasks WHERE run_id=$1",
            "UPDATE app.run_native_attempts SET spec_json=spec_json WHERE run_id=$1",
            "DELETE FROM app.run_native_attempts WHERE run_id=$1",
            "UPDATE app.run_admissions SET limits=limits WHERE run_id=$1",
        ] {
            rejected(
                sqlx::query(query)
                    .bind(run.as_uuid())
                    .execute(&pool)
                    .await
                    .unwrap_err(),
                "23000",
                Some("immutable domain record"),
            );
        }
        let before = snapshot(&pool).await;
        let replayed = replay(&f).await;
        assert!(replayed.replayed);
        assert_eq!(replayed.resource, queued.resource);
        assert_eq!(snapshot(&pool).await, before);
        assert!(f.store.begin_run_dispatch(run, &lease.fence).await.unwrap());
        assert!(!f.store.begin_run_dispatch(run, &lease.fence).await.unwrap());
        let recovered = f.store.native_job(run, &lease.fence).await.unwrap();
        assert_eq!(
            serde_json::to_vec(&recovered.spec).unwrap(),
            serde_json::to_vec(&job.spec).unwrap()
        );
    }
}

#[sqlx::test(migrations = false)]
async fn upgrade_080005_accepts_only_explicit_null_or_positive_typed_execution_limits(
    pool: PgPool,
) {
    forward_support::support::migrate_before(&pool, CUTOVER).await;
    let null = json!({"schema_version":1,"experiments":0,"cpu_seconds":null,"wall_seconds":null,"memory_mib":null,"output_bytes":null});
    rejected(
        sqlx::query_scalar::<_, Value>("SELECT app.forward_execution_limits($1)")
            .bind(&null)
            .fetch_one(&pool)
            .await
            .unwrap_err(),
        "23514",
        Some("forward source execution limits are invalid"),
    );
    let finite = serde_json::to_value(limits(Some(5), Some(2), Some(128), Some(512))).unwrap();
    let before: Value = sqlx::query_scalar("SELECT app.forward_execution_limits($1)")
        .bind(&finite)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, finite);
    let pool = upgrade_after_shutdown(pool).await;
    for valid in [
        null.clone(),
        finite.clone(),
        serde_json::to_value(limits(None, Some(u32::MAX), Some(u32::MAX), None)).unwrap(),
        serde_json::to_value(limits(Some(1), Some(1), None, Some(i64::MAX as u64))).unwrap(),
    ] {
        let actual: Value = sqlx::query_scalar("SELECT app.forward_execution_limits($1)")
            .bind(&valid)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(actual, valid);
        let rust: JobLimitsV1 = serde_json::from_value(valid).unwrap();
        assert_eq!(
            actual,
            serde_json::to_value(domain::execution_limits::forward_evaluation(&rust).unwrap())
                .unwrap()
        );
    }
    for field in [
        "schema_version",
        "experiments",
        "cpu_seconds",
        "wall_seconds",
        "memory_mib",
        "output_bytes",
    ] {
        let mut missing = null.clone();
        missing.as_object_mut().unwrap().remove(field);
        rejected(
            sqlx::query_scalar::<_, Value>("SELECT app.forward_execution_limits($1)")
                .bind(missing)
                .fetch_one(&pool)
                .await
                .unwrap_err(),
            "23514",
            Some("forward source execution limits are invalid"),
        );
    }
    for field in ["memory_mib", "wall_seconds"] {
        for value in [
            json!(0),
            json!(-1),
            json!("1"),
            json!("null"),
            json!(true),
            json!([]),
            json!({}),
            json!(1.5),
            json!(u64::from(u32::MAX) + 1),
        ] {
            let mut malformed = null.clone();
            malformed[field] = value;
            rejected(
                sqlx::query_scalar::<_, Value>("SELECT app.forward_execution_limits($1)")
                    .bind(malformed)
                    .fetch_one(&pool)
                    .await
                    .unwrap_err(),
                "23514",
                None,
            );
        }
    }
    for field in ["cpu_seconds", "output_bytes"] {
        for value in [
            json!(0),
            json!(1),
            json!("0"),
            json!("-1"),
            json!("1.5"),
            json!("null"),
            json!(true),
            json!([]),
            json!({}),
            json!("9223372036854775808"),
        ] {
            let mut malformed = finite.clone();
            malformed[field] = value;
            rejected(
                sqlx::query_scalar::<_, Value>("SELECT app.forward_execution_limits($1)")
                    .bind(malformed)
                    .fetch_one(&pool)
                    .await
                    .unwrap_err(),
                "23514",
                None,
            );
        }
    }
    for malformed in [
        json!(null),
        json!([]),
        json!("limits"),
        json!({"schema_version":1,"experiments":0,"cpu_seconds":"1","wall_seconds":null,"memory_mib":null,"output_bytes":null}),
        json!({"schema_version":1,"experiments":0,"cpu_seconds":null,"wall_seconds":null,"memory_mib":null,"output_bytes":null,"extra":1}),
    ] {
        rejected(
            sqlx::query_scalar::<_, Value>("SELECT app.forward_execution_limits($1)")
                .bind(malformed)
                .fetch_one(&pool)
                .await
                .unwrap_err(),
            "23514",
            None,
        );
    }
}

// Isolate the actual migrated column's CHECK from task binding. LIKE copies the
// production checks and nullability but not FKs/triggers; this is deliberately a
// representation probe, separate from the real admission/launch cases above.
async fn cpu_constraint(pool: &PgPool, row: &Value, cpu: Value, expected: Option<&str>) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql("CREATE TEMP TABLE qz_cpu_probe (LIKE app.run_native_tasks INCLUDING CONSTRAINTS) ON COMMIT DROP").execute(&mut *tx).await.unwrap();
    let mut record = row.clone();
    record["cpu"] = cpu.clone();
    let result = sqlx::query(
        "INSERT INTO qz_cpu_probe SELECT (jsonb_populate_record(NULL::app.run_native_tasks,$1)).*",
    )
    .bind(record)
    .execute(&mut *tx)
    .await;
    if let Some(code) = expected {
        rejected(result.unwrap_err(), code, None);
    } else {
        assert_eq!(result.unwrap().rows_affected(), 1);
        let stored: Option<i64> = sqlx::query_scalar("SELECT cpu FROM qz_cpu_probe")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(serde_json::to_value(stored).unwrap(), cpu);
    }
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn upgrade_080005_cpu_column_keeps_positive_u32_bounds_and_old_rows(pool: PgPool) {
    forward_support::support::migrate_before(&pool, CUTOVER).await;
    let f =
        forward_support::setup_with_candidate_limits(&pool, limits(None, None, Some(512), None))
            .await;
    let historical = historical_forward(&pool, &f).await;
    let row: Value =
        sqlx::query_scalar("SELECT to_jsonb(t) FROM app.run_native_tasks t WHERE run_id=$1")
            .bind(historical.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    cpu_constraint(&pool, &row, Value::Null, Some("23502")).await;
    for value in [json!(1), json!(u32::MAX)] {
        cpu_constraint(&pool, &row, value, None).await;
    }
    for value in [json!(0), json!(-1), json!(u64::from(u32::MAX) + 1)] {
        cpu_constraint(&pool, &row, value, Some("23514")).await;
    }
    let pool = upgrade_after_shutdown(pool).await;
    let unchanged: Value =
        sqlx::query_scalar("SELECT to_jsonb(t) FROM app.run_native_tasks t WHERE run_id=$1")
            .bind(historical.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, unchanged);
    for value in [Value::Null, json!(1), json!(u32::MAX)] {
        cpu_constraint(&pool, &row, value, None).await;
    }
    for value in [json!(0), json!(-1), json!(u64::from(u32::MAX) + 1)] {
        cpu_constraint(&pool, &row, value, Some("23514")).await;
    }
}
