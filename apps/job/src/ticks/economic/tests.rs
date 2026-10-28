use super::*;
use serde_json::json;

pub(super) fn request() -> NativeTickEconomicRequestV1 {
    let mut research = crate::ticks::tests::request();
    research.input.representation =
        Some(contracts::ticks::NativeTickRepresentationV1::EventTimeCohortFlowCacheV1);
    research.input.maximum_rows = None;
    research.input.maximum_source_rows = Some(counter(100).unwrap());
    research.input.event_end_ns = counter(5 * DAY).unwrap();
    research.input.decision_cutoff_ns = research.input.event_end_ns;
    research.train_end_ns = counter(3 * DAY).unwrap();
    research.validation_end_ns = counter(4 * DAY).unwrap();
    serde_json::from_value(json!({"schema_version":1,"research":research,
        "scenario":{"initial_usdt":"3000","usdt0_per_usdt":"1","spot_fraction":"0.5",
            "gross_fraction":"0.5","maximum_asset_weight":"0.1","participation_fraction":"0.01",
            "perp_taker_fee_bps":"4.5","spot_taker_fee_bps":"7","extra_cost_bps":"10","initial_margin_fraction":"0.5","maintenance_margin_fraction":"0.25",
            "decision_interval_ns":"60000000000","base_latency_ns":"1000000000","jitter_min_ns":"100000","jitter_max_ns":"1500000",
            "random_seed":"0","maximum_mark_age_ns":"172800000000000"},
        "fx_instrument_ids":["AUD/USD.SIM"],"funding_source":"funding-rates.jsonl"})).unwrap()
}

#[test]
fn registered_latency_seed_is_reproducible_and_absolute_lag_is_not_jitter() {
    let request = request();
    validate(&request).unwrap();
    let (mut a, mut b) = (0, 0);
    for _ in 0..100 {
        let lag = delay(&mut a, &request.scenario).unwrap();
        assert_eq!(lag, delay(&mut b, &request.scenario).unwrap());
        assert!((1_000_100_000..=1_001_500_000).contains(&lag));
    }
    let mut invalid = request.clone();
    invalid.scenario.spot_fraction = "1".parse().unwrap();
    assert!(validate(&invalid).is_err());
    invalid = request.clone();
    invalid.research.validation_end_ns = counter(4 * DAY + DAY / 2).unwrap();
    assert!(invalid.research.evaluation_regimes.is_none());
    assert_eq!(
        validate(&invalid).unwrap_err().to_string(),
        "ECONOMIC_UTC_DAILY_BOUNDARIES_REQUIRED"
    );
}

#[test]
fn actual_rate_stream_rejects_repeated_clock_and_does_not_erase_submillisecond_offset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rates.jsonl");
    let line = "{\"instrument_id\":\"BTC-USD-PERP.HYPERLIQUID\",\"event_ns\":\"1753660800002000000\",\"rate\":\"0.0000238727\"}\n";
    std::fs::write(&path, format!("{line}{line}")).unwrap();
    let mut rates =
        Funding::open(&path, BTreeSet::from(["BTC-USD-PERP.HYPERLIQUID".into()])).unwrap();
    assert_eq!(
        rates.next.as_ref().unwrap().event_ns.get(),
        1_753_660_800_002_000_000
    );
    assert_eq!(
        rates.advance().unwrap_err().to_string(),
        "ECONOMIC_FUNDING_ORDER_INVALID"
    );
}

#[test]
fn missing_exposed_funding_hour_is_unknown_but_zero_rate_observation_is_present() {
    let mut expected = BTreeSet::from([("BTC".into(), 0), ("BTC".into(), 1)]);
    let mut seen = BTreeSet::from([("BTC".into(), 0)]);
    assert!(!missing_rates_before(HOUR, &mut expected, &mut seen));
    assert!(missing_rates_before(2 * HOUR, &mut expected, &mut seen));
    assert!(expected.is_empty() && seen.is_empty());
}

