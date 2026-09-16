//! Append-only Candidate-scoped human decisions; reopening never approves delivery.
use super::*;
use contracts::{
    control::{ListQuery, OperatorCommand, Page},
    delivery::*,
};
use sqlx::postgres::PgRow;

fn view(row: &PgRow) -> Result<ReleaseDecisionViewV1, StoreError> {
    Ok(ReleaseDecisionViewV1 {
        id: db::id(row.try_get("id")?)?,
        created_at: row.try_get("created_at")?,
        project_id: db::id(row.try_get("project_id")?)?,
        release_id: db::id(row.try_get("release_id")?)?,
        candidate_id: db::id(row.try_get("candidate_id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        environment: db::enum_value(row, "environment")?,
        ordinal: u32::try_from(row.try_get::<i32, _>("ordinal")?)
            .map_err(|_| StoreError::Integrity)?,
        decision: db::enum_value(row, "decision")?,
        supersedes_decision_id: db::optional_id(row, "supersedes_decision_id")?,
        reason_code: row.try_get("reason_code")?,
        reason: row.try_get("reason")?,
        decided_at: row.try_get("decided_at")?,
        decided_by: row.try_get("decided_by")?,
    })
}

impl Store {
    pub async fn reject_release(
        &self,
        actor: &Actor,
        key: &str,
        release: Id,
        request: &ReleaseRejectV1,
    ) -> Result<CommandResult<ReleaseDecisionViewV1>, StoreError> {
        self.decide_release(
            actor,
            key,
            release,
            OperatorCommand::ReleaseReject(request.clone()),
        )
        .await
    }
    pub async fn reopen_release(
        &self,
        actor: &Actor,
        key: &str,
        decision: Id,
        request: &ReleaseReopenV1,
    ) -> Result<CommandResult<ReleaseDecisionViewV1>, StoreError> {
        self.decide_release(
            actor,
            key,
            decision,
            OperatorCommand::ReleaseReopen(request.clone()),
        )
        .await
    }
    async fn decide_release(
        &self,
        actor: &Actor,
        key: &str,
        target: Id,
        command: OperatorCommand,
    ) -> Result<CommandResult<ReleaseDecisionViewV1>, StoreError> {
        domain::control::command(&command)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            command.operation(),
            key,
            Some(target),
            command
                .normalized_request()
                .map_err(|_| StoreError::Integrity)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let (release, downstream, environment, expected, decision, reason_code, reason) =
            match command {
                OperatorCommand::ReleaseReject(r) => (
                    target,
                    r.downstream_id,
                    r.environment,
                    r.expected_latest_decision_id,
                    ReleaseDecisionV1::Reject,
                    r.reason_code,
                    r.reason,
                ),
                OperatorCommand::ReleaseReopen(r) => {
                    if r.expected_latest_decision_id != target {
                        return Err(StoreError::Conflict);
                    }
                    let old=sqlx::query("SELECT release_id,downstream_id,environment FROM app.release_decisions WHERE id=$1").bind(target.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
                    (
                        db::id(old.try_get("release_id")?)?,
                        db::id(old.try_get("downstream_id")?)?,
                        db::enum_value(&old, "environment")?,
                        Some(target),
                        ReleaseDecisionV1::Reopen,
                        r.reason_code,
                        r.reason,
                    )
                }
                _ => return Err(StoreError::Invalid("release_decision_operation")),
            };
        let row=sqlx::query("SELECT c.id,c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1 FOR UPDATE OF c").bind(release.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let candidate = db::id(row.try_get("id")?)?;
        let project = db::id(row.try_get("project_id")?)?;
        if !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM app.downstream_integrations WHERE id=$1)",
        )
        .bind(downstream.as_uuid())
        .fetch_one(&mut *tx)
        .await?
        {
            return Err(StoreError::NotFound);
        }
        let previous=sqlx::query("SELECT id,ordinal,decision FROM app.release_decisions WHERE candidate_id=$1 AND downstream_id=$2 AND environment=$3 ORDER BY ordinal DESC LIMIT 1").bind(candidate.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&environment)?).fetch_optional(&mut *tx).await?;
        let latest = previous
            .as_ref()
            .map(|r| db::id(r.try_get("id")?))
            .transpose()?;
        if latest != expected
            || (decision == ReleaseDecisionV1::Reopen
                && previous
                    .as_ref()
                    .is_none_or(|r| r.try_get::<&str, _>("decision").ok() != Some("REJECT")))
        {
            return Err(StoreError::Conflict);
        }
        let ordinal = previous
            .as_ref()
            .map(|r| r.try_get::<i32, _>("ordinal"))
            .transpose()?
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(StoreError::Conflict)?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let row=sqlx::query("INSERT INTO app.release_decisions(release_id,candidate_id,downstream_id,environment,ordinal,decision,supersedes_decision_id,reason_code,reason,decided_at,decided_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,clock_timestamp(),'OPERATOR') RETURNING *, $10::uuid AS project_id")
            .bind(release.as_uuid()).bind(candidate.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&environment)?).bind(ordinal).bind(db::code(&decision)?).bind(latest.map(Id::as_uuid)).bind(reason_code).bind(reason).bind(project.as_uuid()).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn release_decisions(
        &self,
        actor: &Actor,
        release: Id,
        query: &ListQuery,
    ) -> Result<Page<ReleaseDecisionViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT c.id,c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1").bind(release.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let candidate = db::id(row.try_get("id")?)?;
        let project = db::id(row.try_get("project_id")?)?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        let rows=sqlx::query("SELECT d.*,$4::uuid AS project_id FROM app.release_decisions d WHERE candidate_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3").bind(candidate.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).bind(project.as_uuid()).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |r| r.id))
    }
}
