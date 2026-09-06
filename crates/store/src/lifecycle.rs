//! Native PGMQ admission and fenced Run/Attempt transitions. Only trusted domain
//! services receive this Store; external workers, shell code and Agent tools do
//! not receive database credentials or a generic dispatch/terminal endpoint.
use crate::{
    authority::{self, Actor},
    commands, db,
    turns::WorkerFence,
    Store, StoreError,
};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    budget::{BudgetV1, StopRuleV1},
    control::{CommandResult, MachineScope, Page, PrincipalKind},
    lifecycle::*,
    runs::{ProjectState, RunKind, RunSnapshotV1, RunState},
    DbCounter, Id, Revision, SchemaV1,
};
use domain::{
    admission::{self, BudgetUsage, CostUsage, Reservation},
    runs::{self, AttemptLease, RemoteTerminal},
    DomainError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;
const FIELDS: &str = "r.id::uuid,r.project_id::uuid,r.cycle_id::uuid,r.kind,r.input_set_id::uuid,r.state,r.current_attempt_no::bigint,r.active_attempt_id::uuid,r.last_event_seq::bigint,r.deadline_at::timestamptz,r.cancellation_requested_at::timestamptz,r.terminal_reason_code,r.queued_at::timestamptz,r.started_at::timestamptz,r.finished_at::timestamptz,r.revision::bigint";

/// A caller must already have authorized the specific research operation. This
/// is intentionally not Deserialize and is not exposed as a generic HTTP tool.
#[derive(Clone)]
pub struct RunSubmission {
    pub cycle_id: Id,
    pub input_set_id: Id,
    pub runtime_id: Id,
    pub runtime_revision: Revision,
    pub kind: RunKind,
    pub limits: JobLimitsV1,
}
/// Explicit bounded non-research job limits supplied by trusted deployment services.
#[derive(Clone)]
pub struct StandaloneRunSubmission {
    pub project_id: Id,
    pub input_set_id: Id,
    pub runtime_id: Id,
    pub runtime_revision: Revision,
    pub kind: RunKind,
    pub limits: JobLimitsV1,
    pub max_parallel_runs: u16,
}
#[derive(Clone, Debug)]
pub struct RunMessage {
    pub message_id: i64,
    pub run_id: Id,
    pub read_count: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueuePayload {
    schema_version: SchemaV1,
    run_id: Id,
}

/// Native transport settings are a trusted runtime snapshot, never an Agent's
/// URL, command, image, environment or credential injection surface.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSnapshot {
    pub schema_version: SchemaV1,
    pub endpoint: String,
    pub credential_ref: String,
    pub tls_policy: String,
    pub protocol_version: String,
    pub allowed_capabilities: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextRuntimeAction {
    PrepareDispatch,
    Reconcile,
    Cancel,
}
#[derive(Clone)]
pub struct RunLease {
    pub run: RunSnapshotV1,
    pub fence: WorkerFence,
    pub attempt_no: u32,
    pub external_job_id: String,
    pub lease_expires_at: DateTime<Utc>,
    pub runtime: RuntimeSnapshot,
    pub limits: JobLimitsV1,
    pub action: NextRuntimeAction,
}
#[derive(Clone)]
pub enum ClaimResult {
    Busy,
    Terminal(RunSnapshotV1),
    Leased(Box<RunLease>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeOutcome {
    Succeeded,
    Failed,
    Cancelled,
    ConfirmedAbsent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureClass {
    RetryableInfra,
    PermanentConfig,
    InvalidInput,
    Cancelled,
    ResourceLimit,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TerminalObservation {
    pub schema_version: SchemaV1,
    pub external_job_id: String,
    pub outcome: NativeOutcome,
    pub manifest_artifact_id: Option<Id>,
    pub failure_class: Option<FailureClass>,
    pub failure_code: Option<String>,
    pub observed_at: DateTime<Utc>,
}
fn counter(n: i64) -> Result<DbCounter, StoreError> {
    DbCounter::new(u64::try_from(n).map_err(|_| StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)
}
fn snapshot(row: &PgRow) -> Result<RunSnapshotV1, StoreError> {
    Ok(RunSnapshotV1 {
        schema_version: SchemaV1,
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        cycle_id: db::optional_id(row, "cycle_id")?,
        kind: db::enum_value(row, "kind")?,
        input_set_id: db::id(row.try_get("input_set_id")?)?,
        state: db::enum_value(row, "state")?,
        current_attempt_no: u32::try_from(row.try_get::<i64, _>("current_attempt_no")?)
            .map_err(|_| StoreError::Integrity)?,
        active_attempt_id: db::optional_id(row, "active_attempt_id")?,
        last_event_seq: counter(row.try_get("last_event_seq")?)?,
        deadline_at: row.try_get("deadline_at")?,
        cancellation_requested_at: row.try_get("cancellation_requested_at")?,
        terminal_reason_code: row.try_get("terminal_reason_code")?,
        queued_at: row.try_get("queued_at")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        revision: db::revision(row.try_get("revision")?)?,
    })
}
async fn now(tx: &mut Tx<'_>) -> Result<DateTime<Utc>, StoreError> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?)
}
async fn run_row(tx: &mut Tx<'_>, id: Id, write: bool) -> Result<PgRow, StoreError> {
    let query = format!(
        "SELECT {FIELDS} FROM app.runs r WHERE r.id=$1 {}",
        if write {
            "FOR UPDATE OF r"
        } else {
            "FOR SHARE OF r"
        }
    );
    sqlx::query(&query)
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)
}
async fn append(
    tx: &mut Tx<'_>,
    id: Id,
    kind: RunEventKind,
    reason: RunReason,
) -> Result<RunSnapshotV1, StoreError> {
    let run = snapshot(&run_row(tx, id, true).await?)?;
    let next = run
        .last_event_seq
        .checked_add(1)
        .ok_or(StoreError::Integrity)?;
    let payload = RunStatePayload {
        schema_version: SchemaV1,
        state: run.state,
        reason,
    };
    sqlx::query("INSERT INTO app.run_events(run_id,seq,attempt_id,event_type,schema_version,payload,occurred_at) VALUES($1,$2,$3,$4,1,$5,clock_timestamp())")
        .bind(id.as_uuid()).bind(next.get() as i64).bind(run.active_attempt_id.map(Id::as_uuid)).bind(kind.code()).bind(db::json(&payload)?).execute(&mut **tx).await?;
    snapshot(&run_row(tx, id, true).await?)
}
struct LockedRun {
    run: RunSnapshotV1,
    admission: PgRow,
    project_state: ProjectState,
    cycle_state: Option<String>,
}
impl LockedRun {
    fn admission_open(&self) -> bool {
        if self.run.cycle_id.is_some() {
            self.project_state == ProjectState::Active
                && self.cycle_state.as_deref() == Some("RUNNING")
        } else {
            standalone_kind(self.run.kind)
                && (self.project_state != ProjectState::Archived
                    || self.run.kind == RunKind::Export)
        }
    }
}
fn standalone_kind(kind: RunKind) -> bool {
    matches!(
        kind,
        RunKind::Import | RunKind::Export | RunKind::DataValidate
    )
}
/// Establish the same project -> cycle -> Run ordering used by model spending.
async fn lock_run(tx: &mut Tx<'_>, id: Id) -> Result<LockedRun, StoreError> {
    let refs = sqlx::query("SELECT project_id::uuid,cycle_id::uuid FROM app.runs WHERE id=$1")
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let project: uuid::Uuid = refs.try_get("project_id")?;
    let cycle: Option<uuid::Uuid> = refs.try_get("cycle_id")?;
    let p = sqlx::query("SELECT state FROM app.projects WHERE id=$1 FOR UPDATE")
        .bind(project)
        .fetch_one(&mut **tx)
        .await?;
    let cycle_state = match cycle {
        Some(cycle) => Some(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM app.research_cycles WHERE id=$1 AND project_id=$2 FOR UPDATE",
            )
            .bind(cycle)
            .bind(project)
            .fetch_one(&mut **tx)
            .await?,
        ),
        None => None,
    };
    let run = snapshot(&run_row(tx, id, true).await?)?;
    let admission = sqlx::query(
        "SELECT * FROM app.run_admissions WHERE run_id=$1 AND project_id=$2 AND cycle_id IS NOT DISTINCT FROM $3::uuid",
    )
    .bind(id.as_uuid())
    .bind(project)
    .bind(cycle)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::Invalid("run_has_no_admission"))?;
    Ok(LockedRun {
        run,
        admission,
        project_state: db::enum_value(&p, "state")?,
        cycle_state,
    })
}
async fn attempt(tx: &mut Tx<'_>, run: &RunSnapshotV1) -> Result<PgRow, StoreError> {
    let id = run
        .active_attempt_id
        .ok_or(StoreError::Invalid("run_has_no_attempt"))?;
    Ok(
        sqlx::query("SELECT * FROM app.run_attempts WHERE id=$1 AND run_id=$2 FOR UPDATE")
            .bind(id.as_uuid())
            .bind(run.id.as_uuid())
            .fetch_one(&mut **tx)
            .await?,
    )
}
fn current_lease(row: &PgRow) -> Result<AttemptLease, StoreError> {
    Ok(AttemptLease {
        attempt_no: u32::try_from(row.try_get::<i64, _>("attempt_no")?)
            .map_err(|_| StoreError::Integrity)?,
        worker_owner_id: row.try_get("worker_owner_id")?,
        owner_epoch: db::revision(row.try_get("owner_epoch")?)?,
        lease_expires_at: row.try_get("lease_expires_at")?,
    })
}
async fn fence(
    tx: &mut Tx<'_>,
    run: &RunSnapshotV1,
    presented: &WorkerFence,
) -> Result<PgRow, StoreError> {
    if run.active_attempt_id != Some(presented.attempt_id) {
        return Err(DomainError::StaleAttempt.into());
    }
    let row = attempt(tx, run).await?;
    let current = current_lease(&row)?;
    runs::validate_owner(
        &current,
        &AttemptLease {
            attempt_no: run.current_attempt_no,
            worker_owner_id: presented.worker_owner_id.clone(),
            owner_epoch: presented.owner_epoch,
            lease_expires_at: current.lease_expires_at,
        },
        now(tx).await?,
    )?;
    Ok(row)
}
fn lease_view(locked: LockedRun, row: &PgRow) -> Result<RunLease, StoreError> {
    let action = if locked.run.state == RunState::CancelRequested {
        NextRuntimeAction::Cancel
    } else if row.try_get::<String, _>("dispatch_state")? == "NOT_SENT" {
        NextRuntimeAction::PrepareDispatch
    } else {
        NextRuntimeAction::Reconcile
    };
    Ok(RunLease {
        fence: WorkerFence {
            attempt_id: locked.run.active_attempt_id.ok_or(StoreError::Integrity)?,
            worker_owner_id: row.try_get("worker_owner_id")?,
            owner_epoch: db::revision(row.try_get("owner_epoch")?)?,
        },
        attempt_no: locked.run.current_attempt_no,
        external_job_id: row
            .try_get::<Option<String>, _>("external_job_id")?
            .ok_or(StoreError::Integrity)?,
        lease_expires_at: row.try_get("lease_expires_at")?,
        runtime: serde_json::from_value(locked.admission.try_get("runtime_snapshot")?)
            .map_err(|_| StoreError::Integrity)?,
        limits: serde_json::from_value(locked.admission.try_get("limits")?)
            .map_err(|_| StoreError::Integrity)?,
        run: locked.run,
        action,
    })
}
async fn queue_matches(tx: &mut Tx<'_>, message: &RunMessage) -> Result<(), StoreError> {
    if message.message_id <= 0 {
        return Err(StoreError::Invalid("queue_message"));
    }
    let payload: Value =
        sqlx::query_scalar("SELECT message FROM pgmq.q_runs WHERE msg_id=$1 FOR UPDATE")
            .bind(message.message_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    let payload: QueuePayload =
        serde_json::from_value(payload).map_err(|_| StoreError::Integrity)?;
    let _version = payload.schema_version;
    if payload.run_id != message.run_id {
        return Err(StoreError::Conflict);
    }
    Ok(())
}
async fn finish(
    tx: &mut Tx<'_>,
    locked: &mut LockedRun,
    state: RunState,
    reason: RunReason,
    observation: Value,
) -> Result<RunSnapshotV1, StoreError> {
    let limits: JobLimitsV1 = serde_json::from_value(locked.admission.try_get("limits")?)
        .map_err(|_| StoreError::Integrity)?;
    // Retain consumed/cancelled trials. Management jobs have no research
    // budget and cannot be used to erase or refund a cycle's reservations.
    if let Some(cycle) = locked.run.cycle_id {
        let changed=sqlx::query("UPDATE app.research_cycles SET reserved_experiments=reserved_experiments-$2,used_experiments=used_experiments+$2 WHERE id=$1 AND reserved_experiments >= $2")
            .bind(cycle.as_uuid()).bind(i64::from(limits.experiments)).execute(&mut **tx).await?;
        if changed.rows_affected() != 1 {
            return Err(StoreError::Integrity);
        }
    } else if limits.experiments != 0 || !standalone_kind(locked.run.kind) {
        return Err(StoreError::Integrity);
    }
    sqlx::query("UPDATE app.runs SET state=$2,terminal_reason_code=$3,finished_at=clock_timestamp() WHERE id=$1")
        .bind(locked.run.id.as_uuid()).bind(db::code(&state)?).bind(db::code(&reason)?).execute(&mut **tx).await?;
    let result = append(tx, locked.run.id, RunEventKind::StateChanged, reason).await?;
    sqlx::query("INSERT INTO app.run_terminal_receipts(run_id,attempt_id,observation,terminal_state,result_snapshot) VALUES($1,$2,$3,$4,$5)")
        .bind(result.id.as_uuid()).bind(result.active_attempt_id.map(Id::as_uuid)).bind(observation).bind(db::code(&state)?).bind(db::json(&result)?).execute(&mut **tx).await?;
    Ok(result)
}

impl Store {
    /// Native database transaction is the sole admission/queue commit boundary.
    /// No model request is sent here: each later native turn separately reserves
    /// the existing durable token/cost/turn ledger before the first wire write.
    pub async fn enqueue_run(
        &self,
        key: &str,
        request: &RunSubmission,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError> {
        commands::key(key)?;
        let normalized = json!({"schema_version":1,"cycle_id":request.cycle_id,"input_set_id":request.input_set_id,"runtime_id":request.runtime_id,"runtime_revision":request.runtime_revision,"kind":request.kind,"limits":request.limits});
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id::uuid FROM app.research_cycles WHERE id=$1")
                .bind(request.cycle_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let p = sqlx::query("SELECT state FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(project)
            .fetch_one(&mut *tx)
            .await?;
        let c=sqlx::query("SELECT brief_id::uuid,state,budget_snapshot,reserved_experiments::bigint,used_experiments::bigint,reserved_cpu_seconds::bigint FROM app.research_cycles WHERE id=$1 AND project_id=$2 FOR UPDATE")
            .bind(request.cycle_id.as_uuid()).bind(project).fetch_one(&mut *tx).await?;
        if let Some(row)=sqlx::query("SELECT normalized_request,initial_snapshot FROM app.run_admissions WHERE cycle_id=$1 AND command_key=$2").bind(request.cycle_id.as_uuid()).bind(key).fetch_optional(&mut *tx).await?{
            if row.try_get::<Value,_>("normalized_request")?!=normalized{return Err(StoreError::IdempotencyConflict);}
            let run=serde_json::from_value(row.try_get("initial_snapshot")?).map_err(|_|StoreError::Integrity)?;
            tx.commit().await?;return Ok(CommandResult{schema_version:SchemaV1,replayed:true,resource:run});
        }
        let b=sqlx::query("SELECT state,budget,stop_rule FROM app.research_briefs WHERE id=$1 AND project_id=$2 FOR SHARE").bind(c.try_get::<uuid::Uuid,_>("brief_id")?).bind(project).fetch_one(&mut *tx).await?;
        if b.try_get::<String, _>("state")? != "FROZEN"
            || c.try_get::<String, _>("state")? != "RUNNING"
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        let budget_json: Value = b.try_get("budget")?;
        if budget_json != c.try_get::<Value, _>("budget_snapshot")? {
            return Err(StoreError::Invalid("frozen_budget_snapshot_mismatch"));
        }
        let budget: BudgetV1 =
            serde_json::from_value(budget_json).map_err(|_| StoreError::Integrity)?;
        let stop: StopRuleV1 =
            serde_json::from_value(b.try_get("stop_rule")?).map_err(|_| StoreError::Integrity)?;
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            request.input_set_id,
            db::id(project)?,
            request.runtime_id,
        )
        .await?;
        let r = sqlx::query("SELECT * FROM app.runtime_integrations WHERE id=$1 FOR SHARE")
            .bind(request.runtime_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let caps: Vec<String> = r.try_get("allowed_capabilities")?;
        if !r.try_get::<bool, _>("enabled")?
            || r.try_get::<i64, _>("revision")? != request.runtime_revision.get() as i64
            || !caps.contains(&db::code(&request.kind)?)
        {
            return Err(DomainError::CapabilityUnavailable("runtime_job_kind_or_revision").into());
        }
        let runtime = RuntimeSnapshot {
            schema_version: SchemaV1,
            endpoint: r.try_get("endpoint")?,
            credential_ref: r.try_get("credential_ref")?,
            tls_policy: r.try_get("tls_policy")?,
            protocol_version: r.try_get("protocol_version")?,
            allowed_capabilities: caps,
        };
        let active:i64=sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE cycle_id=$1 AND state NOT IN ('SUCCEEDED','FAILED','CANCELLED')").bind(request.cycle_id.as_uuid()).fetch_one(&mut *tx).await?;
        let ledger=sqlx::query("SELECT coalesce(sum(reserved_tokens),0)::bigint reserved_tokens,coalesce(sum(used_tokens),0)::bigint used_tokens,coalesce(sum(reserved_cost),0)::numeric reserved_cost,coalesce(sum(used_cost),0)::numeric used_cost FROM app.model_turn_accounting WHERE cycle_id=$1")
            .bind(request.cycle_id.as_uuid()).fetch_one(&mut *tx).await?;
        let mismatch:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.model_turn_reservations WHERE cycle_id=$1 AND (cost_currency IS DISTINCT FROM $2::text OR (reserved_cost IS NULL) <> ($2::text IS NULL)))")
            .bind(request.cycle_id.as_uuid()).bind(&budget.cost_currency).fetch_one(&mut *tx).await?;
        if mismatch {
            return Err(StoreError::Integrity);
        }
        let decimal = |field: &str| -> Result<contracts::DecimalValue, StoreError> {
            ledger
                .try_get::<bigdecimal::BigDecimal, _>(field)?
                .to_plain_string()
                .parse()
                .map_err(|_| StoreError::Integrity)
        };
        let cost = budget
            .cost_currency
            .as_ref()
            .map(|currency| {
                Ok::<_, StoreError>(CostUsage {
                    currency: currency.clone(),
                    reserved: decimal("reserved_cost")?,
                    used: decimal("used_cost")?,
                })
            })
            .transpose()?;
        let usage = BudgetUsage {
            reserved_experiments: u32::try_from(c.try_get::<i64, _>("reserved_experiments")?)
                .map_err(|_| StoreError::Integrity)?,
            used_experiments: u32::try_from(c.try_get::<i64, _>("used_experiments")?)
                .map_err(|_| StoreError::Integrity)?,
            reserved_cpu_seconds: counter(c.try_get("reserved_cpu_seconds")?)?,
            active_runs: u16::try_from(active)
                .map_err(|_| DomainError::BudgetExhausted("parallel_runs"))?,
            reserved_tokens: counter(ledger.try_get("reserved_tokens")?)?,
            used_tokens: counter(ledger.try_get("used_tokens")?)?,
            cost,
            mission: None,
        };
        let l = &request.limits;
        let reserved = admission::reserve(
            db::enum_value(&p, "state")?,
            &budget,
            &stop,
            &usage,
            &Reservation {
                experiments: l.experiments,
                cpu_seconds: l.cpu_seconds,
                wall_seconds: l.wall_seconds,
                memory_mib: l.memory_mib,
                output_bytes: l.output_bytes,
                model: None,
            },
        )?;
        let time = now(&mut tx).await?;
        let deadline = time
            .checked_add_signed(Duration::seconds(i64::from(l.wall_seconds)))
            .ok_or(StoreError::Invalid("deadline"))?;
        let id = Id::new();
        sqlx::query("UPDATE app.research_cycles SET reserved_experiments=$2,reserved_cpu_seconds=$3 WHERE id=$1").bind(request.cycle_id.as_uuid()).bind(i64::from(reserved.reserved_experiments)).bind(reserved.reserved_cpu_seconds.get() as i64).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at) VALUES($1,$2,$3,$4,$5,'QUEUED',$6,$7)")
            .bind(id.as_uuid()).bind(project).bind(request.cycle_id.as_uuid()).bind(db::code(&request.kind)?).bind(request.input_set_id.as_uuid()).bind(deadline).bind(time).execute(&mut *tx).await?;
        let run = append(&mut tx, id, RunEventKind::Created, RunReason::Admitted).await?;
        let msg: i64 = sqlx::query_scalar("SELECT pgmq.send('runs',$1)")
            .bind(json!({"schema_version":1,"run_id":id}))
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(id.as_uuid()).bind(project).bind(request.cycle_id.as_uuid()).bind(key).bind(normalized).bind(db::json(&run)?).bind(db::json(l)?).bind(request.runtime_id.as_uuid()).bind(request.runtime_revision.get() as i64).bind(db::json(&runtime)?).bind(msg).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CommandResult {
            schema_version: SchemaV1,
            replayed: false,
            resource: run,
        })
    }

    /// Trusted administration only. No public generic execution DTO, model
    /// calls, research trials or implicit creation of a research Cycle.
    pub async fn enqueue_standalone_run(
        &self,
        key: &str,
        request: &StandaloneRunSubmission,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError> {
        commands::key(key)?;
        let l = &request.limits;
        if !standalone_kind(request.kind)
            || l.experiments != 0
            || l.cpu_seconds.get() == 0
            || l.wall_seconds == 0
            || l.memory_mib == 0
            || l.output_bytes.get() == 0
            || request.max_parallel_runs == 0
        {
            return Err(StoreError::Invalid("standalone_run_limits_or_kind"));
        }
        let normalized = json!({"schema_version":1,"project_id":request.project_id,"input_set_id":request.input_set_id,
            "runtime_id":request.runtime_id,"runtime_revision":request.runtime_revision,"kind":request.kind,
            "limits":l,"max_parallel_runs":request.max_parallel_runs});
        let mut tx = self.pool.begin().await?;
        let p = sqlx::query("SELECT state FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(request.project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        if let Some(prior) = sqlx::query("SELECT normalized_request,initial_snapshot FROM app.run_admissions WHERE project_id=$1 AND cycle_id IS NULL AND command_key=$2")
            .bind(request.project_id.as_uuid()).bind(key).fetch_optional(&mut *tx).await?
        {
            if prior.try_get::<Value, _>("normalized_request")? != normalized {
                return Err(StoreError::IdempotencyConflict);
            }
            let resource = serde_json::from_value(prior.try_get("initial_snapshot")?).map_err(|_| StoreError::Integrity)?;
            tx.commit().await?;
            return Ok(CommandResult { schema_version: SchemaV1, replayed: true, resource });
        }
        if db::enum_value::<ProjectState>(&p, "state")? == ProjectState::Archived
            && request.kind != RunKind::Export
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            request.input_set_id,
            request.project_id,
            request.runtime_id,
        )
        .await?;
        let r = sqlx::query("SELECT * FROM app.runtime_integrations WHERE id=$1 FOR SHARE")
            .bind(request.runtime_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let caps: Vec<String> = r.try_get("allowed_capabilities")?;
        if !r.try_get::<bool, _>("enabled")?
            || r.try_get::<i64, _>("revision")? != request.runtime_revision.get() as i64
            || !caps.contains(&db::code(&request.kind)?)
        {
            return Err(DomainError::CapabilityUnavailable("runtime_job_kind_or_revision").into());
        }
        let active: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE project_id=$1 AND cycle_id IS NULL AND state NOT IN ('SUCCEEDED','FAILED','CANCELLED')")
            .bind(request.project_id.as_uuid()).fetch_one(&mut *tx).await?;
        if active >= i64::from(request.max_parallel_runs) {
            return Err(DomainError::BudgetExhausted("standalone_parallel_runs").into());
        }
        let runtime = RuntimeSnapshot {
            schema_version: SchemaV1,
            endpoint: r.try_get("endpoint")?,
            credential_ref: r.try_get("credential_ref")?,
            tls_policy: r.try_get("tls_policy")?,
            protocol_version: r.try_get("protocol_version")?,
            allowed_capabilities: caps,
        };
        let time = now(&mut tx).await?;
        let deadline = time
            .checked_add_signed(Duration::seconds(i64::from(l.wall_seconds)))
            .ok_or(StoreError::Invalid("deadline"))?;
        let id = Id::new();
        sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at) VALUES($1,$2,NULL,$3,$4,'QUEUED',$5,$6)")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(db::code(&request.kind)?).bind(request.input_set_id.as_uuid())
            .bind(deadline).bind(time).execute(&mut *tx).await?;
        let run = append(&mut tx, id, RunEventKind::Created, RunReason::Admitted).await?;
        let msg: i64 = sqlx::query_scalar("SELECT pgmq.send('runs',$1)")
            .bind(json!({"schema_version":1,"run_id":id}))
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO app.run_admissions(run_id,project_id,cycle_id,command_key,normalized_request,initial_snapshot,limits,runtime_id,runtime_revision,runtime_snapshot,initial_queue_message_id) VALUES($1,$2,NULL,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(key).bind(normalized).bind(db::json(&run)?)
            .bind(db::json(l)?).bind(request.runtime_id.as_uuid()).bind(request.runtime_revision.get() as i64)
            .bind(db::json(&runtime)?).bind(msg).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CommandResult {
            schema_version: SchemaV1,
            replayed: false,
            resource: run,
        })
    }

    pub async fn read_run_messages(
        &self,
        visibility_seconds: i32,
        limit: i32,
    ) -> Result<Vec<RunMessage>, StoreError> {
        if !(1..=300).contains(&visibility_seconds) || !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("queue_read_limit"));
        }
        let rows = sqlx::query("SELECT msg_id,read_ct,message FROM pgmq.read('runs',$1,$2)")
            .bind(visibility_seconds)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                let payload: QueuePayload = serde_json::from_value(row.try_get("message")?)
                    .map_err(|_| StoreError::Integrity)?;
                let _version = payload.schema_version;
                Ok(RunMessage {
                    message_id: row.try_get("msg_id")?,
                    run_id: payload.run_id,
                    read_count: row.try_get("read_ct")?,
                })
            })
            .collect()
    }

    /// Visibility timeout never authorizes a second active owner. Expired leases
    /// adopt the exact attempt and native identity, not a fresh scientific run.
    pub async fn claim_run(
        &self,
        message: &RunMessage,
        owner: &str,
        lease_seconds: u16,
    ) -> Result<ClaimResult, StoreError> {
        domain::control::text(owner, 1, 120, false)?;
        if !(1..=300).contains(&lease_seconds) {
            return Err(StoreError::Invalid("lease_seconds"));
        }
        let mut tx = self.pool.begin().await?;
        let mut locked = lock_run(&mut tx, message.run_id).await?;
        queue_matches(&mut tx, message).await?;
        if locked.run.state.is_terminal() {
            let result = locked.run;
            tx.commit().await?;
            return Ok(ClaimResult::Terminal(result));
        }
        let time = now(&mut tx).await?;
        let expiry = time + Duration::seconds(i64::from(lease_seconds));
        if locked.run.active_attempt_id.is_none() {
            if locked.run.deadline_at <= time {
                let run=finish(&mut tx,&mut locked,RunState::Failed,RunReason::DeadlineExceeded,json!({"schema_version":1,"source":"NOT_DISPATCHED","reason":"DEADLINE_EXCEEDED"})).await?;
                tx.commit().await?;
                return Ok(ClaimResult::Terminal(run));
            }
            if !locked.admission_open() {
                return Err(DomainError::AdmissionClosed.into());
            }
            if locked.run.state != RunState::Queued {
                return Err(StoreError::Integrity);
            }
            let aid = Id::new();
            let external = format!("{}/1", locked.run.id);
            sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,runtime_id,external_job_id,dispatch_state,runtime_state) VALUES($1,$2,1,$3,1,$4,$5,$6,'NOT_SENT','UNKNOWN')")
                .bind(aid.as_uuid()).bind(locked.run.id.as_uuid()).bind(owner).bind(expiry).bind(locked.admission.try_get::<uuid::Uuid,_>("runtime_id")?).bind(external).execute(&mut *tx).await?;
            sqlx::query("UPDATE app.runs SET state='DISPATCHING',current_attempt_no=1,active_attempt_id=$2,started_at=clock_timestamp() WHERE id=$1").bind(locked.run.id.as_uuid()).bind(aid.as_uuid()).execute(&mut *tx).await?;
            locked.run = append(
                &mut tx,
                locked.run.id,
                RunEventKind::StateChanged,
                RunReason::DispatchReserved,
            )
            .await?;
        } else {
            let old = attempt(&mut tx, &locked.run).await?;
            let current = current_lease(&old)?;
            let time = now(&mut tx).await?;
            if current.lease_expires_at > time {
                if current.worker_owner_id != owner {
                    tx.commit().await?;
                    return Ok(ClaimResult::Busy);
                }
                let lease = lease_view(locked, &old)?;
                tx.commit().await?;
                return Ok(ClaimResult::Leased(Box::new(lease)));
            }
            if locked.run.deadline_at <= time
                && old.try_get::<String, _>("dispatch_state")? == "NOT_SENT"
            {
                // This identity was never authorized for a wire write. Close the
                // local attempt without inventing an observed remote failure.
                sqlx::query("UPDATE app.run_attempts SET dispatch_state='TERMINAL' WHERE id=$1")
                    .bind(locked.run.active_attempt_id.map(Id::as_uuid))
                    .execute(&mut *tx)
                    .await?;
                let result = finish(&mut tx, &mut locked, RunState::Failed, RunReason::DeadlineExceeded,
                    json!({"schema_version":1,"source":"NOT_DISPATCHED","reason":"DEADLINE_EXCEEDED"})).await?;
                tx.commit().await?;
                return Ok(ClaimResult::Terminal(result));
            }
            let epoch = current.owner_epoch.next().ok_or(StoreError::Integrity)?;
            sqlx::query("UPDATE app.run_attempts SET worker_owner_id=$2,owner_epoch=$3,lease_expires_at=$4 WHERE id=$1")
                .bind(locked.run.active_attempt_id.map(Id::as_uuid)).bind(owner).bind(epoch.get() as i64).bind(time+Duration::seconds(i64::from(lease_seconds))).execute(&mut *tx).await?;
            let state = runs::reconcile(locked.run.state)?;
            sqlx::query("UPDATE app.runs SET state=$2 WHERE id=$1")
                .bind(locked.run.id.as_uuid())
                .bind(db::code(&state)?)
                .execute(&mut *tx)
                .await?;
            locked.run = append(
                &mut tx,
                locked.run.id,
                RunEventKind::StateChanged,
                RunReason::LeaseTakenOver,
            )
            .await?;
        }
        let a = attempt(&mut tx, &locked.run).await?;
        let lease = lease_view(locked, &a)?;
        tx.commit().await?;
        Ok(ClaimResult::Leased(Box::new(lease)))
    }

    /// True is the single first-send permit. False means query/cancel the stable
    /// remote identity; a lost return value is never grounds for a blind resend.
    pub async fn begin_run_dispatch(
        &self,
        id: Id,
        owner: &WorkerFence,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, id).await?;
        let a = fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        if a.try_get::<String, _>("dispatch_state")? != "NOT_SENT"
            || locked.run.state == RunState::CancelRequested
        {
            tx.commit().await?;
            return Ok(false);
        }
        if !locked.admission_open() || locked.run.deadline_at <= now(&mut tx).await? {
            return Err(DomainError::AdmissionClosed.into());
        }
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            locked.run.input_set_id,
            locked.run.project_id,
            db::id(locked.admission.try_get("runtime_id")?)?,
        )
        .await?;
        let runtime = sqlx::query(
            "SELECT enabled,revision::bigint FROM app.runtime_integrations WHERE id=$1 FOR SHARE",
        )
        .bind(locked.admission.try_get::<uuid::Uuid, _>("runtime_id")?)
        .fetch_one(&mut *tx)
        .await?;
        if !runtime.try_get::<bool, _>("enabled")?
            || runtime.try_get::<i64, _>("revision")?
                != locked.admission.try_get::<i64, _>("runtime_revision")?
        {
            return Err(DomainError::CapabilityUnavailable("runtime_revision_changed").into());
        }
        // Runtime configuration can be locked by an Operator update. A lease
        // valid before that wait is not authority after it; query DB time only
        // after the last potentially conflicting authority lock.
        fence(&mut tx, &locked.run, owner).await?;
        if locked.run.deadline_at <= now(&mut tx).await? {
            return Err(DomainError::AdmissionClosed.into());
        }
        sqlx::query("UPDATE app.run_attempts SET dispatch_state='SENT_UNKNOWN' WHERE id=$1")
            .bind(owner.attempt_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn renew_run_lease(
        &self,
        id: Id,
        owner: &WorkerFence,
        seconds: u16,
    ) -> Result<DateTime<Utc>, StoreError> {
        if !(1..=300).contains(&seconds) {
            return Err(StoreError::Invalid("lease_seconds"));
        }
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, id).await?;
        fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        let expiry = now(&mut tx).await? + Duration::seconds(i64::from(seconds));
        let expiry: DateTime<Utc> = sqlx::query_scalar("UPDATE app.run_attempts SET lease_expires_at=GREATEST(lease_expires_at,$2) WHERE id=$1 RETURNING lease_expires_at::timestamptz").bind(owner.attempt_id.as_uuid()).bind(expiry).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(expiry)
    }

    pub async fn observe_run_running(
        &self,
        id: Id,
        owner: &WorkerFence,
        external_job_id: &str,
    ) -> Result<RunSnapshotV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, id).await?;
        let a = fence(&mut tx, &locked.run, owner).await?;
        if a.try_get::<Option<String>, _>("external_job_id")?
            .as_deref()
            != Some(external_job_id)
            || a.try_get::<String, _>("dispatch_state")? == "NOT_SENT"
        {
            return Err(StoreError::Conflict);
        }
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        let state = runs::confirm_running(locked.run.state)?;
        sqlx::query("UPDATE app.run_attempts SET dispatch_state='ACKNOWLEDGED',runtime_state='RUNNING' WHERE id=$1").bind(owner.attempt_id.as_uuid()).execute(&mut *tx).await?;
        let result = if state == locked.run.state {
            locked.run
        } else {
            sqlx::query("UPDATE app.runs SET state=$2 WHERE id=$1")
                .bind(id.as_uuid())
                .bind(db::code(&state)?)
                .execute(&mut *tx)
                .await?;
            append(
                &mut tx,
                id,
                RunEventKind::StateChanged,
                RunReason::RuntimeRunning,
            )
            .await?
        };
        tx.commit().await?;
        Ok(result)
    }

    /// Metadata validation is defense in depth; the trusted adapter must already
    /// verify actual immutable manifest content/schema/resources before this call.
    /// Result adoption does not confer PASS, qualification or delivery authority.
    pub async fn accept_run_terminal(
        &self,
        id: Id,
        owner: &WorkerFence,
        observation: &TerminalObservation,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError> {
        if !observation
            .observed_at
            .timestamp_subsec_nanos()
            .is_multiple_of(1000)
            || observation.external_job_id.len() > 200
        {
            return Err(StoreError::Invalid("terminal_observation"));
        }
        let failed = observation.outcome == NativeOutcome::Failed;
        if failed != observation.failure_class.is_some()
            || failed != observation.failure_code.is_some()
            || observation.failure_code.as_ref().is_some_and(|code| {
                code.is_empty()
                    || code.len() > 64
                    || !code
                        .bytes()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            })
            || (observation.outcome == NativeOutcome::ConfirmedAbsent
                && observation.manifest_artifact_id.is_some())
        {
            return Err(StoreError::Invalid("failure_class_or_code"));
        }
        let expected = db::json(observation)?;
        let mut tx = self.pool.begin().await?;
        let mut locked = lock_run(&mut tx, id).await?;
        if let Some(receipt)=sqlx::query("SELECT attempt_id::uuid,observation,result_snapshot FROM app.run_terminal_receipts WHERE run_id=$1").bind(id.as_uuid()).fetch_optional(&mut *tx).await?{
            if receipt.try_get::<Option<uuid::Uuid>,_>("attempt_id")?!=Some(owner.attempt_id.as_uuid()) || receipt.try_get::<Value,_>("observation")?!=expected{return Err(StoreError::Conflict);}
            let result=serde_json::from_value(receipt.try_get("result_snapshot")?).map_err(|_|StoreError::Integrity)?;
            tx.commit().await?;return Ok(CommandResult{schema_version:SchemaV1,replayed:true,resource:result});
        }
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        let a = fence(&mut tx, &locked.run, owner).await?;
        if a.try_get::<Option<String>, _>("external_job_id")?
            .as_deref()
            != Some(&observation.external_job_id)
        {
            return Err(StoreError::Conflict);
        }
        let time = now(&mut tx).await?;
        if observation.observed_at > time
            || observation.observed_at < a.try_get::<DateTime<Utc>, _>("created_at")?
        {
            return Err(StoreError::Invalid("observed_at"));
        }
        if a.try_get::<String, _>("dispatch_state")? == "NOT_SENT"
            && observation.outcome != NativeOutcome::ConfirmedAbsent
        {
            return Err(StoreError::Conflict);
        }
        if observation.outcome == NativeOutcome::Succeeded
            && observation.manifest_artifact_id.is_none()
        {
            return Err(StoreError::Invalid("manifest_required"));
        }
        if let Some(manifest) = observation.manifest_artifact_id {
            let limits: JobLimitsV1 = serde_json::from_value(locked.admission.try_get("limits")?)
                .map_err(|_| StoreError::Integrity)?;
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 AND project_id=$2 AND producer_run_id=$3 AND producer_attempt_id=$4 AND kind='REPORT' AND media_type='application/json' AND schema_name='qz.job_result' AND schema_version='1' AND byte_count>0 AND byte_count<=$5)")
                .bind(manifest.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(id.as_uuid()).bind(owner.attempt_id.as_uuid()).bind(limits.output_bytes.get() as i64).fetch_one(&mut *tx).await?;
            if !valid {
                return Err(StoreError::Invalid("manifest_exact_producer"));
            }
        }
        let native = match observation.outcome {
            NativeOutcome::Succeeded => RemoteTerminal::Succeeded,
            NativeOutcome::Failed => RemoteTerminal::Failed,
            NativeOutcome::Cancelled => RemoteTerminal::Cancelled,
            NativeOutcome::ConfirmedAbsent => RemoteTerminal::ConfirmedAbsent,
        };
        let current = current_lease(&a)?;
        let presented = AttemptLease {
            attempt_no: locked.run.current_attempt_no,
            worker_owner_id: owner.worker_owner_id.clone(),
            owner_epoch: owner.owner_epoch,
            lease_expires_at: current.lease_expires_at,
        };
        let state = runs::accept_terminal(
            locked.run.state,
            Some(native),
            &current,
            &presented,
            now(&mut tx).await?,
        )?;
        let reason = match (state, observation.outcome) {
            (RunState::Succeeded, _) => RunReason::RuntimeSucceeded,
            (RunState::Failed, _) => RunReason::RuntimeFailed,
            (RunState::Cancelled, NativeOutcome::Succeeded) => {
                RunReason::ResultDiscardedAfterCancel
            }
            _ => RunReason::RuntimeCancelled,
        };
        let runtime_state = match observation.outcome {
            NativeOutcome::Succeeded => "SUCCEEDED",
            NativeOutcome::Failed => "FAILED",
            _ => "CANCELLED",
        };
        sqlx::query("UPDATE app.run_attempts SET dispatch_state='TERMINAL',runtime_state=$2,result_manifest_artifact_id=$3,accepted_at=CASE WHEN $3::uuid IS NULL THEN NULL ELSE clock_timestamp() END,error_class=$4,error_code=$5 WHERE id=$1")
            .bind(owner.attempt_id.as_uuid()).bind(runtime_state).bind(observation.manifest_artifact_id.map(Id::as_uuid)).bind(observation.failure_class.as_ref().map(db::code).transpose()?).bind(observation.failure_code.as_deref()).execute(&mut *tx).await?;
        let result = finish(&mut tx, &mut locked, state, reason, expected).await?;
        tx.commit().await?;
        Ok(CommandResult {
            schema_version: SchemaV1,
            replayed: false,
            resource: result,
        })
    }

    pub async fn acknowledge_run(&self, message: &RunMessage) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, message.run_id).await?;
        let receipt: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.run_terminal_receipts WHERE run_id=$1)",
        )
        .bind(message.run_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if !locked.run.state.is_terminal() || !receipt {
            return Err(StoreError::Conflict);
        }
        match queue_matches(&mut tx, message).await {
            Ok(()) => {
                let archived: bool = sqlx::query_scalar("SELECT pgmq.archive('runs',$1)")
                    .bind(message.message_id)
                    .fetch_one(&mut *tx)
                    .await?;
                if !archived {
                    return Err(StoreError::Integrity);
                }
            }
            Err(StoreError::NotFound) => {
                let value: Value =
                    sqlx::query_scalar("SELECT message FROM pgmq.a_runs WHERE msg_id=$1")
                        .bind(message.message_id)
                        .fetch_optional(&mut *tx)
                        .await?
                        .ok_or(StoreError::NotFound)?;
                let payload: QueuePayload =
                    serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
                if payload.run_id != message.run_id {
                    return Err(StoreError::Conflict);
                }
            }
            Err(error) => return Err(error),
        }
        tx.commit().await?;
        Ok(())
    }
}

