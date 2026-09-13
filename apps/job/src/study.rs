//! Offline rolling science, with one native account and no fabricated source identities.
use anyhow::{ensure, Result};
use contracts::{portfolio::*, science::*, DbCounter, Id, SchemaV1};
use std::{collections::BTreeMap, path::Path};

fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

pub fn evaluate(
    catalog: &Path,
    request: &NativePortfolioStudyRequestV1,
    mut read: impl FnMut(Id) -> Result<Vec<u8>>,
) -> Result<NativePortfolioStudyResultV1> {
    let cutoffs = domain::execution::portfolio_study_cutoffs(request)?;
    let mut objects = BTreeMap::new();
    for id in request
        .members
        .iter()
        .flat_map(|m| std::iter::once(m.model_artifact_id).chain(m.calibration_artifact_id))
        .chain(std::iter::once(
            request.mandate.constraints.transaction_costs_ref,
        ))
        .chain(request.mandate.constraints.liquidity_ref)
    {
        if let std::collections::btree_map::Entry::Vacant(entry) = objects.entry(id) {
            entry.insert(read(id)?);
        }
    }
    let costs: NativeSimulationSettingsV1 =
        serde_json::from_slice(&objects[&request.mandate.constraints.transaction_costs_ref])?;
    ensure!(
        serde_json::to_value(&costs)? == serde_json::to_value(&request.execution_settings)?,
        "PORTFOLIO_EXECUTION_SETTINGS_SOURCE_MISMATCH"
    );
    if let Some(policy) = &request.rolling_liquidity {
        let original: NativeRollingBarLiquidityPolicyV1 = serde_json::from_slice(
            &objects[&request
                .mandate
                .constraints
                .liquidity_ref
                .ok_or_else(|| anyhow::anyhow!("STUDY_LIQUIDITY_POLICY_MISSING"))?],
        )?;
        ensure!(original == *policy, "STUDY_LIQUIDITY_POLICY_MISMATCH");
    }
    let mut models = request.members.clone();
    for member in &models {
        if let Some(id) = member.calibration_artifact_id {
            let model: NativeFrozenCalibrationV1 = serde_json::from_slice(&objects[&id])?;
            domain::execution::check_alpha_calibration(&model)?;
            ensure!(
                model.fit_end_available_ns <= request.research_available_through_ns,
                "STUDY_CALIBRATION_AFTER_RESEARCH"
            );
        }
    }
    for model in &mut models {
        model.parameters.total_fuel =
            count(model.parameters.total_fuel.get() / cutoffs.len() as u64)?;
    }
    let mut frames = Vec::new();
    let mut points = Vec::new();
    let mut consumed = 0_u64;
    let mut return_values = 0_usize;
    let ttl = u64::from(request.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000;
    // ponytail: bounded prefix recomputation; stream/cache frames if this measured cost limits throughput.
    for cutoff in cutoffs {
        let mut selection = request.source_selection.clone();
        selection.event_end_ns = cutoff;
        selection.decision_cutoff_ns = cutoff;
        let prepared = crate::portfolio::prepare(
            catalog,
            &selection,
            &request.mandate,
            &request.execution_settings,
            &request.assets,
            &models,
            |id| {
                objects
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("STUDY_MODEL_MISSING"))
            },
        )?;
        return_values = return_values
            .checked_add(prepared.returns.end_ns.len() * request.assets.len())
            .ok_or_else(|| anyhow::anyhow!("STUDY_INPUT_LIMIT"))?;
        ensure!(return_values <= MAX_RETURN_VALUES, "STUDY_INPUT_LIMIT");
        consumed = consumed
            .checked_add(prepared.consumed_fuel.get())
            .ok_or_else(|| anyhow::anyhow!("PORTFOLIO_FUEL_OVERFLOW"))?;
        let bar_notionals = if request.rolling_liquidity.is_some() {
            crate::catalog::last_bar_notionals(&crate::catalog::load_catalog(catalog, &selection)?)?
        } else {
            Vec::new()
        };
        let source_assets = domain::execution::portfolio_study_liquidity_assets(
            request,
            cutoff,
            prepared.forecasts.forecast_asof_ns,
            cutoff,
            &bar_notionals,
        )?;
        let assets = domain::execution::portfolio_costs(
            &selection,
            &request.mandate,
            &request.execution_settings,
            &source_assets,
            &prepared.slippage,
        )?;
        let input = crate::portfolio::allocation_input(
            &request.mandate,
            assets,
            "1".parse().unwrap(),
            &prepared,
        );
        frames.push(crate::simulation::StudyInput {
            cutoff_ns: cutoff,
            input,
            slippage: prepared.slippage,
            bar_notionals,
            liquidity_maximum_age: request
                .rolling_liquidity
                .as_ref()
                .map(|p| p.maximum_age_seconds),
        });
        // Schedule only. Actual weights are produced inside the native account before any order.
        points.push(NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: cutoff,
            valid_until_ns: count(cutoff.get() + ttl)?,
            cash_weight: "1".parse().unwrap(),
            targets: request
                .assets
                .iter()
                .map(|asset| AllocationTargetV1 {
                    instrument_id: asset.instrument_id.clone(),
                    currency: asset.currency.clone(),
                    weight: "0".parse().unwrap(),
                })
                .collect(),
        });
    }
    let mut selection = request.source_selection.clone();
    selection.event_start_ns = request.evaluation_start_ns;
    let mut replay = NativeSimulationRequestV1 {
        schema_version: SchemaV1,
        selection,
        settings: request.execution_settings.clone(),
        target_points: points,
    };
    let (simulation, frames) = crate::simulation::run(catalog, &replay, frames)?;
    let simulation_request = if simulation.is_some() {
        replay.target_points = frames
            .iter()
            .map(|frame| {
                Ok(NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    asof_ns: frame.input.forecasts.decision_asof_ns,
                    valid_until_ns: count(frame.cutoff_ns.get() + ttl)?,
                    targets: frame
                        .allocation
                        .targets
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("STUDY_TARGETS_MISSING"))?,
                    cash_weight: frame
                        .allocation
                        .cash_weight
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("STUDY_CASH_MISSING"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Some(replay)
    } else {
        None
    };
    let result = NativePortfolioStudyResultV1 {
        schema_version: SchemaV1,
        consumed_fuel: count(consumed)?,
        frames,
        simulation_request,
        simulation,
    };
    domain::execution::check_portfolio_study(request, &result)?;
    Ok(result)
}
