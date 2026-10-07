use super::*;
use contracts::{spot_fees::FrozenSpotFeeSourceV1, DbCounter, SchemaV1};
use nautilus_core::UnixNanos;
use nautilus_execution::models::fee::MakerTakerFeeModel;
use nautilus_model::{
    enums::OrderType,
    identifiers::{InstrumentId, Symbol},
    orders::OrderTestBuilder,
};

const ID: &str = "BTC-USDC-SPOT.HYPERLIQUID";

fn pair() -> InstrumentAny {
    InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from(ID))
            .raw_symbol(Symbol::new("@142"))
            .base_currency(Currency::BTC())
            .quote_currency(Currency::USDC())
            .price_precision(8)
            .size_precision(8)
            .price_increment(Price::from("0.00000001"))
            .size_increment(Quantity::from("0.00000001"))
            .ts_event(UnixNanos::from(1))
            .ts_init(UnixNanos::from(1))
            .build()
            .unwrap(),
    )
}

fn schedule() -> FrozenSpotFeeScheduleV1 {
    let mut rules = Vec::new();
    for order_side in [SpotFeeOrderSideV1::Buy, SpotFeeOrderSideV1::Sell] {
        for liquidity in [SpotFeeLiquidityV1::Maker, SpotFeeLiquidityV1::Taker] {
            let (basis, fee_currency, currency) = match order_side {
                SpotFeeOrderSideV1::Buy => (
                    SpotFeeBasisV1::BaseQuantity, SpotFeeCurrencyV1::Base, Currency::BTC(),
                ),
                SpotFeeOrderSideV1::Sell => (
                    SpotFeeBasisV1::QuoteNotional, SpotFeeCurrencyV1::Quote, Currency::USDC(),
                ),
            };
            rules.push(FrozenSpotFeeRuleV1 {
                instrument_id: ID.into(), order_side, liquidity,
                rate: match liquidity {
                    SpotFeeLiquidityV1::Maker => "0.0004",
                    SpotFeeLiquidityV1::Taker => "0.0007",
                }.parse().unwrap(),
                basis, fee_currency, currency_code: currency.code.to_string(),
                currency_precision: currency.precision,
                rounding: SpotFeeRoundingV1::HalfEvenCurrencyPrecision,
            });
        }
    }
    FrozenSpotFeeScheduleV1 {
        schema_version: SchemaV1,
        source: FrozenSpotFeeSourceV1 {
            status: SpotFeeEvidenceStatusV1::Synthetic,
            source_ref: "official-public-table-observation-20261006".into(),
            source_uri: HYPERLIQUID_PUBLIC_FEE_SOURCE.into(),
            observed_at_ns: DbCounter::new(9).unwrap(),
            assumptions: "Research assumption only: no account tier, discount, rebate, builder surcharge or historical rounding evidence; Buy base/Sell quote and half-even rounding explicitly assumed".into(),
        },
        valid_from_ns: DbCounter::new(10).unwrap(),
        valid_until_ns: DbCounter::new(20).unwrap(),
        rules,
    }
}

fn model(spec: FrozenSpotFeeScheduleV1) -> Result<FrozenSpotFeeModel> {
    FrozenSpotFeeModel::try_new(spec, &[pair()], 10, 20)
}

fn order(side: OrderSide, liquidity: LiquiditySide) -> OrderAny {
    let mut value = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from(ID))
        .side(side)
        .quantity(Quantity::from("10"))
        // Deliberately outside the fee window: this is not the fill timestamp.
        .ts_init(UnixNanos::from(999))
        .build();
    value.set_liquidity_side(liquidity);
    value
}

fn fee(model: &impl FeeModel, side: OrderSide, liquidity: LiquiditySide, qty: &str, px: &str) -> Result<Money> {
    model.get_commission(&order(side, liquidity), Quantity::from(qty), Price::from(px), &pair())
}

