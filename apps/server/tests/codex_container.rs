//! Real Docker acceptance, using only fresh homes and containers created here.
//! An unavailable daemon/image is a failure; no account or OAuth is fabricated.
#![cfg(feature = "codex-container")]

use bollard::{
    errors::Error,
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    models::{ContainerCreateBody, ContainerInspectResponse},
    query_parameters::{CreateContainerOptionsBuilder, ListContainersOptionsBuilder},
    Docker, API_DEFAULT_VERSION,
};
use contracts::{lifecycle::JobLimitsV1, DbCounter, Id, SchemaV1};
use futures_util::StreamExt;
use server::codex_native::{
    Client, ContainerBackend, Launch, MissionOptions, MissionProcess, NativeFailure, Observation,
    ThreadOptions, TurnStatus,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    time::Duration,
};

#[path = "support/codex_responses.rs"]
#[allow(dead_code)] // Reuse the upstream fault fixture; the native shell still executes.
mod responses;

struct Fixture {
    root: tempfile::TempDir,
    docker: Docker,
    image: String,
    socket: PathBuf,
}

impl Fixture {
    async fn new() -> Self {
        let image = std::env::var("CODEX_CONTAINER_TEST_IMAGE")
            .expect("codex-container acceptance requires a separately built Codex image");
        let socket = std::env::var_os("CODEX_DOCKER_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/var/run/docker.sock".into());
        let docker = Docker::connect_with_unix(socket.to_str().unwrap(), 10, API_DEFAULT_VERSION)
            .unwrap()
            .negotiate_version()
            .await
            .expect("codex-container acceptance requires access to a real Docker daemon");
        docker.inspect_image(&image).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        for name in ["home", "codex", "workspace"] {
            fs::create_dir(root.path().join(name)).unwrap();
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.path().join("codex.lock"))
            .unwrap();
        Self {
            root,
            docker,
            image,
            socket,
        }
    }

    fn launch(&self) -> Launch {
        Launch {
            // Neither a host Codex executable nor its PATH can serve this test.
            binary: self.root.path().join("host-codex-does-not-exist"),
            home: self.root.path().join("home"),
            codex_home: self.root.path().join("codex"),
            working_directory: self.root.path().join("workspace"),
            executable_path: "/deliberately-absent-host-path".into(),
            native_environment: BTreeMap::new(),
            container: Some(ContainerBackend {
                image: self.image.clone(),
                socket: self.socket.clone(),
                lock_file: self.root.path().join("codex.lock"),
            }),
        }
    }

    async fn session_id(&self) -> String {
        let filters = HashMap::from([(
            "label".to_owned(),
            vec![format!("io.quazonai.codex.image={}", self.image)],
        )]);
        let options = ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();
        let mut ids = Vec::new();
        for container in self.docker.list_containers(Some(options)).await.unwrap() {
            let id = container.id.unwrap();
            let inspect = match self.docker.inspect_container(&id, None).await {
                Ok(inspect) => inspect,
                Err(error) if missing(&error) => continue,
                Err(error) => panic!("cannot inspect test image container: {error}"),
            };
            if inspect
                .mounts
                .unwrap_or_default()
                .iter()
                .any(|mount| mount.source.as_deref() == self.root.path().join("codex").to_str())
            {
                ids.push(id);
            }
        }
        assert_eq!(ids.len(), 1, "one App Server must own the fresh CODEX_HOME");
        ids.pop().unwrap()
    }
}

fn limits(run: Id, remaining: u32) -> MissionProcess {
    MissionProcess::new(
        run,
        JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: DbCounter::new(30).unwrap(),
            wall_seconds: 60,
            memory_mib: 256,
            output_bytes: DbCounter::new(1_048_576).unwrap(),
        },
        remaining,
    )
    .unwrap()
}

fn missing(error: &Error) -> bool {
    matches!(
        error,
        Error::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

async fn wait_removed(docker: &Docker, id: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match docker.inspect_container(id, None).await {
                Err(error) if missing(&error) => break,
                Ok(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                Err(error) => panic!("cannot verify container cleanup: {error}"),
            }
        }
    })
    .await
    .expect("the original container and all its processes must be removed");
}

