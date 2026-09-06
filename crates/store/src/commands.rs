//! Immutable original command results. Operator writes serialize on the real
//! single-operator authority row, not on an invented workflow/intent service.
use crate::{
    auth,
    authority::{self, Actor},
    db, Store, StoreError,
};
use chrono::{DateTime, Duration, Utc};
use contracts::{control::*, Id, SchemaV1};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};

pub(crate) fn key(value: &str) -> Result<(), StoreError> {
    if value.is_empty()
        || value.len() > 200
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(StoreError::Invalid("idempotency_key"));
    }
    Ok(())
}
pub(crate) struct Prepared {
    pub target: Id,
    scope: String,
    operation: &'static str,
    key: String,
    request: Value,
    grant: Option<Id>,
    pub replay: Option<Value>,
}
impl Prepared {
    pub fn replay<T: DeserializeOwned>(&self) -> Result<Option<CommandResult<T>>, StoreError> {
        self.replay
            .as_ref()
            .map(|value| {
                let mut result: CommandResult<T> =
                    serde_json::from_value(value.clone()).map_err(|_| StoreError::Integrity)?;
                result.replayed = true;
                Ok(result)
            })
            .transpose()
    }
}
struct Receipt {
    id: Id,
    target: Id,
    response: Value,
}
async fn prior(
    tx: &mut Transaction<'_, Postgres>,
    scope: &str,
    operation: &str,
    key: &str,
    request: &Value,
) -> Result<Option<Receipt>, StoreError> {
    let Some(row)=sqlx::query("SELECT id,resource_id,normalized_nonsecret_request,response_nonsecret_body FROM app.command_receipts WHERE principal_scope=$1 AND operation=$2 AND idempotency_key=$3")
        .bind(scope).bind(operation).bind(key).fetch_optional(&mut **tx).await? else{return Ok(None)};
    if row.try_get::<Value, _>("normalized_nonsecret_request")? != *request {
        return Err(StoreError::IdempotencyConflict);
    }
    Ok(Some(Receipt {
        id: db::id(row.try_get("id")?)?,
        target: db::id(row.try_get("resource_id")?)?,
        response: row
            .try_get::<Option<Value>, _>("response_nonsecret_body")?
            .ok_or(StoreError::Integrity)?,
    }))
}

pub(crate) async fn operator(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    operation: OperatorOperation,
    idempotency_key: &str,
    target: Option<Id>,
    request: Value,
) -> Result<Prepared, StoreError> {
    key(idempotency_key)?;
    if operation.creates() != target.is_none() {
        return Err(StoreError::Invalid("operation_target"));
    }
    let (scope, grant) = match actor {
        Actor::Browser { .. } => {
            authority::browser(tx, actor, true, true).await?;
            (String::from("OPERATOR"), None)
        }
        Actor::Machine { operator_grant, .. } => {
            let epoch:i64=sqlx::query_scalar("SELECT session_epoch::bigint FROM app.operator_auth_state WHERE singleton AND initialized FOR UPDATE")
                .fetch_optional(&mut **tx).await?.ok_or(StoreError::AuthenticationRequired)?;
            let machine = authority::machine(tx, actor, true).await?;
            if machine.kind != PrincipalKind::Cli {
                return Err(StoreError::Forbidden);
            }
            let grant_id = operator_grant.ok_or(StoreError::Forbidden)?;
            let grant=sqlx::query("SELECT id,credential_id,operation,target_id,auth_epoch,expires_at,normalized_nonsecret_request FROM app.operator_command_grants WHERE id=$1 FOR UPDATE")
                .bind(grant_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Forbidden)?;
            if grant.try_get::<uuid::Uuid, _>("credential_id")? != machine.credential_id.as_uuid()
                || grant.try_get::<String, _>("operation")? != operation.code()
                || grant.try_get::<i64, _>("auth_epoch")? != epoch
                || grant
                    .try_get::<Option<Value>, _>("normalized_nonsecret_request")?
                    .as_ref()
                    != Some(&request)
                || target.is_some_and(|t| {
                    grant.try_get::<uuid::Uuid, _>("target_id").ok() != Some(t.as_uuid())
                })
            {
                return Err(StoreError::Forbidden);
            }
            (format!("CREDENTIAL:{}", machine.credential_id), Some(grant))
        }
    };
    let previous = prior(tx, &scope, operation.code(), idempotency_key, &request).await?;
    let target = if let Some(previous) = &previous {
        if target.is_some_and(|target| target != previous.target) {
            return Err(StoreError::IdempotencyConflict);
        }
        previous.target
    } else if let Some(grant) = &grant {
        db::id(grant.try_get("target_id")?)?
    } else {
        target.unwrap_or_default()
    };
    let grant_id = if let Some(grant) = grant {
        if grant.try_get::<uuid::Uuid, _>("target_id")? != target.as_uuid() {
            return Err(StoreError::Forbidden);
        }
        let id = db::id(grant.try_get("id")?)?;
        let consumed: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT command_receipt_id FROM app.operator_command_consumptions WHERE grant_id=$1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?;
        match (&previous, consumed) {
            (Some(previous), Some(receipt)) if receipt == previous.id.as_uuid() => {}
            (None, None) => {
                let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
                    .fetch_one(&mut **tx)
                    .await?;
                if grant.try_get::<DateTime<Utc>, _>("expires_at")? <= now {
                    return Err(StoreError::Forbidden);
                }
            }
            _ => return Err(StoreError::Conflict),
        }
        Some(id)
    } else {
        None
    };
    Ok(Prepared {
        target,
        scope,
        operation: operation.code(),
        key: idempotency_key.into(),
        request,
        grant: grant_id,
        replay: previous.map(|p| p.response),
    })
}

