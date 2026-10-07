//! Real claim/Worker cleanup entry with PostgreSQL. No native model process is configured.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/expired_mission_resource.rs"]
mod expired;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, sync::Arc};

fn cleanup_worker(store: &store::Store) -> (server::worker::Worker, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    integrations::secrets::SecretVault::initialize_key(&key).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(root.path().join("secrets"))
        .unwrap();
    let workspace = root.path().join("workspaces");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&workspace)
        .unwrap();
    // An empty deployment cannot launch or probe any model; only cleanup works.
    let launcher = server::worker::mission::MissionLauncher::new(
        server::codex_profiles::CodexDeployment::default(),
        workspace,
        env!("CARGO_BIN_EXE_server").into(),
        "http://localhost:19873".into(),
        true,
    )
    .unwrap();
    let worker = server::worker::Worker::new(
        store.clone(),
        integrations::secrets::SecretVault::open(&root.path().join("secrets"), &key).unwrap(),
        integrations::artifacts::ArtifactStore::open(&root.path().join("objects")).unwrap(),
        server::runtime_transport::RuntimeTargets::new(Vec::new(), false).unwrap(),
        1,
    )
    .unwrap()
    .with_missions(Arc::new(launcher));
    (worker, root)
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_unsent_reserved_resource_enters_worker_cleanup_before_terminal(pool: PgPool) {
    let f = expired::setup(&pool, false).await;
    let (worker, _root) = cleanup_worker(&f.store);
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    worker
        .process_mission_message(
            f.message.clone(),
            "cleanup-after-expired-bootstrap",
            shutdown,
        )
        .await
        .unwrap();
    expired::assert_no_model_or_new_resource(&pool, f.lease.run.id).await;
    let facts: (String, bool, i64) = sqlx::query_as(
        "SELECT r.state,resource.closed,(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2) FROM app.runs r JOIN app.mission_resources resource ON resource.run_id=r.id WHERE r.id=$1"
    ).bind(f.lease.run.id.as_uuid()).bind(f.message.message_id).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, ("FAILED".into(), true, 1));
}

#[cfg(feature = "native-codex")]
#[sqlx::test(migrations = "../../migrations")]
async fn expired_launched_worker_revokes_a_sigstopped_late_native_service_before_terminal(
    pool: PgPool,
) {
    use std::{process::Stdio, time::Duration};
    use tokio::process::Command;
    let f = expired::setup(&pool, true).await;
    let unit = format!("{}.service", f.resource.name());
    let mut child = Command::new("/usr/bin/bash")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "kill -STOP $$; exec \"$@\"",
            "qz-late-worker-fixture",
            "/usr/bin/systemd-run",
            "--user",
            "--quiet",
            "--pipe",
            "--wait",
            "--collect",
            "--service-type=exec",
            "--expand-environment=no",
        ])
        .arg(format!("--unit={unit}"))
        .args(["--property=RuntimeMaxSec=20s", "/usr/bin/sleep", "20"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let value = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
            if value
                .lines()
                .any(|line| line.starts_with("State:") && line.contains('T'))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (worker, _root) = cleanup_worker(&f.store);
    let (_alive, shutdown) = tokio::sync::watch::channel(false);
    let recovered = worker
        .process_mission_message(f.message.clone(), "takeover-before-late-service", shutdown)
        .await;
    let resumed = Command::new("/usr/bin/kill")
        .args(["-CONT", &pid.to_string()])
        .status()
        .await
        .unwrap();
    assert!(resumed.success());
    let late = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    let observed = Command::new("/usr/bin/systemctl")
        .args([
            "--user",
            "show",
            "--property=LoadState,ActiveState,MainPID,Job,ControlGroup",
            "--",
            &unit,
        ])
        .output()
        .await
        .unwrap();
    let observed = String::from_utf8(observed.stdout).unwrap();
    // Remove only this test's persistent random-name tombstone after the owned
    // late launcher has exited. Production never unmarks a revoked identity.
    assert!(Command::new("/usr/bin/systemctl")
        .args(["--user", "unmask", "--", &unit])
        .status()
        .await
        .unwrap()
        .success());
    recovered.unwrap();
    assert!(
        !late.success(),
        "old launcher must be refused by native service manager"
    );
    assert!(observed.lines().any(|line| line == "LoadState=masked"));
    assert!(observed.lines().any(|line| line == "MainPID=0"));
    assert!(observed
        .lines()
        .any(|line| line == "Job=" || line == "Job=0"));
    assert!(observed.lines().any(|line| line == "ControlGroup="));
    expired::assert_no_model_or_new_resource(&pool, f.lease.run.id).await;
    let facts: (String, bool, Option<i64>) = sqlx::query_as(
        "SELECT r.state,m.closed,m.cpu_nanoseconds FROM app.runs r JOIN app.mission_resources m ON m.run_id=r.id WHERE r.id=$1"
    ).bind(f.lease.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(facts, ("FAILED".into(), true, None));
}
