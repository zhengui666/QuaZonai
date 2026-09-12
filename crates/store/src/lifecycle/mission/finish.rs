//! Finish the bounded native conversation, not an evaluation or qualification.
use super::*;
use crate::turns::{TurnOutcome, UsageReceipt};

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
        let stopping = locked.run.state == RunState::CancelRequested;
        if stopping {
            let unsent = sqlx::query("SELECT r.id,r.cost_currency,(r.reserved_cost IS NOT NULL) AS priced,t.reason_code FROM app.model_turn_reservations r LEFT JOIN app.model_turn_terminals t ON t.reservation_id=r.id WHERE r.run_id=$1 AND r.attempt_id=$2 AND NOT EXISTS(SELECT 1 FROM app.model_turn_dispatches d WHERE d.reservation_id=r.id) AND NOT EXISTS(SELECT 1 FROM app.model_turn_receipts receipt WHERE receipt.reservation_id=r.id) AND (t.reservation_id IS NULL OR t.outcome='NOT_SENT') ORDER BY r.ordinal")
                .bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).fetch_all(&mut *tx).await?;
            for item in unsent {
                tx = Self::settle_turn_in_transaction(
                    tx,
                    db::id(item.try_get("id")?)?,
                    owner,
                    &UsageReceipt {
                        outcome: TurnOutcome::NotSent,
                        actual_tokens: DbCounter::ZERO,
                        actual_cost: item
                            .try_get::<bool, _>("priced")?
                            .then(contracts::DecimalValue::zero),
                        currency: item.try_get("cost_currency")?,
                        reason_code: item
                            .try_get::<Option<String>, _>("reason_code")?
                            .unwrap_or_else(|| "MISSION_CANCELLED_BEFORE_TURN_SEND".into()),
                    },
                )
                .await?;
            }
        }
        let latest = sqlx::query("SELECT r.id,s.artifact_id FROM app.model_turn_reservations r JOIN app.model_turn_receipts receipt ON receipt.reservation_id=r.id LEFT JOIN app.model_turn_summaries s ON s.reservation_id=r.id WHERE r.run_id=$1 AND r.attempt_id=$2 AND r.ordinal=(SELECT max(ordinal) FROM app.model_turn_reservations WHERE run_id=$1) AND ($3 OR (receipt.outcome='SUCCEEDED' AND s.artifact_id IS NOT NULL))")
            .bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).bind(stopping).fetch_optional(&mut *tx).await?;
        if !stopping && latest.is_none() {
            tx.commit().await?;
            return Ok(false);
        }
        let session: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM app.codex_sessions WHERE run_id=$1")
                .bind(run.as_uuid())
                .fetch_optional(&mut *tx)
                .await?;
        let unaccounted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.model_turn_reservations r LEFT JOIN app.model_turn_receipts receipt ON receipt.reservation_id=r.id LEFT JOIN app.model_turn_terminals terminal ON terminal.reservation_id=r.id LEFT JOIN app.model_turn_summaries summary ON summary.reservation_id=r.id WHERE r.run_id=$1 AND (receipt.reservation_id IS NULL OR terminal.reservation_id IS NULL OR terminal.outcome IS DISTINCT FROM receipt.outcome OR (NOT $2 AND receipt.outcome='SUCCEEDED' AND summary.reservation_id IS NULL)))")
            .bind(run.as_uuid()).bind(stopping).fetch_one(&mut *tx).await?;
        let proposed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.experiments e JOIN app.experiment_authorship a ON a.experiment_id=e.id WHERE e.project_id=$1 AND e.cycle_id=$2 AND e.outcome='PENDING' AND e.run_id IS NULL AND NOT EXISTS(SELECT 1 FROM app.experiment_compilations c WHERE c.experiment_id=e.id) AND ((e.code_artifact_id IS NOT NULL AND e.parameter_artifact_id IS NOT NULL) OR a.author_run_id=$3))")
            .bind(locked.run.project_id.as_uuid()).bind(locked.run.cycle_id.map(Id::as_uuid)).bind(run.as_uuid()).fetch_one(&mut *tx).await?;
        let scientific_pending: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.experiment_compilations c
             JOIN app.runs compiled ON compiled.id=c.compile_run_id
             LEFT JOIN app.experiment_forecasts f ON f.experiment_id=c.experiment_id
             LEFT JOIN app.runs predicted ON predicted.id=f.run_id
             LEFT JOIN app.experiment_validations v ON v.experiment_id=c.experiment_id
             LEFT JOIN app.runs validated ON validated.id=v.run_id
             LEFT JOIN app.evaluations ev ON ev.run_id=v.run_id
               AND ev.subject_alpha_version_id=v.alpha_version_id AND ev.policy_id=v.policy_id
               AND ev.evaluation_kind='WALK_FORWARD'
             LEFT JOIN app.evaluation_publications published ON published.evaluation_id=ev.id
             WHERE c.mission_run_id=$1 AND (
               NOT EXISTS(SELECT 1 FROM app.run_terminal_receipts t
                 LEFT JOIN app.run_attempts a ON a.id=t.attempt_id
                 WHERE t.run_id=compiled.id AND t.terminal_state=compiled.state
                   AND t.attempt_id IS NOT DISTINCT FROM compiled.active_attempt_id
                   AND (a.id IS NULL OR a.dispatch_state='TERMINAL'))
               OR (NOT $3 AND compiled.state='SUCCEEDED' AND f.run_id IS NULL)
               OR (f.run_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM app.run_terminal_receipts t
                 LEFT JOIN app.run_attempts a ON a.id=t.attempt_id
                 WHERE t.run_id=predicted.id AND t.terminal_state=predicted.state
                   AND t.attempt_id IS NOT DISTINCT FROM predicted.active_attempt_id
                   AND (a.id IS NULL OR a.dispatch_state='TERMINAL')))
               OR (v.run_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM app.run_terminal_receipts t
                 LEFT JOIN app.run_attempts a ON a.id=t.attempt_id
                 WHERE t.run_id=validated.id AND t.terminal_state=validated.state
                   AND t.attempt_id IS NOT DISTINCT FROM validated.active_attempt_id
                   AND (a.id IS NULL OR a.dispatch_state='TERMINAL')))
               OR (NOT $3 AND predicted.state='SUCCEEDED' AND v.run_id IS NULL)
               OR (NOT $3 AND v.run_id IS NOT NULL AND published.evaluation_id IS NULL)
               OR (NOT $3 AND NOT EXISTS(SELECT 1 FROM app.model_turn_reservations r
                 JOIN app.model_turn_receipts t ON t.reservation_id=r.id AND t.outcome='SUCCEEDED'
                 JOIN app.model_turn_summaries s ON s.reservation_id=r.id
                 WHERE r.session_id=$2 AND r.command_key='mission/result/'||coalesce(v.run_id,f.run_id,c.compile_run_id)::text))))")
            .bind(run.as_uuid()).bind(session).bind(stopping).fetch_one(&mut *tx).await?;
        if unaccounted || (!stopping && proposed) || scientific_pending {
            tx.commit().await?;
            return Ok(false);
        }
        let current = current_lease(&attempt)?;
        let state = runs::accept_terminal(
            locked.run.state,
            Some(if stopping {
                RemoteTerminal::Cancelled
            } else {
                RemoteTerminal::Succeeded
            }),
            &current,
            &current,
            now(&mut tx).await?,
        )?;
        let summary = latest
            .as_ref()
            .map(|row| db::optional_id(row, "artifact_id"))
            .transpose()?
            .flatten();
        let concluding = latest
            .as_ref()
            .map(|row| db::id(row.try_get("id")?))
            .transpose()?;
        // The native Mission result is its public answer, not an OCI JobSpec
        // envelope. Cancellation may honestly have no public answer artifact.
        sqlx::query("UPDATE app.run_attempts SET dispatch_state='TERMINAL',runtime_state=$3,result_manifest_artifact_id=$2,accepted_at=CASE WHEN $2::uuid IS NULL THEN NULL ELSE clock_timestamp() END WHERE id=$1")
            .bind(owner.attempt_id.as_uuid()).bind(summary.map(Id::as_uuid)).bind(db::code(&state)?).execute(&mut *tx).await?;
        let reason = if stopping && summary.is_none() {
            RunReason::RuntimeCancelled
        } else if state == RunState::Cancelled {
            RunReason::ResultDiscardedAfterCancel
        } else {
            RunReason::RuntimeSucceeded
        };
        let evaluations: i64 = sqlx::query_scalar("SELECT count(*) FROM app.experiment_compilations c JOIN app.experiment_validations v ON v.experiment_id=c.experiment_id JOIN app.evaluations ev ON ev.run_id=v.run_id JOIN app.evaluation_publications p ON p.evaluation_id=ev.id WHERE c.mission_run_id=$1")
            .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
        finish(&mut tx,&mut locked,state,reason,json!({"schema_version":1,"source":"NATIVE_MISSION","session_id":session.map(db::id).transpose()?,"concluding_reservation_id":concluding,"summary_artifact_id":summary,"cancelled_after_settlement":stopping,"formal_evaluation":if evaluations>0 {"PUBLISHED"} else {"NOT_PERFORMED"},"formal_evaluation_count":counter(evaluations)?})).await?;
        tx.commit().await?;
        Ok(true)
    }
}
