//! Actual Worker scheduling/object IO/PGMQ/publication with controlled historical inputs.
//! These relationship fixtures do not prove production Claim or real market/OCI execution.
#[path = "../../../tests/support/forward.rs"]
#[allow(dead_code)]
mod forward_support;
use contracts::{lifecycle::RunCancelV1, SchemaV1};
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn worker_schedules_original_feedback_once_and_publishes_cancelled_terminal(pool: PgPool) {
    use std::{fs, io::Write, process::Command};
    const CHILD: &str = "QZ_FORWARD_FULL_FILESYSTEM_TEST";
    let Some(root) = std::env::var_os(CHILD) else {
        pool.close().await;
        let root = tempfile::tempdir().unwrap();
        let output = Command::new(std::env::var_os("QZ_TEST_UNSHARE").unwrap_or_else(|| "unshare".into()))
            .args(["--user", "--map-root-user", "--mount", "--", "sh", "-eu", "-c",
                "mount -t tmpfs -o size=16m,mode=0700 tmpfs \"$1\"; export TMPDIR=\"$1\"; exec \"$2\" --exact worker_schedules_original_feedback_once_and_publishes_cancelled_terminal --nocapture",
                "forward-full-filesystem"])
            .arg(root.path()).arg(std::env::current_exe().unwrap()).env(CHILD, root.path())
            .output().expect("native user and mount namespaces must be available");
        assert!(
            output.status.success(),
            "isolated Forward ENOSPC test failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("STORAGE_FULL"));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        return;
    };
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .without_time()
        .with_ansi(false)
        .init();
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
    let tables = [
        "app.runs",
        "app.artifacts",
        "app.input_sets",
        "app.forward_evaluation_inputs",
        "pgmq.q_runs",
        "app.evaluations",
        "app.handoff_offers",
    ];
    let mut before = Vec::new();
    for table in tables {
        before.push(
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT count(*) FROM {table}"
            )))
            .fetch_one(&pool)
            .await
            .unwrap(),
        );
    }
    let filler_path = std::path::PathBuf::from(root).join("filler");
    let mut filler = fs::File::create(&filler_path).unwrap();
    let mut full = false;
    for _ in 0..512 {
        match filler.write_all(&[0; 64 * 1024]) {
            Ok(()) => (),
            Err(error) => {
                assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
                full = true;
                break;
            }
        }
    }
    assert!(full, "private 16 MiB tmpfs must exhaust before 32 MiB");
    let (cursor, rejected) = worker.process_automation(None).await;
    assert_eq!(cursor, Some(f.f.project));
    assert!(rejected.is_err());
    for (table, before) in tables.into_iter().zip(before) {
        let after: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, before, "failed native publication changed {table}");
    }
    let retained =
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap();
    {
        let original = f.objects.lock().unwrap();
        assert_eq!(
            fs::read_dir(directory.path().join("objects"))
                .unwrap()
                .count(),
            original.len()
        );
        for (id, bytes) in original.iter() {
            assert_eq!(
                retained
                    .read(*id, forward_support::count(bytes.len() as u64))
                    .unwrap(),
                *bytes
            );
        }
    }

    drop(filler);
    fs::remove_file(filler_path).unwrap();
    // The failed attempt retains the ordinary retry interval, not an admitted Run.
    assert!(f
        .store
        .prepare_forward_evaluation(f.f.project)
        .await
        .unwrap()
        .is_none());
    let wait_ms: i64 = sqlx::query_scalar("SELECT ceil(extract(epoch FROM greatest(next_attempt_at-clock_timestamp(),interval '0'))*1000)::bigint FROM app.forward_schedule WHERE handoff_id=$1")
        .bind(f.handoff.as_uuid()).fetch_one(&pool).await.unwrap();
    assert!((0..=30_000).contains(&wait_ms));
    tokio::time::sleep(std::time::Duration::from_millis(wait_ms as u64 + 50)).await;
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
