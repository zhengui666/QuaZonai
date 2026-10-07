//! Fenced, monotonic CPU accounting for the native Mission process tree.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourcePurpose {
    Research,
    Reconcile,
}
impl ResourcePurpose {
    fn code(self) -> &'static str {
        match self {
            Self::Research => "RESEARCH",
            Self::Reconcile => "RECONCILE",
        }
    }
}
#[derive(Clone, Debug)]
pub struct ReconciliationPermit {
    pub limits: JobLimitsV1,
    pub deadline_at: chrono::DateTime<chrono::Utc>,
    pub thread_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionResource {
    pub id: Id,
    pub run_id: Id,
    pub attempt_id: Id,
    pub owner_epoch: Revision,
    pub backend: String,
    pub purpose: ResourcePurpose,
    pub effective_limits: JobLimitsV1,
    pub deadline_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub physical_id: Option<String>,
    pub launch_requested: bool,
    pub execution_requested: bool,
    pub cpu_nanoseconds: Option<u64>,
    pub accounting_unknown: bool,
    pub final_accounted: bool,
    pub closed: bool,
}
impl MissionResource {
    pub fn name(&self) -> String {
        format!(
            "quazonai-mission-{}-{}-{}-{}",
            self.run_id,
            self.attempt_id,
            self.owner_epoch.get(),
            self.id
        )
    }
    pub fn reclaimable_by(&self, run: Id, owner: &WorkerFence) -> bool {
        self.run_id == run
            && self.attempt_id == owner.attempt_id
            && (self.owner_epoch < owner.owner_epoch
                || (self.owner_epoch == owner.owner_epoch && !self.launch_requested))
            && !self.closed
    }
}
fn resource(row: &PgRow) -> Result<MissionResource, StoreError> {
    Ok(MissionResource {
        id: db::id(row.try_get("id")?)?,
        run_id: db::id(row.try_get("run_id")?)?,
        attempt_id: db::id(row.try_get("attempt_id")?)?,
        owner_epoch: db::revision(row.try_get("owner_epoch")?)?,
        backend: row.try_get("backend")?,
        purpose: match row.try_get::<String, _>("purpose")?.as_str() {
            "RESEARCH" => ResourcePurpose::Research,
            "RECONCILE" => ResourcePurpose::Reconcile,
            _ => return Err(StoreError::Integrity),
        },
        effective_limits: serde_json::from_value(row.try_get("effective_limits")?)
            .map_err(|_| StoreError::Integrity)?,
        deadline_at: row.try_get("deadline_at")?,
        created_at: row.try_get("created_at")?,
        physical_id: row.try_get("physical_id")?,
        launch_requested: row.try_get("launch_requested")?,
        execution_requested: row.try_get("execution_requested")?,
        cpu_nanoseconds: row
            .try_get::<Option<i64>, _>("cpu_nanoseconds")?
            .map(|n| u64::try_from(n).map_err(|_| StoreError::Integrity))
            .transpose()?,
        accounting_unknown: row.try_get("accounting_unknown")?,
        final_accounted: row.try_get("final_accounted")?,
        closed: row.try_get("closed")?,
    })
}
async fn authority(tx: &mut Tx<'_>, run: Id, owner: &WorkerFence) -> Result<LockedRun, StoreError> {
    let locked = lock_run(tx, run).await?;
    fence(tx, &locked.run, owner).await?;
    if locked.run.kind != RunKind::AgentResearch || locked.run.state.is_terminal() {
        return Err(StoreError::Conflict);
    }
    Ok(locked)
}
async fn total(
    tx: &mut Tx<'_>,
    run: Id,
    purpose: ResourcePurpose,
) -> Result<Option<u64>, StoreError> {
    let values: Vec<(Option<i64>,bool,bool)> = sqlx::query_as("SELECT cpu_nanoseconds,accounting_unknown,history_unknown FROM app.mission_resources WHERE run_id=$1 AND purpose=$2")
        .bind(run.as_uuid()).bind(purpose.code()).fetch_all(&mut **tx).await?;
    let mut sum = 0_u64;
    for (value, unknown, history_unknown) in values {
        if unknown || history_unknown {
            return Ok(None);
        }
        let Some(value) = value else {
            return Ok(None);
        };
        sum = sum
            .checked_add(u64::try_from(value).map_err(|_| StoreError::Integrity)?)
            .ok_or(StoreError::Integrity)?;
    }
    Ok(Some(sum))
}
fn grant(limits: &JobLimitsV1) -> Result<Option<u64>, StoreError> {
    limits
        .cpu_seconds
        .map(|cpu| {
            cpu.get()
                .checked_mul(1_000_000_000)
                .ok_or(StoreError::Integrity)
        })
        .transpose()
}

async fn reconciliation(
    tx: &mut Tx<'_>,
    locked: &LockedRun,
    owner: &WorkerFence,
) -> Result<ReconciliationPermit, StoreError> {
    let run = locked.run.id;
    if locked.run.state != RunState::CancelRequested {
        return Err(StoreError::Conflict);
    }
    let session = sqlx::query("SELECT s.id,s.thread_id FROM app.codex_sessions s WHERE s.run_id=$1 AND EXISTS(SELECT 1 FROM app.model_turn_reservations r JOIN app.model_turn_dispatches d ON d.reservation_id=r.id WHERE r.run_id=$1 AND r.attempt_id=$2 AND r.session_id=s.id AND NOT EXISTS(SELECT 1 FROM app.model_turn_receipts x WHERE x.reservation_id=r.id))")
        .bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("mission_reconciliation_required"))?;
    let original: JobLimitsV1 = serde_json::from_value(locked.admission.try_get("limits")?)
        .map_err(|_| StoreError::Integrity)?;
    let limits = JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: Some(DbCounter::new(110).map_err(|_| StoreError::Integrity)?),
        wall_seconds: Some(110),
        memory_mib: original.memory_mib,
        output_bytes: original.output_bytes,
    };
    // Same clock value binds both timestamps. Existing immutable allowance wins.
    sqlx::query("INSERT INTO app.mission_reconciliation_limits(run_id,attempt_id,session_id,limits,created_at,deadline_at) SELECT $1,$2,$3,$4,t,t+interval '110 seconds' FROM (SELECT clock_timestamp() t) observed ON CONFLICT(run_id) DO NOTHING")
        .bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).bind(session.try_get::<uuid::Uuid,_>("id")?).bind(db::json(&limits)?).execute(&mut **tx).await?;
    let row = sqlx::query("SELECT * FROM app.mission_reconciliation_limits WHERE run_id=$1")
        .bind(run.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    if row.try_get::<uuid::Uuid, _>("attempt_id")? != owner.attempt_id.as_uuid()
        || row.try_get::<uuid::Uuid, _>("session_id")? != session.try_get::<uuid::Uuid, _>("id")?
        || row.try_get::<chrono::DateTime<chrono::Utc>, _>("deadline_at")? <= now(tx).await?
    {
        return Err(StoreError::Invalid("mission_reconciliation_required"));
    }
    let effective: JobLimitsV1 =
        serde_json::from_value(row.try_get("limits")?).map_err(|_| StoreError::Integrity)?;
    if effective != limits {
        return Err(StoreError::Integrity);
    }
    let used = total(tx, run, ResourcePurpose::Reconcile).await?;
    let maximum = grant(&effective)?.ok_or(StoreError::Integrity)?;
    if used.is_none_or(|used| used >= maximum) {
        return Err(StoreError::Invalid("mission_reconciliation_required"));
    }
    Ok(ReconciliationPermit {
        limits: effective,
        deadline_at: row.try_get("deadline_at")?,
        thread_id: session.try_get("thread_id")?,
    })
}
impl Store {
    /// Existing finite in-flight Missions retain their original finite rate /
    /// absolute deadline semantics. Never synthesize historical CPU as zero.
    pub async fn mission_resource_accounting_required(
        &self,
        run: Id,
        owner: &WorkerFence,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = authority(&mut tx, run, owner).await?;
        let limits: JobLimitsV1 = serde_json::from_value(locked.admission.try_get("limits")?)
            .map_err(|_| StoreError::Integrity)?;
        let (ledger,session):(bool,bool)=sqlx::query_as("SELECT EXISTS(SELECT 1 FROM app.mission_resources WHERE run_id=$1),EXISTS(SELECT 1 FROM app.codex_sessions WHERE run_id=$1)")
            .bind(run.as_uuid()).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(ledger || !session || limits.wall_seconds.is_none())
    }

