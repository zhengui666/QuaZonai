//! Trusted entrypoint facts, never deserialized from an HTTP/CLI/MCP body.
//! Native crypto verifies possession; these locked reads establish authority.
use crate::{auth, db, Store, StoreError};
use chrono::{DateTime, Utc};
use contracts::{control::*, Id};
use sqlx::{Postgres, Row, Transaction};

#[derive(Clone)]
pub enum Actor {
    Browser {
        login_id: Id,
    },
    Machine {
        credential_id: Id,
        verifier_ref: Id,
        operator_grant: Option<Id>,
    },
}
// No Debug: a verifier reference belongs only to the trusted secret adapter.
#[derive(Clone)]
pub struct MachineChallenge {
    pub credential_id: Id,
    pub public_token_id: Id,
    pub verifier_ref: Id,
}
impl MachineChallenge {
    /// Call only after verify_capability on this exact verifier returned true.
    pub fn verified_actor(&self, operator_grant: Option<Id>) -> Actor {
        Actor::Machine {
            credential_id: self.credential_id,
            verifier_ref: self.verifier_ref,
            operator_grant,
        }
    }
}
pub(crate) struct MachineAuthority {
    pub credential_id: Id,
    pub kind: PrincipalKind,
    pub project_id: Option<Id>,
    pub downstream_id: Option<Id>,
    pub run_id: Option<Id>,
    pub scopes: Vec<MachineScope>,
    pub expires_at: DateTime<Utc>,
}
impl MachineAuthority {
    pub fn requires(&self, scope: MachineScope) -> Result<(), StoreError> {
        if !self.scopes.contains(&scope) {
            return Err(StoreError::Forbidden);
        }
        Ok(())
    }
    pub fn project(&self, id: Id) -> Result<(), StoreError> {
        if self.project_id != Some(id) {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
impl Store {
    pub async fn machine_session(&self, actor: &Actor) -> Result<MachineSessionView, StoreError> {
        let mut tx = self.pool.begin().await?;
        let verified = machine(&mut tx, actor, false).await?;
        let view = MachineSessionView {
            schema_version: contracts::SchemaV1,
            credential_id: verified.credential_id,
            kind: verified.kind,
            project_id: verified.project_id,
            downstream_id: verified.downstream_id,
            run_id: verified.run_id,
            scope_codes: verified.scopes,
            expires_at: verified.expires_at,
        };
        tx.commit().await?;
        Ok(view)
    }

    pub async fn machine_challenge(
        &self,
        public_token_id: Id,
    ) -> Result<MachineChallenge, StoreError> {
        let row=sqlx::query("SELECT c.id,c.verifier_ref FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id WHERE c.public_token_id=$1 AND p.enabled AND c.principal_epoch=p.credential_epoch AND c.expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM app.machine_credential_revocations r WHERE r.credential_id=c.id AND r.effective_at<=clock_timestamp())")
            .bind(public_token_id.to_string()).fetch_optional(&self.pool).await?.ok_or(StoreError::InvalidCredentials)?;
        Ok(MachineChallenge {
            credential_id: db::id(row.try_get("id")?)?,
            public_token_id,
            verifier_ref: Id::try_from(row.try_get::<String, _>("verifier_ref")?)
                .map_err(|_| StoreError::InvalidCredentials)?,
        })
    }
}

pub(crate) async fn browser(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    recent: bool,
    write: bool,
) -> Result<(), StoreError> {
    let Actor::Browser { login_id } = actor else {
        return Err(StoreError::Forbidden);
    };
    let sql = if write {
        "SELECT id FROM app.operator_auth_state WHERE singleton AND initialized FOR UPDATE"
    } else {
        "SELECT id FROM app.operator_auth_state WHERE singleton AND initialized FOR SHARE"
    };
    if sqlx::query(sql).fetch_optional(&mut **tx).await?.is_none() {
        return Err(StoreError::AuthenticationRequired);
    }
    auth::lock_login(tx, *login_id, recent).await?;
    Ok(())
}

pub(crate) async fn machine(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    write: bool,
) -> Result<MachineAuthority, StoreError> {
    let Actor::Machine {
        credential_id,
        verifier_ref,
        ..
    } = actor
    else {
        return Err(StoreError::Forbidden);
    };
    // Bindings are immutable. This lookup establishes lock order, not authority.
    let bindings=sqlx::query("SELECT p.id,p.project_id,p.run_id,p.kind FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id WHERE c.id=$1 AND c.verifier_ref=$2")
        .bind(credential_id.as_uuid()).bind(verifier_ref.to_string()).fetch_optional(&mut **tx).await?.ok_or(StoreError::InvalidCredentials)?;
    let principal_id: uuid::Uuid = bindings.try_get("id")?;
    let project = db::optional_id(&bindings, "project_id")?;
    let run = db::optional_id(&bindings, "run_id")?;
    let kind: PrincipalKind = db::enum_value(&bindings, "kind")?;
    let project_active = if let Some(project) = project {
        sqlx::query_scalar::<_, String>("SELECT state FROM app.projects WHERE id=$1 FOR SHARE")
            .bind(project.as_uuid())
            .fetch_one(&mut **tx)
            .await?
            == "ACTIVE"
    } else {
        false
    };
    let mission = if kind == PrincipalKind::Mission {
        let run = run.ok_or(StoreError::InvalidCredentials)?;
        Some(
            sqlx::query(
                "SELECT state,deadline_at FROM app.runs WHERE id=$1 AND project_id=$2 FOR SHARE",
            )
            .bind(run.as_uuid())
            .bind(project.map(Id::as_uuid))
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::InvalidCredentials)?,
        )
    } else {
        None
    };
    let principal=sqlx::query(if write {"SELECT enabled,credential_epoch,downstream_id FROM app.machine_principals WHERE id=$1 FOR UPDATE"}else{"SELECT enabled,credential_epoch,downstream_id FROM app.machine_principals WHERE id=$1 FOR SHARE"})
        .bind(principal_id).fetch_one(&mut **tx).await?;
    let credential=sqlx::query(if write {"SELECT principal_epoch,scope_codes,issued_at,expires_at FROM app.machine_credentials WHERE id=$1 FOR UPDATE"}else{"SELECT principal_epoch,scope_codes,issued_at,expires_at FROM app.machine_credentials WHERE id=$1 FOR SHARE"})
        .bind(credential_id.as_uuid()).fetch_one(&mut **tx).await?;
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let expires: DateTime<Utc> = credential.try_get("expires_at")?;
    let revoked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.machine_credential_revocations WHERE credential_id=$1 AND effective_at<=clock_timestamp())")
        .bind(credential_id.as_uuid()).fetch_one(&mut **tx).await?;
    if !principal.try_get::<bool, _>("enabled")?
        || principal.try_get::<i64, _>("credential_epoch")?
            != credential.try_get::<i64, _>("principal_epoch")?
        || credential.try_get::<DateTime<Utc>, _>("issued_at")? > now
        || expires <= now
        || revoked
    {
        return Err(StoreError::InvalidCredentials);
    }
    if let Some(mission) = mission {
        let state: String = mission.try_get("state")?;
        let deadline: DateTime<Utc> = mission.try_get("deadline_at")?;
        if !project_active
            || !matches!(state.as_str(), "DISPATCHING" | "RUNNING" | "RECONCILING")
            || deadline <= now
            || expires > deadline
        {
            return Err(StoreError::InvalidCredentials);
        }
    }
    let codes: Vec<String> = credential.try_get("scope_codes")?;
    let scopes: Vec<MachineScope> = codes
        .into_iter()
        .map(|code| serde_json::from_value(serde_json::Value::String(code)))
        .collect::<Result<_, _>>()
        .map_err(|_| StoreError::InvalidCredentials)?;
    Ok(MachineAuthority {
        credential_id: *credential_id,
        kind,
        project_id: project,
        run_id: run,
        downstream_id: db::optional_id(&principal, "downstream_id")?,
        scopes,
        expires_at: expires,
    })
}

pub(crate) async fn read_project(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    project: Id,
    scope: MachineScope,
) -> Result<(), StoreError> {
    match actor {
        Actor::Browser { .. } => browser(tx, actor, false, false).await,
        Actor::Machine { .. } => {
            let authority = machine(tx, actor, false).await?;
            authority.requires(scope)?;
            authority.project(project)
        }
    }
}