#[test]
fn official_handle_uses_all_four_frozen_rules_without_changing_instruments() {
    let native = pair();
    let before = serde_json::to_value(&native).unwrap();
    let adapter = FrozenSpotFeeModel::try_new(schedule(), &[native.clone()], 10, 20).unwrap();
    assert_eq!(adapter.schedule().source.status, SpotFeeEvidenceStatusV1::Synthetic);
    let handle = adapter.into_handle();
    for (side, liquidity, amount, currency) in [
        (OrderSide::Buy, LiquiditySide::Maker, "0.004", Currency::BTC()),
        (OrderSide::Buy, LiquiditySide::Taker, "0.007", Currency::BTC()),
        (OrderSide::Sell, LiquiditySide::Maker, "8", Currency::USDC()),
        (OrderSide::Sell, LiquiditySide::Taker, "14", Currency::USDC()),
    ] {
        let result = fee(&handle, side, liquidity, "10", "2000").unwrap();
        assert_eq!(result.as_decimal(), Decimal::from_str(amount).unwrap());
        assert_eq!(result.currency, currency);
    }
    assert_eq!(serde_json::to_value(&native).unwrap(), before);
    // Defaults remain untouched. They are demonstrably not this adapter's fees.
    assert_eq!(fee(&MakerTakerFeeModel, OrderSide::Buy, LiquiditySide::Taker, "10", "2000").unwrap().as_decimal(), Decimal::ZERO);
}

#[test]
fn synthetic_signed_rebate_preserves_currency_and_does_not_become_evidence() {
    let mut spec = schedule();
    spec.source.status = SpotFeeEvidenceStatusV1::Synthetic;
    spec.source.source_uri = "synthetic://signed-rebate-fixture".into();
    for rule in &mut spec.rules {
        if rule.liquidity == SpotFeeLiquidityV1::Maker {
            rule.rate = "-0.00001".parse().unwrap();
        }
    }
    let adapter = model(spec).unwrap();
    assert_eq!(adapter.schedule().source.status, SpotFeeEvidenceStatusV1::Synthetic);
    for (side, expected, currency) in [
        (OrderSide::Buy, "-0.0001", Currency::BTC()),
        (OrderSide::Sell, "-0.2", Currency::USDC()),
    ] {
        let result = fee(&adapter, side, LiquiditySide::Maker, "10", "2000").unwrap();
        assert_eq!(result.as_decimal(), Decimal::from_str(expected).unwrap());
        assert_eq!(result.currency, currency);
    }
}

#[test]
fn rounds_once_per_fill_at_native_currency_precision_with_signed_half_even() {
    for (rate, expected) in [("0.000000005", "0"), ("0.000000015", "0.00000002"),
        ("-0.000000005", "0"), ("-0.000000015", "-0.00000002")] {
        let mut spec = schedule();
        spec.source.status = SpotFeeEvidenceStatusV1::Synthetic;
        spec.rules[0].rate = rate.parse().unwrap();
        let result = fee(&model(spec).unwrap(), OrderSide::Buy, LiquiditySide::Maker, "1", "2000").unwrap();
        assert_eq!(result.as_decimal(), Decimal::from_str(expected).unwrap());
    }
}

#[test]
fn partial_fill_commission_uses_fill_quantity_not_order_quantity() {
    let adapter = model(schedule()).unwrap();
    let first = fee(&adapter, OrderSide::Sell, LiquiditySide::Taker, "1", "2000").unwrap();
    let next = fee(&adapter, OrderSide::Sell, LiquiditySide::Taker, "3", "2000").unwrap();
    assert_eq!(first.as_decimal(), Decimal::from_str("1.4").unwrap());
    assert_eq!(next.as_decimal(), Decimal::from_str("4.2").unwrap());
}

