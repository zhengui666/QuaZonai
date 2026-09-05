//! Arithmetic for an admission transaction, not an in-memory scheduler or queue.
use contracts::{
    budget::{BudgetV1, CostEnforcement, StopRuleV1},
    runs::ProjectState,
    DbCounter, DecimalValue,
};

use crate::DomainError;

/// Values read while holding the cycle lock. CPU is the total budget already
/// committed to attempts, including consumed grants; it cannot be reset by retry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetUsage {
    pub reserved_experiments: u32,
    pub used_experiments: u32,
    pub reserved_cpu_seconds: DbCounter,
    pub active_runs: u16,
    pub reserved_tokens: DbCounter,
    pub used_tokens: DbCounter,
    /// Required when a cost cap exists. None is unknown, never an implicit zero.
    pub cost: Option<CostUsage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostUsage {
    pub currency: String,
    pub reserved: DecimalValue,
    /// Settled, still estimated charges. This is not an exact provider invoice.
    pub used: DecimalValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostEstimate {
    pub currency: String,
    pub amount: DecimalValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelReservation {
    /// A bounded native model request, not an Agent-supplied usage claim.
    pub tokens: DbCounter,
    pub estimated_cost: Option<CostEstimate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reservation {
    /// Optuna jobs must reserve every internal trial, not just one job slot.
    pub experiments: u32,
    pub cpu_seconds: DbCounter,
    pub wall_seconds: u32,
    pub memory_mib: u32,
    pub output_bytes: DbCounter,
    /// None denotes a non-model job. The trusted dispatcher determines this.
    pub model: Option<ModelReservation>,
}

pub fn validate_budget(budget: &BudgetV1, stop: &StopRuleV1) -> Result<(), DomainError> {
    if budget.max_experiments == 0
        || budget.max_parallel_runs == 0
        || budget.max_turns_per_mission == 0
        || budget.max_wall_seconds == 0
        || budget.max_cpu_seconds.get() == 0
        || budget.max_memory_mib == 0
        || budget.max_output_bytes.get() == 0
        || budget.max_cycles_per_day == 0
        || budget.max_repair_turns > budget.max_turns_per_mission
        || budget.max_tokens.is_some_and(|value| value.get() == 0)
    {
        return Err(DomainError::Invalid("budget"));
    }
    if stop.stop_on_qualified_count == 0
        || u32::from(stop.stop_on_qualified_count) > budget.max_experiments
        || stop.stop_on_no_improvement_trials == Some(0)
    {
        return Err(DomainError::Invalid("stop_rule"));
    }
    match (
        &budget.max_cost_decimal,
        &budget.cost_currency,
        budget.cost_enforcement,
    ) {
        (None, None, CostEnforcement::Unavailable) => {}
        (Some(amount), Some(currency), CostEnforcement::Estimated)
            if amount.is_positive() && iso_currency::Currency::from_code(currency).is_some() => {}
        (_, _, CostEnforcement::Exact) => {
            return Err(DomainError::CapabilityUnavailable("exact_cost_enforcement"));
        }
        _ => return Err(DomainError::Invalid("cost_budget")),
    }
    Ok(())
}

pub fn reserve(
    project: ProjectState,
    budget: &BudgetV1,
    stop: &StopRuleV1,
    usage: &BudgetUsage,
    request: &Reservation,
) -> Result<BudgetUsage, DomainError> {
    validate_budget(budget, stop)?;
    if project != ProjectState::Active {
        return Err(DomainError::AdmissionClosed);
    }
    if request.experiments == 0
        || request.cpu_seconds.get() == 0
        || request.wall_seconds == 0
        || request.memory_mib == 0
        || request.output_bytes.get() == 0
    {
        return Err(DomainError::Invalid("reservation"));
    }
    if request.wall_seconds > budget.max_wall_seconds
        || request.memory_mib > budget.max_memory_mib
        || request.output_bytes > budget.max_output_bytes
    {
        return Err(DomainError::BudgetExhausted("job_resource_limit"));
    }
    let reserved_experiments = usage
        .reserved_experiments
        .checked_add(request.experiments)
        .ok_or(DomainError::BudgetExhausted("experiments"))?;
    let all_experiments = usage
        .used_experiments
        .checked_add(reserved_experiments)
        .ok_or(DomainError::BudgetExhausted("experiments"))?;
    if all_experiments > budget.max_experiments {
        return Err(DomainError::BudgetExhausted("experiments"));
    }
    let reserved_cpu_seconds = usage
        .reserved_cpu_seconds
        .checked_add(request.cpu_seconds.get())
        .ok_or(DomainError::BudgetExhausted("cpu_seconds"))?;
    if reserved_cpu_seconds > budget.max_cpu_seconds {
        return Err(DomainError::BudgetExhausted("cpu_seconds"));
    }
    let active_runs = usage
        .active_runs
        .checked_add(1)
        .ok_or(DomainError::BudgetExhausted("parallel_runs"))?;
    if active_runs > budget.max_parallel_runs {
        return Err(DomainError::BudgetExhausted("parallel_runs"));
    }
    let requested_tokens = request.model.as_ref().map_or(0, |model| model.tokens.get());
    if request.model.is_some() && requested_tokens == 0 {
        return Err(DomainError::Invalid("model_token_reservation"));
    }
    let reserved_tokens = usage
        .reserved_tokens
        .checked_add(requested_tokens)
        .ok_or(DomainError::BudgetExhausted("tokens"))?;
    let total_tokens = usage
        .used_tokens
        .checked_add(reserved_tokens.get())
        .ok_or(DomainError::BudgetExhausted("tokens"))?;
    if budget.max_tokens.is_some_and(|limit| total_tokens > limit) {
        return Err(DomainError::BudgetExhausted("tokens"));
    }
    let cost = reserve_cost(budget, usage, request)?;
    Ok(BudgetUsage {
        reserved_experiments,
        used_experiments: usage.used_experiments,
        reserved_cpu_seconds,
        active_runs,
        reserved_tokens,
        used_tokens: usage.used_tokens,
        cost,
    })
}

fn reserve_cost(
    budget: &BudgetV1,
    usage: &BudgetUsage,
    request: &Reservation,
) -> Result<Option<CostUsage>, DomainError> {
    let estimate = request
        .model
        .as_ref()
        .and_then(|model| model.estimated_cost.as_ref());
    let Some(limit) = &budget.max_cost_decimal else {
        if usage.cost.is_some() || estimate.is_some() {
            return Err(DomainError::Invalid("unconfigured_cost_accounting"));
        }
        return Ok(None);
    };
    let currency = budget
        .cost_currency
        .as_ref()
        .ok_or(DomainError::Invalid("cost_currency"))?;
    let known = usage
        .cost
        .as_ref()
        .ok_or(DomainError::CapabilityUnavailable("cost_usage_unknown"))?;
    if &known.currency != currency
        || !known.reserved.is_nonnegative()
        || !known.used.is_nonnegative()
    {
        return Err(DomainError::Invalid("cost_usage"));
    }
    if request.model.is_some() && estimate.is_none() {
        return Err(DomainError::CapabilityUnavailable("cost_estimate_missing"));
    }
    let reserved = match estimate {
        Some(estimate) => {
            if &estimate.currency != currency || !estimate.amount.is_nonnegative() {
                return Err(DomainError::Invalid("cost_estimate"));
            }
            known
                .reserved
                .checked_add(&estimate.amount)
                .ok_or(DomainError::BudgetExhausted("estimated_cost"))?
        }
        None => known.reserved.clone(),
    };
    let total = known
        .used
        .checked_add(&reserved)
        .ok_or(DomainError::BudgetExhausted("estimated_cost"))?;
    if &total > limit {
        return Err(DomainError::BudgetExhausted("estimated_cost"));
    }
    Ok(Some(CostUsage {
        currency: currency.clone(),
        reserved,
        used: known.used.clone(),
    }))
}
