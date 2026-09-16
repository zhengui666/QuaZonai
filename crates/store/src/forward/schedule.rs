//! Fair bounded retry reservations on the existing automation polling lane.
use super::*;

impl Store {
    /// Scheduling metadata only. The returned original stream still needs full native admission.
    pub async fn prepare_forward_evaluation(
        &self,
        project: Id,
    ) -> Result<Option<(Id, String)>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        // Messages are immutable and cannot be deleted: count is a retry hint, not an
        // evidence identity. Actual admission still compares every original source ID.
        let candidate = sqlx::query("SELECT h.id AS handoff_id,m.stream_id,count(m.id)::bigint AS messages FROM app.projects p JOIN app.automation_policies policy ON policy.id=p.current_automation_policy_id AND policy.project_id=p.id JOIN app.portfolio_candidates c ON c.project_id=p.id AND c.mandate_id=policy.mandate_id JOIN app.releases r ON r.candidate_id=c.id AND r.environment='REAL' JOIN app.handoff_offers h ON h.release_id=r.id AND h.downstream_id=policy.downstream_id JOIN app.handoff_transfers transfer ON transfer.handoff_id=h.id AND transfer.external_claim_id=h.external_claim_id AND transfer.claimed_at=h.claimed_at AND transfer.provenance='RECORDED_TRANSITION' JOIN app.forward_messages m ON m.handoff_id=h.id LEFT JOIN app.forward_schedule s ON s.handoff_id=h.id AND s.stream_id=m.stream_id WHERE p.id=$1 AND p.state='ACTIVE' AND policy.mode<>'MANUAL' AND policy.enabled_for_new_rebalances AND policy.authorized_at<=clock_timestamp() AND policy.valid_until>clock_timestamp() AND h.state IN ('CLAIMED','ACKNOWLEDGED') AND NOT EXISTS(SELECT 1 FROM app.policy_revocations revoked WHERE revoked.automation_policy_id=policy.id AND revoked.effective_at<=clock_timestamp()) GROUP BY h.id,m.stream_id,s.last_attempt_at,s.next_attempt_at,s.observed_message_count HAVING (s.next_attempt_at IS NULL OR s.next_attempt_at<=clock_timestamp() OR s.observed_message_count<>count(m.id)) AND NOT EXISTS(SELECT 1 FROM app.forward_evaluation_inputs f WHERE f.handoff_id=h.id AND f.request->'request'->'window'->>'stream_id'=m.stream_id AND jsonb_array_length(f.request->'request'->'sources')=count(m.id)) ORDER BY s.last_attempt_at NULLS FIRST,h.id,m.stream_id LIMIT 1")
            .bind(project.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = candidate else {
            tx.commit().await?;
            return Ok(None);
        };
        let handoff = db::id(row.try_get("handoff_id")?)?;
        let stream: String = row.try_get("stream_id")?;
        let messages: i64 = row.try_get("messages")?;
        let reserved: Option<uuid::Uuid> = sqlx::query_scalar("INSERT INTO app.forward_schedule(handoff_id,stream_id,observed_message_count,last_attempt_at,next_attempt_at) VALUES($1,$2,$3,statement_timestamp(),statement_timestamp()+interval '30 seconds') ON CONFLICT(handoff_id,stream_id) DO UPDATE SET observed_message_count=excluded.observed_message_count,last_attempt_at=excluded.last_attempt_at,next_attempt_at=excluded.next_attempt_at WHERE app.forward_schedule.next_attempt_at<=clock_timestamp() OR app.forward_schedule.observed_message_count<>excluded.observed_message_count RETURNING handoff_id")
            .bind(handoff.as_uuid()).bind(&stream).bind(messages).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        Ok(reserved.map(|_| (handoff, stream)))
    }
}
