//! Bind catalog-derived numerical inputs to the frozen portfolio request.
use super::*;
use contracts::{brief::TargetKind, science::*};

pub fn candidate_simulation(
    candidate: Id,
    available_ns: contracts::DbCounter,
    target: &PortfolioTargetsV1,
    request: &NativeSimulationRequestV1,
) -> Result<(), DomainError> {
    selection(&request.selection)?;
    crate::portfolio::simulation_settings(&request.settings)?;
    let [point] = request.target_points.as_slice() else {
        return Err(bad("candidate_simulation.targets"));
    };
    target_source(
        candidate,
        available_ns,
        target,
        point,
        &request.settings.base_currency,
    )?;
    if request.selection.event_start_ns != point.asof_ns
        || request.selection.event_end_ns > point.valid_until_ns
    {
        return Err(bad("candidate_simulation.source"));
    }
    Ok(())
}

fn target_source(
    candidate: Id,
    available_ns: contracts::DbCounter,
    target: &PortfolioTargetsV1,
    point: &NativeTargetPointV1,
    currency: &str,
) -> Result<(), DomainError> {
    let nanos = |time: chrono::DateTime<chrono::Utc>| {
        time.timestamp_nanos_opt()
            .and_then(|n| u64::try_from(n).ok())
            .and_then(|n| contracts::DbCounter::new(n).ok())
            .ok_or_else(|| bad("candidate_simulation.time"))
    };
    let asof = nanos(target.asof)?;
    let effective = if available_ns > asof {
        available_ns
    } else {
        asof
    };
    let until = nanos(target.valid_until)?;
    if target.candidate_id != candidate
        || target.base_currency != currency
        || effective >= until
        || point.asof_ns != effective
        || point.valid_until_ns != until
        || point.targets != target.targets
        || point.cash_weight != target.cash_weight
        || !(1..=256).contains(&target.targets.len())
        || target
            .targets
            .iter()
            .any(|t| t.currency != target.base_currency)
    {
        return Err(bad("candidate_simulation.source"));
    }
    Ok(())
}

pub fn portfolio_sequence(
    sources: &[NativePortfolioTargetSourceV1],
    targets: &[PortfolioTargetsV1],
    request: &NativeSimulationRequestV1,
) -> Result<(), DomainError> {
    selection(&request.selection)?;
    crate::portfolio::simulation_settings(&request.settings)?;
    if !(2..=253).contains(&sources.len())
        || sources.len() != targets.len()
        || sources.len() != request.target_points.len()
    {
        return Err(bad("portfolio_sequence.sources"));
    }
    let mut candidates = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for ((source, target), point) in sources.iter().zip(targets).zip(&request.target_points) {
        if !candidates.insert(source.candidate_id) || !artifacts.insert(source.target_artifact_id) {
            return Err(bad("portfolio_sequence.duplicate"));
        }
        target_source(
            source.candidate_id,
            source.candidate_available_ns,
            target,
            point,
            &request.settings.base_currency,
        )?;
    }
    let points = &request.target_points;
    if request.selection.event_start_ns != points[0].asof_ns
        || request.selection.event_end_ns > points[points.len() - 1].valid_until_ns
        || request.selection.event_end_ns <= points[points.len() - 1].asof_ns
        || points.windows(2).any(|pair| {
            pair[0].asof_ns >= pair[1].asof_ns || pair[0].valid_until_ns < pair[1].asof_ns
        })
    {
        return Err(bad("portfolio_sequence.window"));
    }
    Ok(())
}

/// Thin proportional expectation of the frozen native one-tick model, not a fill simulator.
pub fn portfolio_execution_costs(
    request: &NativePortfolioBuildRequestV1,
    references: &[NativePortfolioSlippageReferenceV1],
) -> Result<Vec<contracts::portfolio::AllocationAssetV1>, DomainError> {
    portfolio_costs(
        &request.selection,
        &request.mandate,
        &request.execution_settings,
        &request.assets,
        references,
    )
}

