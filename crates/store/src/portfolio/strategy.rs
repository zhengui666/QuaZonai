//! Immutable strategy projections; summary reads select the accepted native report.
use super::*;
use contracts::{strategy_portfolio::*, SchemaV1};

mod create;
mod summary;

pub(crate) fn mandate(row: &PgRow) -> Result<StrategyMandateViewV1, StoreError> {
    if row.try_get::<String, _>("allocation_method")? != "FIXED_TARGET_WEIGHTS" {
        return Err(StoreError::Invalid("strategy_mandate_required"));
    }
    let content: StrategyMandateContentV1 =
        serde_json::from_value(row.try_get("strategy_content")?)
            .map_err(|_| StoreError::Integrity)?;
    if content.base_currency != row.try_get::<String, _>("base_currency")?
        || *content.capital_assumption.as_decimal()
            != row.try_get::<bigdecimal::BigDecimal, _>("capital_assumption")?
        || content.universe_version_id != db::id(row.try_get("universe_version_id")?)?
        || content.execution_assumptions_id != db::id(row.try_get("execution_assumptions_id")?)?
        || *content.exposure_tolerance.as_decimal()
            != row.try_get::<bigdecimal::BigDecimal, _>("exposure_tolerance")?
        || db::json(&content.constraints)? != row.try_get::<serde_json::Value, _>("constraints")?
    {
        return Err(StoreError::Integrity);
    }
    Ok(StrategyMandateViewV1 {
        schema_version: SchemaV1,
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        version: u32::try_from(row.try_get::<i32, _>("version")?)
            .map_err(|_| StoreError::Integrity)?,
        content,
        created_at: row.try_get("created_at")?,
    })
}

fn mandate_envelope(row: &PgRow) -> Result<MandateViewEnvelopeV2, StoreError> {
    match row.try_get::<String, _>("allocation_method")?.as_str() {
        "NATIVE_OPTIMIZER" => super::view(row)
            .map(Box::new)
            .map(MandateViewEnvelopeV2::Forecast),
        "FIXED_TARGET_WEIGHTS" => mandate(row)
            .map(Box::new)
            .map(MandateViewEnvelopeV2::Strategy),
        _ => Err(StoreError::Integrity),
    }
}

// The row is published and comes from the same producer-bound query as legacy
// candidates. Validate the materialized projection against its relational keys.
// PostgreSQL stores microseconds; the projection retains original decision nanos.
fn candidate(row: &PgRow) -> Result<StrategyPortfolioCandidateV1, StoreError> {
    let result: StrategyPortfolioCandidateV1 =
        serde_json::from_value(row.try_get("strategy_detail")?)
            .map_err(|_| StoreError::Integrity)?;
    let purpose = match &result.purpose {
        StrategyPortfolioPurposeV1::HistoricalReplay {} => "HISTORICAL_REPLAY",
        StrategyPortfolioPurposeV1::CurrentDecision { .. } => "CURRENT_DECISION",
    };
    if row.try_get::<String, _>("source_kind")? != "STRATEGY_ALPHA"
        || row.try_get::<String, _>("purpose")? != purpose
        || result.id != db::id(row.try_get("id")?)?
        || result.project_id != db::id(row.try_get("project_id")?)?
        || result.mandate_id != db::id(row.try_get("mandate_id")?)?
        || result.input_set_id != db::id(row.try_get("input_set_id")?)?
        || result.run_id != db::id(row.try_get("run_id")?)?
        || result.report_artifact_id != db::id(row.try_get("diagnostics_artifact_id")?)?
        || result.decision_asof.timestamp_micros()
            != row
                .try_get::<chrono::DateTime<chrono::Utc>, _>("decision_asof")?
                .timestamp_micros()
        || result.created_at != row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?
        || *result.cash_weight.as_decimal()
            != row.try_get::<bigdecimal::BigDecimal, _>("cash_weight")?
        || result.accepted_attempt_id != db::id(row.try_get("producer_attempt_id")?)?
        || result.run_id != db::id(row.try_get("producer_run_id")?)?
        || result.accepted_attempt_id != db::id(row.try_get("active_attempt_id")?)?
        || row.try_get::<String, _>("execution_status")? != "SUCCEEDED"
        || row.try_get::<String, _>("report_schema_name")? != "qz.strategy_portfolio"
        || row.try_get::<String, _>("report_schema_version")? != "1"
        || !row.try_get::<bool, _>("strategy_producer_valid")?
    {
        return Err(StoreError::Integrity);
    }
    Ok(result)
}

