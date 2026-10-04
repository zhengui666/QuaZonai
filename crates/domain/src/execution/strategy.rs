//! Fixed target composition. No forecast, estimator, qualification or optimizer.
use super::{artifact, bad, features, selection};
use crate::DomainError;
use bigdecimal::BigDecimal;
use contracts::{
    research::{ArtifactInputRole, DataPartition},
    runtime_jobs::{JobSpecV1, RuntimeInputV1},
    science::*,
    strategy_portfolio::*,
    DbCounter, DecimalValue, Id, SchemaV1,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn mandate(value: &StrategyMandateContentV1) -> Result<(), DomainError> {
    crate::portfolio::portfolio_constraints(&value.constraints)?;
    if !value.capital_assumption.is_positive()
        || !value.exposure_tolerance.is_positive()
        || value.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !contracts::research_currency::supported(&value.base_currency)
        || value.max_input_age_seconds == 0
        || value.target_ttl_seconds == 0
    {
        return Err(bad("strategy.mandate"));
    }
    // These need an explicit risk model, group dictionary or liquidity source;
    // a fixed target policy cannot manufacture those measurements.
    if value.constraints.max_ex_ante_risk.is_some()
        || value.constraints.max_participation.is_some()
        || value.constraints.liquidity_ref.is_some()
        || !value.constraints.group_bounds.is_empty()
    {
        return Err(DomainError::CapabilityUnavailable(
            "strategy_constraint_measurement",
        ));
    }
    Ok(())
}

pub fn build(value: &StrategyPortfolioBuildV1) -> Result<(), DomainError> {
    crate::data::bounded_native_limits(&value.limits)?;
    if !(1..=256).contains(&value.members.len())
        || value
            .members
            .iter()
            .map(|m| m.alpha_version_id)
            .collect::<BTreeSet<_>>()
            .len()
            != value.members.len()
    {
        return Err(bad("strategy.members"));
    }
    weights(value.members.iter().map(|m| &m.ensemble_weight))?;
    if let StrategyPortfolioPurposeV1::CurrentDecision {
        account_start,
        member_inputs,
    } = &value.purpose
    {
        account(account_start)?;
        let ids = member_inputs
            .iter()
            .map(|m| m.alpha_version_id)
            .collect::<BTreeSet<_>>();
        if ids.len() != member_inputs.len()
            || ids != value.members.iter().map(|m| m.alpha_version_id).collect()
        {
            return Err(bad("strategy.current_members"));
        }
        for input in member_inputs {
            features::artifact_ids(&input.feature_artifact_ids)?;
        }
    }
    Ok(())
}

fn weights<'a>(values: impl Iterator<Item = &'a DecimalValue>) -> Result<(), DomainError> {
    let mut total = BigDecimal::from(0);
    for weight in values {
        if !weight.is_fraction() {
            return Err(bad("strategy.weight"));
        }
        total += weight.as_decimal();
    }
    if total != BigDecimal::from(1) {
        return Err(bad("strategy.weight_sum"));
    }
    Ok(())
}
fn account(value: &FreshPaperCashV1) -> Result<(), DomainError> {
    crate::control::text(&value.trader_id, 1, 200, false)?;
    crate::control::text(&value.account_id, 1, 200, false)?;
    if !value.starting_capital.is_positive()
        || !contracts::research_currency::supported(&value.base_currency)
    {
        return Err(bad("strategy.account_start"));
    }
    Ok(())
}

