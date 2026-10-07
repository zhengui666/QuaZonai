//! Owned fresh-run support. Account totals remain authoritative; fill arithmetic
//! is a read-only reconciliation using official CashAccount/Money public APIs.
use crate::{
    catalog::NativeMarketData,
    spot_cash_capture::{
        ClosedBarSourceRow, ObservedSpotCashRecord, SpotCashDecisionSnapshot,
        SpotCashObservationTape,
    },
};
use anyhow::{Result, anyhow, ensure};
use contracts::{
    DbCounter, Id, SchemaV1,
    portfolio::NativeModelRefV1,
    science::{NativeSimulationRequestV1, NativeStatisticGroup, NativeStatisticV1},
    spot_cash::*,
    spot_cash_report::*,
    spot_fees::*,
};
use domain::spot_cash::SpotCashContext;
use nautilus_analysis::{
    analyzer::{PortfolioAnalyzer, Statistic},
    statistics::{
        max_drawdown::MaxDrawdown, returns_volatility::ReturnsVolatility,
        sharpe_ratio::SharpeRatio, sortino_ratio::SortinoRatio,
    },
};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_execution::models::fee::FeeModelHandle;
use nautilus_model::{
    accounts::{Account, AccountAny},
    data::Bar,
    events::{OrderEventAny, OrderFilled},
    instruments::{Instrument, InstrumentAny},
    orders::Order,
    types::{Currency, Money},
};
use rust_decimal::Decimal;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) struct SpotReplayInputs {
    pub dataset_revision_id: Id,
    pub closed_rows: Vec<ClosedBarSourceRow>,
}
fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

pub(crate) fn execution_horizon(request: &NativeSimulationRequestV1) -> Result<DbCounter> {
    request
        .target_points
        .iter()
        .map(|point| point.valid_until_ns)
        .chain(std::iter::once(request.selection.decision_cutoff_ns))
        .max()
        .ok_or_else(|| anyhow!("SPOT_EXECUTION_HORIZON_MISSING"))
}
fn scenario(request: &NativeSimulationRequestV1) -> Result<&FrozenSpotFeeScenarioParametersV1> {
    let NativeModelRefV1::FrozenSpotFeeScenario {
        native_version,
        parameters,
        ..
    } = &request.settings.fee_model
    else {
        return Err(anyhow!("SPOT_EXPLICIT_FEE_SCENARIO_ACCEPTANCE_REQUIRED"));
    };
    ensure!(
        native_version == "0.63.0"
            && matches!(
                parameters.acceptance,
                SpotCashFeeAcceptanceV1::PublicRateScenarioUnverifiedApplicability
            )
            && parameters.schedule.source.status
                == SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability,
        "SPOT_PUBLIC_SCENARIO_POLICY_REQUIRED"
    );
    Ok(parameters)
}
pub(crate) fn fee_model(
    request: &NativeSimulationRequestV1,
    market: &NativeMarketData,
) -> Result<FeeModelHandle> {
    domain::spot_cash::execution_settings(&request.settings)?;
    let parameters = scenario(request)?;
    let start = market
        .series
        .iter()
        .flat_map(|series| &series.bars)
        .map(|bar| bar.ts_init.as_u64())
        .min()
        .ok_or_else(|| anyhow!("SPOT_DATA_EMPTY"))?;
    let instruments: Vec<_> = market
        .series
        .iter()
        .map(|series| series.instrument.clone())
        .collect();
    Ok(
        crate::spot_fees::FrozenSpotFeeModel::try_new_public_rate_scenario(
            parameters.schedule.clone(),
            &instruments,
            start,
            execution_horizon(request)?.get(),
        )?
        .into_handle(),
    )
}

