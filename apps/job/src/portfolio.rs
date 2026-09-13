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
    let original: PortfolioCurrentWeightsV1 =
        serde_json::from_slice(&read(request.current_weights_artifact_id)?)?;
    ensure!(
        original == request.current_weights,
        "PORTFOLIO_CURRENT_WEIGHTS_SOURCE_MISMATCH"
    );
    let market = crate::catalog::load_catalog(catalog, &request.selection)?;
    let horizon = request.members[0].parameters.label_horizon_observations as usize;
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
        request
            .assets
            .iter()
            .map(|a| &a.instrument_id)
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
    for member in &request.members {
        let module = read(member.model_artifact_id)?;
        let calibration: Option<NativeFrozenCalibrationV1> = member
            .calibration_artifact_id
            .map(|id| read(id).and_then(|bytes| Ok(serde_json::from_slice(&bytes)?)))
            .transpose()?;
        let result = crate::forecast::forecast(
            catalog,
            &NativeForecastRequestV1 {
                schema_version: SchemaV1,
                selection: request.selection.clone(),
                parameters: member.parameters.clone(),
            },
            &module,
        )?;
        consumed = consumed
            .checked_add(result.consumed_fuel.get())
            .ok_or_else(|| anyhow::anyhow!("PORTFOLIO_FUEL_OVERFLOW"))?;
        let mut forecasts = Vec::new();
        let mut available = 0;
        for (instrument, bar_type) in instruments.iter().zip(&request.selection.bar_types) {
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
            base_currency: request.mandate.base_currency.clone(),
            asof_ns: asof,
            available_ns: count(available)?,
            ensemble_weight: member.ensemble_weight.clone(),
            bar_types: request.selection.bar_types.clone(),
            instrument_ids: instruments.clone(),
            forecasts,
        });
    }
    let m = &request.mandate;
    let input = AllocationInputV1 {
        schema_version: SchemaV1,
        forecasts: PortfolioForecastInputV1 {
            schema_version: SchemaV1,
            decision_asof_ns: request.selection.decision_cutoff_ns,
            forecast_asof_ns: asof,
            horizon_kind: HorizonKind::FixedBars,
            horizon_value: count(horizon as u64)?,
            base_currency: m.base_currency.clone(),
            max_input_age_seconds: m.rebalance_schedule.max_input_age_seconds,
            bar_types: request.selection.bar_types.clone(),
            instrument_ids: instruments.clone(),
            members,
        },
        objective: m.objective,
        risk: m.risk_measure,
        base_currency: m.base_currency.clone(),
        capital_assumption: m.capital_assumption.clone(),
        current_cash_weight: original.cash_weight,
        exposure_tolerance: m.exposure_tolerance.clone(),
        constraints: m.constraints.clone(),
        optimizer: m.optimizer.clone(),
        alpha_ensemble: m.alpha_ensemble.clone(),
        covariance_estimator: m.covariance_estimator.clone(),
        assets: request.assets.clone(),
        return_history: PortfolioReturnHistoryV1 {
            schema_version: SchemaV1,
            base_currency: m.base_currency.clone(),
            horizon_kind: HorizonKind::FixedBars,
            horizon_value: count(horizon as u64)?,
            instrument_ids: instruments,
            bar_types: request.selection.bar_types.clone(),
            end_ns,
            available_ns,
            asset_returns,
        },
    };
    let allocation = crate::allocate(&input)?;
    let result = NativePortfolioBuildResultV1 {
        schema_version: SchemaV1,
        input,
        allocation,
        consumed_fuel: count(consumed)?,
    };
    domain::execution::portfolio_build_result(request, &result)?;
    Ok(result)
}
