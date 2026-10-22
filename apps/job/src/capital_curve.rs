//! Bounded fresh-account native replay, not a rescaling of previously observed fills.
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, RoundingMode, Zero};
use contracts::{capital_curve::*, DbCounter, DecimalValue, Id, SchemaV1};
use nautilus_model::instruments::Instrument;
use std::{collections::BTreeSet, path::Path, time::Instant};

fn decimal(value: BigDecimal, rounding: RoundingMode) -> Result<DecimalValue> {
    value
        .with_scale_round(18, rounding)
        .normalized()
        .to_plain_string()
        .parse()
        .map_err(anyhow::Error::msg)
}

fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

fn elapsed(start: Instant) -> Result<DbCounter> {
    count(u64::try_from(start.elapsed().as_millis())?)
}

fn minimum_sources(
    catalog: &Path,
    request: &NativeCapitalCurveRequestV1,
) -> Result<Vec<NativeCapitalMinimumSourceV1>> {
    let data = crate::catalog::load_catalog(catalog, &request.simulation.selection)?;
    let mut entered = BTreeSet::new();
    let mut sources = Vec::new();
    for point in &request.simulation.target_points {
        domain::execution::strategy::target_scope(point)?;
        for target in &point.targets {
            if target.weight.as_decimal().is_zero()
                || point
                    .rebalance_instrument_ids
                    .as_ref()
                    .is_some_and(|ids| !ids.contains(&target.instrument_id))
                || entered.contains(&target.instrument_id)
            {
                continue;
            }
            ensure!(
                target.currency == request.capital_basis.currency,
                "CAPITAL_MINIMUM_CURRENCY_MISMATCH"
            );
            let series = data
                .series
                .iter()
                .find(|row| row.instrument.id().to_string() == target.instrument_id)
                .ok_or_else(|| anyhow::anyhow!("CAPITAL_MINIMUM_INSTRUMENT_MISSING"))?;
            let bar = series
                .bars
                .iter()
                .filter(|bar| {
                    bar.ts_init.as_u64() <= point.asof_ns.get()
                        && bar.ts_event.as_u64() <= point.asof_ns.get()
                })
                .max_by_key(|bar| (bar.ts_init, bar.ts_event))
                .ok_or_else(|| anyhow::anyhow!("CAPITAL_MINIMUM_CAUSAL_PRICE_MISSING"))?;
            let instrument = series.instrument_at(point.asof_ns.get())?;
            let lot = instrument.try_calculate_notional_value(
                instrument.size_increment(),
                bar.close,
                Some(false),
            )?;
            let minimum = instrument.try_calculate_notional_value(
                instrument
                    .min_quantity()
                    .unwrap_or(instrument.size_increment()),
                bar.close,
                Some(false),
            )?;
            ensure!(
                lot.currency.to_string() == target.currency
                    && minimum.currency.to_string() == target.currency,
                "CAPITAL_MINIMUM_CURRENCY_MISMATCH"
            );
            let mut notional = lot
                .as_decimal()
                .to_string()
                .parse::<BigDecimal>()?
                .max(minimum.as_decimal().to_string().parse()?);
            if let Some(native) = instrument.min_notional() {
                ensure!(
                    native.currency.to_string() == target.currency,
                    "CAPITAL_MINIMUM_CURRENCY_MISMATCH"
                );
                notional = notional.max(native.as_decimal().to_string().parse()?);
            }
            ensure!(
                notional > BigDecimal::zero(),
                "CAPITAL_MINIMUM_NOT_POSITIVE"
            );
            sources.push(NativeCapitalMinimumSourceV1 {
                instrument_id: target.instrument_id.clone(),
                currency: target.currency.clone(),
                decision_ns: point.asof_ns,
                bar_event_ns: count(bar.ts_event.as_u64())?,
                bar_available_ns: count(bar.ts_init.as_u64())?,
                instrument_available_ns: count(instrument.ts_init().as_u64())?,
                candidate_capital: decimal(
                    &notional / target.weight.as_decimal().abs(),
                    RoundingMode::Up,
                )?,
                minimum_notional: decimal(notional, RoundingMode::Up)?,
                entry_weight: target.weight.clone(),
            });
            entered.insert(target.instrument_id.clone());
        }
    }
    ensure!(!sources.is_empty(), "CAPITAL_MINIMUM_NO_NONZERO_ENTRY");
    Ok(sources)
}

