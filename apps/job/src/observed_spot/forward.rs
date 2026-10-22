//! Causal model estimates from an observed inventory. No engine replay or orders.
use anyhow::{anyhow, ensure, Result};
use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use contracts::{
    observed_spot::*, portfolio::*, science::*, DbCounter, DecimalValue, Id, SchemaV1,
};
use nautilus_model::{instruments::Instrument, types::Quantity};
use std::{collections::BTreeMap, path::Path};

fn decimal(value: BigDecimal) -> Result<DecimalValue> {
    value.to_plain_string().parse().map_err(anyhow::Error::msg)
}
/// Derived continuous intents multiply two bounded source decimals. Retain the
/// full product, rather than rounding it to a database money column's scale.
fn exact_product(value: BigDecimal) -> Result<NativeForwardExactNotionalV1> {
    value
        .normalized()
        .to_plain_string()
        .try_into()
        .map_err(anyhow::Error::msg)
}
fn number(value: &DecimalValue) -> Result<f64> {
    let number = value
        .as_decimal()
        .to_f64()
        .ok_or_else(|| anyhow!("FORWARD_NUMBER_RANGE"))?;
    ensure!(number.is_finite(), "FORWARD_NONFINITE_NUMBER");
    Ok(number)
}

fn comparison(
    input: &AllocationInputV1,
    proposed_weights: Option<&[f64]>,
    method: &str,
    rounded_cost: Option<f64>,
) -> Result<NativeForwardModeledComparisonV1> {
    let mean = crate::validation::aligned_portfolio_forecast(&input.forecasts)?;
    let covariance = domain::portfolio::sample_covariance(
        &input.covariance_estimator,
        &input.return_history.asset_returns,
    )?;
    let aversion = number(&domain::portfolio::optimizer_settings(&input.optimizer)?.risk_aversion)?;
    let current = input
        .assets
        .iter()
        .map(|asset| number(&asset.current_weight))
        .collect::<Result<Vec<_>>>()?;
    let costs = input
        .assets
        .iter()
        .map(|asset| number(&asset.transaction_cost_rate))
        .collect::<Result<Vec<_>>>()?;
    let measure =
        |weights: &[f64], exact_cost: Option<f64>| -> Result<NativeForwardModeledOutcomeV1> {
            let gross_expected_return: f64 = weights.iter().zip(&mean).map(|(w, mu)| w * mu).sum();
            let linear_turnover_cost: f64 = exact_cost.unwrap_or_else(|| {
                weights
                    .iter()
                    .zip(&current)
                    .zip(&costs)
                    .map(|((w, old), cost)| (w - old).abs() * cost)
                    .sum()
            });
            // The same native sample covariance and quadratic form used by Clarabel.
            let vector = nalgebra::DVector::from_column_slice(weights);
            let matrix = nalgebra::DMatrix::from_fn(weights.len(), weights.len(), |row, col| {
                covariance[row][col]
            });
            let variance = vector.dot(&(matrix * &vector));
            let net_expected_return = gross_expected_return - linear_turnover_cost;
            let objective_value = aversion * variance - net_expected_return;
            ensure!(
                [
                    gross_expected_return,
                    linear_turnover_cost,
                    net_expected_return,
                    variance,
                    objective_value
                ]
                .iter()
                .all(|x| x.is_finite()),
                "FORWARD_NONFINITE_ESTIMATE"
            );
            Ok(NativeForwardModeledOutcomeV1 {
                gross_expected_return,
                linear_turnover_cost,
                net_expected_return,
                variance,
                objective_value,
            })
        };
    let proposed = proposed_weights
        .map(|weights| measure(weights, rounded_cost))
        .transpose()?;
    Ok(NativeForwardModeledComparisonV1 {
        method: method.into(),
        basis: "FORWARD_MODEL_ESTIMATE".into(),
        forecast_unit: "RETURN_PER_HORIZON".into(),
        hold: measure(&current, None)?, proposed,
        limitations: vec![
            "Model expectations are not realized returns or a daily-return forecast.".into(),
            "Linear frozen turnover costs omit unmodeled impact, queue, funding and future exit costs.".into(),
            "Public fee scenario and source historical-availability limitations remain unchanged.".into(),
            "Original observed quantities are revalued using cutoff-available marks; this is not a new account observation and must be invalidated when its source state changes.".into(),
        ],
    })
}

