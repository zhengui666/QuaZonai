//! Operator project/identity commands and scoped project reads. No generic table
//! CRUD, external SQL, browser bypass or secret material crosses this boundary.
use crate::{
    authority::{self, Actor},
    commands, db, Store, StoreError,
};
use chrono::{DateTime, Utc};
use contracts::{control::*, runs::ProjectState, Id, SchemaV1};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

const PROJECT:&str="id,root_lineage_id,name,description,state,current_brief_id,current_automation_policy_id,created_by,archived_at,created_at,updated_at,revision";
const PRINCIPAL:&str="id,name,kind,project_id,downstream_id,run_id,enabled,credential_epoch,created_at,updated_at,revision";
const CREDENTIAL:&str="c.id,c.principal_id,c.public_token_id,c.principal_epoch,c.scope_codes,c.issued_at,c.expires_at,(SELECT min(r.effective_at) FROM app.machine_credential_revocations r WHERE r.credential_id=c.id AND r.effective_at<=clock_timestamp()) AS revoked_at";
fn project(row: &PgRow) -> Result<ProjectView, StoreError> {
    Ok(ProjectView {
        id: db::id(row.try_get("id")?)?,
        root_lineage_id: db::id(row.try_get("root_lineage_id")?)?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        state: db::enum_value(row, "state")?,
        current_brief_id: db::optional_id(row, "current_brief_id")?,
        current_automation_policy_id: db::optional_id(row, "current_automation_policy_id")?,
        created_by: db::enum_value(row, "created_by")?,
        archived_at: row.try_get("archived_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        revision: db::revision(row.try_get("revision")?)?,
    })
}
fn principal(row: &PgRow) -> Result<PrincipalView, StoreError> {
    Ok(PrincipalView {
        id: db::id(row.try_get("id")?)?,
        name: row.try_get("name")?,
        kind: db::enum_value(row, "kind")?,
        project_id: db::optional_id(row, "project_id")?,
        downstream_id: db::optional_id(row, "downstream_id")?,
        run_id: db::optional_id(row, "run_id")?,
        enabled: row.try_get("enabled")?,
        credential_epoch: db::revision(row.try_get("credential_epoch")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        revision: db::revision(row.try_get("revision")?)?,
    })
}
fn credential(row: &PgRow) -> Result<CredentialView, StoreError> {
    let scopes: Vec<String> = row.try_get("scope_codes")?;
    Ok(CredentialView {
        id: db::id(row.try_get("id")?)?,
        principal_id: db::id(row.try_get("principal_id")?)?,
        public_token_id: Id::try_from(row.try_get::<String, _>("public_token_id")?)
            .map_err(|_| StoreError::Integrity)?,
        principal_epoch: db::revision(row.try_get("principal_epoch")?)?,
        scope_codes: scopes
            .into_iter()
            .map(|c| serde_json::from_value(serde_json::Value::String(c)))
            .collect::<Result<_, _>>()
            .map_err(|_| StoreError::Integrity)?,
        issued_at: row.try_get("issued_at")?,
        expires_at: row.try_get("expires_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}
async fn locked_principal(
    tx: &mut Transaction<'_, Postgres>,
    id: Id,
) -> Result<PrincipalView, StoreError> {
    let bindings = sqlx::query("SELECT project_id,run_id FROM app.machine_principals WHERE id=$1")
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    if let Some(project) = db::optional_id(&bindings, "project_id")? {
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR SHARE")
            .bind(project.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
    }
    if let Some(run) = db::optional_id(&bindings, "run_id")? {
        sqlx::query("SELECT id FROM app.runs WHERE id=$1 FOR SHARE")
            .bind(run.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
    }
    let row = sqlx::query(&format!(
        "SELECT {PRINCIPAL} FROM app.machine_principals WHERE id=$1 FOR UPDATE"
    ))
    .bind(id.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    principal(&row)
}
pub(crate) fn page<T>(mut items: Vec<T>, limit: u16, id: impl Fn(&T) -> Id) -> Page<T> {
    let more = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    let next_cursor = if more { items.last().map(id) } else { None };
    Page {
        schema_version: SchemaV1,
        items,
        next_cursor,
    }
}

impl Store {
    pub async fn projects(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<ProjectView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let allowed = match actor {
            Actor::Browser { .. } => {
                authority::browser(&mut tx, actor, false, false).await?;
                None
            }
            Actor::Machine { .. } => {
                let a = authority::machine(&mut tx, actor, false).await?;
                a.requires(MachineScope::ResearchRead)?;
                Some(a.project_id.ok_or(StoreError::Forbidden)?.as_uuid())
            }
        };
        let rows=sqlx::query(&format!("SELECT {PROJECT} FROM app.projects WHERE ($1::uuid IS NULL OR id=$1) AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3"))
            .bind(allowed).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(project).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |p| p.id,
        );
        tx.commit().await?;
        Ok(result)
    }
    pub async fn project(&self, actor: &Actor, id: Id) -> Result<ProjectView, StoreError> {
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, id, MachineScope::ResearchRead).await?;
        let row = sqlx::query(&format!("SELECT {PROJECT} FROM app.projects WHERE id=$1"))
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let result = project(&row)?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn create_project(
        &self,
        actor: &Actor,
        key: &str,
        request: &ProjectCreate,
    ) -> Result<CommandResult<ProjectView>, StoreError> {
        domain::control::command(&OperatorCommand::ProjectCreate(request.clone()))?;
        let mut tx = self.pool.begin().await?;
        let command = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ProjectCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = command.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let parent = if let Some(project) = request.fork_from_project_id {
            Some(
                sqlx::query_scalar::<_, uuid::Uuid>(
                    "SELECT root_lineage_id FROM app.projects WHERE id=$1 FOR SHARE",
                )
                .bind(project.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?,
            )
        } else {
            None
        };
        let lineage = Id::new();
        sqlx::query("INSERT INTO app.research_lineages(id,origin,parent_lineage_id,reason) VALUES($1,$2,$3,$4)")
            .bind(lineage.as_uuid()).bind(if parent.is_some(){"FORK"}else{"NEW"}).bind(parent)
            .bind(if parent.is_some(){"Operator-created fork; parent evidence lineage is retained"}else{"Operator-created research project"})
            .execute(&mut *tx).await?;
        let row=sqlx::query(&format!("INSERT INTO app.projects(id,root_lineage_id,name,description,state,created_by) VALUES($1,$2,$3,$4,'DRAFT','OPERATOR') RETURNING {PROJECT}"))
            .bind(command.target.as_uuid()).bind(lineage.as_uuid()).bind(&request.name).bind(&request.description).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, command, project(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn update_project(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &ProjectUpdate,
    ) -> Result<CommandResult<ProjectView>, StoreError> {
        domain::control::command(&OperatorCommand::ProjectUpdate(request.clone()))?;
        let mut tx = self.pool.begin().await?;
        let command = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ProjectUpdate,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = command.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let old = sqlx::query(&format!(
            "SELECT {PROJECT} FROM app.projects WHERE id=$1 FOR UPDATE"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let current = project(&old)?;
        if current.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                current: current.revision,
            });
        }
        if current.state == ProjectState::Archived && request.state != ProjectState::Archived {
            return Err(domain::DomainError::InvalidTransition.into());
        }
        if request.state == ProjectState::Active {
            let frozen:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.research_briefs WHERE id=$1 AND project_id=$2 AND state='FROZEN')")
                .bind(current.current_brief_id.map(Id::as_uuid)).bind(id.as_uuid()).fetch_one(&mut *tx).await?;
            if !frozen {
                return Err(domain::DomainError::AdmissionClosed.into());
            }
        }
        if request.state == ProjectState::Archived {
            let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.runs WHERE project_id=$1 AND state NOT IN ('SUCCEEDED','FAILED','CANCELLED'))")
                .bind(id.as_uuid()).fetch_one(&mut *tx).await?;
            if active {
                return Err(domain::DomainError::InvalidTransition.into());
            }
        }
        let row=sqlx::query(&format!("UPDATE app.projects SET name=$2,description=$3,state=$4,archived_at=CASE WHEN $4='ARCHIVED' THEN coalesce(archived_at,clock_timestamp()) ELSE NULL END WHERE id=$1 AND revision=$5 RETURNING {PROJECT}"))
            .bind(id.as_uuid()).bind(&request.name).bind(&request.description).bind(db::code(&request.state)?).bind(request.expected_revision.get() as i64)
            .fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, command, project(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn principals(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<PrincipalView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        authority::browser(&mut tx, actor, false, false).await?;
        let rows=sqlx::query(&format!("SELECT {PRINCIPAL} FROM app.machine_principals WHERE ($1::uuid IS NULL OR id<$1) ORDER BY id DESC LIMIT $2"))
            .bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(principal).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |p| p.id,
        );
        tx.commit().await?;
        Ok(result)
    }
    pub async fn create_principal(
        &self,
        actor: &Actor,
        key: &str,
        request: &PrincipalCreate,
    ) -> Result<CommandResult<PrincipalView>, StoreError> {
        domain::control::principal(request)?;
        let mut tx = self.pool.begin().await?;
        let command = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::PrincipalCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = command.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        if let Some(project) = request.project_id {
            sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR SHARE")
                .bind(project.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        }
        if let Some(downstream) = request.downstream_id {
            sqlx::query("SELECT id FROM app.downstream_integrations WHERE id=$1 FOR SHARE")
                .bind(downstream.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        }
        let row=sqlx::query(&format!("INSERT INTO app.machine_principals(id,name,kind,project_id,downstream_id,enabled,credential_epoch) VALUES($1,$2,$3,$4,$5,$6,1) RETURNING {PRINCIPAL}"))
            .bind(command.target.as_uuid()).bind(&request.name).bind(db::code(&request.kind)?).bind(request.project_id.map(Id::as_uuid))
            .bind(request.downstream_id.map(Id::as_uuid)).bind(request.enabled).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, command, principal(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn update_principal(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &PrincipalUpdate,
    ) -> Result<CommandResult<PrincipalView>, StoreError> {
        domain::control::name(&request.name)?;
        let mut tx = self.pool.begin().await?;
        let command = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::PrincipalUpdate,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = command.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let old = locked_principal(&mut tx, id).await?;
        if old.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                current: old.revision,
            });
        }
        let epoch = (old.credential_epoch.get() as i64)
            .checked_add(i64::from(old.enabled != request.enabled))
            .ok_or(StoreError::Conflict)?;
        let row=sqlx::query(&format!("UPDATE app.machine_principals SET name=$2,enabled=$3,credential_epoch=$4 WHERE id=$1 AND revision=$5 RETURNING {PRINCIPAL}"))
            .bind(id.as_uuid()).bind(&request.name).bind(request.enabled).bind(epoch).bind(request.expected_revision.get() as i64).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, command, principal(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn credentials(
        &self,
        actor: &Actor,
        principal_id: Id,
        query: &ListQuery,
    ) -> Result<Page<CredentialView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        authority::browser(&mut tx, actor, false, false).await?;
        sqlx::query("SELECT id FROM app.machine_principals WHERE id=$1")
            .bind(principal_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows=sqlx::query(&format!("SELECT {CREDENTIAL} FROM app.machine_credentials c WHERE c.principal_id=$1 AND ($2::uuid IS NULL OR c.id<$2) ORDER BY c.id DESC LIMIT $3"))
            .bind(principal_id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(credential).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |c| c.id,
        );
        tx.commit().await?;
        Ok(result)
    }
    /// Holds the existing Operator command lock until publication or rollback.
    /// Callers must generate a verifier only for the New branch, while this
    /// transaction is alive. Reconciliation uses the same lock as its barrier.
    pub async fn prepare_credential_issuance(
        &self,
        actor: &Actor,
        key: &str,
        principal_id: Id,
        request: &CredentialIssue,
    ) -> Result<CredentialPreparation, StoreError> {
        domain::control::scopes(request)?;
        let mut tx = self.pool.begin().await?;
        let intent = CredentialIssueIntent {
            schema_version: SchemaV1,
            principal_id,
            request: request.clone(),
        };
        let command = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::CredentialIssue,
            key,
            None,
            db::json(&intent)?,
        )
        .await?;
        if let Some(result) = command.replay()? {
            tx.commit().await?;
            return Ok(CredentialPreparation::Replay(result));
        }
        let principal = validate_issuance(&mut tx, principal_id, request).await?;
        Ok(CredentialPreparation::New(Box::new(CredentialIssuance {
            tx,
            command,
            principal,
            actor: actor.clone(),
            request: request.clone(),
        })))
    }

    /// Trusted local reconciliation only. The lock also waits behind a commit
    /// whose response was lost; a non-reference read without that barrier is
    /// insufficient evidence to delete a verifier. The callback never runs for
    /// an immutable credential reference, including revoked/expired credentials.
    pub async fn reconcile_unpublished_verifier<F, Fut>(
        &self,
        reference: Id,
        remove: F,
    ) -> Result<bool, StoreError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT singleton FROM app.operator_auth_state WHERE singleton FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
        let referenced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.machine_credentials WHERE verifier_ref=$1)",
        )
        .bind(reference.to_string())
        .fetch_one(&mut *tx)
        .await?;
        if !referenced {
            remove().await?;
        }
        tx.commit().await?;
        Ok(!referenced)
    }
    pub async fn revoke_credential(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &CredentialRevoke,
    ) -> Result<CommandResult<CredentialView>, StoreError> {
        domain::control::text(&request.reason, 1, 2000, true)?;
        let mut tx = self.pool.begin().await?;
        let command = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::CredentialRevoke,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = command.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let parent: uuid::Uuid =
            sqlx::query_scalar("SELECT principal_id FROM app.machine_credentials WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        locked_principal(&mut tx, db::id(parent)?).await?;
        sqlx::query("SELECT id FROM app.machine_credentials WHERE id=$1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let already:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.machine_credential_revocations WHERE credential_id=$1 AND effective_at<=clock_timestamp())")
            .bind(id.as_uuid()).fetch_one(&mut *tx).await?;
        if !already {
            sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) VALUES($1,clock_timestamp(),$2)")
                .bind(id.as_uuid()).bind(&request.reason).execute(&mut *tx).await?;
        }
        let row = sqlx::query(&format!(
            "SELECT {CREDENTIAL} FROM app.machine_credentials c WHERE c.id=$1"
        ))
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let result = commands::finish(&mut tx, command, credential(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn validate_issuance(
    tx: &mut Transaction<'_, Postgres>,
    id: Id,
    request: &CredentialIssue,
) -> Result<PrincipalView, StoreError> {
    let p = locked_principal(tx, id).await?;
    // Mission credentials are minted by the trusted dispatcher, never this API.
    if !p.enabled || p.kind == PrincipalKind::Mission {
        return Err(StoreError::Forbidden);
    }
    let delivery = [
        MachineScope::DownstreamClaim,
        MachineScope::DownstreamAck,
        MachineScope::ForwardSubmit,
    ];
    if request.scope_codes.contains(&MachineScope::DoctorRead) {
        if !matches!(p.kind, PrincipalKind::Cli | PrincipalKind::Automation)
            || request.scope_codes.len() != 1
        {
            return Err(StoreError::Forbidden);
        }
    } else if p.project_id.is_none() {
        return Err(StoreError::Invalid("project_required"));
    }
    if p.kind == PrincipalKind::Downstream {
        if !request.scope_codes.iter().all(|s| delivery.contains(s)) {
            return Err(StoreError::Forbidden);
        }
    } else if request.scope_codes.iter().any(|s| delivery.contains(s)) {
        return Err(StoreError::Forbidden);
    }
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if request.expires_at <= now {
        return Err(StoreError::Invalid("credential_expiry"));
    }
    Ok(p)
}

/// One-time verifier materialization after authoritative command ownership.
/// Neither variant is a public wire type.
pub enum CredentialPreparation {
    Replay(CommandResult<CredentialView>),
    New(Box<CredentialIssuance>),
}
pub struct CredentialIssuance {
    tx: Transaction<'static, Postgres>,
    command: commands::Prepared,
    principal: PrincipalView,
    actor: Actor,
    request: CredentialIssue,
}
impl CredentialIssuance {
    pub async fn publish(
        self,
        public_token_id: Id,
        verifier_ref: Id,
    ) -> Result<CommandResult<CredentialView>, StoreError> {
        let Self {
            mut tx,
            command,
            principal,
            actor,
            request,
        } = self;
        // Native crypto/IO is bounded but can cross a time boundary. The held
        // row locks preserve revocation/epoch; check time again before issuance.
        match &actor {
            Actor::Browser { login_id } => {
                crate::auth::lock_login(&mut tx, *login_id, true).await?;
            }
            Actor::Machine { operator_grant, .. } => {
                authority::machine(&mut tx, &actor, true).await?;
                let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.operator_command_grants WHERE id=$1 AND expires_at>clock_timestamp())")
                    .bind(operator_grant.map(Id::as_uuid)).fetch_one(&mut *tx).await?;
                if !valid {
                    return Err(StoreError::Forbidden);
                }
            }
        }
        sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,$3,$4,$5,$6,clock_timestamp(),$7,'OPERATOR')")
            .bind(command.target.as_uuid()).bind(principal.id.as_uuid()).bind(public_token_id.to_string()).bind(verifier_ref.to_string())
            .bind(principal.credential_epoch.get() as i64).bind(request.scope_codes.iter().map(|s|s.code()).collect::<Vec<_>>()).bind(request.expires_at)
            .execute(&mut *tx).await?;
        let row = sqlx::query(&format!(
            "SELECT {CREDENTIAL} FROM app.machine_credentials c WHERE c.id=$1"
        ))
        .bind(command.target.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let result = commands::finish(&mut tx, command, credential(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