async fn exec(docker: &Docker, id: &str, script: &str, detached: bool) -> String {
    let command = docker
        .create_exec(
            id,
            CreateExecOptions {
                attach_stdout: Some(!detached),
                attach_stderr: Some(!detached),
                cmd: Some(vec!["/bin/sh", "-c", script]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    if let StartExecResults::Attached { mut output, .. } = docker
        .start_exec(
            &command.id,
            Some(StartExecOptions {
                detach: detached,
                ..Default::default()
            }),
        )
        .await
        .unwrap()
    {
        while let Some(frame) = output.next().await {
            frame.unwrap();
        }
        assert_eq!(
            docker.inspect_exec(&command.id).await.unwrap().exit_code,
            Some(0)
        );
    }
    command.id
}

async fn assert_exec_stopped(docker: &Docker, id: &str) {
    match docker.inspect_exec(id).await {
        Ok(state) => assert_eq!(state.running, Some(false)),
        Err(error) => assert!(missing(&error), "cannot verify descendant stop: {error}"),
    }
}

#[tokio::test]
async fn container_initializes_without_host_codex_and_reuses_persistent_home() {
    let fixture = Fixture::new().await;
    let sentinel = fixture.root.path().join("codex/container-sentinel");
    let mut first = Client::start(fixture.launch()).await.unwrap();
    assert!(domain::codex::valid_codex_version(first.version()));
    assert!(first.account().await.unwrap().account.is_none());
    let first_id = fixture.session_id().await;
    exec(
        &fixture.docker,
        &first_id,
        "test -x /opt/codex/bin/codex && printf persistent > \"$CODEX_HOME/container-sentinel\"",
        false,
    )
    .await;
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "persistent");
    first.close().await.unwrap();
    wait_removed(&fixture.docker, &first_id).await;

    let mut second = Client::start(fixture.launch()).await.unwrap();
    assert!(second.account().await.unwrap().account.is_none());
    let second_id = fixture.session_id().await;
    assert_ne!(first_id, second_id);
    exec(
        &fixture.docker,
        &second_id,
        "test \"$(cat \"$CODEX_HOME/container-sentinel\")\" = persistent",
        false,
    )
    .await;
    second.close().await.unwrap();
    wait_removed(&fixture.docker, &second_id).await;
    assert!(!fixture.root.path().join("codex/auth.json").exists());
}

#[tokio::test]
async fn mission_limits_duplicate_fence_and_close_or_drop_stop_every_process() {
    let fixture = Fixture::new().await;
    let run = Id::new();
    // Reproduce QZ dying after create but before start, without a live owner.
    let orphan = fixture
        .docker
        .create_container(
            Some(
                CreateContainerOptionsBuilder::default()
                    .name(&format!("quazonai-codex-mission-{run}"))
                    .build(),
            ),
            ContainerCreateBody {
                image: Some(fixture.image.clone()),
                labels: Some(HashMap::from([
                    ("io.quazonai.codex.image".into(), fixture.image.clone()),
                    ("io.quazonai.codex.run".into(), run.to_string()),
                    ("io.quazonai.codex.owner".into(), Id::new().to_string()),
                ])),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut client = Client::start_mission(fixture.launch(), limits(run, 60))
        .await
        .unwrap();
    wait_removed(&fixture.docker, &orphan.id).await;
    let id = fixture.session_id().await;
    let ContainerInspectResponse {
        config,
        host_config,
        ..
    } = fixture.docker.inspect_container(&id, None).await.unwrap();
    let config = config.unwrap();
    let metadata = fs::metadata(fixture.root.path().join("codex")).unwrap();
    assert_eq!(
        config.user,
        Some(format!("{}:{}", metadata.uid(), metadata.gid()))
    );
    let host = host_config.unwrap();
    assert_eq!(host.cpu_period, Some(1_000_000));
    assert_eq!(host.cpu_quota, Some(500_000));
    assert_eq!(host.memory, Some(256 * 1024 * 1024));
    assert_eq!(host.memory_swap, host.memory);
    assert_eq!(host.pids_limit, Some(128));
    assert_eq!(host.init, Some(true));
    assert_eq!(host.network_mode.as_deref(), Some("host"));
    let ulimits = host.ulimits.unwrap();
    for (name, value) in [("core", 0), ("fsize", 67_108_864)] {
        assert!(ulimits
            .iter()
            .any(|limit| limit.name.as_deref() == Some(name)
                && limit.soft == Some(value)
                && limit.hard == Some(value)));
    }
    exec(
        &fixture.docker,
        &id,
        r#"set -e
test "$(cat /sys/fs/cgroup/memory.max)" = 268435456
test "$(cat /sys/fs/cgroup/memory.swap.max)" = 0
test "$(cat /sys/fs/cgroup/pids.max)" = 128
test "$(cat /sys/fs/cgroup/cpu.max)" = '500000 1000000'
grep -Eq '^Max file size[[:space:]]+67108864[[:space:]]+67108864[[:space:]]+bytes' /proc/self/limits
grep -Eq '^Max core file size[[:space:]]+0[[:space:]]+0[[:space:]]+bytes' /proc/self/limits"#,
        false,
    )
    .await;
    let child = exec(&fixture.docker, &id, "sleep 120 & wait", true).await;
    assert_eq!(
        fixture.docker.inspect_exec(&child).await.unwrap().running,
        Some(true)
    );
    assert!(Client::start_mission(fixture.launch(), limits(run, 60))
        .await
        .is_err());
    assert!(client.account().await.unwrap().account.is_none());
    client.close().await.unwrap();
    wait_removed(&fixture.docker, &id).await;
    assert_exec_stopped(&fixture.docker, &child).await;

    let client = Client::start_mission(fixture.launch(), limits(run, 60))
        .await
        .unwrap();
    let replacement = fixture.session_id().await;
    assert_ne!(id, replacement);
    let child = exec(&fixture.docker, &replacement, "sleep 120 & wait", true).await;
    drop(client);
    wait_removed(&fixture.docker, &replacement).await;
    assert_exec_stopped(&fixture.docker, &child).await;
}

#[tokio::test]
async fn mission_deadline_stops_the_container_without_worker_intervention() {
    let fixture = Fixture::new().await;
    let run = Id::new();
    let client = Client::start_mission(fixture.launch(), limits(run, 10))
        .await
        .unwrap();
    let id = fixture.session_id().await;
    let child = exec(&fixture.docker, &id, "sleep 120 & wait", true).await;
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let state = match fixture.docker.inspect_container(&id, None).await {
                Ok(container) => container.state.unwrap(),
                Err(error) if missing(&error) => break,
                Err(error) => panic!("cannot verify Mission deadline: {error}"),
            };
            if state.running == Some(false) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the container deadline must also stop background descendants");
    assert_exec_stopped(&fixture.docker, &child).await;
    drop(client);
    wait_removed(&fixture.docker, &id).await;
    let replacement = Client::start_mission(fixture.launch(), limits(run, 60))
        .await
        .unwrap();
    let next_id = fixture.session_id().await;
    replacement.close().await.unwrap();
    wait_removed(&fixture.docker, &next_id).await;
}

#[tokio::test]
async fn mission_thread_executes_its_native_sandbox_inside_the_container() {
    let fixture = Fixture::new().await;
    let provider = responses::Provider::start(&fixture.root.path().join("codex")).await;
    provider.fail_after_tool();
    let run = Id::new();
    let mut client = Client::start_mission(fixture.launch(), limits(run, 60))
        .await
        .unwrap();
    let id = fixture.session_id().await;
    let mut options = ThreadOptions::read_only(fixture.root.path().join("workspace"));
    options.mission = Some(MissionOptions {
        server_binary: std::env::current_exe().unwrap(),
        api_origin: "http://127.0.0.1:8081".into(),
        development_http: true,
        binding: server::mcp::MissionBinding {
            project_id: Id::new(),
            cycle_id: Id::new(),
            run_id: run,
            attempt_id: Id::new(),
            brief_id: Id::new(),
        },
        token: None,
        executable_path: "/usr/bin:/bin".into(),
    });
    let thread = client.start_thread(&options).await.unwrap();
    let turn = client
        .start_turn(
            "container-native-shell",
            &thread.thread.id,
            responses::FIRST_PROMPT,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match client.observations(Duration::from_millis(100)).await {
                Ok(events) => {
                    for event in events {
                        if let Observation::TurnCompleted {
                            thread_id,
                            turn: actual,
                        } = event
                        {
                            assert_eq!(thread_id, thread.thread.id);
                            assert_eq!(actual.id, turn.id);
                            assert_eq!(actual.status, TurnStatus::Failed);
                            return;
                        }
                    }
                }
                Err(NativeFailure::Closed) => return,
                Err(error) => panic!("native shell observation failed: {error}"),
            }
        }
    })
    .await
    .expect("the controlled upstream fault must terminate the native turn");
    // The shared fixture accepts request 2 only after receiving the real shell's
    // QZ_NATIVE_TOOL_DONE output, then intentionally truncates its response.
    assert_eq!(provider.request_count(), 2);
    client.close().await.unwrap();
    wait_removed(&fixture.docker, &id).await;
}

#[tokio::test]
async fn cancelled_start_future_cleans_up_its_own_container() {
    let fixture = Fixture::new().await;
    let run = Id::new();
    let name = format!("quazonai-codex-mission-{run}");
    let launch = fixture.launch();
    let task = tokio::spawn(async move { Client::start_mission(launch, limits(run, 60)).await });
    let id = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match fixture.docker.inspect_container(&name, None).await {
                Ok(container) => break container.id.unwrap(),
                Err(error) if missing(&error) => tokio::time::sleep(Duration::from_millis(5)).await,
                Err(error) => panic!("cannot observe test Mission creation: {error}"),
            }
        }
    })
    .await
    .unwrap();
    task.abort();
    match task.await {
        Ok(Ok(client)) => drop(client),
        Err(error) => assert!(error.is_cancelled()),
        Ok(Err(error)) => panic!("test Mission failed before cancellation: {error}"),
    }
    wait_removed(&fixture.docker, &id).await;
}