/// Check the same toward-zero lot rule used by native replay, without emitting
/// an order or claiming a fill. Continuous solver targets remain unchanged.
fn target_check(
    request: &NativeObservedSpotBuildRequestV1,
    valuation: &NativeObservedSpotValuationResultV1,
    market: &crate::catalog::NativeMarketData,
    rows: &[crate::spot_cash_capture::ClosedBarSourceRow],
    target: &NativeTargetPointV1,
    input: &AllocationInputV1,
) -> Result<Vec<NativeForwardSpotTradeEstimateV1>> {
    let total: BigDecimal = target
        .targets
        .iter()
        .map(|row| row.weight.as_decimal())
        .sum();
    ensure!(
        target.cash_weight.is_nonnegative()
            && target.targets.iter().all(|row| row.weight.is_nonnegative())
            && total <= BigDecimal::from(1),
        "FORWARD_TARGET_BORROWING_OR_SHORTING"
    );
    // A conservative reserve for the existing linear trade deltas.
    // This is only model funding feasibility, never a quote/fee applicability proof.
    let cost: BigDecimal = target
        .targets
        .iter()
        .zip(&input.assets)
        .map(|(target, asset)| {
            (target.weight.as_decimal() - asset.current_weight.as_decimal()).abs()
                * asset.transaction_cost_rate.as_decimal()
        })
        .sum();
    if input.constraints.cash_funding.is_none() {
        ensure!(
            &total + cost <= BigDecimal::from(1),
            "FORWARD_LINEAR_COST_FUNDING_SHORTFALL"
        );
    }
    let mut estimates = Vec::new();
    let mut buys = BigDecimal::from(0);
    let mut sells = BigDecimal::from(0);
    let mut fees = BigDecimal::from(0);
    for ((target, series), asset) in target.targets.iter().zip(&market.series).zip(&input.assets) {
        let instrument = &series.instrument;
        let base = instrument
            .base_currency()
            .ok_or_else(|| anyhow!("FORWARD_BASE_CURRENCY"))?;
        let quantity = request
            .initial_state
            .snapshot
            .balances
            .iter()
            .find(|balance| balance.total.currency == base.to_string())
            .map(|balance| balance.total.amount.as_decimal().clone())
            .unwrap_or_default();
        let row = rows
            .iter()
            .filter(|row| {
                row.native.bar_type == series.bar_type
                    && row.bar_close_ns <= valuation.asof_ns
                    && row.native.ts_init.as_u64() <= valuation.asof_ns.get()
            })
            .max_by_key(|row| (row.native.ts_init, row.bar_close_ns))
            .ok_or_else(|| anyhow!("FORWARD_SIZING_PIT_PRICE_MISSING"))?;
        ensure!(
            valuation.asof_ns.get() - row.bar_close_ns.get()
                <= request
                    .execution_settings
                    .multi_currency_spot_cash
                    .as_ref()
                    .unwrap()
                    .maximum_price_age_ns
                    .get(),
            "FORWARD_SIZING_PRICE_STALE"
        );
        let price: BigDecimal = row.native.close.as_decimal().to_string().parse()?;
        let increment: BigDecimal = instrument
            .size_increment()
            .as_decimal()
            .to_string()
            .parse()?;
        ensure!(
            price > BigDecimal::from(0) && increment > BigDecimal::from(0),
            "FORWARD_SIZING_PRICE_OR_INCREMENT_INVALID"
        );
        let target_notional = target.weight.as_decimal() * valuation.total.as_decimal();
        let current_notional = &quantity * &price;
        let desired = &target_notional / &price;
        let delta = desired - &quantity;
        let units = (&delta.abs() / &increment).with_scale_round(0, RoundingMode::Down) * increment;
        let residue = (delta.abs() - &units) * &price / valuation.total.as_decimal();
        ensure!(
            residue <= *request.execution_settings.exposure_tolerance.as_decimal(),
            "SIMULATION_LOT_ROUNDING_EXCEEDS_TOLERANCE"
        );
        ensure!(
            delta >= BigDecimal::from(0) || units <= quantity,
            "FORWARD_SELL_EXCEEDS_OWNED_INVENTORY"
        );
        if units != BigDecimal::from(0) {
            let native = Quantity::from_decimal_dp(
                units.to_plain_string().parse()?,
                instrument.size_precision(),
            )?;
            let notional =
                instrument.try_calculate_notional_value(native, row.native.close, None)?;
            ensure!(
                instrument.max_quantity().is_none_or(|max| native <= max),
                "FORWARD_QUANTITY_ABOVE_MAXIMUM"
            );
            // Full disposal may close original residual inventory below venue minima,
            // following native risk-engine's whole-position-exit policy.
            let full_exit = !target.weight.is_positive() && units == quantity;
            ensure!(
                full_exit || instrument.min_quantity().is_none_or(|min| native >= min),
                "FORWARD_QUANTITY_BELOW_MINIMUM"
            );
            ensure!(
                full_exit
                    || instrument
                        .min_notional()
                        .is_none_or(|min| notional.currency == min.currency && notional >= min),
                "FORWARD_NOTIONAL_BELOW_MINIMUM"
            );
            ensure!(
                instrument
                    .max_notional()
                    .is_none_or(|max| notional.currency == max.currency && notional <= max),
                "FORWARD_NOTIONAL_ABOVE_MAXIMUM"
            );
        }
        let traded_notional = &units * &price;
        let modeled_linear_cost = &traded_notional * asset.transaction_cost_rate.as_decimal();
        fees += &modeled_linear_cost;
        let signed_quantity = if delta < BigDecimal::from(0) {
            sells += &traded_notional;
            -units
        } else {
            buys += &traded_notional;
            units
        };
        let post_rounding_quantity = &quantity + &signed_quantity;
        let post_rounding_notional = &post_rounding_quantity * &price;
        let rounded_trade_notional = &signed_quantity * &price;
        estimates.push(NativeForwardSpotTradeEstimateV1 {
            instrument_id: target.instrument_id.clone(),
            current_quantity: decimal(quantity)?,
            current_notional: decimal(current_notional.clone())?,
            target_notional: exact_product(target_notional.clone())?,
            net_trade_notional: exact_product(target_notional - current_notional)?,
            rounded_trade_quantity: decimal(signed_quantity)?,
            post_rounding_quantity: decimal(post_rounding_quantity)?,
            post_rounding_notional: decimal(post_rounding_notional)?,
            rounded_trade_notional: decimal(rounded_trade_notional)?,
            price: decimal(price)?,
            price_event_ns: row.bar_close_ns,
            price_available_ns: DbCounter::new(row.native.ts_init.as_u64())
                .map_err(anyhow::Error::msg)?,
            source_row_key: row.source_row_key.clone(),
            modeled_linear_cost: decimal(modeled_linear_cost)?,
        });
    }
    let cash = valuation
        .legs
        .iter()
        .find(|leg| leg.currency == valuation.report_currency)
        .ok_or_else(|| anyhow!("FORWARD_NATIVE_QUOTE_BALANCE_MISSING"))?;
    let funded_cash = cash.report_value.as_decimal() + sells - buys - fees;
    ensure!(
        funded_cash >= BigDecimal::from(0),
        "FORWARD_ROUNDED_TRADE_FUNDING_SHORTFALL"
    );
    if input.constraints.cash_funding.is_some() {
        ensure!(
            funded_cash
                >= (input.constraints.min_cash_weight.as_decimal()
                    - input.exposure_tolerance.as_decimal())
                    * valuation.total.as_decimal(),
            "FORWARD_ROUNDED_MIN_CASH_SHORTFALL"
        );
    }
    Ok(estimates)
}

