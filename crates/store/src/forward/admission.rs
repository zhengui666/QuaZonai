//! Trusted original feedback admission. No public DTO, arbitrary runtime or research trial.
use super::*;
use crate::lifecycle::{
    native::{bind_task, NativeTaskDefinition},
    StandaloneRunSubmission,
};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    artifacts::ArtifactAccess,
    execution::NativeTaskParametersV1,
    lifecycle::JobLimitsV1,
    research::{ArtifactInputRole, DataOrigin, InputItemV1, InputPurpose, InputSetCreate},
    runs::{RunKind, RunSnapshotV1},
    runtime_jobs::RuntimeInputV1,
    DbCounter, SchemaV1,
};

pub(crate) fn limits() -> JobLimitsV1 {
    JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: DbCounter::new(30).expect("fixed native limit"),
        wall_seconds: 60,
        memory_mib: 512,
        output_bytes: DbCounter::new(1024 * 1024).expect("fixed native limit"),
    }
}
async fn header(
    tx: &mut Transaction<'_, Postgres>,
    handoff: Id,
) -> Result<sqlx::postgres::PgRow, StoreError> {
    sqlx::query("SELECT h.release_id,h.downstream_id,c.project_id,c.mandate_id,a.runtime_id FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.run_admissions a ON a.run_id=c.run_id JOIN app.handoff_transfers transfer ON transfer.handoff_id=h.id AND transfer.downstream_id=h.downstream_id AND transfer.external_claim_id=h.external_claim_id AND transfer.claimed_at=h.claimed_at WHERE h.id=$1 AND h.state IN ('CLAIMED','ACKNOWLEDGED') AND r.environment='REAL' AND transfer.provenance='RECORDED_TRANSITION'")
        .bind(handoff.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)
}
async fn current_policy(
    tx: &mut Transaction<'_, Postgres>,
    row: &sqlx::postgres::PgRow,
) -> Result<(contracts::delivery::AutomationPolicyViewV1, DateTime<Utc>), StoreError> {
    let project = db::id(row.try_get("project_id")?)?;
    let id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT current_automation_policy_id FROM app.projects WHERE id=$1 FOR UPDATE",
    )
    .bind(project.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    crate::automation::active_policy(
        tx,
        db::id(id.ok_or(StoreError::Invalid("forward_policy_required"))?)?,
        project,
        db::id(row.try_get("mandate_id")?)?,
        db::id(row.try_get("downstream_id")?)?,
    )
    .await
}

/// Only RunKind::ForwardEvaluate dispatch calls this. Generic research input permissions stay unchanged.
pub(crate) async fn revalidate(
    tx: &mut Transaction<'_, Postgres>,
    input: Id,
    project: Id,
    runtime: Id,
) -> Result<(), StoreError> {
    let row=sqlx::query("SELECT f.*,i.frozen_at,i.purpose FROM app.forward_evaluation_inputs f JOIN app.input_sets i ON i.id=f.input_set_id AND i.project_id=f.project_id WHERE f.input_set_id=$1 AND f.project_id=$2 AND f.runtime_id=$3")
        .bind(input.as_uuid()).bind(project.as_uuid()).bind(runtime.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("forward_native_input_required"))?;
    if row
        .try_get::<Option<DateTime<Utc>>, _>("frozen_at")?
        .is_none()
        || row.try_get::<String, _>("purpose")? != "FORWARD"
    {
        return Err(StoreError::Integrity);
    }
    let NativeTaskParametersV1::EvaluateForward { request, .. } =
        serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?
    else {
        return Err(StoreError::Integrity);
    };
    domain::forward::evaluation::request(&request)?;
    let handoff = db::id(row.try_get("handoff_id")?)?;
    if handoff != request.window.handoff_id {
        return Err(StoreError::Integrity);
    }
    let original = header(tx, handoff).await?;
    if db::id(original.try_get("project_id")?)? != project
        || db::id(original.try_get("runtime_id")?)? != runtime
    {
        return Err(StoreError::Integrity);
    }
    let (policy, _) = current_policy(tx, &original).await?;
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if policy.id != db::id(row.try_get("policy_id")?)?
        || now >= row.try_get::<DateTime<Utc>, _>("valid_until")?
    {
        return Err(StoreError::Invalid("forward_policy_expired_or_replaced"));
    }
    let actual: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.forward_messages WHERE handoff_id=$1 AND stream_id=$2 ORDER BY sequence,message_revision")
        .bind(handoff.as_uuid()).bind(&request.window.stream_id).fetch_all(&mut **tx).await?;
    if actual
        != request
            .sources
            .iter()
            .map(|s| s.id.as_uuid())
            .collect::<Vec<_>>()
    {
        return Err(StoreError::Invalid("forward_sources_changed"));
    }
    let members: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT artifact_id FROM app.input_set_items WHERE input_set_id=$1 AND role='REPORT' ORDER BY ordinal")
        .bind(input.as_uuid()).fetch_all(&mut **tx).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.input_set_items WHERE input_set_id=$1")
            .bind(input.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
    if members
        != request
            .sources
            .iter()
            .map(|s| s.report_artifact_id.as_uuid())
            .collect::<Vec<_>>()
        || count != members.len() as i64
    {
        return Err(StoreError::Integrity);
    }
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='PARAMETERS' AND schema_name='qz.native_task' AND schema_version='1' AND access_class='EVALUATOR_ONLY')")
        .bind(row.try_get::<uuid::Uuid,_>("parameters_artifact_id")?).bind(project.as_uuid()).fetch_one(&mut **tx).await?;
    if !valid {
        return Err(StoreError::Integrity);
    }
    Ok(())
}