/// Execution needs a reliable mark even when the account owns none of the base
/// asset. Do not change cash-only valuation to manufacture a price requirement.
/// `snapshot.asof_ns` is the actual native decision clock, not an event backdate.
pub(crate) fn execution_price(
    snapshot: &SpotCashDecisionSnapshot,
    policy: &NativeSpotCashPolicyV1,
    bar: &Bar,
) -> Result<Decimal> {
    let now = snapshot.asof_ns.get();
    ensure!(
        bar.ts_event <= bar.ts_init && bar.ts_init.as_u64() <= now,
        "SPOT_EXECUTION_PRICE_CLOCK_INVALID"
    );
    let observed = snapshot
        .prices
        .iter()
        .find(|price| price.instrument_id == bar.bar_type.instrument_id().to_string())
        .ok_or_else(|| anyhow!("SPOT_EXECUTION_PRICE_SOURCE_MISSING"))?;
    ensure!(
        observed.available_ns.get() == bar.ts_init.as_u64()
            && observed.event_ns <= observed.available_ns
            && (bar.ts_event.as_u64() == observed.bar_open_ns.get()
                || bar.ts_event.as_u64() == observed.event_ns.get())
            && observed
                .price
                .as_decimal()
                .to_plain_string()
                .parse::<Decimal>()?
                == bar.close.as_decimal(),
        "SPOT_EXECUTION_PRICE_SOURCE_MISMATCH"
    );
    ensure!(
        now - observed.event_ns.get() <= policy.maximum_price_age_ns.get(),
        "SPOT_EXECUTION_PRICE_STALE"
    );
    let price = bar.close.as_decimal();
    ensure!(price > Decimal::ZERO, "SPOT_EXECUTION_PRICE_NONPOSITIVE");
    Ok(price)
}

pub(crate) fn native_balance(
    snapshot: &SpotCashDecisionSnapshot,
    currency: Currency,
    free: bool,
) -> Result<Decimal> {
    let balance = snapshot
        .balances
        .iter()
        .find(|balance| balance.currency == currency.to_string());
    match balance {
        Some(balance) => Decimal::from_str_exact(
            &(if free { &balance.free } else { &balance.total })
                .as_decimal()
                .to_plain_string(),
        )
        .map_err(Into::into),
        // A fresh account has no row for an unowned, already validated base asset.
        None => Ok(Decimal::ZERO),
    }
}
pub(crate) fn inventory_quantity(
    snapshot: Option<&SpotCashDecisionSnapshot>,
    instrument: &InstrumentAny,
    legacy: impl FnOnce() -> Decimal,
) -> Result<Decimal> {
    match snapshot {
        Some(snapshot) => native_balance(
            snapshot,
            instrument
                .base_currency()
                .ok_or_else(|| anyhow!("SPOT_BASE_CURRENCY_MISSING"))?,
            false,
        ),
        None => Ok(legacy()),
    }
}

