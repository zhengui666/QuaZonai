//! Original account identity and input checks for upstream simulation configuration.
use crate::{control, DomainError};
use bigdecimal::BigDecimal;
use contracts::portfolio::*;
use std::collections::BTreeSet;
mod liquidity;
pub use liquidity::{
    bar_liquidity_age, bar_liquidity_assumption, bar_liquidity_values, rolling_liquidity_policy,
};

/// Identity is preserved verbatim. Empty, padded or control-bearing aliases must
/// not create additional initialization scopes for the same native account.
pub fn paper_account_scope(trader_id: &str, account_id: &str) -> Result<(), DomainError> {
    for value in [trader_id, account_id] {
        control::text(value, 1, 200, false)?;
        if value.trim() != value {
            return Err(DomainError::Invalid("paper_initialization_scope"));
        }
    }
    Ok(())
}

/// Compare a supplied lineage to its server-resolved original, never infer it
/// from a caller label, current mandate or new session identity.
pub fn paper_initialization_binding(
    actual: &contracts::science::PaperInitializationRefV1,
    original: &contracts::science::PaperInitializationRefV1,
    downstream_id: contracts::Id,
) -> Result<(), DomainError> {
    paper_account_scope(&actual.trader_id, &actual.account_id)?;
    if actual != original || actual.downstream_id != downstream_id {
        return Err(DomainError::Invalid("paper_initialization_binding"));
    }
    Ok(())
}

pub fn simulation_settings(
    settings: &contracts::science::NativeSimulationSettingsV1,
) -> Result<(), DomainError> {
    // Fail closed until the native adapter, fees and result consumers are wired.
    // Never silently execute the new mode through the legacy single-currency path.
    if settings.multi_currency_spot_cash.is_some() {
        return Err(DomainError::CapabilityUnavailable(
            "multi_currency_spot_cash_not_integrated",
        ));
    }
    simulation_models(settings)?;
    if !settings.starting_capital.is_positive()
        || !settings.leverage.is_positive()
        || settings.leverage.as_decimal() > &BigDecimal::from(100)
        || !settings.exposure_tolerance.is_positive()
        || settings.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || !(1..=86_400_000).contains(&settings.snapshot_interval_ms)
        || !contracts::research_currency::supported(&settings.base_currency)
        || settings.fee_rates.is_empty()
        || (settings.account_kind == contracts::science::NativeAccountKind::Cash
            && settings.leverage.as_decimal() != &BigDecimal::from(1))
    {
        return Err(DomainError::Invalid("simulation_settings"));
    }
    let mut instruments = BTreeSet::new();
    for rate in &settings.fee_rates {
        control::text(&rate.instrument_id, 1, 200, false)?;
        if !instruments.insert(&rate.instrument_id) {
            return Err(DomainError::Invalid("simulation_fee_identity"));
        }
    }
    Ok(())
}

/// Freeze the actual native execution models; no fallback, RNG or fee algorithm here.
pub fn simulation_models(
    settings: &contracts::science::NativeSimulationSettingsV1,
) -> Result<(&NautilusFillParametersV1, &NautilusLatencyParametersV1), DomainError> {
    let unavailable = || DomainError::CapabilityUnavailable("native_simulation_models");
    let fill = match &settings.fill_model {
        NativeModelRefV1::NautilusDefaultFill {
            upstream_class,
            upstream_version,
            parameters,
            ..
        } if upstream_class == NAUTILUS_FILL_CLASS
            && upstream_version == NAUTILUS_EXECUTION_VERSION =>
        {
            parameters
        }
        _ => return Err(unavailable()),
    };
    match &settings.fee_model {
        NativeModelRefV1::FrozenSpotFeeScenario { native_version, parameters, .. }
            if native_version == NAUTILUS_EXECUTION_VERSION
                && settings.multi_currency_spot_cash.is_some()
                && parameters.schedule.source.status == contracts::spot_fees::SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability
                && matches!(parameters.acceptance, contracts::spot_fees::SpotCashFeeAcceptanceV1::PublicRateScenarioUnverifiedApplicability) => {}
        NativeModelRefV1::NautilusMakerTaker {
            upstream_class,
            upstream_version,
            ..
        } if upstream_class == NAUTILUS_FEE_CLASS
            && upstream_version == NAUTILUS_EXECUTION_VERSION => {}
        _ => return Err(unavailable()),
    }
    let latency = match &settings.latency_model {
        NativeModelRefV1::NautilusStaticLatency {
            upstream_class,
            upstream_version,
            parameters,
            ..
        } if upstream_class == NAUTILUS_LATENCY_CLASS
            && upstream_version == NAUTILUS_EXECUTION_VERSION =>
        {
            parameters
        }
        _ => return Err(unavailable()),
    };
    if !fill.prob_fill_on_limit.is_fraction()
        || !fill.prob_slippage.is_fraction()
        || [
            latency.insert_latency_ns,
            latency.update_latency_ns,
            latency.cancel_latency_ns,
        ]
        .iter()
        .any(|value| {
            latency
                .base_latency_ns
                .get()
                .checked_add(value.get())
                .is_none()
        })
        || latency.base_latency_ns.get() + latency.insert_latency_ns.get() == 0
    {
        return Err(DomainError::Invalid("native_simulation_parameters"));
    }
    Ok((fill, latency))
}
