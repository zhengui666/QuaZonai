//! Reconcile native identities; never retry an uncertain START or invent a stopped process.
use crate::{
    config::{RegisteredCatalog, RuntimeConfig},
    engine::{container_name, ContainerObservation, NativeEngine},
    files::{self, RuntimeRoot},
    journal::{Journal, NativeJob},
    materialize, now, Failure, Result,
};
use bollard::models::ContainerCreateBody;
use chrono::{DateTime, Utc};
use contracts::{runtime_jobs::*, DbCounter, SchemaV1};
use futures_util::{stream, StreamExt};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::{watch, Notify};

pub struct RuntimeService {
    pub(crate) config: Arc<RuntimeConfig>,
    pub(crate) catalogs: Vec<RegisteredCatalog>,
    pub(crate) journal: Journal,
    pub(crate) root: Arc<RuntimeRoot>,
    pub(crate) engine: NativeEngine,
    wakeup: Notify,
    // Only guards callers of poll_once inside this process. The native file lock
    // retained by RuntimeRoot provides the cross-process ownership boundary.
    poll: tokio::sync::Mutex<()>,
}

fn instant(micros: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_micros(micros).ok_or(Failure::Integrity)
}
fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(|_| Failure::Integrity)
}
fn launch(row: &NativeJob) -> Result<ContainerCreateBody> {
    Ok(serde_json::from_str(
        row.launch_json.as_deref().ok_or(Failure::Integrity)?,
    )?)
}
fn versions(row: &NativeJob) -> Result<BTreeMap<String, String>> {
    let Some(_) = row.launch_json else {
        return Ok(BTreeMap::from([(
            "runtime".into(),
            env!("CARGO_PKG_VERSION").into(),
        )]));
    };
    let native = launch(row)?;
    let value = native
        .labels
        .as_ref()
        .and_then(|labels| labels.get("io.quazonai.engine-versions"))
        .ok_or(Failure::Integrity)?;
    Ok(serde_json::from_str(value)?)
}
fn failure_manifest(
    row: &NativeJob,
    reason: Option<RuntimeFailureCode>,
    finished: DateTime<Utc>,
) -> Result<ResultManifestV1> {
    let spec = row.spec()?;
    let started = row.started_us.map(instant).transpose()?;
    let elapsed = started.map_or(0, |start| {
        (finished - start).num_milliseconds().max(0) as u64
    });
    Ok(ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        external_job_id: spec.external_job_id,
        input_set_id: spec.input_set_id,
        state: if reason.is_some() {
            RuntimeResultState::Failed
        } else {
            RuntimeResultState::Cancelled
        },
        engine_versions: versions(row)?,
        started_at: started,
        finished_at: finished,
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: count(elapsed)?,
            cpu_nanoseconds: None,
            peak_memory_bytes: None,
            output_bytes: DbCounter::ZERO,
        },
        artifacts: Vec::new(),
        error: reason.map(domain::runtime_jobs::error),
    })
}

impl RuntimeService {
    pub async fn open(config: RuntimeConfig) -> Result<Arc<Self>> {
        let catalogs = config.validate()?;
        let root = Arc::new(RuntimeRoot::open(&config.state_dir)?);
        let path = root.path.join("journal.sqlite");
        if path.try_exists()? {
            let metadata = std::fs::symlink_metadata(&path)?;
            use std::os::unix::fs::MetadataExt;
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1 {
                return Err(Failure::Invalid("native_journal_file"));
            }
        }
        let journal =
            Journal::open(&path, config.storage_quota_bytes, config.max_pending_jobs).await?;
        let config = Arc::new(config);
        let engine = NativeEngine::new(config.clone())?;
        Ok(Arc::new(Self {
            config,
            catalogs,
            journal,
            root,
            engine,
            wakeup: Notify::new(),
            poll: tokio::sync::Mutex::new(()),
        }))
    }

    pub fn notify(&self) {
        self.wakeup.notify_one();
    }
    pub fn journal(&self) -> &Journal {
        &self.journal
    }
    pub fn instance_id(&self) -> contracts::Id {
        self.journal.instance_id
    }