/// Detects dropped/rolled-back cash postings. It never posts money or replaces
/// account balances. Official calculate_pnls supplies the native trade deltas.
fn reconcile(
    account: &AccountAny,
    fills: &[OrderFilled],
    instruments: &[InstrumentAny],
    start: u64,
    end: u64,
) -> Result<()> {
    let AccountAny::Cash(cash) = account else {
        return Err(anyhow!("SPOT_NATIVE_ACCOUNT_NOT_CASH"));
    };
    ensure!(
        !cash.allow_borrowing && account.base_currency().is_none(),
        "SPOT_NATIVE_ACCOUNT_CONFIGURATION"
    );
    let mut expected: BTreeMap<String, Money> = account
        .starting_balances()
        .into_iter()
        .map(|(currency, money)| (currency.to_string(), money))
        .collect();
    let mut known_currencies: BTreeSet<String> = expected.keys().cloned().collect();
    for instrument in instruments {
        let InstrumentAny::CurrencyPair(pair) = instrument else {
            return Err(anyhow!("SPOT_NATIVE_FILL_NOT_SPOT"));
        };
        known_currencies.insert(pair.base_currency.to_string());
        known_currencies.insert(pair.quote_currency.to_string());
    }
    let mut fees = BTreeMap::<String, Money>::new();
    let definitions: BTreeMap<_, _> = instruments
        .iter()
        .map(|instrument| (instrument.id(), instrument))
        .collect();
    let mut events = BTreeSet::new();
    let mut trades = BTreeSet::new();
    for fill in fills {
        ensure!(
            fill.account_id == account.id()
                && start <= fill.ts_event.as_u64()
                && fill.ts_event.as_u64() <= end
                && fill.ts_event <= fill.ts_init
                && fill.last_qty.as_decimal() > Decimal::ZERO
                && fill.last_px.as_decimal() > Decimal::ZERO,
            "SPOT_NATIVE_FILL_IDENTITY_OR_TIME"
        );
        ensure!(
            events.insert(fill.event_id.to_string())
                && trades.insert((fill.instrument_id.to_string(), fill.trade_id.to_string())),
            "SPOT_NATIVE_FILL_REPLAY"
        );
        let instrument = *definitions
            .get(&fill.instrument_id)
            .ok_or_else(|| anyhow!("SPOT_NATIVE_FILL_INSTRUMENT"))?;
        let InstrumentAny::CurrencyPair(pair) = instrument else {
            return Err(anyhow!("SPOT_NATIVE_FILL_NOT_SPOT"));
        };
        let commission = fill
            .commission
            .ok_or_else(|| anyhow!("SPOT_NATIVE_COMMISSION_UNKNOWN"))?;
        let fee_currency = if commission.currency == pair.base_currency {
            pair.base_currency
        } else if commission.currency == pair.quote_currency {
            pair.quote_currency
        } else {
            return Err(anyhow!("SPOT_NATIVE_COMMISSION_CURRENCY"));
        };
        ensure!(
            commission.currency.precision == fee_currency.precision,
            "SPOT_NATIVE_COMMISSION_PRECISION"
        );
        let mut deltas = account.calculate_pnls(instrument, fill, None)?;
        if let Some(delta) = deltas
            .iter_mut()
            .find(|delta| delta.currency == commission.currency)
        {
            *delta = delta
                .checked_sub(commission)
                .ok_or_else(|| anyhow!("SPOT_NATIVE_NET_FLOW_RANGE"))?;
        } else {
            deltas.push(-commission);
        }
        for delta in deltas {
            let key = delta.currency.to_string();
            let previous = expected
                .get(&key)
                .copied()
                .unwrap_or_else(|| Money::zero(delta.currency));
            ensure!(
                previous.currency.precision == delta.currency.precision,
                "SPOT_NATIVE_CURRENCY_DEFINITION_CHANGED"
            );
            let total = previous
                .checked_add(delta)
                .ok_or_else(|| anyhow!("SPOT_NATIVE_TOTAL_RANGE"))?;
            ensure!(
                total.as_decimal() >= Decimal::ZERO,
                "SPOT_NATIVE_FILL_UNFUNDED_OR_ROLLED_BACK"
            );
            expected.insert(key, total);
        }
        let key = commission.currency.to_string();
        let previous = fees
            .get(&key)
            .copied()
            .unwrap_or_else(|| Money::zero(commission.currency));
        fees.insert(
            key,
            previous
                .checked_add(commission)
                .ok_or_else(|| anyhow!("SPOT_NATIVE_FEES_RANGE"))?,
        );
    }
    let mut actual: BTreeMap<_, _> = account
        .balances_total()
        .into_iter()
        .map(|(currency, money)| (currency.to_string(), money))
        .collect();
    for (currency, total) in expected {
        let observed = match actual.remove(&currency) {
            Some(observed) => observed,
            None if total.as_decimal() == Decimal::ZERO => Money::zero(total.currency),
            None => return Err(anyhow!("SPOT_NATIVE_BALANCE_MISSING")),
        };
        ensure!(
            observed == total && observed.currency.precision == total.currency.precision,
            "SPOT_NATIVE_ACCOUNT_FILL_RECONCILIATION_FAILED"
        );
    }
    ensure!(
        actual
            .iter()
            .all(|(currency, money)| known_currencies.contains(currency)
                && money.as_decimal() == Decimal::ZERO),
        "SPOT_NATIVE_UNEXPLAINED_BALANCE"
    );
    let mut actual_fees: BTreeMap<_, _> = cash
        .base
        .commissions
        .iter()
        .map(|(currency, money)| (currency.to_string(), *money))
        .collect();
    for (currency, fee) in fees {
        let observed = actual_fees
            .remove(&currency)
            .unwrap_or_else(|| Money::zero(fee.currency));
        ensure!(
            observed == fee && observed.currency.precision == fee.currency.precision,
            "SPOT_NATIVE_FEE_RECONCILIATION_FAILED"
        );
    }
    ensure!(
        actual_fees
            .values()
            .all(|money| money.as_decimal() == Decimal::ZERO),
        "SPOT_NATIVE_UNEXPLAINED_COMMISSION"
    );
    Ok(())
}