#[test]
fn incomplete_duplicate_unknown_rules_and_currency_mismatches_fail_closed() {
    let mut cases = Vec::new();
    let mut missing = schedule(); missing.rules.pop(); cases.push(missing);
    let mut duplicate = schedule(); duplicate.rules[3] = duplicate.rules[0].clone(); cases.push(duplicate);
    let mut unknown = schedule(); unknown.rules[0].instrument_id = "ETH-USDC-SPOT.HYPERLIQUID".into(); cases.push(unknown);
    let mut wrong_code = schedule(); wrong_code.rules[0].currency_code = "USDC".into(); cases.push(wrong_code);
    let mut wrong_precision = schedule(); wrong_precision.rules[0].currency_precision = 2; cases.push(wrong_precision);
    let mut wrong_basis = schedule(); wrong_basis.rules[0].basis = SpotFeeBasisV1::QuoteNotional; cases.push(wrong_basis);
    for spec in cases { assert!(model(spec).is_err()); }
    assert!(FrozenSpotFeeModel::try_new(schedule(), &[], 10, 20).is_err());
    assert!(FrozenSpotFeeModel::try_new(schedule(), &[pair(), pair()], 10, 20).is_err());
}

#[test]
fn missing_source_identity_and_unverified_evidence_claims_fail_closed() {
    let mut cases = Vec::new();
    let mut no_ref = schedule(); no_ref.source.source_ref.clear(); cases.push(no_ref);
    let mut no_observation = schedule(); no_observation.source.observed_at_ns = DbCounter::ZERO; cases.push(no_observation);
    let mut no_assumptions = schedule(); no_assumptions.source.assumptions.clear(); cases.push(no_assumptions);
    let mut settings_copy = schedule(); settings_copy.source.source_uri = "settings://fee_schedule_artifact".into(); settings_copy.source.status = SpotFeeEvidenceStatusV1::DataBacked; cases.push(settings_copy);
    let mut conservative = schedule(); conservative.source.status = SpotFeeEvidenceStatusV1::ConservativeAssumption; cases.push(conservative);
    let mut claimed = schedule(); claimed.source.status = SpotFeeEvidenceStatusV1::DataBacked; cases.push(claimed);
    for rate in ["1", "-1"] {
        let mut spec = schedule(); spec.rules[0].rate = rate.parse().unwrap(); cases.push(spec);
    }
    for spec in cases { assert!(model(spec).is_err()); }
}

#[test]
fn one_window_must_cover_whole_fill_horizon_including_latency() {
    for (start, end) in [(9, 20), (10, 21), (20, 21), (11, 11), (20, 10)] {
        assert!(FrozenSpotFeeModel::try_new(schedule(), &[pair()], start, end).is_err());
    }
    assert!(FrozenSpotFeeModel::try_new(schedule(), &[pair()], 11, 19).is_ok());
}

#[test]
fn missing_liquidity_quote_quantity_wrong_instrument_and_invalid_fill_fail_closed() {
    let adapter = model(schedule()).unwrap();
    assert!(fee(&adapter, OrderSide::Buy, LiquiditySide::NoLiquiditySide, "1", "2000").is_err());
    assert!(fee(&adapter, OrderSide::Buy, LiquiditySide::Taker, "0", "2000").is_err());
    assert!(fee(&adapter, OrderSide::Buy, LiquiditySide::Taker, "1", "0").is_err());
    assert!(fee(&adapter, OrderSide::Buy, LiquiditySide::Taker, "1", "-1").is_err());
    let mut quoted = order(OrderSide::Buy, LiquiditySide::Taker);
    quoted.set_is_quote_quantity(true);
    assert!(adapter.get_commission(&quoted, Quantity::from("1"), Price::from("2000"), &pair()).is_err());
    let InstrumentAny::CurrencyPair(mut other) = pair() else { unreachable!() };
    other.id = InstrumentId::from("ETH-USDC-SPOT.HYPERLIQUID");
    assert!(adapter.get_commission(&order(OrderSide::Buy, LiquiditySide::Taker), Quantity::from("1"), Price::from("2000"), &InstrumentAny::CurrencyPair(other)).is_err());
}

