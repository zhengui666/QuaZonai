//! Historical replay settings validation only; no valuation or return derivation.
use crate::DomainError;
use bigdecimal::BigDecimal;
use contracts::{
    science::{NativeAccountKind, NativeSimulationSettingsV1},
    spot_cash::*,
};
use std::collections::BTreeSet;
fn invalid(field: &'static str) -> DomainError {
    DomainError::Invalid(field)
}
fn text_ok(text: &str) -> bool {
    !text.is_empty() && text.len() <= 200 && text.trim() == text
}
pub fn account_plan(
    settings: &NativeSimulationSettingsV1,
) -> Result<NativeSpotCashAccountPlanV1, DomainError> {
    let policy = settings
        .multi_currency_spot_cash
        .as_ref()
        .ok_or_else(|| invalid("spot_cash_mode_missing"))?;
    if settings.account_kind != NativeAccountKind::Cash
        || settings.leverage.as_decimal() != &BigDecimal::from(1)
        || !settings.starting_capital.is_positive()
        || policy.report_currency != settings.base_currency
        || !contracts::research_currency::supported(&policy.report_currency)
        || policy.maximum_price_age_ns.get() == 0
        || policy.allowed_instrument_ids.is_empty()
        || !(1..=86_400_000).contains(&settings.snapshot_interval_ms)
    {
        return Err(invalid("spot_cash_settings"));
    }
    let mut ids = BTreeSet::new();
    if policy
        .allowed_instrument_ids
        .iter()
        .any(|id| !text_ok(id) || !ids.insert(id))
    {
        return Err(invalid("spot_cash_allowed_instruments"));
    }
    Ok(NativeSpotCashAccountPlanV1 {
        account_kind: NativeAccountKind::Cash,
        native_base_currency: None,
        allow_cash_borrowing: false,
        leverage: "1".parse().expect("constant decimal"),
        starting_balances: vec![NativeSpotCashMoneyV1 {
            currency: policy.report_currency.clone(),
            amount: settings.starting_capital.clone(),
        }],
    })
}

/// Narrow validation for the owned spot replay branch. The legacy simulation
/// entrypoint remains closed until its producer and consumers are integrated.
pub fn execution_settings(settings: &NativeSimulationSettingsV1) -> Result<(), DomainError> {
    account_plan(settings)?;
    crate::portfolio::simulation_models(settings)?;
    let contracts::portfolio::NativeModelRefV1::FrozenSpotFeeScenario { .. } = &settings.fee_model
    else {
        return Err(invalid("spot_cash_explicit_public_fee_scenario_required"));
    };
    let policy = settings
        .multi_currency_spot_cash
        .as_ref()
        .expect("validated policy");
    if !settings.exposure_tolerance.is_positive()
        || settings.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || settings.fee_rates.len() != policy.allowed_instrument_ids.len()
    {
        return Err(invalid("spot_cash_execution_settings"));
    }
    let mut ids = BTreeSet::new();
    for fee in &settings.fee_rates {
        if !policy.allowed_instrument_ids.contains(&fee.instrument_id)
            || !ids.insert(&fee.instrument_id)
            || fee.maker.as_decimal() != &BigDecimal::new(4.into(), 4)
            || fee.taker.as_decimal() != &BigDecimal::new(7.into(), 4)
        {
            return Err(invalid("spot_cash_fee_planning_scenario_mismatch"));
        }
    }
    Ok(())
}
