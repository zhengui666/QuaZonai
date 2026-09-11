//! Human account-operation intents are not an OAuth implementation. Every native
//! write obtains a committed, one-use send permit; missing replies stay unknown.
use super::*;

pub enum CodexAccountPreparation {
    Replay(CommandResult<CodexAccountOperationRefV1>),
    Start(Box<CodexAccountTicket>),
}

/// Only the original accepted operation owner receives this ticket. It contains
/// no native token, device code or canonical history.
pub struct CodexAccountTicket {
    pub acceptance: CommandResult<CodexAccountOperationRefV1>,
    pub snapshot: CodexProfileSnapshot,
    actor: Actor,
    key: String,
    request: CodexAccountRequestV1,
}

pub enum CodexAccountNext {
    Wait,
    Cancel(String),
    Finished,
}

pub enum CodexAccountCompletion {
    LoggedIn(CodexAccountV1),
    LoggedOut(CodexAccountV1),
    Cancelled,
    Failed(CodexAccountReasonV1),
    Unknown(CodexAccountReasonV1),
}

fn operation_kind(action: CodexAccountActionV1) -> OperatorOperation {
    match action {
        CodexAccountActionV1::Login => OperatorOperation::CodexLoginStart,
        CodexAccountActionV1::Logout => OperatorOperation::CodexLogout,
    }
}

fn reference(row: &sqlx::postgres::PgRow) -> Result<CodexAccountOperationRefV1, StoreError> {
    Ok(CodexAccountOperationRefV1 {
        id: db::id(row.try_get("id")?)?,
        profile_id: db::id(row.try_get("profile_id")?)?,
        profile_revision: db::revision(row.try_get("profile_revision")?)?,
        action: db::enum_value(row, "action")?,
        created_at: row.try_get("created_at")?,
        deadline_at: row.try_get("deadline_at")?,
    })
}

fn operation_view(row: &sqlx::postgres::PgRow) -> Result<CodexAccountOperationV1, StoreError> {
    let account = row
        .try_get::<Option<Value>, _>("account_snapshot")?
        .map(|value| {
            serde_json::from_value(value.get("account").cloned().ok_or(StoreError::Integrity)?)
                .map_err(|_| StoreError::Integrity)
        })
        .transpose()?;
    let reason = row
        .try_get::<Option<String>, _>("reason_code")?
        .map(|value| serde_json::from_value(json!(value)).map_err(|_| StoreError::Integrity))
        .transpose()?;
    Ok(CodexAccountOperationV1 {
        schema_version: SchemaV1,
        operation: reference(row)?,
        state: db::enum_value(row, "state")?,
        revision: db::revision(row.try_get("revision")?)?,
        updated_at: row.try_get("updated_at")?,
        finished_at: row.try_get("finished_at")?,
        reason,
        account,
    })
}

async fn locked_operation(
    tx: &mut Transaction<'_, Postgres>,
    id: Id,
) -> Result<sqlx::postgres::PgRow, StoreError> {
    let profile: uuid::Uuid =
        sqlx::query_scalar("SELECT profile_id FROM app.codex_account_operations WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    // Every update uses profile -> account-operation, including cancellation and native facts.
    super::row(tx, db::id(profile)?, true).await?;
    sqlx::query("SELECT * FROM app.codex_account_operations WHERE id=$1 FOR UPDATE")
        .bind(id.as_uuid())
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
}

pub(super) async fn no_active_account_operation(
    tx: &mut Transaction<'_, Postgres>,
    profile: Id,
) -> Result<(), StoreError> {
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.codex_account_operations WHERE profile_id=$1 AND state IN ('REQUESTED','WAITING','CANCEL_REQUESTED'))")
        .bind(profile.as_uuid()).fetch_one(&mut **tx).await?;
    if active {
        return Err(StoreError::Conflict);
    }
    Ok(())
}

pub(super) async fn observation_after_account_operations(
    tx: &mut Transaction<'_, Postgres>,
    profile: Id,
    began: DateTime<Utc>,
) -> Result<bool, StoreError> {
    sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM app.codex_account_operations WHERE profile_id=$1 AND (state IN ('REQUESTED','WAITING','CANCEL_REQUESTED') OR created_at >= $2 OR finished_at >= $2))")
        .bind(profile.as_uuid()).bind(began).fetch_one(&mut **tx).await.map_err(Into::into)
}

