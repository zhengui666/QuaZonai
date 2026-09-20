//! Finite Cycle transitions and native Mission ownership; no Agent tool loop.
use super::*;
use crate::codex_profiles::CodexProfileSnapshot;
use contracts::codex::{
    CodexEffectiveSettingsV1, CodexProfileViewV1, ConnectionMode, ProfileOrigin,
};
use sqlx::Acquire;

mod credential;
mod finish;

/// Observable native metadata only. No message text, hidden items, token or path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSessionReceipt {
    pub thread_id: String,
    pub codex_version: String,
    pub protocol_schema_version: String,
    pub effective: CodexEffectiveSettingsV1,
    pub requested_service_tier: Option<String>,
}

/// Nonsecret request metadata. Ignored saved settings are not native overrides.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveCodexRequest {
    pub schema_version: SchemaV1,
    pub profile_id: Id,
    pub profile_revision: Revision,
    pub profile_origin: ProfileOrigin,
    pub connection_mode: ConnectionMode,
    pub use_default_model_settings: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

fn requested(
    profile: &CodexProfileViewV1,
    tier: &Option<String>,
) -> Result<EffectiveCodexRequest, StoreError> {
    let saved = &profile.model_settings;
    let defaults = saved.use_default_model_settings;
    if tier.is_some() != (!defaults && saved.saved_fast_mode) {
        return Err(StoreError::Invalid("native_service_tier_override"));
    }
    Ok(EffectiveCodexRequest {
        schema_version: SchemaV1,
        profile_id: profile.id,
        profile_revision: profile.revision,
        profile_origin: profile.profile_origin,
        connection_mode: profile.connection_mode,
        use_default_model_settings: defaults,
        model: if defaults {
            None
        } else {
            saved.saved_model.clone()
        },
        reasoning_effort: if defaults {
            None
        } else {
            saved.saved_reasoning_effort.clone()
        },
        service_tier: tier.clone(),
    })
}