fn report_statistics(returns: &ReportCurrencyDailyReturnsV1) -> Vec<NativeStatisticV1> {
    // Any missing day invalidates aggregate statistics; never join the remaining
    // observations into a shorter invented series or call the position-return API.
    if returns.days.is_empty()
        || returns
            .days
            .iter()
            .any(|day| day.value.is_none() || day.reason.is_some())
    {
        return Vec::new();
    }
    let mut analyzer = PortfolioAnalyzer::default();
    analyzer.register_statistic(Arc::new(MaxDrawdown::new()));
    let replacements: [(Statistic, Statistic); 3] = [
        (
            Arc::new(ReturnsVolatility::new(None)),
            Arc::new(ReturnsVolatility::new(Some(365))),
        ),
        (
            Arc::new(SharpeRatio::new(None)),
            Arc::new(SharpeRatio::new(Some(365))),
        ),
        (
            Arc::new(SortinoRatio::new(None)),
            Arc::new(SortinoRatio::new(Some(365))),
        ),
    ];
    for (old, new) in replacements {
        analyzer.deregister_statistic(&old);
        analyzer.register_statistic(new);
    }
    for day in &returns.days {
        analyzer.portfolio_returns.insert(
            day.day_end_ns.get().into(),
            day.value.expect("complete days"),
        );
    }
    let mut output: Vec<_> = analyzer
        .get_performance_stats_portfolio_returns()
        .into_iter()
        .map(|(native_key, value)| NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key,
            currency: None,
            value: value.is_finite().then_some(value),
            reason_code: (!value.is_finite()).then(|| "NATIVE_STATISTIC_UNAVAILABLE".into()),
        })
        .collect();
    output.sort_by(|a, b| a.native_key.cmp(&b.native_key));
    output
}

