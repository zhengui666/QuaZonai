//! Correlate observable native account and time identities with frozen inputs.
//! Canonical encoding, pricing, fills and daily-return estimation stay in Nautilus.
use super::{bad, instruments};
use crate::{control::text, DomainError};
use bigdecimal::BigDecimal;
use contracts::{
    evidence::{MetricStatus, MetricValueV1},
    science::*,
    DbCounter, Id, SchemaV1,
};
use serde_json::Value;
use std::collections::BTreeSet;

/// Preserve native daily-account statistics, not canonical position fallbacks.
/// This maps evidence only; publication and qualification remain Store duties.
pub fn metrics(
    evaluation: Id,
    artifact: Id,
    request: &NativeSimulationRequestV1,
    result: &NativeSimulationResultV1,
) -> Result<(Vec<MetricValueV1>, Vec<crate::evidence::MetricCapability>), DomainError> {
    binding(request, result)?;
    let start = native_count(&result.canonical_result["run"]["backtest_start_ns"])? / 1000;
    let end = native_count(&result.canonical_result["run"]["backtest_end_ns"])?.div_ceil(1000);
    let period_start = chrono::DateTime::from_timestamp_micros(start as i64)
        .ok_or_else(|| bad("native_output.simulation_period"))?;
    let period_end = chrono::DateTime::from_timestamp_micros(end as i64)
        .ok_or_else(|| bad("native_output.simulation_period"))?;
    let mut records = Vec::with_capacity(3);
    let mut capabilities = Vec::with_capacity(3);
    for (code, key, method, unit, annualization, higher) in [
        (
            "PORTFOLIO_DAILY_RETURN_MEAN",
            "Average (Return)",
            "nautilus-analysis.ReturnsAverage",
            "RETURN_PER_DAY",
            None,
            true,
        ),
        (
            "PORTFOLIO_RETURN_VOLATILITY",
            "Returns Volatility (252 days)",
            "nautilus-analysis.ReturnsVolatility",
            "ANNUALIZED_RETURN_STDDEV",
            Some(252.0),
            false,
        ),
        (
            "PORTFOLIO_SHARPE_RATIO",
            "Sharpe Ratio (252 days)",
            "nautilus-analysis.SharpeRatio",
            "RATIO",
            Some(252.0),
            true,
        ),
    ] {
        let native = result
            .statistics
            .iter()
            .find(|stat| stat.group == NativeStatisticGroup::Returns && stat.native_key == key)
            .ok_or_else(|| bad("native_output.portfolio_statistic_missing"))?;
        let (value, status, reason_code) = if result.returns_status != MetricStatus::Ok {
            (None, result.returns_status, result.returns_reason.clone())
        } else if native.value.is_none() {
            (None, MetricStatus::Failed, native.reason_code.clone())
        } else {
            (native.value, MetricStatus::Ok, None)
        };
        let record = MetricValueV1 {
            schema_version: SchemaV1,
            evaluation_id: evaluation,
            metric_code: code.into(),
            scope: "portfolio".into(),
            value,
            status,
            reason_code,
            unit: unit.into(),
            period_start,
            period_end,
            observation_count: DbCounter::new(result.returns.len() as u64)
                .map_err(|_| bad("native_output.simulation_counts"))?,
            frequency: "UTC_DAY".into(),
            annualization_factor: annualization,
            method_id: method.into(),
            method_version: result.native_version.clone(),
            source_artifact_id: artifact,
            higher_is_better: Some(higher),
        };
        crate::evidence::validate_metric(&record)?;
        capabilities.push(crate::evidence::MetricCapability {
            metric_code: record.metric_code.clone(),
            method_id: record.method_id.clone(),
            method_version: record.method_version.clone(),
            unit: record.unit.clone(),
            frequency: record.frequency.clone(),
        });
        records.push(record);
    }
    Ok((records, capabilities))
}

