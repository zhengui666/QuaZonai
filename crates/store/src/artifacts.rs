//! Artifact metadata and scoped publication. Native file I/O is performed by the
//! trusted entrypoint while this transaction retains its authority/quota locks.
use crate::{
    authority::{self, Actor},
    commands,
    control::page,
    db, Store, StoreError,
};
use contracts::{
    artifacts::*,
    control::{CommandResult, MachineScope, Page, PrincipalKind},
    lifecycle::JobLimitsV1,
    research::ResearchListQuery,
    DbCounter, Id,
};
use serde_json::json;
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

type Tx = Transaction<'static, Postgres>;
const FIELDS: &str = "id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,byte_count,access_class,origin,created_by,created_at";

fn view(row: &PgRow) -> Result<ArtifactView, StoreError> {
    let count =
        u64::try_from(row.try_get::<i64, _>("byte_count")?).map_err(|_| StoreError::Integrity)?;
    Ok(ArtifactView {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        producer_run_id: db::optional_id(row, "producer_run_id")?,
        producer_attempt_id: db::optional_id(row, "producer_attempt_id")?,
        kind: row.try_get("kind")?,
        media_type: row.try_get("media_type")?,
        schema_name: row.try_get("schema_name")?,
        schema_version: row.try_get("schema_version")?,
        byte_count: DbCounter::new(count).map_err(|_| StoreError::Integrity)?,
        access_class: db::enum_value(row, "access_class")?,
        origin: db::enum_value(row, "origin")?,
        created_by: db::enum_value(row, "created_by")?,
        created_at: row.try_get("created_at")?,
    })
}

struct UploadAuthority {
    scope: String,
    run: Option<Id>,
    attempt: Option<Id>,
    producer: &'static str,
}
async fn upload_authority(
    tx: &mut Tx,
    actor: &Actor,
    project: Id,
) -> Result<UploadAuthority, StoreError> {
    match actor {
        Actor::Browser { .. } => {
            authority::browser(tx, actor, true, true).await?;
            crate::research::project_for_write(tx, project).await?;
            Ok(UploadAuthority {
                scope: "OPERATOR".into(),
                run: None,
                attempt: None,
                producer: "OPERATOR",
            })
        }
        Actor::Machine { .. } => {
            // Obtain write locks in their original order, not SHARE then UPDATE.
            let machine = authority::machine(tx, actor, true).await?;
            if !matches!(
                machine.kind,
                PrincipalKind::Cli | PrincipalKind::Automation | PrincipalKind::Mission
            ) {
                return Err(StoreError::Forbidden);
            }
            machine.requires(MachineScope::ArtifactSubmit)?;
            machine.project(project)?;
            crate::research::project_for_write(tx, project).await?;
            let attempt = if let Some(run) = machine.run_id {
                let id = sqlx::query_scalar::<_, uuid::Uuid>(
                    "SELECT r.active_attempt_id::uuid FROM app.runs r JOIN app.machine_credentials c ON c.id=$3 AND c.issuer_attempt_id=r.active_attempt_id WHERE r.id=$1 AND r.project_id=$2 AND r.active_attempt_id IS NOT NULL",
                )
                .bind(run.as_uuid())
                .bind(project.as_uuid())
                .bind(machine.credential_id.as_uuid())
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(StoreError::InvalidCredentials)?;
                Some(db::id(id)?)
            } else {
                None
            };
            Ok(UploadAuthority {
                scope: format!("CREDENTIAL:{}", machine.credential_id),
                run: machine.run_id,
                attempt,
                producer: if machine.kind == PrincipalKind::Mission {
                    "AGENT"
                } else {
                    "OPERATOR"
                },
            })
        }
    }
}

/// Held across native object publication. Never exposed as an HTTP DTO.
/// An existing object MUST be compared byte-for-byte before completing replay.
pub struct ArtifactUpload {
    tx: Tx,
    prepared: commands::Prepared,
    actor: Actor,
    project: Id,
    kind: ResearchArtifactKind,
    byte_count: DbCounter,
    authority: UploadAuthority,
}
impl ArtifactUpload {
    pub fn id(&self) -> Id {
        self.prepared.target
    }
    pub fn replay(&self) -> Result<Option<CommandResult<ArtifactView>>, StoreError> {
        self.prepared.replay()
    }
    /// Call only after native put succeeds, or exact original bytes match replay.
    /// A commit error has unknown durability; never delete the object's published name.
    pub async fn publish(mut self) -> Result<CommandResult<ArtifactView>, StoreError> {
        // Locks prevent revocation from committing, but wall-clock expiry can still pass.
        let current = upload_authority(&mut self.tx, &self.actor, self.project).await?;
        if current.run != self.authority.run
            || current.attempt != self.authority.attempt
            || current.scope != self.authority.scope
        {
            return Err(StoreError::InvalidCredentials);
        }
        if let Some(result) = self.prepared.replay()? {
            self.tx.commit().await?;
            return Ok(result);
        }
        let id = self.prepared.target;
        let query = format!(
            "INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) \
             VALUES($1,$2,$3,$4,$5,$6,$7,'1','LOCAL',$8,'1',$9,'RESEARCH','SYNTHETIC',$10,'REFERENCED') RETURNING {FIELDS}",
        );
        let row = sqlx::query(&query)
            .bind(id.as_uuid())
            .bind(self.project.as_uuid())
            .bind(self.authority.run.map(Id::as_uuid))
            .bind(self.authority.attempt.map(Id::as_uuid))
            .bind(self.kind.code())
            .bind(self.kind.media_type())
            .bind(self.kind.schema_name())
            .bind(id.to_string())
            .bind(self.byte_count.get() as i64)
            .bind(self.authority.producer)
            .fetch_one(&mut *self.tx)
            .await?;
        let result = commands::finish(&mut self.tx, self.prepared, view(&row)?, 201).await?;
        self.tx.commit().await?;
        Ok(result)
    }
}

