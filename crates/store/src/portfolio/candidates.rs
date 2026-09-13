//! Published immutable snapshots only; do not read underlying evaluator files.
use super::*;

const CANDIDATE: &str = "SELECT c.*,r.state AS execution_status,a.origin FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id JOIN app.runs r ON r.id=c.run_id AND r.project_id=c.project_id JOIN app.artifacts a ON a.id=c.diagnostics_artifact_id AND a.project_id=c.project_id";

fn decimal(value: bigdecimal::BigDecimal) -> Result<contracts::DecimalValue, StoreError> {
    value
        .to_plain_string()
        .parse()
        .map_err(|_| StoreError::Integrity)
}

fn candidate(row: &PgRow) -> Result<CandidateViewV1, StoreError> {
    Ok(CandidateViewV1 {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        mandate_id: db::id(row.try_get("mandate_id")?)?,
        input_set_id: db::id(row.try_get("input_set_id")?)?,
        run_id: db::id(row.try_get("run_id")?)?,
        decision_asof: row.try_get("decision_asof")?,
        created_at: row.try_get("created_at")?,
        execution_status: db::enum_value(row, "execution_status")?,
        solver_status: db::enum_value(row, "solver_status")?,
        evidence_status: db::enum_value(row, "evidence_status")?,
        origin: db::enum_value(row, "origin")?,
        reason_code: row.try_get("reason_code")?,
        forecast_artifact_id: db::optional_id(row, "forecast_artifact_id")?,
        covariance_artifact_id: db::optional_id(row, "covariance_artifact_id")?,
        diagnostics_artifact_id: db::id(row.try_get("diagnostics_artifact_id")?)?,
        target_artifact_id: db::optional_id(row, "target_artifact_id")?,
        allocation_evaluation_id: db::optional_id(row, "allocation_evaluation_id")?,
        cash_weight: row
            .try_get::<Option<bigdecimal::BigDecimal>, _>("cash_weight")?
            .map(decimal)
            .transpose()?,
        current_weights_source: db::enum_value(row, "current_weights_source")?,
        current_weights_artifact_id: db::optional_id(row, "current_weights_artifact_id")?,
    })
}

impl Store {
    pub async fn candidates(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<CandidateViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.projects WHERE id=$1)")
                .bind(project.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query(&format!("{CANDIDATE} WHERE c.project_id=$1 AND ($2::uuid IS NULL OR c.id<$2) ORDER BY c.id DESC LIMIT $3"))
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(candidate).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(page(items, query.limit, |v| v.id))
    }

    pub async fn candidate(&self, actor: &Actor, id: Id) -> Result<CandidateDetailV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(&format!("{CANDIDATE} WHERE c.id=$1"))
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(row.try_get("project_id")?)?).await?;
        let header = candidate(&row)?;
        let members = sqlx::query(
            "SELECT * FROM app.candidate_alphas WHERE candidate_id=$1 ORDER BY alpha_version_id",
        )
        .bind(id.as_uuid())
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            Ok(CandidateMemberV1 {
                alpha_version_id: db::id(r.try_get("alpha_version_id")?)?,
                qualification_id: db::id(r.try_get("qualification_id")?)?,
                ensemble_weight: decimal(r.try_get("ensemble_weight")?)?,
                calibration_id: db::optional_id(r, "calibration_id")?,
                forecast_unit: r.try_get("forecast_unit")?,
                coverage_fraction: decimal(r.try_get("coverage_fraction")?)?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
        let targets = sqlx::query(
            "SELECT * FROM app.candidate_targets WHERE candidate_id=$1 ORDER BY instrument_id",
        )
        .bind(id.as_uuid())
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            Ok(CandidateTargetV1 {
                instrument_id: r.try_get("instrument_id")?,
                target_weight: decimal(r.try_get("target_weight")?)?,
                currency: r.try_get("currency")?,
                asof: r.try_get("asof")?,
                valid_until: r.try_get("valid_until")?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(CandidateDetailV1 {
            header,
            members,
            targets,
        })
    }
}
