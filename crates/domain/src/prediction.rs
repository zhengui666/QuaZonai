//! QZ source and capital constraints around the upstream binary-option fee model.
//! This module does not calculate fills, settlement or transaction commissions.
use crate::DomainError;
use bigdecimal::BigDecimal;
use contracts::{portfolio::NativeModelRefV1, DecimalValue};
use serde_json::Value;
use std::str::FromStr;

/// Research execution floor in units of original collateral, in either trade direction.
/// It is a frozen adapter constraint, not a claim about the venue's minimum order rules.
pub const MINIMUM_TRADE_NOTIONAL: &str = "1";

fn invalid() -> DomainError {
    DomainError::Invalid("polymarket_native_contract")
}

fn decimal(value: &Value) -> Result<BigDecimal, DomainError> {
    match value {
        Value::String(s) => BigDecimal::from_str(s).map_err(|_| invalid()),
        Value::Number(n) => BigDecimal::from_str(&n.to_string()).map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}

pub fn uses_native_fee(model: &NativeModelRefV1) -> bool {
    matches!(model, NativeModelRefV1::NautilusPolymarket { .. })
}

/// UTC daily portfolio returns: continuous prediction markets use a calendar year.
/// Existing non-prediction studies retain their frozen 252-day convention.
pub fn portfolio_annualization_days(model: &NativeModelRefV1) -> usize {
    if uses_native_fee(model) {
        365
    } else {
        252
    }
}

/// Inspect original Rust BinaryOption payloads. These checks do not grant PIT status.
pub fn instrument(value: &Value) -> Result<(u64, u64), DomainError> {
    let info = value
        .get("info")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let condition = info
        .get("condition_id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let token = info
        .get("token_id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if condition.is_empty()
        || token.is_empty()
        || !token.bytes().all(|c| c.is_ascii_digit())
        || value.get("id").and_then(Value::as_str)
            != Some(format!("{condition}-{token}.POLYMARKET").as_str())
        || value.get("raw_symbol").and_then(Value::as_str) != Some(token)
        || !value
            .get("currency")
            .and_then(Value::as_str)
            .is_some_and(|v| contracts::research_currency::NATIVE_COLLATERAL.contains(&v))
    {
        return Err(invalid());
    }
    let activation = value
        .get("activation_ns")
        .and_then(Value::as_u64)
        .ok_or_else(invalid)?;
    let expiration = value
        .get("expiration_ns")
        .and_then(Value::as_u64)
        .ok_or_else(invalid)?;
    if activation >= expiration || expiration > i64::MAX as u64 {
        return Err(invalid());
    }
    Ok((activation, expiration))
}

/// Conservative optimizer coefficient sourced from the original native fee schedule.
/// Actual transaction commissions are still calculated exclusively by PolymarketFeeModel.
/// With notional >= 1 and the pinned model's five-decimal rounding, rate + 0.000005
/// bounds taker fee / notional. Confirmed zero rates remain exactly zero; missing is an error.
pub fn planning_fee(value: &Value) -> Result<DecimalValue, DomainError> {
    instrument(value)?;
    let schedule = value
        .get("info")
        .and_then(|v| v.get("fee_schedule"))
        .and_then(Value::as_object)
        .ok_or(DomainError::CapabilityUnavailable(
            "polymarket_fee_schedule_missing",
        ))?;
    let rate = decimal(schedule.get("rate").ok_or_else(invalid)?)?;
    let exponent = decimal(schedule.get("exponent").ok_or_else(invalid)?)?;
    let rebate = decimal(schedule.get("rebateRate").ok_or_else(invalid)?)?;
    if exponent != BigDecimal::from(1)
        || rate < BigDecimal::from(0)
        || !(BigDecimal::from(0)..=BigDecimal::from(1)).contains(&rebate)
        || schedule.get("takerOnly").and_then(Value::as_bool) != Some(true)
    {
        return Err(DomainError::CapabilityUnavailable(
            "polymarket_fee_schedule",
        ));
    }
    let ceiling = if rate == BigDecimal::from(0) {
        rate
    } else {
        rate + BigDecimal::new(5.into(), 6)
    };
    if ceiling >= BigDecimal::from(1) {
        return Err(DomainError::CapabilityUnavailable(
            "polymarket_planning_fee_range",
        ));
    }
    ceiling.to_plain_string().parse().map_err(|_| invalid())
}

/// Validate source coherence only. Nautilus still performs every cash movement.
pub fn settlements(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
) -> Result<(), DomainError> {
    use std::collections::BTreeSet;
    let bad = || DomainError::Invalid("polymarket_settlement_source");
    if groups.len() > 256 {
        return Err(bad());
    }
    let mut conditions = BTreeSet::new();
    let mut instruments = BTreeSet::new();
    for group in groups {
        crate::control::text(&group.condition_id, 1, 120, false)?;
        crate::control::text(&group.source_reference, 1, 2000, false)?;
        if !conditions.insert(&group.condition_id) || group.outcomes.len() != 2 {
            return Err(bad());
        }
        let prefix = format!("{}-", group.condition_id);
        let mut total = BigDecimal::from(0);
        for outcome in &group.outcomes {
            crate::control::text(&outcome.instrument_id, 1, 200, false)?;
            let token = outcome
                .instrument_id
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".POLYMARKET"))
                .ok_or_else(bad)?;
            if token.is_empty()
                || !token.bytes().all(|b| b.is_ascii_digit())
                || !instruments.insert(&outcome.instrument_id)
                || !outcome.close_price.is_fraction()
                || outcome.ts_event > outcome.ts_init
            {
                return Err(bad());
            }
            total += outcome.close_price.as_decimal();
        }
        if total != BigDecimal::from(1) {
            return Err(DomainError::Invalid("polymarket_incoherent_payout_vector"));
        }
    }
    Ok(())
}

/// Select whole conditions, never remove a sibling merely because it is not traded.
pub fn scoped_settlements(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
    instruments: &[String],
) -> Vec<contracts::settlement::NativeSettlementGroupV1> {
    groups
        .iter()
        .filter(|g| {
            g.outcomes
                .iter()
                .any(|o| instruments.contains(&o.instrument_id))
        })
        .cloned()
        .collect()
}

pub fn visible_settlements(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
    instruments: &[String],
    cutoff: contracts::DbCounter,
) -> Vec<contracts::settlement::NativeSettlementGroupV1> {
    scoped_settlements(groups, instruments)
        .into_iter()
        .filter(|g| g.outcomes.iter().all(|o| o.ts_init <= cutoff))
        .collect()
}

/// Bind complete payouts to the original quality window and selected native identities.
pub fn settlement_scope(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
    instruments: &[String],
    selection: &contracts::science::NativeBarSelectionV1,
) -> Result<(), DomainError> {
    settlements(groups)?;
    if groups != scoped_settlements(groups, instruments)
        || groups
            .iter()
            .flat_map(|g| &g.outcomes)
            .any(|o| o.ts_init > selection.decision_cutoff_ns)
    {
        return Err(DomainError::Invalid("polymarket_settlement_scope"));
    }
    Ok(())
}

/// Fixed TTLs are rejected, never silently clipped to conceal an expired target.
pub fn target_window(
    definitions: &[Value],
    ids: &[String],
    asof_ns: u64,
    until_ns: u64,
) -> Result<(), DomainError> {
    if asof_ns >= until_ns {
        return Err(DomainError::Invalid("polymarket_target_lifetime"));
    }
    for id in ids {
        let mut found = false;
        for definition in definitions {
            let (class, payload) = crate::catalogs::instrument_definition(definition)?;
            if payload.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                continue;
            }
            if found {
                return Err(DomainError::Invalid("polymarket_target_identity"));
            }
            found = true;
            if class == "BinaryOption" {
                let (activation, expiration) = instrument(payload)?;
                if asof_ns < activation || asof_ns >= expiration || until_ns > expiration {
                    return Err(DomainError::Invalid("polymarket_target_lifetime"));
                }
            }
        }
        if !found {
            return Err(DomainError::Invalid("polymarket_target_identity"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload() -> Value {
        serde_json::json!({"id":"condition-123.POLYMARKET", "raw_symbol":"123",
            "currency":"pUSD", "activation_ns":1, "expiration_ns":100,
            "info":{"condition_id":"condition", "token_id":"123",
            "fee_schedule":{"rate":0.05,"exponent":1,"rebateRate":0.2,"takerOnly":true}}})
    }
    #[test]
    fn native_schedule_and_research_floor_preserve_units_and_missingness() {
        let value = payload();
        assert_eq!(planning_fee(&value).unwrap(), "0.050005".parse().unwrap());
        let mut free = value.clone();
        free["info"]["fee_schedule"]["rate"] = 0.into();
        assert_eq!(planning_fee(&free).unwrap(), "0".parse().unwrap());
        for currency in contracts::research_currency::NATIVE_COLLATERAL {
            free["currency"] = currency.into();
            assert!(planning_fee(&free).is_ok());
        }
        for mutation in 0..7 {
            let mut wrong = value.clone();
            match mutation {
                0 => wrong["currency"] = "USD".into(),
                1 => wrong["info"]["fee_schedule"] = Value::Null,
                2 => wrong["info"]["fee_schedule"]["exponent"] = 2.into(),
                3 => wrong["info"]["fee_schedule"]["takerOnly"] = false.into(),
                4 => wrong["expiration_ns"] = 0.into(),
                5 => wrong["info"]["token_id"] = "different".into(),
                _ => wrong["info"]["fee_schedule"]["rate"] = (-1).into(),
            }
            assert!(planning_fee(&wrong).is_err(), "mutation {mutation}");
        }
    }
}
