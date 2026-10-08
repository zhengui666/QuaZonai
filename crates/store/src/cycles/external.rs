//! Owner-started external research retains the frozen cycle ledger, without a Mission.
use super::*;

impl Store {
    pub async fn start_external_cycle<R, Read>(
        &self,
        actor: &Actor,
        key: &str,
        request: &ExternalCycleStartIntent,
        mut read: R,
    ) -> Result<CommandResult<CycleViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::CycleStartExternal,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let project = sqlx::query("SELECT revision,state FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(request.project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let current = db::revision(project.try_get("revision")?)?;
        if current != request.request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        if project.try_get::<String, _>("state")? != "ACTIVE" {
            return Err(DomainError::AdmissionClosed.into());
        }
        let row = crate::brief::row(&mut tx, request.request.brief_id, false).await?;
        let brief = crate::brief::view(&mut tx, &row).await?;
        if brief.project_id != request.project_id || brief.state != BriefState::Frozen {
            return Err(invalid("brief_id", "OWNED_FROZEN_BRIEF_REQUIRED").into());
        }
        let context = execution_context(&mut tx, brief.id).await?;
        validate_execution_context(&mut tx, &brief, &context, &mut read).await?;
        let time: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let today: i64 = sqlx::query_scalar("SELECT count(*) FROM app.research_cycles WHERE project_id=$1 AND created_at>=date_trunc('day',$2::timestamptz,'UTC') AND created_at<date_trunc('day',$2::timestamptz,'UTC')+interval '1 day'")
            .bind(request.project_id.as_uuid()).bind(time).fetch_one(&mut *tx).await?;
        if brief.content.budget.max_cycles_per_day.is_some_and(|limit| today >= i64::from(limit)) {
            return Err(DomainError::BudgetExhausted("cycles_per_day").into());
        }
        let previous: i32 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),0) FROM app.research_cycles WHERE project_id=$1",
        )
        .bind(request.project_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let ordinal = previous.checked_add(1).ok_or(StoreError::Integrity)?;
        let cycle = prepared.target;
        sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot,started_at,next_action) VALUES($1,$2,$3,$4,'OPERATOR','RUNNING',$5,$6,'EXTERNAL_EXPERIMENT')")
            .bind(cycle.as_uuid()).bind(request.project_id.as_uuid()).bind(brief.id.as_uuid()).bind(ordinal)
            .bind(db::json(&brief.content.budget)?).bind(time).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.external_research_cycles(cycle_id) VALUES($1)")
            .bind(cycle.as_uuid())
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!("{CYCLE} WHERE c.id=$1")))
            .bind(cycle.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, cycle_view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}

impl Store {
    /// Close only a settled external batch. Unexecuted proposals retain their
    /// historical PENDING status; closing a batch is not a scientific verdict.
    pub async fn finish_external_cycle(
        &self,
        actor: &Actor,
        key: &str,
        request: &CycleFinishExternalIntent,
    ) -> Result<CommandResult<CycleViewV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::CycleFinishExternal,
            key,
            Some(request.cycle_id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.research_cycles WHERE id=$1")
                .bind(request.cycle_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        crate::research::project_for_write(&mut tx, db::id(project)?).await?;
        let cycle = sqlx::query("SELECT c.revision,c.state,c.reserved_experiments FROM app.research_cycles c JOIN app.external_research_cycles x ON x.cycle_id=c.id WHERE c.id=$1 FOR UPDATE OF c")
            .bind(request.cycle_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("external_cycle_required"))?;
        let current = db::revision(cycle.try_get("revision")?)?;
        if current != request.request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        if matches!(
            cycle.try_get::<&str, _>("state")?,
            "COMPLETED" | "CANCELLED" | "FAILED"
        ) {
            return Err(DomainError::AdmissionClosed.into());
        }
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.runs WHERE cycle_id=$1 AND state NOT IN ('SUCCEEDED','FAILED','CANCELLED')) OR EXISTS(SELECT 1 FROM app.external_experiment_requests q JOIN app.experiments e ON e.id=q.experiment_id WHERE e.cycle_id=$1 AND NOT EXISTS(SELECT 1 FROM app.external_experiment_results r WHERE r.experiment_id=e.id))")
            .bind(request.cycle_id.as_uuid()).fetch_one(&mut *tx).await?;
        if pending || cycle.try_get::<i64, _>("reserved_experiments")? != 0 {
            return Err(StoreError::Invalid("external_cycle_work_pending"));
        }
        sqlx::query("UPDATE app.research_cycles SET state='COMPLETED',outcome='INCONCLUSIVE',next_action=NULL,ended_at=clock_timestamp() WHERE id=$1")
            .bind(request.cycle_id.as_uuid()).execute(&mut *tx).await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!("{CYCLE} WHERE c.id=$1")))
            .bind(request.cycle_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, cycle_view(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }
}
