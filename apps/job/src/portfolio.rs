//! Frozen catalog and original Wasm models produce all numerical portfolio inputs.
use anyhow::{ensure, Result};
use contracts::{
    brief::HorizonKind, evidence::ForecastUnit, portfolio::*, science::*, DbCounter, Id, SchemaV1,
};
use nautilus_model::instruments::Instrument;
use std::path::Path;

fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

pub fn build(
    catalog: &Path,
    request: &NativePortfolioBuildRequestV1,
    mut read: impl FnMut(Id) -> Result<Vec<u8>>,
) -> Result<NativePortfolioBuildResultV1> {
    domain::execution::portfolio_build_request(request)?;
    let costs: NativeSimulationSettingsV1 =
        serde_json::from_slice(&read(request.mandate.constraints.transaction_costs_ref)?)?;
    ensure!(
        serde_json::to_value(&costs)? == serde_json::to_value(&request.execution_settings)?,
        "PORTFOLIO_EXECUTION_SETTINGS_SOURCE_MISMATCH"
    );
    if let Some(binding) = &request.bar_liquidity {
        let report = serde_json::from_slice(&read(binding.assumption.report_artifact_id)?)?;
        domain::execution::portfolio_build_liquidity(request, &report)?;
    }
    if let Some(policy) = &request.rolling_liquidity {
        let original: NativeRollingBarLiquidityPolicyV1 = serde_json::from_slice(&read(
            request
                .mandate
                .constraints
                .liquidity_ref
                .ok_or_else(|| anyhow::anyhow!("PORTFOLIO_ROLLING_POLICY_MISSING"))?,
        )?)?;
        ensure!(original == *policy, "PORTFOLIO_ROLLING_POLICY_MISMATCH");
    }
    let original: PortfolioCurrentWeightsV1 =
        serde_json::from_slice(&read(request.current_weights_artifact_id)?)?;
    ensure!(
        original == request.current_weights,
        "PORTFOLIO_CURRENT_WEIGHTS_SOURCE_MISMATCH"
    );
    let prepared = prepare(
        catalog,
        &request.selection,
        &request.mandate,
        &request.execution_settings,
        &request.assets,
        &request.members,
        &mut read,
    )?;
    let bar_notionals = if request.rolling_liquidity.is_some() {
        crate::catalog::last_bar_notionals(&crate::catalog::load_catalog(
            catalog,
            &request.selection,
        )?)?
    } else {
        Vec::new()
    };
    let source_assets = domain::execution::portfolio_rolling_liquidity_assets(
        &request.selection,
        &request.assets,
        request.rolling_liquidity.as_ref(),
        &request.mandate.base_currency,
        prepared.forecasts.forecast_asof_ns,
        request.selection.decision_cutoff_ns,
        &bar_notionals,
    )?;
    let assets = domain::execution::portfolio_costs(
        &request.selection,
        &request.mandate,
        &request.execution_settings,
        &source_assets,
        &prepared.slippage,
    )?;
    let input = allocation_input(&request.mandate, assets, original.cash_weight, &prepared);
    let allocation = crate::allocate(&input)?;
    let result = NativePortfolioBuildResultV1 {
        schema_version: SchemaV1,
        bar_notionals,
        slippage_references: prepared.slippage,
        input,
        allocation,
        consumed_fuel: prepared.consumed_fuel,
    };
    domain::execution::portfolio_build_result(request, &result)?;
    Ok(result)
}

pub(crate) struct Prepared {
    pub forecasts: PortfolioForecastInputV1,
    pub returns: PortfolioReturnHistoryV1,
    pub slippage: Vec<NativePortfolioSlippageReferenceV1>,
    pub consumed_fuel: DbCounter,
}