/// Current fundable capital first, then an identifiable first-entry candidate.
/// Remaining allowance bisects the widest planned numeric gap. A zero anchor
/// permits exploration when a frozen-target first-entry candidate is unavailable
/// or unfunded; zero itself is never replayed. No feasibility is interpolated.
fn plan(capital: &DecimalValue, anchor: &DecimalValue, maximum: u32) -> Result<Vec<DecimalValue>> {
    let mut points = vec![capital.clone()];
    if maximum > 1 && anchor.is_positive() && anchor < capital {
        points.push(anchor.clone());
    }
    while points.len() < maximum as usize {
        let mut ordered = points.clone();
        if !ordered.contains(anchor) {
            ordered.push(anchor.clone());
        }
        ordered.sort();
        let Some(pair) = ordered.windows(2).max_by(|left, right| {
            (left[1].as_decimal() - left[0].as_decimal())
                .cmp(&(right[1].as_decimal() - right[0].as_decimal()))
        }) else {
            break;
        };
        let midpoint = decimal(
            (pair[0].as_decimal() + pair[1].as_decimal()) / BigDecimal::from(2),
            RoundingMode::Down,
        )?;
        if midpoint <= pair[0] || midpoint >= pair[1] {
            break;
        }
        points.push(midpoint);
    }
    Ok(points)
}

pub fn evaluate(
    catalog: &Path,
    request: &NativeCapitalCurveRequestV1,
    dataset_revision_id: Option<Id>,
) -> Result<NativeCapitalCurveResultV1> {
    run(catalog, request, dataset_revision_id, false)
}

/// Native catalog planning for a caller which will perform a fresh allocation
/// and shared-account study at every amount. Planned amounts are not observations.
pub fn plan_only(
    catalog: &Path,
    request: &NativeCapitalCurveRequestV1,
    dataset_revision_id: Option<Id>,
) -> Result<NativeCapitalCurveResultV1> {
    run(catalog, request, dataset_revision_id, true)
}

