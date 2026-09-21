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
        .ok_or_else(|| DomainError::CapabilityUnavailable("polymarket_fee_schedule_missing"))?;
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
