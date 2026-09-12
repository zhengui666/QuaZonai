//! Operator projections of existing Alpha/Validation evidence, never a new verdict.
use crate::{
    authority::{self, Actor},
    control::page,
    db, Store, StoreError,
};
use chrono::{DateTime, Utc};
use contracts::{
    control::{ListQuery, MachineScope, Page, PrincipalKind},
    evidence::*,
    research::ResearchListQuery,
    DbCounter, Id, Revision, SchemaV1,
};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
type Tx<'a> = Transaction<'a, Postgres>;

// Native bindings carry dataset provenance. Generated CODE is not market data.
const VERSION: &str = "SELECT v.*,t.origin FROM app.alpha_versions v LEFT JOIN app.experiment_forecasts f ON f.experiment_id=v.experiment_id AND f.model_artifact_id=v.model_artifact_id LEFT JOIN app.run_native_tasks t ON t.run_id=f.run_id";
// No report bytes or storage locators. This public operation does not disclose
// Sealed or unbound historical reports, even to an ordinary research credential.
const EVALUATION: &str = "SELECT ev.*,report.origin,clock_timestamp() AS checked_at
 FROM app.evaluations ev
 JOIN app.evaluation_publications p ON p.evaluation_id=ev.id
 JOIN app.experiment_validations v ON v.run_id=ev.run_id AND v.alpha_version_id=ev.subject_alpha_version_id AND v.policy_id=ev.policy_id
 JOIN app.runs r ON r.id=v.run_id AND r.project_id=ev.project_id AND r.input_set_id=ev.input_set_id AND r.state=ev.execution_status
 JOIN app.input_sets i ON i.id=r.input_set_id AND i.purpose='VALIDATION' AND i.frozen_at IS NOT NULL
 JOIN app.run_terminal_receipts receipt ON receipt.run_id=r.id AND receipt.terminal_state=r.state AND receipt.attempt_id IS NOT DISTINCT FROM r.active_attempt_id
 JOIN app.artifacts report ON report.id=ev.report_artifact_id AND report.id=ev.method_versions_artifact_id AND report.project_id=ev.project_id
   AND report.producer_run_id=r.id AND report.producer_attempt_id IS NOT DISTINCT FROM r.active_attempt_id
   AND report.schema_name='qz.alpha_evaluation' AND report.schema_version='1' AND report.access_class='EVALUATOR_ONLY'
 WHERE ev.evaluation_kind='WALK_FORWARD'";

async fn authorize(tx: &mut Tx<'_>, actor: &Actor, project: Id) -> Result<(), StoreError> {
    match actor {
        Actor::Browser { .. } => authority::browser(tx, actor, false, false).await,
        Actor::Machine { .. } => {
            let current = authority::machine(tx, actor, false).await?;
            if current.kind != PrincipalKind::Cli {
                return Err(StoreError::Forbidden);
            }
            current.requires(MachineScope::ResearchRead)?;
            current.project(project)
        }
    }
}

