//! QZ-owned adapter at Nautilus 0.63's public FeeModel boundary.
//!
//! No native instrument is changed, and no account balance is adjusted here.
//! The engine books returned Money once; valuation must not deduct it again.
//! The caller must bound ALL possible fill events (including latency) by the
//! replay window admitted below. FeeModel has no fill timestamp, so a changing
//! fee schedule cannot be supported by consulting order.ts_init.
use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{bail, ensure, Context, Result};
use bigdecimal::{BigDecimal, RoundingMode};
use contracts::spot_fees::{
    FrozenSpotFeeRuleV1, FrozenSpotFeeScheduleV1, SpotFeeBasisV1, SpotFeeCurrencyV1,
    SpotFeeEvidenceStatusV1, SpotFeeLiquidityV1, SpotFeeOrderSideV1, SpotFeeRoundingV1,
};
use nautilus_execution::models::fee::{FeeModel, FeeModelHandle};
use nautilus_model::{
    enums::{LiquiditySide, OrderSide},
    instruments::{CurrencyPair, InstrumentAny},
    orders::{Order, OrderAny},
    types::{Currency, Money, Price, Quantity},
};
use rust_decimal::Decimal;

pub const HYPERLIQUID_PUBLIC_FEE_SOURCE: &str =
    "https://hyperliquid.gitbook.io/hyperliquid-docs/trading/fees";

type RuleKey = (String, SpotFeeOrderSideV1, SpotFeeLiquidityV1);

/// Immutable per-run fee research schedule. Current public rates do not
/// establish source coverage for any historical, present or future window.
/// No successful calculation can promote this scenario to market fee evidence.
#[derive(Clone, Debug)]
pub struct FrozenSpotFeeModel {
    schedule: FrozenSpotFeeScheduleV1,
    instruments: BTreeMap<String, (Currency, Currency)>,
    rules: BTreeMap<RuleKey, FrozenSpotFeeRuleV1>,
}

impl FrozenSpotFeeModel {
    /// `fill_start_ns..fill_end_ns` must include the entire execution horizon,
    /// not merely bar timestamps. No later per-fill timestamp check is possible.
    /// This containment is a scenario bound, not verification of source validity.
    /// Only SYNTHETIC is currently admitted; both evidence-bearing statuses fail.
    pub fn try_new(
        schedule: FrozenSpotFeeScheduleV1,
        instruments: &[InstrumentAny],
        fill_start_ns: u64,
        fill_end_ns: u64,
    ) -> Result<Self> {
        Self::build(schedule, instruments, fill_start_ns, fill_end_ns, false)
    }

    /// Explicit policy opt-in for current public-rate research scenarios whose
    /// historical/current/future applicability is unverified. The integration
    /// must obtain policy acceptance before choosing this entrypoint, retain
    /// the fee scenario status in provenance, and keep actual market-data origin
    /// separate. This does not grant scientific or DATA_BACKED admission.
    pub fn try_new_public_rate_scenario(
        schedule: FrozenSpotFeeScheduleV1,
        instruments: &[InstrumentAny],
        fill_start_ns: u64,
        fill_end_ns: u64,
    ) -> Result<Self> {
        ensure!(
            schedule.source.status == SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability,
            "SPOT_FEE_EXPLICIT_PUBLIC_SCENARIO_REQUIRED"
        );
        Self::build(schedule, instruments, fill_start_ns, fill_end_ns, true)
    }

