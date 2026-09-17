//! Only original accepted native quality output can back this historical assumption.
use crate::{db, StoreError};
use contracts::{
    execution::{NativeDataQualityReportV1, NativeDatasetSelectionV1, NativeTaskParametersV1},
    research::{ArtifactInputRole, DataOrigin, DataPartition},
    runtime_jobs::{JobSpecV1, ResultManifestV1, RuntimeInputV1},
    DbCounter, Id,
};
use sqlx::{Postgres, Row, Transaction};
#[cfg(test)]
#[path = "../../tests/support/bar_liquidity_source.rs"]
mod tests;

pub(crate) fn expiry(
    values: &[contracts::execution::NativeBarNotionalV1],
    maximum_age_seconds: u32,
) -> Result<chrono::DateTime<chrono::Utc>, StoreError> {
    let first = values
        .iter()
        .map(|v| v.event_ns.get())
        .min()
        .ok_or(StoreError::Integrity)?;
    let end = first
        .checked_add(u64::from(maximum_age_seconds) * 1_000_000_000)
        .ok_or(StoreError::Invalid("bar_liquidity_expiry"))?;
    // PostgreSQL Time is microsecond precision; never round expiry into the future.
    chrono::DateTime::from_timestamp_micros((end / 1000) as i64)
        .ok_or(StoreError::Invalid("bar_liquidity_expiry"))
}

pub(crate) async fn document<R, Read>(
    tx: &mut Transaction<'_, Postgres>,
    project: Id,
    id: Id,
    schema: &str,
    maximum: usize,
    read: &mut R,
) -> Result<Vec<u8>, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND schema_name=$3 AND schema_version='1' AND media_type='application/json' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1' AND access_class='RESEARCH'")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(schema).fetch_optional(&mut **tx).await?.ok_or(StoreError::Integrity)?;
    if size <= 0 || size as u64 > maximum as u64 {
        return Err(StoreError::Integrity);
    }
    let bytes = read(
        id,
        DbCounter::new(size as u64).map_err(|_| StoreError::Integrity)?,
    )
    .await?;
    if bytes.len() as i64 != size {
        return Err(StoreError::Integrity);
    }
    Ok(bytes)
}

