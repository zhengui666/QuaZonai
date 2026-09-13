//! Immutable Mandate versions using the existing Operator transaction and native probe.
use crate::{authority::Actor, commands, control::page, db, Store, StoreError};
use contracts::{
    control::{CommandResult, ListQuery, OperatorOperation, Page},
    portfolio::*,
    research::SelectionRuleV1,
    runs::RunKind,
    Id,
};
use sqlx::{postgres::PgRow, Row};

pub(crate) fn view(r: &PgRow) -> Result<MandateViewV1, StoreError> {
    Ok(MandateViewV1 {
        id: db::id(r.try_get("id")?)?,
        project_id: db::id(r.try_get("project_id")?)?,
        version: u32::try_from(r.try_get::<i32, _>("version")?)
            .map_err(|_| StoreError::Integrity)?,
        created_at: r.try_get("created_at")?,
        content: MandateContentV1 {
            objective: db::enum_value(r, "objective")?,
            risk_measure: db::enum_value(r, "risk_measure")?,
            base_currency: r.try_get("base_currency")?,
            capital_assumption: r
                .try_get::<bigdecimal::BigDecimal, _>("capital_assumption")?
                .to_plain_string()
                .parse()
                .map_err(|_| StoreError::Integrity)?,
            universe_version_id: db::id(r.try_get("universe_version_id")?)?,
            covariance_estimator: serde_json::from_value(r.try_get("covariance_estimator")?)
                .map_err(|_| StoreError::Integrity)?,
            alpha_ensemble: serde_json::from_value(r.try_get("alpha_ensemble")?)
                .map_err(|_| StoreError::Integrity)?,
            optimizer: serde_json::from_value(r.try_get("optimizer")?)
                .map_err(|_| StoreError::Integrity)?,
            constraints: serde_json::from_value(r.try_get("constraints")?)
                .map_err(|_| StoreError::Integrity)?,
            rebalance_schedule: serde_json::from_value(r.try_get("rebalance_schedule")?)
                .map_err(|_| StoreError::Integrity)?,
            required_evaluation_policy_id: db::id(r.try_get("required_evaluation_policy_id")?)?,
            execution_assumptions_id: db::id(r.try_get("execution_assumptions_id")?)?,
            exposure_tolerance: r
                .try_get::<bigdecimal::BigDecimal, _>("exposure_tolerance")?
                .to_plain_string()
                .parse()
                .map_err(|_| StoreError::Integrity)?,
        },
    })
}

impl Store {
    pub async fn mandates(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<MandateViewV1>, StoreError> {
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
        let rows = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(page(items, query.limit, |v| v.id))
    }

    pub async fn mandate(&self, actor: &Actor, id: Id) -> Result<MandateViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let r = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(r.try_get("project_id")?)?).await?;
        let result = view(&r)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn create_mandate(
        &self,
        actor: &Actor,
        key: &str,
        request: &MandateCreateV1,
    ) -> Result<CommandResult<MandateViewV1>, StoreError> {
        let c = &request.content;
        domain::portfolio::mandate(c)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::MandateCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        crate::research::project_for_write(&mut tx, request.project_id).await?;
        let cap = crate::runtime::require_capabilities(
            &mut tx,
            request.runtime_id,
            request.expected_runtime_revision,
            RunKind::PortfolioBuild,
        )
        .await?;
        for (library, version) in [
            ("clarabel", CLARABEL_VERSION),
            ("ndarray", FIXED_ENSEMBLE_VERSION),
            ("ndarray-stats", SAMPLE_COVARIANCE_VERSION),
            ("portfolio-models", "4"),
        ] {
            if cap.engine_versions.get(library).map(String::as_str) != Some(version) {
                return Err(
                    domain::DomainError::CapabilityUnavailable("mandate_native_models").into(),
                );
            }
        }
        if !cap.solver_capabilities.iter().any(|v| v == "CONVEX_QP") {
            return Err(domain::DomainError::CapabilityUnavailable("mandate_native_solver").into());
        }
        let r = sqlx::query("SELECT e.*,u.calendar_ref AS universe_calendar,u.calendar_version AS universe_calendar_version,p.selection_rule FROM app.execution_assumptions e JOIN app.universe_versions u ON u.id=$2 JOIN app.evaluation_policies p ON p.id=$3 AND p.project_id=$4 WHERE e.id=$1")
            .bind(c.execution_assumptions_id.as_uuid()).bind(c.universe_version_id.as_uuid()).bind(c.required_evaluation_policy_id.as_uuid()).bind(request.project_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let selection: SelectionRuleV1 = serde_json::from_value(r.try_get("selection_rule")?)
            .map_err(|_| StoreError::Integrity)?;
        let image: String = r.try_get("engine_image_ref")?;
        if selection.execution_assumptions_id != c.execution_assumptions_id
            || r.try_get::<String, _>("base_currency")? != c.base_currency
            || r.try_get::<bigdecimal::BigDecimal, _>("starting_capital")?
                != *c.capital_assumption.as_decimal()
            || db::id(r.try_get("fee_schedule_artifact_id")?)?
                != c.constraints.transaction_costs_ref
            || db::optional_id(&r, "liquidity_artifact_id")? != c.constraints.liquidity_ref
            || r.try_get::<Option<bigdecimal::BigDecimal>, _>("participation_limit")?
                .as_ref()
                != c.constraints
                    .max_participation
                    .as_ref()
                    .map(|v| v.as_decimal())
            || r.try_get::<String, _>("cost_assumption_status")? == "INSUFFICIENT"
            || r.try_get::<String, _>("calendar_version")?
                != r.try_get::<String, _>("universe_calendar_version")?
            || c.rebalance_schedule.calendar_ref.as_ref().is_some_and(|v| {
                r.try_get::<String, _>("universe_calendar").ok().as_ref() != Some(v)
            })
            || !cap
                .image_refs
                .iter()
                .any(|v| v.job_kind == RunKind::PortfolioBuild && v.image_ref == image)
        {
            return Err(domain::DomainError::Invalid("mandate_frozen_references").into());
        }
        let previous: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version),0) FROM app.portfolio_mandates WHERE project_id=$1",
        )
        .bind(request.project_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let version = previous.checked_add(1).ok_or(StoreError::Integrity)?;
        let id = prepared.target;
        sqlx::query("INSERT INTO app.portfolio_mandates(id,project_id,version,objective,risk_measure,base_currency,capital_assumption,universe_version_id,covariance_estimator,alpha_ensemble,optimizer,constraints,rebalance_schedule,required_evaluation_policy_id,execution_assumptions_id,exposure_tolerance) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(version).bind(db::code(&c.objective)?).bind(db::code(&c.risk_measure)?).bind(&c.base_currency).bind(c.capital_assumption.as_decimal()).bind(c.universe_version_id.as_uuid()).bind(db::json(&c.covariance_estimator)?).bind(db::json(&c.alpha_ensemble)?).bind(db::json(&c.optimizer)?).bind(db::json(&c.constraints)?).bind(db::json(&c.rebalance_schedule)?).bind(c.required_evaluation_policy_id.as_uuid()).bind(c.execution_assumptions_id.as_uuid()).bind(c.exposure_tolerance.as_decimal()).execute(&mut *tx).await?;
        let r = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let result = commands::finish(&mut tx, prepared, view(&r)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
