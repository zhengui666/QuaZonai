//! A bounded public answer, correlated to native terminal and usage receipts.
use super::*;
use crate::lifecycle::native::NativeObjectPublication;
use contracts::{lifecycle::JobLimitsV1, SchemaV1};
use serde::Serialize;

#[derive(Serialize)]
pub struct NativePublicSummary {
    pub schema_version: SchemaV1,
    pub native_turn_id: String,
    pub native_item_id: String,
    pub phase: Option<String>,
    pub text: String,
}

impl Store {
    pub async fn record_mission_summary<R, Read, P, Published>(
        &self,
        reservation: Id,
        fence: &WorkerFence,
        summary: &NativePublicSummary,
        read: R,
        publish: P,
    ) -> Result<Id, StoreError>
    where
        R: FnOnce(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        if !bounded(&summary.native_turn_id, 200)
            || !bounded(&summary.native_item_id, 200)
            || summary.native_turn_id.chars().any(char::is_control)
            || summary.native_item_id.chars().any(char::is_control)
            || !bounded(&summary.text, 64 * 1024)
            || summary.text.contains('\0')
            || summary
                .phase
                .as_deref()
                .is_some_and(|p| !matches!(p, "commentary" | "final_answer"))
        {
            return Err(StoreError::Invalid("native_public_summary"));
        }
        let mut tx = self.pool.begin().await?;
        let item = load_reservation(&mut tx, reservation).await?;
        let mission = lock_mission(&mut tx, item.run_id, fence).await?;
        if item.attempt_id != fence.attempt_id {
            return Err(DomainError::StaleAttempt.into());
        }
        let terminal = load_terminal(&mut tx, reservation)
            .await?
            .ok_or(StoreError::Integrity)?;
        let usage = receipt(&mut tx, reservation)
            .await?
            .ok_or(StoreError::TurnPending)?;
        if terminal.outcome != TurnOutcome::Succeeded
            || usage.outcome != TurnOutcome::Succeeded
            || terminal.native_turn_id.as_deref() != Some(summary.native_turn_id.as_str())
        {
            return Err(StoreError::Conflict);
        }
        let bytes = serde_json::to_vec(summary).map_err(|_| StoreError::Integrity)?;
        if bytes.len() > 1024 * 1024 {
            return Err(StoreError::Invalid("native_summary_document"));
        }
        if let Some(row)=sqlx::query("SELECT s.artifact_id,a.byte_count FROM app.model_turn_summaries s JOIN app.artifacts a ON a.id=s.artifact_id WHERE s.reservation_id=$1")
            .bind(reservation.as_uuid()).fetch_optional(&mut *tx).await? {
            let artifact=id(row.try_get("artifact_id")?)?;
            let size=count(row.try_get("byte_count")?)?;
            if size.get()!=bytes.len() as u64 || read(artifact,size).await?!=bytes {return Err(StoreError::Conflict);}
            tx.commit().await?;
            return Ok(artifact);
        }
        let limits: serde_json::Value =
            sqlx::query_scalar("SELECT limits FROM app.run_admissions WHERE run_id=$1")
                .bind(item.run_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let limits: JobLimitsV1 =
            serde_json::from_value(limits).map_err(|_| StoreError::Integrity)?;
        let used:i64=sqlx::query_scalar("SELECT coalesce(sum(byte_count),0)::bigint FROM app.artifacts WHERE producer_run_id=$1")
            .bind(item.run_id.as_uuid()).fetch_one(&mut *tx).await?;
        if count(used)?
            .get()
            .checked_add(bytes.len() as u64)
            .is_none_or(|total| total > limits.output_bytes.get())
        {
            return Err(DomainError::BudgetExhausted("output_bytes").into());
        }
        let artifact = Id::new();
        sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'REPORT','application/json','qz.mission_summary','1','LOCAL',$5,'1',$6,'RESEARCH','SYNTHETIC','RUNTIME','REFERENCED')")
            .bind(artifact.as_uuid()).bind(mission.project_id).bind(item.run_id.as_uuid()).bind(item.attempt_id.as_uuid())
            .bind(artifact.to_string()).bind(bytes.len() as i64).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.model_turn_summaries(reservation_id,artifact_id,native_item_id) VALUES($1,$2,$3)")
            .bind(reservation.as_uuid()).bind(artifact.as_uuid()).bind(&summary.native_item_id).execute(&mut *tx).await?;
        publish(NativeObjectPublication {
            id: artifact,
            bytes,
        })
        .await?;
        // Recording already observed output is not a fresh paid-turn admission;
        // configuration changes cannot erase its receipt. The owner must be live.
        lock_mission(&mut tx, item.run_id, fence).await?;
        tx.commit().await?;
        Ok(artifact)
    }
}
