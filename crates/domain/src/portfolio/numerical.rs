//! Registered numerical point representation and certification, not a solver.
use super::{
    allocation_input, allocation_point_feasibility, optimizer_settings, sample_covariance,
};
use crate::DomainError;
use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use contracts::{portfolio::*, DecimalValue};
use ndarray::{Array1, Array2};

fn bad() -> DomainError {
    DomainError::Invalid("cash_numerical_output")
}
fn decimal(value: BigDecimal) -> Result<DecimalValue, DomainError> {
    value
        .normalized()
        .to_plain_string()
        .parse()
        .map_err(|_| bad())
}
fn native(value: &BigDecimal) -> Result<f64, DomainError> {
    value
        .to_f64()
        .filter(|v| v.is_finite() && (*v != 0.0 || value == &BigDecimal::from(0)))
        .ok_or_else(bad)
}

/// Original public 18-place, half-even representation; no constraint repair.
pub fn cash_numerical_weight(value: f64) -> Result<DecimalValue, DomainError> {
    if !value.is_finite() {
        return Err(bad());
    }
    let exact: BigDecimal = value.to_string().parse().map_err(|_| bad())?;
    decimal(exact.with_scale_round(18, RoundingMode::HalfEven))
}

fn capital_cash(targets: &[AllocationTargetV1]) -> Result<DecimalValue, DomainError> {
    decimal(
        BigDecimal::from(1)
            - targets
                .iter()
                .map(|t| t.weight.as_decimal())
                .sum::<BigDecimal>(),
    )
}

/// An anchor is a numerical coordinate candidate. It becomes usable only after
/// all original point constraints pass and the original fee-reserve floor has
/// strictly positive slack. Neither account HOLD nor all-cash is assumed feasible.
pub fn cash_numerical_anchor(
    input: &AllocationInputV1,
    kind: NativeCashAnchorKindV1,
) -> Result<NativeCashFeasibleAnchorV1, DomainError> {
    let targets = input
        .assets
        .iter()
        .map(|asset| AllocationTargetV1 {
            instrument_id: asset.instrument_id.clone(),
            currency: asset.currency.clone(),
            weight: match kind {
                NativeCashAnchorKindV1::CurrentWeights => asset.current_weight.clone(),
                NativeCashAnchorKindV1::AllCash => "0".parse().unwrap(),
            },
        })
        .collect::<Vec<_>>();
    let cash_weight = capital_cash(&targets)?;
    allocation_point_feasibility(input, &targets, &cash_weight, None)?;
    let cost: BigDecimal = targets
        .iter()
        .zip(&input.assets)
        .map(|(t, a)| {
            (t.weight.as_decimal() - a.current_weight.as_decimal()).abs()
                * a.transaction_cost_rate.as_decimal()
        })
        .sum();
    if cash_weight.as_decimal() - cost <= *input.constraints.min_cash_weight.as_decimal() {
        return Err(DomainError::Invalid(
            "cash_numerical_anchor_without_funding_slack",
        ));
    }
    Ok(NativeCashFeasibleAnchorV1 {
        kind,
        targets,
        cash_weight,
    })
}

/// Finite decimal segment coordinate. Cash is a candidate coordinate enforcing
/// capital identity; the caller must certify the entire point before using it.
pub fn cash_numerical_segment(
    raw: &[AllocationTargetV1],
    anchor: &NativeCashFeasibleAnchorV1,
    fraction: &DecimalValue,
) -> Result<(Vec<AllocationTargetV1>, DecimalValue), DomainError> {
    if !fraction.is_fraction() || raw.len() != anchor.targets.len() {
        return Err(bad());
    }
    let t = fraction.as_decimal();
    let mut targets = Vec::with_capacity(raw.len());
    for (raw, anchor) in raw.iter().zip(&anchor.targets) {
        if raw.instrument_id != anchor.instrument_id || raw.currency != anchor.currency {
            return Err(bad());
        }
        let weight = (raw.weight.as_decimal() * (BigDecimal::from(1) - t)
            + anchor.weight.as_decimal() * t)
            .with_scale_round(18, RoundingMode::HalfEven);
        targets.push(AllocationTargetV1 {
            instrument_id: raw.instrument_id.clone(),
            currency: raw.currency.clone(),
            weight: decimal(weight)?,
        });
    }
    let cash = capital_cash(&targets)?;
    Ok((targets, cash))
}