const CANDIDATE: &str = "SELECT c.*,r.state AS execution_status,r.active_attempt_id,a.origin,a.producer_run_id,a.producer_attempt_id,a.schema_name AS report_schema_name,a.schema_version AS report_schema_version,EXISTS(SELECT 1 FROM app.run_attempts accepted JOIN app.run_terminal_receipts terminal ON terminal.run_id=accepted.run_id AND terminal.attempt_id=accepted.id AND terminal.terminal_state='SUCCEEDED' JOIN app.run_native_outputs output ON output.attempt_id=accepted.id AND output.artifact_id=c.diagnostics_artifact_id WHERE accepted.id=r.active_attempt_id AND accepted.run_id=r.id AND accepted.dispatch_state='TERMINAL' AND accepted.accepted_at IS NOT NULL) AS strategy_producer_valid FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id JOIN app.runs r ON r.id=c.run_id AND r.project_id=c.project_id JOIN app.artifacts a ON a.id=c.diagnostics_artifact_id AND a.project_id=c.project_id";

pub(crate) async fn snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Id,
) -> Result<StrategyPortfolioCandidateV1, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{CANDIDATE} WHERE c.id=$1 AND c.source_kind='STRATEGY_ALPHA'"
    )))
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::NotFound)?;
    candidate(&row)
}

impl Store {
    pub async fn mandate_envelope(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<MandateViewEnvelopeV2, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(row.try_get("project_id")?)?).await?;
        let result = mandate_envelope(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn mandates_envelope(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<MandateViewEnvelopeV2>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(mandate_envelope)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(page(items, query.limit, |v| match v {
            MandateViewEnvelopeV2::Forecast(v) => v.id,
            MandateViewEnvelopeV2::Strategy(v) => v.id,
        }))
    }

    pub async fn candidate_envelope(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<PortfolioCandidateEnvelopeV2, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!("{CANDIDATE} WHERE c.id=$1")))
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(row.try_get("project_id")?)?).await?;
        let result = match row.try_get::<String, _>("source_kind")?.as_str() {
            "FORECAST" => PortfolioCandidateEnvelopeV2::Forecast(
                super::candidates::snapshot(&mut tx, id).await?,
            ),
            "STRATEGY_ALPHA" => PortfolioCandidateEnvelopeV2::Strategy(candidate(&row)?),
            _ => return Err(StoreError::Integrity),
        };
        tx.commit().await?;
        Ok(result)
    }

    pub async fn candidates_envelope(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<PortfolioCandidateListEnvelopeV2>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{CANDIDATE} WHERE c.project_id=$1 AND ($2::uuid IS NULL OR c.id<$2) ORDER BY c.id DESC LIMIT $3")))
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(
                |row| match row.try_get::<String, _>("source_kind")?.as_str() {
                    "FORECAST" => super::candidates::candidate(row)
                        .map(PortfolioCandidateListEnvelopeV2::Forecast),
                    "STRATEGY_ALPHA" => {
                        candidate(row).map(PortfolioCandidateListEnvelopeV2::Strategy)
                    }
                    _ => Err(StoreError::Integrity),
                },
            )
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(page(items, query.limit, |v| match v {
            PortfolioCandidateListEnvelopeV2::Forecast(v) => v.id,
            PortfolioCandidateListEnvelopeV2::Strategy(v) => v.id,
        }))
    }
}
