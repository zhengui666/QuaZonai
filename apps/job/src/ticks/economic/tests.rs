use super::*;
use serde_json::json;

fn request() -> NativeTickEconomicRequestV1 {
    let mut research = super::super::super::tests::request();
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
            "taker_fee_bps":"5","extra_cost_bps":"10","initial_margin_fraction":"0.5","maintenance_margin_fraction":"0.25",
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
