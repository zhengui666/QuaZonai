//! Trusted finite-concurrency PGMQ driver. Research code runs only at the registered
//! native Runtime; this process owns no scientific engine or duplicate queue.
use crate::runtime_transport::{RuntimeRequestError, RuntimeTargets, RuntimeTransport};
use contracts::{runs::RunKind, runtime_jobs::*, Id, SchemaV1};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use std::{sync::Arc, time::Duration};
use store::{
    lifecycle::{
        native::{NativeJob, NativePayloads},
        ClaimResult, NativeOutcome, NextRuntimeAction, RunLease, RunMessage, TerminalObservation,
    },
    turns::WorkerFence,
    Store, StoreError,
};
use tokio::{sync::watch, task::JoinSet};

#[derive(Clone)]
pub struct Worker {
    store: Store,
    vault: Arc<SecretVault>,
    objects: Arc<ArtifactStore>,
    targets: Arc<RuntimeTargets>,
    parallelism: usize,
}

#[derive(Debug)]
pub enum WorkerFailure {
    LostAuthority,
    Store,
    Runtime,
    TaskKind,
    Contract,
}
impl std::fmt::Display for WorkerFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::LostAuthority => "native worker lost its Attempt authority",
            Self::Store => "native worker storage or transaction is unavailable",
            Self::Runtime => "native Runtime is unavailable or the remote outcome is unknown",
            Self::TaskKind => "this Run requires a different registered task driver",
            Self::Contract => "native task input or output violates its fixed contract",
        })
    }
}
impl std::error::Error for WorkerFailure {}
impl From<StoreError> for WorkerFailure {
    fn from(error: StoreError) -> Self {
        if matches!(error, StoreError::Domain(domain::DomainError::StaleAttempt)) {
            Self::LostAuthority
        } else {
            Self::Store
        }
    }
}

impl Worker {
    pub fn new(
        store: Store,
        vault: SecretVault,
        objects: ArtifactStore,
        targets: RuntimeTargets,
        parallelism: usize,
    ) -> Result<Self, WorkerFailure> {
        if !(1..=32).contains(&parallelism) {
            return Err(WorkerFailure::Contract);
        }
        Ok(Self {
            store,
            vault: Arc::new(vault),
            objects: Arc::new(objects),
            targets: Arc::new(targets),
            parallelism,
        })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> Result<(), WorkerFailure> {
        let owner = format!("worker/{}", Id::new());
        let mut jobs = JoinSet::new();
        while !*shutdown.borrow() {
            while let Some(result) = jobs.try_join_next() {
                if !matches!(result, Ok(Ok(()))) {
                    tracing::warn!("worker task deferred for native reconciliation");
                }
            }
            if jobs.len() < self.parallelism {
                let remaining = (self.parallelism - jobs.len()).min(32) as i32;
                match self.store.read_run_messages(60, remaining).await {
                    Ok(messages) => {
                        for message in messages {
                            // A visibility replay in this process is a different
                            // claimant, not another driver sharing an active fence.
                            let worker = self.clone();
                            let owner = format!("{owner}/{}", Id::new());
                            let shutdown = shutdown.clone();
                            jobs.spawn(async move {
                                worker.process_message(message, &owner, shutdown).await
                            });
                        }
                    }
                    Err(_) => tracing::warn!(
                        "native PGMQ read unavailable; no result or acknowledgement inferred"
                    ),
                }
            }
            tokio::select! {
                changed = shutdown.changed() => { if changed.is_err() { break; } }
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                result = jobs.join_next(), if !jobs.is_empty() => {
                    if !matches!(result, Some(Ok(Ok(())))) { tracing::warn!("worker task deferred for native reconciliation"); }
                }
            }
        }
        // Stop new work; let already bounded I/O and local publication reach a
        // known result. Gateway jobs continue under their native deadlines.
        while let Some(result) = jobs.join_next().await {
            if !matches!(result, Ok(Ok(()))) {
                tracing::warn!("worker shutdown left a durable task for reconciliation");
            }
        }
        Ok(())
    }

    /// The same method is exercised by native PostgreSQL/TCP fault tests. It is
    /// not an HTTP route or an Agent capability and receives a trusted Store.
    pub async fn process_message(
        &self,
        message: RunMessage,
        owner: &str,
        shutdown: watch::Receiver<bool>,
    ) -> Result<(), WorkerFailure> {
        let lease = match self.store.claim_run(&message, owner, 60).await? {
            ClaimResult::Busy => return Ok(()),
            ClaimResult::Terminal(_) => {
                self.store.acknowledge_run(&message).await?;
                return Ok(());
            }
            ClaimResult::Leased(lease) => *lease,
        };
        if lease.run.kind == RunKind::AgentResearch {
            // The native Codex driver owns Mission/Thread/Turn. Never execute an
            // Agent loop through a scientific container or call it a successful job.
            return Err(WorkerFailure::TaskKind);
        }
        let (alive, health) = watch::channel(true);
        let heartbeat = async {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if self
                    .store
                    .renew_run_lease(lease.run.id, &lease.fence, 60)
                    .await
                    .is_err()
                {
                    let _ = alive.send(false);
                    break;
                }
            }
        };
        let drive = self.drive(&lease, &health, &shutdown);
        tokio::pin!(heartbeat, drive);
        // Both futures belong to this task; abort/crash cannot detach a renewal
        // loop which keeps an abandoned owner alive. A failed renewal stops new
        // side effects but lets bounded I/O/publication finish and clean up.
        let result = tokio::select! {
            result = &mut drive => result,
            () = &mut heartbeat => drive.await,
        };
        result?;
        self.store.acknowledge_run(&message).await?;
        Ok(())
    }