#[test]
fn daily_net_metrics_keep_unavailable_days_and_do_not_annualize_noncontiguous_elapsed_time() {
    use contracts::{factors::NativeFactorPartitionV1, ticks::NativeTickRegimeV1};
    let request = request();
    let days = vec![
        NativeTickEconomicDayV1 {
            day_start_ns: counter(0).unwrap(),
            equity_usdt: None,
            net_return: Some(0.01),
            unknown_reason: None,
        },
        NativeTickEconomicDayV1 {
            day_start_ns: counter(DAY).unwrap(),
            equity_usdt: None,
            net_return: None,
            unknown_reason: Some("MISSING_MARK".into()),
        },
        NativeTickEconomicDayV1 {
            day_start_ns: counter(2 * DAY).unwrap(),
            equity_usdt: None,
            net_return: Some(-0.01),
            unknown_reason: None,
        },
    ];
    let reports = metrics(&request, &days).unwrap();
    let train = reports
        .iter()
        .find(|r| {
            r.partition == NativeFactorPartitionV1::Train && r.regime == NativeTickRegimeV1::Unknown
        })
        .unwrap();
    assert_eq!(train.days.get(), 3);
    assert_eq!(train.valid_return_days.get(), 2);
    assert_eq!(train.sharpe_365, Some(0.0));
    assert!(train.conditional_annualized_return_365.unwrap() < 0.0);
}

#[test]
fn bridge_conversion_respects_native_lot_and_minimum_without_spending_dust() {
    use nautilus_model::{
        identifiers::{InstrumentId, Symbol},
        instruments::CurrencyPair,
        types::{Price, Quantity},
    };
    let bridge = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from("USDT-USDC.HYPERLIQUID"))
            .raw_symbol(Symbol::new("@test-fx"))
            .base_currency(Currency::USDT())
            .quote_currency(Currency::USDC())
            .price_precision(4)
            .size_precision(2)
            .price_increment(Price::from("0.0001"))
            .size_increment(Quantity::from("0.01"))
            .min_notional(Money::from("10 USDC"))
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    assert_eq!(
        conversion_quantity(
            Decimal::from(3000),
            Decimal::new(1, 3),
            Decimal::ONE,
            &bridge
        )
        .unwrap(),
        Decimal::ZERO
    );
    let spent = conversion_quantity(
        Decimal::from(3000),
        Decimal::new(20555, 3),
        Decimal::ONE,
        &bridge,
    )
    .unwrap();
    assert_eq!(spent, Decimal::new(2055, 2));
    let dust = Decimal::new(1, 3);
    assert_eq!(
        conversion_quantity(dust, dust, Decimal::ONE, &bridge).unwrap(),
        Decimal::ZERO
    );
    assert_eq!(Decimal::from(3000) - spent, Decimal::new(297945, 2));
}

#[test]
fn optional_funding_pair_is_bound_to_source_window_before_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let request = request();
    assert!(funding_sources(dir.path(), &request.research)
        .unwrap()
        .is_empty());
    std::fs::write(dir.path().join("funding-rates.jsonl"), "").unwrap();
    assert!(funding_sources(dir.path(), &request.research).is_err());
    let mut manifest = json!({"source_revision":request.research.input.source_revision,"start_ns":"0","end_ns":request.research.input.event_end_ns,"instruments":[],"rows":0,"missing_instruments":[]});
    let path = dir.path().join("funding-rates.manifest.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(
        funding_sources(dir.path(), &request.research)
            .unwrap()
            .len(),
        2
    );
    manifest["end_ns"] = json!("1");
    std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(funding_sources(dir.path(), &request.research).is_err());
}

#[test]
fn derived_wire_values_round_only_at_output_boundary() {
    let value = Decimal::ONE / Decimal::from(3);
    assert_eq!(
        exact(value).unwrap().as_decimal().to_plain_string(),
        "0.333333333333333333"
    );
    let fx = Decimal::from_str("1.003").unwrap();
    let wealth = Decimal::from(3000) / fx;
    let serialized = exact(wealth).unwrap();
    assert!((decimal(&serialized).unwrap() - wealth).abs() <= Decimal::new(5, 19));
}