    /// Durable journal discovery, not a second research queue. Already created
    /// native jobs and cancellations are always reconciled before new launches.
    pub async fn poll_once(&self) -> Result<usize> {
        let _owner = self.poll.lock().await;
        let pending = self.journal.scheduling().await?;
        let active = pending
            .iter()
            .filter(|(_, phase, _)| phase != "QUEUED")
            .count();
        let mut slots = (self.config.max_parallel_jobs as usize).saturating_sub(active);
        let chosen: Vec<_> = pending
            .into_iter()
            .filter_map(|(id, phase, stopping)| {
                if phase != "QUEUED" || stopping {
                    return Some(id);
                }
                if slots > 0 {
                    slots -= 1;
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        let length = chosen.len();
        let mut results = stream::iter(chosen.into_iter().map(|id| async move {
            let result = self.drive(&id).await;
            (id, result)
        }))
        .buffer_unordered(8);
        while let Some((id, result)) = results.next().await {
            if let Err(error) = result {
                // Display is closed/static; native errors and payloads never appear here.
                tracing::warn!(external_id = %id, code = %error, "native reconciliation deferred");
            }
        }
        Ok(length)
    }

    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        loop {
            if *shutdown.borrow() {
                break;
            }
            self.poll_once().await?;
            tokio::select! {
                changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break; } }
                _ = self.wakeup.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
            }
        }
        // Do not abort native jobs on gateway shutdown. Their native timeout,
        // cgroup and stable identity survive for the next process to reconcile.
        Ok(())
    }

    async fn complete_without_launch(
        &self,
        row: &NativeJob,
        reason: Option<RuntimeFailureCode>,
    ) -> Result<()> {
        if row.launch_json.is_some() {
            return Err(Failure::Integrity);
        }
        let manifest = failure_manifest(row, reason, now())?;
        self.journal
            .finish(&row.external_id, manifest, vec![])
            .await?;
        Ok(())
    }