pub fn portfolio_costs(
    selected: &NativeBarSelectionV1,
    mandate: &contracts::portfolio::MandateContentV1,
    settings: &NativeSimulationSettingsV1,
    assets: &[contracts::portfolio::AllocationAssetV1],
    references: &[NativePortfolioSlippageReferenceV1],
) -> Result<Vec<contracts::portfolio::AllocationAssetV1>, DomainError> {
    let (fill, _) = crate::portfolio::simulation_models(settings)?;
    let mut assets = assets.to_vec();
    if !fill.prob_slippage.is_positive() {
        if !references.is_empty() {
            return Err(bad("portfolio.slippage_reference"));
        }
        return Ok(assets);
    }
    if references.len() != assets.len() {
        return Err(bad("portfolio.slippage_reference"));
    }
    for (asset, reference) in assets.iter_mut().zip(references) {
        if reference.instrument_id != asset.instrument_id
            || reference.currency != asset.currency
            || !reference.close_price.is_positive()
            || !reference.price_increment.is_positive()
            || reference.price_increment.as_decimal() >= reference.close_price.as_decimal()
            || reference.event_ns < selected.event_start_ns
            || reference.event_ns >= selected.event_end_ns
            || reference.available_ns < reference.event_ns
            || reference.available_ns > selected.decision_cutoff_ns
            || selected.decision_cutoff_ns.get() - reference.event_ns.get()
                > u64::from(mandate.rebalance_schedule.max_input_age_seconds) * 1_000_000_000
            || !asset.transaction_cost_rate.is_fraction()
        {
            return Err(bad("portfolio.slippage_reference"));
        }
        let fee = asset.transaction_cost_rate.as_decimal();
        let rate = fee
            + fill.prob_slippage.as_decimal()
                * reference.price_increment.as_decimal()
                * (bigdecimal::BigDecimal::from(1) + fee)
                / reference.close_price.as_decimal();
        asset.transaction_cost_rate = rate
            .with_scale_round(18, bigdecimal::RoundingMode::Ceiling)
            .to_plain_string()
            .parse()
            .map_err(|_| bad("portfolio.slippage_rate"))?;
        if !asset.transaction_cost_rate.is_fraction() {
            return Err(bad("portfolio.slippage_rate"));
        }
    }
    Ok(assets)
}

pub fn portfolio_build_request(request: &NativePortfolioBuildRequestV1) -> Result<(), DomainError> {
    selection(&request.selection)?;
    portfolio_settings(
        &request.mandate,
        &request.execution_settings,
        &request.assets,
    )?;
    portfolio_members(&request.selection, &request.assets, &request.members)?;
    let constraints = &request.mandate.constraints;
    if let Some(liquidity) = &request.bar_liquidity {
        crate::portfolio::bar_liquidity_assumption(&liquidity.assumption)?;
        selection(&liquidity.source.selection)?;
        if constraints.liquidity_ref != Some(liquidity.assumption.report_artifact_id)
            || constraints.max_participation.as_ref()
                != Some(&liquidity.assumption.participation_limit)
            || request.assets.iter().any(|a| {
                a.available_notional
                    .as_ref()
                    .is_none_or(|n| !n.is_nonnegative())
            })
        {
            return Err(bad("portfolio.liquidity_binding"));
        }
    } else if constraints.liquidity_ref.is_some()
        || constraints.max_participation.is_some()
        || request
            .assets
            .iter()
            .any(|a| a.available_notional.is_some())
    {
        return Err(bad("portfolio.liquidity_binding"));
    }
    let weights = &request.current_weights;
    let cutoff = request.selection.decision_cutoff_ns;
    if weights.asof_ns > weights.available_ns
        || weights.available_ns > cutoff
        || weights.valid_until_ns <= cutoff
        || weights.asof_ns > cutoff
        || cutoff.get() - weights.asof_ns.get()
            > u64::from(request.mandate.rebalance_schedule.max_input_age_seconds) * 1_000_000_000
        || weights.base_currency != request.mandate.base_currency
        || weights.weights.len() != request.assets.len()
    {
        return Err(bad("portfolio.current_weights"));
    }
    if let PortfolioWeightsSourceV1::ForwardSnapshot {
        external_message_id,
        ..
    } = &weights.source
    {
        crate::control::text(external_message_id, 1, 200, false)?;
    }
    let mut total = weights.cash_weight.as_decimal().clone();
    let mut instruments = BTreeSet::new();
    for (weight, asset) in weights.weights.iter().zip(&request.assets) {
        if weight.instrument_id != asset.instrument_id
            || weight.currency != weights.base_currency
            || asset.currency != weights.base_currency
            || weight.weight != asset.current_weight
            || !instruments.insert(&weight.instrument_id)
        {
            return Err(bad("portfolio.current_weights"));
        }
        total += weight.weight.as_decimal();
    }
    if (total - bigdecimal::BigDecimal::from(1)).abs()
        > *request.mandate.exposure_tolerance.as_decimal()
    {
        return Err(bad("portfolio.current_weights"));
    }
    Ok(())
}

