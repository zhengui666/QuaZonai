//! Freeze declared simulation models against original registered instrument fees.
use crate::{
    authority::Actor, commands, db, lifecycle::native::NativeObjectPublication, Store, StoreError,
};
use contracts::{
    control::{CommandResult, ListQuery, OperatorOperation, Page},
    execution_assumptions::*,
    portfolio::NAUTILUS_EXECUTION_VERSION,
    research::DataPartition,
    runs::RunKind,
    DbCounter, DecimalValue, Id,
};
use sqlx::{postgres::PgRow, Row};

const VIEW: &str = "SELECT e.*,s.project_id,s.input_set_id,s.dataset_revision_id,s.runtime_id,s.capability_snapshot_artifact_id,s.settings FROM app.execution_assumptions e JOIN app.execution_assumption_sources s ON s.assumptions_id=e.id";

fn view(row: &PgRow) -> Result<ExecutionAssumptionsViewV1, StoreError> {
    if row.try_get::<String, _>("cost_assumption_status")? != "CONSERVATIVE_ASSUMPTION" {
        return Err(StoreError::Integrity);
    }
    Ok(ExecutionAssumptionsViewV1 {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        input_set_id: db::id(row.try_get("input_set_id")?)?,
        dataset_revision_id: db::id(row.try_get("dataset_revision_id")?)?,
        runtime_id: db::id(row.try_get("runtime_id")?)?,
        capability_snapshot_artifact_id: db::id(row.try_get("capability_snapshot_artifact_id")?)?,
        fee_schedule_artifact_id: db::id(row.try_get("fee_schedule_artifact_id")?)?,
        engine_image_ref: row.try_get("engine_image_ref")?,
        venue_capability_ref: row.try_get("venue_capability_ref")?,
        calendar_version: row.try_get("calendar_version")?,
        settlement_rule_ref: row.try_get("settlement_rule_ref")?,
        cost_assumption_status: ConservativeAssumption::Conservative,
        settings: serde_json::from_value(row.try_get("settings")?)
            .map_err(|_| StoreError::Integrity)?,
        created_at: row.try_get("created_at")?,
    })
}

