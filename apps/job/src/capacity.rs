//! Conditional executable capital. Original BAR quantities are not future depth.
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, RoundingMode, Zero};
use contracts::{capacity::*, DbCounter, DecimalValue};
use nautilus_model::instruments::Instrument;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn decimal(value: BigDecimal) -> Result<DecimalValue> {
    value
        .with_scale_round(18, RoundingMode::Down)
        .to_plain_string()
        .parse()
        .map_err(anyhow::Error::msg)
}
fn ceil(value: BigDecimal) -> Result<DecimalValue> {
    value
        .with_scale_round(18, RoundingMode::Up)
        .to_plain_string()
        .parse()
        .map_err(anyhow::Error::msg)
}
#[derive(Clone, Default)]
struct Demand {
    instrument: String,
    change: BigDecimal,
    exit: BigDecimal,
    liquidity: BigDecimal,
    minimum: BigDecimal,
    decision_ns: u64,
    exit_decision_ns: u64,
    bar_event_ns: u64,
    bar_available_ns: u64,
}
fn envelope(request: &NativeCapacityRequestV1) -> NativeCapacityReportV1 {
    NativeCapacityReportV1 {
        status: "UNAVAILABLE".into(), reason: None, scope: "portfolio".into(),
        currency: request.simulation.settings.base_currency.clone(), basis: "HISTORICAL_BAR_ESTIMATE".into(),
        maximum_capital: None, minimum_capital: None, binding_instrument: None,
        observation_start_ns: None, observation_end_ns: None,
        decision_clock_ns: request.simulation.target_points.last().map(|point| point.asof_ns),
        valid_until_ns: request.simulation.target_points.last().map(|point| point.valid_until_ns),
        assumptions: NativeCapacityAssumptionsV1 { participation: request.policy.participation_limit.clone(), maximum_age_seconds: request.policy.maximum_age_seconds,
            execution_window: "LAST_AVAILABLE_BAR; cumulative absolute net-target changes share each original BAR budget; initial cash establishment".into(),
            exit: "Standalone full liquidation of retained target exposure at each decision; alternative to, not added to, strategy turnover; future depth unknown".into(),
            fees: "Original native simulation settings retained, but this liquidity ceiling does not estimate execution costs or after-cost profitability".into(),
            depth: "BAR_NOTIONAL_ONLY; no orderbook depth, impact calibration or confidence interval".into() }, scenarios: Vec::new(),
        methodology: Some(NativeCapacityMethodologyV1 {
            id: "HISTORICAL_BAR_PARTICIPATION_V2".into(),
            formula: "C_max = min_(instrument,BAR) participation * observedNotional / max(sum(abs(netTargetChange)), max(retainedTargetWeight))".into(),
            notes: vec![
                "Static capital and target-weight approximation, not native fill/P&L/holding-drift reconstruction".into(),
                "Only original event/available clocks at or before the decision; no future volume or replenishment of reused BARs".into(),
                "Scoped decisions retain out-of-scope holdings; native replay requires complete target vectors, including explicit zero targets for liquidation".into(),
                "Turnover is peak per instrument/BAR; full exit is a separate alternative path, not a simultaneous round trip".into(),
                "Volume multiplier 1 is baseline; 0.5 is assumed reduced-liquidity sensitivity, not an estimated probability".into(),
            ],
        }),
        constraints: Some(Vec::new()),
        coverage: Some(NativeCapacityCoverageV1 { decision_count: request.simulation.target_points.len(), ..Default::default() }),
        volume_stress: Some(Vec::new()),
        economic_capacity: Some(NativeCapacityEconomicV1 {
            status: "UNAVAILABLE".into(), maximum_capital: None, reason: "required_economic_inputs_missing".into(),
            missing_inputs: ["matched_out_of_sample_returns_at_capital_scales", "venue_calibrated_market_impact", "matched_spread_fees_and_execution_timing", "validated_exit_execution"].map(String::from).to_vec(),
        }),
    }
}
fn unavailable(
    mut report: NativeCapacityReportV1,
    capitals: &[DecimalValue],
    reason: &str,
) -> NativeCapacityReportV1 {
    report.reason = Some(reason.into());
    report.scenarios = capitals
        .iter()
        .map(|capital| NativeCapacityScenarioV1 {
            capital: capital.clone(),
            required_turnover: None,
            maximum_participation: None,
            usable_capital: None,
            unallocated_cash: None,
            status: "UNAVAILABLE".into(),
            reason: Some(reason.into()),
            entry_rebalance_turnover: None,
            exit_turnover: None,
            entry_rebalance_participation: None,
            exit_participation: None,
        })
        .collect();
    report.volume_stress = Some(
        ["1", "0.5"]
            .map(|multiplier| NativeCapacityVolumeStressV1 {
                volume_multiplier: multiplier.parse().expect("constant decimal"),
                maximum_capital: None,
                status: report.status.clone(),
                reason: report.reason.clone(),
                scenarios: report.scenarios.clone(),
            })
            .to_vec(),
    );
    report
}
fn scenarios(
    capitals: &[DecimalValue],
    budgets: &[Demand],
    upper: Option<&BigDecimal>,
    lower: &BigDecimal,
    volume_multiplier: &BigDecimal,
) -> Result<Vec<NativeCapacityScenarioV1>> {
    capitals
        .iter()
        .map(|capital| {
            let mut turnover = [BigDecimal::zero(), BigDecimal::zero()];
            let mut participation = turnover.clone();
            let mut zero_liquidity = [false, false];
            for demand in budgets {
                let liquidity = &demand.liquidity * volume_multiplier;
                for (index, weight) in [&demand.change, &demand.exit].into_iter().enumerate() {
                    let trade = capital.as_decimal() * weight;
                    turnover[index] = turnover[index].clone().max(trade.clone());
                    if liquidity.is_zero() {
                        zero_liquidity[index] |= !weight.is_zero();
                    } else {
                        participation[index] = participation[index].clone().max(trade / &liquidity);
                    }
                }
            }
            let feasible = upper.map(|bound| capital.as_decimal().clone().min(bound.clone()));
            let tiny = feasible.as_ref().is_some_and(|amount| amount < lower);
            let usable = feasible
                .map(|amount| decimal(if tiny { BigDecimal::zero() } else { amount }))
                .transpose()?;
            let missing = zero_liquidity.iter().any(|value| *value);
            Ok(NativeCapacityScenarioV1 {
                capital: capital.clone(),
                required_turnover: Some(decimal(turnover[0].clone().max(turnover[1].clone()))?),
                maximum_participation: (!missing)
                    .then(|| ceil(participation[0].clone().max(participation[1].clone())))
                    .transpose()?,
                unallocated_cash: usable
                    .as_ref()
                    .map(|amount| decimal(capital.as_decimal() - amount.as_decimal()))
                    .transpose()?,
                usable_capital: usable,
                status: if missing {
                    "CONSTRAINED"
                } else if tiny {
                    "TOO_SMALL"
                } else if upper.is_some_and(|bound| capital.as_decimal() > bound) {
                    "CONSTRAINED"
                } else if upper.is_none() {
                    "NON_BINDING"
                } else {
                    "FEASIBLE"
                }
                .into(),
                reason: if missing {
                    Some("zero_observed_liquidity".into())
                } else if tiny {
                    Some("minimum_order_or_lot".into())
                } else {
                    None
                },
                entry_rebalance_turnover: Some(decimal(turnover[0].clone())?),
                exit_turnover: Some(decimal(turnover[1].clone())?),
                entry_rebalance_participation: (!zero_liquidity[0])
                    .then(|| ceil(participation[0].clone()))
                    .transpose()?,
                exit_participation: (!zero_liquidity[1])
                    .then(|| ceil(participation[1].clone()))
                    .transpose()?,
            })
        })
        .collect()
}
fn finish(
    mut report: NativeCapacityReportV1,
    request: &NativeCapacityRequestV1,
    demands: &[Demand],
) -> Result<NativeCapacityReportV1> {
    let mut buckets = BTreeMap::new();
    let mut lower = BigDecimal::zero();
    for demand in demands {
        for weight in [&demand.change, &demand.exit] {
            if !weight.is_zero() {
                lower = lower.max(&demand.minimum / weight);
            }
        }
        let bucket = buckets
            .entry((
                demand.instrument.clone(),
                demand.bar_event_ns,
                demand.bar_available_ns,
            ))
            .or_insert_with(|| Demand {
                change: BigDecimal::zero(),
                exit: BigDecimal::zero(),
                ..demand.clone()
            });
        bucket.change += &demand.change;
        if !demand.change.is_zero() {
            bucket.decision_ns = demand.decision_ns;
        }
        if demand.exit > bucket.exit {
            bucket.exit = demand.exit.clone();
            bucket.exit_decision_ns = demand.exit_decision_ns;
        }
        bucket.liquidity = bucket.liquidity.clone().min(demand.liquidity.clone());
    }
    let budgets: Vec<_> = buckets.into_values().collect();
    let mut constraints = Vec::new();
    let mut upper: Option<BigDecimal> = None;
    for kind in [
        NativeCapacityConstraintKindV1::EntryRebalance,
        NativeCapacityConstraintKindV1::FullExit,
    ] {
        let mut binding: Option<(&Demand, BigDecimal, &BigDecimal, u64)> = None;
        for demand in &budgets {
            let (weight, clock) = match kind {
                NativeCapacityConstraintKindV1::EntryRebalance => {
                    (&demand.change, demand.decision_ns)
                }
                NativeCapacityConstraintKindV1::FullExit => (&demand.exit, demand.exit_decision_ns),
            };
            if weight.is_zero() {
                continue;
            }
            let cap = &demand.liquidity * request.policy.participation_limit.as_decimal() / weight;
            if binding.as_ref().is_none_or(|(_, bound, _, _)| cap < *bound) {
                binding = Some((demand, cap, weight, clock));
            }
        }
        if let Some((demand, cap, weight, clock)) = binding {
            if upper.as_ref().is_none_or(|bound| cap < *bound) {
                upper = Some(cap.clone());
                report.binding_instrument = Some(demand.instrument.clone());
            }
            constraints.push(NativeCapacityConstraintV1 {
                kind,
                maximum_capital: decimal(cap)?,
                instrument_id: demand.instrument.clone(),
                decision_ns: DbCounter::new(clock).map_err(anyhow::Error::msg)?,
                bar_event_ns: DbCounter::new(demand.bar_event_ns).map_err(anyhow::Error::msg)?,
                bar_available_ns: DbCounter::new(demand.bar_available_ns)
                    .map_err(anyhow::Error::msg)?,
                observed_notional: decimal(demand.liquidity.clone())?,
                required_weight: decimal(weight.clone())?,
            });
        }
    }
    report.status = if upper.is_some() {
        "ESTIMATED"
    } else {
        "NON_BINDING"
    }
    .into();
    report.reason = upper.is_none().then(|| "no_nonzero_trade_demand".into());
    report.maximum_capital = upper.clone().map(decimal).transpose()?;
    let minimum = ceil(lower)?;
    let lower = minimum.as_decimal();
    report.minimum_capital = (!demands.is_empty()).then_some(minimum.clone());
    report.constraints = Some(constraints);
    let mut capitals = request.capitals.clone();
    for multiplier in ["0.1", "1", "10"] {
        let value = decimal(
            request.simulation.settings.starting_capital.as_decimal()
                * multiplier.parse::<BigDecimal>()?,
        )?;
        if value.is_positive() && !capitals.contains(&value) {
            capitals.push(value);
        }
    }
    report.scenarios = scenarios(
        &capitals,
        &budgets,
        report
            .maximum_capital
            .as_ref()
            .map(DecimalValue::as_decimal),
        lower,
        &BigDecimal::from(1),
    )?;
    let mut stress = Vec::new();
    for multiplier in ["1", "0.5"] {
        let multiplier = multiplier.parse::<BigDecimal>()?;
        let bound = upper
            .as_ref()
            .map(|value| decimal(value * &multiplier))
            .transpose()?;
        stress.push(NativeCapacityVolumeStressV1 {
            volume_multiplier: decimal(multiplier.clone())?,
            maximum_capital: bound.clone(),
            status: report.status.clone(),
            reason: report.reason.clone(),
            scenarios: scenarios(
                &capitals,
                &budgets,
                bound.as_ref().map(DecimalValue::as_decimal),
                lower,
                &multiplier,
            )?,
        });
    }
    report.volume_stress = Some(stress);
    Ok(report)
}