pub(crate) async fn original<R, Read>(
    tx: &mut Transaction<'_, Postgres>,
    project: Id,
    runtime: Id,
    input_set: Id,
    selection: &NativeDatasetSelectionV1,
    report: Id,
    read: &mut R,
) -> Result<(NativeDataQualityReportV1, RuntimeInputV1, DataOrigin), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    crate::research::revalidate_frozen_inputs(tx, input_set, project, runtime).await?;
    let row = sqlx::query("SELECT r.id AS run_id,r.active_attempt_id,a.created_at,a.result_manifest_artifact_id,n.spec_json,t.parameters_artifact_id,o.remote_storage_ref,f.origin FROM app.artifacts f JOIN app.run_native_outputs o ON o.artifact_id=f.id AND o.attempt_id=f.producer_attempt_id JOIN app.runs r ON r.id=f.producer_run_id AND r.active_attempt_id=o.attempt_id JOIN app.run_attempts a ON a.id=o.attempt_id AND a.run_id=r.id AND a.dispatch_state='TERMINAL' AND a.accepted_at IS NOT NULL JOIN app.run_terminal_receipts terminal ON terminal.run_id=r.id AND terminal.attempt_id=a.id AND terminal.terminal_state='SUCCEEDED' JOIN app.run_native_attempts n ON n.attempt_id=a.id AND n.run_id=r.id JOIN app.run_native_tasks t ON t.run_id=r.id JOIN app.artifacts manifest ON manifest.id=a.result_manifest_artifact_id AND manifest.producer_run_id=r.id AND manifest.producer_attempt_id=a.id WHERE f.id=$1 AND f.project_id=$2 AND r.project_id=$2 AND r.input_set_id=$3 AND r.kind='DATA_VALIDATE' AND r.state='SUCCEEDED' AND f.kind='DATA_QUALITY' AND f.schema_name='qz.data_quality' AND f.schema_version='1' AND f.origin=t.origin AND f.access_class='RESEARCH'")
        .bind(report.as_uuid()).bind(project.as_uuid()).bind(input_set.as_uuid())
        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("bar_liquidity_native_source"))?;
    let run = db::id(row.try_get("run_id")?)?;
    let attempt = db::id(row.try_get("active_attempt_id")?)?;
    let spec: JobSpecV1 =
        serde_json::from_value(row.try_get("spec_json")?).map_err(|_| StoreError::Integrity)?;
    let parameter = db::id(row.try_get("parameters_artifact_id")?)?;
    if spec.run_id != run
        || spec.input_set_id != input_set
        || spec.parameters_artifact_id != parameter
    {
        return Err(StoreError::Integrity);
    }
    let bound_runtime: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT runtime_id FROM app.run_attempts WHERE id=$1 AND run_id=$2")
            .bind(attempt.as_uuid())
            .bind(run.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
    if bound_runtime != Some(runtime.as_uuid()) {
        return Err(StoreError::Invalid("bar_liquidity_runtime"));
    }
    let parameters: NativeTaskParametersV1 = serde_json::from_slice(
        &document(
            tx,
            project,
            parameter,
            "qz.native_task",
            8 * 1024 * 1024,
            read,
        )
        .await?,
    )
    .map_err(|_| StoreError::Integrity)?;
    domain::execution::task(&spec, &parameters).map_err(|_| StoreError::Integrity)?;
    let NativeTaskParametersV1::ValidateData { selections, .. } = &parameters else {
        return Err(StoreError::Integrity);
    };
    if !selections.iter().any(|value| {
        value.dataset_revision_id == selection.dataset_revision_id
            && value.selection == selection.selection
    }) || spec.inputs.iter().any(|input| {
        matches!(
            input,
            RuntimeInputV1::Dataset {
                role: DataPartition::Sealed,
                ..
            }
        )
    }) {
        return Err(StoreError::Invalid("bar_liquidity_selection"));
    }
    let manifest: ResultManifestV1 = serde_json::from_slice(
        &document(
            tx,
            project,
            db::id(row.try_get("result_manifest_artifact_id")?)?,
            "qz.job_result",
            domain::runtime_jobs::MAX_RESULT_MANIFEST_BYTES,
            read,
        )
        .await?,
    )
    .map_err(|_| StoreError::Integrity)?;
    let now = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    domain::runtime_jobs::manifest(&manifest, &spec, row.try_get("created_at")?, now)
        .map_err(|_| StoreError::Integrity)?;
    if manifest.state != contracts::runtime_jobs::RuntimeResultState::Succeeded {
        return Err(StoreError::Integrity);
    }
    if manifest
        .engine_versions
        .get("bar-notional")
        .map(String::as_str)
        != Some("1")
    {
        return Err(
            domain::DomainError::CapabilityUnavailable("bar_liquidity_native_source").into(),
        );
    }
    let [output] = manifest.artifacts.as_slice() else {
        return Err(StoreError::Integrity);
    };
    if output.storage_ref.as_uuid() != row.try_get::<uuid::Uuid, _>("remote_storage_ref")? {
        return Err(StoreError::Integrity);
    }
    let bytes = document(
        tx,
        project,
        report,
        "qz.data_quality",
        contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
        read,
    )
    .await?;
    domain::execution::output_bindings(
        &parameters,
        None,
        manifest.started_at.ok_or(StoreError::Integrity)?,
        manifest.finished_at,
        &[(output.clone(), bytes.clone())],
    )
    .map_err(|_| StoreError::Integrity)?;
    let value: NativeDataQualityReportV1 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    Ok((
        value,
        RuntimeInputV1::Artifact {
            artifact_id: report,
            storage_version: "1".into(),
            byte_count: output.byte_count,
            role: ArtifactInputRole::DataQuality,
        },
        db::enum_value(&row, "origin")?,
    ))
}

pub(crate) struct FrozenLiquidity {
    pub binding: contracts::science::NativePortfolioLiquidityV1,
    pub report: NativeDataQualityReportV1,
    pub input: RuntimeInputV1,
    pub origin: DataOrigin,
}

/// Declared policy bytes, not a measured snapshot or a market-origin contribution.
pub(crate) async fn rolling<R, Read>(
    tx: &mut Transaction<'_, Postgres>,
    project: Id,
    runtime: Id,
    assumptions: Id,
    read: &mut R,
) -> Result<
    Option<(
        contracts::science::NativeRollingBarLiquidityPolicyV1,
        RuntimeInputV1,
    )>,
    StoreError,