fn portfolio_settings(
    mandate: &contracts::portfolio::MandateContentV1,
    costs: &NativeSimulationSettingsV1,
    assets: &[contracts::portfolio::AllocationAssetV1],
) -> Result<(), DomainError> {
    crate::portfolio::mandate(mandate)?;
    crate::portfolio::simulation_settings(costs)?;
    if costs.base_currency != mandate.base_currency
        || costs.starting_capital != mandate.capital_assumption
        || costs.fee_rates.len() != assets.len()
        || assets.iter().any(|asset| {
            !costs.fee_rates.iter().any(|fee| {
                fee.instrument_id == asset.instrument_id && fee.taker == asset.transaction_cost_rate
            })
        })
    {
        return Err(bad("portfolio.execution_settings"));
    }
    Ok(())
}

fn portfolio_members(
    selected: &NativeBarSelectionV1,
    assets: &[contracts::portfolio::AllocationAssetV1],
    members: &[NativePortfolioAlphaV1],
) -> Result<(), DomainError> {
    if !(2..=256).contains(&members.len()) || assets.len() != selected.bar_types.len() {
        return Err(bad("portfolio.members"));
    }
    crate::portfolio::ensemble_weights(members.iter().map(|v| &v.ensemble_weight))?;
    let mut versions = BTreeSet::new();
    let mut alphas = BTreeSet::new();
    let horizon = members[0].parameters.label_horizon_observations;
    for member in members {
        forecast_request(&NativeForecastRequestV1 {
            schema_version: contracts::SchemaV1,
            selection: selected.clone(),
            parameters: member.parameters.clone(),
        })?;
        if !versions.insert(member.alpha_version_id)
            || member.parameters.label_horizon_observations != horizon
            || (member.target_kind == TargetKind::Score) != member.calibration_artifact_id.is_some()
        {
            return Err(bad("portfolio.alpha_binding"));
        }
        if member.ensemble_weight.is_positive() {
            alphas.insert(member.alpha_id);
        }
    }
    if alphas.len() < 2 {
        return Err(bad("portfolio.distinct_alphas"));
    }
    Ok(())
}