    async fn drive(&self, id: &str) -> Result<()> {
        let mut row = self.journal.get(id).await?;
        if row.phase == "TERMINAL" {
            return Ok(());
        }
        let spec = row.spec()?;
        if now() >= spec.deadline_at && row.stop_code.is_none() && row.cancel_requested_us.is_none()
        {
            // Gateway downtime does not invalidate a process that actually finished
            // within its native deadline. Inspect first, without restarting anything.
            if let Some(container) = row.container_id.as_deref() {
                if let Some(observed) = self
                    .engine
                    .inspect(container, self.journal.instance_id, &spec)
                    .await?
                {
                    if observed.role == "JOB"
                        && !observed.running
                        && !observed.created_only
                        && observed
                            .finished_at
                            .is_some_and(|finished| finished <= spec.deadline_at)
                    {
                        if let Some(started) = observed.started_at {
                            self.journal.observe_started(id, started).await?;
                            row = self.journal.get(id).await?;
                        }
                        return self.adopt_exited(row, observed).await;
                    }
                }
            }
            self.journal
                .request_stop(id, RuntimeFailureCode::DeadlineExceeded)
                .await?;
            row = self.journal.get(id).await?;
        }
        if row.cancel_requested_us.is_some() || row.stop_code.is_some() {
            return self.stop(row).await;
        }
        if row.launch_json.is_none() {
            // All files are materialized before native CREATE becomes possible.
            // The next iteration reads the same durable launch after any crash.
            let image = self.engine.image(&spec.image_ref).await?;
            let inputs =
                match materialize::inputs(self.root.clone(), &self.journal, &spec, &self.catalogs)
                    .await
                {
                    Ok(inputs) => inputs,
                    Err(Failure::Invalid(_)) => {
                        return self
                            .complete_without_launch(&row, Some(RuntimeFailureCode::InvalidInput))
                            .await
                    }
                    Err(Failure::Missing) => {
                        return self
                            .complete_without_launch(
                                &row,
                                Some(RuntimeFailureCode::InputUnavailable),
                            )
                            .await
                    }
                    Err(error) => return Err(error),
                };
            let native = NativeEngine::launch(
                self.journal.instance_id,
                &spec,
                &image,
                inputs.mounts,
                false,
            )?;
            if !self.journal.prepare_launch(id, &native).await? {
                return Ok(());
            }
            row = self.journal.get(id).await?;
        }
        if row.container_id.is_none() {
            let name = container_name(self.journal.instance_id, &spec);
            let observed = self
                .engine
                .inspect(&name, self.journal.instance_id, &spec)
                .await?;
            if let Some(observed) = observed {
                if observed.role != "JOB" {
                    return Err(Failure::Integrity);
                }
                self.journal.bind_container(id, &observed.id).await?;
            } else {
                // Repeating CREATE uses the same native name and exact stored body.
                // It cannot create two runnable identities; START is a separate gate.
                if let Some(container) = self.engine.create(&name, launch(&row)?).await? {
                    self.journal.bind_container(id, &container).await?;
                }
            }
            return Ok(());
        }
        let container = row.container_id.as_deref().ok_or(Failure::Integrity)?;
        let observed = self
            .engine
            .inspect(container, self.journal.instance_id, &spec)
            .await?;
        let Some(observed) = observed else {
            // A missing daemon object after CREATE/START is not proof that a delayed
            // request cannot execute. Close the name with a native barrier first.
            self.journal
                .request_stop(id, RuntimeFailureCode::EngineUnavailable)
                .await?;
            return Ok(());
        };
        if observed.role != "JOB" {
            return Err(Failure::Integrity);
        }
        if let Some(started) = observed.started_at {
            self.journal.observe_started(id, started).await?;
            row = self.journal.get(id).await?;
        }
        if observed.running {
            if observed.started_at.is_some_and(|started| {
                now() >= started + chrono::Duration::seconds(i64::from(spec.limits.wall_seconds))
            }) {
                self.journal
                    .request_stop(id, RuntimeFailureCode::DeadlineExceeded)
                    .await?;
                return Ok(());
            }
            let usage = self.engine.usage(&observed.id).await?;
            if usage.cpu_nanoseconds.is_some_and(|used| {
                u128::from(used) > u128::from(spec.limits.cpu_seconds.get()) * 1_000_000_000
            }) {
                self.journal
                    .request_stop(id, RuntimeFailureCode::CpuLimit)
                    .await?;
                return Ok(());
            }
            let root = self.root.clone();
            let spec = spec.clone();
            let usage = tokio::task::spawn_blocking(move || {
                files::output_usage(
                    &root.job(spec.run_id, spec.attempt_no).join("output"),
                    spec.limits.output_bytes.get() + 1024 * 1024,
                )
            })
            .await
            .map_err(|_| Failure::Integrity)?;
            if matches!(usage, Err(Failure::Capacity | Failure::Invalid(_))) {
                self.journal
                    .request_stop(id, RuntimeFailureCode::OutputLimit)
                    .await?;
            } else {
                usage?;
            }
            return Ok(());
        }
        if observed.created_only {
            if row.start_intent_us.is_none() {
                if self.journal.start_intent(id).await? {
                    self.engine.start(&observed.id).await?;
                }
            } else if row
                .start_intent_us
                .is_some_and(|sent| now().timestamp_micros() - sent >= 10_000_000)
            {
                // Never resend an unknown START. The original ID must be closed
                // before control-plane retry policy may choose another Attempt.
                self.journal
                    .request_stop(id, RuntimeFailureCode::EngineUnavailable)
                    .await?;
            }
            return Ok(());
        }
        self.adopt_exited(row, observed).await
    }