async fn alpha_project(tx: &mut Tx<'_>, actor: &Actor, alpha: Id) -> Result<Id, StoreError> {
    let project: uuid::Uuid = sqlx::query_scalar("SELECT project_id FROM app.alphas WHERE id=$1")
        .bind(alpha.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let project = db::id(project)?;
    authorize(tx, actor, project).await?;
    Ok(project)
}

fn alpha(row: &PgRow) -> Result<AlphaView, StoreError> {
    let active_version_id = db::optional_id(row, "active_version_id")?;
    let active_version = row
        .try_get::<Option<i32>, _>("active_version")?
        .map(|v| db::revision(i64::from(v)))
        .transpose()?;
    if active_version_id.is_some() != active_version.is_some() {
        return Err(StoreError::Integrity);
    }
    Ok(AlphaView {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        name: row.try_get("name")?,
        lifecycle: db::enum_value(row, "lifecycle")?,
        active_version_id,
        active_version,
        revision: db::revision(row.try_get("revision")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn version(row: &PgRow) -> Result<AlphaVersionView, StoreError> {
    Ok(AlphaVersionView {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        alpha_id: db::id(row.try_get("alpha_id")?)?,
        version: db::revision(i64::from(row.try_get::<i32, _>("version")?))?,
        experiment_id: db::id(row.try_get("experiment_id")?)?,
        root_lineage_id: db::id(row.try_get("root_lineage_id")?)?,
        code_artifact_id: db::id(row.try_get("code_artifact_id")?)?,
        model_artifact_id: db::optional_id(row, "model_artifact_id")?,
        signal_contract_version: row.try_get("signal_contract_version")?,
        signal_kind: db::enum_value(row, "signal_kind")?,
        horizon_kind: db::enum_value(row, "horizon_kind")?,
        horizon_value: row
            .try_get::<Option<i64>, _>("horizon_value")?
            .map(count)
            .transpose()?,
        forecast_unit: db::enum_value(row, "forecast_unit")?,
        calibration_id: db::optional_id(row, "calibration_id")?,
        runtime_image_ref: row.try_get("runtime_image_ref")?,
        origin: row
            .try_get::<Option<String>, _>("origin")?
            .map(|v| {
                serde_json::from_value(serde_json::Value::String(v))
                    .map_err(|_| StoreError::Integrity)
            })
            .transpose()?,
        created_at: row.try_get("created_at")?,
    })
}

fn evaluation(row: &PgRow) -> Result<EvaluationView, StoreError> {
    let checked_at: DateTime<Utc> = row.try_get("checked_at")?;
    let valid_until: Option<DateTime<Utc>> = row.try_get("valid_until")?;
    Ok(EvaluationView {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        subject_alpha_version_id: db::optional_id(row, "subject_alpha_version_id")?,
        subject_candidate_id: db::optional_id(row, "subject_candidate_id")?,
        input_set_id: db::id(row.try_get("input_set_id")?)?,
        policy_id: db::id(row.try_get("policy_id")?)?,
        run_id: db::id(row.try_get("run_id")?)?,
        evaluation_kind: db::enum_value(row, "evaluation_kind")?,
        execution_status: db::enum_value(row, "execution_status")?,
        evidence_status: db::enum_value(row, "evidence_status")?,
        decision: db::enum_value(row, "decision")?,
        report_artifact_id: db::id(row.try_get("report_artifact_id")?)?,
        method_versions_artifact_id: db::id(row.try_get("method_versions_artifact_id")?)?,
        origin: db::enum_value(row, "origin")?,
        concluded_at: row.try_get("concluded_at")?,
        valid_until,
        checked_at,
        unexpired_at_read: valid_until.is_some_and(|until| until > checked_at),
    })
}

fn count(value: i64) -> Result<DbCounter, StoreError> {
    DbCounter::new(u64::try_from(value).map_err(|_| StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)
}

/// Shared with the policy-filtered Mission feedback; this decoder grants no access.
pub(crate) fn metric(row: &PgRow) -> Result<MetricValueV1, StoreError> {
    Ok(MetricValueV1 {
        schema_version: SchemaV1,
        evaluation_id: db::id(row.try_get("evaluation_id")?)?,
        metric_code: row.try_get("metric_code")?,
        scope: row.try_get("scope")?,
        value: row.try_get("value")?,
        status: db::enum_value(row, "status")?,
        reason_code: row.try_get("reason_code")?,
        unit: row.try_get("unit")?,
        period_start: row.try_get("period_start")?,
        period_end: row.try_get("period_end")?,
        observation_count: count(row.try_get("observation_count")?)?,
        frequency: row.try_get("frequency")?,
        annualization_factor: row.try_get("annualization_factor")?,
        method_id: row.try_get("method_id")?,
        method_version: row.try_get("method_version")?,
        source_artifact_id: db::id(row.try_get("source_artifact_id")?)?,
        higher_is_better: row.try_get("higher_is_better")?,
    })
}

async fn read_evaluation(
    tx: &mut Tx<'_>,
    actor: &Actor,
    id: Id,
) -> Result<EvaluationView, StoreError> {
    let project: uuid::Uuid =
        sqlx::query_scalar("SELECT project_id FROM app.evaluations WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    authorize(tx, actor, db::id(project)?).await?;
    let row = sqlx::query(&format!("{EVALUATION} AND ev.id=$1"))
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    evaluation(&row)
}

impl Store {
    pub async fn alphas(
        &self,
        actor: &Actor,
        query: &ResearchListQuery,
    ) -> Result<Page<AlphaView>, StoreError> {
        if !(1..=100).contains(&query.limit) {
            return Err(StoreError::Invalid("limit"));
        }
        let mut tx = self.pool.begin().await?;
        authorize(&mut tx, actor, query.project_id).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR SHARE")
            .bind(query.project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT a.*,v.version AS active_version FROM app.alphas a LEFT JOIN app.alpha_versions v ON v.id=a.active_version_id AND v.alpha_id=a.id WHERE a.project_id=$1 AND ($2::uuid IS NULL OR a.id<$2) ORDER BY a.id DESC LIMIT $3 FOR SHARE OF a")
            .bind(query.project_id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(alpha).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |v| v.id,
        );
        tx.commit().await?;
        Ok(result)
    }

    pub async fn alpha_versions(
        &self,
        actor: &Actor,
        alpha: Id,
        query: &ListQuery,
    ) -> Result<Page<AlphaVersionView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        alpha_project(&mut tx, actor, alpha).await?;
        let rows = sqlx::query(&format!("{VERSION} WHERE v.alpha_id=$1 AND ($2::uuid IS NULL OR v.id<$2) ORDER BY v.id DESC LIMIT $3"))
            .bind(alpha.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(version).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |v| v.id,
        );
        tx.commit().await?;
        Ok(result)
    }

    pub async fn alpha_version(
        &self,
        actor: &Actor,
        alpha: Id,
        number: Revision,
    ) -> Result<AlphaVersionView, StoreError> {
        let mut tx = self.pool.begin().await?;
        alpha_project(&mut tx, actor, alpha).await?;
        let row = sqlx::query(&format!("{VERSION} WHERE v.alpha_id=$1 AND v.version=$2"))
            .bind(alpha.as_uuid())
            .bind(number.get() as i64)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let result = version(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn alpha_evaluations(
        &self,
        actor: &Actor,
        version: Id,
        query: &ListQuery,
    ) -> Result<Page<EvaluationView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.alpha_versions WHERE id=$1")
                .bind(version.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        authorize(&mut tx, actor, db::id(project)?).await?;
        let rows = sqlx::query(&format!("{EVALUATION} AND ev.subject_alpha_version_id=$1 AND ($2::uuid IS NULL OR ev.id<$2) ORDER BY ev.id DESC LIMIT $3"))
            .bind(version.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(evaluation).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |v| v.id,
        );
        tx.commit().await?;
        Ok(result)
    }

    pub async fn evaluation(&self, actor: &Actor, id: Id) -> Result<EvaluationView, StoreError> {
        let mut tx = self.pool.begin().await?;
        let result = read_evaluation(&mut tx, actor, id).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn evaluation_metrics(
        &self,
        actor: &Actor,
        id: Id,
        query: &ListQuery,
    ) -> Result<Page<MetricValueV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_evaluation(&mut tx, actor, id).await?;
        let rows = sqlx::query("SELECT * FROM app.metric_values WHERE evaluation_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter()
                .map(|r| Ok::<_, StoreError>((db::id(r.try_get("id")?)?, metric(r)?)))
                .collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |v| v.0,
        );
        tx.commit().await?;
        Ok(Page {
            schema_version: SchemaV1,
            items: result.items.into_iter().map(|(_, v)| v).collect(),
            next_cursor: result.next_cursor,
        })
    }
}
