//! Trusted Mission bootstrap, not an Agent loop. Only the native App Server owns
//! messages/tools/history. This module never returns the ephemeral MCP secret.
use super::{Worker, WorkerFailure};
use crate::{
    codex_native::{self as native, Client, MissionOptions},
    codex_profiles::CodexDeployment,
    mcp::MissionBinding,
};
use contracts::{codex::CodexEffectiveSettingsV1, Id};
use integrations::{authentication, secrets::SecretVault};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use store::{
    lifecycle::{
        mission::{MissionSession, NativeSessionReceipt},
        ClaimResult, ExperimentWork, NextRuntimeAction, RunLease, RunMessage,
    },
    turns::{NativePublicSummary, TurnOutcome, WorkerFence},
    Store, StoreError,
};
use tokio::sync::watch;

mod turn;
pub use turn::TurnProgress;

pub struct MissionLauncher {
    deployment: CodexDeployment,
    workspace_root: PathBuf,
    server_binary: PathBuf,
    api_origin: String,
    development_http: bool,
}

/// Internal native connection only. No Debug, Serialize, raw token or history.
pub struct MissionConnection {
    pub client: Client,
    pub session: MissionSession,
}

impl Worker {
    /// Trusted queue entry point, shared by the daemon and actual native tests.
    pub async fn process_mission_message(
        &self,
        message: RunMessage,
        owner: &str,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), WorkerFailure> {
        let launcher = self.missions.as_ref().ok_or(WorkerFailure::TaskKind)?;
        if *shutdown.borrow() || shutdown.has_changed().is_err() {
            return Err(WorkerFailure::LostAuthority);
        }
        let lease = match self.store.claim_mission(&message, owner, 60).await? {
            None => return Err(WorkerFailure::TaskKind),
            Some(ClaimResult::Busy) => return Ok(()),
            Some(ClaimResult::Terminal(_)) => {
                self.store.acknowledge_run(&message).await?;
                return Ok(());
            }
            Some(ClaimResult::Leased(lease)) => *lease,
        };
        let heartbeat = async {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if self
                    .store
                    .renew_run_lease(lease.run.id, &lease.fence, 60)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        };
        let observed = shutdown.clone();
        // Dropping this finite driver also drops/kills its owned native process.
        // Neither shutdown nor a lost renewal fabricates a Turn/Run receipt.
        tokio::select! {
            biased;
            _ = shutdown.changed() => Err(WorkerFailure::LostAuthority),
            _ = heartbeat => Err(WorkerFailure::LostAuthority),
            result = self.drive_mission(launcher, &lease, &observed) => {
                if result? {
                    self.store.acknowledge_run(&message).await?;
                }
                Ok(())
            },
        }
    }