    async fn transport(&self, lease: &RunLease) -> Result<RuntimeTransport, WorkerFailure> {
        let vault = self.vault.clone();
        let targets = self.targets.clone();
        let snapshot = lease.runtime.clone();
        tokio::task::spawn_blocking(move || {
            crate::runtime::native_transport(&vault, &targets, &snapshot)
        })
        .await
        .map_err(|_| WorkerFailure::Runtime)?
        .map_err(|_| WorkerFailure::Runtime)
    }

    async fn drive(
        &self,
        lease: &RunLease,
        health: &watch::Receiver<bool>,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<(), WorkerFailure> {
        let run = lease.run.id;
        let fence = &lease.fence;
        if self
            .store
            .settle_unsubmitted_native_run(run, fence)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let native = self.transport(lease).await?;
        loop {
            if !*health.borrow() || *shutdown.borrow() || shutdown.has_changed().is_err() {
                return Err(WorkerFailure::LostAuthority);
            }
            if self
                .store
                .settle_unsubmitted_native_run(run, fence)
                .await?
                .is_some()
            {
                return Ok(());
            }
            self.refresh(&native, run, fence).await?;
            let job = self.store.native_job(run, fence).await?;
            if job.action == NextRuntimeAction::PrepareDispatch {
                self.upload(&native, &job, fence, health, shutdown).await?;
                self.refresh(&native, run, fence).await?;
                if self.store.begin_run_dispatch(run, fence).await? {
                    if !*health.borrow() || *shutdown.borrow() || shutdown.has_changed().is_err() {
                        return Err(WorkerFailure::LostAuthority);
                    }
                    // Exactly one caller owns this send permit. A failed/unknown
                    // HTTP result remains SENT_UNKNOWN and is queried, never reposted.
                    match native.submit_job(&job.spec).await {
                        Ok(status) => {
                            if self.status(&native, &job, fence, status).await? {
                                return Ok(());
                            }
                        }
                        Err(_) => {
                            tracing::warn!(run_id=%run, "native submit acknowledgement unavailable; preserving original identity")
                        }
                    }
                }
            } else {
                let status = if job.action == NextRuntimeAction::Cancel
                    || chrono::Utc::now() >= job.spec.deadline_at
                {
                    native
                        .cancel_job(
                            &job.spec.external_job_id,
                            &RuntimeCancelV1 {
                                schema_version: SchemaV1,
                                run_id: run,
                                attempt_no: job.spec.attempt_no,
                                owner_epoch: fence.owner_epoch,
                            },
                        )
                        .await
                } else {
                    native.job_status(&job.spec.external_job_id).await
                };
                match status {
                    Ok(status) => {
                        if self.status(&native, &job, fence, status).await? {
                            return Ok(());
                        }
                    }
                    Err(RuntimeRequestError::Missing | RuntimeRequestError::Unavailable) => {}
                    Err(_) => return Err(WorkerFailure::Runtime),
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn refresh(
        &self,
        native: &RuntimeTransport,
        run: Id,
        fence: &WorkerFence,
    ) -> Result<(), WorkerFailure> {
        use contracts::runtime::RuntimeProbeOutcomeV1;
        let Some(ticket) = self.store.prepare_run_runtime_probe(run, fence).await? else {
            return Ok(());
        };
        // The actual remote probe is outside every database transaction. The
        // pending Run's fence, not an implicit Operator identity, owns this refresh.
        let outcome = match native.capabilities().await {
            Ok(capabilities) => RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            Err(reason) => RuntimeProbeOutcomeV1::Unavailable { reason },
        };
        let objects = self.objects.clone();
        let mut allocated = None;
        let result = self
            .store
            .complete_run_runtime_probe(ticket, outcome, |id, bytes| {
                allocated = Some(id);
                async move {
                    tokio::task::spawn_blocking(move || objects.put(id, &bytes))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                }
            })
            .await;
        if let Some(id) = allocated.filter(|_| result.is_err()) {
            let objects = self.objects.clone();
            if self
                .store
                .discard_unpublished_native_object(run, id, move |id| async move {
                    tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                })
                .await
                .is_err()
            {
                tracing::warn!(artifact_id=%id, "fenced native probe cleanup deferred");
            }
        }
        let result = result?;
        if !matches!(result.outcome, RuntimeProbeOutcomeV1::Available { .. }) {
            return Err(WorkerFailure::Runtime);
        }
        Ok(())
    }

    async fn upload(
        &self,
        native: &RuntimeTransport,
        job: &NativeJob,
        fence: &WorkerFence,
        health: &watch::Receiver<bool>,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<(), WorkerFailure> {
        for input in &job.spec.inputs {
            if let RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                storage_version,
                ..
            } = input
            {
                if !*health.borrow() || *shutdown.borrow() || shutdown.has_changed().is_err() {
                    return Err(WorkerFailure::LostAuthority);
                }
                let actual = self
                    .store
                    .native_input(job.run.id, fence, *artifact_id)
                    .await?;
                if actual != (*byte_count, storage_version.clone()) {
                    return Err(WorkerFailure::Contract);
                }
                let objects = self.objects.clone();
                let id = *artifact_id;
                let count = *byte_count;
                let bytes = tokio::task::spawn_blocking(move || objects.read(id, count))
                    .await
                    .map_err(|_| WorkerFailure::Store)?
                    .map_err(|_| WorkerFailure::Store)?;
                // The original input set may be revoked while bounded local I/O runs.
                self.store.native_input(job.run.id, fence, id).await?;
                native
                    .upload_object(id, storage_version, bytes)
                    .await
                    .map_err(|_| WorkerFailure::Runtime)?;
            }
        }
        Ok(())
    }

    async fn status(
        &self,
        native: &RuntimeTransport,
        job: &NativeJob,
        fence: &WorkerFence,
        status: RuntimeJobStatusV1,
    ) -> Result<bool, WorkerFailure> {
        match status.state {
            RuntimeJobState::Accepted => {
                self.store
                    .observe_native_accepted(job.run.id, fence, &status)
                    .await?;
                return Ok(false);
            }
            RuntimeJobState::Running => {
                self.store
                    .observe_run_running(job.run.id, fence, &status.external_job_id)
                    .await?;
                return Ok(false);
            }
            RuntimeJobState::CancelRequested => return Ok(false),
            RuntimeJobState::Cancelled if !status.has_result => {
                // This is the actual gateway's durable pre-submit cancellation
                // tombstone, never a bare HTTP404 or a requested-cancellation echo.
                self.store
                    .accept_run_terminal(
                        job.run.id,
                        fence,
                        &TerminalObservation {
                            schema_version: SchemaV1,
                            external_job_id: status.external_job_id,
                            outcome: NativeOutcome::ConfirmedAbsent,
                            manifest_artifact_id: None,
                            failure_class: None,
                            failure_code: None,
                            observed_at: chrono::DateTime::from_timestamp_micros(
                                chrono::Utc::now().timestamp_micros(),
                            )
                            .ok_or(WorkerFailure::Contract)?,
                        },
                    )
                    .await?;
                return Ok(true);
            }
            _ => {}
        }
        let result = native
            .job_result(&job.spec, job.submitted_not_before)
            .await
            .map_err(|_| WorkerFailure::Runtime)?;
        let mut outputs = Vec::new();
        let mut invalid = false;
        for descriptor in &result.manifest.artifacts {
            match native
                .job_artifact(&job.spec.external_job_id, descriptor)
                .await
            {
                Ok(bytes) => {
                    if domain::execution::output_shape(descriptor, &bytes).is_err() {
                        invalid = true;
                        break;
                    }
                    outputs.push((descriptor.clone(), bytes));
                }
                Err(RuntimeRequestError::Unavailable | RuntimeRequestError::Authentication) => {
                    return Err(WorkerFailure::Runtime)
                }
                Err(_) => {
                    invalid = true;
                    break;
                }
            }
        }
        let payloads = if invalid {
            NativePayloads::InvalidOutput
        } else {
            NativePayloads::Verified(outputs)
        };
        let reading = self.objects.clone();
        let publishing = self.objects.clone();
        let mut allocated = Vec::new();
        let outcome = self
            .store
            .publish_native_result(
                job.run.id,
                fence,
                result.raw_document,
                payloads,
                move |id, count| async move {
                    tokio::task::spawn_blocking(move || reading.read(id, count))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                },
                |objects| {
                    allocated.extend(objects.iter().map(|object| object.id));
                    async move {
                        tokio::task::spawn_blocking(move || {
                            for object in objects {
                                publishing
                                    .put(object.id, &object.bytes)
                                    .map_err(|_| StoreError::Integrity)?;
                            }
                            Ok(())
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                    }
                },
            )
            .await;
        if outcome.is_err() {
            for id in allocated {
                let objects = self.objects.clone();
                if self
                    .store
                    .discard_unpublished_native_object(job.run.id, id, move |id| async move {
                        tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    })
                    .await
                    .is_err()
                {
                    tracing::warn!(artifact_id=%id, "native result cleanup deferred; unknown references retained");
                }
            }
        }
        outcome?;
        Ok(true)
    }
}
