//! Arithmetic for an admission transaction, not an in-memory scheduler or queue.
use qz_contracts::{
    budget::{BudgetV1, CostEnforcement, StopRuleV1},
    runs::ProjectState,
    DbCounter,
};

use crate::DomainError;

/// Values read while holding the cycle lock. CPU is the total budget already
/// committed to attempts, including consumed grants; it cannot be reset by retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetUsage {
    pub reserved_experiments: u32,
    pub used_experiments: u32,
    pub reserved_cpu_seconds: DbCounter,
    pub active_runs: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reservation {
    /// Optuna jobs must reserve every internal trial, not just one job slot.
    pub experiments: u32,
    pub cpu_seconds: DbCounter,
    pub wall_seconds: u32,
    pub memory_mib: u32,
    pub output_bytes: DbCounter,
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
            if amount.is_positive()
                && currency.len() == 3
                && currency.bytes().all(|byte| byte.is_ascii_uppercase()) => {}
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
    usage: BudgetUsage,
    request: Reservation,
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
    Ok(BudgetUsage {
        reserved_experiments,
        used_experiments: usage.used_experiments,
        reserved_cpu_seconds,
        active_runs,
    })
}