    fn build(
        schedule: FrozenSpotFeeScheduleV1,
        instruments: &[InstrumentAny],
        fill_start_ns: u64,
        fill_end_ns: u64,
        allow_unverified_public_scenario: bool,
    ) -> Result<Self> {
        ensure!(
            fill_start_ns < fill_end_ns
                && schedule.valid_from_ns.get() <= fill_start_ns
                && fill_end_ns <= schedule.valid_until_ns.get(),
            "SPOT_FEE_WINDOW_DOES_NOT_COVER_EXECUTION"
        );
        let source = &schedule.source;
        ensure!(
            bounded_text(&source.source_ref, 200)
                && bounded_text(&source.source_uri, 500)
                && bounded_text(&source.assumptions, 2000)
                && source.observed_at_ns.get() > 0,
            "SPOT_FEE_SOURCE_IDENTITY_REQUIRED"
        );
        match source.status {
            SpotFeeEvidenceStatusV1::DataBacked => {
                bail!("SPOT_FEE_DATA_BACKED_REQUIRES_INDEPENDENT_SOURCE_VERIFIER")
            }
            SpotFeeEvidenceStatusV1::ConservativeAssumption => {
                bail!("SPOT_FEE_CONSERVATIVE_REQUIRES_SOURCE_COVERAGE_VERIFIER")
            }
            SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability => {
                ensure!(allow_unverified_public_scenario, "SPOT_FEE_PUBLIC_SCENARIO_POLICY_OPT_IN_REQUIRED");
                ensure!(source.source_uri == HYPERLIQUID_PUBLIC_FEE_SOURCE, "SPOT_FEE_PUBLIC_SOURCE_REQUIRED");
            }
            SpotFeeEvidenceStatusV1::Synthetic => {}
        }
        ensure!(
            !instruments.is_empty() && instruments.len() <= 256,
            "SPOT_FEE_INSTRUMENT_SCOPE_INVALID"
        );
        let mut admitted = BTreeMap::new();
        for instrument in instruments {
            let pair = spot_pair(instrument)?;
            ensure!(
                admitted
                    .insert(pair.id.to_string(), (pair.base_currency, pair.quote_currency))
                    .is_none(),
                "SPOT_FEE_DUPLICATE_INSTRUMENT"
            );
        }
        ensure!(
            schedule.rules.len() == admitted.len() * 4,
            "SPOT_FEE_COMPLETE_FOUR_WAY_COVERAGE_REQUIRED"
        );
        let mut rules = BTreeMap::new();
        for rule in &schedule.rules {
            let (base, quote) = admitted
                .get(&rule.instrument_id)
                .copied()
                .context("SPOT_FEE_RULE_OUTSIDE_INSTRUMENT_SCOPE")?;
            let currency = rule_currency(rule, base, quote)?;
            ensure!(
                rule.currency_code == currency.code.as_str()
                    && rule.currency_precision == currency.precision,
                "SPOT_FEE_CURRENCY_DEFINITION_MISMATCH"
            );
            let rate = rule.rate.as_decimal();
            ensure!(rate.abs() < BigDecimal::from(1), "SPOT_FEE_RATE_OUT_OF_RANGE");
            if source.status == SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability {
                // The inspected table's tier-0 rate is a scenario input, NOT a
                // conservative historical bound or a verified personal fee.
                let public_rate = match rule.liquidity {
                    SpotFeeLiquidityV1::Maker => BigDecimal::new(4.into(), 4),
                    SpotFeeLiquidityV1::Taker => BigDecimal::new(7.into(), 4),
                };
                ensure!(rate == &public_rate, "SPOT_FEE_PUBLIC_SCENARIO_RATE_MISMATCH");
                ensure!(
                    matches!(
                        (rule.order_side, rule.basis, rule.fee_currency),
                        (SpotFeeOrderSideV1::Buy, SpotFeeBasisV1::BaseQuantity, SpotFeeCurrencyV1::Base)
                            | (SpotFeeOrderSideV1::Sell, SpotFeeBasisV1::QuoteNotional, SpotFeeCurrencyV1::Quote)
                    ),
                    "SPOT_FEE_UNSUPPORTED_PUBLIC_SCENARIO_CURRENCY"
                );
            }
            ensure!(
                rules
                    .insert((rule.instrument_id.clone(), rule.order_side, rule.liquidity), rule.clone())
                    .is_none(),
                "SPOT_FEE_DUPLICATE_RULE"
            );
        }
        // Each key has only two allowed sides and two liquidity variants, so
        // exact cardinality plus unique in-scope keys proves complete coverage.
        Ok(Self { schedule, instruments: admitted, rules })
    }

    pub fn schedule(&self) -> &FrozenSpotFeeScheduleV1 {
        &self.schedule
    }

    pub fn into_handle(self) -> FeeModelHandle {
        FeeModelHandle::new(self)
    }
}

