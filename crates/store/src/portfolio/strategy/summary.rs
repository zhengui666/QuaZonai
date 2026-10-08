//! Project-authorized projection of the already accepted strategy report only.
use super::*;
use contracts::DbCounter;

impl Store {
    pub async fn strategy_portfolio_summary<R, Read>(
        &self,
        actor: &Actor,
        id: Id,
        mut read: R,
    ) -> Result<StrategyPortfolioSummaryV1, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CANDIDATE} WHERE c.id=$1 AND c.source_kind='STRATEGY_ALPHA'"
        )))
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(row.try_get("project_id")?)?).await?;
        let candidate = candidate(&row)?;
        // This specific accepted, published candidate permits its summary read.
        // Generic EVALUATOR_ONLY artifact access and other projects stay unchanged.
        let size: Option<i64> = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 AND producer_run_id=$3 AND producer_attempt_id=$4 AND kind='REPORT' AND schema_name='qz.strategy_portfolio' AND schema_version='1' AND media_type='application/json' AND access_class='EVALUATOR_ONLY' AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1'")
            .bind(candidate.report_artifact_id.as_uuid())
            .bind(candidate.project_id.as_uuid())
            .bind(candidate.run_id.as_uuid())
            .bind(candidate.accepted_attempt_id.as_uuid())
            .fetch_optional(&mut *tx).await?;
        let size = u64::try_from(size.ok_or(StoreError::Integrity)?)
            .ok()
            .and_then(|value| DbCounter::new(value).ok())
            .ok_or(StoreError::Integrity)?;
        if size == DbCounter::ZERO {
            return Err(StoreError::Integrity);
        }
        let bytes = read(candidate.report_artifact_id, size).await?;
        if bytes.len() as u64 != size.get() {
            return Err(StoreError::Integrity);
        }
        let report: NativeStrategyCompositionResultV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        let row = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2 AND allocation_method='FIXED_TARGET_WEIGHTS'")
            .bind(candidate.mandate_id.as_uuid()).bind(candidate.project_id.as_uuid())
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::Integrity)?;
        if db::json(&mandate(&row)?.content)? != db::json(&report.request.mandate)? {
            return Err(StoreError::Integrity);
        }
        let summary = domain::execution::strategy_portfolio_summary(&candidate, &report)
            .map_err(|_| StoreError::Integrity)?;
        tx.commit().await?;
        Ok(summary)
    }
}