pub fn assess(catalog: &Path, request: &NativeCapacityRequestV1) -> Result<NativeCapacityReportV1> {
    domain::portfolio::rolling_liquidity_policy(&request.policy)?;
    ensure!(!request.capitals.is_empty(), "capacity_scenarios_required");
    ensure!(
        request.capitals.iter().all(DecimalValue::is_positive),
        "capacity_capital_invalid"
    );
    let mut report = envelope(request);
    let data = crate::catalog::load_catalog(catalog, &request.simulation.selection)?;
    let points = &request.simulation.target_points;
    ensure!(
        !points.is_empty()
            && points
                .windows(2)
                .all(|pair| pair[0].asof_ns < pair[1].asof_ns),
        "capacity_decisions_invalid"
    );
    let mut previous = BTreeMap::<String, BigDecimal>::new();
    let mut schedule = Vec::new();
    for point in points {
        ensure!(
            point.asof_ns < point.valid_until_ns && point.cash_weight.is_fraction(),
            "capacity_target_invalid"
        );
        // Native replay requires the complete ordered universe, even for scoped points.
        // A dropped row is not an executable zero target or a liquidation instruction.
        if point.targets.len() != data.series.len()
            || point
                .targets
                .iter()
                .zip(&data.series)
                .any(|(target, series)| target.instrument_id != series.instrument.id().to_string())
        {
            return Ok(unavailable(
                report,
                &request.capitals,
                "capacity_target_identity_mismatch",
            ));
        }
        if domain::execution::strategy::target_scope(point).is_err() {
            return Ok(unavailable(
                report,
                &request.capitals,
                "capacity_target_scope_invalid",
            ));
        }
        let mut total = point.cash_weight.as_decimal().clone();
        let mut observations = Vec::new();
        for (index, target) in point.targets.iter().enumerate() {
            ensure!(target.weight.is_fraction(), "capacity_target_invalid");
            total += target.weight.as_decimal();
            if target.currency != report.currency {
                return Ok(unavailable(report, &request.capitals, "currency_mismatch"));
            }
            let old = previous
                .get(&target.instrument_id)
                .cloned()
                .unwrap_or_default();
            let weight = if point
                .rebalance_instrument_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&target.instrument_id))
            {
                target.weight.as_decimal().clone()
            } else {
                old.clone()
            };
            let change = (&weight - old).abs();
            previous.insert(target.instrument_id.clone(), weight.clone());
            if change.is_zero() && weight.is_zero() {
                continue;
            }
            observations.push((
                index,
                Demand {
                    instrument: target.instrument_id.clone(),
                    change,
                    exit: weight,
                    decision_ns: point.asof_ns.get(),
                    exit_decision_ns: point.asof_ns.get(),
                    ..Default::default()
                },
            ));
        }
        ensure!(total == BigDecimal::from(1), "capacity_target_sum_invalid");
        schedule.push(observations);
    }
    report.coverage.as_mut().unwrap().required_observations = schedule.iter().map(Vec::len).sum();
    let mut demands = Vec::new();
    let mut observed_bars = BTreeSet::new();
    let age = u64::from(request.policy.maximum_age_seconds) * 1_000_000_000;
    for (point, observations) in points.iter().zip(schedule) {
        for (index, mut demand) in observations {
            let series = &data.series[index];
            let Some(bar) = series
                .bars
                .iter()
                .filter(|bar| {
                    bar.ts_init.as_u64() <= point.asof_ns.get()
                        && bar.ts_event.as_u64() <= point.asof_ns.get()
                })
                .max_by_key(|bar| (bar.ts_init, bar.ts_event))
            else {
                return Ok(unavailable(
                    report,
                    &request.capitals,
                    "available_at_liquidity_missing",
                ));
            };
            if point.asof_ns.get().saturating_sub(bar.ts_event.as_u64()) >= age {
                return Ok(unavailable(report, &request.capitals, "liquidity_stale"));
            }
            let instrument = series.instrument_at(point.asof_ns.get())?;
            let notional =
                instrument.try_calculate_notional_value(bar.volume, bar.close, Some(false))?;
            if notional.currency.to_string() != report.currency {
                return Ok(unavailable(report, &request.capitals, "currency_mismatch"));
            }
            let lot = instrument.try_calculate_notional_value(
                instrument.size_increment(),
                bar.close,
                Some(false),
            )?;
            let minimum_qty = instrument
                .min_quantity()
                .unwrap_or(instrument.size_increment());
            let minimum =
                instrument.try_calculate_notional_value(minimum_qty, bar.close, Some(false))?;
            let mut minimum = minimum
                .as_decimal()
                .to_string()
                .parse::<BigDecimal>()?
                .max(lot.as_decimal().to_string().parse()?);
            if let Some(native) = instrument.min_notional() {
                if native.currency.to_string() != report.currency {
                    return Ok(unavailable(
                        report,
                        &request.capitals,
                        "minimum_order_currency_mismatch",
                    ));
                }
                minimum = minimum.max(native.as_decimal().to_string().parse()?);
            }
            let observed = DbCounter::new(bar.ts_event.as_u64()).map_err(anyhow::Error::msg)?;
            report.observation_start_ns = Some(
                report
                    .observation_start_ns
                    .map_or(observed, |old| old.min(observed)),
            );
            report.observation_end_ns = Some(
                report
                    .observation_end_ns
                    .map_or(observed, |old| old.max(observed)),
            );
            demand.liquidity = notional.as_decimal().to_string().parse()?;
            demand.minimum = minimum;
            demand.bar_event_ns = bar.ts_event.as_u64();
            demand.bar_available_ns = bar.ts_init.as_u64();
            observed_bars.insert((
                demand.instrument.clone(),
                demand.bar_event_ns,
                demand.bar_available_ns,
            ));
            demands.push(demand);
            let coverage = report.coverage.as_mut().unwrap();
            coverage.covered_observations += 1;
            coverage.unique_bars = observed_bars.len();
            coverage.reused_bar_observations = coverage.covered_observations - coverage.unique_bars;
        }
        report.coverage.as_mut().unwrap().assessed_decision_count += 1;
    }
    finish(report, request, &demands)
}

