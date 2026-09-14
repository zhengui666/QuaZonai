//! Two domain drivers share PGMQ's native conditional read. Neither hides or
//! claims the other's messages; PGMQ owns read counts and visibility.
use super::*;

impl Store {
    pub async fn read_native_run_messages(
        &self,
        visibility_seconds: i32,
        limit: i32,
    ) -> Result<Vec<RunMessage>, StoreError> {
        self.read_owned_run_messages(visibility_seconds, limit, false)
            .await
    }

    pub async fn read_mission_messages(
        &self,
        visibility_seconds: i32,
        limit: i32,
    ) -> Result<Vec<RunMessage>, StoreError> {
        self.read_owned_run_messages(visibility_seconds, limit, true)
            .await
    }

    async fn read_owned_run_messages(
        &self,
        visibility_seconds: i32,
        limit: i32,
        mission: bool,
    ) -> Result<Vec<RunMessage>, StoreError> {
        if !(1..=300).contains(&visibility_seconds) || !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("queue_read_limit"));
        }
        let mut tx = self.pool.begin().await?;
        // Only queue rows are locked here. No project/Run locks are acquired in
        // this order, so normal result adoption can retain project -> Run -> queue.
        let candidates: Vec<Value> = sqlx::query_scalar(
            "SELECT q.message FROM pgmq.q_runs q JOIN app.runs r ON q.message=jsonb_build_object('schema_version',1,'run_id',r.id) WHERE q.vt<=clock_timestamp() AND (($2 AND r.kind='AGENT_RESEARCH' AND EXISTS(SELECT 1 FROM app.run_missions m WHERE m.run_id=r.id)) OR (NOT $2 AND r.kind IN ('DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE','FORWARD_EVALUATE') AND EXISTS(SELECT 1 FROM app.run_native_tasks t WHERE t.run_id=r.id))) ORDER BY q.msg_id LIMIT $1 FOR UPDATE OF q SKIP LOCKED",
        )
        .bind(limit)
        .bind(mission)
        .fetch_all(&mut *tx)
        .await?;
        let mut messages = Vec::with_capacity(candidates.len());
        for expected in candidates {
            // Native conditional read is supported by the pinned PGMQ1.10.0.
            // An older/other consumer may already have reserved this exact run;
            // an empty native read is normal, never grounds to select another driver.
            let Some(row) =
                sqlx::query("SELECT msg_id,read_ct,message FROM pgmq.read('runs',$1,1,$2::jsonb)")
                    .bind(visibility_seconds)
                    .bind(&expected)
                    .fetch_optional(&mut *tx)
                    .await?
            else {
                continue;
            };
            let document: Value = row.try_get("message")?;
            if document != expected {
                return Err(StoreError::Integrity);
            }
            let payload: QueuePayload =
                serde_json::from_value(document).map_err(|_| StoreError::Integrity)?;
            messages.push(RunMessage {
                message_id: row.try_get("msg_id")?,
                run_id: payload.run_id,
                read_count: row.try_get("read_ct")?,
            });
        }
        tx.commit().await?;
        Ok(messages)
    }

    /// The direct driver entrypoint must not borrow a Mission lease either.
    /// Both kind and native-task membership are immutable once committed, so a
    /// successful selection cannot change between this read and ordinary claim.
    pub async fn claim_native_run(
        &self,
        message: &RunMessage,
        owner: &str,
        lease_seconds: u16,
    ) -> Result<Option<ClaimResult>, StoreError> {
        self.claim_owned_run(message, owner, lease_seconds, false)
            .await
    }

    pub async fn claim_mission(
        &self,
        message: &RunMessage,
        owner: &str,
        lease_seconds: u16,
    ) -> Result<Option<ClaimResult>, StoreError> {
        self.claim_owned_run(message, owner, lease_seconds, true)
            .await
    }

    async fn claim_owned_run(
        &self,
        message: &RunMessage,
        owner: &str,
        lease_seconds: u16,
        mission: bool,
    ) -> Result<Option<ClaimResult>, StoreError> {
        let eligible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.runs r WHERE r.id=$1 AND (($2 AND r.kind='AGENT_RESEARCH' AND EXISTS(SELECT 1 FROM app.run_missions m WHERE m.run_id=r.id)) OR (NOT $2 AND r.kind IN ('DATA_VALIDATE','ALPHA_EVALUATE','PORTFOLIO_BUILD','PORTFOLIO_SIMULATE','FORWARD_EVALUATE') AND EXISTS(SELECT 1 FROM app.run_native_tasks t WHERE t.run_id=r.id))))",
        )
        .bind(message.run_id.as_uuid())
        .bind(mission)
        .fetch_one(&self.pool)
        .await?;
        if !eligible {
            return Ok(None);
        }
        self.claim_run(message, owner, lease_seconds)
            .await
            .map(Some)
    }
}