#[test]
fn native_money_overflow_returns_error_instead_of_clamping_or_zero_fee() {
    let mut spec = schedule();
    spec.source.status = SpotFeeEvidenceStatusV1::Synthetic;
    spec.rules[3].rate = "0.9".parse().unwrap();
    assert!(fee(&model(spec).unwrap(), OrderSide::Sell, LiquiditySide::Taker, "999999999", "999999999").is_err());
}

#[test]
fn strict_contract_rejects_dynamic_rules_unknown_rounding_and_float_rates() {
    let encoded = serde_json::to_value(schedule()).unwrap();
    let restored: FrozenSpotFeeScheduleV1 = serde_json::from_value(encoded.clone()).unwrap();
    assert!(model(restored).is_ok());
    for (key, value) in [
        ("fill_timestamp_ns", serde_json::json!("12")),
        ("rounding", serde_json::json!("EXCHANGE_VERIFIED")),
        ("fee_currency", serde_json::json!("THIRD_TOKEN")),
        ("rate", serde_json::json!(0.0007)),
    ] {
        let mut mutated = encoded.clone();
        mutated["rules"][0][key] = value;
        assert!(serde_json::from_value::<FrozenSpotFeeScheduleV1>(mutated).is_err());
    }
}

#[test]
fn quote_notional_keeps_more_than_native_decimal_scale_until_final_rounding() {
    let mut spec = schedule();
    spec.source.status = SpotFeeEvidenceStatusV1::Synthetic;
    spec.rules[3].rate = "0.123456789012345678".parse().unwrap();
    // Independently precomputed exact product; round at the official currency
    // definition, rather than assuming the venue token uses six native decimals.
    let result = fee(&model(spec).unwrap(), OrderSide::Sell, LiquiditySide::Taker,
        "1.00000001", "234.12345678").unwrap();
    let expected = BigDecimal::from_str("28.9041304955707942949477191365279684").unwrap()
        .with_scale_round(i64::from(Currency::USDC().precision), RoundingMode::HalfEven);
    assert_eq!(BigDecimal::from_str(&result.as_decimal().to_string()).unwrap(), expected);
}

#[test]
fn unsupported_native_instrument_shape_and_changed_base_currency_fail_closed() {
    for id in ["BTC-USDC-SPOT.BINANCE", "BTC-USD-PERP.HYPERLIQUID", "xyz:BTC-USDC-SPOT.HYPERLIQUID"] {
        let InstrumentAny::CurrencyPair(mut native) = pair() else { unreachable!() };
        native.id = InstrumentId::from(id);
        assert!(FrozenSpotFeeModel::try_new(schedule(), &[InstrumentAny::CurrencyPair(native)], 10, 20).is_err());
    }
    let InstrumentAny::CurrencyPair(mut scaled) = pair() else { unreachable!() };
    scaled.multiplier = Quantity::from("2");
    assert!(FrozenSpotFeeModel::try_new(schedule(), &[InstrumentAny::CurrencyPair(scaled)], 10, 20).is_err());
    let adapter = model(schedule()).unwrap();
    let InstrumentAny::CurrencyPair(mut changed) = pair() else { unreachable!() };
    changed.base_currency = Currency::ETH();
    assert!(adapter.get_commission(&order(OrderSide::Buy, LiquiditySide::Taker), Quantity::from("1"), Price::from("2000"), &InstrumentAny::CurrencyPair(changed)).is_err());
}


