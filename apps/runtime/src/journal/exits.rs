//! Immutable observations of the original native process, captured before removal.
use super::*;
use crate::engine::ContainerObservation;

#[derive(sqlx::FromRow)]
struct NativeExit {
    container_id: String,
    started_us: i64,
    finished_us: i64,
    exit_code: i64,
    oom_killed: bool,
    failure_code: Option<String>,
}

impl Journal {
    pub async fn record_native_exit(
        &self,
        id: &str,
        observed: &ContainerObservation,
    ) -> Result<Option<RuntimeFailureCode>> {
        if observed.role != "JOB" || observed.running || observed.created_only {
            return Err(Failure::Invalid("native_exit_observation"));
        }
        native_id(&observed.id)?;
        let started = observed.started_at.ok_or(Failure::Integrity)?;
        let finished = observed.finished_at.ok_or(Failure::Integrity)?;
        let exit_code = observed.exit_code.ok_or(Failure::Integrity)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        let spec = row.spec()?;
        if row.container_id.as_deref() != Some(observed.id.as_str())
            || row.started_us != Some(started.timestamp_micros())
            || finished < started
            || finished > now() + chrono::Duration::seconds(5)
        {
            return Err(Failure::Integrity);
        }
        let reason = if observed.oom_killed {
            Some(RuntimeFailureCode::MemoryLimit)
        } else if exit_code == 124
            || finished > spec.deadline_at
            || finished - started > chrono::Duration::seconds(i64::from(spec.limits.wall_seconds))
        {
            Some(RuntimeFailureCode::DeadlineExceeded)
        } else if exit_code != 0 {
            Some(RuntimeFailureCode::NativeJobFailed)
        } else {
            None
        };
        let reason_code = reason.as_ref().map(code).transpose()?;
        let previous: Option<NativeExit> = sqlx::query_as("SELECT container_id,started_us,finished_us,exit_code,oom_killed,failure_code FROM native_exit_observations WHERE external_id=?")
            .bind(id).fetch_optional(&mut *tx).await?;
        if let Some(previous) = previous {
            if previous.container_id != observed.id
                || previous.started_us != started.timestamp_micros()
                || previous.finished_us != finished.timestamp_micros()
                || previous.exit_code != exit_code
                || previous.oom_killed != observed.oom_killed
                || previous.failure_code != reason_code
            {
                return Err(Failure::Conflict);
            }
        } else {
            if row.phase == "TERMINAL" {
                return Err(Failure::NotReady);
            }
            sqlx::query("INSERT INTO native_exit_observations(external_id,container_id,started_us,finished_us,exit_code,oom_killed,failure_code,observed_us) VALUES(?,?,?,?,?,?,?,?)")
                .bind(id).bind(&observed.id).bind(started.timestamp_micros()).bind(finished.timestamp_micros())
                .bind(exit_code).bind(observed.oom_killed).bind(reason_code).bind(now().timestamp_micros())
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(reason)
    }

    pub(super) async fn failure_before_cancellation(
        tx: &mut Transaction<'_, Sqlite>,
        row: &NativeJob,
    ) -> Result<Option<(RuntimeFailureCode, DateTime<Utc>)>> {
        let Some(cancelled) = row.cancel_requested_us else {
            return Ok(None);
        };
        let observed: Option<NativeExit> = sqlx::query_as("SELECT container_id,started_us,finished_us,exit_code,oom_killed,failure_code FROM native_exit_observations WHERE external_id=? AND finished_us<? AND failure_code IS NOT NULL")
            .bind(&row.external_id).bind(cancelled).fetch_optional(&mut **tx).await?;
        observed
            .map(|exit| {
                let reason = serde_json::from_value(serde_json::Value::String(
                    exit.failure_code.ok_or(Failure::Integrity)?,
                ))?;
                Ok((reason, instant(exit.finished_us)?))
            })
            .transpose()
    }
}
