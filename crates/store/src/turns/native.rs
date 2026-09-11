//! Exact public request artifacts and native recovery projections. No canonical
//! Codex history, hidden reasoning, credential or model-selected URL is stored.
use super::*;
use crate::lifecycle::native::NativeObjectPublication;
use contracts::{lifecycle::JobLimitsV1, SchemaV1};
use serde::{Deserialize, Serialize};

const MAX_DOCUMENT: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestDocument {
    schema_version: SchemaV1,
    run_id: Id,
    session_id: Id,
    attempt_id: Id,
    command_key: String,
    turn_kind: String,
    prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTurnCheckpoint {
    pub reservation: Reservation,
    pub sent: bool,
    pub native_turn_id: Option<String>,
    pub terminal: Option<TurnTerminal>,
    pub receipt: Option<UsageReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionTurnCheckpoint {
    pub latest: Option<NativeTurnCheckpoint>,
    /// Settled native total only. Outstanding reservations are not native usage.
    pub accounted_tokens: DbCounter,
}

fn prompt(value: &str) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > 256 * 1024 || value.contains('\0') {
        return Err(StoreError::Invalid("native_turn_prompt"));
    }
    Ok(())
}

async fn request_size(
    tx: &mut Tx<'_>,
    mission: &Mission,
    artifact: Id,
    attempt: Id,
) -> Result<Option<DbCounter>, StoreError> {
    let Some(row) = sqlx::query("SELECT * FROM app.artifacts WHERE id=$1 FOR SHARE")
        .bind(artifact.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
    else {
        return Ok(None);
    };
    let bytes = count(row.try_get("byte_count")?)?;
    if row.try_get::<Uuid, _>("project_id")? != mission.project_id
        || row.try_get::<Option<Uuid>, _>("producer_run_id")? != Some(mission.run_id.as_uuid())
        || row.try_get::<Option<Uuid>, _>("producer_attempt_id")? != Some(attempt.as_uuid())
        || row.try_get::<String, _>("kind")? != "PARAMETERS"
        || row.try_get::<String, _>("schema_name")? != "qz.mission_turn"
        || row.try_get::<String, _>("schema_version")? != "1"
        || row.try_get::<String, _>("media_type")? != "application/json"
        || row.try_get::<String, _>("access_class")? != "RESEARCH"
        || row.try_get::<String, _>("created_by")? != "RUNTIME"
        || row.try_get::<String, _>("origin")? != "SYNTHETIC"
        || row.try_get::<String, _>("storage_backend")? != "LOCAL"
        || row.try_get::<String, _>("storage_object_ref")? != artifact.to_string()
        || row.try_get::<String, _>("storage_version")? != "1"
        || bytes.get() == 0
        || bytes.get() > MAX_DOCUMENT
    {
        return Err(StoreError::Integrity);
    }
    Ok(Some(bytes))
}

impl Store {
    /// Native status only; retain the first DB observation time on exact replay.
    pub async fn observe_mission_turn_terminal(
        &self,
        reservation_id: Id,
        fence: &WorkerFence,
        outcome: TurnOutcome,
        reason: &str,
    ) -> Result<TurnTerminal, StoreError> {
        let mut tx = self.pool.begin().await?;
        let item = load_reservation(&mut tx, reservation_id).await?;
        let mission = lock_mission(&mut tx, item.run_id, fence).await?;
        if item.attempt_id != fence.attempt_id || outcome == TurnOutcome::NotSent {
            return Err(StoreError::Conflict);
        }
        let native_turn_id: String = sqlx::query_scalar(
            "SELECT native_turn_id::text FROM app.model_turn_bindings WHERE reservation_id=$1",
        )
        .bind(reservation_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::Integrity)?;
        let prior = load_terminal(&mut tx, reservation_id).await?;
        let terminal = TurnTerminal {
            outcome,
            native_turn_id: Some(native_turn_id),
            reason_code: reason.into(),
            observed_at: prior.map_or(mission.now, |prior| prior.observed_at),
        };
        record_terminal(&mut tx, reservation_id, &terminal, mission.now).await?;
        tx.commit().await?;
        Ok(terminal)
    }

    /// Artifact metadata, reservation and PGMQ notification commit together.
    /// The supplied text is QZ's public request, not a native history export.
    pub async fn prepare_mission_turn<R, Read, P, Published>(
        &self,
        run: Id,
        fence: &WorkerFence,
        request: &TurnRequest,
        text: &str,
        read: R,
        publish: P,
    ) -> Result<Reservation, StoreError>
    where
        R: FnOnce(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        prompt(text)?;
        let mut tx = self.pool.begin().await?;
        let mission = lock_mission(&mut tx, run, fence).await?;
        let defined: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.run_missions WHERE run_id=$1)")
                .bind(run.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !defined {
            return Err(StoreError::Invalid("mission_not_defined"));
        }
        let document = RequestDocument {
            schema_version: SchemaV1,
            run_id: run,
            session_id: id(mission.session_id)?,
            attempt_id: fence.attempt_id,
            command_key: request.command_key.clone(),
            turn_kind: kind_name(request.turn_kind).into(),
            prompt: text.into(),
        };
        let bytes = serde_json::to_vec(&document).map_err(|_| StoreError::Integrity)?;
        if bytes.len() as u64 > MAX_DOCUMENT {
            return Err(StoreError::Invalid("native_turn_document"));
        }
        let artifact = request.request_artifact_id;
        let existing = request_size(&mut tx, &mission, artifact, fence.attempt_id).await?;
        if let Some(size) = existing {
            if size.get() != bytes.len() as u64 || read(artifact, size).await? != bytes {
                return Err(StoreError::Conflict);
            }
        } else {
            let limits: serde_json::Value =
                sqlx::query_scalar("SELECT limits FROM app.run_admissions WHERE run_id=$1")
                    .bind(run.as_uuid())
                    .fetch_one(&mut *tx)
                    .await?;
            let limits: JobLimitsV1 =
                serde_json::from_value(limits).map_err(|_| StoreError::Integrity)?;
            let used:i64=sqlx::query_scalar("SELECT coalesce(sum(byte_count),0)::bigint FROM app.artifacts WHERE producer_run_id=$1")
                .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
            if count(used)?
                .get()
                .checked_add(bytes.len() as u64)
                .is_none_or(|total| total > limits.output_bytes.get())
            {
                return Err(DomainError::BudgetExhausted("output_bytes").into());
            }
            sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'PARAMETERS','application/json','qz.mission_turn','1','LOCAL',$5,'1',$6,'RESEARCH','SYNTHETIC','RUNTIME','REFERENCED')")
                .bind(artifact.as_uuid()).bind(mission.project_id).bind(run.as_uuid()).bind(fence.attempt_id.as_uuid())
                .bind(artifact.to_string()).bind(bytes.len() as i64).execute(&mut *tx).await?;
        }
        // One shared admission implementation; failed admission publishes no file.
        let result = reserve_in_transaction(&mut tx, run, fence, request).await?;
        if existing.is_none() {
            publish(NativeObjectPublication {
                id: artifact,
                bytes,
            })
            .await?;
            // Local publication can outlast the lease/deadline. Preserve bytes on
            // uncertainty; the existing Run-locked orphan reconciler handles them.
            lock_mission(&mut tx, run, fence)
                .await?
                .admit(request.deadline_at)?;
        }
        tx.commit().await?;
        Ok(result)
    }

    pub async fn mission_turn_prompt<R, Read>(
        &self,
        run: Id,
        fence: &WorkerFence,
        reservation_id: Id,
        read: R,
    ) -> Result<String, StoreError>
    where
        R: FnOnce(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let mission = lock_mission(&mut tx, run, fence).await?;
        let item = load_reservation(&mut tx, reservation_id).await?;
        if item.run_id != run
            || item.session_id.as_uuid() != mission.session_id
            || item.attempt_id != fence.attempt_id
        {
            return Err(StoreError::Conflict);
        }
        let size = request_size(&mut tx, &mission, item.request_artifact_id, item.attempt_id)
            .await?
            .ok_or(StoreError::Integrity)?;
        let bytes = read(item.request_artifact_id, size).await?;
        if bytes.len() as u64 != size.get() {
            return Err(StoreError::Integrity);
        }
        let document: RequestDocument =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        let command_key: String = sqlx::query_scalar(
            "SELECT command_key::text FROM app.model_turn_reservations WHERE id=$1",
        )
        .bind(item.id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if document.run_id != run
            || document.session_id != item.session_id
            || document.attempt_id != item.attempt_id
            || document.turn_kind != kind_name(item.turn_kind)
            || document.command_key != command_key
        {
            return Err(StoreError::Integrity);
        }
        prompt(&document.prompt)?;
        lock_mission(&mut tx, run, fence).await?;
        tx.commit().await?;
        Ok(document.prompt)
    }

    /// No native request or inference of a missing ACK/usage. Configuration
    /// changes do not erase already observed work or its outstanding reservation.
    pub async fn mission_turn_checkpoint(
        &self,
        run: Id,
        fence: &WorkerFence,
    ) -> Result<MissionTurnCheckpoint, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mission = lock_mission(&mut tx, run, fence).await?;
        let sql=format!("SELECT {RESERVATION_COLUMNS} FROM app.model_turn_reservations WHERE session_id=$1 ORDER BY ordinal DESC LIMIT 1");
        let latest = if let Some(row) = sqlx::query(&sql)
            .bind(mission.session_id)
            .fetch_optional(&mut *tx)
            .await?
        {
            let item = reservation(row)?;
            let sent: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM app.model_turn_dispatches WHERE reservation_id=$1)",
            )
            .bind(item.id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
            let native_turn_id = sqlx::query_scalar(
                "SELECT native_turn_id::text FROM app.model_turn_bindings WHERE reservation_id=$1",
            )
            .bind(item.id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
            Some(NativeTurnCheckpoint {
                sent,
                native_turn_id,
                terminal: load_terminal(&mut tx, item.id).await?,
                receipt: receipt(&mut tx, item.id).await?,
                reservation: item,
            })
        } else {
            None
        };
        let tokens:i64=sqlx::query_scalar("SELECT coalesce(sum(t.actual_tokens),0)::bigint FROM app.model_turn_receipts t JOIN app.model_turn_reservations r ON r.id=t.reservation_id WHERE r.session_id=$1")
            .bind(mission.session_id).fetch_one(&mut *tx).await?;
        let accounted_tokens = count(tokens)?;
        tx.commit().await?;
        Ok(MissionTurnCheckpoint {
            latest,
            accounted_tokens,
        })
    }
}
