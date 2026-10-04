//! Fixed-target mandates use the existing operator grant and immutable receipt.
use super::*;

impl Store {
    pub async fn create_strategy_mandate(
        &self,
        actor: &Actor,
        key: &str,
        request: &StrategyMandateCreateV1,
    ) -> Result<CommandResult<StrategyMandateViewV1>, StoreError> {
        let content = &request.content;
        domain::execution::strategy::mandate(content)?;
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
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        crate::research::project_for_write(&mut tx, request.project_id).await?;
        let capabilities = crate::runtime::require_capabilities(
            &mut tx,
            request.runtime_id,
            request.expected_runtime_revision,
            RunKind::PortfolioBuild,
        )
        .await?;
        if capabilities
            .engine_versions
            .get("strategy-composition")
            .map(String::as_str)
            != Some("1")
            || capabilities
                .engine_versions
                .get("nautilus")
                .map(String::as_str)
                != Some(NAUTILUS_EXECUTION_VERSION)
            || !capabilities
                .artifact_schemas
                .iter()
                .any(|schema| schema.name == "qz.strategy_portfolio" && schema.version == "1")
        {
            return Err(domain::DomainError::CapabilityUnavailable("strategy_composition").into());
        }
        let row = sqlx::query("SELECT e.*,s.input_set_id,s.settings,u.calendar_version AS universe_calendar_version FROM app.execution_assumptions e JOIN app.execution_assumption_sources s ON s.assumptions_id=e.id AND s.project_id=$2 AND s.runtime_id=$3 JOIN app.dataset_revisions d ON d.id=s.dataset_revision_id AND d.universe_version_id=$4 JOIN app.universe_versions u ON u.id=d.universe_version_id WHERE e.id=$1")
            .bind(content.execution_assumptions_id.as_uuid()).bind(request.project_id.as_uuid()).bind(request.runtime_id.as_uuid()).bind(content.universe_version_id.as_uuid())
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("strategy_mandate_frozen_references"))?;
        let settings: contracts::science::NativeSimulationSettingsV1 =
            serde_json::from_value(row.try_get("settings")?).map_err(|_| StoreError::Integrity)?;
        domain::portfolio::simulation_settings(&settings)?;
        // The immutable source was validated by execution_fees/execution_account
        // against its catalog. A fresh all-cash balance does not imply the native
        // Cash account type: CurrencyPair execution uses Margin with leverage one.
        if settings.leverage.as_decimal() != &bigdecimal::BigDecimal::from(1) {
            return Err(
                domain::DomainError::CapabilityUnavailable("strategy_account_model").into(),
            );
        }
        let image: String = row.try_get("engine_image_ref")?;
        if row.try_get::<String, _>("base_currency")? != content.base_currency
            || row.try_get::<bigdecimal::BigDecimal, _>("starting_capital")?
                != *content.capital_assumption.as_decimal()
            || settings.base_currency != content.base_currency
            || settings.starting_capital != content.capital_assumption
            || settings.exposure_tolerance != content.exposure_tolerance
            || db::id(row.try_get("fee_schedule_artifact_id")?)?
                != content.constraints.transaction_costs_ref
            || db::optional_id(&row, "liquidity_artifact_id")? != content.constraints.liquidity_ref
            || row
                .try_get::<Option<bigdecimal::BigDecimal>, _>("participation_limit")?
                .as_ref()
                != content
                    .constraints
                    .max_participation
                    .as_ref()
                    .map(|v| v.as_decimal())
            || row.try_get::<String, _>("cost_assumption_status")? == "INSUFFICIENT"
            || row.try_get::<String, _>("calendar_version")?
                != row.try_get::<String, _>("universe_calendar_version")?
            || !capabilities
                .image_refs
                .iter()
                .any(|v| v.job_kind == RunKind::PortfolioBuild && v.image_ref == image)
        {
            return Err(StoreError::Invalid("strategy_mandate_frozen_references"));
        }
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            db::id(row.try_get("input_set_id")?)?,
            request.project_id,
            request.runtime_id,
        )
        .await?;
        let previous: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version),0) FROM app.portfolio_mandates WHERE project_id=$1",
        )
        .bind(request.project_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let version = previous.checked_add(1).ok_or(StoreError::Integrity)?;
        let schedule = RebalanceScheduleV1 {
            schema_version: SchemaV1,
            kind: RebalanceKind::Manual,
            interval_seconds: None,
            calendar_ref: None,
            timezone: "UTC".into(),
            session_offset_seconds: None,
            max_input_age_seconds: content.max_input_age_seconds,
            target_ttl_seconds: content.target_ttl_seconds,
        };
        sqlx::query("INSERT INTO app.portfolio_mandates(id,project_id,version,allocation_method,strategy_content,base_currency,capital_assumption,universe_version_id,constraints,rebalance_schedule,execution_assumptions_id,exposure_tolerance) VALUES($1,$2,$3,'FIXED_TARGET_WEIGHTS',$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(prepared.target.as_uuid()).bind(request.project_id.as_uuid()).bind(version).bind(db::json(content)?)
            .bind(&content.base_currency).bind(content.capital_assumption.as_decimal()).bind(content.universe_version_id.as_uuid())
            .bind(db::json(&content.constraints)?).bind(db::json(&schedule)?).bind(content.execution_assumptions_id.as_uuid()).bind(content.exposure_tolerance.as_decimal())
            .execute(&mut *tx).await?;
        let row = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1")
            .bind(prepared.target.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, mandate(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
