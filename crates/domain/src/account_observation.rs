//! Structure and freshness only. Native Nautilus owns account/PnL valuation.
use crate::{control::text, DomainError};
use contracts::account_observation::*;
use std::collections::BTreeSet;

fn invalid() -> DomainError {
    DomainError::Invalid("native_account_observation")
}

fn currency(value: &str) -> Result<(), DomainError> {
    text(value, 1, 32, false)?;
    if !value
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
    {
        return Err(invalid());
    }
    Ok(())
}

fn money(values: &[AccountMoneyV1]) -> Result<(), DomainError> {
    let mut currencies = BTreeSet::new();
    for value in values {
        currency(&value.currency)?;
        if !currencies.insert(&value.currency) {
            return Err(invalid());
        }
    }
    Ok(())
}

fn labels(values: &[String]) -> Result<(), DomainError> {
    let mut seen = BTreeSet::new();
    for value in values {
        text(value, 1, 200, false)?;
        if !seen.insert(value) {
            return Err(invalid());
        }
    }
    Ok(())
}

pub fn binding(value: &NativeAccountBindingV1) -> Result<(), DomainError> {
    for label in [
        &value.native_trader_id,
        &value.native_session_id,
        &value.native_account_id,
    ] {
        text(label, 1, 200, false)?;
    }
    // The version is a source declaration; compatible typed observations are
    // not rejected merely because the producer has upgraded its native library.
    if value.native_version.trim().is_empty() {
        return Err(invalid());
    }
    Ok(())
}

pub fn observation(value: &AccountObservationSubmitV1) -> Result<(), DomainError> {
    binding(&value.binding)?;
    if value.sequence.get() == 0
        || value.dropped_events >= value.sequence
        || value.observed_at_ns.get() == 0
    {
        return Err(invalid());
    }
    let Some(s) = &value.snapshot else {
        return Ok(());
    };
    if s.account_id != value.binding.native_account_id
        || s.ts_event > s.ts_init
        || s.ts_init > value.observed_at_ns
    {
        return Err(invalid());
    }
    // UUID4 is a native event identity, distinct from Q's UUID7 resource ID.
    let id = s.event_id.as_bytes();
    if id.len() != 36
        || id[14] != b'4'
        || !matches!(id[19], b'8' | b'9' | b'a' | b'b')
        || id.iter().enumerate().any(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                *c != b'-'
            } else {
                !c.is_ascii_hexdigit() || c.is_ascii_uppercase()
            }
        })
    {
        return Err(invalid());
    }
    for values in [&s.total_equity, &s.realized_pnls, &s.unrealized_pnls] {
        money(values)?;
    }
    if let Some(base) = &s.base_currency {
        currency(base)?;
    }
    if let Some(base) = &s.base_currency_equity {
        money(std::slice::from_ref(base))?;
        if s.base_currency.as_ref() != Some(&base.currency) {
            return Err(invalid());
        }
    }
    let mut balances = BTreeSet::new();
    for b in &s.balances {
        currency(&b.total.currency)?;
        if !balances.insert(&b.total.currency)
            || b.total.currency != b.locked.currency
            || b.total.currency != b.free.currency
        {
            return Err(invalid());
        }
    }
    for m in &s.margins {
        currency(&m.initial.currency)?;
        if m.initial.currency != m.maintenance.currency {
            return Err(invalid());
        }
        if let Some(id) = &m.instrument_id {
            text(id, 1, 200, false)?;
        }
    }
    labels(&s.stale_instruments)?;
    labels(&s.unpriced_instruments)?;
    labels(&s.stale_currencies)?;
    for c in &s.stale_currencies {
        currency(c)?;
    }
    if !s.is_stale
        && (!s.stale_instruments.is_empty()
            || !s.stale_currencies.is_empty()
            || !s.unpriced_instruments.is_empty())
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn valuation(snapshot: Option<&NativePortfolioSnapshotV1>) -> AccountValuationV1 {
    match snapshot {
        None => AccountValuationV1::Unavailable,
        Some(s) if !s.unpriced_instruments.is_empty() => AccountValuationV1::Unpriced,
        Some(s) if s.is_stale => AccountValuationV1::Stale,
        Some(s) if s.total_equity.is_empty() => AccountValuationV1::Unavailable,
        Some(_) => AccountValuationV1::Priced,
    }
}

