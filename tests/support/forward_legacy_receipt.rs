//! Declared relational fixture for the pre-104 Forward producer. This never
//! calls the current enqueue implementation or modifies historical guards.
//! The fixed resource tuple and one-core/two-slot choices are taken verbatim
//! from migration 068 and the corresponding original admission producer.
use super::forward_support::ForwardFixture;
use contracts::{control::CommandResult, execution::NativeTaskParametersV1, forward::*, research::ArtifactInputRole, runtime_jobs::RuntimeInputV1, runs::RunSnapshotV1, DbCounter, Id, SchemaV1};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

pub async fn insert(pool: &PgPool, f: &ForwardFixture) -> CommandResult<RunSnapshotV1> {
    let version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success").fetch_one(pool).await.unwrap();
    assert_eq!(version, 202610060103, "historical fixture must run under the actual original schema");
    let window = f.store.forward_window(&f.operator, f.handoff, &ForwardWindowQueryV1 { stream_id: "daily".into() }, |id, size| {
        let bytes = f.objects.lock().unwrap().get(&id).cloned().unwrap();
        assert_eq!(bytes.len() as u64, size.get());
        async move { Ok(bytes) }
    }).await.unwrap();
    // This fixture has one original message; assert membership before building
    // exactly the native request the historical producer persisted.
    let sources: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.forward_messages WHERE handoff_id=$1 AND stream_id='daily' ORDER BY sequence,message_revision")
        .bind(f.handoff.as_uuid()).fetch_all(pool).await.unwrap();
    assert_eq!(sources, vec![f.original.id.as_uuid()]);
    let end = window.window_end.unwrap();
    let request = NativeForwardRequestV1 { window, sources: vec![f.original.clone()] };
    domain::forward::evaluation::request(&request).unwrap();
    let parameters = NativeTaskParametersV1::EvaluateForward { schema_version: SchemaV1, request: Box::new(request.clone()) };
    let output_schemas = parameters.output_schemas();
    let parameter = Id::new();
    let bytes = serde_json::to_vec(&parameters).unwrap();
    let size = i64::try_from(bytes.len()).unwrap();
    let input = Id::new();
    let run = Id::new();
    let fixed = json!({"schema_version":1,"experiments":0,"cpu_seconds":"30","wall_seconds":60,"memory_mib":512,"output_bytes":"1048576"});
    let mut tx = pool.begin().await.unwrap();
    let observed: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&mut *tx).await.unwrap();
    let policy_until = f.policy.content.valid_until;
    let until = i64::try_from(f.policy.content.max_feedback_age_seconds.get())
        .ok().and_then(chrono::Duration::try_seconds)
        .and_then(|age| end.checked_add_signed(age))
        .unwrap_or(policy_until).min(policy_until);
    assert!(until > observed);
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','REAL','OPERATOR','AUDIT')")
        .bind(parameter.as_uuid()).bind(f.f.project.as_uuid()).bind(parameter.to_string()).bind(size).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,'FORWARD',$3)")
        .bind(input.as_uuid()).bind(f.f.project.as_uuid()).bind(end).execute(&mut *tx).await.unwrap();
    let mut bindings = Vec::new();
    for (ordinal, source) in request.sources.iter().enumerate() {
        sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) VALUES($1,$2,'REPORT',$3)")
            .bind(input.as_uuid()).bind(source.report_artifact_id.as_uuid()).bind(i32::try_from(ordinal).unwrap()).execute(&mut *tx).await.unwrap();
        let artifact = sqlx::query("SELECT storage_version,byte_count FROM app.artifacts WHERE id=$1").bind(source.report_artifact_id.as_uuid()).fetch_one(&mut *tx).await.unwrap();
        bindings.push(RuntimeInputV1::Artifact { artifact_id: source.report_artifact_id, storage_version: artifact.get("storage_version"), byte_count: DbCounter::new(u64::try_from(artifact.get::<i64,_>("byte_count")).unwrap()).unwrap(), role: ArtifactInputRole::Report });
    }
    sqlx::query("UPDATE app.input_sets SET frozen_at=clock_timestamp() WHERE id=$1").bind(input.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.forward_evaluation_inputs(input_set_id,project_id,handoff_id,policy_id,runtime_id,parameters_artifact_id,request,valid_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(input.as_uuid()).bind(f.f.project.as_uuid()).bind(f.handoff.as_uuid()).bind(f.policy.id.as_uuid()).bind(f.runtime.as_uuid()).bind(parameter.as_uuid()).bind(serde_json::to_value(&parameters).unwrap()).bind(until).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at) VALUES($1,$2,NULL,'FORWARD_EVALUATE',$3,'QUEUED',$4+interval '60 seconds',$4)")
        .bind(run.as_uuid()).bind(f.f.project.as_uuid()).bind(input.as_uuid()).bind(observed).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_events(run_id,seq,attempt_id,event_type,schema_version,payload,occurred_at) VALUES($1,1,NULL,'run.created',1,'{\"schema_version\":1,\"state\":\"QUEUED\",\"reason\":\"ADMITTED\"}',clock_timestamp())")
        .bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    let snapshot: Value = sqlx::query_scalar("SELECT jsonb_build_object('schema_version',1,'id',id,'project_id',project_id,'cycle_id',cycle_id,'kind',kind,'input_set_id',input_set_id,'state',state,'current_attempt_no',current_attempt_no,'active_attempt_id',active_attempt_id,'last_event_seq',last_event_seq::text,'deadline_at',deadline_at,'cancellation_requested_at',cancellation_requested_at,'terminal_reason_code',terminal_reason_code,'queued_at',queued_at,'started_at',started_at,'finished_at',finished_at,'revision',revision::text) FROM app.runs WHERE id=$1")
        .bind(run.as_uuid()).fetch_one(&mut *tx).await.unwrap();
    let resource: RunSnapshotV1 = serde_json::from_value(snapshot.clone()).unwrap();
    let queue: i64 = sqlx::query_scalar("SELECT pgmq.send('runs',$1)").bind(json!({"schema_version":1,"run_id":run})).fetch_one(&mut *tx).await.unwrap();
    let runtime = sqlx::query("SELECT revision,last_capability_snapshot_artifact_id,jsonb_build_object('schema_version',1,'endpoint',endpoint,'credential_ref',credential_ref,'tls_policy',tls_policy,'ca_certificate_ref',ca_certificate_ref,'development_http',development_http,'protocol_version',protocol_version,'allowed_capabilities',allowed_capabilities) AS snapshot FROM app.runtime_integrations WHERE id=$1")
        .bind(f.runtime.as_uuid()).fetch_one(&mut *tx).await.unwrap();
    let revision: i64 = runtime.get("revision");
    let normalized = json!({"schema_version":1,"project_id":f.f.project,"input_set_id":input,"runtime_id":f.runtime,"runtime_revision":revision.to_string(),"kind":"FORWARD_EVALUATE","limits":fixed,"max_parallel_runs":2});
    sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) VALUES($1,$2,NULL,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(run.as_uuid()).bind(f.f.project.as_uuid()).bind(format!("forward/{input}")).bind(normalized).bind(snapshot).bind(fixed).bind(f.runtime.as_uuid()).bind(revision).bind(runtime.get::<Value,_>("snapshot")).bind(queue).execute(&mut *tx).await.unwrap();
    bindings.push(RuntimeInputV1::Artifact { artifact_id: parameter, storage_version: "1".into(), byte_count: DbCounter::new(u64::try_from(size).unwrap()).unwrap(), role: ArtifactInputRole::Parameters });
    let image = &f.caps.image_refs.iter().find(|image| image.job_kind == contracts::runs::RunKind::ForwardEvaluate).unwrap().image_ref;
    sqlx::query("INSERT INTO app.run_native_tasks(run_id,parameters_artifact_id,input_bindings,image_ref,cpu,capability_snapshot_artifact_id,output_schemas,origin,access_class) VALUES($1,$2,$3,$4,1,$5,$6,'REAL','EVALUATOR_ONLY')")
        .bind(run.as_uuid()).bind(parameter.as_uuid()).bind(serde_json::to_value(bindings).unwrap()).bind(image).bind(runtime.get::<uuid::Uuid,_>("last_capability_snapshot_artifact_id")).bind(serde_json::to_value(output_schemas).unwrap()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    f.objects.lock().unwrap().insert(parameter, bytes);
    CommandResult { schema_version: SchemaV1, replayed: false, resource }
}
