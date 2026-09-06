//! Durable per-native-turn spending: reserve -> dispatch intent -> native binding
//! -> immutable usage receipt. Unknown sends retain their reservation. This is
//! not a replacement for Codex's tool loop, and a JSON-RPC ID is not idempotency.
use crate::{Store, StoreError};
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use contracts::{
    budget::{BudgetV1, StopRuleV1},
    runs::ProjectState,
    DbCounter, DecimalValue, Id, Revision,
};
use domain::{
    admission::{
        reserve_model_turn, BudgetUsage, CostEstimate, CostUsage, MissionUsage, ModelReservation,
        TurnKind,
    },
    DomainError,
};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use uuid::Uuid;

type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Clone, Debug)]
pub struct WorkerFence {
    pub attempt_id: Id,
    pub worker_owner_id: String,
    pub owner_epoch: Revision,
}

#[derive(Clone, Debug)]
pub struct TurnRequest {
    pub command_key: String,
    pub turn_kind: TurnKind,
    pub tokens: DbCounter,
    pub estimated_cost: Option<CostEstimate>,
    pub request_artifact_id: Id,
    pub deadline_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reservation {
    pub id: Id,
    pub run_id: Id,
    pub session_id: Id,
    pub attempt_id: Id,
    pub request_artifact_id: Id,
    pub owner_epoch: Revision,
    pub profile_revision: Revision,
    pub ordinal: u16,
    pub turn_kind: TurnKind,
    pub tokens: DbCounter,
    pub reserved_cost: Option<DecimalValue>,
    pub cost_currency: Option<String>,
    pub deadline_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchDecision {
    /// This call inserted the unique intent. Only this owner may send once.
    Send {
        rpc_request_id: String,
    },
    /// An earlier owner may have sent it; query native state, never send again.
    Reconcile {
        native_turn_id: Option<String>,
    },
    Settled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Succeeded,
    Failed,
    Cancelled,
    NotSent,
}

impl TurnOutcome {
    fn name(self) -> &'static str {
        match self {
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::NotSent => "NOT_SENT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnTerminal {
    pub outcome: TurnOutcome,
    pub native_turn_id: Option<String>,
    pub reason_code: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageReceipt {
    pub outcome: TurnOutcome,
    pub actual_tokens: DbCounter,
    pub actual_cost: Option<DecimalValue>,
    pub currency: Option<String>,
    pub reason_code: String,
}

struct Mission {
    project_id: Uuid,
    cycle_id: Uuid,
    run_id: Id,
    session_id: Uuid,
    profile_revision: i64,
    budget: BudgetV1,
    budget_matches_brief: bool,
    stop: StopRuleV1,
    project_state: ProjectState,
    cycle_state: String,
    brief_state: String,
    run_state: String,
    run_deadline: DateTime<Utc>,
    now: DateTime<Utc>,
}

fn id(value: Uuid) -> Result<Id, StoreError> {
    Id::try_from(value.to_string()).map_err(|_| StoreError::Invalid("database_identity"))
}
fn count(value: i64) -> Result<DbCounter, StoreError> {
    DbCounter::new(u64::try_from(value).map_err(|_| StoreError::Invalid("negative_usage"))?)
        .map_err(|_| StoreError::Invalid("usage_range"))
}
fn decimal(value: BigDecimal) -> Result<DecimalValue, StoreError> {
    value
        .to_plain_string()
        .parse()
        .map_err(|_| StoreError::Invalid("decimal_usage_range"))
}
fn kind(value: &str) -> Result<TurnKind, StoreError> {
    match value {
        "RESEARCH" => Ok(TurnKind::Research),
        "REPAIR" => Ok(TurnKind::Repair),
        _ => Err(StoreError::Invalid("turn_kind")),
    }
}
fn kind_name(value: TurnKind) -> &'static str {
    match value {
        TurnKind::Research => "RESEARCH",
        TurnKind::Repair => "REPAIR",
    }
}
fn bounded(text: &str, limit: usize) -> bool {
    !text.trim().is_empty() && text.len() <= limit
}

// Domain identities are immutable, so the first lookup can determine lock order
// without granting authority. Every authoritative field is re-read after locks.
async fn lock_mission(
    tx: &mut Tx<'_>,
    run_id: Id,
    fence: &WorkerFence,
) -> Result<Mission, StoreError> {
    let refs = sqlx::query("SELECT project_id::uuid,cycle_id::uuid FROM app.runs WHERE id=$1")
        .bind(run_id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let project_id: Uuid = refs.try_get("project_id")?;
    let cycle_id: Uuid = refs
        .try_get::<Option<Uuid>, _>("cycle_id")?
        .ok_or(StoreError::Invalid("mission_cycle"))?;
    let state: String = sqlx::query_scalar("SELECT state FROM app.projects WHERE id=$1 FOR UPDATE")
        .bind(project_id)
        .fetch_one(&mut **tx)
        .await?;
    let project_state = match state.as_str() {
        "DRAFT" => ProjectState::Draft,
        "ACTIVE" => ProjectState::Active,
        "PAUSED" => ProjectState::Paused,
        "ARCHIVED" => ProjectState::Archived,
        _ => return Err(StoreError::Invalid("project_state")),
    };
    let cycle=sqlx::query("SELECT state,budget_snapshot,brief_id::uuid FROM app.research_cycles WHERE id=$1 AND project_id=$2 FOR UPDATE")
        .bind(cycle_id).bind(project_id).fetch_one(&mut **tx).await?;
    // Lock the referenced Brief too: a mutable draft is never an admission
    // contract. This lock is held through the reservation and queue commit.
    let brief = sqlx::query(
        "SELECT state,budget,stop_rule FROM app.research_briefs WHERE id=$1 AND project_id=$2 FOR SHARE",
    )
    .bind(cycle.try_get::<Uuid, _>("brief_id")?)
    .bind(project_id)
    .fetch_one(&mut **tx)
    .await?;
    let snapshot: serde_json::Value = cycle.try_get("budget_snapshot")?;
    let budget_matches_brief = snapshot == brief.try_get::<serde_json::Value, _>("budget")?;
    let budget: BudgetV1 =
        serde_json::from_value(snapshot).map_err(|_| StoreError::Invalid("budget_snapshot"))?;
    let stop: StopRuleV1 = serde_json::from_value(brief.try_get("stop_rule")?)
        .map_err(|_| StoreError::Invalid("stop_rule"))?;
    let run=sqlx::query("SELECT kind,state,active_attempt_id::uuid,deadline_at::timestamptz FROM app.runs WHERE id=$1 AND project_id=$2 AND cycle_id=$3 FOR UPDATE")
        .bind(run_id.as_uuid()).bind(project_id).bind(cycle_id).fetch_one(&mut **tx).await?;
    if run.try_get::<String, _>("kind")? != "AGENT_RESEARCH" {
        return Err(StoreError::Invalid("not_agent_mission"));
    }
    if run.try_get::<Option<Uuid>, _>("active_attempt_id")? != Some(fence.attempt_id.as_uuid()) {
        return Err(DomainError::StaleAttempt.into());
    }
    let attempt=sqlx::query("SELECT worker_owner_id,owner_epoch::bigint,lease_expires_at::timestamptz FROM app.run_attempts WHERE id=$1 AND run_id=$2 FOR UPDATE")
        .bind(fence.attempt_id.as_uuid()).bind(run_id.as_uuid()).fetch_one(&mut **tx).await?;
    let session =
        sqlx::query("SELECT id::uuid, profile_revision::bigint FROM app.codex_sessions WHERE run_id=$1 FOR UPDATE")
            .bind(run_id.as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    // Read clock_timestamp AFTER any lock wait, never use transaction-start time.
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if attempt.try_get::<String, _>("worker_owner_id")? != fence.worker_owner_id
        || attempt.try_get::<i64, _>("owner_epoch")? != fence.owner_epoch.get() as i64
        || attempt.try_get::<DateTime<Utc>, _>("lease_expires_at")? <= now
    {
        return Err(DomainError::StaleAttempt.into());
    }
    Ok(Mission {
        project_id,
        cycle_id,
        run_id,
        session_id: session.try_get("id")?,
        profile_revision: session.try_get("profile_revision")?,
        budget,
        budget_matches_brief,
        stop,
        project_state,
        cycle_state: cycle.try_get("state")?,
        brief_state: brief.try_get("state")?,
        run_state: run.try_get("state")?,
        run_deadline: run.try_get("deadline_at")?,
        now,
    })
}

impl Mission {
    fn admit(&self, deadline: DateTime<Utc>) -> Result<(), StoreError> {
        if self.project_state != ProjectState::Active
            || self.cycle_state != "RUNNING"
            || self.brief_state != "FROZEN"
            || self.run_state != "RUNNING"
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        // Only an exact copy of the locked frozen Brief may authorize new
        // spending. Keep reconciliation separate: existing real usage must not
        // disappear merely because an earlier Cycle snapshot was invalid.
        if !self.budget_matches_brief {
            return Err(StoreError::Invalid("frozen_budget_snapshot_mismatch"));
        }
        if deadline <= self.now || deadline > self.run_deadline {
            return Err(StoreError::Invalid("turn_deadline"));
        }
        Ok(())
    }
    async fn usage(&self, tx: &mut Tx<'_>) -> Result<BudgetUsage, StoreError> {
        let inconsistent: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.model_turn_reservations WHERE cycle_id=$1 AND (cost_currency IS DISTINCT FROM $2::text OR (reserved_cost IS NULL) <> ($2::text IS NULL)))")
            .bind(self.cycle_id).bind(self.budget.cost_currency.as_deref()).fetch_one(&mut **tx).await?;
        if inconsistent {
            return Err(StoreError::Invalid("ledger_cost_currency"));
        }
        let row=sqlx::query("SELECT coalesce(sum(reserved_tokens),0)::bigint reserved_tokens,coalesce(sum(used_tokens),0)::bigint used_tokens,coalesce(sum(reserved_cost),0)::numeric reserved_cost,coalesce(sum(used_cost),0)::numeric used_cost FROM app.model_turn_accounting WHERE cycle_id=$1")
            .bind(self.cycle_id).fetch_one(&mut **tx).await?;
        let turns=sqlx::query("SELECT coalesce(sum(used_turns),0)::bigint used_turns,coalesce(sum(reserved_turns),0)::bigint reserved_turns,coalesce(sum(used_repair_turns),0)::bigint used_repair_turns,coalesce(sum(reserved_repair_turns),0)::bigint reserved_repair_turns FROM app.model_turn_accounting WHERE session_id=$1")
            .bind(self.session_id).fetch_one(&mut **tx).await?;
        let turn_count = |field: &str| -> Result<u16, StoreError> {
            u16::try_from(turns.try_get::<i64, _>(field)?)
                .map_err(|_| StoreError::Invalid("turn_count_overflow"))
        };
        let cost = match &self.budget.cost_currency {
            Some(currency) => Some(CostUsage {
                currency: currency.clone(),
                reserved: decimal(row.try_get("reserved_cost")?)?,
                used: decimal(row.try_get("used_cost")?)?,
            }),
            None => None,
        };
        // This projection is used ONLY by reserve_model_turn, which deliberately
        // does not admit jobs or consult job CPU/experiment/concurrency fields.
        Ok(BudgetUsage {
            reserved_experiments: 0,
            used_experiments: 0,
            reserved_cpu_seconds: DbCounter::ZERO,
            active_runs: 0,
            reserved_tokens: count(row.try_get("reserved_tokens")?)?,
            used_tokens: count(row.try_get("used_tokens")?)?,
            cost,
            mission: Some(MissionUsage {
                mission_id: self.run_id,
                used_turns: turn_count("used_turns")?,
                reserved_turns: turn_count("reserved_turns")?,
                used_repair_turns: turn_count("used_repair_turns")?,
                reserved_repair_turns: turn_count("reserved_repair_turns")?,
            }),
        })
    }
}

const RESERVATION_COLUMNS: &str = "id::uuid,run_id::uuid,session_id::uuid,attempt_id::uuid,request_artifact_id::uuid,owner_epoch::bigint,profile_revision::bigint,ordinal,turn_kind,reserved_tokens::bigint,reserved_cost::numeric,cost_currency::text,deadline_at::timestamptz";
fn reservation(row: PgRow) -> Result<Reservation, StoreError> {
    Ok(Reservation {
        id: id(row.try_get("id")?)?,
        run_id: id(row.try_get("run_id")?)?,
        session_id: id(row.try_get("session_id")?)?,
        attempt_id: id(row.try_get("attempt_id")?)?,
        request_artifact_id: id(row.try_get("request_artifact_id")?)?,
        owner_epoch: Revision::try_from(row.try_get::<i64, _>("owner_epoch")?.to_string())
            .map_err(|_| StoreError::Invalid("owner_epoch"))?,
        profile_revision: Revision::try_from(
            row.try_get::<i64, _>("profile_revision")?.to_string(),
        )
        .map_err(|_| StoreError::Invalid("profile_revision"))?,
        ordinal: u16::try_from(row.try_get::<i32, _>("ordinal")?)
            .map_err(|_| StoreError::Invalid("turn_ordinal"))?,
        turn_kind: kind(&row.try_get::<String, _>("turn_kind")?)?,
        tokens: count(row.try_get("reserved_tokens")?)?,
        reserved_cost: row
            .try_get::<Option<BigDecimal>, _>("reserved_cost")?
            .map(decimal)
            .transpose()?,
        cost_currency: row.try_get("cost_currency")?,
        deadline_at: row.try_get("deadline_at")?,
    })
}
async fn load_reservation(tx: &mut Tx<'_>, reservation_id: Id) -> Result<Reservation, StoreError> {
    let sql = format!("SELECT {RESERVATION_COLUMNS} FROM app.model_turn_reservations WHERE id=$1");
    reservation(
        sqlx::query(&sql)
            .bind(reservation_id.as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotFound)?,
    )
}

impl Store {
    pub async fn reserve_turn(
        &self,
        run_id: Id,
        fence: &WorkerFence,
        request: &TurnRequest,
    ) -> Result<Reservation, StoreError> {
        if !bounded(&request.command_key, 200) {
            return Err(StoreError::Invalid("command_key"));
        }
        if !request
            .deadline_at
            .timestamp_subsec_nanos()
            .is_multiple_of(1000)
        {
            return Err(StoreError::Invalid("timestamp_precision"));
        }
        let mut tx = self.pool.begin().await?;
        let mission = lock_mission(&mut tx, run_id, fence).await?;
        let sql=format!("SELECT {RESERVATION_COLUMNS} FROM app.model_turn_reservations WHERE session_id=$1 AND command_key=$2");
        if let Some(row) = sqlx::query(&sql)
            .bind(mission.session_id)
            .bind(&request.command_key)
            .fetch_optional(&mut *tx)
            .await?
        {
            let original = reservation(row)?;
            if original.attempt_id != fence.attempt_id
                || original.turn_kind != request.turn_kind
                || original.tokens != request.tokens
                || original.request_artifact_id != request.request_artifact_id
                || original.deadline_at != request.deadline_at
                || original.reserved_cost.as_ref()
                    != request.estimated_cost.as_ref().map(|c| &c.amount)
                || original.cost_currency.as_ref()
                    != request.estimated_cost.as_ref().map(|c| &c.currency)
            {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(original);
        }
        mission.admit(request.deadline_at)?;
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.model_turn_reservations r WHERE r.session_id=$1 AND NOT EXISTS(SELECT 1 FROM app.model_turn_receipts t WHERE t.reservation_id=r.id))")
            .bind(mission.session_id).fetch_one(&mut *tx).await?;
        if pending {
            return Err(StoreError::TurnPending);
        }
        // The request is an immutable, project-scoped, nonsealed artifact. Never
        // permit an Agent to reference another project's or evaluator-only data.
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='PARAMETERS' AND access_class IN ('OPERATOR','RESEARCH'))")
            .bind(request.request_artifact_id.as_uuid()).bind(mission.project_id).fetch_one(&mut *tx).await?;
        if !allowed {
            return Err(StoreError::Invalid("request_artifact"));
        }
        let usage = mission.usage(&mut tx).await?;
        reserve_model_turn(
            mission.project_state,
            &mission.budget,
            &mission.stop,
            &usage,
            &ModelReservation {
                mission_id: run_id,
                turn_kind: request.turn_kind,
                tokens: request.tokens,
                estimated_cost: request.estimated_cost.clone(),
            },
        )?;
        // The session lock makes ordinal allocation deterministic, including
        // refunded reservations. A refund cannot erase audit identities.
        let ordinal: i32 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),0)+1 FROM app.model_turn_reservations WHERE session_id=$1")
            .bind(mission.session_id).fetch_one(&mut *tx).await?;
        if ordinal > i32::from(u16::MAX) {
            return Err(StoreError::Invalid("turn_ordinal_exhausted"));
        }
        let new_id = Id::new();
        sqlx::query("INSERT INTO app.model_turn_reservations(id,project_id,cycle_id,run_id,session_id,attempt_id,command_key,turn_kind,reserved_tokens,reserved_cost,cost_currency,request_artifact_id,deadline_at,owner_epoch,profile_revision,ordinal) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(new_id.as_uuid()).bind(mission.project_id).bind(mission.cycle_id).bind(run_id.as_uuid()).bind(mission.session_id)
            .bind(fence.attempt_id.as_uuid()).bind(&request.command_key).bind(kind_name(request.turn_kind)).bind(request.tokens.get() as i64)
            .bind(request.estimated_cost.as_ref().map(|c|c.amount.as_decimal())).bind(request.estimated_cost.as_ref().map(|c|c.currency.as_str()))
            .bind(request.request_artifact_id.as_uuid()).bind(request.deadline_at)
            .bind(fence.owner_epoch.get() as i64).bind(mission.profile_revision).bind(ordinal).execute(&mut *tx).await?;
        let message = serde_json::json!({"reservation_id":new_id});
        sqlx::query("SELECT pgmq.send('model_turns', $1::jsonb)")
            .bind(message)
            .execute(&mut *tx)
            .await?;
        let result = load_reservation(&mut tx, new_id).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// A notification is disposable only after its exact immutable usage receipt
    /// committed. Queue visibility is not proof of execution or ownership.
    pub async fn acknowledge_settled_turn_message(
        &self,
        message_id: i64,
        reservation_id: Id,
    ) -> Result<bool, StoreError> {
        if message_id <= 0 {
            return Err(StoreError::Invalid("queue_message_id"));
        }
        let mut tx = self.pool.begin().await?;
        load_reservation(&mut tx, reservation_id).await?;
        // Immutable receipts permit safe duplicate notification cleanup even
        // after the final owner's lease expires. This never adopts a result.
        if receipt(&mut tx, reservation_id).await?.is_none() {
            return Err(StoreError::TurnPending);
        }
        let message: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT message FROM pgmq.q_model_turns WHERE msg_id=$1 FOR UPDATE")
                .bind(message_id)
                .fetch_optional(&mut *tx)
                .await?;
        let expected = serde_json::json!({"reservation_id":reservation_id});
        match message {
            Some(message) if message == expected => {
                let archived: bool = sqlx::query_scalar("SELECT pgmq.archive('model_turns',$1)")
                    .bind(message_id)
                    .fetch_one(&mut *tx)
                    .await?;
                tx.commit().await?;
                Ok(archived)
            }
            Some(_) => Err(StoreError::Conflict),
            None => {
                let previous: Option<serde_json::Value> =
                    sqlx::query_scalar("SELECT message FROM pgmq.a_model_turns WHERE msg_id=$1")
                        .bind(message_id)
                        .fetch_optional(&mut *tx)
                        .await?;
                if previous.as_ref() != Some(&expected) {
                    return Err(StoreError::Conflict);
                }
                tx.commit().await?;
                Ok(false)
            }
        }
    }

    pub async fn claim_turn_dispatch(
        &self,
        reservation_id: Id,
        fence: &WorkerFence,
    ) -> Result<DispatchDecision, StoreError> {
        let mut tx = self.pool.begin().await?;
        let item = load_reservation(&mut tx, reservation_id).await?;
        let mission = lock_mission(&mut tx, item.run_id, fence).await?;
        if item.attempt_id != fence.attempt_id {
            return Err(DomainError::StaleAttempt.into());
        }
        if receipt(&mut tx, reservation_id).await?.is_some() {
            tx.commit().await?;
            return Ok(DispatchDecision::Settled);
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.model_turn_dispatches WHERE reservation_id=$1) OR EXISTS(SELECT 1 FROM app.model_turn_terminals WHERE reservation_id=$1)",
        )
        .bind(reservation_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            let native_turn_id = sqlx::query_scalar(
                "SELECT native_turn_id::text FROM app.model_turn_bindings WHERE reservation_id=$1",
            )
            .bind(reservation_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(DispatchDecision::Reconcile { native_turn_id });
        }
        mission.admit(item.deadline_at)?;
        // A different Attempt cannot dispatch a reservation owned by an earlier
        // attempt. An owner-epoch takeover within the same attempt is supported.
        if item.attempt_id != fence.attempt_id {
            return Err(DomainError::StaleAttempt.into());
        }
        let rpc_request_id = reservation_id.to_string();
        sqlx::query("INSERT INTO app.model_turn_dispatches(reservation_id,owner_epoch,rpc_request_id) VALUES($1,$2,$3)")
            .bind(reservation_id.as_uuid()).bind(fence.owner_epoch.get() as i64).bind(&rpc_request_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(DispatchDecision::Send { rpc_request_id })
    }

    /// Call only with an observed native response, never a user-supplied turn ID.
    pub async fn bind_native_turn(
        &self,
        reservation_id: Id,
        fence: &WorkerFence,
        native_turn_id: &str,
    ) -> Result<(), StoreError> {
        if !bounded(native_turn_id, 200) {
            return Err(StoreError::Invalid("native_turn_id"));
        }
        let mut tx = self.pool.begin().await?;
        let item = load_reservation(&mut tx, reservation_id).await?;
        lock_mission(&mut tx, item.run_id, fence).await?;
        if item.attempt_id != fence.attempt_id {
            return Err(DomainError::StaleAttempt.into());
        }
        if let Some(old) = sqlx::query_scalar::<_, String>(
            "SELECT native_turn_id::text FROM app.model_turn_bindings WHERE reservation_id=$1",
        )
        .bind(reservation_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        {
            if old != native_turn_id {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(());
        }
        sqlx::query("INSERT INTO app.model_turn_bindings(reservation_id,session_id,native_turn_id) VALUES($1,$2,$3)")
            .bind(reservation_id.as_uuid()).bind(item.session_id.as_uuid()).bind(native_turn_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Preserve an observed terminal event even when usage is absent. It does
    /// not settle/refund any resources or acknowledge the queue notification.
    pub async fn observe_turn_terminal(
        &self,
        reservation_id: Id,
        fence: &WorkerFence,
        terminal: &TurnTerminal,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let item = load_reservation(&mut tx, reservation_id).await?;
        if item.attempt_id != fence.attempt_id {
            return Err(DomainError::StaleAttempt.into());
        }
        // An exact committed terminal is a read, not new result adoption. A
        // lost acknowledgement remains recoverable after lease loss/takeover.
        if exact_terminal(&mut tx, reservation_id, terminal).await? {
            tx.commit().await?;
            return Ok(());
        }
        let mission = match lock_mission(&mut tx, item.run_id, fence).await {
            Ok(mission) => Some(mission),
            Err(StoreError::Domain(DomainError::StaleAttempt)) => None,
            Err(error) => return Err(error),
        };
        // The first report can commit while this transaction waits for locks.
        // Do not turn its now-visible immutable fact into a stale-worker error.
        if exact_terminal(&mut tx, reservation_id, terminal).await? {
            tx.commit().await?;
            return Ok(());
        }
        let mission = mission.ok_or(DomainError::StaleAttempt)?;
        record_terminal(&mut tx, reservation_id, terminal, mission.now).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Unknown native usage must NOT call this with fabricated zeros. Preserve
    /// outstanding resources until the trusted native reconciler has evidence.
    pub async fn settle_turn(
        &self,
        reservation_id: Id,
        fence: &WorkerFence,
        usage: &UsageReceipt,
    ) -> Result<(), StoreError> {
        if !bounded(&usage.reason_code, 120)
            || usage
                .actual_cost
                .as_ref()
                .is_some_and(|v| !v.is_nonnegative())
        {
            return Err(StoreError::Invalid("usage_receipt"));
        }
        let mut tx = self.pool.begin().await?;
        let item = load_reservation(&mut tx, reservation_id).await?;
        if item.attempt_id != fence.attempt_id {
            return Err(DomainError::StaleAttempt.into());
        }
        // An exact immutable receipt is a read, not another settlement. A lost
        // response must remain recoverable after this worker loses its lease.
        if exact_receipt(&mut tx, reservation_id, usage).await? {
            tx.commit().await?;
            return Ok(());
        }
        let mission = match lock_mission(&mut tx, item.run_id, fence).await {
            Ok(mission) => Some(mission),
            Err(StoreError::Domain(DomainError::StaleAttempt)) => None,
            Err(error) => return Err(error),
        };
        // Another owner may have committed while we waited for the Mission
        // locks. Re-read before treating even a now-stale fence as a failure.
        if exact_receipt(&mut tx, reservation_id, usage).await? {
            tx.commit().await?;
            return Ok(());
        }
        let mission = mission.ok_or(DomainError::StaleAttempt)?;
        if usage.currency != item.cost_currency
            || usage.actual_cost.is_some() != item.reserved_cost.is_some()
        {
            return Err(StoreError::Conflict);
        }
        if usage.outcome == TurnOutcome::NotSent
            && (usage.actual_tokens.get() != 0
                || usage.actual_cost.as_ref().is_some_and(|c| c.is_positive()))
        {
            return Err(StoreError::Invalid("not_sent_usage"));
        }
        if let Some(old) = load_terminal(&mut tx, reservation_id).await? {
            if old.outcome != usage.outcome || old.reason_code != usage.reason_code {
                return Err(StoreError::Conflict);
            }
        } else {
            // A trusted event can contain both terminal status and usage:
            // a single event may deliver terminal and usage together. These are
            // still distinct immutable rows with an exact composite FK.
            let native_turn_id: Option<String> = sqlx::query_scalar(
                "SELECT native_turn_id::text FROM app.model_turn_bindings WHERE reservation_id=$1",
            )
            .bind(reservation_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
            record_terminal(
                &mut tx,
                reservation_id,
                &TurnTerminal {
                    outcome: usage.outcome,
                    native_turn_id,
                    reason_code: usage.reason_code.clone(),
                    observed_at: mission.now,
                },
                mission.now,
            )
            .await?;
        }
        sqlx::query("INSERT INTO app.model_turn_receipts(reservation_id,outcome,actual_tokens,actual_cost,cost_currency,usage_source,reason_code) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(reservation_id.as_uuid()).bind(usage.outcome.name()).bind(usage.actual_tokens.get() as i64)
            .bind(usage.actual_cost.as_ref().map(DecimalValue::as_decimal)).bind(usage.currency.as_deref())
            .bind(if usage.outcome==TurnOutcome::NotSent{"CONFIRMED_NOT_SENT"}else{"NATIVE_REPORT"})
            .bind(&usage.reason_code).execute(&mut *tx).await?;
        // Native usage above a policy cap is retained, not clamped. An aggregate
        // outside the wire/storage range fails the transaction without wrapping.
        mission.usage(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn exact_receipt(
    tx: &mut Tx<'_>,
    reservation_id: Id,
    expected: &UsageReceipt,
) -> Result<bool, StoreError> {
    match receipt(tx, reservation_id).await? {
        Some(old) if old == *expected => Ok(true),
        Some(_) => Err(StoreError::Conflict),
        None => Ok(false),
    }
}

async fn receipt(tx: &mut Tx<'_>, reservation_id: Id) -> Result<Option<UsageReceipt>, StoreError> {
    let Some(row)=sqlx::query("SELECT outcome,actual_tokens::bigint,actual_cost::numeric,cost_currency::text,reason_code::text FROM app.model_turn_receipts WHERE reservation_id=$1")
        .bind(reservation_id.as_uuid()).fetch_optional(&mut **tx).await? else {return Ok(None)};
    let outcome = match row.try_get::<String, _>("outcome")?.as_str() {
        "SUCCEEDED" => TurnOutcome::Succeeded,
        "FAILED" => TurnOutcome::Failed,
        "CANCELLED" => TurnOutcome::Cancelled,
        "NOT_SENT" => TurnOutcome::NotSent,
        _ => return Err(StoreError::Invalid("receipt_outcome")),
    };
    Ok(Some(UsageReceipt {
        outcome,
        actual_tokens: count(row.try_get("actual_tokens")?)?,
        actual_cost: row
            .try_get::<Option<BigDecimal>, _>("actual_cost")?
            .map(decimal)
            .transpose()?,
        currency: row.try_get("cost_currency")?,
        reason_code: row.try_get("reason_code")?,
    }))
}

fn outcome(text: &str) -> Result<TurnOutcome, StoreError> {
    match text {
        "SUCCEEDED" => Ok(TurnOutcome::Succeeded),
        "FAILED" => Ok(TurnOutcome::Failed),
        "CANCELLED" => Ok(TurnOutcome::Cancelled),
        "NOT_SENT" => Ok(TurnOutcome::NotSent),
        _ => Err(StoreError::Invalid("terminal_outcome")),
    }
}
async fn exact_terminal(
    tx: &mut Tx<'_>,
    reservation_id: Id,
    expected: &TurnTerminal,
) -> Result<bool, StoreError> {
    match load_terminal(tx, reservation_id).await? {
        Some(old) if old == *expected => Ok(true),
        Some(_) => Err(StoreError::Conflict),
        None => Ok(false),
    }
}

async fn load_terminal(
    tx: &mut Tx<'_>,
    reservation_id: Id,
) -> Result<Option<TurnTerminal>, StoreError> {
    let Some(row) = sqlx::query(
        "SELECT outcome,native_turn_id::text,reason_code::text,observed_at::timestamptz FROM app.model_turn_terminals WHERE reservation_id=$1")
        .bind(reservation_id.as_uuid()).fetch_optional(&mut **tx).await? else { return Ok(None) };
    Ok(Some(TurnTerminal {
        outcome: outcome(&row.try_get::<String, _>("outcome")?)?,
        native_turn_id: row.try_get("native_turn_id")?,
        reason_code: row.try_get("reason_code")?,
        observed_at: row.try_get("observed_at")?,
    }))
}
async fn record_terminal(
    tx: &mut Tx<'_>,
    reservation_id: Id,
    terminal: &TurnTerminal,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    if !bounded(&terminal.reason_code, 120)
        || terminal
            .native_turn_id
            .as_ref()
            .is_some_and(|v| !bounded(v, 200))
        || !terminal
            .observed_at
            .timestamp_subsec_nanos()
            .is_multiple_of(1000)
        || terminal.observed_at > now
        || (terminal.outcome == TurnOutcome::NotSent) != terminal.native_turn_id.is_none()
    {
        return Err(StoreError::Invalid("native_terminal"));
    }
    if let Some(old) = load_terminal(tx, reservation_id).await? {
        if old != *terminal {
            return Err(StoreError::Conflict);
        }
        return Ok(());
    }
    sqlx::query("INSERT INTO app.model_turn_terminals(reservation_id,native_turn_id,outcome,reason_code,observed_at) VALUES($1,$2,$3,$4,$5)")
        .bind(reservation_id.as_uuid()).bind(terminal.native_turn_id.as_deref())
        .bind(terminal.outcome.name()).bind(&terminal.reason_code).bind(terminal.observed_at)
        .execute(&mut **tx).await?;
    Ok(())
}