fn native_count(value: &Value) -> Result<u64, DomainError> {
    let value = value
        .as_str()
        .ok_or_else(|| bad("native_output.canonical_counter"))?;
    let count = value
        .parse::<u64>()
        .map_err(|_| bad("native_output.canonical_counter"))?;
    if count.to_string() != value {
        return Err(bad("native_output.canonical_counter"));
    }
    Ok(count)
}
fn money(value: &Value, currency: &str) -> Result<BigDecimal, DomainError> {
    // Only associate the native Money serialization with the selected currency.
    // BigDecimal owns exact numerical parsing/comparison; no float conversion.
    let amount = value
        .as_str()
        .and_then(|value| value.strip_suffix(&format!(" {currency}")))
        .ok_or_else(|| bad("native_output.account_currency"))?;
    if amount.len() > 120 {
        return Err(bad("native_output.account_money"));
    }
    amount
        .parse()
        .map_err(|_| bad("native_output.account_money"))
}
fn nullable(
    value: Option<f64>,
    reason: &Option<String>,
    expected: &str,
) -> Result<(), DomainError> {
    if value.is_some_and(|n| !n.is_finite())
        || (value.is_some() && reason.is_some())
        || (value.is_none() && reason.as_deref() != Some(expected))
    {
        return Err(bad("native_output.statistic_missing_reason"));
    }
    Ok(())
}

pub(super) fn shape(value: &NativeSimulationResultV1) -> Result<(), DomainError> {
    if value.native_version != "0.63.0"
        || value.iterations.get() == 0
        || value.consumed_target_points.get() == 0
        || value.returns_kind != NativeReturnsKind::PortfolioDaily
        || value.returns.len() > 1_000_002
        || value.statistics.len() > 4096
        || value.summary.len() > 8192
        || value.summary.get("venues.total").map(String::as_str) != Some("1")
        || value.summary.get("orders.open").map(String::as_str) != Some("0")
        || value.summary.get("orders.inflight").map(String::as_str) != Some("0")
    {
        return Err(bad("native_output.simulation"));
    }
    let mut statistics = BTreeSet::new();
    for stat in &value.statistics {
        text(&stat.native_key, 1, 200, false)?;
        let group = match stat.group {
            NativeStatisticGroup::Pnl => 0,
            NativeStatisticGroup::Returns => 1,
            NativeStatisticGroup::General => 2,
        };
        if !statistics.insert((group, &stat.native_key, &stat.currency))
            || (stat.group == NativeStatisticGroup::Pnl) != stat.currency.is_some()
            || stat
                .currency
                .as_ref()
                .is_some_and(|code| iso_currency::Currency::from_code(code).is_none())
        {
            return Err(bad("native_output.simulation_statistic"));
        }
        nullable(
            stat.value,
            &stat.reason_code,
            "NATIVE_STATISTIC_UNAVAILABLE",
        )?;
    }
    if value
        .returns
        .windows(2)
        .any(|pair| pair[0].timestamp_ns >= pair[1].timestamp_ns)
    {
        return Err(bad("native_output.simulation_returns_order"));
    }
    for point in &value.returns {
        nullable(point.value, &point.reason_code, "NATIVE_RETURN_UNAVAILABLE")?;
    }
    let (state, reason) = if value.returns.is_empty() {
        (
            MetricStatus::InsufficientData,
            Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE"),
        )
    } else if value.returns.iter().any(|point| point.value.is_none()) {
        (MetricStatus::Failed, Some("NATIVE_RETURN_UNAVAILABLE"))
    } else {
        (MetricStatus::Ok, None)
    };
    if value.returns_status != state || value.returns_reason.as_deref() != reason {
        return Err(bad("native_output.simulation_returns_state"));
    }
    let canonical = &value.canonical_result;
    if canonical.get("schema").and_then(Value::as_str) != Some("nautilus-backtest-result/v1")
        || canonical.pointer("/run/outcome").and_then(Value::as_str) != Some("completed")
        || canonical.get("summary")
            != Some(
                &serde_json::to_value(&value.summary).map_err(|_| bad("native_output.summary"))?,
            )
        || canonical
            .get("accounts")
            .and_then(Value::as_array)
            .map(Vec::len)
            != Some(1)
        || canonical
            .get("portfolio_snapshots")
            .and_then(Value::as_array)
            .is_none_or(|rows| rows.is_empty() || rows.len() > 1_000_002)
    {
        return Err(bad("native_output.canonical_binding"));
    }
    for (name, expected) in [
        ("iterations", value.iterations),
        ("total_events", value.events),
        ("total_orders", value.orders),
        ("total_positions", value.positions),
    ] {
        if native_count(&canonical["run"][name])? != expected.get() {
            return Err(bad("native_output.canonical_counts"));
        }
    }
    Ok(())
}

