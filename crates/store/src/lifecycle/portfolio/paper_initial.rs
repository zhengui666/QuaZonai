//! One immutable Paper model initial condition, never a balance ledger.
use super::*;
use contracts::{forward::ForwardEnvironmentV1, strategy_portfolio::FreshPaperCashV1};

pub(super) fn reference(
    row: &sqlx::postgres::PgRow,
) -> Result<PaperInitializationRefV1, StoreError> {
    Ok(PaperInitializationRefV1 {
        artifact_id: db::id(row.try_get("weights_artifact_id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        trader_id: row.try_get("trader_id")?,
        account_id: row.try_get("account_id")?,
    })
}

pub(super) async fn original(
    tx: &mut Tx<'_>,
    root: &PaperInitializationRefV1,
    project: Id,
) -> Result<sqlx::postgres::PgRow, StoreError> {
    // Immutable metadata needs no row lock. Claim serializes on downstream then
    // root; taking root first here would invert that order during approval.
    let row = sqlx::query("SELECT * FROM app.paper_initial_capital_sources WHERE weights_artifact_id=$1 AND project_id=$2")
        .bind(root.artifact_id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(StoreError::Invalid("paper_initialization_root"))?;
    domain::portfolio::paper_initialization_binding(root, &reference(&row)?, root.downstream_id)?;
    Ok(row)
}

pub(super) async fn validate(
    tx: &mut Tx<'_>,
    project: Id,
    request: &PortfolioBuildRequestV1,
    weights: &PortfolioCurrentWeightsV1,
) -> Result<(), StoreError> {
    let Some(root) = &weights.paper_initialization else {
        return Ok(());
    };
    if request.environment != ForwardEnvironmentV1::Paper {
        return Err(StoreError::Invalid("paper_initialization_paper_only"));
    }
    let row = original(tx, root, project).await?;
    let matching: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2 AND base_currency=$3 AND capital_assumption=$4 AND execution_assumptions_id=$5)")
        .bind(request.mandate_id.as_uuid()).bind(project.as_uuid()).bind(row.try_get::<String,_>("base_currency")?)
        .bind(row.try_get::<bigdecimal::BigDecimal,_>("starting_capital")?).bind(row.try_get::<uuid::Uuid,_>("execution_assumptions_id")?)
        .fetch_one(&mut **tx).await?;
    if !matching || weights.base_currency != row.try_get::<String, _>("base_currency")? {
        return Err(StoreError::Invalid("paper_initialization_capital_binding"));
    }
    if matches!(
        weights.source,
        PortfolioWeightsSourceV1::PaperInitialCapital { .. }
    ) {
        let unused: bool = sqlx::query_scalar("SELECT app.paper_initial_capital_unused($1)")
            .bind(root.artifact_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
        if !unused {
            return Err(StoreError::Invalid("paper_account_already_has_state"));
        }
    }
    Ok(())
}

pub(super) async fn resolve<R, Read>(
    tx: &mut Tx<'_>,
    project: Id,
    request: &PortfolioBuildRequestV1,
    read: &mut R,
) -> Result<weights::Resolved, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let PortfolioBuildWeightsV1::PaperInitialCapital {
        downstream_id,
        trader_id,
        account_id,
    } = &request.current_weights_source
    else {
        return Err(StoreError::Integrity);
    };
    if request.environment != ForwardEnvironmentV1::Paper {
        return Err(StoreError::Invalid("paper_initialization_paper_only"));
    }
    domain::portfolio::paper_account_scope(trader_id, account_id)?;
    sqlx::query("SELECT id FROM app.downstream_integrations WHERE id=$1 AND enabled AND environments IN ('PAPER','BOTH') FOR UPDATE")
        .bind(downstream_id.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(StoreError::Invalid("paper_initialization_downstream"))?;
    let existing = sqlx::query("SELECT s.*,a.byte_count FROM app.paper_initial_capital_sources s JOIN app.artifacts a ON a.id=s.weights_artifact_id WHERE s.downstream_id=$1 AND s.trader_id=$2 AND s.account_id=$3")
        .bind(downstream_id.as_uuid()).bind(trader_id).bind(account_id).fetch_optional(&mut **tx).await?;
    if let Some(row) = existing {
        // A renamed mandate/project is not a request for fresh funding.
        if row.try_get::<uuid::Uuid, _>("project_id")? != project.as_uuid()
            || row.try_get::<uuid::Uuid, _>("mandate_id")? != request.mandate_id.as_uuid()
        {
            return Err(StoreError::Invalid(
                "paper_initialization_scope_already_bound",
            ));
        }
        let root = reference(&row)?;
        let size = counter(row.try_get("byte_count")?)?;
        if size == DbCounter::ZERO || size.get() > 1024 * 1024 {
            return Err(StoreError::Integrity);
        }
        let bytes = read(root.artifact_id, size).await?;
        let content: PortfolioCurrentWeightsV1 =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
        if bytes.len() as u64 != size.get()
            || content.paper_initialization.as_ref() != Some(&root)
            || !matches!(
                content.source,
                PortfolioWeightsSourceV1::PaperInitialCapital { .. }
            )
        {
            return Err(StoreError::Integrity);
        }
        validate(tx, project, request, &content).await?;
        return Ok(weights::Resolved {
            content,
            origin: DataOrigin::Synthetic,
            artifact: Some(RuntimeInputV1::Artifact {
                artifact_id: root.artifact_id,
                storage_version: "1".into(),
                byte_count: size,
                role: ArtifactInputRole::Report,
            }),
        });
    }
    let empty: bool = sqlx::query_scalar("SELECT app.paper_account_uninitialized($1,$2,$3)")
        .bind(downstream_id.as_uuid())
        .bind(trader_id)
        .bind(account_id)
        .fetch_one(&mut **tx)
        .await?;
    if !empty {
        return Err(StoreError::Invalid("paper_account_already_has_state"));
    }
    let mandate = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2")
        .bind(request.mandate_id.as_uuid())
        .bind(project.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    let mandate = crate::portfolio::view(&mandate)?;
    let settings: serde_json::Value = sqlx::query_scalar("SELECT settings FROM app.execution_assumption_sources WHERE assumptions_id=$1 AND project_id=$2 AND runtime_id=$3")
        .bind(mandate.content.execution_assumptions_id.as_uuid()).bind(project.as_uuid()).bind(request.runtime_id.as_uuid()).fetch_one(&mut **tx).await?;
    let settings: NativeSimulationSettingsV1 =
        serde_json::from_value(settings).map_err(|_| StoreError::Integrity)?;
    if settings.starting_capital != mandate.content.capital_assumption
        || settings.base_currency != mandate.content.base_currency
    {
        return Err(StoreError::Invalid("paper_initialization_capital_binding"));
    }
    let datasets = crate::data_validation::dataset_bindings(
        tx,
        request.input_set_id,
        project,
        request.runtime_id,
        &[contracts::research::InputPurpose::Forward],
        read,
    )
    .await?;
    let [dataset] = datasets.as_slice() else {
        return Err(StoreError::Integrity);
    };
    if dataset.origin != DataOrigin::Real
        || dataset.metadata.pit_status != contracts::research::PitStatus::Verified
        || dataset.metadata.revision_policy != contracts::catalogs::DataRevisionPolicy::AsKnownThen
    {
        return Err(StoreError::Invalid("portfolio_real_pit_required"));
    }
    let [quality] = dataset.metadata.quality.datasets.as_slice() else {
        return Err(StoreError::Integrity);
    };
    if !(1..=MAX_ALLOCATION_ASSETS).contains(&quality.instrument_ids.len()) {
        return Err(StoreError::Invalid("paper_initialization_assets"));
    }
    let cutoff = dataset.selection.selection.decision_cutoff_ns;
    let until = cutoff
        .get()
        .checked_add(
            u64::from(mandate.content.rebalance_schedule.target_ttl_seconds) * 1_000_000_000,
        )
        .ok_or(StoreError::Integrity)?;
    let root = PaperInitializationRefV1 {
        artifact_id: Id::new(),
        downstream_id: *downstream_id,
        trader_id: trader_id.clone(),
        account_id: account_id.clone(),
    };
    // These timestamps describe the explicit model condition at its frozen
    // cutoff. The database receipt's created_at remains the actual clock.
    let content = PortfolioCurrentWeightsV1 {
        schema_version: SchemaV1,
        source: PortfolioWeightsSourceV1::PaperInitialCapital {
            account_start: FreshPaperCashV1 {
                downstream_id: *downstream_id,
                trader_id: trader_id.clone(),
                account_id: account_id.clone(),
                base_currency: settings.base_currency.clone(),
                starting_capital: settings.starting_capital,
                execution_assumptions_id: mandate.content.execution_assumptions_id,
            },
        },
        paper_initialization: Some(root),
        asof_ns: cutoff,
        available_ns: cutoff,
        valid_until_ns: DbCounter::new(until).map_err(|_| StoreError::Integrity)?,
        base_currency: settings.base_currency.clone(),
        cash_weight: "1".parse().map_err(|_| StoreError::Integrity)?,
        weights: quality
            .instrument_ids
            .iter()
            .map(|id| AllocationTargetV1 {
                instrument_id: id.clone(),
                currency: settings.base_currency.clone(),
                weight: "0".parse().unwrap(),
            })
            .collect(),
    };
    Ok(weights::Resolved {
        content,
        artifact: None,
        origin: DataOrigin::Synthetic,
    })
}

pub(super) async fn record(
    tx: &mut Tx<'_>,
    project: Id,
    request: &PortfolioBuildRequestV1,
    weights: &PortfolioCurrentWeightsV1,
    artifact: Id,
) -> Result<(), StoreError> {
    let PortfolioWeightsSourceV1::PaperInitialCapital { account_start } = &weights.source else {
        return Ok(());
    };
    let root = weights
        .paper_initialization
        .as_ref()
        .ok_or(StoreError::Integrity)?;
    if root.artifact_id != artifact {
        return Err(StoreError::Integrity);
    }
    sqlx::query("INSERT INTO app.paper_initial_capital_sources(weights_artifact_id,project_id,mandate_id,downstream_id,trader_id,account_id,execution_assumptions_id,base_currency,starting_capital) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(artifact.as_uuid()).bind(project.as_uuid()).bind(request.mandate_id.as_uuid()).bind(root.downstream_id.as_uuid())
        .bind(&root.trader_id).bind(&root.account_id).bind(account_start.execution_assumptions_id.as_uuid())
        .bind(&account_start.base_currency).bind(account_start.starting_capital.as_decimal()).execute(&mut **tx).await?;
    Ok(())
}