>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{} WHERE e.id=$1 AND s.project_id=$2 AND s.runtime_id=$3",
        super::VIEW
    )))
    .bind(assumptions.as_uuid())
    .bind(project.as_uuid())
    .bind(runtime.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::Integrity)?;
    let saved = super::view(&row)?;
    let Some(policy) = saved.rolling_liquidity else {
        return Ok(None);
    };
    let id = saved
        .rolling_liquidity_artifact_id
        .ok_or(StoreError::Integrity)?;
    let declared: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 AND kind='PARAMETERS' AND origin='SYNTHETIC' AND created_by='OPERATOR')")
        .bind(id.as_uuid()).fetch_one(&mut **tx).await?;
    if !declared {
        return Err(StoreError::Integrity);
    }
    let bytes = document(
        tx,
        project,
        id,
        "qz.rolling_bar_liquidity",
        1024 * 1024,
        read,
    )
    .await?;
    let original: contracts::science::NativeRollingBarLiquidityPolicyV1 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    if original != policy {
        return Err(StoreError::Integrity);
    }
    crate::research::revalidate_frozen_inputs(tx, saved.input_set_id, project, runtime).await?;
    Ok(Some((
        policy,
        RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: DbCounter::new(bytes.len() as u64).map_err(|_| StoreError::Integrity)?,
            role: ArtifactInputRole::Parameters,
        },
    )))
}

/// Reread immutable source facts; current expiry is separate from corruption.
pub(crate) async fn frozen<R, Read>(
    tx: &mut Transaction<'_, Postgres>,
    project: Id,
    runtime: Id,
    assumptions: Id,
    read: &mut R,
) -> Result<Option<FrozenLiquidity>, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{} WHERE e.id=$1 AND s.project_id=$2 AND s.runtime_id=$3",
        super::VIEW
    )))
    .bind(assumptions.as_uuid())
    .bind(project.as_uuid())
    .bind(runtime.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::Integrity)?;
    let saved = super::view(&row)?;
    if saved.rolling_liquidity.is_some() {
        return Err(StoreError::Invalid("rolling_liquidity_requires_study"));
    }
    let Some(assumption) = saved.bar_liquidity else {
        return if saved.bar_liquidity_valid_until.is_none() {
            Ok(None)
        } else {
            Err(StoreError::Integrity)
        };
    };
    let source = crate::data_validation::dataset_bindings(
        tx,
        saved.input_set_id,
        project,
        runtime,
        &[
            contracts::research::InputPurpose::Discovery,
            contracts::research::InputPurpose::Validation,
        ],
        read,
    )
    .await?
    .into_iter()
    .find(|b| b.selection.dataset_revision_id == saved.dataset_revision_id)
    .ok_or(StoreError::Integrity)?;
    let (report, input, origin) = original(
        tx,
        project,
        runtime,
        saved.input_set_id,
        &source.selection,
        assumption.report_artifact_id,
        read,
    )
    .await
    .map_err(|error| match error {
        StoreError::Invalid(
            "bar_liquidity_native_source" | "bar_liquidity_runtime" | "bar_liquidity_selection",
        )
        | StoreError::Domain(domain::DomainError::CapabilityUnavailable(
            "bar_liquidity_native_source",
        )) => StoreError::Integrity,
        other => other,
    })?;
    let quality = report
        .datasets
        .iter()
        .find(|q| q.dataset_revision_id == saved.dataset_revision_id)
        .ok_or(StoreError::Integrity)?;
    let values = domain::portfolio::bar_liquidity_values(
        &assumption,
        quality,
        &saved.settings.base_currency,
        source.selection.selection.decision_cutoff_ns,
    )
    .map_err(|_| StoreError::Integrity)?;
    let until =
        expiry(values, assumption.maximum_age_seconds).map_err(|_| StoreError::Integrity)?;
    if saved.bar_liquidity_valid_until != Some(until) {
        return Err(StoreError::Integrity);
    }
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if until <= now {
        return Err(StoreError::Invalid("bar_liquidity_expired"));
    }
    Ok(Some(FrozenLiquidity {
        binding: contracts::science::NativePortfolioLiquidityV1 {
            schema_version: contracts::SchemaV1,
            assumption,
            source: source.selection,
        },
        report,
        input,
        origin: crate::data_validation::combine_origin(origin, source.origin),
    }))
}
