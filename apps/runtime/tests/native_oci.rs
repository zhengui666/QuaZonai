//! Mandatory native Docker acceptance. This target is selected explicitly by the
//! native-runtime CI; no test is ignored and missing native prerequisites are failures.
#[path = "support/oci.rs"]
mod support;
use bollard::{
    errors::Error as DockerError,
    models::ContainerCreateBody,
    query_parameters::{CreateContainerOptionsBuilder, RemoveContainerOptionsBuilder},
};
use contracts::{runtime_jobs::*, Id, SchemaV1};
use reqwest::{Method, StatusCode};
use runtime::engine::{NativeEngine, NativeImage};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, time::Duration};
use support::{count, docker, Fixture, SIGNAL, SLOW_SIGNAL};

#[tokio::test]
async fn real_native_compile_publishes_exact_model_and_concurrent_retry_has_one_container() {
    let mut f = Fixture::open().await;
    let spec = f.compile(SIGNAL, 30).await;
    let (first, replay) = tokio::join!(f.submit(&spec), f.submit(&spec));
    assert_eq!(first.run_id, replay.run_id);
    assert_eq!(first.attempt_no, replay.attempt_no);
    assert_eq!(first.submitted_at, replay.submitted_at);
    assert_eq!(first.external_job_id, spec.external_job_id);
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Succeeded);
    let manifest = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&manifest, &spec, first.submitted_at, runtime::now()).unwrap();
    assert_eq!(manifest.engine_versions["rustc"], "1.98.1");
    assert!(manifest.resource_usage.cpu_nanoseconds.is_none());
    assert!(manifest.resource_usage.peak_memory_bytes.is_none());
    let model = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == RuntimeOutputKind::Model)
        .unwrap();
    let bytes = f
        .client
        .get(f.url(&[
            "jobs",
            &spec.external_job_id,
            "artifacts",
            &model.storage_ref.to_string(),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(bytes.status(), StatusCode::OK);
    assert_eq!(bytes.headers()["content-type"], "application/wasm");
    let bytes = bytes.bytes().await.unwrap();
    assert!(bytes.starts_with(b"\0asm\x01\0\0\0"));
    assert_eq!(bytes.len() as u64, model.byte_count.get());
    let native = f.native_container(&spec).await;
    let native_id = native.id.unwrap();
    assert_eq!(native.state.unwrap().exit_code, Some(0));
    let repeated = f.submit(&spec).await;
    assert_eq!(repeated, terminal);
    assert_eq!(
        f.native_container(&spec).await.id.as_deref(),
        Some(native_id.as_str())
    );
    let mut conflict = spec.clone();
    conflict.limits.memory_mib += 1;
    let response = f
        .client
        .post(f.url(&["jobs"]))
        .json(&conflict)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    f.assert_private_logs();
}

#[tokio::test]
async fn completed_job_and_exact_bytes_survive_abrupt_gateway_restart_without_reexecution() {
    let mut f = Fixture::open().await;
    let spec = f.compile(SIGNAL, 30).await;
    f.submit(&spec).await;
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Succeeded);
    let before = f.manifest(&spec).await;
    let native_before = f.native_container(&spec).await;
    f.crash();
    f.restart().await;
    assert_eq!(f.submit(&spec).await, terminal);
    let after = f.manifest(&spec).await;
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    let native_after = f.native_container(&spec).await;
    assert_eq!(native_after.id, native_before.id);
    assert_eq!(
        native_after.state.as_ref().unwrap().started_at,
        native_before.state.as_ref().unwrap().started_at
    );
    assert_eq!(native_after.restart_count, Some(0));
    f.assert_private_logs();
}

#[tokio::test]
async fn native_wall_deadline_stops_the_process_while_gateway_is_dead_then_reconciles_same_identity(
) {
    let mut f = Fixture::open().await;
    let spec = f.compile(SLOW_SIGNAL, 4).await;
    let submitted = f.submit(&spec).await;
    let docker = docker().await;
    let original = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let container = f.native_container(&spec).await;
            if container.state.as_ref().unwrap().running == Some(true) {
                return container;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("native compiler must actually start before gateway crash");
    let id = original.id.clone().unwrap();
    f.crash();
    let stopped = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let current = docker.inspect_container(&id, None).await.unwrap();
            if current.state.as_ref().unwrap().running == Some(false) {
                return current;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("native deadline must not depend on a live gateway");
    let stopped_state = stopped.state.as_ref().unwrap();
    assert!(matches!(stopped_state.exit_code, Some(124 | 137)));
    assert_eq!(stopped_state.oom_killed, Some(false));
    let started =
        chrono::DateTime::parse_from_rfc3339(stopped_state.started_at.as_deref().unwrap()).unwrap();
    let finished =
        chrono::DateTime::parse_from_rfc3339(stopped_state.finished_at.as_deref().unwrap())
            .unwrap();
    let elapsed = (finished - started).num_milliseconds();
    assert!(
        (3_750..=7_000).contains(&elapsed),
        "the native deadline, not an unrelated startup error, must stop this job"
    );
    f.restart().await;
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Failed);
    let result = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&result, &spec, submitted.submitted_at, runtime::now()).unwrap();
    assert!(result.artifacts.is_empty());
    assert_eq!(
        f.native_container(&spec).await.id.as_deref(),
        Some(id.as_str())
    );
    assert_eq!(f.submit(&spec).await, terminal);
    f.assert_private_logs();
}

#[tokio::test]
async fn cancelled_native_identity_blocks_both_late_create_and_old_id_start() {
    let mut f = Fixture::open().await;
    let spec = f.compile(SLOW_SIGNAL, 60).await;
    f.submit(&spec).await;
    let original = f.native_container(&spec).await;
    let id = original.id.clone().unwrap();
    let name = original
        .name
        .clone()
        .unwrap()
        .trim_start_matches('/')
        .to_owned();
    let request = RuntimeCancelV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: 1,
        owner_epoch: spec.owner_epoch.next().unwrap(),
    };
    let pending: RuntimeJobStatusV1 = f
        .json(
            Method::POST,
            &["jobs", &spec.external_job_id, "cancel"],
            Some(serde_json::to_value(request).unwrap()),
            &[StatusCode::OK, StatusCode::ACCEPTED],
        )
        .await;
    assert!(matches!(
        pending.state,
        RuntimeJobState::CancelRequested | RuntimeJobState::Cancelled
    ));
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Cancelled);
    assert_eq!(f.submit(&spec).await, terminal);
    let barrier = f.native_container(&spec).await;
    assert_ne!(barrier.id.as_deref(), Some(id.as_str()));
    assert_eq!(
        barrier.config.as_ref().unwrap().labels.as_ref().unwrap()["io.quazonai.role"],
        "TOMBSTONE"
    );
    assert_eq!(barrier.state.as_ref().unwrap().running, Some(false));
    let docker = docker().await;
    assert!(matches!(
        docker.start_container(&id, None).await,
        Err(DockerError::DockerResponseServerError {
            status_code: 404,
            ..
        })
    ));
    let options = CreateContainerOptionsBuilder::default().name(&name).build();
    assert!(matches!(
        docker
            .create_container(
                Some(options),
                ContainerCreateBody {
                    image: Some(support::image()),
                    ..Default::default()
                }
            )
            .await,
        Err(DockerError::DockerResponseServerError {
            status_code: 409,
            ..
        })
    ));
    let manifest = f.manifest(&spec).await;
    assert_eq!(manifest.state, RuntimeResultState::Cancelled);
    assert!(manifest.artifacts.is_empty());
    assert_eq!(manifest.resource_usage.output_bytes.get(), 0);
    f.assert_private_logs();
}

