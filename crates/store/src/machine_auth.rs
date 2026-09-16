//! Reserve before native Argon2. Successful verification refunds only its own
//! window; failures, process cancellation and unknown outcomes stay bounded.
use crate::{Store, StoreError};
use chrono::{DateTime, Utc};
use contracts::Id;

pub struct MachineAuthAttempt {
    credential_id: Id,
    windows: [DateTime<Utc>; 2],
}
impl Store {
    pub async fn reserve_machine_auth_attempt(
        &self,
        credential_id: Id,
    ) -> Result<MachineAuthAttempt, StoreError> {
        let mut tx = self.native_pool().begin().await?;
        let mut windows = [DateTime::<Utc>::UNIX_EPOCH; 2];
        for (index, bucket) in [None, Some(credential_id.as_uuid())]
            .into_iter()
            .enumerate()
        {
            let started: Option<DateTime<Utc>> = sqlx::query_scalar(
                "INSERT INTO app.machine_auth_rate_windows(credential_id,window_started_at,attempts) VALUES($1,clock_timestamp(),1)
                 ON CONFLICT(credential_id) DO UPDATE SET
                 window_started_at=CASE WHEN app.machine_auth_rate_windows.window_started_at + interval '60 seconds'<=clock_timestamp() THEN clock_timestamp() ELSE app.machine_auth_rate_windows.window_started_at END,
                 attempts=CASE WHEN app.machine_auth_rate_windows.window_started_at + interval '60 seconds'<=clock_timestamp() THEN 1 ELSE app.machine_auth_rate_windows.attempts+1 END
                 WHERE app.machine_auth_rate_windows.window_started_at + interval '60 seconds'<=clock_timestamp()
                    OR app.machine_auth_rate_windows.attempts < CASE WHEN $1::uuid IS NULL THEN 32 ELSE 5 END
                 RETURNING window_started_at")
                .bind(bucket).fetch_optional(&mut *tx).await?;
            let Some(started) = started else {
                let seconds: i32 = sqlx::query_scalar("SELECT greatest(1,ceil(extract(epoch FROM window_started_at+interval '60 seconds'-clock_timestamp())))::integer FROM app.machine_auth_rate_windows WHERE credential_id IS NOT DISTINCT FROM $1")
                    .bind(bucket).fetch_one(&mut *tx).await?;
                tx.rollback().await?;
                return Err(StoreError::AuthRateLimited {
                    retry_after_seconds: seconds.clamp(1, 60) as u32,
                });
            };
            windows[index] = started;
        }
        tx.commit().await?;
        Ok(MachineAuthAttempt {
            credential_id,
            windows,
        })
    }

    pub async fn machine_auth_succeeded(
        &self,
        attempt: MachineAuthAttempt,
    ) -> Result<(), StoreError> {
        let mut tx = self.native_pool().begin().await?;
        for (bucket, window) in [None, Some(attempt.credential_id.as_uuid())]
            .into_iter()
            .zip(attempt.windows)
        {
            sqlx::query("UPDATE app.machine_auth_rate_windows SET attempts=greatest(0,attempts-1) WHERE credential_id IS NOT DISTINCT FROM $1 AND window_started_at=$2")
                .bind(bucket).bind(window).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
