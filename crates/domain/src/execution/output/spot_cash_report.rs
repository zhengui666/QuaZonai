//! Verify the separate observed cash report without mutating canonical evidence.
use super::{bad, simulation};
use crate::{DomainError, spot_cash::SpotCashContext};
use contracts::{
    DecimalValue,
    evidence::MetricStatus,
    science::{NativeSimulationRequestV1, NativeSimulationResultV1, NativeStatisticGroup},
    spot_cash::*,
    spot_cash_report::*,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn equal(left: &impl Serialize, right: &impl Serialize) -> Result<bool, DomainError> {
    Ok(
        serde_json::to_value(left).map_err(|_| bad("spot_report.encoding"))?
            == serde_json::to_value(right).map_err(|_| bad("spot_report.encoding"))?,
    )
}
fn raw_count(value: &Value) -> Result<u64, DomainError> {
    value
        .as_u64()
        .map(Ok)
        .unwrap_or_else(|| simulation::native_count(value))
}
fn decimal(value: &Value) -> Result<DecimalValue, DomainError> {
    value
        .as_str()
        .ok_or_else(|| bad("spot_report.decimal"))?
        .parse()
        .map_err(|_| bad("spot_report.decimal"))
}

pub(super) fn money(value: &Value, currency: &str) -> Result<DecimalValue, DomainError> {
    value
        .as_str()
        .and_then(|value| value.strip_suffix(&format!(" {currency}")))
        .ok_or_else(|| bad("spot_report.money_currency"))?
        .parse()
        .map_err(|_| bad("spot_report.money_range"))
}

pub(super) fn returns_state(report: &NativeSpotCashReportV1) -> (MetricStatus, Option<String>) {
    if report.daily_returns.days.is_empty() {
        (
            MetricStatus::InsufficientData,
            Some("REPORT_CURRENCY_DAILY_RETURNS_UNAVAILABLE".into()),
        )
    } else if report
        .daily_returns
        .days
        .iter()
        .any(|day| day.value.is_none())
    {
        (
            MetricStatus::Failed,
            Some("REPORT_CURRENCY_DAILY_RETURN_GAP".into()),
        )
    } else {
        (MetricStatus::Ok, None)
    }
}

pub(super) fn shape(report: &NativeSpotCashReportV1) -> Result<(), DomainError> {
    if report.observations.is_empty()
        || report.instruments.is_empty()
        || report.statistics.len() > 4096
    {
        return Err(bad("spot_report.bounds"));
    }
    let mut keys = BTreeSet::new();
    for statistic in &report.statistics {
        crate::control::text(&statistic.native_key, 1, 200, false)?;
        if statistic.group != NativeStatisticGroup::Returns
            || statistic.currency.is_some()
            || !keys.insert(&statistic.native_key)
            || statistic.value.is_some_and(|x| !x.is_finite())
            || (statistic.value.is_some() && statistic.reason_code.is_some())
            || (statistic.value.is_none()
                && statistic.reason_code.as_deref() != Some("NATIVE_STATISTIC_UNAVAILABLE"))
        {
            return Err(bad("spot_report.statistics"));
        }
    }
    // A gap cannot disappear by passing just the usable days to a statistic.
    if returns_state(report).0 != MetricStatus::Ok
        && report.statistics.iter().any(|s| s.value.is_some())
    {
        return Err(bad("spot_report.statistics_across_gap"));
    }
    Ok(())
}

fn snapshot_binding(
    observation: &NativeSpotCashObservationV1,
    native: &Value,
    frame: &NativeSpotValuationFrameV1,
    origin: NativeSpotSnapshotOriginV1,
) -> Result<(), DomainError> {
    let b = &frame.snapshot.binding;
    if native.get("base_currency") != Some(&Value::Null)
        || native["account_type"].as_str() != Some("CASH")
        || native["account_id"].as_str() != Some(b.account_id.as_str())
        || native["event_id"].as_str() != Some(b.event_id.to_string().as_str())
        || raw_count(&native["ts_event"])? != b.asof_ns.get()
        || raw_count(&native["ts_init"])? != b.asof_ns.get()
        || b.asof_ns != observation.native_clock_ns
        || b.snapshot_sequence != observation.sequence
        || b.origin != origin
        || frame.snapshot.native_base_currency.is_some()
        || frame.snapshot.account_kind != contracts::science::NativeAccountKind::Cash
        || !frame.snapshot.balances_complete
        || !native["total_equity"].is_array()
    {
        return Err(bad("spot_report.native_snapshot_binding"));
    }
    let balances = native["balances"]
        .as_array()
        .ok_or_else(|| bad("spot_report.native_balances"))?;
    if balances.len() != frame.snapshot.balances.len() || balances.is_empty() {
        return Err(bad("spot_report.native_balances"));
    }
    let mut by_currency = BTreeMap::new();
    for balance in balances {
        let currency = balance["currency"]
            .as_str()
            .ok_or_else(|| bad("spot_report.native_balance_currency"))?;
        if by_currency.insert(currency, balance).is_some() {
            return Err(bad("spot_report.native_balance_duplicate"));
        }
    }
    for balance in &frame.snapshot.balances {
        let row = by_currency
            .remove(balance.currency.as_str())
            .ok_or_else(|| bad("spot_report.native_balance_missing"))?;
        for (name, expected) in [
            ("total", &balance.total),
            ("free", &balance.free),
            ("locked", &balance.locked),
        ] {
            if money(&row[name], &balance.currency)? != *expected {
                return Err(bad("spot_report.native_balance_value"));
            }
        }
    }
    if !by_currency.is_empty() {
        return Err(bad("spot_report.native_balance_missing"));
    }
    Ok(())
}

// A snapshot has only unordered per-currency/per-instrument arrays. Compare its
// complete contents semantically after removing the canonical event alias and
// accounting for canonical decimal-string counters. No result document is edited.
fn comparable_snapshot(value: &Value) -> Result<Vec<u8>, DomainError> {
    fn normalize(value: &mut Value) {
        match value {
            Value::Number(number) => *value = Value::String(number.to_string()),
            Value::Array(values) => {
                for item in values.iter_mut() {
                    normalize(item);
                }
                values.sort_by_cached_key(|item| item.to_string());
            }
            Value::Object(object) => {
                for value in object.values_mut() {
                    normalize(value);
                }
            }
            _ => {}
        }
    }
    let mut value = value.clone();
    // serde_json/preserve_order can be enabled by the native job's feature graph.
    // Object insertion order is not part of the original snapshot's meaning.
    value.sort_all_objects();
    let object = value
        .as_object_mut()
        .ok_or_else(|| bad("spot_report.snapshot_document"))?;
    if object.remove("event_id").is_none() {
        return Err(bad("spot_report.snapshot_event_missing"));
    }
    normalize(&mut value);
    serde_json::to_vec(&value).map_err(|_| bad("spot_report.snapshot_document"))
}

pub(super) fn binding(
    request: &NativeSimulationRequestV1,
    result: &NativeSimulationResultV1,
    report: &NativeSpotCashReportV1,
) -> Result<(), DomainError> {
    shape(report)?;
    let session = &report.session;
    match &request.settings.fee_model {
        contracts::portfolio::NativeModelRefV1::FrozenSpotFeeScenario {
            native_version,
            parameters,
            ..
        } if native_version == "0.63.0"
            && parameters.acceptance == report.fee_acceptance
            && parameters.schedule == report.fee_schedule => {}
        _ => return Err(bad("spot_report.frozen_fee_model")),
    }
    if report.fee_schedule.source.status
        != contracts::spot_fees::SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability
        || report.fee_schedule.valid_from_ns > session.period_start_ns
        || report.fee_schedule.valid_until_ns <= session.period_end_ns
    {
        return Err(bad("spot_report.fee_scenario"));
    }
    let context = SpotCashContext::new(&request.settings, session.clone(), &report.instruments)?;
    if session.venue != "HYPERLIQUID" {
        return Err(bad("spot_report.venue"));
    }
    let selected_ids = super::instruments(&request.selection)?;
    let selected: BTreeSet<_> = selected_ids.iter().copied().collect();
    let bar_instruments: BTreeMap<_, _> = request
        .selection
        .bar_types
        .iter()
        .map(String::as_str)
        .zip(selected_ids)
        .collect();
    let actual: BTreeSet<_> = report
        .instruments
        .iter()
        .map(|i| i.instrument_id.as_str())
        .collect();
    if actual != selected {
        return Err(bad("spot_report.selected_instruments"));
    }
    let receipt = &report.native_run_receipt;
    let run_id: NativeSpotRuntimeIdV1 = receipt["run_id"]
        .as_str()
        .ok_or_else(|| bad("spot_report.run_receipt"))?
        .parse()
        .map_err(|_| bad("spot_report.run_receipt"))?;
    if receipt["instance_id"].as_str() != Some(session.native_instance_id.to_string().as_str())
        || run_id != report.flow_evidence.native_run_id
        || raw_count(&receipt["backtest_start"])? != session.period_start_ns.get()
        || raw_count(&receipt["backtest_end"])? != session.period_end_ns.get()
        || raw_count(&receipt["run_started"])? > raw_count(&receipt["run_finished"])?
        || simulation::native_count(&result.canonical_result["run"]["backtest_start_ns"])?
            != session.period_start_ns.get()
        || simulation::native_count(&result.canonical_result["run"]["backtest_end_ns"])?
            != session.period_end_ns.get()
        || receipt.get("summary")
            != Some(
                &serde_json::to_value(&result.summary)
                    .map_err(|_| bad("spot_report.receipt_summary"))?,
            )
    {
        return Err(bad("spot_report.run_receipt_binding"));
    }
    for (name, expected) in [
        ("iterations", result.iterations),
        ("total_events", result.events),
        ("total_orders", result.orders),
        ("total_positions", result.positions),
    ] {
        if raw_count(&receipt[name])? != expected.get() {
            return Err(bad("spot_report.receipt_counts"));
        }
    }
    let mut frames = Vec::new();
    let mut published = Vec::new();
    let mut latest = BTreeMap::<String, NativeSpotPriceV1>::new();
    let mut row_keys = BTreeSet::new();
    let mut snapshot_ids = BTreeSet::new();
    let mut previous_clock = session.period_start_ns;
    for (index, observation) in report.observations.iter().enumerate() {
        if observation.sequence.get() != index as u64 + 1
            || observation.native_clock_ns < previous_clock
            || observation.native_clock_ns < session.period_start_ns
            || observation.native_clock_ns > session.period_end_ns
        {
            return Err(bad("spot_report.observer_sequence_or_clock"));
        }
        previous_clock = observation.native_clock_ns;
        match &observation.record {
            NativeSpotCashObservationKindV1::Bar {
                event_time,
                native,
                source_row_key,
                bar_open_ns,
                bar_close_ns,
            } => {
                crate::control::text(source_row_key, 1, 200, false)?;
                let bar_type = native["bar_type"]
                    .as_str()
                    .ok_or_else(|| bad("spot_report.bar_type"))?;
                let instrument_id = bar_instruments
                    .get(bar_type)
                    .ok_or_else(|| bad("spot_report.bar_selection"))?
                    .to_string();
                if bar_open_ns >= bar_close_ns
                    || raw_count(&native["ts_event"])?
                        != match event_time {
                            NativeSpotBarEventTimeV1::Open => bar_open_ns.get(),
                            NativeSpotBarEventTimeV1::CloseExclusive => bar_close_ns.get(),
                        }
                    || raw_count(&native["ts_init"])? != observation.native_clock_ns.get()
                    || *bar_close_ns > observation.native_clock_ns
                    || !row_keys.insert(source_row_key)
                {
                    return Err(bad("spot_report.bar_source"));
                }
                latest.insert(
                    instrument_id.clone(),
                    NativeSpotPriceV1 {
                        session_id: session.session_id,
                        native_instance_id: session.native_instance_id,
                        dataset_revision_id: session.dataset_revision_id,
                        source_row_key: source_row_key.clone(),
                        instrument_id,
                        method: NativeSpotPriceMethodV1::ClosedBarClose,
                        observed_sequence: observation.sequence,
                        bar_open_ns: *bar_open_ns,
                        event_ns: *bar_close_ns,
                        available_ns: observation.native_clock_ns,
                        price: decimal(&native["close"])?,
                    },
                );
            }
            NativeSpotCashObservationKindV1::Snapshot {
                origin,
                native,
                frame,
                valuation,
            } => {
                snapshot_binding(observation, native, frame, *origin)?;
                if !snapshot_ids.insert(frame.snapshot.binding.event_id)
                    || !equal(&frame.prices, &latest.values().cloned().collect::<Vec<_>>())?
                    || !equal(valuation, &context.value(frame))?
                {
                    return Err(bad("spot_report.valuation_binding"));
                }
                if *origin == NativeSpotSnapshotOriginV1::NativePublication {
                    published.push(native);
                }
                frames.push(frame.clone());
            }
        }
    }
    if let Some(source) = &report.source_evidence {
        let mut clocks = Vec::new();
        let mut responses = BTreeSet::new();
        let prefix = format!("{}:", source.capture_id);
        for observation in &report.observations {
            if let NativeSpotCashObservationKindV1::Bar {
                event_time,
                source_row_key,
                ..
            } = &observation.record
            {
                if *event_time != NativeSpotBarEventTimeV1::Open {
                    return Err(bad("spot_report.source_clock_kind"));
                }
                let key = source_row_key
                    .strip_prefix(&prefix)
                    .ok_or_else(|| bad("spot_report.source_capture"))?;
                let (response, row) = key
                    .split_once(':')
                    .ok_or_else(|| bad("spot_report.source_locator"))?;
                let response = response
                    .parse::<u64>()
                    .map_err(|_| bad("spot_report.source_locator"))?;
                let row_index = row
                    .parse::<u64>()
                    .map_err(|_| bad("spot_report.source_locator"))?;
                if response == 0 || format!("{response}:{row_index}") != key {
                    return Err(bad("spot_report.source_locator"));
                }
                responses.insert(response);
                clocks.push(observation.native_clock_ns);
            }
        }
        if source.native_version != "0.63.0"
            || clocks.len() as u64 != source.selected_row_count.get()
            || responses.len() as u64 != source.response_count.get()
            || clocks.iter().min().copied() != Some(source.first_received_ns)
            || clocks.iter().max().copied() != Some(source.last_received_ns)
        {
            return Err(bad("spot_report.source_observation_binding"));
        }
        // Historical availability is an explicit one-variant UNVERIFIED enum;
        // complete valuations/daily returns never replace that source status.
    }
    let opening = frames
        .first()
        .ok_or_else(|| bad("spot_report.opening_snapshot"))?;
    if opening.snapshot.binding.asof_ns != session.period_start_ns
        || opening.snapshot.binding.origin != NativeSpotSnapshotOriginV1::NativePublication
        || opening.snapshot.balances.len() != 1
        || opening.snapshot.balances[0].currency != request.settings.base_currency
        || opening.snapshot.balances[0].total != request.settings.starting_capital
        || opening.snapshot.balances[0].free != request.settings.starting_capital
        || opening.snapshot.balances[0].locked != DecimalValue::zero()
    {
        return Err(bad("spot_report.fresh_opening_capital"));
    }
    let expected_returns = context.daily_returns(&frames, &report.flow_evidence)?;
    if !equal(&report.daily_returns, &expected_returns)? {
        return Err(bad("spot_report.daily_returns_binding"));
    }
    let snapshots = result.canonical_result["portfolio_snapshots"]
        .as_array()
        .ok_or_else(|| bad("spot_report.canonical_snapshots"))?;
    if snapshots.is_empty() || snapshots.len() > published.len() {
        return Err(bad("spot_report.canonical_snapshot_count"));
    }
    // The official native ring may retain only a tail. Extra decision snapshots
    // never enter that ring; require exact complete tail contents, including zeros.
    let mut originals = published[published.len() - snapshots.len()..]
        .iter()
        .map(|snapshot| comparable_snapshot(snapshot))
        .collect::<Result<Vec<_>, _>>()?;
    let mut canonical = snapshots
        .iter()
        .map(comparable_snapshot)
        .collect::<Result<Vec<_>, _>>()?;
    originals.sort();
    canonical.sort();
    if originals != canonical {
        return Err(bad("spot_report.canonical_snapshot_contents"));
    }
    Ok(())
}

pub(crate) fn dataset(
    result: &NativeSimulationResultV1,
    dataset: contracts::Id,
) -> Result<(), DomainError> {
    if result
        .spot_cash_report
        .as_ref()
        .is_some_and(|r| r.session.dataset_revision_id != dataset)
    {
        return Err(bad("spot_report.frozen_dataset"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
