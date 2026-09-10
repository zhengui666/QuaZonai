//! Durable native identities and exact payloads. No research budget or approval authority.
use crate::{now, Failure, Result};
use chrono::{DateTime, Utc};
use contracts::{runtime::RuntimeCapabilitiesV1, runtime_jobs::*, DbCounter, Id, SchemaV1};
use domain::runtime_jobs as boundary;
use serde::Serialize;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    ConnectOptions, Row, Sqlite, SqlitePool, Transaction,
};
use std::{path::Path, time::Duration};

mod exits;
mod materialization;

#[derive(Clone)]
pub struct Journal {
    pool: SqlitePool,
    pub instance_id: Id,
    quota: i64,
    max_pending: i64,
}

#[derive(Clone, sqlx::FromRow)]
pub struct NativeJob {
    pub external_id: String,
    pub run_id: String,
    pub attempt_no: i64,
    pub owner_epoch: i64,
    pub spec_json: Option<String>,
    pub submitted_us: i64,
    pub deadline_us: i64,
    pub phase: String,
    pub launch_json: Option<String>,
    pub container_id: Option<String>,
    pub start_intent_us: Option<i64>,
    pub started_us: Option<i64>,
    pub cancel_requested_us: Option<i64>,
    pub cancel_owner_epoch: Option<i64>,
    pub stop_code: Option<String>,
    pub barrier_id: Option<String>,
    pub terminal_state: Option<String>,
    pub finished_us: Option<i64>,
    pub manifest_json: Option<String>,
    pub output_reservation: i64,
}

fn request_document(spec: &JobSpecV1) -> Result<String> {
    let identity = boundary::external_id(spec.run_id, spec.attempt_no)
        .map_err(|_| Failure::Invalid("external_job_id"))?;
    if identity != spec.external_job_id {
        return Err(Failure::Invalid("external_job_id"));
    }
    let document = serde_json::to_string(spec)?;
    if document.len() > boundary::MAX_JOB_REQUEST_BYTES {
        return Err(Failure::Invalid("job_size"));
    }
    Ok(document)
}
fn replay_status(row: &NativeJob, document: &str) -> Result<RuntimeJobStatusV1> {
    // A pre-submit tombstone permanently closes the identity even without a spec.
    if row.spec_json.as_deref().is_some_and(|old| old != document) {
        return Err(Failure::Conflict);
    }
    row.status()
}

fn instant(value: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_micros(value).ok_or(Failure::Integrity)
}
fn code(value: &impl Serialize) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or(Failure::Integrity)
}
fn native_id(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Failure::Integrity);
    }
    Ok(())
}
impl NativeJob {
    pub fn spec(&self) -> Result<JobSpecV1> {
        Ok(serde_json::from_str(
            self.spec_json.as_deref().ok_or(Failure::Integrity)?,
        )?)
    }
    pub fn status(&self) -> Result<RuntimeJobStatusV1> {
        let state = if let Some(state) = &self.terminal_state {
            serde_json::from_value(serde_json::Value::String(state.clone()))?
        } else if self.cancel_requested_us.is_some() {
            RuntimeJobState::CancelRequested
        } else if self.started_us.is_some() {
            RuntimeJobState::Running
        } else {
            RuntimeJobState::Accepted
        };
        Ok(RuntimeJobStatusV1 {
            schema_version: SchemaV1,
            run_id: self
                .run_id
                .clone()
                .try_into()
                .map_err(|_| Failure::Integrity)?,
            attempt_no: u32::try_from(self.attempt_no).map_err(|_| Failure::Integrity)?,
            external_job_id: self.external_id.clone(),
            state,
            has_result: self.manifest_json.is_some(),
            submitted_at: instant(self.submitted_us)?,
            started_at: self.started_us.map(instant).transpose()?,
            finished_at: self.finished_us.map(instant).transpose()?,
        })
    }
}