    async fn drive_mission(
        &self,
        launcher: &MissionLauncher,
        lease: &RunLease,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<bool, WorkerFailure> {
        let run = lease.run.id;
        let fence = &lease.fence;
        let job = self.store.mission_job(run, fence).await?;
        if job.session.is_some()
            && self
                .store
                .mission_turn_checkpoint(run, fence)
                .await?
                .latest
                .is_some_and(|latest| {
                    latest.receipt.is_some_and(|receipt| {
                        receipt.outcome != TurnOutcome::Succeeded
                            || latest.summary_artifact_id.is_some()
                    })
                })
        {
            return self.advance_mission_experiment(lease).await;
        }
        let mut connection = launcher
            .open(&self.store, self.vault.clone(), run, fence)
            .await?;
        let result: Result<(), WorkerFailure> = async {
            let reading = self.objects.clone();
            let publishing = self.objects.clone();
            self.store
                .prepare_initial_mission_turn(
                    run,
                    fence,
                    move |id, size| async move {
                        tokio::task::spawn_blocking(move || reading.read(id, size))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    },
                    move |object| async move {
                        tokio::task::spawn_blocking(move || {
                            publishing.put(object.id, &object.bytes)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                    },
                )
                .await?;
            connection
                .drive_turn(&self.store, self.objects.clone(), run, fence, shutdown)
                .await?;
            self.capture_mission_summary(&mut connection, lease).await?;
            Ok(())
        }
        .await;
        let closed = connection.client.close().await;
        result?;
        closed.map_err(|reason| WorkerFailure::Codex("CLOSE_MISSION", reason))?;
        self.advance_mission_experiment(lease).await
    }

    async fn capture_mission_summary(
        &self,
        connection: &mut MissionConnection,
        lease: &RunLease,
    ) -> Result<(), WorkerFailure> {
        let Some(latest) = self
            .store
            .mission_turn_checkpoint(lease.run.id, &lease.fence)
            .await?
            .latest
        else {
            return Ok(());
        };
        if latest.summary_artifact_id.is_some()
            || !latest
                .receipt
                .is_some_and(|receipt| receipt.outcome == TurnOutcome::Succeeded)
        {
            return Ok(());
        }
        let turn = latest.native_turn_id.ok_or(WorkerFailure::Contract)?;
        let message = connection
            .client
            .public_summary(&connection.session.native.thread_id, &turn)
            .await
            .map_err(|reason| WorkerFailure::Codex("PUBLIC_SUMMARY", reason))?
            .ok_or(WorkerFailure::Contract)?;
        let summary = NativePublicSummary {
            schema_version: contracts::SchemaV1,
            native_turn_id: turn,
            native_item_id: message.id,
            phase: message.phase,
            text: message.text,
        };
        let reading = self.objects.clone();
        let publishing = self.objects.clone();
        self.store
            .record_mission_summary(
                latest.reservation.id,
                &lease.fence,
                &summary,
                move |id, size| async move {
                    tokio::task::spawn_blocking(move || reading.read(id, size))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                },
                move |object| async move {
                    tokio::task::spawn_blocking(move || publishing.put(object.id, &object.bytes))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                },
            )
            .await?;
        Ok(())
    }

    async fn advance_mission_experiment(&self, lease: &RunLease) -> Result<bool, WorkerFailure> {
        if !self
            .store
            .mission_turn_checkpoint(lease.run.id, &lease.fence)
            .await?
            .latest
            .is_some_and(|latest| latest.receipt.is_some())
        {
            return Ok(false);
        }
        let Some(work) = self
            .store
            .next_mission_experiment(lease.run.id, &lease.fence)
            .await?
        else {
            let reading = self.objects.clone();
            let publishing = self.objects.clone();
            let prepared = self
                .store
                .prepare_mission_result_turn(
                    lease.run.id,
                    &lease.fence,
                    move |id, size| {
                        let objects = reading.clone();
                        async move {
                            tokio::task::spawn_blocking(move || objects.read(id, size))
                                .await
                                .map_err(|_| StoreError::Integrity)?
                                .map_err(|_| StoreError::Integrity)
                        }
                    },
                    move |object| async move {
                        tokio::task::spawn_blocking(move || {
                            publishing.put(object.id, &object.bytes)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                    },
                )
                .await?;
            return if prepared {
                Ok(false)
            } else {
                Ok(self
                    .store
                    .complete_research_mission(lease.run.id, &lease.fence)
                    .await?)
            };
        };
        if !matches!(work, ExperimentWork::RecordAlpha(_)) {
            let native = self.transport(lease).await?;
            self.refresh(&native, lease.run.id, &lease.fence).await?;
        }
        let mut limits = lease.limits.clone();
        let publishing = self.objects.clone();
        let publish = move |object: store::lifecycle::native::NativeObjectPublication| async move {
            tokio::task::spawn_blocking(move || publishing.put(object.id, &object.bytes))
                .await
                .map_err(|_| StoreError::Integrity)?
                .map_err(|_| StoreError::Integrity)
        };
        match work {
            ExperimentWork::RecordAlpha(experiment) => {
                self.store
                    .prepare_research_alpha(lease.run.id, &lease.fence, experiment)
                    .await?;
            }
            ExperimentWork::Compile(experiment) => {
                limits.experiments = 1;
                self.store
                    .start_experiment_compilation(
                        lease.run.id,
                        &lease.fence,
                        experiment,
                        &limits,
                        publish,
                    )
                    .await?;
            }
            ExperimentWork::Forecast(experiment) => {
                limits.experiments = 0;
                let reading = self.objects.clone();
                self.store
                    .start_experiment_forecast(
                        lease.run.id,
                        &lease.fence,
                        experiment,
                        &limits,
                        move |id, size| {
                            let objects = reading.clone();
                            async move {
                                tokio::task::spawn_blocking(move || objects.read(id, size))
                                    .await
                                    .map_err(|_| StoreError::Integrity)?
                                    .map_err(|_| StoreError::Integrity)
                            }
                        },
                        publish,
                    )
                    .await?;
            }
        }
        // Preparation never acknowledges a Mission or sends a new model request.
        Ok(false)
    }
}

fn private_directory(path: &Path) -> Result<(), WorkerFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|_| WorkerFailure::Contract)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(WorkerFailure::Contract);
    }
    Ok(())
}

impl MissionLauncher {
    pub fn new(
        deployment: CodexDeployment,
        workspace_root: PathBuf,
        server_binary: PathBuf,
        api_origin: String,
        development_http: bool,
    ) -> Result<Self, WorkerFailure> {
        domain::settings::endpoint(&api_origin, development_http)
            .map_err(|_| WorkerFailure::Contract)?;
        if !workspace_root.is_absolute() || !server_binary.is_absolute() || !server_binary.is_file()
        {
            return Err(WorkerFailure::Contract);
        }
        private_directory(&workspace_root)?;
        Ok(Self {
            deployment,
            workspace_root: fs::canonicalize(workspace_root)
                .map_err(|_| WorkerFailure::Contract)?,
            server_binary: fs::canonicalize(server_binary).map_err(|_| WorkerFailure::Contract)?,
            api_origin,
            development_http,
        })
    }

    async fn workspace(&self, run: Id) -> Result<PathBuf, WorkerFailure> {
        private_directory(&self.workspace_root)?;
        let path = self.workspace_root.join(run.to_string());
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {
                // A new, unrelated native Git working tree. No source checkout,
                // auth copy, template hooks or personal Git configuration.
                let mut command = tokio::process::Command::new("git");
                command
                    .args(["init", "--quiet", "--template=", "--initial-branch=mission"])
                    .current_dir(&path)
                    .env_clear()
                    .env("PATH", self.deployment.executable_path())
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true);
                let status = tokio::time::timeout(Duration::from_secs(10), command.status())
                    .await
                    .map_err(|_| WorkerFailure::Contract)?
                    .map_err(|_| WorkerFailure::Contract)?;
                if !status.success() {
                    return Err(WorkerFailure::Contract);
                }
                fs::File::open(&self.workspace_root)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| WorkerFailure::Contract)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(WorkerFailure::Contract),
        }
        private_directory(&path)?;
        // A failed initialization is retained for diagnosis, not silently reused
        // or replaced. Linked external repositories are not Mission workspaces.
        let git = fs::symlink_metadata(path.join(".git")).map_err(|_| WorkerFailure::Contract)?;
        if !git.is_dir() {
            return Err(WorkerFailure::Contract);
        }
        Ok(path)
    }

