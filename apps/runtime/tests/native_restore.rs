//! Cold restoration of an owned native Runtime checkpoint, not production RPO/RTO.
//! Real Docker and the built job image are mandatory; no account or market fixture is used.
#[path = "support/oci.rs"]
mod support;

use bollard::query_parameters::ListContainersOptionsBuilder;
use contracts::{runtime_jobs::*, Id, SchemaV1};
use reqwest::{header, Method, StatusCode};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};
use support::{docker, Fixture, SIGNAL};

async fn tar(mode: &str, archive: &Path, directory: &Path) {
    let mut command = tokio::process::Command::new("tar");
    command
        .env_clear()
        .args([mode, "--file"])
        .arg(archive)
        .arg("--directory")
        .arg(directory)
        .kill_on_drop(true);
    if mode == "--create" {
        command.arg(".");
    } else if mode == "--extract" {
        command.arg("--preserve-permissions");
    }
    let result = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("native archive command deadline")
        .expect("native GNU tar must be installed");
    assert!(
        result.status.success() && result.stdout.is_empty() && result.stderr.is_empty(),
        "native archive operation failed; archive and diagnostics remain private"
    );
}

async fn output_bytes(
    fixture: &Fixture,
    spec: &JobSpecV1,
    manifest: &ResultManifestV1,
) -> Vec<Vec<u8>> {
    assert!(!manifest.artifacts.is_empty());
    let mut outputs = Vec::new();
    for artifact in &manifest.artifacts {
        let response = fixture
            .client
            .get(fixture.url(&[
                "jobs",
                &spec.external_job_id,
                "artifacts",
                &artifact.storage_ref.to_string(),
            ]))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            artifact.media_type
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment"
        );
        let bytes = response.bytes().await.unwrap();
        assert_eq!(bytes.len() as u64, artifact.byte_count.get());
        if artifact.kind == RuntimeOutputKind::Model {
            assert!(bytes.starts_with(b"\0asm"));
        }
        outputs.push(bytes.to_vec());
    }
    assert!(manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == RuntimeOutputKind::Model));
    outputs
}