pub fn request(value: &NativeStrategyCompositionRequestV1) -> Result<(), DomainError> {
    crate::data::recorded_feature_provenance(&value.input_provenance)?;
    selection(&value.selection)?;
    mandate(&value.mandate)?;
    crate::portfolio::simulation_settings(&value.settings)?;
    if !(1..=256).contains(&value.members.len())
        || !(1..=1_000_000_000).contains(&value.total_fuel.get())
        || value.settings.base_currency != value.mandate.base_currency
        || value.settings.starting_capital != value.mandate.capital_assumption
        || value.settings.leverage.as_decimal() != &BigDecimal::from(1)
        || value.settings.exposure_tolerance != value.mandate.exposure_tolerance
    {
        return Err(bad("strategy.request"));
    }
    weights(value.members.iter().map(|m| &m.ensemble_weight))?;
    let mut members = BTreeSet::new();
    let mut feature_ids = BTreeSet::new();
    let instrument = &value.members[0].policy.instrument_id;
    // Existing external FeatureModel policies have one traded instrument. Members
    // must have the same whole target universe, never an implicit sparse union.
    if value.selection.bar_types.len() != 1
        || value.selection.bar_types[0].rsplitn(5, '-').nth(4) != Some(instrument.as_str())
        || value.settings.fee_rates.len() != 1
        || value.settings.fee_rates[0].instrument_id != *instrument
    {
        return Err(bad("strategy.instrument_alignment"));
    }
    for member in &value.members {
        let policy = &member.policy;
        features::schema(&policy.feature_schema)?;
        features::artifact_ids(&policy.feature_artifact_ids)?;
        if !(1..=2 * MAX_FEATURE_ARTIFACTS).contains(&member.feature_artifact_ids.len())
            || member
                .feature_artifact_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != member.feature_artifact_ids.len()
        {
            return Err(bad("strategy.feature_artifacts"));
        }
        feature_ids.extend(member.feature_artifact_ids.iter().copied());
        if !members.insert(member.alpha_version_id)
            || policy.model_abi != FEATURE_MODEL_ABI_V2
            || policy.instrument_id != *instrument
            || policy.base_currency != value.mandate.base_currency
            || policy.target_ttl_ns == DbCounter::ZERO
            || policy.initialization.first_event_ns > policy.initialization.first_decision_ns
        {
            return Err(bad("strategy.member"));
        }
        match &value.purpose {
            StrategyPortfolioPurposeV1::HistoricalReplay {} => {
                if member.feature_artifact_ids != policy.feature_artifact_ids {
                    return Err(bad("strategy.historical_features"));
                }
            }
            StrategyPortfolioPurposeV1::CurrentDecision { member_inputs, .. } => {
                let supplied = member_inputs
                    .iter()
                    .find(|m| m.alpha_version_id == member.alpha_version_id)
                    .ok_or_else(|| bad("strategy.current_members"))?;
                let expected = policy
                    .feature_artifact_ids
                    .iter()
                    .chain(&supplied.feature_artifact_ids)
                    .copied()
                    .collect::<Vec<_>>();
                if member.feature_artifact_ids != expected
                    || expected.iter().copied().collect::<BTreeSet<_>>().len() != expected.len()
                {
                    return Err(bad("strategy.continuation_features"));
                }
            }
        }
    }
    if value
        .input_provenance
        .feature_artifact_origins
        .keys()
        .copied()
        .collect::<BTreeSet<_>>()
        != feature_ids
    {
        return Err(bad("strategy.feature_provenance"));
    }
    if let StrategyPortfolioPurposeV1::CurrentDecision {
        account_start,
        member_inputs,
    } = &value.purpose
    {
        account(account_start)?;
        if account_start.base_currency != value.mandate.base_currency
            || account_start.starting_capital != value.mandate.capital_assumption
            || account_start.execution_assumptions_id != value.mandate.execution_assumptions_id
            || member_inputs.len() != members.len()
            || member_inputs
                .iter()
                .map(|m| m.alpha_version_id)
                .collect::<BTreeSet<_>>()
                != members
        {
            return Err(bad("strategy.account_binding"));
        }
    }
    Ok(())
}