/// Same frozen covariance, ensemble, lambda and actual absolute-turnover cost.
/// This is the original MAX_UTILITY objective, never an additional estimator.
pub fn cash_numerical_objective(
    input: &AllocationInputV1,
    targets: &[AllocationTargetV1],
) -> Result<f64, DomainError> {
    allocation_input(input)?;
    if input.objective != AllocationObjective::MaxUtility
        || input.risk != AllocationRisk::Variance
        || targets.len() != input.assets.len()
        || targets
            .iter()
            .zip(&input.assets)
            .any(|(t, a)| t.instrument_id != a.instrument_id || t.currency != a.currency)
    {
        return Err(bad());
    }
    let n = input.assets.len();
    let covariance = sample_covariance(
        &input.covariance_estimator,
        &input.return_history.asset_returns,
    )
    .map_err(|_| bad())?;
    let covariance = Array2::from_shape_vec((n, n), covariance.into_iter().flatten().collect())
        .map_err(|_| bad())?;
    let weights = Array1::from_vec(
        targets
            .iter()
            .map(|t| native(t.weight.as_decimal()))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let member_weights = Array1::from_vec(
        input
            .forecasts
            .members
            .iter()
            .map(|m| native(m.ensemble_weight.as_decimal()))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let forecasts = Array2::from_shape_vec(
        (input.forecasts.members.len(), n),
        input
            .forecasts
            .members
            .iter()
            .flat_map(|m| m.forecasts.iter().copied())
            .collect(),
    )
    .map_err(|_| bad())?;
    let forecasts = member_weights.dot(&forecasts);
    let cost: BigDecimal = targets
        .iter()
        .zip(&input.assets)
        .map(|(t, a)| {
            (t.weight.as_decimal() - a.current_weight.as_decimal()).abs()
                * a.transaction_cost_rate.as_decimal()
        })
        .sum();
    let lambda = native(
        optimizer_settings(&input.optimizer)?
            .risk_aversion
            .as_decimal(),
    )?;
    let value =
        lambda * weights.dot(&covariance.dot(&weights)) + native(&cost)? - forecasts.dot(&weights);
    if !value.is_finite() {
        return Err(bad());
    }
    Ok(value)
}

/// Direct exact difference of the two represented points. Converting the
/// original finite f64 QP coefficients with BigDecimal::try_from preserves
/// their binary rational values exactly (not their shortest display strings).
/// This cannot lose a small difference by subtracting large f64 objectives.
pub fn cash_numerical_objective_delta(
    input: &AllocationInputV1,
    raw: &[AllocationTargetV1],
    final_targets: &[AllocationTargetV1],
) -> Result<BigDecimal, DomainError> {
    allocation_input(input)?;
    let n = input.assets.len();
    if input.objective != AllocationObjective::MaxUtility
        || input.risk != AllocationRisk::Variance
        || raw.len() != n
        || final_targets.len() != n
        || raw
            .iter()
            .zip(final_targets)
            .zip(&input.assets)
            .any(|((r, f), a)| {
                r.instrument_id != a.instrument_id
                    || f.instrument_id != a.instrument_id
                    || r.currency != a.currency
                    || f.currency != a.currency
            })
    {
        return Err(bad());
    }
    let covariance = sample_covariance(
        &input.covariance_estimator,
        &input.return_history.asset_returns,
    )
    .map_err(|_| bad())?;
    let lambda = native(
        optimizer_settings(&input.optimizer)?
            .risk_aversion
            .as_decimal(),
    )?;
    let member_weights = Array1::from_vec(
        input
            .forecasts
            .members
            .iter()
            .map(|m| native(m.ensemble_weight.as_decimal()))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let forecasts = Array2::from_shape_vec(
        (input.forecasts.members.len(), n),
        input
            .forecasts
            .members
            .iter()
            .flat_map(|m| m.forecasts.iter().copied())
            .collect(),
    )
    .map_err(|_| bad())?;
    let forecasts = member_weights.dot(&forecasts);
    let half = BigDecimal::new(5.into(), 1);
    let mut delta = BigDecimal::from(0);
    for i in 0..n {
        let a = &input.assets[i];
        let r = raw[i].weight.as_decimal();
        let f = final_targets[i].weight.as_decimal();
        delta -= BigDecimal::try_from(forecasts[i]).map_err(|_| bad())? * (f - r);
        delta += ((f - a.current_weight.as_decimal()).abs()
            - (r - a.current_weight.as_decimal()).abs())
            * a.transaction_cost_rate.as_decimal();
        for j in 0..n {
            // Same P scaling/order as the actual Clarabel adapter, then exact
            // division by two; no rounding of its finite coefficient identity.
            let p = BigDecimal::try_from(2.0 * lambda * covariance[i][j]).map_err(|_| bad())?;
            delta += p
                * &half
                * (f * final_targets[j].weight.as_decimal() - r * raw[j].weight.as_decimal());
        }
    }
    if delta.normalized().to_plain_string().len() > 2048 {
        return Err(bad());
    }
    Ok(delta)
}

pub(super) fn validate_result(
    input: &AllocationInputV1,
    result: &AllocationResultV1,
) -> Result<(), DomainError> {
    let settings = optimizer_settings(&input.optimizer)?;
    let successful = matches!(
        result.solver_status,
        SolverStatus::Optimal | SolverStatus::AcceptableInaccurate
    );
    let Some(trace) = &result.numerical_output else {
        return if settings.cash_numerical_output.is_some() && successful {
            Err(bad())
        } else {
            Ok(())
        };
    };
    let n = input.assets.len();
    if settings.cash_numerical_output != Some(trace.protocol)
        || trace.raw_x.len() != 3 * n + 1
        || trace.raw_x.iter().any(|x| !x.is_finite())
        || !trace.raw_objective_value.is_finite()
        || !trace.raw_point_objective_value.is_finite()
        || !trace.raw_primal_residual.is_finite()
        || trace.raw_primal_residual < 0.0
        || !trace.raw_dual_residual.is_finite()
        || trace.raw_dual_residual < 0.0
        || result.primal_residual != Some(trace.raw_primal_residual)
        || result.dual_residual != Some(trace.raw_dual_residual)
        || trace.objective_change_limit != settings.solver_tolerance
        || trace.search_steps > 122
        || !(trace.raw_native_status == "Solved"
            || (trace.raw_native_status == "AlmostSolved" && settings.accept_inaccurate))
        || trace.raw_targets.len() != n
    {
        return Err(bad());
    }
    for (i, (raw, asset)) in trace.raw_targets.iter().zip(&input.assets).enumerate() {
        if raw.instrument_id != asset.instrument_id
            || raw.currency != asset.currency
            || raw.weight != cash_numerical_weight(trace.raw_x[i])?
        {
            return Err(bad());
        }
    }
    if trace.raw_cash_weight != cash_numerical_weight(trace.raw_x[n])?
        || trace.raw_point_objective_value != cash_numerical_objective(input, &trace.raw_targets)?
    {
        return Err(bad());
    }
    let raw_error =
        allocation_point_feasibility(input, &trace.raw_targets, &trace.raw_cash_weight, None)
            .err()
            .map(|e| e.to_string());
    if raw_error != trace.raw_constraint_error {
        return Err(bad());
    }
    match trace.outcome {
        NativeCashNumericalOutcomeV1::Rejected => {
            if result.solver_status != SolverStatus::Failed
                || trace.raw_constraint_error.is_none()
                || trace.reason_code.as_ref().is_none_or(|r| r.is_empty())
                || result.reason_code != trace.reason_code
                || trace.anchor.is_some()
                || trace.segment_fraction.is_some()
                || trace.objective_delta.is_some()
                || trace.objective_delta_exact.is_some()
            {
                return Err(bad());
            }
        }
        NativeCashNumericalOutcomeV1::Unchanged | NativeCashNumericalOutcomeV1::Corrected => {
            let targets = result.targets.as_ref().ok_or_else(bad)?;
            let cash = result.cash_weight.as_ref().ok_or_else(bad)?;
            let objective = cash_numerical_objective(input, targets)?;
            let delta = cash_numerical_objective_delta(input, &trace.raw_targets, targets)?;
            let approximate_delta = delta.to_f64().filter(|v| v.is_finite()).ok_or_else(bad)?;
            if !successful
                || trace.reason_code.is_some()
                || result.objective_value != Some(objective)
                || (trace.raw_native_status == "Solved"
                    && result.solver_status != SolverStatus::Optimal)
                || (trace.raw_native_status == "AlmostSolved"
                    && result.solver_status != SolverStatus::AcceptableInaccurate)
                || trace.objective_delta != Some(approximate_delta)
                || trace.objective_delta_exact.as_deref()
                    != Some(delta.normalized().to_plain_string().as_str())
                || delta.abs() > *settings.solver_tolerance.as_decimal()
            {
                return Err(bad());
            }
            match trace.outcome {
                NativeCashNumericalOutcomeV1::Unchanged => {
                    if raw_error.is_some()
                        || *targets != trace.raw_targets
                        || *cash != trace.raw_cash_weight
                        || trace.anchor.is_some()
                        || trace.segment_fraction.is_some()
                        || trace.search_steps != 0
                    {
                        return Err(bad());
                    }
                }
                NativeCashNumericalOutcomeV1::Corrected => {
                    let anchor = trace.anchor.as_ref().ok_or_else(bad)?;
                    if raw_error.is_none() || *anchor != cash_numerical_anchor(input, anchor.kind)?
                    {
                        return Err(bad());
                    }
                    let fraction = trace.segment_fraction.as_ref().ok_or_else(bad)?;
                    // A wider precision is not an alternative protocol grid.
                    if fraction.as_decimal()
                        != &fraction
                            .as_decimal()
                            .with_scale_round(18, RoundingMode::HalfEven)
                    {
                        return Err(bad());
                    }
                    let (expected, expected_cash) =
                        cash_numerical_segment(&trace.raw_targets, anchor, fraction)?;
                    if *targets != expected || *cash != expected_cash {
                        return Err(bad());
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