pub fn build(
    catalog: &Path,
    request: &NativeObservedSpotBuildRequestV1,
    mut read: impl FnMut(Id) -> Result<Vec<u8>>,
) -> Result<NativeObservedSpotBuildResultV1> {
    #[cfg(not(feature = "hyperliquid-offline"))]
    {
        let _ = (catalog, request, read);
        Err(anyhow!(
            "SPOT_SOURCE_CAPABILITY_NOT_BUILT: hyperliquid-offline"
        ))
    }
    #[cfg(feature = "hyperliquid-offline")]
    {
        domain::execution::observed_spot_build_request(request)?;
        let cutoff = request.selection.decision_cutoff_ns;
        let start = &request.initial_state;
        domain::observed_spot::starting_balances(start, &request.execution_settings, cutoff)?;
        let maximum_age =
            u64::from(request.mandate.rebalance_schedule.max_input_age_seconds) * 1_000_000_000;
        ensure!(
            cutoff.get() - start.snapshot.ts_event.get() <= maximum_age,
            "FORWARD_ACCOUNT_STATE_STALE"
        );
        let until = DbCounter::new(
            cutoff
                .get()
                .checked_add(
                    u64::from(request.mandate.rebalance_schedule.target_ttl_seconds)
                        * 1_000_000_000,
                )
                .ok_or_else(|| anyhow!("FORWARD_TARGET_CLOCK_RANGE"))?,
        )
        .map_err(anyhow::Error::msg)?;
        let costs: NativeSimulationSettingsV1 =
            serde_json::from_slice(&read(request.mandate.constraints.transaction_costs_ref)?)?;
        ensure!(
            serde_json::to_value(&costs)? == serde_json::to_value(&request.execution_settings)?,
            "PORTFOLIO_EXECUTION_SETTINGS_SOURCE_MISMATCH"
        );
        if let Some(policy) = &request.rolling_liquidity {
            let original: NativeRollingBarLiquidityPolicyV1 =
                serde_json::from_slice(&read(request.mandate.constraints.liquidity_ref.unwrap())?)?;
            ensure!(original == *policy, "PORTFOLIO_ROLLING_POLICY_MISMATCH");
        }
        let market = crate::catalog::load_catalog(catalog, &request.selection)?;
        super::validate_initial_market(start, &market, &costs)?;
        let instruments = market
            .series
            .iter()
            .map(|series| series.instrument.clone())
            .collect::<Vec<_>>();
        let NativeModelRefV1::FrozenSpotFeeScenario { parameters, .. } = &costs.fee_model else {
            return Err(anyhow!("FORWARD_SPOT_FEE_MODEL_REQUIRED"));
        };
        ensure!(
            parameters.schedule.source.observed_at_ns <= cutoff,
            "FORWARD_FEE_SOURCE_FROM_FUTURE"
        );
        crate::spot_fees::FrozenSpotFeeModel::try_new_public_rate_scenario(
            parameters.schedule.clone(),
            &instruments,
            cutoff.get(),
            until.get(),
        )?;
        let valuation_selection = request.selection.clone();
        let source = crate::spot_cash_source::load_closed_rows(catalog, &valuation_selection)?;
        let mut valuation = super::value_with_closed_rows_at(
            catalog,
            &NativeObservedSpotValuationRequestV1 {
                schema_version: SchemaV1,
                dataset_revision_id: request.dataset_revision_id,
                initial_state: start.clone(),
                selection: valuation_selection,
                settings: costs.clone(),
            },
            &source.rows,
            cutoff,
        )?;
        valuation.source_evidence = Some(source.evidence);
        let prepared = crate::portfolio::prepare_observed_spot(
            &market,
            &request.selection,
            &request.mandate,
            &costs,
            &request.assets,
            &request.members,
            &mut read,
        )?;
        let bar_notionals = if request.rolling_liquidity.is_some() {
            crate::catalog::last_bar_notionals(&market)?
        } else {
            Vec::new()
        };
        let source_assets = domain::execution::portfolio_rolling_liquidity_assets(
            &request.selection,
            &request.assets,
            request.rolling_liquidity.as_ref(),
            &request.mandate.base_currency,
            prepared.forecasts.forecast_asof_ns,
            cutoff,
            &bar_notionals,
        )?;
        let mut assets = domain::execution::portfolio_costs(
            &request.selection,
            &request.mandate,
            &costs,
            &source_assets,
            &prepared.slippage,
        )?;
        let by_instrument: BTreeMap<_, _> = valuation
            .legs
            .iter()
            .filter_map(|leg| leg.instrument_id.as_ref().map(|id| (id.as_str(), leg)))
            .collect();
        let mut allocated = BigDecimal::from(0);
        for asset in &mut assets {
            let value = by_instrument
                .get(asset.instrument_id.as_str())
                .map(|leg| leg.report_value.as_decimal().clone())
                .unwrap_or_default();
            // Same 18-place native ratio representation as the rolling adapter.
            asset.current_weight = decimal(
                (value / valuation.total.as_decimal()).with_scale_round(18, RoundingMode::HalfEven),
            )?;
            allocated += asset.current_weight.as_decimal();
        }
        let cash = decimal(BigDecimal::from(1) - allocated)?;
        let mut input =
            crate::portfolio::allocation_input(&request.mandate, assets, cash, &prepared);
        input.capital_assumption = valuation.total.clone();
        let allocation = crate::allocate(&input)?;
        domain::portfolio::allocation_result(&input, &allocation)?;
        let proposed_weights = allocation
            .targets
            .as_ref()
            .map(|targets| {
                targets
                    .iter()
                    .map(|target| number(&target.weight))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?;
        let modeled_comparison = comparison(
            &input,
            proposed_weights.as_deref(),
            "MATCHED_LINEAR_RETURN_VARIANCE_V1",
            None,
        )?;
        let mut target = allocation
            .targets
            .clone()
            .zip(allocation.cash_weight.clone())
            .map(|(targets, cash_weight)| NativeTargetPointV1 {
                schema_version: SchemaV1,
                rebalance_instrument_ids: None,
                asof_ns: cutoff,
                valid_until_ns: until,
                targets,
                cash_weight,
            });
        let mut target_block_reason = allocation.reason_code.clone();
        let mut trade_estimates = Vec::new();
        if let Some(point) = &target {
            match target_check(request, &valuation, &market, &source.rows, point, &input) {
                Ok(estimates) => trade_estimates = estimates,
                Err(error) => {
                    target_block_reason = Some(error.to_string());
                    target = None;
                }
            }
        }
        let rounded_modeled_comparison = if target.is_some() {
            let weights = input
                .assets
                .iter()
                .map(|asset| {
                    let value = trade_estimates
                        .iter()
                        .find(|trade| trade.instrument_id == asset.instrument_id)
                        .map(|trade| trade.post_rounding_notional.as_decimal().clone())
                        .unwrap_or_default();
                    number(&decimal(
                        (value / valuation.total.as_decimal())
                            .with_scale_round(18, RoundingMode::HalfEven),
                    )?)
                })
                .collect::<Result<Vec<_>>>()?;
            let cost: BigDecimal = trade_estimates
                .iter()
                .map(|trade| trade.modeled_linear_cost.as_decimal())
                .sum();
            // A dimensionless model statistic may be non-terminating decimal;
            // original exact trade amounts remain unchanged in the evidence.
            let normalized_cost = cost
                .to_f64()
                .ok_or_else(|| anyhow!("FORWARD_NUMBER_RANGE"))?
                / number(&valuation.total)?;
            Some(comparison(
                &input,
                Some(&weights),
                "MATCHED_ROUNDED_LINEAR_RETURN_VARIANCE_V1",
                Some(normalized_cost),
            )?)
        } else {
            None
        };
        Ok(NativeObservedSpotBuildResultV1 {
            schema_version: SchemaV1,
            kind: "OBSERVED_SPOT_FORWARD_BUILD".into(),
            basis: "FORWARD_MODEL_ESTIMATE_FROM_OBSERVED_SPOT_BALANCES".into(),
            request: request.clone(),
            valuation,
            build: NativePortfolioBuildResultV1 {
                schema_version: SchemaV1,
                bar_notionals,
                slippage_references: prepared.slippage,
                input,
                allocation,
                consumed_fuel: prepared.consumed_fuel,
            },
            modeled_comparison,
            rounded_modeled_comparison,
            instrument_definitions: instruments
                .iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<Vec<_>, _>>()?,
            trade_estimates,
            target,
            target_block_reason,
        })
    }
}