pub(crate) async fn finish<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    prepared: Prepared,
    resource: T,
    status: i32,
) -> Result<CommandResult<T>, StoreError> {
    if prepared.replay.is_some() {
        return Err(StoreError::Integrity);
    }
    let result = CommandResult {
        schema_version: SchemaV1,
        replayed: false,
        resource,
    };
    let receipt:uuid::Uuid=sqlx::query_scalar("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(prepared.scope).bind(prepared.operation).bind(prepared.key).bind(prepared.request)
        .bind(prepared.target.as_uuid()).bind(status).bind(db::json(&result)?).fetch_one(&mut **tx).await?;
    if let Some(grant) = prepared.grant {
        sqlx::query("INSERT INTO app.operator_command_consumptions(grant_id,command_receipt_id,operation,target_id) VALUES($1,$2,$3,$4)")
            .bind(grant.as_uuid()).bind(receipt).bind(prepared.operation).bind(prepared.target.as_uuid()).execute(&mut **tx).await?;
    }
    Ok(result)
}

impl Store {
    /// The adapter has cryptographically verified the code against this native
    /// snapshot; this transaction consumes its exact step and binds one command.
    pub async fn issue_operator_grant(
        &self,
        actor: &Actor,
        idempotency_key: &str,
        command: &OperatorCommand,
        requested_target: Option<Id>,
        snapshot: &auth::AuthSnapshot,
        verified_step: i64,
    ) -> Result<CommandResult<OperatorGrantView>, StoreError> {
        key(idempotency_key)?;
        domain::control::command(command)?;
        let operation = command.operation();
        if operation.creates() != requested_target.is_none() {
            return Err(StoreError::Invalid("operation_target"));
        }
        let request = json!({"schema_version":1,"command":command,"target_id":requested_target});
        let mut tx = self.pool.begin().await?;
        let epoch:i64=sqlx::query_scalar("SELECT session_epoch::bigint FROM app.operator_auth_state WHERE singleton AND initialized FOR UPDATE")
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::AuthenticationRequired)?;
        let machine = authority::machine(&mut tx, actor, true).await?;
        if machine.kind != PrincipalKind::Cli {
            return Err(StoreError::Forbidden);
        }
        let scope = format!("CREDENTIAL:{}", machine.credential_id);
        if let Some(receipt) = prior(
            &mut tx,
            &scope,
            "OPERATOR_GRANT_ISSUE",
            idempotency_key,
            &request,
        )
        .await?
        {
            let mut result: CommandResult<OperatorGrantView> =
                serde_json::from_value(receipt.response).map_err(|_| StoreError::Integrity)?;
            if result.resource.auth_epoch.get() != epoch as u64 {
                return Err(StoreError::Forbidden);
            }
            result.replayed = true;
            tx.commit().await?;
            return Ok(result);
        }
        let (epoch, now) = auth::consume_step(&mut tx, snapshot, verified_step).await?;
        let expires_at = std::cmp::min(now + Duration::seconds(300), machine.expires_at);
        if expires_at <= now {
            return Err(StoreError::InvalidCredentials);
        }
        let target = requested_target.unwrap_or_default();
        let grant = Id::new();
        sqlx::query("INSERT INTO app.operator_command_grants(id,credential_id,operation,target_id,auth_epoch,authenticated_at,expires_at,normalized_nonsecret_request) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(grant.as_uuid()).bind(machine.credential_id.as_uuid()).bind(operation.code()).bind(target.as_uuid()).bind(epoch)
            .bind(now).bind(expires_at).bind(command.normalized_request().map_err(|_|StoreError::Integrity)?)
            .execute(&mut *tx).await?;
        let result = finish(
            &mut tx,
            Prepared {
                scope,
                operation: "OPERATOR_GRANT_ISSUE",
                key: idempotency_key.into(),
                request,
                target: grant,
                grant: None,
                replay: None,
            },
            OperatorGrantView {
                id: grant,
                credential_id: machine.credential_id,
                operation,
                target_id: target,
                auth_epoch: db::revision(epoch)?,
                authenticated_at: now,
                expires_at,
            },
            201,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }
}