fn frozen_profile(row: &PgRow) -> Result<CodexProfileViewV1, StoreError> {
    let document: Value = row.try_get("profile_snapshot")?;
    serde_json::from_value(
        document
            .get("profile")
            .cloned()
            .ok_or(StoreError::Integrity)?,
    )
    .map_err(|_| StoreError::Integrity)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionSession {
    pub id: Id,
    pub requested_settings: EffectiveCodexRequest,
    pub native: NativeSessionReceipt,
}

/// Trusted launcher input; intentionally not serializable or Debug.
pub struct MissionJob {
    pub lease: RunLease,
    pub brief_id: Id,
    pub role: String,
    pub profile: CodexProfileSnapshot,
    pub session: Option<MissionSession>,
    pub observed_at: DateTime<Utc>,
}

fn session(row: &PgRow) -> Result<MissionSession, StoreError> {
    let requested_settings: EffectiveCodexRequest =
        serde_json::from_value(row.try_get("requested_settings")?)
            .map_err(|_| StoreError::Integrity)?;
    Ok(MissionSession {
        id: db::id(row.try_get("id")?)?,
        native: NativeSessionReceipt {
            requested_service_tier: requested_settings.service_tier.clone(),
            thread_id: row.try_get("thread_id")?,
            codex_version: row.try_get("codex_version")?,
            protocol_schema_version: row.try_get("protocol_schema_version")?,
            effective: CodexEffectiveSettingsV1 {
                model: row
                    .try_get::<Option<String>, _>("observed_model")?
                    .ok_or(StoreError::Integrity)?,
                provider: row
                    .try_get::<Option<String>, _>("observed_provider")?
                    .ok_or(StoreError::Integrity)?,
                reasoning_effort: row.try_get("observed_effort")?,
                service_tier: row.try_get("observed_service_tier")?,
            },
        },
        requested_settings,
    })
}

pub(crate) async fn current_profile(
    tx: &mut Tx<'_>,
    run: Id,
) -> Result<CodexProfileSnapshot, StoreError> {
    let m = sqlx::query("SELECT m.profile_id,m.profile_revision,r.enabled,r.revision AS actual_revision,a.runtime_revision FROM app.run_missions m JOIN app.run_admissions a ON a.run_id=m.run_id JOIN app.runtime_integrations r ON r.id=a.runtime_id WHERE m.run_id=$1 FOR SHARE OF r")
        .bind(run.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::Invalid("mission_not_defined"))?;
    if !m.try_get::<bool, _>("enabled")?
        || m.try_get::<i64, _>("actual_revision")? != m.try_get::<i64, _>("runtime_revision")?
    {
        return Err(DomainError::CapabilityUnavailable("mission_runtime_binding").into());
    }
    crate::codex_profiles::snapshot(
        tx,
        db::id(m.try_get("profile_id")?)?,
        db::revision(m.try_get("profile_revision")?)?,
    )
    .await
}

async fn waiting(tx: &mut Tx<'_>, cycle: Id, reason: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE app.research_cycles SET state='WAITING_INPUT',next_action=$2 WHERE id=$1")
        .bind(cycle.as_uuid())
        .bind(reason)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

impl Store {
    /// A native partial usage observation can stop spending, never settle it.
    /// This trusted driver operation is not exposed to an Agent or HTTP caller.
    pub async fn observe_mission_token_limit(
        &self,
        run: Id,
        owner: &WorkerFence,
        reservation: Id,
        observed_tokens: DbCounter,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        fence(&mut tx, &locked.run, owner).await?;
        if locked.run.kind != RunKind::AgentResearch || locked.run.state.is_terminal() {
            return Err(StoreError::Conflict);
        }
        let reserved: i64 = sqlx::query_scalar("SELECT r.reserved_tokens FROM app.model_turn_reservations r JOIN app.model_turn_bindings b ON b.reservation_id=r.id WHERE r.id=$1 AND r.run_id=$2 AND r.attempt_id=$3")
            .bind(reservation.as_uuid()).bind(run.as_uuid()).bind(owner.attempt_id.as_uuid())
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::Conflict)?;
        let reserved_tokens = counter(reserved)?;
        if observed_tokens < reserved_tokens {
            return Err(StoreError::Invalid("native_token_limit_not_reached"));
        }
        let (already_recorded, settled): (bool, Option<i64>) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM app.run_events WHERE run_id=$1 AND event_type='mission.token_limit' AND payload->>'reservation_id'=$2),(SELECT actual_tokens FROM app.model_turn_receipts WHERE reservation_id=$3)")
            .bind(run.as_uuid()).bind(reservation.to_string()).bind(reservation.as_uuid())
            .fetch_one(&mut *tx).await?;
        if settled.is_some_and(|tokens| tokens < observed_tokens.get() as i64) {
            return Err(StoreError::Conflict);
        }
        if !already_recorded && settled.is_none() {
            let next = locked
                .run
                .last_event_seq
                .checked_add(1)
                .ok_or(StoreError::Integrity)?;
            sqlx::query("INSERT INTO app.run_events(run_id,seq,attempt_id,event_type,schema_version,payload,occurred_at) VALUES($1,$2,$3,'mission.token_limit',1,$4,clock_timestamp())")
                .bind(run.as_uuid()).bind(next.get() as i64).bind(owner.attempt_id.as_uuid())
                .bind(json!({"schema_version":1,"reservation_id":reservation,"observed_tokens":observed_tokens,"reserved_tokens":reserved_tokens}))
                .execute(&mut *tx).await?;
            if locked.run.state != RunState::CancelRequested {
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
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn mission_job(
        &self,
        run: Id,
        owner: &WorkerFence,
    ) -> Result<MissionJob, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut locked = lock_run(&mut tx, run).await?;
        let a = fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        let m = sqlx::query("SELECT m.*,c.brief_id FROM app.run_missions m JOIN app.research_cycles c ON c.id=m.cycle_id WHERE m.run_id=$1")
            .bind(run.as_uuid()).fetch_optional(&mut *tx).await?
            .ok_or(StoreError::Invalid("mission_not_defined"))?;
        expire_sent_run(&mut tx, &mut locked, &a).await?;
        let profile = CodexProfileSnapshot {
            profile: frozen_profile(&m)?,
        };
        let native = sqlx::query("SELECT * FROM app.codex_sessions WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .as_ref()
            .map(session)
            .transpose()?;
        let job = MissionJob {
            observed_at: now(&mut tx).await?,
            lease: lease_view(locked, &a)?,
            brief_id: db::id(m.try_get("brief_id")?)?,
            role: m.try_get("role")?,
            profile,
            session: native,
        };
        tx.commit().await?;
        Ok(job)
    }

    /// Record the first native acknowledgement before any model-turn reservation.
    /// A cancellation/profile edit in flight cannot erase this observed identity.
    pub async fn bind_mission_session(
        &self,
        run: Id,
        owner: &WorkerFence,
        receipt: &NativeSessionReceipt,
    ) -> Result<MissionSession, StoreError> {
        for text in [
            &receipt.thread_id,
            &receipt.codex_version,
            &receipt.protocol_schema_version,
            &receipt.effective.model,
            &receipt.effective.provider,
        ]
        .into_iter()
        .chain(receipt.effective.reasoning_effort.iter())
        .chain(receipt.effective.service_tier.iter())
        .chain(receipt.requested_service_tier.iter())
        {
            domain::control::text(text, 1, 200, false)?;
        }
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        let a = fence(&mut tx, &locked.run, owner).await?;
        if locked.run.state.is_terminal() {
            return Err(DomainError::TerminalRun.into());
        }
        if a.try_get::<String, _>("dispatch_state")? == "NOT_SENT" {
            return Err(StoreError::Conflict);
        }
        let m = sqlx::query("SELECT * FROM app.run_missions WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::Invalid("mission_not_defined"))?;
        if let Some(row) = sqlx::query("SELECT * FROM app.codex_sessions WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
        {
            let original = session(&row)?;
            if original.native != *receipt {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(original);
        }
        let profile = frozen_profile(&m)?;
        let requested = requested(&profile, &receipt.requested_service_tier)?;
        let row = sqlx::query("INSERT INTO app.codex_sessions(project_id,cycle_id,run_id,role,profile_id,profile_revision,thread_id,codex_version,protocol_schema_version,requested_settings,observed_model,observed_effort,observed_provider,observed_service_tier,native_history_ref) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$7) RETURNING *")
            .bind(locked.run.project_id.as_uuid()).bind(locked.run.cycle_id.map(Id::as_uuid)).bind(run.as_uuid())
            .bind(m.try_get::<String,_>("role")?).bind(profile.id.as_uuid()).bind(profile.revision.get() as i64)
            .bind(&receipt.thread_id).bind(&receipt.codex_version).bind(&receipt.protocol_schema_version).bind(db::json(&requested)?)
            .bind(&receipt.effective.model).bind(&receipt.effective.reasoning_effort).bind(&receipt.effective.provider).bind(&receipt.effective.service_tier)
            .fetch_one(&mut *tx).await?;
        let result = session(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    /// Called before either normal or redelivered science completion is acked.
    /// False retains the queue notification for a paused/in-progress Cycle.
    pub async fn advance_initial_cycle(&self, run: Id) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let startup = sqlx::query("SELECT * FROM app.cycle_startups WHERE initial_run_id=$1")
            .bind(run.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
        let Some(startup) = startup else {
            tx.commit().await?;
            return Ok(true);
        };
        let locked = lock_run(&mut tx, run).await?;
        let cycle = locked.run.cycle_id.ok_or(StoreError::Integrity)?;
        if locked.run.kind != RunKind::DataValidate {
            return Err(StoreError::Integrity);
        }
        if !locked.run.state.is_terminal() {
            return Ok(false);
        }
        let already: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.run_missions WHERE cycle_id=$1 AND role='RESEARCHER')",
        )
        .bind(cycle.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if already
            || matches!(
                locked.cycle_state.as_deref(),
                Some("COMPLETED" | "FAILED" | "CANCELLED" | "WAITING_INPUT")
            )
        {
            tx.commit().await?;
            return Ok(true);
        }
        if locked.run.state != RunState::Succeeded {
            let state = if locked.run.state == RunState::Cancelled {
                "CANCELLED"
            } else {
                "FAILED"
            };
            sqlx::query("UPDATE app.research_cycles SET state=$2,ended_at=clock_timestamp(),next_action='DATA_VALIDATE_NOT_SUCCEEDED' WHERE id=$1")
                .bind(cycle.as_uuid()).bind(state).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(true);
        }
        if !locked.admission_open() {
            return Ok(false);
        }
        let proven: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.run_native_tasks n JOIN app.run_attempts a ON a.run_id=n.run_id JOIN app.run_terminal_receipts t ON t.run_id=n.run_id AND t.attempt_id=a.id JOIN app.run_native_outputs o ON o.attempt_id=a.id JOIN app.artifacts f ON f.id=o.artifact_id WHERE n.run_id=$1 AND a.id=$2 AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL AND t.terminal_state='SUCCEEDED' AND f.producer_run_id=n.run_id AND f.producer_attempt_id=a.id AND f.kind='DATA_QUALITY' AND f.schema_name='qz.data_quality' AND f.schema_version='1' AND f.access_class='RESEARCH')")
            .bind(run.as_uuid()).bind(locked.run.active_attempt_id.map(Id::as_uuid)).fetch_one(&mut *tx).await?;
        if !proven {
            waiting(&mut tx, cycle, "NATIVE_DATA_VALIDATION_REQUIRED").await?;
            tx.commit().await?;
            return Ok(true);
        }
        let (tx, advanced) = admit_role(tx, &locked, &startup, "RESEARCHER").await?;
        tx.commit().await?;
        Ok(advanced)
    }
}

pub(super) async fn admit_role<'a>(
    mut tx: Tx<'a>,
    locked: &LockedRun,
    startup: &PgRow,
    role: &str,
) -> Result<(Tx<'a>, bool), StoreError> {
    let cycle = locked.run.cycle_id.ok_or(StoreError::Integrity)?;
    let (profile_column, revision_column, key, next) = match role {
        "RESEARCHER" => (
            "researcher_profile_id",
            "researcher_profile_revision",
            "cycle/researcher",
            "RESEARCH_MISSION",
        ),
        "INDEPENDENT_REVIEWER" => (
            "reviewer_profile_id",
            "reviewer_profile_revision",
            "cycle/reviewer",
            "INDEPENDENT_REVIEW",
        ),
        _ => return Err(StoreError::Integrity),
    };
    let choice = match (
        db::optional_id(startup, profile_column)?,
        startup.try_get::<Option<i64>, _>(revision_column)?,
    ) {
        (Some(id), Some(revision)) => Some((id, db::revision(revision)?)),
        (None, None) => None,
        _ => return Err(StoreError::Integrity),
    };
    let Some((profile, revision)) = choice else {
        waiting(&mut tx, cycle, "EXPLICIT_CODEX_PROFILE_REQUIRED").await?;
        return Ok((tx, true));
    };
    let selected = match crate::codex_profiles::snapshot(&mut tx, profile, revision).await {
        Ok(selected) if selected.profile.home_binding.is_some() => selected,
        Err(StoreError::Conflict) => {
            // A human login/logout in flight is temporary, not a new Cycle
            // decision. Retain the notification so completion can resume it.
            sqlx::query("UPDATE app.research_cycles SET next_action='WAITING_FOR_CODEX_ACCOUNT' WHERE id=$1")
                    .bind(cycle.as_uuid()).execute(&mut *tx).await?;
            return Ok((tx, false));
        }
        Ok(_)
        | Err(
            StoreError::NotFound
            | StoreError::RevisionConflict { .. }
            | StoreError::Domain(DomainError::CapabilityUnavailable(_)),
        ) => {
            waiting(&mut tx, cycle, "CODEX_PROFILE_REQUIRES_ATTENTION").await?;
            return Ok((tx, true));
        }
        Err(error) => return Err(error),
    };
    let c = sqlx::query("SELECT brief_id,budget_snapshot FROM app.research_cycles WHERE id=$1")
        .bind(cycle.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
    let context =
        crate::cycles::execution_context(&mut tx, db::id(c.try_get("brief_id")?)?).await?;
    let budget: BudgetV1 =
        serde_json::from_value(c.try_get("budget_snapshot")?).map_err(|_| StoreError::Integrity)?;
    let request = RunSubmission {
        cycle_id: cycle,
        input_set_id: context.discovery_input_set_id,
        runtime_id: context.runtime_id,
        runtime_revision: context.runtime_revision,
        kind: RunKind::AgentResearch,
        limits: JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: counter((budget.max_cpu_seconds.get() / 10).clamp(1, 300) as i64)?,
            wall_seconds: budget.max_wall_seconds,
            memory_mib: budget.max_memory_mib.min(4096),
            output_bytes: counter(budget.max_output_bytes.get().min(64 * 1024 * 1024) as i64)?,
        },
    };
    // A native savepoint permits a domain rejection to leave an honest Cycle
    // status, while no partial reservation/Run/PGMQ can survive the rejection.
    let admitted = async {
        let (inner, admitted) =
            Store::enqueue_run_in_transaction(tx.begin().await?, key, &request).await?;
        inner.commit().await?;
        Ok::<_, StoreError>(admitted)
    }
    .await;
    match admitted {
        Ok(admitted) => {
            sqlx::query("INSERT INTO app.run_missions(run_id,project_id,cycle_id,role,profile_id,profile_revision,profile_snapshot) VALUES($1,$2,$3,$7,$4,$5,$6)")
                    .bind(admitted.resource.id.as_uuid()).bind(locked.run.project_id.as_uuid()).bind(cycle.as_uuid()).bind(profile.as_uuid()).bind(revision.get() as i64)
                    .bind(json!({"schema_version":1,"profile":selected.profile})).bind(role)
                    .execute(&mut *tx).await?;
            sqlx::query("UPDATE app.research_cycles SET next_action=$2 WHERE id=$1")
                .bind(cycle.as_uuid())
                .bind(next)
                .execute(&mut *tx)
                .await?;
        }
        Err(StoreError::Domain(DomainError::BudgetExhausted(_))) => {
            sqlx::query("UPDATE app.research_cycles SET state='COMPLETED',outcome='BUDGET_EXHAUSTED',ended_at=clock_timestamp(),next_action='REVIEW_CYCLE_BUDGET' WHERE id=$1")
                    .bind(cycle.as_uuid()).execute(&mut *tx).await?;
        }
        Err(StoreError::Domain(DomainError::Fields(_) | DomainError::CapabilityUnavailable(_))) => {
            waiting(&mut tx, cycle, "RESEARCH_INPUTS_REQUIRE_ATTENTION").await?;
        }
        Err(error) => return Err(error),
    }
    Ok((tx, true))
}