/// Native locator stays inside trusted server code, never serialized to a client.
pub struct ArtifactContent {
    pub metadata: ArtifactView,
    pub local_object_id: Id,
}

fn visible(actor: &Actor) -> Vec<&'static str> {
    match actor {
        Actor::Browser { .. } => vec!["RESEARCH", "OPERATOR", "DELIVERY"],
        Actor::Machine { .. } => vec!["RESEARCH"],
    }
}
impl Store {
    pub async fn prepare_artifact_upload(
        &self,
        actor: &Actor,
        key: &str,
        request: &ArtifactCreate,
    ) -> Result<ArtifactUpload, StoreError> {
        commands::key(key)?;
        let byte_count = domain::artifacts::upload(request)?;
        let mut tx = self.pool.begin().await?;
        let authority = upload_authority(&mut tx, actor, request.project_id).await?;
        let prepared = commands::artifact_submit(&mut tx, authority.scope.clone(), key, json!({
            "schema_version":1,"project_id":request.project_id,"kind":request.kind,"byte_count":byte_count,
        })).await?;
        if let Some(replay) = prepared.replay::<ArtifactView>()? {
            let original: (String, String, String) = sqlx::query_as(
                "SELECT storage_backend,storage_object_ref,storage_version FROM app.artifacts WHERE id=$1 AND project_id=$2",
            ).bind(prepared.target.as_uuid()).bind(request.project_id.as_uuid()).fetch_one(&mut *tx).await?;
            if original != ("LOCAL".into(), prepared.target.to_string(), "1".into())
                || replay.resource.byte_count != byte_count
                || replay.resource.id != prepared.target
                || replay.resource.access_class != ArtifactAccess::Research
                || replay.resource.origin != contracts::research::DataOrigin::Synthetic
            {
                return Err(StoreError::Integrity);
            }
        } else if let Some(run) = authority.run {
            let limits: serde_json::Value = sqlx::query_scalar(
                "SELECT limits FROM app.run_admissions WHERE run_id=$1 AND project_id=$2",
            )
            .bind(run.as_uuid())
            .bind(request.project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::Invalid("run_has_no_admission"))?;
            let limits: JobLimitsV1 =
                serde_json::from_value(limits).map_err(|_| StoreError::Integrity)?;
            let used: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(byte_count),0)::bigint FROM app.artifacts WHERE producer_run_id=$1",
            ).bind(run.as_uuid()).fetch_one(&mut *tx).await?;
            if used < 0
                || (used as u64)
                    .checked_add(byte_count.get())
                    .is_none_or(|total| total > limits.output_bytes.get())
            {
                return Err(domain::DomainError::BudgetExhausted("artifact_output_bytes").into());
            }
        }
        Ok(ArtifactUpload {
            tx,
            prepared,
            actor: actor.clone(),
            project: request.project_id,
            kind: request.kind,
            byte_count,
            authority,
        })
    }

    pub async fn artifacts(
        &self,
        actor: &Actor,
        query: &ResearchListQuery,
    ) -> Result<Page<ArtifactView>, StoreError> {
        if !(1..=100).contains(&query.limit) {
            return Err(StoreError::Invalid("limit"));
        }
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, query.project_id, MachineScope::ResearchRead)
            .await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.projects WHERE id=$1)")
                .bind(query.project_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query(&format!(
            "SELECT {FIELDS} FROM app.artifacts WHERE project_id=$1 AND access_class=ANY($2) AND ($3::uuid IS NULL OR id<$3) ORDER BY id DESC LIMIT $4",
        )).bind(query.project_id.as_uuid()).bind(visible(actor)).bind(query.cursor.map(Id::as_uuid))
            .bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(page(items, query.limit, |v| v.id))
    }

    pub async fn artifact(&self, actor: &Actor, id: Id) -> Result<ArtifactView, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = authorized_row(&mut tx, actor, id).await?;
        let result = view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn artifact_content(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<ArtifactContent, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = authorized_row(&mut tx, actor, id).await?;
        if row.try_get::<String, _>("storage_backend")? != "LOCAL"
            || row.try_get::<String, _>("storage_version")? != "1"
        {
            return Err(
                domain::DomainError::CapabilityUnavailable("artifact_content_backend").into(),
            );
        }
        let local_object_id = Id::try_from(row.try_get::<String, _>("storage_object_ref")?)
            .map_err(|_| StoreError::Integrity)?;
        // LOCAL objects in this layout are generated keys, not arbitrary paths or aliases.
        if local_object_id != id {
            return Err(StoreError::Integrity);
        }
        let metadata = view(&row)?;
        tx.commit().await?;
        Ok(ArtifactContent {
            metadata,
            local_object_id,
        })
    }
}

async fn authorized_row(tx: &mut Tx, actor: &Actor, id: Id) -> Result<PgRow, StoreError> {
    let row = sqlx::query(&format!("SELECT {FIELDS},storage_backend,storage_object_ref,storage_version FROM app.artifacts WHERE id=$1 AND project_id IS NOT NULL AND access_class=ANY($2)"))
        .bind(id.as_uuid()).bind(visible(actor)).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    let project = db::id(row.try_get("project_id")?)?;
    authority::read_project(tx, actor, project, MachineScope::ResearchRead).await?;
    Ok(row)
}