impl Store {
    /// Native Worker only. No caller-selected limits, image, runtime, body or source artifact.
    pub async fn enqueue_forward_evaluation<R, Read, P, Published>(
        &self,
        handoff: Id,
        stream: &str,
        mut read: R,
        mut publish: P,
    ) -> Result<CommandResult<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::control::text(stream, 1, 200, false)?;
        let mut tx = self.pool.begin().await?;
        let initial = header(&mut tx, handoff).await?;
        let project = db::id(initial.try_get("project_id")?)?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(project.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let original = header(&mut tx, handoff).await?;
        let source_ids: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.forward_messages WHERE handoff_id=$1 AND stream_id=$2 ORDER BY sequence,message_revision LIMIT 256")
            .bind(handoff.as_uuid()).bind(stream).fetch_all(&mut *tx).await?;
        if source_ids.len() > 255 {
            return Err(
                domain::DomainError::CapabilityUnavailable("forward_native_source_limit").into(),
            );
        }
        // Immutable original IDs identify a replay without rereading protected bytes.
        if let Some(run)=sqlx::query_scalar::<_,serde_json::Value>("SELECT a.initial_snapshot FROM app.forward_evaluation_inputs f JOIN app.runs r ON r.input_set_id=f.input_set_id AND r.kind='FORWARD_EVALUATE' JOIN app.run_admissions a ON a.run_id=r.id WHERE f.handoff_id=$1 AND f.request->'request'->'window'->>'stream_id'=$2 AND (SELECT array_agg((s.value->>'id')::uuid ORDER BY s.ordinal) FROM jsonb_array_elements(f.request->'request'->'sources') WITH ORDINALITY AS s(value,ordinal))=$3::uuid[]")
            .bind(handoff.as_uuid()).bind(stream).bind(&source_ids).fetch_optional(&mut *tx).await? {
            let resource=serde_json::from_value(run).map_err(|_| StoreError::Integrity)?;
            tx.commit().await?;
            return Ok(CommandResult {schema_version:SchemaV1,replayed:true,resource});
        }
        let (policy, policy_until) = current_policy(&mut tx, &original).await?;
        let request = window::load(&mut tx, handoff, stream, &mut read).await?;
        domain::forward::evaluation::request(&request)?;
        let end = request.window.window_end.ok_or(StoreError::Integrity)?;
        // Positive ages beyond chrono's range still expire at the finite policy deadline.
        let feedback_until = i64::try_from(policy.content.max_feedback_age_seconds.get())
            .ok()
            .and_then(Duration::try_seconds)
            .and_then(|age| end.checked_add_signed(age))
            .unwrap_or(policy_until);
        let until = feedback_until.min(policy_until);
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if until <= now {
            return Err(StoreError::Invalid("forward_feedback_stale"));
        }
        let runtime = db::id(original.try_get("runtime_id")?)?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM app.runtime_integrations WHERE id=$1 FOR SHARE",
        )
        .bind(runtime.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let revision = db::revision(revision)?;
        let limits = limits();
        let capabilities = crate::runtime::require_capabilities(
            &mut tx,
            runtime,
            revision,
            RunKind::ForwardEvaluate,
        )
        .await?;
        domain::runtime::job_limits(&capabilities, &limits)?;
        let parameters = NativeTaskParametersV1::EvaluateForward {
            schema_version: SchemaV1,
            request: Box::new(request.clone()),
        };
        let schemas = parameters.output_schemas();
        if !schemas.iter().all(|s| {
            capabilities
                .artifact_schemas
                .iter()
                .any(|a| a.name == s.name && a.version == s.version)
        }) {
            return Err(StoreError::Invalid("forward_output_capability"));
        }
        let image = capabilities
            .image_refs
            .iter()
            .find(|i| i.job_kind == RunKind::ForwardEvaluate)
            .ok_or(StoreError::Invalid("forward_image_capability"))?
            .image_ref
            .clone();
        let capability: uuid::Uuid = sqlx::query_scalar(
            "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
        )
        .bind(runtime.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let input = Id::new();
        let parameter = Id::new();
        let bytes = serde_json::to_vec(&parameters).map_err(|_| StoreError::Integrity)?;
        let size = bytes.len() as u64;
        let mut bindings = Vec::new();
        for source in &request.sources {
            let row=sqlx::query("SELECT byte_count,storage_version FROM app.artifacts WHERE id=$1 AND project_id=$2 AND kind='REPORT' AND schema_name='qz.forward_report' AND schema_version='1' AND access_class='EVALUATOR_ONLY' AND origin='REAL' AND storage_backend='LOCAL' AND storage_object_ref=id::text")
                .bind(source.report_artifact_id.as_uuid()).bind(project.as_uuid()).fetch_one(&mut *tx).await?;
            bindings.push(RuntimeInputV1::Artifact {
                artifact_id: source.report_artifact_id,
                storage_version: row.try_get("storage_version")?,
                byte_count: DbCounter::new(row.try_get::<i64, _>("byte_count")? as u64)
                    .map_err(|_| StoreError::Integrity)?,
                role: ArtifactInputRole::Report,
            });
        }
        publish(NativeObjectPublication {
            id: parameter,
            bytes,
        })
        .await?;
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_task','1','LOCAL',$3,'1',$4,'EVALUATOR_ONLY','REAL','OPERATOR','AUDIT')")
            .bind(parameter.as_uuid()).bind(project.as_uuid()).bind(parameter.to_string()).bind(size as i64).execute(&mut *tx).await?;
        crate::research::insert_frozen_input(
            &mut tx,
            input,
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: project,
                purpose: InputPurpose::Forward,
                decision_cutoff: request.window.window_end.ok_or(StoreError::Integrity)?,
                items: request
                    .sources
                    .iter()
                    .map(|s| InputItemV1::Artifact {
                        artifact_id: s.report_artifact_id,
                        role: ArtifactInputRole::Report,
                    })
                    .collect(),
            },
        )
        .await?;
        sqlx::query("INSERT INTO app.forward_evaluation_inputs(input_set_id,project_id,handoff_id,policy_id,runtime_id,parameters_artifact_id,request,valid_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(input.as_uuid()).bind(project.as_uuid()).bind(handoff.as_uuid()).bind(policy.id.as_uuid()).bind(runtime.as_uuid()).bind(parameter.as_uuid()).bind(db::json(&parameters)?).bind(until).execute(&mut *tx).await?;
        let submission = StandaloneRunSubmission {
            project_id: project,
            input_set_id: input,
            runtime_id: runtime,
            runtime_revision: revision,
            kind: RunKind::ForwardEvaluate,
            limits,
            max_parallel_runs: 2,
        };
        let (mut tx, result) = Self::enqueue_standalone_run_in_transaction(
            tx,
            &format!("forward/{input}"),
            &submission,
        )
        .await?;
        bindings.push(RuntimeInputV1::Artifact {
            artifact_id: parameter,
            storage_version: "1".into(),
            byte_count: DbCounter::new(size).map_err(|_| StoreError::Integrity)?,
            role: ArtifactInputRole::Parameters,
        });
        bind_task(
            &mut tx,
            &result.resource,
            NativeTaskDefinition {
                parameters_artifact_id: parameter,
                inputs: bindings,
                image_ref: image,
                cpu: 1,
                capability_snapshot_artifact_id: db::id(capability)?,
                output_schemas: schemas,
                origin: DataOrigin::Real,
                access: ArtifactAccess::EvaluatorOnly,
            },
        )
        .await?;
        revalidate(&mut tx, input, project, runtime).await?;
        tx.commit().await?;
        Ok(result)
    }
}