#[tokio::test]
async fn compiler_cannot_read_a_test_owned_host_secret_or_runtime_credential_namespace() {
    let mut f = Fixture::open().await;
    let sentinel = f.directory.path().join("not-mounted-native-sentinel");
    fs::write(&sentinel, "fixture-host-secret-never-shared").unwrap();
    let code = format!(
        "{}\nconst _: &str = include_str!({:?});\n",
        SIGNAL, sentinel
    );
    let spec = f.compile(&code, 30).await;
    f.submit(&spec).await;
    assert_eq!(f.terminal(&spec).await.state, RuntimeJobState::Failed);
    let manifest = f.manifest(&spec).await;
    assert!(manifest.artifacts.is_empty());
    assert_eq!(
        manifest.error.unwrap().code,
        RuntimeFailureCode::NativeJobFailed
    );
    let response = f
        .client
        .get(f.url(&["objects", &spec.parameters_artifact_id.to_string()]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    f.assert_private_logs();
}

/// Exercise the exact production-generated HostConfig in an actual native container.
/// Only this CI helper changes the fixed process entrypoint to its probe. Neither
/// HTTP nor JobSpec has an entrypoint/command override, verified separately above.
async fn isolated_probe(
    mode: &str,
) -> (bollard::models::ContainerInspectResponse, tempfile::TempDir) {
    let mut fixture = Fixture::open().await;
    let mut spec = fixture.compile(SIGNAL, 10).await;
    spec.limits.memory_mib = 64;
    spec.limits.output_bytes = count(1024 * 1024);
    fixture.crash();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input");
    let output = directory.path().join("output");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    fs::set_permissions(&input, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&output, fs::Permissions::from_mode(0o1777)).unwrap();
    fs::write(input.join("fixture.txt"), b"native read-only input").unwrap();
    fs::set_permissions(input.join("fixture.txt"), fs::Permissions::from_mode(0o444)).unwrap();
    let sentinel = directory.path().join("host-only-sentinel");
    fs::write(&sentinel, b"test owned secret").unwrap();
    let native_image = NativeImage {
        id: support::image(),
        versions: BTreeMap::from([("native-ci-probe".into(), "1".into())]),
    };
    let mut body = NativeEngine::launch(
        Id::new(),
        &spec,
        &native_image,
        vec![
            runtime::engine::bind(&input, "/input", true).unwrap(),
            runtime::engine::bind(&output, "/output", false).unwrap(),
        ],
        false,
    )
    .unwrap();
    body.entrypoint = Some(vec!["/usr/local/bin/isolation-probe".into()]);
    body.cmd = Some(vec![mode.into(), sentinel.to_str().unwrap().into()]);
    let docker = docker().await;
    let options = CreateContainerOptionsBuilder::default()
        .name(&format!("native-boundary-probe-{}", spec.run_id))
        .build();
    let created = docker.create_container(Some(options), body).await.unwrap();
    docker.start_container(&created.id, None).await.unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let current = docker.inspect_container(&created.id, None).await.unwrap();
            if current.state.as_ref().unwrap().running == Some(false) {
                return current;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    let cleanup = RemoveContainerOptionsBuilder::default()
        .force(true)
        .v(true)
        .build();
    docker
        .remove_container(&created.id, Some(cleanup))
        .await
        .unwrap();
    (outcome.expect("bounded native isolation probe"), directory)
}

#[tokio::test]
async fn actual_native_namespaces_forbid_secret_socket_network_root_input_writes_and_tmp_execution()
{
    let (outcome, directory) = isolated_probe("inspect").await;
    assert_eq!(outcome.state.unwrap().exit_code, Some(0));
    assert_eq!(
        fs::read(directory.path().join("output/verified")).unwrap(),
        b"native namespace and resource controls verified"
    );
}

#[tokio::test]
async fn actual_native_memory_pids_and_file_size_limits_are_enforced_by_the_kernel() {
    let (memory, _) = isolated_probe("memory").await;
    let state = memory.state.unwrap();
    assert_eq!(state.oom_killed, Some(true));
    assert_ne!(state.exit_code, Some(0));
    let (pids, directory) = isolated_probe("pids").await;
    assert_eq!(pids.state.unwrap().exit_code, Some(0));
    let children: u32 = fs::read_to_string(directory.path().join("output/pids-verified"))
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..64).contains(&children));
    let (files, directory) = isolated_probe("output").await;
    let exit = files.state.unwrap().exit_code;
    assert!(exit.is_some() && exit != Some(0) && exit != Some(99));
    assert!(
        fs::metadata(directory.path().join("output/bounded-file"))
            .unwrap()
            .len()
            <= 1024 * 1024
    );
}