pub(super) fn task(
    spec: &JobSpecV1,
    dataset: Id,
    value: &NativeStrategyCompositionRequestV1,
) -> Result<(), DomainError> {
    request(value)?;
    if dataset != value.input_provenance.dataset_revision_id {
        return Err(bad("strategy.dataset"));
    }
    let partition = match value.purpose {
        StrategyPortfolioPurposeV1::HistoricalReplay {} => DataPartition::Validation,
        StrategyPortfolioPurposeV1::CurrentDecision { .. } => DataPartition::Forward,
    };
    let mut objects = BTreeMap::new();
    objects.insert(spec.parameters_artifact_id, ArtifactInputRole::Parameters);
    objects.insert(
        value.mandate.constraints.transaction_costs_ref,
        ArtifactInputRole::Parameters,
    );
    for member in &value.members {
        objects.insert(member.policy.model_artifact_id, ArtifactInputRole::Model);
        objects.insert(
            member.policy.source.report_artifact_id,
            ArtifactInputRole::Report,
        );
        for id in &member.feature_artifact_ids {
            objects.insert(*id, ArtifactInputRole::Parameters);
        }
    }
    if !spec.inputs.iter().any(|i| matches!(i,RuntimeInputV1::Dataset { revision_id,role,.. } if *revision_id==dataset && *role==partition))
        || objects.iter().any(|(id, role)| !artifact(spec,*id,*role))
        || spec.inputs.iter().any(|i| match i {
            RuntimeInputV1::Dataset {revision_id,role,..} => *revision_id!=dataset || *role!=partition,
            RuntimeInputV1::Artifact {artifact_id,role,..} => objects.get(artifact_id)!=Some(role),
        })
    { return Err(bad("strategy.inputs")); }
    Ok(())
}

/// The selected original fold, including its exact model and observation identity.
pub fn source<'a>(
    policy: &FrozenTargetPolicyV1,
    report: &'a NativeExperimentEvaluationResultV1,
) -> Result<&'a NativeExperimentFoldV1, DomainError> {
    super::check_experiment_evaluation(
        &report.request,
        report.dataset_revision_id,
        report.model_artifact_id,
        &report.feature_artifact_ids,
        report,
    )?;
    let fold = report
        .folds
        .get(usize::from(policy.initialization.source_fold_index))
        .ok_or_else(|| bad("strategy.source_fold"))?;
    let first = fold
        .decisions
        .first()
        .ok_or_else(|| bad("strategy.source_fold"))?;
    if policy.dataset_revision_id != report.dataset_revision_id
        || policy.model_artifact_id != report.model_artifact_id
        || policy.feature_artifact_ids != report.feature_artifact_ids
        || policy.feature_schema != report.request.feature_schema
        || policy.instrument_id != report.instrument_id
        || policy.base_currency != report.request.settings.base_currency
        || policy.target_ttl_ns != report.request.target_ttl_ns
        || policy.model_abi != FEATURE_MODEL_ABI_V2
        || policy.initialization.first_ordinal != first.ordinal
        || policy.initialization.first_event_ns != first.event_ns
        || policy.initialization.first_decision_ns != first.decision_ns
    {
        return Err(bad("strategy.source_binding"));
    }
    Ok(fold)
}

fn decimal(value: BigDecimal) -> Result<DecimalValue, DomainError> {
    value
        .to_plain_string()
        .parse()
        .map_err(|_| bad("strategy.target_precision"))
}

/// Combine target weights and cash only; account equity is never an input.
pub fn blend(
    members: &[NativeStrategyMemberV1],
    points: &[&NativeTargetPointV1],
    ttl_seconds: u32,
) -> Result<NativeTargetPointV1, DomainError> {
    if members.is_empty() || members.len() != points.len() {
        return Err(bad("strategy.target_members"));
    }
    weights(members.iter().map(|m| &m.ensemble_weight))?;
    let first = points[0];
    let mut until = first
        .asof_ns
        .get()
        .checked_add(u64::from(ttl_seconds) * 1_000_000_000)
        .ok_or_else(|| bad("strategy.target_clock"))?;
    let mut assets = vec![BigDecimal::from(0); first.targets.len()];
    let mut cash = BigDecimal::from(0);
    for (member, point) in members.iter().zip(points) {
        if point.asof_ns != first.asof_ns
            || point.targets.len() != first.targets.len()
            || point.valid_until_ns <= point.asof_ns
            || !point.cash_weight.is_fraction()
        {
            return Err(bad("strategy.target_alignment"));
        }
        let mut total = point.cash_weight.as_decimal().clone();
        for ((sum, target), expected) in assets.iter_mut().zip(&point.targets).zip(&first.targets) {
            if target.instrument_id != expected.instrument_id
                || target.currency != expected.currency
                || !target.weight.is_fraction()
            {
                return Err(bad("strategy.target_alignment"));
            }
            total += target.weight.as_decimal();
            *sum += member.ensemble_weight.as_decimal() * target.weight.as_decimal();
        }
        if total != BigDecimal::from(1) {
            return Err(bad("strategy.target_sum"));
        }
        cash += member.ensemble_weight.as_decimal() * point.cash_weight.as_decimal();
        until = until.min(point.valid_until_ns.get());
    }
    if until <= first.asof_ns.get() {
        return Err(bad("strategy.target_clock"));
    }
    Ok(NativeTargetPointV1 {
        schema_version: SchemaV1,
        asof_ns: first.asof_ns,
        valid_until_ns: DbCounter::new(until).map_err(|_| bad("strategy.target_clock"))?,
        targets: first
            .targets
            .iter()
            .zip(assets)
            .map(|(target, weight)| {
                Ok(contracts::portfolio::AllocationTargetV1 {
                    instrument_id: target.instrument_id.clone(),
                    currency: target.currency.clone(),
                    weight: decimal(weight)?,
                })
            })
            .collect::<Result<_, DomainError>>()?,
        cash_weight: decimal(cash)?,
    })
}