#[test]
fn current_public_table_cannot_self_declare_conservative_window_coverage() {
    let observed = 1_791_288_000_000_000_000;
    // Before, around, or after a real current observation. No timestamp relation
    // proves source coverage, so all fail without an independent verifier.
    for (start, end) in [
        (1_735_689_600_000_000_000, 1_735_776_000_000_000_000),
        (observed, observed + 1),
        (observed + 86_400_000_000_000, observed + 172_800_000_000_000),
    ] {
        let mut spec = schedule();
        spec.source.observed_at_ns = DbCounter::new(observed).unwrap();
        spec.valid_from_ns = DbCounter::new(start).unwrap();
        spec.valid_until_ns = DbCounter::new(end).unwrap();
        let scenario = FrozenSpotFeeModel::try_new(spec.clone(), &[pair()], start, end).unwrap();
        assert_eq!(scenario.schedule().source.status, SpotFeeEvidenceStatusV1::Synthetic);
        spec.source.status = SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability;
        assert!(FrozenSpotFeeModel::try_new(spec.clone(), &[pair()], start, end).is_err());
        let public = FrozenSpotFeeModel::try_new_public_rate_scenario(spec.clone(), &[pair()], start, end).unwrap();
        assert_eq!(public.schedule().source.status, SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability);
        spec.source.status = SpotFeeEvidenceStatusV1::ConservativeAssumption;
        let error = FrozenSpotFeeModel::try_new(spec, &[pair()], start, end).unwrap_err();
        assert!(error.to_string().contains("SOURCE_COVERAGE_VERIFIER"));
    }
}

#[test]
fn explicit_synthetic_zero_is_distinct_from_a_missing_fee_rule() {
    let mut spec = schedule();
    spec.rules[0].rate = "0".parse().unwrap();
    let scenario = model(spec.clone()).unwrap();
    assert_eq!(scenario.schedule().source.status, SpotFeeEvidenceStatusV1::Synthetic);
    assert_eq!(fee(&scenario, OrderSide::Buy, LiquiditySide::Maker, "1", "2000").unwrap().as_decimal(), Decimal::ZERO);
    spec.rules.remove(0);
    assert!(model(spec).is_err());
}


#[test]
fn current_public_rate_scenario_requires_explicit_policy_opt_in_and_preserves_status() {
    let mut spec = schedule();
    spec.source.status = SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability;
    assert!(model(spec.clone()).is_err());
    let scenario = FrozenSpotFeeModel::try_new_public_rate_scenario(spec, &[pair()], 10, 20).unwrap();
    let frozen = serde_json::to_value(scenario.schedule()).unwrap();
    assert_eq!(frozen["source"]["status"], "PUBLIC_RATE_SCENARIO_UNVERIFIED_APPLICABILITY");
    assert_eq!(fee(&scenario, OrderSide::Buy, LiquiditySide::Taker, "10", "2000").unwrap().as_decimal(), Decimal::from_str("0.007").unwrap());
    // The explicit public-rate entrypoint never silently admits another status.
    for status in [SpotFeeEvidenceStatusV1::Synthetic, SpotFeeEvidenceStatusV1::ConservativeAssumption, SpotFeeEvidenceStatusV1::DataBacked] {
        let mut wrong = schedule();
        wrong.source.status = status;
        assert!(FrozenSpotFeeModel::try_new_public_rate_scenario(wrong, &[pair()], 10, 20).is_err());
    }
}

#[test]
fn public_rate_scenario_rejects_unknown_discount_rebate_source_and_currency_rules() {
    let public = || {
        let mut spec = schedule();
        spec.source.status = SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability;
        spec
    };
    let mut cases = Vec::new();
    for rate in ["0", "0.00039", "-0.00001", "0.0005"] {
        let mut spec = public(); spec.rules[0].rate = rate.parse().unwrap(); cases.push(spec);
    }
    let mut source = public(); source.source.source_uri = "settings://fee_schedule_artifact".into(); cases.push(source);
    let mut quote_buy = public();
    quote_buy.rules[0].basis = SpotFeeBasisV1::QuoteNotional;
    quote_buy.rules[0].fee_currency = SpotFeeCurrencyV1::Quote;
    quote_buy.rules[0].currency_code = Currency::USDC().code.to_string();
    quote_buy.rules[0].currency_precision = Currency::USDC().precision;
    cases.push(quote_buy);
    for spec in cases {
        assert!(FrozenSpotFeeModel::try_new_public_rate_scenario(spec, &[pair()], 10, 20).is_err());
    }
}