impl Store {
    pub async fn execution_assumptions(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<ExecutionAssumptionsViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query(&format!("{VIEW} WHERE s.project_id=$1 AND ($2::uuid IS NULL OR e.id<$2) ORDER BY e.id DESC LIMIT $3"))
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| v.id))
    }

    pub async fn execution_assumption(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<ExecutionAssumptionsViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(&format!("{VIEW} WHERE e.id=$1"))
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(row.try_get("project_id")?)?).await?;
        let result = view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn create_execution_assumptions<R, Read, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &ExecutionAssumptionsCreateV1,
        mut read: R,
        publish: P,
    ) -> Result<CommandResult<ExecutionAssumptionsViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::control::text(&request.settlement_rule_ref, 1, 200, false)?;
        domain::portfolio::simulation_settings(&request.settings)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ExecutionAssumptionsCreate,
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
        let bindings = crate::data_validation::dataset_bindings(
            &mut tx,
            request.input_set_id,
            request.project_id,
            request.runtime_id,
            &[
                DataPartition::Discovery,
                DataPartition::Validation,
                DataPartition::Forward,
            ],
            &mut read,
        )
        .await?;
        let binding = bindings
            .into_iter()
            .find(|b| b.selection.dataset_revision_id == request.dataset_revision_id)
            .ok_or(StoreError::Invalid("execution_assumptions_dataset"))?;
        let cap = crate::runtime::require_capabilities(
            &mut tx,
            request.runtime_id,
            request.expected_runtime_revision,
            RunKind::PortfolioSimulate,
        )
        .await?;
        if cap
            .engine_versions
            .get("simulation-models")
            .map(String::as_str)
            != Some("1")
            || cap.engine_versions.get("nautilus").map(String::as_str)
                != Some(NAUTILUS_EXECUTION_VERSION)
        {
            return Err(
                domain::DomainError::CapabilityUnavailable("execution_assumption_models").into(),
            );
        }
        let image = cap
            .image_refs
            .iter()
            .find(|v| v.job_kind == RunKind::PortfolioSimulate)
            .ok_or(StoreError::Integrity)?
            .image_ref
            .clone();
        let metadata = &binding.metadata;
        let ids = &metadata.quality.datasets[0].instrument_ids;
        if ids.len() != request.settings.fee_rates.len() {
            return Err(StoreError::Invalid("execution_assumptions_fees"));
        }
        let mut venue = None;
        let definitions = metadata
            .universe
            .instrument_definitions
            .iter()
            .map(domain::catalogs::instrument_definition)
            .collect::<Result<Vec<_>, _>>()?;
        for rate in &request.settings.fee_rates {
            if !ids.contains(&rate.instrument_id) {
                return Err(StoreError::Invalid("execution_assumptions_fee_identity"));
            }
            let (class, definition) = definitions
                .iter()
                .copied()
                .find(|(_, v)| v["id"].as_str() == Some(&rate.instrument_id))
                .ok_or(StoreError::Integrity)?;
            let currency = match class {
                "CurrencyPair" => &definition["quote_currency"],
                "Equity" => &definition["currency"],
                _ => {
                    return Err(domain::DomainError::CapabilityUnavailable(
                        "execution_assumption_instrument",
                    )
                    .into())
                }
            };
            let maker: DecimalValue = serde_json::from_value(definition["maker_fee"].clone())
                .map_err(|_| StoreError::Integrity)?;
            let taker: DecimalValue = serde_json::from_value(definition["taker_fee"].clone())
                .map_err(|_| StoreError::Integrity)?;
            let native_venue = rate
                .instrument_id
                .rsplit_once('.')
                .map(|(_, v)| v)
                .ok_or(StoreError::Integrity)?;
            if currency.as_str() != Some(&request.settings.base_currency)
                || maker != rate.maker
                || taker != rate.taker
                || venue.is_some_and(|v| v != native_venue)
                || !cap.venues.iter().any(|v| {
                    v.venue == native_venue && v.instrument_classes.iter().any(|c| c == class)
                })
            {
                return Err(StoreError::Invalid("execution_assumptions_native_fees"));
            }
            venue = Some(native_venue);
        }
        let capability: uuid::Uuid = sqlx::query_scalar(
            "SELECT last_capability_snapshot_artifact_id FROM app.runtime_integrations WHERE id=$1",
        )
        .bind(request.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let id = prepared.target;
        let artifact = Id::new();
        let bytes = serde_json::to_vec(&request.settings).map_err(|_| StoreError::Integrity)?;
        let size = i64::try_from(bytes.len()).map_err(|_| StoreError::Integrity)?;
        publish(NativeObjectPublication {
            id: artifact,
            bytes,
        })
        .await?;
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','qz.native_simulation_settings','1','LOCAL',$3,'1',$4,'RESEARCH','SYNTHETIC','OPERATOR','REFERENCED')")
            .bind(artifact.as_uuid()).bind(request.project_id.as_uuid()).bind(artifact.to_string()).bind(size).execute(&mut *tx).await?;
        let s = &request.settings;
        sqlx::query("INSERT INTO app.execution_assumptions(id,venue_capability_ref,engine_image_ref,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,latency_model,cost_assumption_status,calendar_version,settlement_rule_ref) VALUES($1,$2,$3,'BAR',$4,$5,$6,$7,$7,$8,'CONSERVATIVE_ASSUMPTION',$9,$10)")
            .bind(id.as_uuid()).bind(venue.ok_or(StoreError::Integrity)?).bind(image).bind(s.starting_capital.as_decimal()).bind(&s.base_currency).bind(artifact.as_uuid()).bind(db::json(&s.fill_model)?).bind(db::json(&s.latency_model)?).bind(&metadata.universe.calendar_version).bind(&request.settlement_rule_ref).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.execution_assumption_sources(assumptions_id,project_id,input_set_id,dataset_revision_id,runtime_id,capability_snapshot_artifact_id,settings) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(request.input_set_id.as_uuid()).bind(request.dataset_revision_id.as_uuid()).bind(request.runtime_id.as_uuid()).bind(capability).bind(db::json(s)?).execute(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        crate::research::revalidate_frozen_inputs(
            &mut tx,
            request.input_set_id,
            request.project_id,
            request.runtime_id,
        )
        .await?;
        crate::runtime::require_capabilities(
            &mut tx,
            request.runtime_id,
            request.expected_runtime_revision,
            RunKind::PortfolioSimulate,
        )
        .await?;
        let row = sqlx::query(&format!("{VIEW} WHERE e.id=$1"))
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let result = commands::finish(&mut tx, prepared, view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