/// Enforce declared bounds against the actual account's current weights. Current
/// decisions pass the declared fresh cash vector; historical replay passes native
/// settled positions valued at the same decision cross-section.
pub fn constraints(
    mandate: &StrategyMandateContentV1,
    point: &NativeTargetPointV1,
    current: &[DecimalValue],
) -> Result<(), DomainError> {
    self::mandate(mandate)?;
    target_bounds(
        &mandate.constraints,
        &mandate.base_currency,
        &mandate.exposure_tolerance,
        point,
        current,
    )
}

pub fn target_bounds(
    c: &contracts::portfolio::PortfolioConstraintsV1,
    currency: &str,
    exposure_tolerance: &DecimalValue,
    point: &NativeTargetPointV1,
    current: &[DecimalValue],
) -> Result<(), DomainError> {
    crate::portfolio::portfolio_constraints(c)?;
    if c.max_ex_ante_risk.is_some()
        || c.max_participation.is_some()
        || c.liquidity_ref.is_some()
        || !c.group_bounds.is_empty()
    {
        return Err(DomainError::CapabilityUnavailable(
            "strategy_constraint_measurement",
        ));
    }
    if !exposure_tolerance.is_positive()
        || exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
    {
        return Err(bad("strategy.exposure_tolerance"));
    }
    let tolerance = exposure_tolerance.as_decimal();
    if point.targets.is_empty()
        || point.targets.len() != current.len()
        || point.asof_ns >= point.valid_until_ns
    {
        return Err(bad("strategy.target"));
    }
    let within = |value: &BigDecimal, low: &DecimalValue, high: &DecimalValue| {
        value >= &(low.as_decimal() - tolerance) && value <= &(high.as_decimal() + tolerance)
    };
    let mut net = BigDecimal::from(0);
    let mut gross = BigDecimal::from(0);
    let mut turnover = BigDecimal::from(0);
    let mut ids = BTreeSet::new();
    for (target, old) in point.targets.iter().zip(current) {
        if !ids.insert(&target.instrument_id)
            || target.currency != currency
            || !target.weight.is_fraction()
        {
            return Err(bad("strategy.target"));
        }
        let bounds = c
            .asset_overrides
            .iter()
            .find(|v| v.instrument_id == target.instrument_id)
            .map(|v| (&v.min, &v.max))
            .unwrap_or((&c.min_asset_weight, &c.max_asset_weight));
        if !within(target.weight.as_decimal(), bounds.0, bounds.1) {
            return Err(bad("strategy.asset_bound"));
        }
        net += target.weight.as_decimal();
        gross += target.weight.as_decimal().abs();
        turnover += (target.weight.as_decimal() - old.as_decimal()).abs();
    }
    if c.asset_overrides
        .iter()
        .any(|v| !ids.contains(&v.instrument_id))
        || !point.cash_weight.is_fraction()
        || (&net + point.cash_weight.as_decimal() - BigDecimal::from(1)).abs() > *tolerance
        || !within(
            point.cash_weight.as_decimal(),
            &c.min_cash_weight,
            &c.max_cash_weight,
        )
        || !within(&net, &c.min_net_exposure, &c.max_net_exposure)
        || gross > c.max_gross_exposure.as_decimal() + tolerance
        || turnover > c.max_turnover_per_rebalance.as_decimal() + tolerance
    {
        return Err(bad("strategy.exposure_bound"));
    }
    Ok(())
}