/// Pure model/catalog preparation, deliberately without any weights-source identity.
pub(crate) fn prepare(
    catalog: &Path,
    selection: &NativeBarSelectionV1,
    mandate: &MandateContentV1,
    settings: &NativeSimulationSettingsV1,
    assets: &[AllocationAssetV1],
    models: &[NativePortfolioAlphaV1],
    mut read: impl FnMut(Id) -> Result<Vec<u8>>,
) -> Result<Prepared> {
    ensure!((2..=256).contains(&models.len()), "PORTFOLIO_MEMBERS");
    let market = crate::catalog::load_catalog(catalog, selection)?;
    crate::simulation::execution_market(&market, settings)?;
    let horizon = models[0].parameters.label_horizon_observations as usize;
    let first = &market.series[0];
    let rows = first.bars.len();
    ensure!(
        rows >= horizon + 2 && rows - horizon <= MAX_RETURN_OBSERVATIONS,
        "PORTFOLIO_RETURN_SAMPLE_LIMIT"
    );
    ensure!(
        market
            .series
            .len()
            .checked_mul(rows - horizon)
            .is_some_and(|n| n <= MAX_RETURN_VALUES),
        "PORTFOLIO_RETURN_SIZE_LIMIT"
    );
    let instruments = market
        .series
        .iter()
        .map(|s| s.instrument.id().to_string())
        .collect::<Vec<_>>();
    ensure!(
        assets
            .iter()
            .map(|asset| &asset.instrument_id)
            .eq(instruments.iter()),
        "PORTFOLIO_ASSET_ORDER"
    );
    for series in &market.series {
        ensure!(
            series.bars.len() == rows
                && series
                    .bars
                    .iter()
                    .zip(&first.bars)
                    .all(|(a, b)| a.ts_event == b.ts_event),
            "PORTFOLIO_INCOMPLETE_COMMON_WINDOWS"
        );
    }
    let asof = count(first.bars[rows - 1].ts_event.as_u64())?;
    let end_ns = first.bars[horizon..]
        .iter()
        .map(|b| count(b.ts_event.as_u64()))
        .collect::<Result<Vec<_>>>()?;
    let available_ns = (horizon..rows)
        .map(|index| {
            count(
                market
                    .series
                    .iter()
                    .map(|s| s.bars[index].ts_init.as_u64())
                    .max()
                    .unwrap(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let asset_returns = market
        .series
        .iter()
        .map(|s| {
            (horizon..rows)
                .map(|index| {
                    s.bars[index].close.as_f64() / s.bars[index - horizon].close.as_f64() - 1.0
                })
                .collect()
        })
        .collect();
    let mut members = Vec::new();
    let mut consumed = 0_u64;
    for member in models {
        let module = read(member.model_artifact_id)?;
        let calibration: Option<NativeFrozenCalibrationV1> = member
            .calibration_artifact_id
            .map(|id| read(id).and_then(|bytes| Ok(serde_json::from_slice(&bytes)?)))
            .transpose()?;
        let result = crate::forecast::forecast(
            catalog,
            &NativeForecastRequestV1 {
                schema_version: SchemaV1,
                selection: selection.clone(),
                parameters: member.parameters.clone(),
            },
            &module,
        )?;
        consumed = consumed
            .checked_add(result.consumed_fuel.get())
            .ok_or_else(|| anyhow::anyhow!("PORTFOLIO_FUEL_OVERFLOW"))?;
        let mut forecasts = Vec::new();
        let mut available = 0;
        for (instrument, bar_type) in instruments.iter().zip(&selection.bar_types) {
            let point = result
                .points
                .iter()
                .rev()
                .find(|p| &p.instrument_id == instrument)
                .ok_or_else(|| anyhow::anyhow!("PORTFOLIO_FORECAST_MISSING"))?;
            ensure!(point.event_ns == asof, "PORTFOLIO_FORECAST_ASOF");
            let value = point
                .forecast
                .ok_or_else(|| anyhow::anyhow!("PORTFOLIO_FORECAST_WARMUP"))?;
            let value = match &calibration {
                Some(model) => crate::validation::predict_frozen_calibration(
                    model,
                    bar_type,
                    count(horizon as u64)?,
                    point.available_ns,
                    &[value],
                )?[0],
                None => value,
            };
            forecasts.push(value);
            available = available.max(point.available_ns.get());
        }
        members.push(AlphaForecastV1 {
            alpha_id: member.alpha_id,
            alpha_version_id: member.alpha_version_id,
            forecast_unit: ForecastUnit::ReturnPerHorizon,
            horizon_kind: HorizonKind::FixedBars,
            horizon_value: count(horizon as u64)?,
            base_currency: mandate.base_currency.clone(),
            asof_ns: asof,
            available_ns: count(available)?,
            ensemble_weight: member.ensemble_weight.clone(),
            bar_types: selection.bar_types.clone(),
            instrument_ids: instruments.clone(),
            forecasts,
        });
    }
    let (fill, _) = domain::portfolio::simulation_models(settings)?;
    let slippage_references = if fill.prob_slippage.is_positive() {
        market
            .series
            .iter()
            .map(|series| {
                let bar = &series.bars[rows - 1];
                Ok(NativePortfolioSlippageReferenceV1 {
                    instrument_id: series.instrument.id().to_string(),
                    currency: series.instrument.quote_currency().to_string(),
                    event_ns: count(bar.ts_event.as_u64())?,
                    available_ns: count(bar.ts_init.as_u64())?,
                    close_price: bar.close.to_string().parse().map_err(anyhow::Error::msg)?,
                    price_increment: series
                        .instrument
                        .price_increment()
                        .to_string()
                        .parse()
                        .map_err(anyhow::Error::msg)?,
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    Ok(Prepared {
        forecasts: PortfolioForecastInputV1 {
            schema_version: SchemaV1,
            decision_asof_ns: selection.decision_cutoff_ns,
            forecast_asof_ns: asof,
            horizon_kind: HorizonKind::FixedBars,
            horizon_value: count(horizon as u64)?,
            base_currency: mandate.base_currency.clone(),
            max_input_age_seconds: mandate.rebalance_schedule.max_input_age_seconds,
            bar_types: selection.bar_types.clone(),
            instrument_ids: instruments.clone(),
            members,
        },
        returns: PortfolioReturnHistoryV1 {
            schema_version: SchemaV1,
            base_currency: mandate.base_currency.clone(),
            horizon_kind: HorizonKind::FixedBars,
            horizon_value: count(horizon as u64)?,
            instrument_ids: instruments,
            bar_types: selection.bar_types.clone(),
            end_ns,
            available_ns,
            asset_returns,
        },
        slippage: slippage_references,
        consumed_fuel: count(consumed)?,
    })
}

pub(crate) fn allocation_input(
    mandate: &MandateContentV1,
    assets: Vec<AllocationAssetV1>,
    cash: contracts::DecimalValue,
    prepared: &Prepared,
) -> AllocationInputV1 {
    AllocationInputV1 {
        schema_version: SchemaV1,
        forecasts: prepared.forecasts.clone(),
        objective: mandate.objective,
        risk: mandate.risk_measure,
        base_currency: mandate.base_currency.clone(),
        capital_assumption: mandate.capital_assumption.clone(),
        current_cash_weight: cash,
        exposure_tolerance: mandate.exposure_tolerance.clone(),
        constraints: mandate.constraints.clone(),
        optimizer: mandate.optimizer.clone(),
        alpha_ensemble: mandate.alpha_ensemble.clone(),
        covariance_estimator: mandate.covariance_estimator.clone(),
        assets,
        return_history: prepared.returns.clone(),
    }
}