    /// Callers must maintain the existing Attempt heartbeat while this bounded
    /// bootstrap runs. It does not send a paid turn or mark the Run as RUNNING.
    pub async fn open(
        &self,
        store: &Store,
        vault: Arc<SecretVault>,
        run: Id,
        fence: &WorkerFence,
    ) -> Result<MissionConnection, WorkerFailure> {
        tokio::time::timeout(
            Duration::from_secs(110),
            self.open_inner(store, vault, run, fence),
        )
        .await
        .map_err(|_| {
            WorkerFailure::Codex("BOOTSTRAP_TIMEOUT", native::NativeFailure::Unavailable)
        })?
    }

    async fn open_inner(
        &self,
        store: &Store,
        vault: Arc<SecretVault>,
        run: Id,
        fence: &WorkerFence,
    ) -> Result<MissionConnection, WorkerFailure> {
        let job = store.mission_job(run, fence).await?;
        if job.lease.action == NextRuntimeAction::Cancel
            || (job.session.is_none() && job.lease.action != NextRuntimeAction::PrepareDispatch)
        {
            // An unknown Thread start is never a license to create a second one.
            return Err(WorkerFailure::Codex(
                "THREAD_IDENTITY",
                native::NativeFailure::Correlation,
            ));
        }
        let resources = native::MissionProcess::new(
            run,
            job.lease.limits.clone(),
            u32::try_from((job.lease.run.deadline_at - job.observed_at).num_seconds())
                .map_err(|_| WorkerFailure::Contract)?
                .min(job.lease.limits.wall_seconds),
        )
        .map_err(|reason| WorkerFailure::Codex("RESOURCE_BOUNDS", reason))?;
        let workspace = self.workspace(run).await?;
        let (mut client, mut options) = self
            .deployment
            .mission_connection(&job.profile, vault.clone(), &workspace, resources)
            .await
            .map_err(|_| {
                WorkerFailure::Codex("PROFILE_CONNECTION", native::NativeFailure::Unavailable)
            })?;
        if let Some(session) = &job.session {
            let requested = &session.requested_settings;
            if session.native.codex_version != native::VERSION
                || session.native.protocol_schema_version != "v2"
                || options.model != requested.model
                || options.reasoning_effort != requested.reasoning_effort
                || options.service_tier != requested.service_tier
            {
                return Err(WorkerFailure::Codex(
                    "REQUESTED_SETTINGS",
                    native::NativeFailure::ModelUnavailable,
                ));
            }
        }
        let token = issue(store, vault, run, fence).await?;
        options.mission = Some(MissionOptions {
            server_binary: self.server_binary.clone(),
            api_origin: self.api_origin.clone(),
            development_http: self.development_http,
            binding: MissionBinding {
                project_id: job.lease.run.project_id,
                cycle_id: job.lease.run.cycle_id.ok_or(WorkerFailure::Contract)?,
                run_id: run,
                attempt_id: fence.attempt_id,
                brief_id: job.brief_id,
            },
            token,
            executable_path: self
                .deployment
                .executable_path()
                .to_str()
                .ok_or(WorkerFailure::Contract)?
                .to_owned(),
        });
        let thread = if let Some(session) = &job.session {
            client
                .resume_thread(&session.native.thread_id, &options)
                .await
                .map_err(|reason| WorkerFailure::Codex("RESUME_THREAD", reason))?
        } else {
            if !store.begin_run_dispatch(run, fence).await? {
                return Err(WorkerFailure::LostAuthority);
            }
            client
                .start_thread(&options)
                .await
                .map_err(|reason| WorkerFailure::Codex("START_THREAD", reason))?
        };
        let receipt = NativeSessionReceipt {
            thread_id: thread.thread.id,
            codex_version: native::VERSION.into(),
            protocol_schema_version: "v2".into(),
            requested_service_tier: options.service_tier,
            effective: CodexEffectiveSettingsV1 {
                model: thread.model,
                provider: thread.model_provider,
                reasoning_effort: thread.reasoning_effort,
                service_tier: thread.service_tier,
            },
        };
        // The same transaction validates native metadata on resume. Neither new
        // defaults nor a lost/empty native Thread can replace the original.
        let session = store.bind_mission_session(run, fence, &receipt).await?;
        let tools = client
            .mission_tool_names(&session.native.thread_id)
            .await
            .map_err(|reason| WorkerFailure::Codex("MCP_INVENTORY", reason))?;
        if tools.len() != 1
            || !tools.get("quazonai_mission").is_some_and(|names| {
                names
                    .iter()
                    .any(|name| name.ends_with("research.get_brief"))
            })
        {
            return Err(WorkerFailure::Codex(
                "MCP_REQUIRED",
                native::NativeFailure::Contract,
            ));
        }
        Ok(MissionConnection { client, session })
    }
}

async fn issue(
    store: &Store,
    vault: Arc<SecretVault>,
    run: Id,
    fence: &WorkerFence,
) -> Result<String, WorkerFailure> {
    let public = Id::new();
    let generating = vault.clone();
    let (token, reference) = tokio::task::spawn_blocking(move || {
        let secret = authentication::random_capability();
        let verifier =
            authentication::capability_verifier(&secret).map_err(|_| WorkerFailure::Contract)?;
        let token = authentication::format_machine_token(public, &secret)
            .map_err(|_| WorkerFailure::Contract)?;
        let reference = generating
            .put("MACHINE_VERIFIER", verifier.as_bytes())
            .map_err(|_| WorkerFailure::Contract)?;
        Ok::<_, WorkerFailure>((token, reference))
    })
    .await
    .map_err(|_| WorkerFailure::Contract)??;
    if let Err(error) = store
        .issue_mission_credential(run, fence, public, reference)
        .await
    {
        if crate::secrets::reconcile_verifier(store, vault, reference)
            .await
            .is_err()
        {
            tracing::warn!("Mission verifier requires native reconciliation");
        }
        return Err(error.into());
    }
    Ok(token)
}