pub fn result(
    request: &NativeStrategyCompositionRequestV1,
    value: &NativeStrategyCompositionResultV1,
) -> Result<(), DomainError> {
    self::request(request)?;
    if serde_json::to_value(request).ok() != serde_json::to_value(&value.request).ok()
        || value.consumed_fuel > request.total_fuel
        || value.native_versions
            != BTreeMap::from([
                ("nautilus-backtest".into(), "0.63.0".into()),
                ("wasmi".into(), "2.0.0".into()),
                ("strategy-composition".into(), "1".into()),
            ])
    {
        return Err(bad("strategy.result_binding"));
    }
    match (&request.purpose, &value.outcome) {
        (
            StrategyPortfolioPurposeV1::HistoricalReplay {},
            StrategyCompositionOutcomeV1::HistoricalReplay {
                simulation_request,
                simulation,
            },
        ) => {
            if simulation_request.selection != request.selection
                || serde_json::to_value(&simulation_request.settings).ok()
                    != serde_json::to_value(&request.settings).ok()
                || !simulation_request.settlements.is_empty()
                || value.consumed_fuel != DbCounter::ZERO
            {
                return Err(bad("strategy.replay_binding"));
            }
            super::output::check_simulation(simulation_request, simulation)?;
        }
        (
            StrategyPortfolioPurposeV1::CurrentDecision { account_start, .. },
            StrategyCompositionOutcomeV1::CurrentDecision {
                account_start: actual,
                target,
                predictions_per_member,
            },
        ) => {
            if value.consumed_fuel == DbCounter::ZERO
                || account_start != actual.as_ref()
                || target.asof_ns > request.selection.decision_cutoff_ns
                || predictions_per_member.len() != request.members.len()
                || request.members.iter().any(|m| {
                    predictions_per_member
                        .get(&m.alpha_version_id)
                        .is_none_or(|n| n.get() == 0 || n.get() > MAX_EXPERIMENT_DECISIONS as u64)
                })
                || target.targets.len() != 1
                || target.targets[0].instrument_id != request.members[0].policy.instrument_id
                || request.members.iter().any(|m| {
                    target.asof_ns <= m.policy.initialization.first_decision_ns
                        || target
                            .valid_until_ns
                            .get()
                            .saturating_sub(target.asof_ns.get())
                            > m.policy.target_ttl_ns.get()
                })
                || target
                    .valid_until_ns
                    .get()
                    .saturating_sub(target.asof_ns.get())
                    > u64::from(request.mandate.target_ttl_seconds) * 1_000_000_000
            {
                return Err(bad("strategy.current_binding"));
            }
            constraints(
                &request.mandate,
                target,
                &vec![decimal(BigDecimal::from(0))?; target.targets.len()],
            )?;
        }
        _ => return Err(bad("strategy.purpose_binding")),
    }
    Ok(())
}

/// Verify that a current result actually advances beyond the entire selected
/// original fold. The native job additionally checks exact replay prefix values.
pub fn current_continuation(
    fold: &NativeExperimentFoldV1,
    target: &NativeTargetPointV1,
    predictions: DbCounter,
) -> Result<(), DomainError> {
    let last = fold
        .decisions
        .last()
        .ok_or_else(|| bad("strategy.source_fold"))?;
    if predictions.get() <= fold.decisions.len() as u64
        || predictions.get() > MAX_EXPERIMENT_DECISIONS as u64
        || target.asof_ns <= last.decision_ns
    {
        return Err(bad("strategy.current_continuation"));
    }
    Ok(())
}
