//! Two independently owned native runs from the same observed spot balances.
//! No opening fills, synthetic positions, account writes or inferred cash.
use crate::spot_cash_capture::ClosedBarSourceRow;
use anyhow::{anyhow, ensure, Result};
use contracts::{observed_spot::*, science::*, Id, SchemaV1};
use nautilus_model::instruments::{Instrument, InstrumentAny};
use std::{collections::BTreeSet, path::Path};

const BASIS: &str = "HISTORICAL_REPLAY_FROM_OBSERVED_SPOT_BALANCES";

/// Exact valuation of original balances, distinct from an account observation
/// and from either leg's subsequent simulated performance.
pub fn value_with_closed_rows(
    catalog: &Path,
    request: &NativeObservedSpotValuationRequestV1,
    rows: &[ClosedBarSourceRow],
) -> Result<NativeObservedSpotValuationResultV1> {
    let asof = request.initial_state.snapshot.ts_event;
    use nautilus_model::types::Money;
    let initial = &request.initial_state;
    let mut available = initial.available_ns.max(initial.open_orders_available_ns);
    let balances = domain::observed_spot::starting_balances(initial, &request.settings, available)?;
    ensure!(
        request.selection.decision_cutoff_ns == asof && request.selection.event_end_ns <= asof,
        "OBSERVED_SPOT_VALUATION_CUTOFF_MISMATCH"
    );
    let market = crate::catalog::load_catalog(catalog, &request.selection)?;
    validate_initial_market(initial, &market, &request.settings)?;
    let policy = request
        .settings
        .multi_currency_spot_cash
        .as_ref()
        .expect("validated spot policy");
    ensure!(
        balances
            .iter()
            .any(|balance| balance.currency == policy.report_currency),
        "OBSERVED_SPOT_REPORT_CURRENCY_BALANCE_MISSING"
    );
    let mut total = contracts::DecimalValue::zero();
    let mut legs = Vec::new();
    for balance in balances {
        let native = Money::from_str_exact(&format!(
            "{} {}",
            balance.amount.as_decimal().to_plain_string(),
            balance.currency
        ))
        .map_err(anyhow::Error::msg)?;
        let roundtrip: contracts::DecimalValue = native
            .as_decimal()
            .to_string()
            .parse()
            .map_err(anyhow::Error::msg)?;
        ensure!(
            roundtrip == balance.amount,
            "OBSERVED_SPOT_MONEY_PRECISION_LOSS"
        );
        let direct = balance.currency == policy.report_currency;
        let series = if direct {
            None
        } else {
            Some(
                market
                    .series
                    .iter()
                    .find(|series| {
                        series
                            .instrument
                            .base_currency()
                            .is_some_and(|currency| currency.to_string() == balance.currency)
                    })
                    .ok_or_else(|| anyhow!("OBSERVED_SPOT_UNVALUED_CURRENCY"))?,
            )
        };
        let row = if direct || !balance.amount.is_positive() {
            None
        } else {
            let series = series.expect("non-report currency");
            let row = rows
                .iter()
                .filter(|row| {
                    row.native.bar_type == series.bar_type
                        && row.bar_close_ns <= asof
                        && row.native.ts_init.as_u64() <= asof.get()
                })
                .max_by_key(|row| (row.native.ts_init, row.bar_close_ns))
                .ok_or_else(|| anyhow!("OBSERVED_SPOT_PIT_PRICE_MISSING"))?;
            ensure!(
                row.bar_open_ns < row.bar_close_ns
                    && row.bar_close_ns.get() <= row.native.ts_init.as_u64()
                    && asof.get() - row.bar_close_ns.get() <= policy.maximum_price_age_ns.get()
                    && series.bars.iter().any(|bar| serde_json::to_value(bar).ok()
                        == serde_json::to_value(row.native).ok()),
                "OBSERVED_SPOT_PIT_PRICE_SOURCE_OR_AGE"
            );
            Some(row)
        };
        let price: Option<contracts::DecimalValue> = row
            .map(|row| row.native.close.as_decimal().to_string().parse())
            .transpose()
            .map_err(anyhow::Error::msg)?;
        if let Some(row) = row {
            available = available.max(
                contracts::DbCounter::new(row.native.ts_init.as_u64())
                    .map_err(anyhow::Error::msg)?,
            );
        }
        let report_value =
            domain::spot_cash::inventory_report_value(&balance.amount, price.as_ref())
                .map_err(|reason| anyhow!("OBSERVED_SPOT_VALUE_UNAVAILABLE: {reason:?}"))?;
        total = total
            .checked_add(&report_value)
            .ok_or_else(|| anyhow!("OBSERVED_SPOT_VALUE_RANGE"))?;
        legs.push(NativeObservedSpotValuationLegV1 {
            currency: balance.currency,
            native_total: balance.amount,
            report_value,
            instrument_id: series.map(|series| series.instrument.id().to_string()),
            price,
            price_event_ns: row.map(|row| row.bar_close_ns),
            price_available_ns: row.map(|row| {
                contracts::DbCounter::new(row.native.ts_init.as_u64())
                    .expect("source clock validated")
            }),
            source_row_key: row.map(|row| row.source_row_key.clone()),
        });
    }
    ensure!(total.is_positive(), "OBSERVED_SPOT_NONPOSITIVE_NAV");
    Ok(NativeObservedSpotValuationResultV1 {
        schema_version: SchemaV1,
        kind: "OBSERVED_SPOT_VALUATION".into(),
        basis: "DERIVED_REPORT_CURRENCY_INVENTORY_VALUE".into(),
        request: request.clone(),
        report_currency: policy.report_currency.clone(),
        asof_ns: asof,
        available_ns: available,
        total,
        legs,
        source_evidence: None,
    })
}

