//! Original bounded evaluator-only source reads under the same publication barrier.
use super::*;
use contracts::DbCounter;

impl Store {
    pub async fn forward_window<R, Read>(
        &self,
        actor: &Actor,
        handoff: Id,
        query: &ForwardWindowQueryV1,
        mut read: R,
    ) -> Result<ForwardWindowViewV1, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        domain::control::text(&query.stream_id, 1, 200, false)?;
        let mut tx = self.pool.begin().await?;
        let header=sqlx::query("SELECT c.project_id,h.downstream_id FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE h.id=$1")
            .bind(handoff.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let project = db::id(header.try_get("project_id")?)?;
        let downstream = db::id(header.try_get("downstream_id")?)?;
        if messages::read_authority(&mut tx, actor, project)
            .await?
            .is_some_and(|id| id != downstream)
        {
            return Err(StoreError::NotFound);
        }
        // ponytail: one bounded snapshot per original stream; use native streaming for histories beyond these documented limits.
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR SHARE")
            .bind(project.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let result = load(&mut tx, handoff, &query.stream_id, &mut read)
            .await?
            .window;
        messages::read_authority(&mut tx, actor, project).await?;
        tx.commit().await?;
        Ok(result)
    }
}

/// Caller owns the Project publication barrier; no user authority is inferred here.
pub(super) async fn load<R, Read>(
    tx: &mut Transaction<'_, Postgres>,
    handoff: Id,
    stream: &str,
    read: &mut R,
) -> Result<NativeForwardRequestV1, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let rows=sqlx::query("SELECT m.*,c.project_id,h.release_id,h.environment AS forward_environment,h.claimed_at AS forward_claimed_at,h.external_claim_id AS forward_claim_id,a.byte_count,EXISTS(SELECT 1 FROM app.command_receipts receipt WHERE receipt.operation='FORWARD_SUBMIT' AND receipt.idempotency_key=m.external_message_id AND receipt.resource_id=m.id AND receipt.principal_scope='DOWNSTREAM:'||m.downstream_id::text AND receipt.normalized_nonsecret_request->>'schema_version'='1' AND receipt.normalized_nonsecret_request->>'project_id'=c.project_id::text AND receipt.normalized_nonsecret_request->>'handoff_id'=h.id::text AND receipt.normalized_nonsecret_request->>'report_artifact_id'=a.id::text AND receipt.response_nonsecret_body->>'schema_version'='1' AND receipt.response_nonsecret_body->'resource'->>'id'=m.id::text) AS native FROM app.forward_messages m JOIN app.handoff_offers h ON h.id=m.handoff_id JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.artifacts a ON a.id=m.report_artifact_id WHERE h.id=$1 AND m.stream_id=$2 ORDER BY m.sequence,m.message_revision LIMIT 10001")
        .bind(handoff.as_uuid()).bind(stream).fetch_all(&mut **tx).await?;
    let mut total = 0u64;
    for row in &rows {
        let size = u64::try_from(row.try_get::<i64, _>("byte_count")?)
            .map_err(|_| StoreError::Integrity)?;
        if size == 0 || size > 2 * 1024 * 1024 || !row.try_get::<bool, _>("native")? {
            return Err(StoreError::Invalid("forward_source_provenance"));
        }
        total = total.checked_add(size).ok_or(StoreError::Integrity)?;
    }
    if rows.len() > 10000 || total > 64 * 1024 * 1024 {
        return Err(domain::DomainError::CapabilityUnavailable("forward_window_limit").into());
    }
    let mut sources = Vec::new();
    let mut points = 0usize;
    for row in &rows {
        let message = messages::view(row)?;
        let size = DbCounter::new(row.try_get::<i64, _>("byte_count")? as u64)
            .map_err(|_| StoreError::Integrity)?;
        let bytes = read(message.report_artifact_id, size).await?;
        if bytes.len() as u64 != size.get() {
            return Err(StoreError::Integrity);
        }
        let report: ForwardReportV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        let claimed: chrono::DateTime<chrono::Utc> = row
            .try_get::<Option<_>, _>("forward_claimed_at")?
            .ok_or(StoreError::Integrity)?;
        if report.environment != db::enum_value::<ForwardEnvironmentV1>(row, "forward_environment")?
            || Some(report.content.external_claim_id.as_str())
                != row
                    .try_get::<Option<String>, _>("forward_claim_id")?
                    .as_deref()
            || report.content.window_start < claimed
        {
            return Err(StoreError::Invalid("forward_source_binding"));
        }
        points = points
            .checked_add(report.content.returns.len())
            .ok_or(StoreError::Integrity)?;
        if points > 1000000 {
            return Err(domain::DomainError::CapabilityUnavailable("forward_window_limit").into());
        }
        sources.push(domain::forward::ForwardWindowSource { message, report });
    }
    let window = domain::forward::window(handoff, stream, &sources)?.view;
    Ok(NativeForwardRequestV1 {
        window,
        sources: sources.into_iter().map(|s| s.message).collect(),
    })
}