pub fn portfolio_study_cutoffs(
    request: &NativePortfolioStudyRequestV1,
) -> Result<Vec<contracts::DbCounter>, DomainError> {
    selection(&request.source_selection)?;
    portfolio_settings(
        &request.mandate,
        &request.execution_settings,
        &request.assets,
    )?;
    portfolio_members(&request.source_selection, &request.assets, &request.members)?;
    let schedule = &request.mandate.rebalance_schedule;
    if schedule.kind != contracts::portfolio::RebalanceKind::CalendarSession
        && request.calendar.is_some()
    {
        return Err(bad("portfolio_study.unbound_calendar"));
    }
    let constraints = &request.mandate.constraints;
    if request
        .assets
        .iter()
        .any(|a| a.available_notional.is_some())
    {
        return Err(bad("portfolio_study.handwritten_liquidity"));
    }
    match &request.rolling_liquidity {
        Some(policy)
            if policy.maximum_age_seconds > 0
                && policy.participation_limit.is_positive()
                && policy.participation_limit.is_fraction()
                && constraints.liquidity_ref.is_some()
                && constraints.max_participation.as_ref() == Some(&policy.participation_limit) => {}
        None if constraints.liquidity_ref.is_none() && constraints.max_participation.is_none() => {}
        _ => return Err(bad("portfolio_study.liquidity_policy")),
    }
    if request.assets.iter().any(|a| {
        !a.current_weight
            .as_decimal()
            .eq(&bigdecimal::BigDecimal::from(0))
            || a.currency != request.mandate.base_currency
    }) || request.evaluation_start_ns <= request.source_selection.event_start_ns
        || request.evaluation_start_ns <= request.research_available_through_ns
        || request.evaluation_start_ns >= request.source_selection.event_end_ns
    {
        return Err(bad("portfolio_study.initial_state"));
    }
    let cutoffs = match schedule.kind {
        contracts::portfolio::RebalanceKind::Manual => request
            .manual_cutoffs_ns
            .clone()
            .ok_or_else(|| bad("portfolio_study.manual_cutoffs"))?,
        contracts::portfolio::RebalanceKind::FixedInterval => {
            if request.manual_cutoffs_ns.is_some() {
                return Err(bad("portfolio_study.manual_cutoffs"));
            }
            let step = u64::from(
                schedule
                    .interval_seconds
                    .ok_or_else(|| bad("portfolio_study.schedule"))?,
            ) * 1_000_000_000;
            let n = (request.source_selection.event_end_ns.get()
                - request.evaluation_start_ns.get()
                - 1)
                / step
                + 1;
            if !(2..=256).contains(&n) {
                return Err(bad("portfolio_study.limits"));
            }
            (0..n)
                .map(|i| {
                    contracts::DbCounter::new(request.evaluation_start_ns.get() + i * step)
                        .map_err(|_| bad("portfolio_study.time"))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        contracts::portfolio::RebalanceKind::CalendarSession => calendar_cutoffs(request)?,
    };
    let ttl = u64::from(schedule.target_ttl_seconds) * 1_000_000_000;
    let n = cutoffs.len() as u64;
    if !(2..=256).contains(&n)
        || cutoffs.first() != Some(&request.evaluation_start_ns)
        || request
            .members
            .iter()
            .any(|m| m.parameters.total_fuel.get() < n)
        || (request.assets.len() as u64) * (request.members.len() as u64) * n
            > contracts::portfolio::MAX_RETURN_VALUES as u64
    {
        return Err(bad("portfolio_study.limits"));
    }
    for (i, cutoff) in cutoffs.iter().enumerate() {
        let until = cutoff
            .get()
            .checked_add(ttl)
            .filter(|until| *until <= i64::MAX as u64)
            .ok_or_else(|| bad("portfolio_study.time"))?;
        let next = cutoffs
            .get(i + 1)
            .unwrap_or(&request.source_selection.event_end_ns);
        if cutoff >= next || until < next.get() {
            return Err(bad("portfolio_study.schedule_coverage"));
        }
    }
    Ok(cutoffs)
}

fn calendar_cutoffs(
    request: &NativePortfolioStudyRequestV1,
) -> Result<Vec<contracts::DbCounter>, DomainError> {
    let calendar = &request
        .calendar
        .as_ref()
        .ok_or_else(|| bad("portfolio_study.calendar_missing"))?
        .calendar;
    let schedule = &request.mandate.rebalance_schedule;
    crate::control::text(&calendar.calendar_ref, 1, 120, false)?;
    crate::control::text(&calendar.calendar_version, 1, 120, false)?;
    crate::control::text(&calendar.source_reference, 1, 2000, false)?;
    if request.manual_cutoffs_ns.is_some()
        || schedule.calendar_ref.as_ref() != Some(&calendar.calendar_ref)
        || schedule.timezone != calendar.timezone
        || calendar.available_at_ns > request.evaluation_start_ns
        || calendar.coverage_start_ns >= calendar.coverage_end_ns
        || !(1..=4096).contains(&calendar.sessions.len())
    {
        return Err(bad("portfolio_study.calendar_binding"));
    }
    let offset = i64::from(
        schedule
            .session_offset_seconds
            .ok_or_else(|| bad("portfolio_study.calendar_offset"))?,
    ) * 1_000_000_000;
    let start = request
        .evaluation_start_ns
        .get()
        .checked_add_signed(-offset)
        .ok_or_else(|| bad("portfolio_study.calendar_time"))?;
    let end = request
        .source_selection
        .event_end_ns
        .get()
        .checked_add_signed(-offset)
        .ok_or_else(|| bad("portfolio_study.calendar_time"))?;
    if calendar.coverage_start_ns.get() > start || calendar.coverage_end_ns.get() < end {
        return Err(bad("portfolio_study.calendar_coverage"));
    }
    let mut cutoffs = Vec::new();
    let mut previous_close = None;
    for session in &calendar.sessions {
        if session.open_ns >= session.close_ns
            || previous_close.is_some_and(|close| session.open_ns < close)
            || session.close_ns < calendar.coverage_start_ns
            || session.close_ns >= calendar.coverage_end_ns
        {
            return Err(bad("portfolio_study.calendar_sessions"));
        }
        previous_close = Some(session.close_ns);
        if session.close_ns.get() >= start && session.close_ns.get() < end {
            cutoffs.push(
                contracts::DbCounter::new(
                    session
                        .close_ns
                        .get()
                        .checked_add_signed(offset)
                        .ok_or_else(|| bad("portfolio_study.calendar_time"))?,
                )
                .map_err(|_| bad("portfolio_study.calendar_time"))?,
            );
        }
    }
    Ok(cutoffs)
}

pub fn portfolio_study_liquidity_assets(
    request: &NativePortfolioStudyRequestV1,
    cutoff: contracts::DbCounter,
    forecast_asof: contracts::DbCounter,
    decision: contracts::DbCounter,
    values: &[contracts::execution::NativeBarNotionalV1],
) -> Result<Vec<contracts::portfolio::AllocationAssetV1>, DomainError> {
    let mut assets = request.assets.clone();
    let Some(policy) = &request.rolling_liquidity else {
        if !values.is_empty() {
            return Err(bad("portfolio_study.unbound_liquidity"));
        }
        return Ok(assets);
    };
    if values.len() != assets.len() {
        return Err(bad("portfolio_study.liquidity_assets"));
    }
    crate::portfolio::bar_liquidity_age(
        values,
        &request.mandate.base_currency,
        policy.maximum_age_seconds,
        decision,
    )?;
    for (value, asset) in values.iter().zip(&mut assets) {
        crate::catalogs::bar_notional(value)?;
        if value.instrument_id != asset.instrument_id
            || value.currency != asset.currency
            || value.event_ns != forecast_asof
            || value.event_ns < request.source_selection.event_start_ns
            || value.event_ns >= cutoff
            || value.available_ns > cutoff
        {
            return Err(bad("portfolio_study.liquidity_source"));
        }
        asset.available_notional = Some(value.notional_value.clone());
    }
    Ok(assets)
}

/// Verify original report bytes before using the frozen numerical copy.
pub fn portfolio_build_liquidity(
    request: &NativePortfolioBuildRequestV1,
    report: &contracts::execution::NativeDataQualityReportV1,
) -> Result<(), DomainError> {
    portfolio_build_request(request)?;
    super::output::quality(report)?;
    let binding = request
        .bar_liquidity
        .as_ref()
        .ok_or_else(|| bad("portfolio.liquidity_binding"))?;
    let quality = report
        .datasets
        .iter()
        .find(|q| q.dataset_revision_id == binding.source.dataset_revision_id)
        .ok_or_else(|| bad("portfolio.liquidity_source"))?;
    if quality.selection != binding.source.selection {
        return Err(bad("portfolio.liquidity_source"));
    }
    let values = crate::portfolio::bar_liquidity_values(
        &binding.assumption,
        quality,
        &request.mandate.base_currency,
        request.selection.decision_cutoff_ns,
    )?;
    if values.len() != request.assets.len()
        || values.iter().zip(&request.assets).any(|(value, asset)| {
            value.instrument_id != asset.instrument_id
                || value.currency != asset.currency
                || asset.available_notional.as_ref() != Some(&value.notional_value)
        })
    {
        return Err(bad("portfolio.liquidity_values"));
    }
    Ok(())
}

pub fn portfolio_build_result(
    request: &NativePortfolioBuildRequestV1,
    result: &NativePortfolioBuildResultV1,
) -> Result<(), DomainError> {
    portfolio_build_request(request)?;
    let input = &result.input;
    let m = &request.mandate;
    let forecasts = &input.forecasts;
    let maximum_fuel = request
        .members
        .iter()
        .map(|m| m.parameters.total_fuel.get())
        .sum::<u64>();
    if input.objective != m.objective
        || input.risk != m.risk_measure
        || input.base_currency != m.base_currency
        || input.capital_assumption != m.capital_assumption
        || input.current_cash_weight != request.current_weights.cash_weight
        || input.exposure_tolerance != m.exposure_tolerance
        || input.constraints != m.constraints
        || input.optimizer != m.optimizer
        || input.alpha_ensemble != m.alpha_ensemble
        || input.covariance_estimator != m.covariance_estimator
        || input.assets != portfolio_execution_costs(request, &result.slippage_references)?
        || result
            .slippage_references
            .iter()
            .any(|r| r.event_ns != forecasts.forecast_asof_ns)
        || forecasts.bar_types != request.selection.bar_types
        || forecasts.decision_asof_ns != request.selection.decision_cutoff_ns
        || forecasts.forecast_asof_ns < request.selection.event_start_ns
        || forecasts.forecast_asof_ns >= request.selection.event_end_ns
        || forecasts.max_input_age_seconds != m.rebalance_schedule.max_input_age_seconds
        || forecasts.members.len() != request.members.len()
        || result.consumed_fuel.get() > maximum_fuel
        || result.allocation.iterations
            > crate::portfolio::optimizer_settings(&m.optimizer)?.max_iterations
    {
        return Err(bad("portfolio.result_binding"));
    }
    for (actual, expected) in forecasts.members.iter().zip(&request.members) {
        if actual.alpha_id != expected.alpha_id
            || actual.alpha_version_id != expected.alpha_version_id
            || actual.ensemble_weight != expected.ensemble_weight
            || actual.horizon_value.get()
                != u64::from(expected.parameters.label_horizon_observations)
        {
            return Err(bad("portfolio.forecast_binding"));
        }
    }
    if input
        .return_history
        .end_ns
        .iter()
        .any(|t| *t < request.selection.event_start_ns || *t >= request.selection.event_end_ns)
    {
        return Err(bad("portfolio.return_window"));
    }
    crate::portfolio::allocation_result(input, &result.allocation)
}
