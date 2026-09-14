//! Operator-frozen policy versions. Registration never implies automatic delivery.
use crate::{authority::Actor, commands, control::page, db, Store, StoreError};
use contracts::{control::*, delivery::*, Id};
use sqlx::{postgres::PgRow, Row};

pub(crate) fn view(row: &PgRow) -> Result<AutomationPolicyViewV1, StoreError> {
    Ok(AutomationPolicyViewV1 {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        created_at: row.try_get("created_at")?,
        authorized_at: row.try_get("authorized_at")?,
        content: AutomationPolicyContentV1 {
            mode: db::enum_value(row, "mode")?,
            mandate_id: db::id(row.try_get("mandate_id")?)?,
            downstream_id: db::id(row.try_get("downstream_id")?)?,
            required_paper_observations: u32::try_from(
                row.try_get::<i32, _>("required_paper_observations")?,
            )
            .map_err(|_| StoreError::Integrity)?,
            minimum_paper_elapsed_seconds: row
                .try_get::<i64, _>("minimum_paper_elapsed_seconds")?
                .to_string()
                .try_into()
                .map_err(|_| StoreError::Integrity)?,
            max_feedback_age_seconds: row
                .try_get::<i64, _>("max_feedback_age_seconds")?
                .to_string()
                .try_into()
                .map_err(|_| StoreError::Integrity)?,
            promotion_metric_requirements: serde_json::from_value(
                row.try_get("promotion_metric_requirements")?,
            )
            .map_err(|_| StoreError::Integrity)?,
            degradation_metric_requirements: serde_json::from_value(
                row.try_get("degradation_metric_requirements")?,
            )
            .map_err(|_| StoreError::Integrity)?,
            valid_until: row.try_get("valid_until")?,
            enabled_for_new_rebalances: row.try_get("enabled_for_new_rebalances")?,
            max_rebalances_per_day: u32::try_from(row.try_get::<i32, _>("max_rebalances_per_day")?)
                .map_err(|_| StoreError::Integrity)?,
        },
    })
}
fn revocation(row: &PgRow) -> Result<PolicyRevocationViewV1, StoreError> {
    Ok(PolicyRevocationViewV1 {
        id: db::id(row.try_get("id")?)?,
        automation_policy_id: db::id(row.try_get("automation_policy_id")?)?,
        created_at: row.try_get("created_at")?,
        effective_at: row.try_get("effective_at")?,
        reason: row.try_get("reason")?,
    })
}
impl Store {
    pub async fn automation_policy(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<AutomationPolicyViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM app.automation_policies WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let result = view(&row)?;
        crate::evidence::authorize(&mut tx, actor, result.project_id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn automation_policies(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<AutomationPolicyViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows=sqlx::query("SELECT * FROM app.automation_policies WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(page(items, query.limit, |v| v.id))
    }
    pub async fn authorize_automation(
        &self,
        actor: &Actor,
        key: &str,
        project: Id,
        request: &AutomationAuthorizeV1,
    ) -> Result<CommandResult<AutomationPolicyViewV1>, StoreError> {
        domain::delivery::automation_policy(&request.content)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::PolicyAuthorize,
            key,
            Some(project),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        crate::research::project_for_write(&mut tx, project).await?;
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let revision = db::revision(revision)?;
        if revision != request.expected_project_revision {
            return Err(StoreError::RevisionConflict { current: revision });
        }
        let c = &request.content;
        sqlx::query("SELECT id FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2")
            .bind(c.mandate_id.as_uuid())
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let downstream = sqlx::query(
            "SELECT enabled,environments FROM app.downstream_integrations WHERE id=$1 FOR SHARE",
        )
        .bind(c.downstream_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let environment: String = downstream.try_get("environments")?;
        if !downstream.try_get::<bool, _>("enabled")?
            || matches!(c.mode, AutomationModeV1::AutoPaper) && environment == "LIVE"
            || matches!(c.mode, AutomationModeV1::AutoHandoff) && environment != "BOTH"
        {
            return Err(StoreError::Invalid("automation_downstream"));
        }
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if c.valid_until <= now {
            return Err(StoreError::Invalid("automation_expiry"));
        }
        let row=sqlx::query("INSERT INTO app.automation_policies(project_id,mode,mandate_id,downstream_id,required_paper_observations,minimum_paper_elapsed_seconds,max_feedback_age_seconds,promotion_metric_requirements,degradation_metric_requirements,authorized_at,valid_until,enabled_for_new_rebalances,max_rebalances_per_day) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING *")
            .bind(project.as_uuid()).bind(db::code(&c.mode)?).bind(c.mandate_id.as_uuid()).bind(c.downstream_id.as_uuid()).bind(c.required_paper_observations as i32).bind(c.minimum_paper_elapsed_seconds.get() as i64).bind(c.max_feedback_age_seconds.get() as i64).bind(db::json(&c.promotion_metric_requirements)?).bind(db::json(&c.degradation_metric_requirements)?).bind(now).bind(c.valid_until).bind(c.enabled_for_new_rebalances).bind(c.max_rebalances_per_day as i32).fetch_one(&mut *tx).await?;
        let resource = view(&row)?;
        sqlx::query("UPDATE app.projects SET current_automation_policy_id=$2 WHERE id=$1")
            .bind(project.as_uuid())
            .bind(resource.id.as_uuid())
            .execute(&mut *tx)
            .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn automation_revocations(
        &self,
        actor: &Actor,
        id: Id,
        query: &ListQuery,
    ) -> Result<Page<PolicyRevocationViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.automation_policies WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(project)?).await?;
        let rows=sqlx::query("SELECT * FROM app.policy_revocations WHERE automation_policy_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(revocation).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(page(items, query.limit, |v| v.id))
    }
    pub async fn revoke_automation(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &PolicyRevokeV1,
    ) -> Result<CommandResult<PolicyRevocationViewV1>, StoreError> {
        domain::control::text(&request.reason, 1, 2000, true)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::PolicyRevoke,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.automation_policies WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(project)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.automation_policies WHERE id=$1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let latest:Option<uuid::Uuid>=sqlx::query_scalar("SELECT id FROM app.policy_revocations WHERE automation_policy_id=$1 ORDER BY id DESC LIMIT 1").bind(id.as_uuid()).fetch_optional(&mut *tx).await?;
        if latest != request.expected_latest_revocation_id.map(Id::as_uuid) {
            return Err(StoreError::Conflict);
        }
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let effective = request.effective_at.unwrap_or(now);
        if effective < now {
            return Err(StoreError::Invalid("revocation_time"));
        }
        let row=sqlx::query("INSERT INTO app.policy_revocations(automation_policy_id,effective_at,reason) VALUES($1,$2,$3) RETURNING *").bind(id.as_uuid()).bind(effective).bind(&request.reason).fetch_one(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, revocation(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
