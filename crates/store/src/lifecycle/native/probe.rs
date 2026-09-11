//! Owner-fenced native readiness refresh for an already authorized unsent job.
//! Reuses the Operator probe's immutable native publication contract, not an
//! Operator grant, fake configuration success, or a second observations table.
use super::*;
use contracts::runtime::{RuntimeProbeOutcomeV1, RuntimeProbeViewV1};

pub struct RunProbeTicket {
    run_id: Id,
    owner: WorkerFence,
    runtime_id: Id,
    revision: Revision,
    started_at: DateTime<Utc>,
}

impl Store {
    pub async fn prepare_run_runtime_probe(
        &self,
        id: Id,
        owner: &WorkerFence,
    ) -> Result<Option<RunProbeTicket>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, id).await?;
        let attempt = fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal()
            || locked.run.state == RunState::CancelRequested
            || attempt.try_get::<String, _>("dispatch_state")? != "NOT_SENT"
        {
            tx.commit().await?;
            return Ok(None);
        }
        let registered: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.run_native_tasks WHERE run_id=$1)")
                .bind(id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !registered {
            return Err(StoreError::Invalid("native_task_not_defined"));
        }
        let runtime_id = db::id(locked.admission.try_get("runtime_id")?)?;
        let revision = db::revision(locked.admission.try_get("runtime_revision")?)?;
        let runtime = sqlx::query(
            "SELECT enabled,revision FROM app.runtime_integrations WHERE id=$1 FOR SHARE",
        )
        .bind(runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let current = db::revision(runtime.try_get("revision")?)?;
        if current != revision {
            return Err(StoreError::RevisionConflict { current });
        }
        if !runtime.try_get::<bool, _>("enabled")? {
            return Err(DomainError::CapabilityUnavailable("runtime_disabled").into());
        }
        let started_at = now(&mut tx).await?;
        if started_at >= locked.run.deadline_at {
            return Err(DomainError::AdmissionClosed.into());
        }
        if crate::runtime::latest(&mut tx, runtime_id)
            .await?
            .is_some_and(|latest| {
                latest.integration_revision == revision
                    && latest.valid_until > started_at + Duration::seconds(10)
            })
        {
            tx.commit().await?;
            return Ok(None);
        }
        fence(&mut tx, &locked.run, owner).await?;
        tx.commit().await?;
        Ok(Some(RunProbeTicket {
            run_id: id,
            owner: owner.clone(),
            runtime_id,
            revision,
            started_at,
        }))
    }

    pub async fn complete_run_runtime_probe<F, Fut>(
        &self,
        ticket: RunProbeTicket,
        outcome: RuntimeProbeOutcomeV1,
        publish: F,
    ) -> Result<RuntimeProbeViewV1, StoreError>
    where
        F: FnOnce(Id, Vec<u8>) -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, ticket.run_id).await?;
        let attempt = fence(&mut tx, &locked.run, &ticket.owner).await?;
        if locked.run.state.is_terminal()
            || locked.run.state == RunState::CancelRequested
            || attempt.try_get::<String, _>("dispatch_state")? != "NOT_SENT"
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        let runtime = sqlx::query(
            "SELECT enabled,revision FROM app.runtime_integrations WHERE id=$1 FOR UPDATE",
        )
        .bind(ticket.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let current = db::revision(runtime.try_get("revision")?)?;
        if current != ticket.revision {
            return Err(StoreError::RevisionConflict { current });
        }
        if !runtime.try_get::<bool, _>("enabled")? {
            return Err(DomainError::CapabilityUnavailable("runtime_disabled").into());
        }
        // The native Runtime row lock may have waited past this owner's lease.
        fence(&mut tx, &locked.run, &ticket.owner).await?;
        let clock = now(&mut tx).await?;
        if clock >= locked.run.deadline_at {
            return Err(DomainError::AdmissionClosed.into());
        }
        if let Some(latest) = crate::runtime::latest(&mut tx, ticket.runtime_id).await? {
            if latest.integration_revision == current
                && latest.observed_at >= ticket.started_at
                && latest.valid_until > clock
            {
                // A concurrent native observer already supplied a newer fact.
                // A delayed caller neither republishes files nor replaces it.
                tx.commit().await?;
                return Ok(latest);
            }
        }
        let bytes = crate::runtime::encode_probe(&outcome, ticket.started_at, clock)?;
        let artifact = Id::new();
        publish(artifact, bytes).await?;
        fence(&mut tx, &locked.run, &ticket.owner).await?;
        if now(&mut tx).await? >= locked.run.deadline_at {
            return Err(DomainError::AdmissionClosed.into());
        }
        let observed = crate::runtime::record_probe(
            &mut tx,
            crate::runtime::ProbePublication {
                runtime_id: ticket.runtime_id,
                revision: current,
                started_at: ticket.started_at,
                artifact,
                outcome: &outcome,
                created_by: "RUNTIME",
            },
        )
        .await?;
        tx.commit().await?;
        Ok(observed)
    }
}