impl Journal {
    pub async fn open(path: &Path, quota_bytes: u64, max_pending: u32) -> Result<Self> {
        if !(64 * 1024 * 1024..=1_099_511_627_776).contains(&quota_bytes)
            || !(1..=4096).contains(&max_pending)
        {
            return Err(Failure::Invalid("journal_limits"));
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            .disable_statement_logging();
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|_| Failure::Integrity)?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let previous: Option<String> =
            sqlx::query_scalar("SELECT instance_id FROM runtime_meta WHERE singleton=1")
                .fetch_optional(&mut *tx)
                .await?;
        let instance_id = match previous {
            Some(id) => id.try_into().map_err(|_| Failure::Integrity)?,
            None => {
                let id = Id::new();
                sqlx::query("INSERT INTO runtime_meta(singleton,instance_id) VALUES(1,?)")
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await?;
                id
            }
        };
        tx.commit().await?;
        Ok(Self {
            pool,
            instance_id,
            quota: quota_bytes as i64,
            max_pending: i64::from(max_pending),
        })
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn capacity(&self, tx: &mut Transaction<'_, Sqlite>, additional: i64) -> Result<()> {
        let occupied: i64 = sqlx::query_scalar("SELECT COALESCE((SELECT SUM(byte_count) FROM input_objects),0)+COALESCE((SELECT SUM(byte_count) FROM job_outputs),0)+COALESCE((SELECT SUM(output_reservation) FROM runtime_jobs),0)+COALESCE((SELECT SUM(byte_count) FROM materialization_reservations),0)")
            .fetch_one(&mut **tx).await?;
        if additional < 0 || occupied > self.quota.saturating_sub(additional) {
            return Err(Failure::Capacity);
        }
        Ok(())
    }

    pub async fn put_object(
        &self,
        id: Id,
        version: &str,
        bytes: &[u8],
    ) -> Result<(RuntimeObjectReceiptV1, bool)> {
        boundary::storage_version(version).map_err(|_| Failure::Invalid("storage_version"))?;
        if bytes.is_empty() || bytes.len() as u64 > boundary::MAX_INPUT_OBJECT_BYTES {
            return Err(Failure::Invalid("object_size"));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let existing = sqlx::query("SELECT storage_version,bytes FROM input_objects WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&mut *tx)
            .await?;
        let replayed = if let Some(row) = existing {
            if row.try_get::<String, _>("storage_version")? != version
                || row.try_get::<Vec<u8>, _>("bytes")? != bytes
            {
                return Err(Failure::Conflict);
            }
            true
        } else {
            self.capacity(&mut tx, bytes.len() as i64).await?;
            sqlx::query("INSERT INTO input_objects(id,storage_version,bytes,byte_count,created_us) VALUES(?,?,?,?,?)")
                .bind(id.to_string()).bind(version).bind(bytes).bind(bytes.len() as i64)
                .bind(now().timestamp_micros()).execute(&mut *tx).await?;
            false
        };
        tx.commit().await?;
        Ok((
            RuntimeObjectReceiptV1 {
                schema_version: SchemaV1,
                artifact_id: id,
                storage_version: version.to_owned(),
                byte_count: DbCounter::new(bytes.len() as u64).map_err(|_| Failure::Integrity)?,
            },
            replayed,
        ))
    }

    /// Trusted materializer only. There is deliberately no public input-download endpoint.
    pub async fn input_object(&self, id: Id) -> Result<(String, Vec<u8>)> {
        sqlx::query_as("SELECT storage_version,bytes FROM input_objects WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Failure::Missing)
    }

    /// A durable receipt remains readable while the native engine is unavailable.
    /// No caller may turn failure to read the journal into a new external identity.
    pub async fn replay(&self, spec: &JobSpecV1) -> Result<Option<RuntimeJobStatusV1>> {
        let document = request_document(spec)?;
        let mut tx = self.pool.begin().await?;
        let result = Self::find(&mut tx, &spec.external_job_id)
            .await?
            .map(|row| replay_status(&row, &document))
            .transpose()?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn submit(
        &self,
        spec: &JobSpecV1,
        capability: &RuntimeCapabilitiesV1,
    ) -> Result<(RuntimeJobStatusV1, bool)> {
        let document = request_document(spec)?;
        let identity = &spec.external_job_id;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(existing) = Self::find(&mut tx, identity).await? {
            let status = replay_status(&existing, &document)?;
            tx.commit().await?;
            return Ok((status, true));
        }
        let submitted = now();
        boundary::admit_spec(spec, capability, submitted)
            .map_err(|_| Failure::Invalid("job_not_admitted"))?;
        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM runtime_jobs WHERE phase!='TERMINAL'")
                .fetch_one(&mut *tx)
                .await?;
        if pending >= self.max_pending {
            return Err(Failure::Busy);
        }
        for input in &spec.inputs {
            if let RuntimeInputV1::Artifact {
                artifact_id,
                storage_version,
                byte_count,
                ..
            } = input
            {
                let actual: Option<(String, i64)> = sqlx::query_as(
                    "SELECT storage_version,byte_count FROM input_objects WHERE id=?",
                )
                .bind(artifact_id.to_string())
                .fetch_optional(&mut *tx)
                .await?;
                if actual != Some((storage_version.clone(), byte_count.get() as i64)) {
                    return Err(Failure::Invalid("input_object"));
                }
            }
        }
        let parameters: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM input_objects WHERE id=?)")
                .bind(spec.parameters_artifact_id.to_string())
                .fetch_one(&mut *tx)
                .await?;
        if !parameters {
            return Err(Failure::Invalid("parameters_artifact_id"));
        }
        self.capacity(&mut tx, spec.limits.output_bytes.get() as i64)
            .await?;
        sqlx::query("INSERT INTO runtime_jobs(external_id,run_id,attempt_no,owner_epoch,spec_json,submitted_us,deadline_us,phase,output_reservation) VALUES(?,?,?,?,?,?,?,'QUEUED',?)")
            .bind(identity).bind(spec.run_id.to_string()).bind(i64::from(spec.attempt_no))
            .bind(spec.owner_epoch.get() as i64).bind(document).bind(submitted.timestamp_micros())
            .bind(spec.deadline_at.timestamp_micros()).bind(spec.limits.output_bytes.get() as i64).execute(&mut *tx).await?;
        let result = Self::find(&mut tx, identity)
            .await?
            .ok_or(Failure::Integrity)?
            .status()?;
        tx.commit().await?;
        Ok((result, false))
    }

    async fn find(tx: &mut Transaction<'_, Sqlite>, id: &str) -> Result<Option<NativeJob>> {
        Ok(
            sqlx::query_as("SELECT * FROM runtime_jobs WHERE external_id=?")
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?,
        )
    }
    pub async fn get(&self, id: &str) -> Result<NativeJob> {
        boundary::parse_external_id(id).map_err(|_| Failure::Invalid("external_job_id"))?;
        sqlx::query_as("SELECT * FROM runtime_jobs WHERE external_id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Failure::Missing)
    }
    /// Read only bounded scheduling metadata; do not load thousands of 1 MiB specs.
    pub async fn scheduling(&self) -> Result<Vec<(String, String, bool)>> {
        Ok(sqlx::query_as("SELECT external_id,phase,(cancel_requested_us IS NOT NULL OR stop_code IS NOT NULL OR deadline_us<=?) FROM runtime_jobs WHERE phase!='TERMINAL' ORDER BY 3 DESC,(phase!='QUEUED') DESC,submitted_us,external_id LIMIT 4096")
            .bind(now().timestamp_micros()).fetch_all(&self.pool).await?)
    }

    pub async fn pending(&self, limit: u32) -> Result<Vec<NativeJob>> {
        if !(1..=4096).contains(&limit) {
            return Err(Failure::Invalid("limit"));
        }
        Ok(sqlx::query_as("SELECT * FROM runtime_jobs WHERE phase!='TERMINAL' ORDER BY submitted_us,external_id LIMIT ?")
            .bind(i64::from(limit)).fetch_all(&self.pool).await?)
    }

    pub async fn cancel(&self, id: &str, request: &RuntimeCancelV1) -> Result<RuntimeJobStatusV1> {
        let identity = boundary::external_id(request.run_id, request.attempt_no)
            .map_err(|_| Failure::Invalid("external_job_id"))?;
        if identity != id {
            return Err(Failure::Invalid("external_job_id"));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let time = now().timestamp_micros();
        if let Some(row) = Self::find(&mut tx, id).await? {
            if row.terminal_state.is_some() {
                let status = row.status()?;
                tx.commit().await?;
                return Ok(status);
            }
            let epoch = request.owner_epoch.get() as i64;
            if epoch < row.owner_epoch || row.cancel_owner_epoch.is_some_and(|old| epoch < old) {
                return Err(Failure::StaleOwner);
            }
            sqlx::query("UPDATE runtime_jobs SET cancel_requested_us=COALESCE(cancel_requested_us,?),cancel_owner_epoch=? WHERE external_id=?")
                .bind(time).bind(epoch).bind(id).execute(&mut *tx).await?;
        } else {
            sqlx::query("INSERT INTO runtime_jobs(external_id,run_id,attempt_no,owner_epoch,submitted_us,deadline_us,phase,cancel_requested_us,cancel_owner_epoch,terminal_state,finished_us,output_reservation) VALUES(?,?,?,?,?,?,'TERMINAL',?,?,'CANCELLED',?,0)")
                .bind(id).bind(request.run_id.to_string()).bind(i64::from(request.attempt_no))
                .bind(request.owner_epoch.get() as i64).bind(time).bind(time).bind(time)
                .bind(request.owner_epoch.get() as i64).bind(time).execute(&mut *tx).await?;
        }
        let result = Self::find(&mut tx, id)
            .await?
            .ok_or(Failure::Integrity)?
            .status()?;
        tx.commit().await?;
        Ok(result)
    }

    /// Persist the exact native Docker request before the first possible CREATE.
    pub async fn prepare_launch(
        &self,
        id: &str,
        launch: &bollard::models::ContainerCreateBody,
    ) -> Result<bool> {
        let document = serde_json::to_string(launch)?;
        if document.len() > 2 * 1024 * 1024 {
            return Err(Failure::Invalid("native_launch_size"));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.phase == "TERMINAL" || row.cancel_requested_us.is_some() || row.stop_code.is_some() {
            tx.commit().await?;
            return Ok(false);
        }
        if let Some(previous) = row.launch_json {
            if previous != document {
                return Err(Failure::Conflict);
            }
            tx.commit().await?;
            return Ok(false);
        }
        if row.phase != "QUEUED" {
            return Err(Failure::Integrity);
        }
        sqlx::query("UPDATE runtime_jobs SET launch_json=?,phase='CREATING' WHERE external_id=?")
            .bind(document)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn bind_container(&self, id: &str, container: &str) -> Result<()> {
        native_id(container)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.container_id.as_deref() == Some(container) {
            tx.commit().await?;
            return Ok(());
        }
        if row.container_id.is_some() || row.phase != "CREATING" || row.launch_json.is_none() {
            return Err(Failure::Integrity);
        }
        sqlx::query("UPDATE runtime_jobs SET container_id=?,phase='CREATED' WHERE external_id=?")
            .bind(container)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// True authorizes one native START. A persisted old intent never authorizes a restart.
    pub async fn start_intent(&self, id: &str) -> Result<bool> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.phase == "TERMINAL"
            || row.start_intent_us.is_some()
            || row.cancel_requested_us.is_some()
            || row.stop_code.is_some()
        {
            tx.commit().await?;
            return Ok(false);
        }
        if row.phase != "CREATED" || row.container_id.is_none() {
            return Err(Failure::Integrity);
        }
        sqlx::query(
            "UPDATE runtime_jobs SET start_intent_us=?,phase='STARTING' WHERE external_id=?",
        )
        .bind(now().timestamp_micros())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn observe_started(&self, id: &str, started: DateTime<Utc>) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.phase == "TERMINAL" {
            tx.commit().await?;
            return Ok(());
        }
        if row.start_intent_us.is_none()
            || started.timestamp_micros() < row.submitted_us
            || started > now() + chrono::Duration::seconds(5)
        {
            return Err(Failure::Integrity);
        }
        if let Some(old) = row.started_us {
            if old != started.timestamp_micros() {
                return Err(Failure::Integrity);
            }
        } else {
            sqlx::query("UPDATE runtime_jobs SET started_us=?,phase='RUNNING' WHERE external_id=?")
                .bind(started.timestamp_micros())
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn request_stop(&self, id: &str, reason: RuntimeFailureCode) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.phase != "TERMINAL" && row.stop_code.is_none() {
            sqlx::query("UPDATE runtime_jobs SET stop_code=? WHERE external_id=?")
                .bind(code(&reason)?)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn bind_barrier(&self, id: &str, container: &str) -> Result<()> {
        native_id(container)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.phase == "TERMINAL" || row.barrier_id.as_deref() == Some(container) {
            tx.commit().await?;
            return Ok(());
        }
        if row.barrier_id.is_some()
            || row.container_id.as_deref() == Some(container)
            || (row.cancel_requested_us.is_none() && row.stop_code.is_none())
        {
            return Err(Failure::Integrity);
        }
        sqlx::query("UPDATE runtime_jobs SET barrier_id=? WHERE external_id=?")
            .bind(container)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The native engine has confirmed the process stopped before calling this.
    /// Payloads and the unique public terminal fact commit in one SQLite transaction.
    pub async fn finish(
        &self,
        id: &str,
        mut manifest: ResultManifestV1,
        mut outputs: Vec<(RuntimeOutputV1, Vec<u8>)>,
    ) -> Result<RuntimeJobStatusV1> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.phase == "TERMINAL" {
            let status = row.status()?;
            tx.commit().await?;
            return Ok(status);
        }
        let spec = row.spec()?;
        if row.launch_json.is_some()
            && (row.cancel_requested_us.is_some() || row.stop_code.is_some())
            && row.barrier_id.is_none()
        {
            // Cancellation can race output parsing. The caller must close the native
            // identity before publishing a stopped result; no JSON assertion is proof.
            return Err(Failure::NotReady);
        }
        if manifest.started_at.map(|time| time.timestamp_micros()) != row.started_us {
            return Err(Failure::Integrity);
        }
        if row.launch_json.is_some() && row.container_id.is_none() && row.barrier_id.is_none() {
            return Err(Failure::Integrity);
        }
        let earlier_failure = Self::failure_before_cancellation(&mut tx, &row).await?;
        if let Some((reason, finished)) = earlier_failure.filter(|_| row.stop_code.is_none()) {
            // The original process had already failed before cancellation was
            // requested. Its durable observation survives removal and restart.
            manifest.state = RuntimeResultState::Failed;
            manifest.error = Some(boundary::error(reason));
            manifest.finished_at = finished;
            let elapsed = manifest.started_at.map_or(0, |started| {
                (finished - started).num_milliseconds().max(0) as u64
            });
            manifest.resource_usage.wall_milliseconds =
                DbCounter::new(elapsed).map_err(|_| Failure::Integrity)?;
            manifest.artifacts.clear();
            outputs.clear();
            manifest.resource_usage.output_bytes = DbCounter::ZERO;
        } else if let Some(reason) = row.stop_code {
            let reason: RuntimeFailureCode =
                serde_json::from_value(serde_json::Value::String(reason))?;
            manifest.state = RuntimeResultState::Failed;
            manifest.error = Some(boundary::error(reason));
            manifest.artifacts.clear();
            outputs.clear();
            manifest.resource_usage.output_bytes =
                DbCounter::new(0).map_err(|_| Failure::Integrity)?;
        } else if row.cancel_requested_us.is_some() {
            manifest.state = RuntimeResultState::Cancelled;
            manifest.error = None;
            manifest.finished_at = now();
            let elapsed = manifest.started_at.map_or(0, |started| {
                (manifest.finished_at - started).num_milliseconds().max(0) as u64
            });
            manifest.resource_usage.wall_milliseconds =
                DbCounter::new(elapsed).map_err(|_| Failure::Integrity)?;
            manifest.artifacts.clear();
            outputs.clear();
            manifest.resource_usage.output_bytes =
                DbCounter::new(0).map_err(|_| Failure::Integrity)?;
        }
        boundary::manifest(&manifest, &spec, instant(row.submitted_us)?, now())
            .map_err(|_| Failure::Invalid("result_manifest"))?;
        if manifest.external_job_id != id || outputs.len() != manifest.artifacts.len() {
            return Err(Failure::Integrity);
        }
        for ((metadata, bytes), expected) in outputs.iter().zip(&manifest.artifacts) {
            if serde_json::to_value(metadata)? != serde_json::to_value(expected)?
                || bytes.len() as u64 != metadata.byte_count.get()
            {
                return Err(Failure::Integrity);
            }
            sqlx::query("INSERT INTO job_outputs(external_id,storage_ref,metadata_json,bytes,byte_count) VALUES(?,?,?,?,?)")
                .bind(id).bind(metadata.storage_ref.to_string()).bind(serde_json::to_string(metadata)?)
                .bind(bytes).bind(bytes.len() as i64).execute(&mut *tx).await?;
        }
        let document = serde_json::to_string(&manifest)?;
        if document.len() > boundary::MAX_RESULT_MANIFEST_BYTES {
            return Err(Failure::Invalid("manifest_size"));
        }
        sqlx::query("UPDATE runtime_jobs SET phase='TERMINAL',terminal_state=?,finished_us=?,manifest_json=?,output_reservation=0 WHERE external_id=?")
            .bind(code(&manifest.state)?).bind(manifest.finished_at.timestamp_micros()).bind(document)
            .bind(id).execute(&mut *tx).await?;
        let result = Self::find(&mut tx, id)
            .await?
            .ok_or(Failure::Integrity)?
            .status()?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn result(&self, id: &str) -> Result<Vec<u8>> {
        let row = self.get(id).await?;
        if row.phase != "TERMINAL" {
            return Err(Failure::NotReady);
        }
        Ok(row.manifest_json.ok_or(Failure::Missing)?.into_bytes())
    }
    pub async fn output(&self, id: &str, storage: Id) -> Result<(RuntimeOutputV1, Vec<u8>)> {
        let result = self.result(id).await?;
        let manifest: ResultManifestV1 = serde_json::from_slice(&result)?;
        let expected = manifest
            .artifacts
            .iter()
            .find(|item| item.storage_ref == storage)
            .ok_or(Failure::Missing)?;
        let (metadata, bytes): (String, Vec<u8>) = sqlx::query_as(
            "SELECT metadata_json,bytes FROM job_outputs WHERE external_id=? AND storage_ref=?",
        )
        .bind(id)
        .bind(storage.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Failure::Integrity)?;
        let metadata: RuntimeOutputV1 = serde_json::from_str(&metadata)?;
        if serde_json::to_value(&metadata)? != serde_json::to_value(expected)?
            || metadata.byte_count.get() != bytes.len() as u64
        {
            return Err(Failure::Integrity);
        }
        Ok((metadata, bytes))
    }
}