pub(super) fn binding(
    request: &NativeSimulationRequestV1,
    value: &NativeSimulationResultV1,
) -> Result<(), DomainError> {
    shape(value)?;
    crate::portfolio::simulation_models(&request.settings)?;
    let ids = instruments(&request.selection)?;
    if request.target_points.is_empty()
        || request.target_points.len() > 10_000
        || value.consumed_target_points.get() != request.target_points.len() as u64
        || !request.settings.starting_capital.is_positive()
    {
        return Err(bad("native_output.simulation_parameters"));
    }
    let currency = &request.settings.base_currency;
    if iso_currency::Currency::from_code(currency).is_none() {
        return Err(bad("native_output.account_currency"));
    }
    for point in &request.target_points {
        if point
            .targets
            .iter()
            .map(|target| target.instrument_id.as_str())
            .collect::<Vec<_>>()
            != ids
            || point
                .targets
                .iter()
                .any(|target| target.currency != *currency)
        {
            return Err(bad("native_output.simulation_instruments"));
        }
    }
    let canonical = &value.canonical_result;
    let start = native_count(&canonical["run"]["backtest_start_ns"])?;
    let end = native_count(&canonical["run"]["backtest_end_ns"])?;
    if start < request.selection.event_start_ns.get()
        || start > end
        || end > request.selection.decision_cutoff_ns.get()
    {
        return Err(bad("native_output.simulation_interval"));
    }
    let (variant, account_type) = match request.settings.account_kind {
        NativeAccountKind::Cash => ("Cash", "CASH"),
        NativeAccountKind::Margin => ("Margin", "MARGIN"),
    };
    let account = &canonical["accounts"][0];
    if account.as_object().map(|o| o.len()) != Some(1) {
        return Err(bad("native_output.account"));
    }
    let base = account
        .get(variant)
        .and_then(|a| a.get("base"))
        .ok_or_else(|| bad("native_output.account"))?;
    if base["account_type"].as_str() != Some(account_type)
        || base["base_currency"].as_str() != Some(currency)
        || base["balances_starting"].as_object().map(|o| o.len()) != Some(1)
        || money(&base["balances_starting"][currency], currency)?
            != *request.settings.starting_capital.as_decimal()
    {
        return Err(bad("native_output.starting_account"));
    }
    let account_id = base["id"]
        .as_str()
        .ok_or_else(|| bad("native_output.account"))?;
    let snapshots = canonical["portfolio_snapshots"]
        .as_array()
        .ok_or_else(|| bad("native_output.snapshots"))?;
    for snapshot in snapshots {
        let time = native_count(&snapshot["ts_event"])?;
        if snapshot["account_id"].as_str() != Some(account_id)
            || snapshot["account_type"].as_str() != Some(account_type)
            || snapshot["base_currency"].as_str() != Some(currency)
            || time < start
            || time > end
        {
            return Err(bad("native_output.snapshot_identity"));
        }
        let equity = snapshot["total_equity"]
            .as_array()
            .ok_or_else(|| bad("native_output.snapshot_equity"))?;
        if equity.len() != 1 {
            return Err(bad("native_output.snapshot_equity"));
        }
        money(&equity[0], currency)?;
    }
    // Native returns are keyed by UTC day, including the partially observed first
    // day. Do not use the position-return fallback or invent intraday daily samples.
    const DAY: u64 = 86_400_000_000_000;
    let first_day = start / DAY * DAY;
    let last_day = end / DAY * DAY;
    if value.returns.iter().any(|point| {
        point.timestamp_ns.get() < first_day
            || point.timestamp_ns.get() > last_day
            || !point.timestamp_ns.get().is_multiple_of(DAY)
    }) {
        return Err(bad("native_output.simulation_return_window"));
    }
    Ok(())
}