    async fn adopt_exited(&self, row: NativeJob, observed: ContainerObservation) -> Result<()> {
        if observed.running || observed.created_only {
            return Err(Failure::Integrity);
        }
        let spec = row.spec()?;
        let finished = observed.finished_at.ok_or(Failure::Integrity)?;
        let reason = if observed.oom_killed {
            Some(RuntimeFailureCode::MemoryLimit)
        } else if observed.exit_code == Some(124)
            || finished > spec.deadline_at
            || observed.started_at.is_some_and(|started| {
                finished - started > chrono::Duration::seconds(i64::from(spec.limits.wall_seconds))
            })
        {
            Some(RuntimeFailureCode::DeadlineExceeded)
        } else if observed.exit_code != Some(0) {
            Some(RuntimeFailureCode::NativeJobFailed)
        } else {
            None
        };
        if let Some(reason) = reason {
            let manifest = failure_manifest(&row, Some(reason), finished)?;
            self.journal
                .finish(&row.external_id, manifest, vec![])
                .await?;
            return Ok(());
        }
        let root = self.root.clone();
        let request = spec.clone();
        let outputs =
            match tokio::task::spawn_blocking(move || materialize::outputs(&root, &request))
                .await
                .map_err(|_| Failure::Integrity)?
            {
                Ok(outputs) => outputs,
                Err(_) => {
                    let manifest =
                        failure_manifest(&row, Some(RuntimeFailureCode::InvalidOutput), finished)?;
                    self.journal
                        .finish(&row.external_id, manifest, vec![])
                        .await?;
                    return Ok(());
                }
            };
        let mut manifest = failure_manifest(&row, None, finished)?;
        manifest.state = RuntimeResultState::Succeeded;
        manifest.artifacts = outputs
            .iter()
            .map(|(metadata, _)| metadata.clone())
            .collect();
        let output_bytes = outputs
            .iter()
            .try_fold(0u64, |sum, (_, bytes)| sum.checked_add(bytes.len() as u64))
            .ok_or(Failure::Integrity)?;
        manifest.resource_usage.output_bytes = count(output_bytes)?;
        // No exact post-exit cgroup observation is exposed by Docker's stats API.
        // Retain null rather than labeling a previous sample as final CPU or peak.
        self.journal
            .finish(&row.external_id, manifest, outputs)
            .await?;
        Ok(())
    }

    async fn stop(&self, mut row: NativeJob) -> Result<()> {
        let spec = row.spec()?;
        let reason = row
            .stop_code
            .as_ref()
            .map(|code| {
                serde_json::from_value::<RuntimeFailureCode>(serde_json::Value::String(
                    code.clone(),
                ))
            })
            .transpose()?;
        if row.launch_json.is_none() {
            return self.complete_without_launch(&row, reason).await;
        }
        let name = container_name(self.journal.instance_id, &spec);
        if let Some(observed) = self
            .engine
            .inspect(&name, self.journal.instance_id, &spec)
            .await?
        {
            if observed.role == "TOMBSTONE" {
                self.journal
                    .bind_barrier(&row.external_id, &observed.id)
                    .await?;
                row = self.journal.get(&row.external_id).await?;
                let manifest = failure_manifest(&row, reason, now())?;
                self.journal
                    .finish(&row.external_id, manifest, vec![])
                    .await?;
                return Ok(());
            }
            if row.container_id.as_deref() == Some(observed.id.as_str()) {
                if let Some(started) = observed.started_at {
                    self.journal
                        .observe_started(&row.external_id, started)
                        .await?;
                }
            }
            if observed.running {
                self.engine.kill(&observed.id).await?;
            } else {
                // Native non-force removal loses a race to a late START safely:
                // it fails while running, so no terminal cancellation is published.
                self.engine.remove_stopped(&observed.id).await?;
            }
            return Ok(());
        }
        // A permanent non-runnable name reservation fences late CREATE and the
        // removed old container ID fences late START. This object is never started.
        let mut barrier = launch(&row)?;
        barrier
            .labels
            .as_mut()
            .ok_or(Failure::Integrity)?
            .insert("io.quazonai.role".into(), "TOMBSTONE".into());
        barrier.cmd = Some(vec!["--version".into()]);
        barrier
            .host_config
            .as_mut()
            .ok_or(Failure::Integrity)?
            .mounts = Some(vec![]);
        if let Some(id) = self.engine.create(&name, barrier).await? {
            self.journal.bind_barrier(&row.external_id, &id).await?;
        }
        Ok(())
    }
}