/// Called only by simulation's owned fresh engine branch: only frozen Bar data,
/// one QZ TargetReplay strategy and the explicit fee adapter were installed.
/// The generic observer has no access to this attestation path.
pub(crate) fn finish_report(
    engine: &BacktestEngine,
    request: &NativeSimulationRequestV1,
    tape: SpotCashObservationTape,
    fills: &[OrderFilled],
) -> Result<NativeSpotCashReportV1> {
    let parameters = scenario(request)?;
    let accounts = engine.kernel().cache.borrow().accounts_all_owned();
    ensure!(
        accounts.len() == 1 && accounts[0].id().to_string() == tape.session.account_id,
        "SPOT_NATIVE_ACCOUNT_SCOPE"
    );
    let instruments = {
        let cache = engine.kernel().cache.borrow();
        cache
            .instrument_ids(None)
            .iter()
            .map(|id| {
                cache
                    .instrument(id)
                    .cloned()
                    .ok_or_else(|| anyhow!("SPOT_NATIVE_INSTRUMENT_MISSING"))
            })
            .collect::<Result<Vec<_>>>()?
    };
    // Compare the callback evidence to all actual cached native order events.
    // A zero-net omitted round-trip cannot disappear behind balance equality.
    let native_fills = {
        let cache = engine.kernel().cache.borrow();
        let mut observed = BTreeMap::new();
        for order in cache.orders_refs(None, None, None, None, None) {
            for event in order.events() {
                if let OrderEventAny::Filled(fill) = event {
                    ensure!(
                        observed
                            .insert(fill.event_id.to_string(), serde_json::to_value(fill)?)
                            .is_none(),
                        "SPOT_NATIVE_CACHED_FILL_REPLAY"
                    );
                }
            }
        }
        observed
    };
    let mut callbacks = BTreeMap::new();
    for fill in fills {
        ensure!(
            callbacks
                .insert(fill.event_id.to_string(), serde_json::to_value(fill)?)
                .is_none(),
            "SPOT_NATIVE_FILL_REPLAY"
        );
    }
    ensure!(
        native_fills == callbacks,
        "SPOT_NATIVE_FILL_CALLBACK_STREAM_INCOMPLETE"
    );
    reconcile(
        &accounts[0],
        fills,
        &instruments,
        tape.session.period_start_ns.get(),
        tape.session.period_end_ns.get(),
    )?;
    let frames: Vec<_> = tape
        .records
        .iter()
        .filter_map(|record| match record {
            ObservedSpotCashRecord::Snapshot { frame, .. } => Some(frame.clone()),
            _ => None,
        })
        .collect();
    let opening = frames
        .first()
        .ok_or_else(|| anyhow!("SPOT_NATIVE_OPENING_SNAPSHOT_MISSING"))?
        .snapshot
        .binding
        .clone();
    let closing = frames
        .last()
        .ok_or_else(|| anyhow!("SPOT_NATIVE_CLOSING_SNAPSHOT_MISSING"))?
        .snapshot
        .binding
        .clone();
    ensure!(
        opening.origin == NativeSpotSnapshotOriginV1::NativePublication
            && closing.origin == NativeSpotSnapshotOriginV1::NativePublication,
        "SPOT_NATIVE_LIFECYCLE_SNAPSHOTS_REQUIRED"
    );
    let flow_evidence = NativeSpotCashFlowEvidenceV1 {
        native_run_id: tape.native_run_id,
        native_instance_id: tape.native_instance_id,
        session_id: tape.observer_session_id,
        dataset_revision_id: tape.dataset_revision_id,
        account_id: tape.session.account_id.clone(),
        venue: tape.session.venue.clone(),
        period_start_ns: tape.session.period_start_ns,
        period_end_ns: tape.session.period_end_ns,
        opening_snapshot: opening,
        closing_snapshot: closing,
        observed_snapshot_count: count(frames.len() as u64)?,
        external_flow_count: Some(DbCounter::ZERO),
    };
    let context = SpotCashContext::new(&request.settings, tape.session.clone(), &tape.instruments)?;
    let daily_returns = context.daily_returns(&frames, &flow_evidence)?;
    let statistics = report_statistics(&daily_returns);
    let observations = tape
        .records
        .into_iter()
        .map(|record| -> Result<_> {
            Ok(match record {
                ObservedSpotCashRecord::Bar {
                    sequence,
                    native_clock_ns,
                    source,
                } => NativeSpotCashObservationV1 {
                    sequence,
                    native_clock_ns,
                    record: NativeSpotCashObservationKindV1::Bar {
                        event_time: source.event_time,
                        native: serde_json::to_value(source.native)?,
                        source_row_key: source.source_row_key,
                        bar_open_ns: source.bar_open_ns,
                        bar_close_ns: source.bar_close_ns,
                    },
                },
                ObservedSpotCashRecord::Snapshot {
                    sequence,
                    native_clock_ns,
                    native,
                    frame,
                    valuation,
                } => NativeSpotCashObservationV1 {
                    sequence,
                    native_clock_ns,
                    record: NativeSpotCashObservationKindV1::Snapshot {
                        origin: frame.snapshot.binding.origin,
                        native: serde_json::to_value(native)?,
                        frame,
                        valuation,
                    },
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NativeSpotCashReportV1 {
        schema_version: SchemaV1,
        session: tape.session,
        native_run_receipt: tape.native_run_receipt,
        instruments: tape.instruments,
        observations,
        flow_evidence,
        daily_returns,
        statistics,
        fee_schedule: parameters.schedule.clone(),
        fee_acceptance: parameters.acceptance,
        source_evidence: None,
    })
}

#[cfg(test)]
mod tests;