impl Store {
    pub async fn prepare_codex_account(
        &self,
        actor: &Actor,
        key: &str,
        request: &CodexAccountRequestV1,
        action: CodexAccountActionV1,
    ) -> Result<CodexAccountPreparation, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            operation_kind(action),
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(CodexAccountPreparation::Replay(replay));
        }
        let row = super::row(&mut tx, request.profile_id, true).await?;
        let profile = super::view(&row)?;
        if profile.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                current: profile.revision,
            });
        }
        if profile.connection_mode != ConnectionMode::System || profile.home_binding.is_none() {
            return Err(StoreError::Invalid("native_system_profile_required"));
        }
        // A fresh human command may retire an expired owner, never resend its RPC.
        sqlx::query("UPDATE app.codex_account_operations SET state=CASE WHEN dispatch_started_at IS NULL THEN 'FAILED' ELSE 'UNKNOWN' END,reason_code='WAIT_WINDOW_ENDED',finished_at=clock_timestamp() WHERE profile_id=$1 AND state IN ('REQUESTED','WAITING','CANCEL_REQUESTED') AND deadline_at<=clock_timestamp()")
            .bind(profile.id.as_uuid()).execute(&mut *tx).await?;
        no_active_account_operation(&mut tx, profile.id).await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let duration = if action == CodexAccountActionV1::Login {
            900
        } else {
            120
        };
        let row = sqlx::query("INSERT INTO app.codex_account_operations(id,profile_id,profile_revision,action,state,created_at,updated_at,deadline_at) VALUES($1,$2,$3,$4,'REQUESTED',$5,$5,$6) RETURNING *")
            .bind(prepared.target.as_uuid()).bind(profile.id.as_uuid())
            .bind(profile.revision.get() as i64).bind(db::code(&action)?)
            .bind(now).bind(now + Duration::seconds(duration)).fetch_one(&mut *tx).await?;
        let acceptance = commands::finish(&mut tx, prepared, reference(&row)?, 202).await?;
        tx.commit().await?;
        Ok(CodexAccountPreparation::Start(Box::new(
            CodexAccountTicket {
                acceptance,
                snapshot: CodexProfileSnapshot {
                    profile,
                    credential_ref: None,
                },
                actor: actor.clone(),
                key: key.to_owned(),
                request: request.clone(),
            },
        )))
    }

    pub async fn read_codex_account_operation(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<CodexAccountOperationV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        crate::settings::read_authority(&mut tx, actor).await?;
        let row = sqlx::query("SELECT * FROM app.codex_account_operations WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let result = operation_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn latest_codex_account_operation(
        &self,
        actor: &Actor,
        profile: Id,
    ) -> Result<Option<CodexAccountOperationV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        super::row(&mut tx, profile, false).await?;
        let row = sqlx::query("SELECT * FROM app.codex_account_operations WHERE profile_id=$1 ORDER BY created_at DESC,id DESC LIMIT 1")
            .bind(profile.as_uuid()).fetch_optional(&mut *tx).await?;
        let result = row.as_ref().map(operation_view).transpose()?;
        tx.commit().await?;
        Ok(result)
    }

    /// A native child may be initialized before this permission, but no account
    /// mutation is sent until this transaction commits successfully.
    pub async fn begin_codex_account(
        &self,
        ticket: &CodexAccountTicket,
    ) -> Result<bool, StoreError> {
        let expected = &ticket.acceptance.resource;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            &ticket.actor,
            operation_kind(expected.action),
            &ticket.key,
            None,
            db::json(&ticket.request)?,
        )
        .await?;
        let replay: CommandResult<CodexAccountOperationRefV1> =
            prepared.replay()?.ok_or(StoreError::Integrity)?;
        if replay.resource != *expected {
            return Err(StoreError::Integrity);
        }
        let row = locked_operation(&mut tx, expected.id).await?;
        let view = operation_view(&row)?;
        if view.state != CodexAccountOperationStateV1::Requested
            || row
                .try_get::<Option<DateTime<Utc>>, _>("dispatch_started_at")?
                .is_some()
        {
            tx.commit().await?;
            return Ok(false);
        }
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if now >= expected.deadline_at || now >= expected.created_at + Duration::seconds(120) {
            return Err(StoreError::IntegrationUnavailable);
        }
        let current = super::view(&super::row(&mut tx, expected.profile_id, true).await?)?;
        if current.revision != expected.profile_revision
            || current.connection_mode != ConnectionMode::System
        {
            return Err(StoreError::RevisionConflict {
                current: current.revision,
            });
        }
        commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
        sqlx::query("UPDATE app.codex_account_operations SET dispatch_started_at=clock_timestamp() WHERE id=$1")
            .bind(expected.id.as_uuid()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn bind_codex_login(
        &self,
        ticket: &CodexAccountTicket,
        native_login_id: &str,
    ) -> Result<CodexAccountOperationV1, StoreError> {
        domain::control::text(native_login_id, 1, 200, false)?;
        let mut tx = self.pool.begin().await?;
        let row = locked_operation(&mut tx, ticket.acceptance.resource.id).await?;
        let current = operation_view(&row)?;
        if current.operation.action != CodexAccountActionV1::Login
            || current.state.is_terminal()
            || row
                .try_get::<Option<DateTime<Utc>>, _>("dispatch_started_at")?
                .is_none()
        {
            return Err(StoreError::Conflict);
        }
        if let Some(previous) = row.try_get::<Option<String>, _>("native_login_id")? {
            if previous != native_login_id {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(current);
        }
        let row = sqlx::query("UPDATE app.codex_account_operations SET native_login_id=$2,state=CASE WHEN state='CANCEL_REQUESTED' THEN state ELSE 'WAITING' END WHERE id=$1 RETURNING *")
            .bind(current.operation.id.as_uuid()).bind(native_login_id).fetch_one(&mut *tx).await?;
        let result = operation_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn cancel_codex_login(
        &self,
        actor: &Actor,
        key: &str,
        request: &CodexLoginCancelV1,
    ) -> Result<CommandResult<CodexAccountOperationV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::CodexLoginCancel,
            key,
            Some(request.operation_id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let row = locked_operation(&mut tx, request.operation_id).await?;
        let current = operation_view(&row)?;
        if current.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                current: current.revision,
            });
        }
        if current.operation.action != CodexAccountActionV1::Login {
            return Err(StoreError::Invalid("login_operation_required"));
        }
        let next = if current.state.is_terminal()
            || current.state == CodexAccountOperationStateV1::CancelRequested
        {
            current
        } else {
            let unsent = row
                .try_get::<Option<DateTime<Utc>>, _>("dispatch_started_at")?
                .is_none();
            let row = if unsent {
                sqlx::query("UPDATE app.codex_account_operations SET state='CANCELLED',cancel_requested_at=clock_timestamp(),finished_at=clock_timestamp(),reason_code='CONFIRMED_NOT_SENT' WHERE id=$1 RETURNING *")
                    .bind(request.operation_id.as_uuid()).fetch_one(&mut *tx).await?
            } else {
                sqlx::query("UPDATE app.codex_account_operations SET state='CANCEL_REQUESTED',cancel_requested_at=clock_timestamp() WHERE id=$1 RETURNING *")
                    .bind(request.operation_id.as_uuid()).fetch_one(&mut *tx).await?
            };
            operation_view(&row)?
        };
        let result = commands::finish(&mut tx, prepared, next, 202).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Only the current accepted login's native client consumes this permission.
    /// Its database-time deadline requests cancellation; it does not prove it.
    pub async fn next_codex_account(
        &self,
        ticket: &CodexAccountTicket,
    ) -> Result<CodexAccountNext, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut row = locked_operation(&mut tx, ticket.acceptance.resource.id).await?;
        let current = operation_view(&row)?;
        if current.state.is_terminal() {
            tx.commit().await?;
            return Ok(CodexAccountNext::Finished);
        }
        if current.operation.action != CodexAccountActionV1::Login {
            return Err(StoreError::Conflict);
        }
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if now >= current.operation.deadline_at
            && current.state != CodexAccountOperationStateV1::CancelRequested
        {
            row = sqlx::query("UPDATE app.codex_account_operations SET state='CANCEL_REQUESTED',cancel_requested_at=clock_timestamp() WHERE id=$1 RETURNING *")
                .bind(current.operation.id.as_uuid()).fetch_one(&mut *tx).await?;
        }
        let action = if row.try_get::<String, _>("state")? == "CANCEL_REQUESTED"
            && row
                .try_get::<Option<DateTime<Utc>>, _>("cancel_dispatch_started_at")?
                .is_none()
        {
            let login_id = row
                .try_get::<Option<String>, _>("native_login_id")?
                .ok_or(StoreError::Conflict)?;
            sqlx::query("UPDATE app.codex_account_operations SET cancel_dispatch_started_at=clock_timestamp() WHERE id=$1")
                .bind(current.operation.id.as_uuid()).execute(&mut *tx).await?;
            CodexAccountNext::Cancel(login_id)
        } else {
            CodexAccountNext::Wait
        };
        tx.commit().await?;
        Ok(action)
    }

    pub async fn complete_codex_account(
        &self,
        ticket: &CodexAccountTicket,
        completion: CodexAccountCompletion,
    ) -> Result<CodexAccountOperationV1, StoreError> {
        use CodexAccountOperationStateV1 as State;
        use CodexAccountReasonV1 as Reason;
        let (state, reason, account) = match completion {
            CodexAccountCompletion::LoggedIn(account) => {
                rules::account_snapshot(&account)?;
                if account.authentication_kind != Some(CodexAuthenticationKind::Chatgpt) {
                    return Err(StoreError::Invalid("native_chatgpt_login_not_observed"));
                }
                (
                    State::Succeeded,
                    Reason::NativeLoginCompleted,
                    Some(account),
                )
            }
            CodexAccountCompletion::LoggedOut(account) => {
                rules::account_snapshot(&account)?;
                (
                    State::Succeeded,
                    Reason::NativeLogoutCompleted,
                    Some(account),
                )
            }
            CodexAccountCompletion::Cancelled => {
                (State::Cancelled, Reason::NativeCancelConfirmed, None)
            }
            CodexAccountCompletion::Failed(reason) => {
                if !matches!(
                    reason,
                    Reason::NativeLoginRejected
                        | Reason::NativeContractUnsupported
                        | Reason::NativeVersionUnsupported
                        | Reason::DeploymentUnavailable
                        | Reason::ProfileChanged
                ) {
                    return Err(StoreError::Invalid("native_account_failure_reason"));
                }
                (State::Failed, reason, None)
            }
            CodexAccountCompletion::Unknown(reason) => {
                if !matches!(
                    reason,
                    Reason::NativeResponseUnknown
                        | Reason::NativeContractUnsupported
                        | Reason::WaitWindowEnded
                        | Reason::OwnerUnavailable
                ) {
                    return Err(StoreError::Invalid("native_account_unknown_reason"));
                }
                (State::Unknown, reason, None)
            }
        };
        let mut tx = self.pool.begin().await?;
        let row = locked_operation(&mut tx, ticket.acceptance.resource.id).await?;
        let current = operation_view(&row)?;
        if current.operation != ticket.acceptance.resource {
            return Err(StoreError::Integrity);
        }
        if current.state.is_terminal() {
            if current.state != state
                || current.reason != Some(reason)
                || current.account != account
            {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(current);
        }
        let snapshot = account.map(|account| json!({"schema_version":1,"account":account}));
        let row = sqlx::query("UPDATE app.codex_account_operations SET state=$2,reason_code=$3,account_snapshot=$4,finished_at=clock_timestamp() WHERE id=$1 RETURNING *")
            .bind(current.operation.id.as_uuid()).bind(db::code(&state)?).bind(db::code(&reason)?)
            .bind(snapshot).fetch_one(&mut *tx).await?;
        let result = operation_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    /// A human cancellation found no owned native client in the single API owner.
    /// Preserve uncertainty; never call a fresh login/cancel RPC to guess its past result.
    pub async fn orphaned_codex_login(
        &self,
        expected: &CodexAccountOperationRefV1,
    ) -> Result<CodexAccountOperationV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = locked_operation(&mut tx, expected.id).await?;
        let current = operation_view(&row)?;
        if current.operation != *expected {
            return Err(StoreError::Conflict);
        }
        if current.state.is_terminal() {
            tx.commit().await?;
            return Ok(current);
        }
        if current.state != CodexAccountOperationStateV1::CancelRequested {
            return Err(StoreError::Conflict);
        }
        let row = sqlx::query("UPDATE app.codex_account_operations SET state='UNKNOWN',reason_code='OWNER_UNAVAILABLE',finished_at=clock_timestamp() WHERE id=$1 RETURNING *")
            .bind(expected.id.as_uuid()).fetch_one(&mut *tx).await?;
        let result = operation_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }
}
