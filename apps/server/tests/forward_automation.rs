//! Actual Worker scheduling/object IO/PGMQ/publication with controlled historical inputs.
//! These relationship fixtures do not prove production Claim or real market/OCI execution.
#[path = "../../../tests/support/forward.rs"]
#[allow(dead_code)]
mod forward_support;
use contracts::{lifecycle::RunCancelV1, SchemaV1};
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn worker_schedules_original_feedback_once_and_publishes_cancelled_terminal(pool: PgPool) {
    let f = forward_support::setup(&pool).await;
    let directory = tempfile::tempdir().unwrap();
    let objects =
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap();
    for (id, bytes) in f.objects.lock().unwrap().iter() {
        objects.put(*id, bytes).unwrap();
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir(directory.path().join("secrets")).unwrap();
    std::fs::set_permissions(
        directory.path().join("secrets"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    integrations::secrets::SecretVault::initialize_key(&directory.path().join("master.key"))
        .unwrap();
    let worker = server::worker::Worker::new(
        f.store.clone(),
        integrations::secrets::SecretVault::open(
            &directory.path().join("secrets"),
            &directory.path().join("master.key"),
        )
        .unwrap(),
        objects,
        server::runtime_transport::RuntimeTargets::new(Vec::new(), false).unwrap(),
        1,
    )
    .unwrap();
    // Paper eligibility may fail on the explicit relational candidate; feedback must still run.
    let (a, b) = tokio::join!(
        worker.process_automation(None),
        worker.process_automation(None)
    );
    assert_eq!(a.0, Some(f.f.project));
    assert_eq!(b.0, Some(f.f.project));
    let runs: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM app.runs WHERE project_id=$1 AND kind='FORWARD_EVALUATE'",
    )
    .bind(f.f.project.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(runs.len(), 1);
    let id: contracts::Id = runs[0].to_string().try_into().unwrap();
    let run = f.store.get_run(&f.operator, id).await.unwrap();
    let objects =
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap();
    let parameter:(uuid::Uuid,i64)=sqlx::query_as("SELECT a.id,a.byte_count FROM app.run_native_tasks t JOIN app.artifacts a ON a.id=t.parameters_artifact_id WHERE t.run_id=$1").bind(id.as_uuid()).fetch_one(&pool).await.unwrap();
    let bytes = objects
        .read(
            parameter.0.to_string().try_into().unwrap(),
            forward_support::count(parameter.1 as u64),
        )
        .unwrap();
    let task: contracts::execution::NativeTaskParametersV1 =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(task.job_kind(), contracts::runs::RunKind::ForwardEvaluate);
    f.store
        .cancel_run(
            &f.operator,
            "cancel-worker-feedback",
            id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    let messages = f.store.read_native_run_messages(60, 100).await.unwrap();
    let message = messages.into_iter().find(|m| m.run_id == id).unwrap();
    assert!(f
        .store
        .claim_mission(&message, "wrong-driver", 60)
        .await
        .unwrap()
        .is_none());
    let (_stop, shutdown) = tokio::sync::watch::channel(false);
    sqlx::query("CREATE FUNCTION app.fail_forward_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled lost ACK'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_forward_ack BEFORE INSERT ON pgmq.a_runs FOR EACH ROW EXECUTE FUNCTION app.fail_forward_ack()").execute(&pool).await.unwrap();
    assert!(worker
        .process_message(message.clone(), "forward-worker", shutdown.clone())
        .await
        .is_err());
    let before_retry:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.evaluations WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$2)").bind(id.as_uuid()).bind(id.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(before_retry, (1, 1));
    sqlx::query("DROP TRIGGER fail_forward_ack ON pgmq.a_runs")
        .execute(&pool)
        .await
        .unwrap();
    // A lost ACK retries the still-queued original terminal message, not an archived one.
    worker
        .process_message(message, "forward-worker-retry", shutdown)
        .await
        .unwrap();
    let facts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.evaluations WHERE run_id=$1 AND execution_status='CANCELLED' AND evidence_status='INCOMPLETE' AND decision='INCONCLUSIVE'),(SELECT count(*) FROM app.forward_evidence_windows w JOIN app.evaluations e ON e.id=w.evaluation_id WHERE e.run_id=$1 AND NOT w.is_contiguous AND w.complete_observations=0),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$2),(SELECT count(*) FROM pgmq.a_runs WHERE message->>'run_id'=$2)").bind(id.as_uuid()).bind(id.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, (1, 1, 0, 1));
    let observations:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.forward_observation_publications p JOIN app.degradation_observations o ON o.id=p.observation_id WHERE p.run_id=$1 AND o.classification='INSUFFICIENT_DATA'),(SELECT count(*) FROM app.wake_events w JOIN app.degradation_observations o ON o.id=w.observation_id JOIN app.evaluations e ON e.id=o.evaluation_id WHERE e.run_id=$1)").bind(id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(observations, (1, 0));
    let (cursor, _paper_or_feedback_result) = worker.process_automation(None).await;
    assert_eq!(cursor, Some(f.f.project));
    assert!(f
        .store
        .prepare_forward_evaluation(f.f.project)
        .await
        .unwrap()
        .is_none());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.runs WHERE project_id=$1 AND kind='FORWARD_EVALUATE'",
    )
    .bind(f.f.project.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}