impl FeeModel for FrozenSpotFeeModel {
    fn get_commission(
        &self,
        order: &OrderAny,
        fill_quantity: Quantity,
        fill_px: Price,
        instrument: &InstrumentAny,
    ) -> Result<Money> {
        let pair = spot_pair(instrument)?;
        ensure!(order.instrument_id() == pair.id, "SPOT_FEE_ORDER_INSTRUMENT_MISMATCH");
        ensure!(!order.is_quote_quantity(), "SPOT_FEE_QUOTE_QUANTITY_UNSUPPORTED");
        let admitted = self.instruments.get(&pair.id.to_string())
            .context("SPOT_FEE_INSTRUMENT_NOT_ADMITTED")?;
        ensure!(
            *admitted == (pair.base_currency, pair.quote_currency)
                && admitted.0.precision == pair.base_currency.precision
                && admitted.1.precision == pair.quote_currency.precision,
            "SPOT_FEE_NATIVE_CURRENCY_CHANGED"
        );
        let side = match order.order_side() {
            OrderSide::Buy => SpotFeeOrderSideV1::Buy,
            OrderSide::Sell => SpotFeeOrderSideV1::Sell,
            _ => bail!("SPOT_FEE_ORDER_SIDE_REQUIRED"),
        };
        let liquidity = match order.liquidity_side() {
            Some(LiquiditySide::Maker) => SpotFeeLiquidityV1::Maker,
            Some(LiquiditySide::Taker) => SpotFeeLiquidityV1::Taker,
            _ => bail!("SPOT_FEE_LIQUIDITY_SIDE_REQUIRED"),
        };
        let rule = self.rules.get(&(pair.id.to_string(), side, liquidity))
            .context("SPOT_FEE_RULE_MISSING")?;
        ensure!(
            fill_quantity.as_decimal() > Decimal::ZERO && fill_px.as_decimal() > Decimal::ZERO,
            "SPOT_FEE_POSITIVE_FILL_REQUIRED"
        );
        let quantity = BigDecimal::from_str(&fill_quantity.as_decimal().to_string())?;
        let basis = match rule.basis {
            SpotFeeBasisV1::BaseQuantity => quantity,
            SpotFeeBasisV1::QuoteNotional => {
                quantity * BigDecimal::from_str(&fill_px.as_decimal().to_string())?
            }
        };
        let currency = rule_currency(rule, pair.base_currency, pair.quote_currency)?;
        let amount = basis * rule.rate.as_decimal();
        let rounded = match rule.rounding {
            SpotFeeRoundingV1::HalfEvenCurrencyPrecision =>
                amount.with_scale_round(i64::from(currency.precision), RoundingMode::HalfEven),
        };
        // Exact decimal until one declared currency-precision rounding. Money is
        // the public native conversion/range boundary; no f64 conversion or clamp.
        let decimal = Decimal::from_str_exact(&rounded.to_plain_string())
            .context("SPOT_FEE_AMOUNT_OUT_OF_NATIVE_DECIMAL_RANGE")?;
        Money::from_decimal(decimal, currency).map_err(Into::into)
    }
}

fn spot_pair(instrument: &InstrumentAny) -> Result<&CurrencyPair> {
    let InstrumentAny::CurrencyPair(pair) = instrument else {
        bail!("SPOT_FEE_CURRENCY_PAIR_REQUIRED")
    };
    ensure!(
        pair.id.to_string().ends_with("-SPOT.HYPERLIQUID")
            && !pair.id.symbol.as_str().contains(':')
            && pair.base_currency != pair.quote_currency
            && pair.multiplier.as_decimal() == Decimal::ONE,
        "SPOT_FEE_UNSUPPORTED_INSTRUMENT"
    );
    Ok(pair)
}

fn rule_currency(rule: &FrozenSpotFeeRuleV1, base: Currency, quote: Currency) -> Result<Currency> {
    match (rule.basis, rule.fee_currency) {
        (SpotFeeBasisV1::BaseQuantity, SpotFeeCurrencyV1::Base) => Ok(base),
        (SpotFeeBasisV1::QuoteNotional, SpotFeeCurrencyV1::Quote) => Ok(quote),
        _ => bail!("SPOT_FEE_BASIS_CURRENCY_MISMATCH"),
    }
}

fn bounded_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.trim() == value && value.len() <= max
}

#[cfg(test)]
mod tests;