/// Authority must be rechecked for every durable batch, not just on opening a
/// long-lived stream. Return only server-derived project/Mission restrictions.
async fn read_scope(
    tx: &mut Tx<'_>,
    actor: &Actor,
) -> Result<(Option<Id>, Option<Id>), StoreError> {
    match actor {
        Actor::Browser { .. } => {
            authority::browser(tx, actor, false, false).await?;
            Ok((None, None))
        }
        Actor::Machine { .. } => {
            let machine = authority::machine(tx, actor, false).await?;
            machine.requires(MachineScope::RunRead)?;
            let project = machine.project_id.ok_or(StoreError::Forbidden)?;
            if machine.kind == PrincipalKind::Downstream {
                return Err(StoreError::Forbidden);
            }
            Ok((Some(project), machine.run_id))
        }
    }
}
fn within_scope(run: &RunSnapshotV1, scope: (Option<Id>, Option<Id>)) -> Result<(), StoreError> {
    if scope.0.is_some_and(|p| p != run.project_id) || scope.1.is_some_and(|r| r != run.id) {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}
impl Store {
    pub async fn get_run(&self, actor: &Actor, id: Id) -> Result<RunSnapshotV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let scope = read_scope(&mut tx, actor).await?;
        let run = snapshot(&run_row(&mut tx, id, false).await?)?;
        within_scope(&run, scope)?;
        tx.commit().await?;
        Ok(run)
    }
    pub async fn list_runs(
        &self,
        actor: &Actor,
        query: &RunListQuery,
    ) -> Result<Page<RunSnapshotV1>, StoreError> {
        if !(1..=100).contains(&query.limit) {
            return Err(StoreError::Invalid("limit"));
        }
        let mut tx = self.pool.begin().await?;
        let scope = read_scope(&mut tx, actor).await?;
        if query.project_id.is_some() && scope.0.is_some() && query.project_id != scope.0 {
            return Err(StoreError::NotFound);
        }
        let project = query.project_id.or(scope.0);
        let sql=format!("SELECT {FIELDS} FROM app.runs r WHERE ($1::uuid IS NULL OR r.project_id=$1) AND ($2::uuid IS NULL OR r.id=$2) AND ($3::text IS NULL OR r.state=$3) AND ($4::uuid IS NULL OR r.id>$4) ORDER BY r.id LIMIT $5");
        let rows = sqlx::query(&sql)
            .bind(project.map(Id::as_uuid))
            .bind(scope.1.map(Id::as_uuid))
            .bind(query.state.as_ref().map(db::code).transpose()?)
            .bind(query.cursor.map(Id::as_uuid))
            .bind(i64::from(query.limit) + 1)
            .fetch_all(&mut *tx)
            .await?;
        let mut items: Vec<_> = rows.iter().map(snapshot).collect::<Result<_, _>>()?;
        let more = items.len() > usize::from(query.limit);
        items.truncate(usize::from(query.limit));
        let next_cursor = if more {
            items.last().map(|r| r.id)
        } else {
            None
        };
        tx.commit().await?;
        Ok(Page {
            schema_version: SchemaV1,
            items,
            next_cursor,
        })
    }
    pub async fn run_events(
        &self,
        actor: &Actor,
        id: Id,
        after: DbCounter,
        limit: u16,
    ) -> Result<RunEventBatchV1, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("event_limit"));
        }
        let mut tx = self.pool.begin().await?;
        let scope = read_scope(&mut tx, actor).await?;
        let run = snapshot(&run_row(&mut tx, id, false).await?)?;
        within_scope(&run, scope)?;
        if after > run.last_event_seq {
            return Err(StoreError::EventCursorExpired);
        }
        let rows=sqlx::query("SELECT seq::bigint,attempt_id::uuid,event_type,schema_version,payload,occurred_at::timestamptz FROM app.run_events WHERE run_id=$1 AND seq>$2 ORDER BY seq LIMIT $3")
            .bind(id.as_uuid()).bind(after.get() as i64).bind(i64::from(limit)).fetch_all(&mut *tx).await?;
        let mut events = Vec::with_capacity(rows.len());
        let mut cursor = after;
        for row in rows {
            let seq = counter(row.try_get("seq")?)?;
            if cursor.checked_add(1) != Some(seq) {
                return Err(StoreError::EventCursorExpired);
            }
            if row.try_get::<i32, _>("schema_version")? != 1 {
                return Err(StoreError::EventContractUnsupported);
            }
            let event = RunEventV1 {
                schema_version: SchemaV1,
                run_id: id,
                seq,
                attempt_id: db::optional_id(&row, "attempt_id")?,
                event_type: row.try_get("event_type")?,
                occurred_at: row.try_get("occurred_at")?,
                payload: row.try_get("payload")?,
            };
            event
                .validate()
                .map_err(|_| StoreError::EventContractUnsupported)?;
            events.push(event);
            cursor = seq;
        }
        if events.is_empty() && after < run.last_event_seq {
            return Err(StoreError::EventCursorExpired);
        }
        tx.commit().await?;
        Ok(RunEventBatchV1 {
            schema_version: SchemaV1,
            run_id: id,
            events,
            last_event_seq: run.last_event_seq,
            state: run.state,
        })
    }
    pub async fn cancel_run(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &RunCancelV1,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError> {
        commands::key(key)?;
        let mut tx = self.pool.begin().await?;
        // Browser authority locks precede project locks everywhere. Machine
        // checks follow the already locked project, avoiding SHARE->UPDATE
        // upgrade deadlocks between simultaneous machine cancellations.
        if matches!(actor, Actor::Browser { .. }) {
            authority::browser(&mut tx, actor, true, false).await?;
        }
        let mut locked = lock_run(&mut tx, id).await?;
        let scope = match actor {
            Actor::Browser { .. } => String::from("OPERATOR"),
            Actor::Machine { .. } => {
                let machine = authority::machine(&mut tx, actor, false).await?;
                if !matches!(machine.kind, PrincipalKind::Cli | PrincipalKind::Automation) {
                    return Err(StoreError::Forbidden);
                }
                machine.requires(MachineScope::RunCancel)?;
                machine.project(locked.run.project_id)?;
                format!("CREDENTIAL:{}", machine.credential_id)
            }
        };
        let prepared = commands::run_cancel(
            &mut tx,
            scope,
            key,
            id,
            json!({"schema_version":1,"run_id":id,"command":request}),
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        if locked.run.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                current: locked.run.revision,
            });
        }
        let state = runs::request_cancel(locked.run.state)?;
        let result = if state == RunState::Cancelled {
            // QUEUED + no Attempt means no externally authorized side effect.
            if locked.run.active_attempt_id.is_some() {
                return Err(StoreError::Integrity);
            }
            sqlx::query(
                "UPDATE app.runs SET cancellation_requested_at=clock_timestamp() WHERE id=$1",
            )
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await?;
            finish(&mut tx,&mut locked,state,RunReason::CancelledBeforeDispatch,json!({"schema_version":1,"source":"NOT_DISPATCHED","reason":"CANCELLED_BEFORE_DISPATCH"})).await?
        } else if state == locked.run.state {
            locked.run
        } else {
            sqlx::query("UPDATE app.runs SET state='CANCEL_REQUESTED',cancellation_requested_at=clock_timestamp() WHERE id=$1").bind(id.as_uuid()).execute(&mut *tx).await?;
            append(
                &mut tx,
                id,
                RunEventKind::StateChanged,
                RunReason::CancelRequested,
            )
            .await?
        };
        let response = commands::finish(&mut tx, prepared, result, 202).await?;
        tx.commit().await?;
        Ok(response)
    }
}