pub fn size_account(request: &NativeAccountSizingRequestV1) -> Result<NativeAccountSizingResultV1> {
    ensure!(
        request.equity.is_nonnegative()
            && request.reserved.is_nonnegative()
            && request.cash_buffer.is_nonnegative()
            && request.maximum_capital.is_nonnegative()
            && request.participation.is_positive()
            && request.participation.is_fraction()
            && request.maximum_age_seconds > 0,
        "sizing_budget_invalid"
    );
    let budget = (request.equity.as_decimal()
        - request.reserved.as_decimal()
        - request.cash_buffer.as_decimal())
    .max(BigDecimal::zero());
    let mut capital = budget
        .clone()
        .min(request.maximum_capital.as_decimal().clone());
    let mut result = NativeAccountSizingResultV1 {
        status: "BLOCKED".into(),
        reason: None,
        budget: decimal(budget.clone())?,
        capital: DecimalValue::zero(),
        residual_cash: request.equity.clone(),
        target: None,
        members: Vec::new(),
    };
    macro_rules! blocked {
        ($reason:expr) => {{
            result.reason = Some($reason.into());
            return Ok(result);
        }};
    }
    let member_weights = request
        .members
        .iter()
        .map(|row| {
            row.weight.as_decimal()
                * row
                    .pool_weight
                    .as_ref()
                    .map_or(BigDecimal::from(1), |weight| weight.as_decimal().clone())
        })
        .collect::<Vec<_>>();
    ensure!(
        member_weights.iter().sum::<BigDecimal>() == BigDecimal::from(1)
            && request.members.iter().all(|row| row.weight.is_positive()
                && row
                    .pool_weight
                    .as_ref()
                    .is_none_or(DecimalValue::is_positive))
            && request
                .members
                .iter()
                .map(|row| &row.factor_id)
                .collect::<BTreeSet<_>>()
                .len()
                == request.members.len(),
        "sizing_member_weights_invalid"
    );
    if !request.target.cash_weight.is_fraction()
        || request.target.cash_weight.as_decimal()
            + request
                .target
                .targets
                .iter()
                .map(|row| row.weight.as_decimal())
                .sum::<BigDecimal>()
            != BigDecimal::from(1)
        || request
            .target
            .targets
            .iter()
            .any(|row| row.currency != request.currency || !row.weight.is_fraction())
        || request
            .target
            .targets
            .iter()
            .map(|row| &row.instrument_id)
            .collect::<BTreeSet<_>>()
            .len()
            != request.target.targets.len()
    {
        blocked!("target_sum_or_currency_invalid");
    }
    let ids: BTreeSet<_> = request
        .target
        .targets
        .iter()
        .map(|target| &target.instrument_id)
        .collect();
    if request
        .holdings
        .iter()
        .any(|holding| !ids.contains(&holding.instrument_id) || !holding.value.is_nonnegative())
    {
        blocked!("holdings_outside_approved_pool");
    }
    let holdings: BTreeMap<_, _> = request
        .holdings
        .iter()
        .map(|holding| (&holding.instrument_id, holding.value.as_decimal()))
        .collect();
    if holdings.len() != request.holdings.len() {
        blocked!("holdings_duplicate");
    }
    let rebalances = |instrument: &str| {
        request
            .target
            .rebalance_instrument_ids
            .as_ref()
            .is_none_or(|ids| ids.iter().any(|id| id == instrument))
    };
    if request.projection_only {
        if request
            .minimum_capital
            .as_ref()
            .is_some_and(|minimum| capital < *minimum.as_decimal())
        {
            capital = BigDecimal::zero();
        }
        let mut target = request.target.clone();
        let mut invested = BigDecimal::zero();
        for weight in &mut target.targets {
            let notional = if rebalances(&weight.instrument_id) {
                &capital * weight.weight.as_decimal()
            } else {
                holdings
                    .get(&weight.instrument_id)
                    .copied()
                    .cloned()
                    .unwrap_or_default()
            };
            weight.weight = if request.equity.is_positive() {
                decimal(&notional / request.equity.as_decimal())?
            } else {
                DecimalValue::zero()
            };
            invested += notional;
        }
        if invested > capital {
            blocked!("retained_holdings_exceed_capacity_budget");
        }
        target.cash_weight = decimal(
            BigDecimal::from(1)
                - target
                    .targets
                    .iter()
                    .map(|weight| weight.weight.as_decimal())
                    .sum::<BigDecimal>(),
        )?;
        result.residual_cash = decimal(request.equity.as_decimal() - invested)?;
        result.members = request
            .members
            .iter()
            .zip(&member_weights)
            .map(|(row, weight)| {
                Ok(NativeSizedMemberV1 {
                    factor_id: row.factor_id.clone(),
                    capital: decimal(&capital * weight)?,
                    scale: if budget.is_zero() {
                        DecimalValue::zero()
                    } else {
                        decimal(&capital / &budget)?
                    },
                })
            })
            .collect::<Result<_>>()?;
        result.capital = decimal(capital)?;
        result.target = Some(target);
        result.status = "PROJECTED".into();
        result.reason = Some(
            if request.target.valid_until_ns <= request.clock_ns {
                "source_target_expired"
            } else {
                "current_execution_capability_or_evidence_unavailable"
            }
            .into(),
        );
        return Ok(result);
    }
    if request.target.valid_until_ns <= request.clock_ns
        || request.target.asof_ns > request.clock_ns
    {
        blocked!("source_target_expired_or_future");
    }
    let mut fees = BigDecimal::zero();
    let mut total = request.target.cash_weight.as_decimal().clone();
    let mut lower = BigDecimal::zero();
    for target in &request.target.targets {
        if target.currency != request.currency || !target.weight.is_fraction() {
            blocked!("currency_or_weight_mismatch");
        }
        total += target.weight.as_decimal();
        let current = holdings
            .get(&target.instrument_id)
            .copied()
            .cloned()
            .unwrap_or_default();
        if !rebalances(&target.instrument_id) || !target.weight.is_positive() && current.is_zero() {
            continue;
        }
        let Some(liquidity) = request
            .liquidity
            .iter()
            .find(|row| row.instrument_id == target.instrument_id)
        else {
            blocked!("current_liquidity_missing");
        };
        if request
            .liquidity
            .iter()
            .filter(|row| row.instrument_id == target.instrument_id)
            .count()
            != 1
        {
            blocked!("liquidity_duplicate");
        }
        if liquidity.currency != request.currency
            || liquidity.available_ns < liquidity.event_ns
            || liquidity.available_ns > request.clock_ns
            || liquidity.event_ns > request.clock_ns
            || request
                .clock_ns
                .get()
                .saturating_sub(liquidity.event_ns.get())
                >= u64::from(request.maximum_age_seconds) * 1_000_000_000
        {
            blocked!("current_liquidity_stale_or_mismatched");
        }
        if !liquidity.notional.is_nonnegative()
            || !liquidity.lot_notional.is_positive()
            || !liquidity.minimum_notional.is_nonnegative()
            || !liquidity.taker_fee.is_fraction()
        {
            blocked!("execution_constraints_invalid");
        }
        let allowance = liquidity.notional.as_decimal() * request.participation.as_decimal();
        if target.weight.is_positive() {
            capital = capital.min((&current + &allowance) / target.weight.as_decimal());
            lower = lower
                .max((&current - &allowance).max(BigDecimal::zero()) / target.weight.as_decimal());
            fees += target.weight.as_decimal() * liquidity.taker_fee.as_decimal();
        } else if current > allowance {
            blocked!("reduction_exceeds_current_liquidity");
        }
    }
    if total != BigDecimal::from(1) || !request.target.cash_weight.is_fraction() {
        blocked!("target_sum_invalid");
    }
    // Reserve fees for the entire approved allocation; sell costs are checked below too.
    capital = capital.min(&budget / (BigDecimal::from(1) + fees));
    if capital < lower {
        blocked!("rebalance_exceeds_current_liquidity");
    }
    if request
        .minimum_capital
        .as_ref()
        .is_some_and(|minimum| capital < *minimum.as_decimal())
    {
        capital = BigDecimal::zero();
    }
    let capital = decimal(capital)?.as_decimal().clone();
    let mut target = request.target.clone();
    let mut invested = BigDecimal::zero();
    let mut cost = BigDecimal::zero();
    for weight in &mut target.targets {
        let current = holdings
            .get(&weight.instrument_id)
            .copied()
            .cloned()
            .unwrap_or_default();
        if !rebalances(&weight.instrument_id) {
            weight.weight = if request.equity.is_positive() {
                decimal(&current / request.equity.as_decimal())?
            } else {
                DecimalValue::zero()
            };
            invested += current;
            continue;
        }
        let desired = &capital * weight.weight.as_decimal();
        let Some(liquidity) = request
            .liquidity
            .iter()
            .find(|row| row.instrument_id == weight.instrument_id)
        else {
            weight.weight = DecimalValue::zero();
            continue;
        };
        let difference = &desired - &current;
        let lots = (difference.abs() / liquidity.lot_notional.as_decimal()).with_scale_round(
            0,
            if difference < BigDecimal::zero() {
                RoundingMode::Up
            } else {
                RoundingMode::Down
            },
        );
        let trade = lots * liquidity.lot_notional.as_decimal();
        let trade = if trade < *liquidity.minimum_notional.as_decimal() {
            BigDecimal::zero()
        } else {
            trade
        };
        if difference > BigDecimal::zero() && trade.is_zero() {
            blocked!("minimum_order_or_lot");
        }
        if difference < BigDecimal::zero() && (trade.is_zero() || trade > current) {
            blocked!("holding_reduction_not_feasible_with_lots");
        }
        let notional = if difference < BigDecimal::zero() {
            &current - &trade
        } else {
            &current + &trade
        };
        let delta = trade;
        if delta > liquidity.notional.as_decimal() * request.participation.as_decimal() {
            blocked!("rounded_rebalance_exceeds_liquidity");
        }
        cost += delta * liquidity.taker_fee.as_decimal();
        // Weights refer to total account equity, not just the strategy sleeve.
        weight.weight = if request.equity.is_positive() {
            decimal(&notional / request.equity.as_decimal())?
        } else {
            DecimalValue::zero()
        };
        invested += request.equity.as_decimal() * weight.weight.as_decimal();
    }
    if invested > capital {
        blocked!("retained_holdings_exceed_capacity_budget");
    }
    if &invested + &cost > budget {
        blocked!("execution_cost_exceeds_budget");
    }
    target.cash_weight = decimal(
        BigDecimal::from(1)
            - target
                .targets
                .iter()
                .map(|row| row.weight.as_decimal())
                .sum::<BigDecimal>(),
    )?;
    result.members = request
        .members
        .iter()
        .zip(&member_weights)
        .map(|(row, weight)| {
            Ok(NativeSizedMemberV1 {
                factor_id: row.factor_id.clone(),
                capital: decimal(&capital * weight)?,
                scale: if budget.is_zero() {
                    DecimalValue::zero()
                } else {
                    decimal(&capital / &budget)?
                },
            })
        })
        .collect::<Result<_>>()?;
    result.status = "SIZED".into();
    result.capital = decimal(capital)?;
    result.residual_cash = decimal(request.equity.as_decimal() - invested - cost)?;
    result.target = Some(target);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::science::NativeRollingBarLiquidityPolicyV1;
    use serde_json::json;
    fn sizing() -> NativeAccountSizingRequestV1 {
        serde_json::from_value(json!({"schemaVersion":1,"equity":"1000.000000000000000001","reserved":"100","cashBuffer":"100","maximumCapital":"10000","minimumCapital":"1","participation":"0.1","maximumAgeSeconds":60,"currency":"USD","clockNs":"2000000000",
            "target":{"schema_version":1,"asof_ns":"1000000000","valid_until_ns":"5000000000","cash_weight":"0","targets":[{"instrument_id":"A","currency":"USD","weight":"1"}]},
            "holdings":[],"liquidity":[{"instrumentId":"A","currency":"USD","eventNs":"1000000000","availableNs":"1000000000","notional":"1000","lotNotional":"0.01","minimumNotional":"1","takerFee":"0.001"}],"members":[{"factorId":"original","weight":"1"}]})).unwrap()
    }
    #[test]
    fn account_delta_reserves_fees_lots_and_exact_money() {
        let mut input = sizing();
        let result = size_account(&input).unwrap();
        assert_eq!(result.capital.as_decimal(), &BigDecimal::from(100));
        assert_eq!(
            result.budget.as_decimal(),
            &"800.000000000000000001".parse::<BigDecimal>().unwrap()
        );
        input.holdings = vec![NativeSizingHoldingV1 {
            instrument_id: "A".into(),
            value: "50".parse().unwrap(),
        }];
        assert_eq!(
            size_account(&input).unwrap().capital.as_decimal(),
            &BigDecimal::from(150)
        );
        input.reserved = "2000".parse().unwrap();
        assert!(size_account(&input).unwrap().budget.as_decimal().is_zero());
    }
    #[test]
    fn used_liquidity_requires_event_before_availability_before_decision() {
        let mut input = sizing();
        for available in [1_000_000_000, 1_500_000_000] {
            input.liquidity[0].available_ns = DbCounter::new(available).unwrap();
            assert_eq!(size_account(&input).unwrap().status, "SIZED");
        }
        input.liquidity[0].available_ns = DbCounter::new(999_999_999).unwrap();
        let result = size_account(&input).unwrap();
        assert_eq!(result.status, "BLOCKED");
        assert_eq!(
            result.reason.as_deref(),
            Some("current_liquidity_stale_or_mismatched")
        );
        assert!(result.target.is_none());
        assert!(result.members.is_empty());
        assert!(result.capital.as_decimal().is_zero());

        let mut unused = input.liquidity[0].clone();
        unused.instrument_id = "B".into();
        input.liquidity[0].available_ns = input.liquidity[0].event_ns;
        input.liquidity.push(unused);
        input.target.rebalance_instrument_ids = Some(vec!["A".into()]);
        let mut other = input.target.targets[0].clone();
        other.instrument_id = "B".into();
        other.weight = "0.5".parse().unwrap();
        input.target.targets[0].weight = "0.5".parse().unwrap();
        input.target.targets.push(other);
        assert_eq!(size_account(&input).unwrap().status, "SIZED");
    }
    #[test]
    fn live_minimum_or_lot_cannot_erase_a_funded_buy() {
        for (lot, minimum) in [("1000", "1"), ("0.01", "1000")] {
            let mut input = sizing();
            input.liquidity[0].lot_notional = lot.parse().unwrap();
            input.liquidity[0].minimum_notional = minimum.parse().unwrap();
            let result = size_account(&input).unwrap();
            assert_eq!(result.status, "BLOCKED");
            assert_eq!(result.reason.as_deref(), Some("minimum_order_or_lot"));
            assert!(result.capital.as_decimal().is_zero());
            assert!(result.members.is_empty());
            assert!(result.target.is_none());
        }
    }
    #[test]
    fn scoped_rebalance_keeps_actual_untraded_holdings_and_cannot_spend_their_proceeds() {
        let mut input = sizing();
        input.equity = "1000".parse().unwrap();
        input.reserved = "400".parse().unwrap();
        input.cash_buffer = "100".parse().unwrap();
        input.target.targets[0].weight = "0.5".parse().unwrap();
        input.target.cash_weight = "0.5".parse().unwrap();
        let mut held = input.target.targets[0].clone();
        held.instrument_id = "B".into();
        held.weight = DecimalValue::zero();
        input.target.targets.push(held);
        input.target.rebalance_instrument_ids = Some(vec!["A".into()]);
        input.holdings = vec![NativeSizingHoldingV1 {
            instrument_id: "B".into(),
            value: "500".parse().unwrap(),
        }];
        input.liquidity[0].notional = "100000".parse().unwrap();
        assert!(size_account(&input).unwrap().target.is_none());
        input.holdings[0].value = "200".parse().unwrap();
        let result = size_account(&input).unwrap();
        assert_eq!(result.status, "SIZED");
        assert_eq!(
            result.target.unwrap().targets[1].weight.as_decimal(),
            &"0.2".parse::<BigDecimal>().unwrap()
        );
        assert!(result.residual_cash.as_decimal() >= &BigDecimal::from(500));
        input.projection_only = true;
        assert_eq!(size_account(&input).unwrap().status, "PROJECTED");
        input.maximum_capital = "100".parse().unwrap();
        assert!(size_account(&input).unwrap().target.is_none());
    }
    #[test]
    fn selling_lot_rounding_never_retains_more_than_the_capacity_budget() {
        let mut input = sizing();
        input.maximum_capital = "100".parse().unwrap();
        input.holdings = vec![NativeSizingHoldingV1 {
            instrument_id: "A".into(),
            value: "150".parse().unwrap(),
        }];
        input.liquidity[0].lot_notional = "100".parse().unwrap();
        let result = size_account(&input).unwrap();
        assert_eq!(result.status, "SIZED");
        assert!(
            result.target.unwrap().targets[0].weight.as_decimal() * input.equity.as_decimal()
                <= *result.capital.as_decimal()
        );
        input.holdings[0].value = "50".parse().unwrap();
        input.maximum_capital = DecimalValue::zero();
        assert!(size_account(&input).unwrap().target.is_none());
    }
    #[test]
    fn missing_stale_zero_liquidity_currency_and_tiny_accounts_are_not_executable() {
        let mut input = sizing();
        input.liquidity.clear();
        assert!(size_account(&input).unwrap().target.is_none());
        let mut input = sizing();
        input.liquidity[0].currency = "EUR".into();
        assert!(size_account(&input).unwrap().target.is_none());
        let mut input = sizing();
        input.clock_ns = DbCounter::new(70_000_000_000).unwrap();
        assert!(size_account(&input).unwrap().target.is_none());
        let mut input = sizing();
        input.liquidity[0].notional = DecimalValue::zero();
        assert_eq!(
            size_account(&input).unwrap().capital.as_decimal(),
            &BigDecimal::zero()
        );
        let mut input = sizing();
        input.equity = "0.001".parse().unwrap();
        input.reserved = DecimalValue::zero();
        input.cash_buffer = DecimalValue::zero();
        let result = size_account(&input).unwrap();
        assert!(result.capital.as_decimal().is_zero());
        assert!(result.target.unwrap().targets[0]
            .weight
            .as_decimal()
            .is_zero());
    }
    #[test]
    fn projection_validates_original_pool_and_preserves_nested_member_weights() {
        let mut input = sizing();
        input.projection_only = true;
        input.members = serde_json::from_value(json!([
            {"factorId":"r1.t2.m0","weight":"0.25","poolWeight":"0.4"},
            {"factorId":"r1.t2.m1","weight":"0.75","poolWeight":"0.4"},
            {"factorId":"r3.t4.m0","weight":"1","poolWeight":"0.6"}
        ]))
        .unwrap();
        let result = size_account(&input).unwrap();
        assert_eq!(
            result.members[0].capital.as_decimal(),
            decimal(result.capital.as_decimal() * "0.1".parse::<BigDecimal>().unwrap())
                .unwrap()
                .as_decimal()
        );
        let allocated = result
            .members
            .iter()
            .map(|row| row.capital.as_decimal())
            .sum::<BigDecimal>();
        assert!(allocated <= *result.capital.as_decimal());
        assert!(
            result.capital.as_decimal() - allocated
                < "0.000000000000000003".parse::<BigDecimal>().unwrap()
        );
        input.members[0].weight = "0.5".parse().unwrap();
        assert!(size_account(&input).is_err());
        input.members[0].weight = "0.25".parse().unwrap();
        input.target.cash_weight = "1".parse().unwrap();
        assert!(size_account(&input).unwrap().target.is_none());
    }
    #[test]
    fn joint_capacity_is_tightest_asset_not_sum_of_member_limits() {
        let input = sizing();
        let request = NativeCapacityRequestV1 { schema_version: contracts::SchemaV1, simulation: serde_json::from_value(json!({"schema_version":1,"selection":{"schema_version":1,"bar_types":["A-1-MINUTE-LAST-EXTERNAL"],"event_start_ns":"1","event_end_ns":"2","decision_cutoff_ns":"2","maximum_rows":1},"settings":{
          "schema_version":1,"base_currency":"USD","starting_capital":"1000","account_kind":"CASH","leverage":"1","fee_model":{"schema_version":1,"adapter_kind":"NAUTILUS_MAKER_TAKER","upstream_class":"native","upstream_version":"0.67.0","parameters":{}},"fill_model":{"schema_version":1,"adapter_kind":"NAUTILUS_DEFAULT_FILL","upstream_class":"native","upstream_version":"0.67.0","parameters":{"prob_fill_on_limit":"1","prob_slippage":"0","random_seed":"7"}},"latency_model":{"schema_version":1,"adapter_kind":"NAUTILUS_STATIC_LATENCY","upstream_class":"native","upstream_version":"0.67.0","parameters":{"base_latency_ns":"0","insert_latency_ns":"0","update_latency_ns":"0","cancel_latency_ns":"0"}},"snapshot_interval_ms":1,"exposure_tolerance":"0","fee_rates":[]},"target_points":[input.target]})).unwrap(), policy: NativeRollingBarLiquidityPolicyV1 { schema_version: contracts::SchemaV1, maximum_age_seconds: 60, participation_limit: "0.1".parse().unwrap() }, capitals: vec!["10".parse().unwrap(), "100".parse().unwrap(), "1000".parse().unwrap()] };
        let demands = vec![
            Demand {
                instrument: "A".into(),
                change: "0.75".parse().unwrap(),
                exit: "0.75".parse().unwrap(),
                liquidity: "300".parse().unwrap(),
                minimum: "1".parse().unwrap(),
                ..Default::default()
            },
            Demand {
                instrument: "B".into(),
                change: "0.25".parse().unwrap(),
                exit: "0.25".parse().unwrap(),
                liquidity: "1000".parse().unwrap(),
                minimum: "1".parse().unwrap(),
                ..Default::default()
            },
        ];
        let result = finish(envelope(&request), &request, &demands).unwrap();
        assert_eq!(
            result.maximum_capital.unwrap().as_decimal(),
            &BigDecimal::from(40)
        );
        assert_eq!(result.binding_instrument.as_deref(), Some("A"));
        assert_eq!(
            result.scenarios[..3]
                .iter()
                .map(|row| row.status.as_str())
                .collect::<Vec<_>>(),
            vec!["FEASIBLE", "CONSTRAINED", "CONSTRAINED"]
        );
        let result = finish(envelope(&request), &request, &[]).unwrap();
        assert!(result.maximum_capital.is_none());
        assert_eq!(result.status, "NON_BINDING");
        let exit_only = finish(
            envelope(&request),
            &request,
            &[Demand {
                instrument: "A".into(),
                change: BigDecimal::zero(),
                exit: BigDecimal::from(1),
                liquidity: BigDecimal::from(1000),
                minimum: BigDecimal::from(1),
                ..Default::default()
            }],
        )
        .unwrap();
        assert_eq!(
            exit_only.scenarios[0]
                .required_turnover
                .as_ref()
                .unwrap()
                .as_decimal(),
            &BigDecimal::from(10)
        );
        assert_eq!(
            exit_only.scenarios[0]
                .maximum_participation
                .as_ref()
                .unwrap()
                .as_decimal(),
            &"0.01".parse::<BigDecimal>().unwrap()
        );
        let mut fractional = Demand {
            instrument: "A".into(),
            change: "0.3".parse().unwrap(),
            exit: "0.3".parse().unwrap(),
            liquidity: BigDecimal::from(1),
            ..Default::default()
        };
        let result = finish(envelope(&request), &request, &[fractional.clone()]).unwrap();
        let row = &result.scenarios[0];
        assert_eq!(
            row.usable_capital.as_ref().unwrap().as_decimal()
                + row.unallocated_cash.as_ref().unwrap().as_decimal(),
            *row.capital.as_decimal()
        );
        fractional.minimum = "0.1".parse().unwrap();
        let result = finish(envelope(&request), &request, &[fractional]).unwrap();
        assert_eq!(result.scenarios[0].status, "TOO_SMALL");
        assert!(result.scenarios[0]
            .usable_capital
            .as_ref()
            .unwrap()
            .as_decimal()
            .is_zero());
        for change in ["0", "1"] {
            let result = finish(
                envelope(&request),
                &request,
                &[Demand {
                    instrument: "A".into(),
                    change: change.parse().unwrap(),
                    exit: "1".parse().unwrap(),
                    liquidity: BigDecimal::zero(),
                    minimum: "1".parse().unwrap(),
                    ..Default::default()
                }],
            )
            .unwrap();
            assert!(result
                .scenarios
                .iter()
                .all(|row| row.status == "CONSTRAINED"
                    && row.reason.as_deref() == Some("zero_observed_liquidity")
                    && row.usable_capital.as_ref().unwrap().as_decimal().is_zero()));
        }
    }
}
