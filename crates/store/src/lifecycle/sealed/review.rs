//! One original PASS target per queue consumption, not a second workflow engine.
use super::*;
use contracts::evidence::AlphaEvaluateRequestV1;
use sqlx::Acquire;

pub(in crate::lifecycle) async fn pending(tx: &mut Tx<'_>, run: Id) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.mission_review_turns review JOIN app.mission_reviews answer ON answer.reservation_id=review.reservation_id AND answer.decision='PASS' WHERE review.run_id=$1 AND NOT EXISTS(SELECT 1 FROM app.mission_sealed_evaluations held WHERE held.review_reservation_id=review.reservation_id))")
        .bind(run.as_uuid()).fetch_one(&mut **tx).await?)
}

impl Store {
    pub async fn prepare_review_sealed<R, Read, P, Published>(
        &self,
        run: Id,
        owner: &WorkerFence,
        read: R,
        publish: P,
    ) -> Result<(), StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal()
            || locked.run.state == RunState::CancelRequested
            || !locked.admission_open()
            || locked.run.deadline_at <= now(&mut tx).await?
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        let row = sqlx::query("SELECT review.reservation_id,review.alpha_version_id,review.cycle_id,selection.policy_id,c.brief_id FROM app.run_missions mission JOIN app.mission_review_turns review ON review.run_id=mission.run_id JOIN app.mission_reviews answer ON answer.reservation_id=review.reservation_id AND answer.decision='PASS' JOIN app.cycle_selections selection ON selection.cycle_id=review.cycle_id JOIN app.cycle_selection_trials target ON target.cycle_id=review.cycle_id AND target.experiment_id=review.experiment_id JOIN app.research_cycles c ON c.id=review.cycle_id WHERE mission.run_id=$1 AND mission.role='INDEPENDENT_REVIEWER' AND NOT EXISTS(SELECT 1 FROM app.cycle_selection_trials t WHERE t.cycle_id=c.id AND t.review_alpha_version_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM app.mission_review_turns turn JOIN app.mission_reviews assessed ON assessed.reservation_id=turn.reservation_id WHERE turn.run_id=mission.run_id AND turn.experiment_id=t.experiment_id)) AND NOT EXISTS(SELECT 1 FROM app.mission_sealed_evaluations held WHERE held.review_reservation_id=review.reservation_id) ORDER BY target.rank LIMIT 1")
            .bind(run.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(());
        };
        let cycle = db::id(row.try_get("cycle_id")?)?;
        let context =
            crate::cycles::execution_context(&mut tx, db::id(row.try_get("brief_id")?)?).await?;
        let mut limits: JobLimitsV1 = serde_json::from_value(locked.admission.try_get("limits")?)
            .map_err(|_| StoreError::Integrity)?;
        limits.experiments = 0;
        let request = AlphaEvaluateRequestV1 {
            schema_version: SchemaV1,
            cycle_id: cycle,
            policy_id: db::id(row.try_get("policy_id")?)?,
            input_set_id: context.sealed_input_set_id,
            runtime_id: context.runtime_id,
            expected_runtime_revision: context.runtime_revision,
            limits,
        };
        let result = async {
            let (nested, admitted) = admission::admit(
                tx.begin().await?,
                db::id(row.try_get("alpha_version_id")?)?,
                &request,
                "RUNTIME",
                Some(locked.run.deadline_at),
                read,
                publish,
            )
            .await?;
            nested.commit().await?;
            Ok::<_, StoreError>(admitted)
        }
        .await;
        // Reads/publication and their failures may outlast ownership. A stale
        // worker cannot commit either a new task or a Cycle rejection reason.
        fence(&mut tx, &locked.run, owner).await?;
        if locked.run.deadline_at <= now(&mut tx).await? {
            return Err(DomainError::AdmissionClosed.into());
        }
        match result {
            Ok(admitted) => {
                sqlx::query("INSERT INTO app.mission_sealed_evaluations(review_reservation_id,run_id) VALUES($1,$2)")
                    .bind(row.try_get::<uuid::Uuid,_>("reservation_id")?).bind(admitted.resource.id.as_uuid())
                    .execute(&mut *tx).await?;
                sqlx::query(
                    "UPDATE app.research_cycles SET next_action='SEALED_EVALUATION' WHERE id=$1",
                )
                .bind(cycle.as_uuid())
                .execute(&mut *tx)
                .await?;
            }
            Err(StoreError::Domain(DomainError::BudgetExhausted(_))) => {
                sqlx::query("UPDATE app.research_cycles SET state='COMPLETED',outcome='BUDGET_EXHAUSTED',ended_at=clock_timestamp(),next_action='REVIEW_CYCLE_BUDGET' WHERE id=$1")
                    .bind(cycle.as_uuid()).execute(&mut *tx).await?;
            }
            Err(
                StoreError::Invalid("original_validation_required" | "sealed_policy_not_defined")
                | StoreError::Domain(DomainError::Fields(_) | DomainError::CapabilityUnavailable(_)),
            ) => {
                sqlx::query("UPDATE app.research_cycles SET state='WAITING_INPUT',next_action='SEALED_INPUTS_REQUIRE_ATTENTION' WHERE id=$1")
                    .bind(cycle.as_uuid()).execute(&mut *tx).await?;
            }
            Err(error) => return Err(error),
        }
        tx.commit().await?;
        Ok(())
    }
}