async fn container_ids(run: Id) -> Vec<String> {
    let filters = HashMap::from([("label".to_owned(), vec![format!("io.quazonai.run={run}")])]);
    docker()
        .await
        .list_containers(Some(
            ListContainersOptionsBuilder::default()
                .all(true)
                .filters(&filters)
                .build(),
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|container| container.id.unwrap())
        .collect()
}

fn fresh_identity(fixture: &mut Fixture, original: &JobSpecV1) -> JobSpecV1 {
    let mut spec = original.clone();
    spec.run_id = Id::new();
    spec.external_job_id = domain::runtime_jobs::external_id(spec.run_id, spec.attempt_no).unwrap();
    spec.deadline_at =
        runtime::now() + chrono::Duration::seconds(i64::from(spec.limits.wall_seconds) + 20);
    fixture.runs.push(spec.run_id);
    spec
}

async fn cold_round_trip() {
    let mut fixture = Fixture::open().await;
    let original = fixture.compile(SIGNAL, 30).await;
    fixture.submit(&original).await;
    let status = fixture.terminal(&original).await;
    assert_eq!(status.state, RuntimeJobState::Succeeded);
    assert!(status.has_result);
    let manifest = fixture.manifest(&original).await;
    assert_eq!(manifest.state, RuntimeResultState::Succeeded);
    assert!(manifest.error.is_none());
    let bytes = output_bytes(&fixture, &original, &manifest).await;
    let manifest_value = serde_json::to_value(&manifest).unwrap();
    let native = fixture.native_container(&original).await;
    let container_id = native.id.clone().unwrap();
    let native_state = native.state.as_ref().unwrap();
    assert_eq!(native_state.running, Some(false));
    let instance =
        native.config.as_ref().unwrap().labels.as_ref().unwrap()["io.quazonai.runtime"].clone();
    assert_eq!(
        container_ids(original.run_id).await.as_slice(),
        std::slice::from_ref(&container_id)
    );

    // Cancel before any POST for this new identity. The resulting tombstone must
    // survive the archive, even though it has no launch, container or outputs.
    let cancelled = fresh_identity(&mut fixture, &original);
    let cancellation = RuntimeCancelV1 {
        schema_version: SchemaV1,
        run_id: cancelled.run_id,
        attempt_no: cancelled.attempt_no,
        owner_epoch: cancelled.owner_epoch,
    };
    let tombstone: RuntimeJobStatusV1 = fixture
        .json(
            Method::POST,
            &["jobs", &cancelled.external_job_id, "cancel"],
            Some(serde_json::to_value(&cancellation).unwrap()),
            &[StatusCode::OK, StatusCode::ACCEPTED],
        )
        .await;
    assert_eq!(tombstone.state, RuntimeJobState::Cancelled);
    assert!(!tombstone.has_result);
    assert!(container_ids(cancelled.run_id).await.is_empty());

    // All native work is terminal before stopping the sole journal/filesystem
    // writer. Killing only a gateway would not quiesce an active Docker job.
    fixture.crash();
    let state = fixture.config.state_dir.clone();
    let wal_bytes = fs::metadata(state.join("journal.sqlite-wal"))
        .expect("unclean native shutdown must retain its WAL checkpoint")
        .len();
    assert!(
        wal_bytes > 32,
        "exercise actual WAL recovery, not an empty archive"
    );
    let checkpoint = fixture.directory.path().join("checkpoint.tar");
    let saved = fixture.directory.path().join("retained-original-state");
    let before_inode = fs::metadata(&state).unwrap().ino();
    tar("--create", &checkpoint, &state).await;
    tar("--compare", &checkpoint, &state).await;
    let restore_started = Instant::now();
    fs::rename(&state, &saved).unwrap();
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    tar("--extract", &checkpoint, &state).await;
    assert_ne!(fs::metadata(&state).unwrap().ino(), before_inode);
    assert_eq!(fs::metadata(&state).unwrap().mode() & 0o777, 0o700);
    // Comparison precedes reopening SQLite, which may legitimately checkpoint WAL.
    tar("--compare", &checkpoint, &state).await;
    tar("--compare", &checkpoint, &saved).await;
    assert!(!state.join("credential").exists());
    assert!(!state.join("runtime.json").exists());

    fixture.restart().await;
    assert_eq!(fixture.status(&original).await, status);
    let recovered = fixture.manifest(&original).await;
    assert!(serde_json::to_value(&recovered).unwrap() == manifest_value);
    assert!(output_bytes(&fixture, &original, &recovered).await == bytes);
    assert_eq!(fixture.submit(&original).await, status);
    let mut conflicting = original.clone();
    conflicting.input_set_id = Id::new();
    let conflict: serde_json::Value = fixture
        .json(
            Method::POST,
            &["jobs"],
            Some(serde_json::to_value(&conflicting).unwrap()),
            &[StatusCode::CONFLICT],
        )
        .await;
    assert_eq!(conflict["code"], "RUNTIME_IDEMPOTENCY_CONFLICT");
    assert_eq!(fixture.status(&cancelled).await, tombstone);
    assert_eq!(fixture.submit(&cancelled).await, tombstone);
    assert!(container_ids(cancelled.run_id).await.is_empty());
    let recovered_container = fixture.native_container(&original).await;
    assert_eq!(
        recovered_container.id.as_deref(),
        Some(container_id.as_str())
    );
    assert_eq!(recovered_container.restart_count, native.restart_count);
    let recovered_state = recovered_container.state.as_ref().unwrap();
    assert_eq!(recovered_state.running, Some(false));
    assert_eq!(recovered_state.started_at, native_state.started_at);
    assert_eq!(recovered_state.finished_at, native_state.finished_at);
    assert_eq!(container_ids(original.run_id).await, [container_id]);
    let restore_elapsed_ms = restore_started.elapsed().as_millis();

    // Use the restored input and parameter IDs, without a PUT or regenerated
    // artifact. A genuinely new native compile proves the inputs also survived.
    let next = fresh_identity(&mut fixture, &original);
    fixture.submit(&next).await;
    let next_status = fixture.terminal(&next).await;
    assert_eq!(next_status.state, RuntimeJobState::Succeeded);
    let next_manifest = fixture.manifest(&next).await;
    assert_eq!(next_manifest.run_id, next.run_id);
    assert_eq!(next_manifest.state, RuntimeResultState::Succeeded);
    output_bytes(&fixture, &next, &next_manifest).await;
    let next_container = fixture.native_container(&next).await;
    let next_labels = next_container
        .config
        .as_ref()
        .unwrap()
        .labels
        .as_ref()
        .unwrap();
    assert_eq!(next_labels["io.quazonai.runtime"], instance);
    assert_ne!(next_container.id, native.id);
    assert!(container_ids(cancelled.run_id).await.is_empty());
    assert_eq!(fixture.status(&original).await, status);
    assert!(serde_json::to_value(fixture.manifest(&original).await).unwrap() == manifest_value);
    tar("--compare", &checkpoint, &saved).await;
    fixture.assert_private_logs();
    println!(
        "{}",
        serde_json::json!({
            "scope": "isolated same-host terminal Runtime checkpoint; not full T40 or production RPO/RTO",
            "wal_bytes_at_backup": wal_bytes,
            "restored_output_count": bytes.len(),
            "cold_restore_elapsed_ms": restore_elapsed_ms,
            "original_identity_replayed": true,
            "cancelled_identity_stayed_closed": true,
            "new_compile_used_restored_inputs": true
        })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_archive_restores_native_identity_outputs_and_cancellation() {
    tokio::time::timeout(Duration::from_secs(150), cold_round_trip())
        .await
        .expect("native cold-restore acceptance deadline");
}
