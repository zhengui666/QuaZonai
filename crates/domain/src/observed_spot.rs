//! Validation for explicitly observed inventory; never infer missing cash or open orders.
use crate::DomainError;
use contracts::{
    account_observation::NativeAccountTypeV1, observed_spot::NativeObservedSpotStartV1,
    science::NativeSimulationSettingsV1, spot_cash::NativeSpotCashMoneyV1, DbCounter,
};
use std::collections::BTreeSet;

pub fn starting_balances(
    start: &NativeObservedSpotStartV1,
    settings: &NativeSimulationSettingsV1,
    evaluation_start: DbCounter,
) -> Result<Vec<NativeSpotCashMoneyV1>, DomainError> {
    let bad = || DomainError::Invalid("observed_spot_start");
    crate::spot_cash::execution_settings(settings)?;
    crate::control::text(&start.snapshot_evidence_ref, 1, 512, false)?;
    crate::control::text(&start.open_orders_evidence_ref, 1, 512, false)?;
    let snapshot = &start.snapshot;
    if start.source_native_version != "0.63.0"
        || snapshot.account_type != NativeAccountTypeV1::Cash
        || snapshot.base_currency.is_some()
        || snapshot.account_id != start.open_orders_account_id
        || snapshot.account_id.is_empty()
        || snapshot.is_stale
        || !snapshot.stale_instruments.is_empty()
        || !snapshot.stale_currencies.is_empty()
        || !snapshot.unpriced_instruments.is_empty()
        || !snapshot.margins.is_empty()
        || snapshot.ts_event > snapshot.ts_init
        || snapshot.ts_init > start.available_ns
        || start.available_ns > evaluation_start
        || start.open_orders_count != 0
        || start.open_orders_asof_ns != snapshot.ts_event
        || start.open_orders_available_ns < start.open_orders_asof_ns
        || start.open_orders_available_ns > evaluation_start
        || snapshot.balances.is_empty()
    {
        return Err(bad());
    }
    let mut currencies = BTreeSet::new();
    let mut positive = false;
    let mut balances = Vec::new();
    for balance in &snapshot.balances {
        if balance.total.currency.is_empty()
            || balance.total != balance.free
            || balance.total.currency != balance.locked.currency
            || balance.locked.amount != contracts::DecimalValue::zero()
            || !balance.total.amount.is_nonnegative()
            || !currencies.insert(balance.total.currency.clone())
        {
            return Err(bad());
        }
        positive |= balance.total.amount.is_positive();
        balances.push(NativeSpotCashMoneyV1 {
            currency: balance.total.currency.clone(),
            amount: balance.total.amount.clone(),
        });
    }
    if !positive || !currencies.contains(&settings.base_currency) {
        return Err(bad());
    }
    Ok(balances)
}