fn run(
    catalog: &Path,
    request: &NativeCapitalCurveRequestV1,
    dataset_revision_id: Option<Id>,
    plan_only: bool,
) -> Result<NativeCapitalCurveResultV1> {
    let started = Instant::now();
    let basis = &request.capital_basis;
    ensure!(
        basis.currency == request.simulation.settings.base_currency,
        "CAPITAL_BASIS_CURRENCY_MISMATCH"
    );
    ensure!(
        basis.equity.is_nonnegative()
            && basis.reserved.is_nonnegative()
            && basis.owner_liquidity_reserve.is_nonnegative(),
        "CAPITAL_BASIS_NEGATIVE"
    );
    let deployable = basis.equity.as_decimal()
        - basis.reserved.as_decimal()
        - basis.owner_liquidity_reserve.as_decimal();
    ensure!(
        deployable >= BigDecimal::zero(),
        "CAPITAL_RESERVES_EXCEED_EQUITY"
    );
    ensure!(
        (1..=64).contains(&request.budget.maximum_simulations)
            && request.budget.maximum_elapsed_ms.get() > 0,
        "CAPITAL_RESOURCE_BUDGET_INVALID"
    );
    ensure!(
        !request.simulation.target_points.is_empty()
            && request
                .simulation
                .target_points
                .windows(2)
                .all(|pair| pair[0].asof_ns < pair[1].asof_ns),
        "CAPITAL_TARGET_CLOCKS_INVALID"
    );
    for point in &request.simulation.target_points {
        ensure!(
            point.asof_ns < point.valid_until_ns,
            "CAPITAL_TARGET_INVALID"
        );
        domain::execution::strategy::target_scope(point)?;
        let mut identities = BTreeSet::new();
        let mut sum = point.cash_weight.as_decimal().clone();
        let mut gross = BigDecimal::zero();
        for target in &point.targets {
            ensure!(
                target.currency == basis.currency && identities.insert(&target.instrument_id),
                "CAPITAL_TARGET_INVALID"
            );
            sum += target.weight.as_decimal();
            gross += target.weight.as_decimal().abs();
        }
        let tolerance = request.simulation.settings.exposure_tolerance.as_decimal();
        ensure!(
            (point.rebalance_instrument_ids.is_none() || sum == BigDecimal::from(1))
                && (sum - BigDecimal::from(1)).abs() <= *tolerance
                && gross <= request.simulation.settings.leverage.as_decimal() + tolerance,
            "CAPITAL_TARGET_SUM_OR_LEVERAGE_INVALID"
        );
    }
    if request
        .simulation
        .settings
        .multi_currency_spot_cash
        .is_some()
    {
        domain::spot_cash::execution_settings(&request.simulation.settings)?;
        ensure!(
            dataset_revision_id.is_some(),
            "SPOT_DATASET_REVISION_ID_REQUIRED"
        );
    } else {
        domain::portfolio::simulation_settings(&request.simulation.settings)?;
    }
    let deployable_capital = decimal(deployable, RoundingMode::Down)?;
    let mut report = NativeCapitalCurveResultV1 {
        schema_version: SchemaV1,
        request: request.clone(),
        dataset_revision_id,
        deployable_capital: deployable_capital.clone(),
        generation: NativeCapitalCurveGenerationV1 {
            method: "CURRENT_CAPITAL_FIRST_ENTRY_OR_ZERO_ANCHOR_WIDEST_GAP_V2".into(),
            minimum_candidate_capital: None,
            minimum_reason_code: None,
            minimum_sources: vec![],
            lower_planning_anchor: DecimalValue::zero(),
            planned_capitals: vec![],
        },
        points: vec![],
        resources: NativeCapitalCurveResourcesV1 {
            attempted_simulations: 0,
            completed_simulations: 0,
            failed_simulations: 0,
            elapsed_ms: DbCounter::ZERO,
            completed_native_events: DbCounter::ZERO,
        },
        termination: NativeCapitalCurveTerminationV1::Completed,
        economic_capacity_status: NativeCapitalEconomicStatusV1::Unavailable,
        limitations: vec![
            "FRESH_ACCOUNT_FROZEN_TARGET_PATH_NOT_EXISTING_HOLDINGS_OR_REOPTIMIZATION".into(),
            "FIRST_ENTRY_MINIMUM_IS_CANDIDATE_EXCLUDES_FEES_AND_LATER_DRIFT".into(),
            "COMPLETED_REPLAY_IS_NOT_TARGET_ACHIEVEMENT_OR_ECONOMIC_FEASIBILITY".into(),
            "NO_INTERPOLATION_MONOTONICITY_OR_CAPACITY_UPPER_BOUND".into(),
            "NO_IDENTIFIED_SCALE_IMPACT_SPREAD_AND_EXIT_MODEL".into(),
            "ELAPSED_BUDGET_CONTROLS_ADMISSION_NOT_IN_FLIGHT_CANCELLATION".into(),
            "FAILED_RUN_NATIVE_EVENTS_CPU_MEMORY_AND_FUEL_UNMETERED".into(),
        ],
    };
    if !deployable_capital.is_positive() {
        report.termination = NativeCapitalCurveTerminationV1::NoDeployableCapital;
        report.generation.minimum_reason_code = Some("NO_DEPLOYABLE_CAPITAL".into());
        report.resources.elapsed_ms = elapsed(started)?;
        return Ok(report);
    }
    match minimum_sources(catalog, request) {
        Ok(sources) => {
            report.generation.minimum_candidate_capital = sources
                .iter()
                .map(|source| source.candidate_capital.clone())
                .max();
            report.generation.minimum_sources = sources;
        }
        Err(error) => report.generation.minimum_reason_code = Some(format!("{error:#}")),
    }
    if let Some(minimum) = &report.generation.minimum_candidate_capital {
        if minimum.is_positive() && minimum < &deployable_capital {
            report.generation.lower_planning_anchor = minimum.clone();
        } else {
            report.generation.minimum_reason_code =
                Some("CAPITAL_MINIMUM_NOT_A_FUNDED_LOWER_ANCHOR".into());
        }
    }
    report.generation.planned_capitals = plan(
        &deployable_capital,
        &report.generation.lower_planning_anchor,
        request.budget.maximum_simulations,
    )?;
    if plan_only {
        report.termination = NativeCapitalCurveTerminationV1::PlannedOnly;
        report.resources.elapsed_ms = elapsed(started)?;
        return Ok(report);
    }
    for capital in &report.generation.planned_capitals {
        if started.elapsed().as_millis() >= u128::from(request.budget.maximum_elapsed_ms.get()) {
            report.termination = NativeCapitalCurveTerminationV1::TimeBudgetExhausted;
            break;
        }
        let mut simulation = request.simulation.clone();
        simulation.settings.starting_capital = capital.clone();
        report.resources.attempted_simulations += 1;
        let point_started = Instant::now();
        let outcome =
            crate::simulation::simulate_explicit(catalog, &simulation, dataset_revision_id);
        let duration = elapsed(point_started)?;
        let (status, result, reason_code) = match outcome {
            Ok(result) => {
                report.resources.completed_simulations += 1;
                report.resources.completed_native_events = count(
                    report
                        .resources
                        .completed_native_events
                        .get()
                        .checked_add(result.events.get())
                        .ok_or_else(|| anyhow::anyhow!("CAPITAL_EVENT_COUNTER_OVERFLOW"))?,
                )?;
                (NativeCapitalReplayStatusV1::Completed, Some(result), None)
            }
            Err(error) => {
                report.resources.failed_simulations += 1;
                (
                    NativeCapitalReplayStatusV1::Failed,
                    None,
                    Some(format!("{error:#}")),
                )
            }
        };
        report.points.push(NativeCapitalCurvePointV1 {
            capital: capital.clone(),
            kind: NativeCapitalPointKindV1::TestedPoint,
            lower: capital.clone(),
            upper: capital.clone(),
            status,
            elapsed_ms: duration,
            result,
            reason_code,
        });
    }
    report.resources.elapsed_ms = elapsed(started)?;
    Ok(report)
}