    pub async fn begin_mission_resource_launch(
        &self,
        run: Id,
        owner: &WorkerFence,
        id: Id,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        authority(&mut tx, run, owner).await?;
        let changed=sqlx::query("UPDATE app.mission_resources SET launch_requested=true,execution_requested=(backend='HOST') WHERE id=$1 AND run_id=$2 AND attempt_id=$3 AND owner_epoch=$4 AND NOT launch_requested AND NOT closed")
            .bind(id.as_uuid()).bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).bind(owner.owner_epoch.get() as i64).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(StoreError::Conflict);
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn begin_mission_resource_execution(
        &self,
        run: Id,
        owner: &WorkerFence,
        id: Id,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        authority(&mut tx, run, owner).await?;
        let changed=sqlx::query("UPDATE app.mission_resources SET execution_requested=true WHERE id=$1 AND run_id=$2 AND attempt_id=$3 AND owner_epoch=$4 AND backend='DOCKER' AND launch_requested AND physical_id IS NOT NULL AND NOT execution_requested AND NOT closed")
            .bind(id.as_uuid()).bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).bind(owner.owner_epoch.get() as i64).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(StoreError::Conflict);
        }
        tx.commit().await?;
        Ok(())
    }

    /// The caller may inspect/reclaim only older epochs returned under a fresh
    /// current DB lease. A same-epoch duplicate cannot acquire cleanup authority.
    pub async fn mission_open_resources(
        &self,
        run: Id,
        owner: &WorkerFence,
    ) -> Result<Vec<MissionResource>, StoreError> {
        let mut tx = self.pool.begin().await?;
        authority(&mut tx, run, owner).await?;
        let rows = sqlx::query(
            "SELECT * FROM app.mission_resources WHERE run_id=$1 AND NOT closed FOR UPDATE",
        )
        .bind(run.as_uuid())
        .fetch_all(&mut *tx)
        .await?;
        let resources = rows.iter().map(resource).collect::<Result<Vec<_>, _>>()?;
        if resources.iter().any(|r| !r.reclaimable_by(run, owner)) {
            return Err(StoreError::Conflict);
        }
        tx.commit().await?;
        Ok(resources)
    }
    pub async fn prepare_mission_reconciliation(
        &self,
        run: Id,
        owner: &WorkerFence,
    ) -> Result<ReconciliationPermit, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = authority(&mut tx, run, owner).await?;
        let result = reconciliation(&mut tx, &locked, owner).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn reserve_mission_resource(
        &self,
        run: Id,
        owner: &WorkerFence,
        backend: &str,
    ) -> Result<(MissionResource, Option<u64>), StoreError> {
        self.reserve_mission_resource_for(run, owner, backend, ResourcePurpose::Research)
            .await
    }
    pub async fn reserve_mission_resource_for(
        &self,
        run: Id,
        owner: &WorkerFence,
        backend: &str,
        purpose: ResourcePurpose,
    ) -> Result<(MissionResource, Option<u64>), StoreError> {
        if !matches!(backend, "HOST" | "DOCKER") {
            return Err(StoreError::Invalid("mission_resource_backend"));
        }
        let mut tx = self.pool.begin().await?;
        let locked = authority(&mut tx, run, owner).await?;
        let (limits, deadline) = if purpose == ResourcePurpose::Reconcile {
            let permit = reconciliation(&mut tx, &locked, owner).await?;
            (permit.limits, Some(permit.deadline_at))
        } else {
            if locked.run.state == RunState::CancelRequested {
                return Err(DomainError::AdmissionClosed.into());
            }
            (
                serde_json::from_value::<JobLimitsV1>(locked.admission.try_get("limits")?)
                    .map_err(|_| StoreError::Integrity)?,
                locked.run.deadline_at,
            )
        };
        let open: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.mission_resources WHERE run_id=$1 AND NOT closed)",
        )
        .bind(run.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if open {
            return Err(StoreError::Conflict);
        }
        // A migrated already-started Mission without a ledger has unknown CPU.
        let history: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.mission_resources WHERE run_id=$1 AND purpose=$2)",
        )
        .bind(run.as_uuid())
        .bind(purpose.code())
        .fetch_one(&mut *tx)
        .await?;
        let prior_session: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.codex_sessions WHERE run_id=$1)")
                .bind(run.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let cap = grant(&limits)?;
        let missing_history = purpose == ResourcePurpose::Research && prior_session && !history;
        if missing_history && cap.is_some() {
            return Err(StoreError::Invalid("mission_resource_history_unknown"));
        }
        let used = if missing_history {
            None
        } else {
            total(&mut tx, run, purpose).await?
        };
        if cap.is_some() && used.is_none() {
            return Err(StoreError::Invalid("mission_cpu_usage_unknown"));
        }
        if cap.zip(used).is_some_and(|(cap, used)| used >= cap) {
            return Err(DomainError::BudgetExhausted("cpu_seconds").into());
        }
        let row = sqlx::query("INSERT INTO app.mission_resources(id,run_id,attempt_id,owner_epoch,backend,history_unknown,purpose,effective_limits,deadline_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *")
            .bind(Id::new().as_uuid()).bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).bind(owner.owner_epoch.get() as i64).bind(backend).bind(missing_history).bind(purpose.code()).bind(db::json(&limits)?).bind(deadline)
            .fetch_one(&mut *tx).await?;
        let result = resource(&row)?;
        tx.commit().await?;
        Ok((result, used))
    }
    pub async fn bind_mission_resource(
        &self,
        run: Id,
        owner: &WorkerFence,
        id: Id,
        physical: &str,
    ) -> Result<(), StoreError> {
        domain::control::text(physical, 1, 4096, false)?;
        let mut tx = self.pool.begin().await?;
        authority(&mut tx, run, owner).await?;
        let changed = sqlx::query("UPDATE app.mission_resources SET physical_id=$5 WHERE id=$1 AND run_id=$2 AND attempt_id=$3 AND owner_epoch=$4 AND launch_requested AND NOT closed AND (physical_id IS NULL OR physical_id=$5)")
            .bind(id.as_uuid()).bind(run.as_uuid()).bind(owner.attempt_id.as_uuid()).bind(owner.owner_epoch.get() as i64).bind(physical).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(StoreError::Conflict);
        }
        tx.commit().await?;
        Ok(())
    }
    /// Final accounting means the entire resource was observed frozen. It is
    /// committed BEFORE destruction, so a crash cannot reset its CPU baseline.
    pub async fn checkpoint_mission_resource(
        &self,
        run: Id,
        owner: &WorkerFence,
        id: Id,
        nanos: Option<u64>,
        final_accounted: bool,
        closed: bool,
    ) -> Result<bool, StoreError> {
        let nanos = nanos
            .map(|n| i64::try_from(n).map_err(|_| StoreError::Integrity))
            .transpose()?;
        if final_accounted && nanos.is_none() {
            return Err(StoreError::Invalid("resource_final_usage_missing"));
        }
        let mut tx = self.pool.begin().await?;
        let locked = authority(&mut tx, run, owner).await?;
        let row =
            sqlx::query("SELECT * FROM app.mission_resources WHERE id=$1 AND run_id=$2 FOR UPDATE")
                .bind(id.as_uuid())
                .bind(run.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let prior = resource(&row)?;
        let cap = grant(&prior.effective_limits)?;
        // A confirmed stop may have no exact final sample. Closing honestly
        // with unknown usage is permitted; spending another finite research
        // grant remains forbidden by reserve_mission_resource's total check.
        if (cap.is_some() && nanos.is_none() && !closed)
            || (closed && !final_accounted && nanos.is_some())
        {
            return Err(StoreError::Invalid("mission_cpu_usage_unknown"));
        }

        if (!prior.execution_requested && nanos.is_some_and(|n| n != 0))
            || prior.attempt_id != owner.attempt_id
            || prior.owner_epoch > owner.owner_epoch
            || nanos
                .zip(prior.cpu_nanoseconds)
                .is_some_and(|(n, prior)| n < prior as i64)
            || (prior.final_accounted
                && (nanos != prior.cpu_nanoseconds.map(|n| n as i64) || !final_accounted))
            || (prior.closed && !closed)
        {
            return Err(StoreError::Conflict);
        }
        sqlx::query("UPDATE app.mission_resources SET cpu_nanoseconds=$2,accounting_unknown=$3,final_accounted=$4,closed=$5 WHERE id=$1")
            .bind(id.as_uuid()).bind(nanos).bind(nanos.is_none()).bind(final_accounted).bind(closed).execute(&mut *tx).await?;
        let used = total(&mut tx, run, prior.purpose).await?;
        let exhausted = cap.zip(used).is_some_and(|(cap, used)| used >= cap);
        let stop_required = exhausted || (cap.is_some() && closed && used.is_none());
        if stop_required && locked.run.state != RunState::CancelRequested {
            let state = runs::request_cancel(locked.run.state)?;
            sqlx::query("UPDATE app.runs SET state=$2,cancellation_requested_at=clock_timestamp() WHERE id=$1")
                .bind(run.as_uuid()).bind(db::code(&state)?).execute(&mut *tx).await?;
            append(
                &mut tx,
                run,
                RunEventKind::StateChanged,
                RunReason::CancelRequested,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(exhausted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_requires_exact_run_attempt_and_strictly_older_epoch() {
        let run = Id::new();
        let attempt = Id::new();
        let mut owner = WorkerFence {
            attempt_id: attempt,
            worker_owner_id: "replacement".into(),
            owner_epoch: Revision::INITIAL.next().unwrap(),
        };
        let mut r = MissionResource {
            id: Id::new(),
            run_id: run,
            attempt_id: attempt,
            owner_epoch: Revision::INITIAL,
            backend: "HOST".into(),
            purpose: ResourcePurpose::Research,
            effective_limits: serde_json::from_value(serde_json::json!({"schema_version":1,"experiments":0,"cpu_seconds":null,"wall_seconds":null,"memory_mib":64,"output_bytes":null})).unwrap(),
            deadline_at: None,
            created_at: chrono::Utc::now(),
            physical_id: None,
            launch_requested: true,
            execution_requested: true,
            cpu_nanoseconds: Some(9),
            accounting_unknown: false,
            final_accounted: false,
            closed: false,
        };
        assert!(r.reclaimable_by(run, &owner));
        assert!(!r.reclaimable_by(Id::new(), &owner));
        owner.attempt_id = Id::new();
        assert!(!r.reclaimable_by(run, &owner));
        owner.attempt_id = attempt;
        owner.owner_epoch = Revision::INITIAL;
        assert!(!r.reclaimable_by(run, &owner));
        owner.owner_epoch = Revision::INITIAL.next().unwrap();
        r.closed = true;
        assert!(!r.reclaimable_by(run, &owner));
    }
    #[test]
    fn resource_identity_includes_all_fences_and_launch_nonce() {
        let r = MissionResource {
            id: Id::new(),
            run_id: Id::new(),
            attempt_id: Id::new(),
            owner_epoch: Revision::INITIAL,
            backend: "HOST".into(),
            purpose: ResourcePurpose::Research,
            effective_limits: serde_json::from_value(serde_json::json!({"schema_version":1,"experiments":0,"cpu_seconds":null,"wall_seconds":null,"memory_mib":64,"output_bytes":null})).unwrap(),
            deadline_at: None,
            created_at: chrono::Utc::now(),
            physical_id: None,
            launch_requested: true,
            execution_requested: true,
            cpu_nanoseconds: Some(0),
            accounting_unknown: false,
            final_accounted: false,
            closed: false,
        };
        for id in [r.id, r.run_id, r.attempt_id] {
            assert!(r.name().contains(&id.to_string()));
        }
        let mut next = r.clone();
        next.owner_epoch = next.owner_epoch.next().unwrap();
        assert_ne!(r.name(), next.name());
        next = r.clone();
        next.id = Id::new();
        assert_ne!(r.name(), next.name());
    }
}