pub fn connection(
    state: AccountConnectionV1,
    observed_ns: u64,
    now_ns: u64,
) -> AccountConnectionFreshnessV1 {
    match state {
        AccountConnectionV1::Disconnected => AccountConnectionFreshnessV1::Disconnected,
        AccountConnectionV1::Unknown => AccountConnectionFreshnessV1::Unknown,
        AccountConnectionV1::Connected
            if observed_ns > now_ns
                || now_ns - observed_ns > CONNECTION_STALE_SECONDS as u64 * 1_000_000_000 =>
        {
            AccountConnectionFreshnessV1::Stale
        }
        AccountConnectionV1::Connected => AccountConnectionFreshnessV1::Connected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::DbCounter;
    fn fixture() -> AccountObservationSubmitV1 {
        serde_json::from_str(include_str!(
            "../../../tests/contracts/native-account-paper-snapshot.json"
        ))
        .unwrap()
    }
    #[test]
    fn preserves_currency_and_precision_without_combining_or_attributing() {
        let value = fixture();
        observation(&value).unwrap();
        let wire = serde_json::to_value(&value).unwrap();
        assert_eq!(wire["snapshot"]["ts_event"], "1700000000000000001");
        assert_eq!(wire["snapshot"]["total_equity"][1]["amount"], "12.3456789");
        assert_eq!(wire["snapshot"]["realized_pnls"][0]["amount"], "-2.51");
        assert!(wire.get("evaluation_id").is_none());
        assert!(wire.get("handoff_id").is_none());
    }
    #[test]
    fn heartbeat_and_native_valuation_are_independent() {
        let mut value = fixture();
        let now = value.observed_at_ns.get();
        assert_eq!(
            connection(AccountConnectionV1::Connected, now, now),
            AccountConnectionFreshnessV1::Connected
        );
        assert_eq!(
            connection(AccountConnectionV1::Connected, now, now + 121_000_000_000),
            AccountConnectionFreshnessV1::Stale
        );
        assert_eq!(
            connection(AccountConnectionV1::Unknown, now, now),
            AccountConnectionFreshnessV1::Unknown
        );
        let snapshot = value.snapshot.as_mut().unwrap();
        snapshot.is_stale = true;
        snapshot.unpriced_instruments.push("BTCUSDT.SIM".into());
        assert_eq!(valuation(Some(snapshot)), AccountValuationV1::Unpriced);
        observation(&value).unwrap();
        value.snapshot = None;
        observation(&value).unwrap();
        assert_eq!(valuation(None), AccountValuationV1::Unavailable);
    }
    #[test]
    fn rejects_wrong_binding_missing_version_clocks_and_duplicate_currency() {
        let mut v = fixture();
        v.snapshot.as_mut().unwrap().account_id = "OTHER-001".into();
        assert!(observation(&v).is_err());
        let mut v = fixture();
        v.binding.native_version.clear();
        assert!(observation(&v).is_err());
        let mut v = fixture();
        v.observed_at_ns = DbCounter::new(1).unwrap();
        assert!(observation(&v).is_err());
        let mut v = fixture();
        let s = v.snapshot.as_mut().unwrap();
        s.total_equity.push(s.total_equity[0].clone());
        assert!(observation(&v).is_err());
        let mut v = fixture();
        v.sequence = DbCounter::ZERO;
        assert!(observation(&v).is_err());
        let mut v = fixture();
        v.dropped_events = v.sequence;
        assert!(observation(&v).is_err());
    }
    #[test]
    fn compatible_native_versions_and_large_native_lists_are_preserved() {
        let mut value = fixture();
        value.binding.native_version = "0.64.0".into();
        let snapshot = value.snapshot.as_mut().unwrap();
        snapshot.total_equity = (0..257)
            .map(|index| AccountMoneyV1 {
                amount: "1".parse().unwrap(),
                currency: format!("C{index}"),
            })
            .collect();
        snapshot.balances = snapshot
            .total_equity
            .iter()
            .map(|money| NativeAccountBalanceV1 {
                total: money.clone(),
                locked: AccountMoneyV1 {
                    amount: "0".parse().unwrap(),
                    currency: money.currency.clone(),
                },
                free: money.clone(),
            })
            .collect();
        snapshot.margins = (0..4097)
            .map(|index| NativeAccountMarginV1 {
                initial: AccountMoneyV1 {
                    amount: "1".parse().unwrap(),
                    currency: "USD".into(),
                },
                maintenance: AccountMoneyV1 {
                    amount: "1".parse().unwrap(),
                    currency: "USD".into(),
                },
                instrument_id: Some(format!("ASSET{index}.SIM")),
            })
            .collect();
        snapshot.is_stale = true;
        snapshot.stale_instruments = (0..4097).map(|index| format!("ASSET{index}.SIM")).collect();
        observation(&value).unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(bytes.len() < 2 * 1024 * 1024);
        let decoded: AccountObservationSubmitV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, value);
    }
    #[test]
    fn missing_values_stay_unavailable_and_numbers_are_never_accepted_for_money_or_time() {
        let mut value = fixture();
        value.snapshot.as_mut().unwrap().total_equity.clear();
        assert_eq!(
            valuation(value.snapshot.as_ref()),
            AccountValuationV1::Unavailable
        );
        let mut wire = serde_json::to_value(fixture()).unwrap();
        wire["snapshot"]["ts_event"] = serde_json::json!(1700000000000000001_u64);
        assert!(serde_json::from_value::<AccountObservationSubmitV1>(wire).is_err());
        let mut wire = serde_json::to_value(fixture()).unwrap();
        wire["snapshot"]["total_equity"][0]["amount"] = serde_json::json!(1000.25);
        assert!(serde_json::from_value::<AccountObservationSubmitV1>(wire).is_err());
    }
}