pub fn value(
    catalog: &Path,
    request: &NativeObservedSpotValuationRequestV1,
) -> Result<NativeObservedSpotValuationResultV1> {
    #[cfg(feature = "hyperliquid-offline")]
    {
        let source = crate::spot_cash_source::load_closed_rows(catalog, &request.selection)?;
        let mut result = value_with_closed_rows(catalog, request, &source.rows)?;
        result.source_evidence = Some(source.evidence);
        Ok(result)
    }
    #[cfg(not(feature = "hyperliquid-offline"))]
    {
        let _ = (catalog, request);
        Err(anyhow!(
            "SPOT_SOURCE_CAPABILITY_NOT_BUILT: hyperliquid-offline"
        ))
    }
}

pub(crate) fn validate_market(
    market: &crate::catalog::NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<()> {
    domain::spot_cash::execution_settings(settings)?;
    let policy = settings
        .multi_currency_spot_cash
        .as_ref()
        .expect("validated spot policy");
    let venue = market
        .series
        .first()
        .ok_or_else(|| anyhow!("OBSERVED_SPOT_EMPTY_MARKET"))?
        .instrument
        .venue();
    let mut bases = BTreeSet::new();
    ensure!(
        market.series.len() == policy.allowed_instrument_ids.len(),
        "OBSERVED_SPOT_UNIVERSE_MISMATCH"
    );
    for series in &market.series {
        let InstrumentAny::CurrencyPair(pair) = &series.instrument else {
            return Err(anyhow!("OBSERVED_SPOT_CURRENCY_PAIR_REQUIRED"));
        };
        ensure!(
            series.instrument_updates.is_empty()
                && pair.venue() == venue
                && pair.quote_currency.to_string() == policy.report_currency
                && pair.base_currency != pair.quote_currency
                && !pair.is_inverse()
                && !pair.has_expiration()
                && pair.multiplier().as_decimal() == rust_decimal::Decimal::ONE
                && bases.insert(pair.base_currency.to_string())
                && policy
                    .allowed_instrument_ids
                    .contains(&pair.id().to_string()),
            "OBSERVED_SPOT_MARKET_UNSUPPORTED"
        );
    }
    Ok(())
}

fn validate_initial_market(
    initial: &NativeObservedSpotStartV1,
    market: &crate::catalog::NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<()> {
    validate_market(market, settings)?;
    // AccountId's official issuer is the original venue. Currency equality
    // alone cannot authorize moving inventory to another venue without a
    // modeled transfer, fees and availability. This adapter has no transfers.
    let account =
        nautilus_model::identifiers::AccountId::new_checked(&initial.snapshot.account_id)?;
    ensure!(
        market
            .series
            .iter()
            .all(|series| series.instrument.venue() == account.get_issuer()),
        "OBSERVED_SPOT_ACCOUNT_VENUE_MISMATCH"
    );
    Ok(())
}

fn simulation_leg(result: Result<NativeSimulationResultV1>) -> NativeObservedSpotSimulationLegV1 {
    match result {
        Ok(result) => NativeObservedSpotSimulationLegV1 {
            status: NativeObservedSpotLegStatusV1::Completed,
            result: Some(result),
            reason: None,
        },
        Err(error) => NativeObservedSpotSimulationLegV1 {
            status: NativeObservedSpotLegStatusV1::Failed,
            result: None,
            reason: Some(format!("{error:#}")),
        },
    }
}

fn replay(
    catalog: &Path,
    request: &NativeSimulationRequestV1,
    initial: &NativeObservedSpotStartV1,
    dataset: Id,
    rows: Vec<ClosedBarSourceRow>,
) -> Result<NativeSimulationResultV1> {
    let result =
        crate::simulation::run_observed_spot(catalog, request, dataset, rows, initial)?;
    domain::execution::check_observed_spot_simulation(request, &result, initial)?;
    Ok(result)
}

/// Trusted in-process source adapter entry, like simulate_spot_cash_candidate.
/// ClosedBarSourceRow cannot be deserialized from caller-asserted JSON.
pub fn compare_with_closed_rows(
    catalog: &Path,
    request: &NativeObservedSpotComparisonRequestV1,
    rows: Vec<ClosedBarSourceRow>,
) -> Result<NativeObservedSpotComparisonResultV1> {
    domain::observed_spot::starting_balances(
        &request.initial_state,
        &request.simulation.settings,
        request.simulation.selection.event_start_ns,
    )?;
    ensure!(
        !request.simulation.target_points.is_empty(),
        "OBSERVED_SPOT_REBALANCE_TARGETS_REQUIRED"
    );
    validate_initial_market(
        &request.initial_state,
        &crate::catalog::load_catalog(catalog, &request.simulation.selection)?,
        &request.simulation.settings,
    )?;
    let mut hold_request = request.simulation.clone();
    hold_request.target_points.clear();
    let hold = simulation_leg(replay(
        catalog,
        &hold_request,
        &request.initial_state,
        request.dataset_revision_id,
        rows.clone(),
    ));
    let mut rebalance = simulation_leg(replay(
        catalog,
        &request.simulation,
        &request.initial_state,
        request.dataset_revision_id,
        rows,
    ));
    if let (Some(hold), Some(rebalance)) = (&hold.result, &mut rebalance.result) {
        add_hold_statistics(hold, rebalance);
    }
    Ok(NativeObservedSpotComparisonResultV1 {
        schema_version: SchemaV1,
        kind: "OBSERVED_SPOT_ACCOUNT_COMPARISON".into(),
        basis: BASIS.into(),
        request: request.clone(),
        hold,
        rebalance,
    })
}

pub fn compare(
    catalog: &Path,
    request: &NativeObservedSpotComparisonRequestV1,
) -> Result<NativeObservedSpotComparisonResultV1> {
    #[cfg(feature = "hyperliquid-offline")]
    {
        let source =
            crate::spot_cash_source::load_closed_rows(catalog, &request.simulation.selection)?;
        let mut result = compare_with_closed_rows(catalog, request, source.rows)?;
        for leg in [&mut result.hold, &mut result.rebalance] {
            if let Some(replay) = &mut leg.result {
                replay
                    .spot_cash_report
                    .as_mut()
                    .ok_or_else(|| anyhow!("OBSERVED_SPOT_REPORT_MISSING"))?
                    .source_evidence = Some(source.evidence.clone());
            }
        }
        Ok(result)
    }
    #[cfg(not(feature = "hyperliquid-offline"))]
    {
        let _ = (catalog, request);
        Err(anyhow!(
            "SPOT_SOURCE_CAPABILITY_NOT_BUILT: hyperliquid-offline"
        ))
    }
}

fn add_hold_statistics(
    hold: &contracts::science::NativeSimulationResultV1,
    rebalance: &mut contracts::science::NativeSimulationResultV1,
) {
    if let (Some(hold), Some(rebalance)) = (&hold.spot_cash_report, &mut rebalance.spot_cash_report)
    {
        rebalance.statistics.extend(crate::performance::versus_hold(
            &rebalance.daily_returns,
            &hold.daily_returns,
        ));
        rebalance
            .statistics
            .sort_by(|a, b| a.native_key.cmp(&b.native_key));
    }
}
