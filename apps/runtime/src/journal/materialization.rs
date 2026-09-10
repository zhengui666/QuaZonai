//! Reserve native cache copies before writing them; release only after durable terminal cleanup.
use super::*;
use std::collections::BTreeSet;

impl Journal {
    pub async fn reserve_materialization(&self, spec: &JobSpecV1) -> Result<u64> {
        self.account_materialization(spec, false).await
    }

    /// Startup-only reconciliation of an already existing, exactly bound native
    /// directory. Existing bytes must count even if a lowered quota is exceeded.
    pub(crate) async fn recover_materialization(&self, spec: &JobSpecV1) -> Result<u64> {
        self.account_materialization(spec, true).await
    }

    async fn account_materialization(&self, spec: &JobSpecV1, existing: bool) -> Result<u64> {
        let document = request_document(spec)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, &spec.external_job_id)
            .await?
            .ok_or(Failure::Missing)?;
        if row.spec_json.as_deref() != Some(document.as_str()) {
            return Err(Failure::Conflict);
        }
        if !existing
            && (row.phase == "TERMINAL"
                || row.cancel_requested_us.is_some()
                || row.stop_code.is_some())
        {
            return Err(Failure::NotReady);
        }
        let mut copied = 0u64;
        let mut identifiers = BTreeSet::new();
        for input in &spec.inputs {
            if let RuntimeInputV1::Artifact {
                artifact_id,
                storage_version,
                byte_count,
                ..
            } = input
            {
                let observed: Option<(String, i64)> = sqlx::query_as(
                    "SELECT storage_version,byte_count FROM input_objects WHERE id=?",
                )
                .bind(artifact_id.to_string())
                .fetch_optional(&mut *tx)
                .await?;
                if !identifiers.insert(*artifact_id)
                    || observed != Some((storage_version.clone(), byte_count.get() as i64))
                {
                    return Err(Failure::Invalid("input_object_binding"));
                }
                copied = copied
                    .checked_add(byte_count.get())
                    .ok_or(Failure::Capacity)?;
            }
        }
        if identifiers.insert(spec.parameters_artifact_id) {
            let size: i64 = sqlx::query_scalar("SELECT byte_count FROM input_objects WHERE id=?")
                .bind(spec.parameters_artifact_id.to_string())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Failure::Missing)?;
            copied = copied
                .checked_add(u64::try_from(size).map_err(|_| Failure::Integrity)?)
                .ok_or(Failure::Capacity)?;
        }
        if copied > boundary::MAX_INPUT_OBJECTS_BYTES {
            return Err(Failure::Capacity);
        }
        let amount = copied
            .checked_add(document.len() as u64)
            .and_then(|bytes| bytes.checked_add(spec.limits.output_bytes.get()))
            .and_then(|bytes| bytes.checked_add(boundary::MAX_RESULT_MANIFEST_BYTES as u64))
            .ok_or(Failure::Capacity)?;
        let amount_i64 = i64::try_from(amount).map_err(|_| Failure::Capacity)?;
        let old: Option<i64> = sqlx::query_scalar(
            "SELECT byte_count FROM materialization_reservations WHERE external_id=?",
        )
        .bind(&spec.external_job_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(old) = old {
            if old != amount_i64 {
                return Err(Failure::Integrity);
            }
        } else {
            if !existing {
                self.capacity(&mut tx, amount_i64).await?;
            }
            sqlx::query("INSERT INTO materialization_reservations(external_id,byte_count,reserved_us) VALUES(?,?,?)")
                .bind(&spec.external_job_id).bind(amount_i64).bind(now().timestamp_micros())
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(amount)
    }

    /// The caller has deleted/fsynced both derived cache directories, or proved
    /// they are absent. Keep this separate from terminal publication so a crash
    /// never makes on-disk copies invisible to the quota.
    pub(crate) async fn release_materialization(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = Self::find(&mut tx, id).await?.ok_or(Failure::Missing)?;
        if row.phase != "TERMINAL" {
            return Err(Failure::NotReady);
        }
        sqlx::query("DELETE FROM materialization_reservations WHERE external_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn terminal_materializations(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT m.external_id FROM materialization_reservations m JOIN runtime_jobs j ON j.external_id=m.external_id WHERE j.phase='TERMINAL' ORDER BY m.reserved_us,m.external_id LIMIT 128")
            .fetch_all(&self.pool).await?)
    }

    pub async fn materialization_bytes(&self, id: &str) -> Result<Option<u64>> {
        boundary::parse_external_id(id).map_err(|_| Failure::Invalid("external_job_id"))?;
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT byte_count FROM materialization_reservations WHERE external_id=?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        value
            .map(|bytes| u64::try_from(bytes).map_err(|_| Failure::Integrity))
            .transpose()
    }
}
