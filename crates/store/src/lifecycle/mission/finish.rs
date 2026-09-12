//! Finish the bounded native conversation, not an evaluation or qualification.
use super::*;

impl Store {
    /// A committed native/public-answer chain is the completion proof. No caller
    /// supplies a verdict, arbitrary report, model identity or fabricated stop.
    pub async fn complete_research_mission(
        &self,
        run: Id,
        owner: &WorkerFence,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut locked = lock_run(&mut tx, run).await?;
        let role: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.run_missions WHERE run_id=$1 AND role='RESEARCHER')",
        )
        .bind(run.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if locked.run.kind != RunKind::AgentResearch || !role {
            return Err(StoreError::Forbidden);
        }
        if locked.run.state.is_terminal() {
            let committed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.run_terminal_receipts WHERE run_id=$1 AND attempt_id=$2)")
                .bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).fetch_one(&mut *tx).await?;
            if !committed {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(true);
        }
        let attempt = fence(&mut tx, &locked.run, owner).await?;
        expire_sent_run(&mut tx, &mut locked, &attempt).await?;
        let latest = sqlx::query("SELECT r.id,r.session_id,s.artifact_id FROM app.model_turn_reservations r JOIN app.model_turn_receipts receipt ON receipt.reservation_id=r.id AND receipt.outcome='SUCCEEDED' JOIN app.model_turn_summaries s ON s.reservation_id=r.id WHERE r.run_id=$1 AND r.attempt_id=$2 AND r.ordinal=(SELECT max(ordinal) FROM app.model_turn_reservations WHERE run_id=$1)")
            .bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(latest) = latest else {
            tx.commit().await?;
            return Ok(false);
        };
        let unaccounted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.model_turn_reservations r LEFT JOIN app.model_turn_receipts receipt ON receipt.reservation_id=r.id LEFT JOIN app.model_turn_terminals terminal ON terminal.reservation_id=r.id LEFT JOIN app.model_turn_summaries summary ON summary.reservation_id=r.id WHERE r.run_id=$1 AND (receipt.reservation_id IS NULL OR terminal.reservation_id IS NULL OR terminal.outcome IS DISTINCT FROM receipt.outcome OR (receipt.outcome='SUCCEEDED' AND summary.reservation_id IS NULL)))")
            .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
        let proposed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.experiments e JOIN app.experiment_authorship a ON a.experiment_id=e.id WHERE e.project_id=$1 AND e.cycle_id=$2 AND e.outcome='PENDING' AND e.run_id IS NULL AND NOT EXISTS(SELECT 1 FROM app.experiment_compilations c WHERE c.experiment_id=e.id) AND ((e.code_artifact_id IS NOT NULL AND e.parameter_artifact_id IS NOT NULL) OR a.author_run_id=$3))")
            .bind(locked.run.project_id.as_uuid()).bind(locked.run.cycle_id.map(Id::as_uuid)).bind(run.as_uuid()).fetch_one(&mut *tx).await?;
        let scientific_pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.experiment_compilations c JOIN app.runs compiled ON compiled.id=c.compile_run_id LEFT JOIN app.experiment_forecasts f ON f.experiment_id=c.experiment_id WHERE c.mission_run_id=$1 AND (NOT EXISTS(SELECT 1 FROM app.run_terminal_receipts t JOIN app.run_attempts a ON a.id=t.attempt_id AND a.run_id=t.run_id WHERE t.run_id=c.compile_run_id AND t.terminal_state=compiled.state AND a.id=compiled.active_attempt_id AND a.dispatch_state='TERMINAL') OR (compiled.state='SUCCEEDED' AND f.run_id IS NULL) OR (f.run_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM app.runs r JOIN app.run_terminal_receipts t ON t.run_id=r.id AND t.attempt_id=r.active_attempt_id AND t.terminal_state=r.state JOIN app.run_attempts a ON a.id=t.attempt_id AND a.dispatch_state='TERMINAL' WHERE r.id=f.run_id)) OR NOT EXISTS(SELECT 1 FROM app.model_turn_reservations r JOIN app.model_turn_receipts t ON t.reservation_id=r.id AND t.outcome='SUCCEEDED' JOIN app.model_turn_summaries s ON s.reservation_id=r.id WHERE r.session_id=$2 AND r.command_key='mission/result/'||coalesce(f.run_id,c.compile_run_id)::text)))")
            .bind(run.as_uuid()).bind(latest.try_get::<uuid::Uuid,_>("session_id")?).fetch_one(&mut *tx).await?;
        let alpha_pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.experiment_compilations c JOIN app.experiment_forecasts f ON f.experiment_id=c.experiment_id JOIN app.runs r ON r.id=f.run_id AND r.state='SUCCEEDED' WHERE c.mission_run_id=$1 AND NOT EXISTS(SELECT 1 FROM app.command_receipts receipt JOIN app.alpha_versions v ON v.id=receipt.resource_id AND v.experiment_id=c.experiment_id AND v.model_artifact_id=f.model_artifact_id WHERE receipt.principal_scope='MISSION:'||$1::uuid::text AND receipt.operation='RESEARCH_ALPHA_CREATE' AND receipt.idempotency_key=c.experiment_id::text))")
            .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
        if unaccounted || proposed || scientific_pending || alpha_pending {
            tx.commit().await?;
            return Ok(false);
        }
        let current = current_lease(&attempt)?;
        let state = runs::accept_terminal(
            locked.run.state,
            Some(RemoteTerminal::Succeeded),
            &current,
            &current,
            now(&mut tx).await?,
        )?;
        let summary = db::id(latest.try_get("artifact_id")?)?;
        // The native Mission result is its public answer, not an OCI JobSpec
        // envelope. Its original artifact has already passed producer checks.
        sqlx::query("UPDATE app.run_attempts SET dispatch_state='TERMINAL',runtime_state='SUCCEEDED',result_manifest_artifact_id=$2,accepted_at=clock_timestamp() WHERE id=$1")
            .bind(owner.attempt_id.as_uuid()).bind(summary.as_uuid()).execute(&mut *tx).await?;
        let reason = if state == RunState::Cancelled {
            RunReason::ResultDiscardedAfterCancel
        } else {
            RunReason::RuntimeSucceeded
        };
        finish(&mut tx,&mut locked,state,reason,json!({"schema_version":1,"source":"NATIVE_MISSION","session_id":db::id(latest.try_get("session_id")?)?,"concluding_reservation_id":db::id(latest.try_get("id")?)?,"summary_artifact_id":summary,"formal_evaluation":"NOT_PERFORMED"})).await?;
        tx.commit().await?;
        Ok(true)
    }
}
