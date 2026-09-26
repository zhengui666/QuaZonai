//! Native original reports remain evaluator-only; receipts expose references only.
use super::*;
use contracts::{
    control::{ListQuery, Page},
    DbCounter, SchemaV1,
};
use sqlx::postgres::PgRow;

const MESSAGE: &str = "SELECT m.*,c.project_id,h.release_id,a.byte_count FROM app.forward_messages m JOIN app.handoff_offers h ON h.id=m.handoff_id JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.artifacts a ON a.id=m.report_artifact_id";
pub(super) fn view(row: &PgRow) -> Result<ForwardMessageViewV1, StoreError> {
    Ok(ForwardMessageViewV1 {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        release_id: db::id(row.try_get("release_id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        handoff_id: db::id(row.try_get("handoff_id")?)?,
        external_message_id: row.try_get("external_message_id")?,
        stream_id: row.try_get("stream_id")?,
        sequence: DbCounter::try_from(row.try_get::<i64, _>("sequence")?.to_string())
            .map_err(|_| StoreError::Integrity)?,
        message_revision: u32::try_from(row.try_get::<i32, _>("message_revision")?)
            .map_err(|_| StoreError::Integrity)?,
        supersedes_message_id: db::optional_id(row, "supersedes_message_id")?,
        window_start: row.try_get("window_start")?,
        window_end: row.try_get("window_end")?,
        coverage_status: db::enum_value(row, "coverage_status")?,
        observation_count: DbCounter::try_from(
            row.try_get::<i64, _>("observation_count")?.to_string(),
        )
        .map_err(|_| StoreError::Integrity)?,
        report_artifact_id: db::id(row.try_get("report_artifact_id")?)?,
        issued_at: row.try_get("issued_at")?,
        received_at: row.try_get("received_at")?,
    })
}
async fn issuer(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    project: Id,
) -> Result<Id, StoreError> {
    let machine = authority::machine(tx, actor, true).await?;
    machine.project(project)?;
    if machine.kind != PrincipalKind::Downstream {
        return Err(StoreError::Forbidden);
    }
    machine.requires(MachineScope::ForwardSubmit)?;
    machine.downstream_id.ok_or(StoreError::Forbidden)
}
pub(super) async fn read_authority(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    project: Id,
) -> Result<Option<Id>, StoreError> {
    Ok(match actor {
        Actor::Browser { .. } | Actor::OwnerDevice { .. } => {
            authority::browser(tx, actor, false).await?;
            None
        }
        Actor::Machine { .. } => {
            let machine = authority::machine(tx, actor, false).await?;
            machine.project(project)?;
            match machine.kind {
                PrincipalKind::Cli => {
                    machine.requires(MachineScope::ResearchRead)?;
                    None
                }
                PrincipalKind::Downstream => {
                    machine.requires(MachineScope::ForwardSubmit)?;
                    Some(machine.downstream_id.ok_or(StoreError::Forbidden)?)
                }
                _ => return Err(StoreError::Forbidden),
            }
        }
    })
}
impl Store {
    pub async fn forward_messages(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<ForwardMessageViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let downstream = read_authority(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows=sqlx::query(sqlx::AssertSqlSafe(format!("{MESSAGE} WHERE c.project_id=$1 AND ($2::uuid IS NULL OR m.downstream_id=$2) AND ($3::uuid IS NULL OR m.id<$3) ORDER BY m.id DESC LIMIT $4")))
            .bind(project.as_uuid()).bind(downstream.map(Id::as_uuid)).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| v.id))
    }
    pub async fn submit_forward_message<R, Read, P, Published>(
        &self,
        actor: &Actor,
        request: &ForwardMessageSubmitV1,
        mut read: R,
        publish: P,
    ) -> Result<CommandResult<ForwardMessageViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::forward::message(request)?;
        commands::key(&request.external_message_id)?;
        let report = &request.report;
        let mut tx = self.pool.begin().await?;
        let downstream = issuer(&mut tx, actor, report.project_id).await?;
        // Same project barrier as Claim/ACK and uncertain-publication cleanup; archival is not deletion of feedback.
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(report.project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let down = sqlx::query(
            "SELECT enabled,environments FROM app.downstream_integrations WHERE id=$1 FOR UPDATE",
        )
        .bind(downstream.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let handoff=sqlx::query("SELECT h.*,r.environment AS release_environment,c.project_id FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE h.id=$1 AND h.downstream_id=$2 AND c.project_id=$3 FOR SHARE OF h")
            .bind(report.handoff_id.as_uuid()).bind(downstream.as_uuid()).bind(report.project_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if handoff
            .try_get::<Option<String>, _>("external_claim_id")?
            .as_deref()
            != Some(report.external_claim_id.as_str())
            || handoff.try_get::<String, _>("release_environment")? != "REAL"
        {
            return Err(StoreError::Invalid("forward_transfer"));
        }
        let original = ForwardReportV1 {
            schema_version: SchemaV1,
            downstream_id: downstream,
            release_id: db::id(handoff.try_get("release_id")?)?,
            environment: db::enum_value(&handoff, "environment")?,
            content: report.clone(),
        };
        let bytes = serde_json::to_vec(&original).map_err(|_| StoreError::Integrity)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(StoreError::Invalid("forward_report_size"));
        }
        let scope = format!("DOWNSTREAM:{downstream}");
        let alias:Option<uuid::Uuid>=sqlx::query_scalar("SELECT resource_id FROM app.command_receipts WHERE principal_scope=$1 AND operation='FORWARD_SUBMIT' AND idempotency_key=$2")
            .bind(&scope).bind(&request.external_message_id).fetch_optional(&mut *tx).await?;
        let existing = if let Some(alias) = alias {
            Some(
                sqlx::query(sqlx::AssertSqlSafe(format!("{MESSAGE} WHERE m.id=$1")))
                    .bind(alias)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(StoreError::Integrity)?,
            )
        } else {
            let external = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{MESSAGE} WHERE m.downstream_id=$1 AND m.external_message_id=$2"
            )))
            .bind(downstream.as_uuid())
            .bind(&request.external_message_id)
            .fetch_optional(&mut *tx)
            .await?;
            if external.is_some() {
                external
            } else {
                sqlx::query(sqlx::AssertSqlSafe(format!("{MESSAGE} WHERE m.handoff_id=$1 AND m.stream_id=$2 AND m.sequence=$3 AND m.message_revision=$4")))
                    .bind(report.handoff_id.as_uuid()).bind(&report.stream_id).bind(report.sequence.get() as i64).bind(report.message_revision as i32).fetch_optional(&mut *tx).await?
            }
        };
        let id = existing
            .as_ref()
            .map(|r| db::id(r.try_get("id")?))
            .transpose()?
            .unwrap_or_default();
        let artifact = existing
            .as_ref()
            .map(|r| db::id(r.try_get("report_artifact_id")?))
            .transpose()?
            .unwrap_or_default();
        if let Some(row) = &existing {
            let size = DbCounter::try_from(row.try_get::<i64, _>("byte_count")?.to_string())
                .map_err(|_| StoreError::Integrity)?;
            if read(artifact, size).await? != bytes {
                return Err(StoreError::IdempotencyConflict);
            }
        }
        let prepared=commands::handoff_command(&mut tx,scope,"FORWARD_SUBMIT",&request.external_message_id,id,serde_json::json!({"schema_version":1,"project_id":report.project_id,"handoff_id":report.handoff_id,"report_artifact_id":artifact})).await?;
        if let Some(result) = prepared.replay()? {
            issuer(&mut tx, actor, report.project_id).await?;
            tx.commit().await?;
            return Ok(result);
        }
        if let Some(row) = existing {
            let mut result = commands::finish(&mut tx, prepared, view(&row)?, 201).await?;
            issuer(&mut tx, actor, report.project_id).await?;
            tx.commit().await?;
            result.replayed = true;
            return Ok(result);
        }
        let state: String = handoff.try_get("state")?;
        let environment: String = handoff.try_get("environment")?;
        let enabled_environments: String = down.try_get("environments")?;
        if !matches!(state.as_str(), "CLAIMED" | "ACKNOWLEDGED")
            || !down.try_get::<bool, _>("enabled")?
            || (enabled_environments != "BOTH" && enabled_environments != environment)
        {
            return Err(StoreError::Invalid("forward_transfer"));
        }
        let claimed: chrono::DateTime<chrono::Utc> = handoff
            .try_get::<Option<_>, _>("claimed_at")?
            .ok_or(StoreError::Invalid("forward_transfer"))?;
        let latest=sqlx::query("SELECT id,message_revision,window_start,window_end FROM app.forward_messages WHERE handoff_id=$1 AND stream_id=$2 AND sequence=$3 ORDER BY message_revision DESC LIMIT 1")
            .bind(report.handoff_id.as_uuid()).bind(&report.stream_id).bind(report.sequence.get() as i64).fetch_optional(&mut *tx).await?;
        if let Some(latest) = latest {
            if report.supersedes_message_id != Some(db::id(latest.try_get("id")?)?)
                || i64::from(report.message_revision)
                    != i64::from(latest.try_get::<i32, _>("message_revision")?) + 1
                || report.window_start
                    != latest.try_get::<chrono::DateTime<chrono::Utc>, _>("window_start")?
                || report.window_end
                    != latest.try_get::<chrono::DateTime<chrono::Utc>, _>("window_end")?
            {
                return Err(StoreError::Conflict);
            }
        } else if report.message_revision != 1 || report.supersedes_message_id.is_some() {
            return Err(StoreError::Conflict);
        }
        let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if report.window_start < claimed
            || report.window_end > now
            || report.issued_at > now + chrono::Duration::seconds(5)
        {
            return Err(StoreError::Invalid("forward_report_time"));
        }
        let size = bytes.len() as i64;
        publish(NativeObjectPublication {
            id: artifact,
            bytes,
        })
        .await?;
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.forward_report','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','REAL','IMPORT','AUDIT')")
            .bind(artifact.as_uuid()).bind(report.project_id.as_uuid()).bind(artifact.to_string()).bind(size).execute(&mut *tx).await?;
        let coverage = if report.supersedes_message_id.is_some() {
            ForwardCoverageV1::Correction
        } else if report.complete {
            ForwardCoverageV1::Complete
        } else {
            ForwardCoverageV1::Partial
        };
        sqlx::query("INSERT INTO app.forward_messages(id,downstream_id,external_message_id,handoff_id,stream_id,sequence,message_revision,supersedes_message_id,window_start,window_end,coverage_status,observation_count,report_artifact_id,issued_at,received_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,clock_timestamp())")
            .bind(id.as_uuid()).bind(downstream.as_uuid()).bind(&request.external_message_id).bind(report.handoff_id.as_uuid()).bind(&report.stream_id).bind(report.sequence.get() as i64).bind(report.message_revision as i32).bind(report.supersedes_message_id.map(Id::as_uuid)).bind(report.window_start).bind(report.window_end).bind(db::code(&coverage)?).bind(report.returns.iter().filter(|p|p.value.is_some()).count() as i64).bind(artifact.as_uuid()).bind(report.issued_at).execute(&mut *tx).await?;
        if issuer(&mut tx, actor, report.project_id).await? != downstream {
            return Err(StoreError::Forbidden);
        }
        let row = sqlx::query(sqlx::AssertSqlSafe(format!("{MESSAGE} WHERE m.id=$1")))
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let result = commands::finish(&mut tx, prepared, view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
