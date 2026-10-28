//! Controlled local replay regressions, never historical market or performance evidence.
use super::*;
use nautilus_model::{
    data::TradeTick,
    enums::{AggressorSide, CurrencyType},
    identifiers::{InstrumentId, Symbol, TradeId},
    instruments::{CryptoPerpetual, CurrencyPair},
    types::{Price, Quantity},
};
use serde_json::json;
use std::path::PathBuf;

const SECOND: u64 = 1_000_000_000;
const BRIDGE: &str = "USDT0-USDC-SPOT.HYPERLIQUID";
const SPOT: &str = "BTC-USDC-SPOT.HYPERLIQUID";
const LATE: &str = "ETH-USD-PERP.HYPERLIQUID";

fn definitions() -> Vec<InstrumentAny> {
    let usdt0 = Currency::try_from_str("USDT0").unwrap_or_else(|| {
        let currency = Currency::new("USDT0", 8, 0, "USDT0", CurrencyType::Crypto);
        Currency::register(currency, false).unwrap();
        currency
    });
    let pair = |id: &str, base: Currency, precision, increment: &str| {
        InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from(id))
                .raw_symbol(Symbol::new("UNIT-ONLY"))
                .base_currency(base)
                .quote_currency(Currency::USDC())
                .price_precision(precision)
                .size_precision(4)
                .price_increment(Price::from(increment))
                .size_increment(Quantity::from("0.0001"))
                .min_notional(Money::from("10 USDC"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        )
    };
    vec![
        pair(BRIDGE, usdt0, 3, "0.001"),
        pair(SPOT, Currency::BTC(), 2, "0.01"),
        InstrumentAny::CryptoPerpetual(
            CryptoPerpetual::builder()
                .instrument_id(InstrumentId::from(LATE))
                .raw_symbol(Symbol::new("UNIT-ONLY-LATE"))
                .base_currency(Currency::ETH())
                .quote_currency(Currency::USD())
                .settlement_currency(Currency::USDC())
                .is_inverse(false)
                .price_precision(2)
                .size_precision(4)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.0001"))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        ),
    ]
}

fn source(
    directory: &Path,
    after_positive: i8,
    late_close_at: Option<u64>,
    include_four_hour_rate: bool,
) -> (PathBuf, NativeTickEconomicRequestV1, Vec<u8>) {
    let instruments = definitions();
    let mut request = super::tests::request();
    request.research.input.instrument_ids = vec![BRIDGE.into(), SPOT.into(), LATE.into()];
    request.research.input.event_end_ns = counter(3 * DAY).unwrap();
    request.research.input.decision_cutoff_ns = request.research.input.event_end_ns;
    request.research.train_end_ns = counter(DAY).unwrap();
    request.research.validation_end_ns = counter(2 * DAY).unwrap();
    request.scenario.participation_fraction = "1".parse().unwrap();
    request.scenario.maximum_asset_weight = "1".parse().unwrap();
    request.scenario.spot_taker_fee_bps = "0".parse().unwrap();
    request.scenario.perp_taker_fee_bps = "0".parse().unwrap();
    request.scenario.extra_cost_bps = "0".parse().unwrap();
    request.scenario.decision_interval_ns = counter(10 * SECOND).unwrap();
    request.scenario.base_latency_ns = counter(SECOND).unwrap();
    request.scenario.jitter_min_ns = counter(0).unwrap();
    request.scenario.jitter_max_ns = counter(0).unwrap();
    request.fx_instrument_ids = vec![BRIDGE.into()];
    let mut rows = Vec::new();
    let mut append = |id: &str, event: u64, price: &str, side: AggressorSide| {
        rows.push(TradeTick::new(
            InstrumentId::from(id),
            Price::from(price),
            Quantity::from("10000.0000"),
            side,
            TradeId::from(format!("UNIT-{}", rows.len()).as_str()),
            event.into(),
            event.into(),
        ));
    };
    for event in [
        10 * SECOND,
        20 * SECOND,
        30 * SECOND,
        40 * SECOND,
        50 * SECOND,
        60 * SECOND,
        DAY + 10 * SECOND,
        2 * DAY + 10 * SECOND,
    ] {
        append(BRIDGE, event, "1.003", AggressorSide::Buy);
        append(BRIDGE, event, "1.003", AggressorSide::Sell);
        // Bridge grids precede sparse spot events: replacing pending targets at every
        // intervening grid would postpone each attempt forever (lag is one second).
        let event = event + SECOND / 2;
        let price = if event < DAY { "100.00" } else { "200.00" };
        if event < 40 * SECOND || after_positive > 0 {
            append(SPOT, event, price, AggressorSide::Buy);
        } else if after_positive < 0 {
            append(SPOT, event, price, AggressorSide::Sell);
        } else {
            append(SPOT, event, price, AggressorSide::Buy);
            append(SPOT, event, price, AggressorSide::Sell);
        }
    }
    if let Some(close_at) = late_close_at {
        for offset in [0, 10, 20, 30, 40, 50] {
            append(
                LATE,
                3 * HOUR + 15 * 60 * SECOND + offset * SECOND,
                "100.00",
                AggressorSide::Buy,
            );
        }
        for offset in [0, 10, 20, 30] {
            let at = close_at + offset * SECOND;
            append(LATE, at, "100.00", AggressorSide::Buy);
            append(LATE, at, "100.00", AggressorSide::Sell);
        }
    } else {
        // Present in the full source, absent from the clipped research view.
        append(LATE, 2 * DAY + SECOND, "100.00", AggressorSide::Buy);
    }
    rows.sort_by_key(|row| (row.ts_event, row.instrument_id));
    let output = directory.join("source");
    let mut builder = cohort_cache::Builder::new_flow(&output, 100, 100, 1_000_000).unwrap();
    for row in rows {
        builder.push(row).unwrap();
    }
    let manifest = builder.seal("fixed-source", 0, 3 * DAY).unwrap();
    builder
        .publish(
            manifest,
            json!({"status":"IMPORTED","availability":"EVENT_TIME_PROXY_UNVERIFIED",
                "source_revision":"fixed-source","event_start_ns":"0","event_end_ns":(3 * DAY).to_string(),
                "missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0,
                "instrument_source":{"currencies":crate::catalog::currencies::definitions(&instruments),"instruments":instruments}}),
        )
        .unwrap();
    let catalog = output.join("catalog");
    let rate_times = if late_close_at.is_some() {
        (4..72)
            .filter(|hour| *hour != 4 || include_four_hour_rate)
            .map(|hour| hour * HOUR + 91_000_000)
            .collect::<Vec<_>>()
    } else {
        vec![2 * DAY + 2 * SECOND]
    };
    let rates = rate_times
        .iter()
        .map(|at| {
            json!({"instrument_id":LATE,"event_ns":at.to_string(),"rate":if late_close_at.is_some() && *at / HOUR == 4 {"0.001"} else {"0"}}).to_string() + "\n"
        })
        .collect::<String>();
    std::fs::write(catalog.join("funding-rates.jsonl"), rates).unwrap();
    std::fs::write(
        catalog.join("funding-rates.manifest.json"),
        serde_json::to_vec(&json!({"source_revision":"fixed-source","start_ns":"0",
            "end_ns":(3 * DAY).to_string(),"instruments":[LATE],"rows":rate_times.len(),"missing_instruments":[]}))
        .unwrap(),
    )
    .unwrap();
    let prepared = directory.join("prepared");
    cohort_cache::prepare_views(&catalog, &request.research, &prepared, 1_000_000).unwrap();
    (prepared, request, crate::ticks::tests::model(7))
}

#[test]
fn clipped_zero_cohort_asset_keeps_identity_and_unstarted_collateral_without_fake_events() {
    let directory = tempfile::tempdir().unwrap();
    let (prepared, mut request, wasm) = source(directory.path(), 0, None, true);
    request.research.clip_end(counter(2 * DAY).unwrap());
    let result = replay(&prepared.join("research/catalog"), &request, &wasm).unwrap();
    assert_eq!(result.coverage.instrument_ids.len(), 3);
    let late = result
        .assets
        .iter()
        .find(|asset| asset.instrument_id == LATE)
        .unwrap();
    assert_eq!(late.cohorts.get(), 0);
    assert_eq!(late.decisions.get(), 0);
    assert_eq!(late.execution_attempts.get(), 0);
    assert_eq!(result.days.len(), 2);
    assert_eq!(
        decimal(result.initial_usdc_after_cost.as_ref().unwrap()).unwrap(),
        Decimal::from(3009)
    );
    assert!(result.days.iter().all(|day| {
        day.equity_usdt
            .as_ref()
            .is_some_and(|equity| decimal(equity).unwrap() == Decimal::from(3000))
    }));
    assert_eq!(result.native_fills.get(), 2);
    assert_eq!(decimal(&result.funding_usdc).unwrap(), Decimal::ZERO);
}

#[test]
fn negative_or_zero_score_schedules_flat_then_native_future_fill_removes_later_price_exposure() {
    for score in [-1, 0] {
        let directory = tempfile::tempdir().unwrap();
        let (prepared, request, wasm) = source(directory.path(), score, None, true);
        let result = replay(&prepared.join("independent/catalog"), &request, &wasm).unwrap();
        assert_eq!(result.assets.len(), 3);
        assert_eq!(result.days.len(), 3);
        assert_eq!(
            result.native_fills.get(),
            2,
            "one delayed native buy and one delayed native close"
        );
        assert_eq!(result.native_rejections.get(), 0);
        if result.target_asof_ns.is_none() {
            assert!(result.targets.is_empty());
            assert!(result.target_cash_weight.is_none());
            assert!(result
                .limitations
                .iter()
                .any(|reason| reason
                    == "TARGET_SNAPSHOT_UNAVAILABLE_NO_COMPLETE_COMMON_DECISION_CLOCK"));
        }
        assert_eq!(decimal(&result.funding_usdc).unwrap(), Decimal::ZERO);
        assert_eq!(
            decimal(&result.initial_conversion_cost_usdc).unwrap(),
            Decimal::ZERO
        );
        assert!(result
            .targets
            .iter()
            .all(|target| decimal(&target.weight).unwrap().is_zero()));
        assert!(
            result.days.iter().all(|day| {
                day.equity_usdt
                    .as_ref()
                    .is_some_and(|equity| decimal(equity).unwrap() == Decimal::from(3000))
            }),
            "the price doubled only after the score had flattened at a later cohort"
        );
    }
}

#[test]
fn independent_context_funds_at_oracle_values_perp_at_mark_and_keeps_trade_price() {
    let directory = tempfile::tempdir().unwrap();
    let (prepared, request, _) = source(directory.path(), 0, None, true);
    let original = directory.path().join("source/catalog");
    let csv = format!(
        "event_ns,oracle_px,mark_px\n{},50,150\n{},60,160\n",
        HOUR,
        2 * HOUR
    );
    std::fs::write(original.join("context-prices-000.csv"), &csv).unwrap();
    let manifest = json!({"schema_version":1,"format":"EVENT_TIME_CONTEXT_PRICE_CSV_V1","clock":"EVENT_TIME_PROXY_UNVERIFIED",
        "source_revision":"independent-observations","research_source_revision":"fixed-source","start_ns":"0","end_ns":(3*DAY).to_string(),
        "rows":"2","instrument_ids":[LATE],"files":[{"instrument_id":LATE,"path":"context-prices-000.csv","bytes":csv.len().to_string(),
            "rows":"2","first_event_ns":HOUR.to_string(),"last_event_ns":(2*HOUR).to_string()}],"missing_dates":["observational-gap"]});
    std::fs::write(
        original.join(CONTEXT_MANIFEST),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let context_prepared = directory.path().join("context-prepared");
    let metadata =
        cohort_cache::prepare_views(&original, &request.research, &context_prepared, 1_000_000)
            .unwrap();
    assert_eq!(metadata["economic_sources"].as_array().unwrap().len(), 4);
    let context_root = context_prepared.join("research/catalog");
    let mut unrelated = request.research.clone();
    unrelated.input.instrument_ids.retain(|id| id != LATE);
    assert!(context_manifest(&context_root, &unrelated).is_err());
    assert_eq!(
        std::fs::read(context_root.join("context-prices-000.csv")).unwrap(),
        csv.as_bytes()
    );
    assert_eq!(
        std::fs::read(context_prepared.join("independent/catalog/context-prices-000.csv")).unwrap(),
        csv.as_bytes()
    );
    let cache =
        cohort_cache::load(&prepared.join("independent/catalog"), &request.research).unwrap();
    let instrument = definitions()
        .into_iter()
        .find(|i| i.id().to_string() == LATE)
        .unwrap();
    let mut assets = vec![Asset {
        instrument: instrument.clone(),
        reader: cache
            .cohorts(
                cache
                    .assets
                    .iter()
                    .find(|a| a.instrument_id == LATE)
                    .unwrap(),
            )
            .unwrap(),
        head: None,
        previous: None,
        fast: ExponentialMovingAverage::new(1, None),
        slow: ExponentialMovingAverage::new(2, None),
        model: None,
        score: None,
        mark: Some((0, Decimal::from(100))),
        cohorts: 0,
        decisions: 0,
        attempts: 0,
        pending: None,
        target_weight: None,
    }];
    let mut margin = Some(build(&assets, true, Decimal::from(1000), &request.scenario).unwrap());
    margin
        .as_mut()
        .unwrap()
        .step(SECOND, &instrument, 100.0, 10.0, None)
        .unwrap();
    margin
        .as_mut()
        .unwrap()
        .step(2 * SECOND, &instrument, 100.0, 10.0, Some(Decimal::ONE))
        .unwrap();
    assert_eq!(margin.as_ref().unwrap().positions().len(), 1);
    let at = HOUR + 30_000_000;
    let mut none = None;
    assert!(value(
        margin.as_ref().unwrap(),
        &assets,
        &BTreeSet::new(),
        at,
        HOUR,
        true,
        &mut none,
        None
    )
    .unwrap()
    .is_none());
    let mut contexts = Some(ContextPrices::open(&context_root, &request.research).unwrap());
    let before = value(
        margin.as_ref().unwrap(),
        &assets,
        &BTreeSet::new(),
        at,
        HOUR,
        true,
        &mut contexts,
        None,
    )
    .unwrap()
    .unwrap();
    let rates = original.join("actual-rate.jsonl");
    std::fs::write(
        &rates,
        format!(
            "{}\n",
            json!({"instrument_id":LATE,"event_ns":at.to_string(),"rate":"0.01"})
        ),
    )
    .unwrap();
    let mut funding = Funding::open(&rates, BTreeSet::from([LATE.into()])).unwrap();
    let mut unknown = None;
    let batch = rates_at(
        &mut funding,
        &mut margin,
        &assets,
        at,
        0,
        HOUR,
        &mut unknown,
        &mut BTreeSet::new(),
        &mut contexts,
    )
    .unwrap();
    assert!(unknown.is_none());
    margin.as_mut().unwrap().funding_batch(at, &batch).unwrap();
    assert_eq!(
        margin.as_ref().unwrap().summary().adjustment_totals["USDC"],
        Decimal::new(-5, 1)
    );
    let after = value(
        margin.as_ref().unwrap(),
        &assets,
        &BTreeSet::new(),
        at,
        HOUR,
        true,
        &mut contexts,
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(before - after, Decimal::new(5, 1));
    assert!(
        before > Decimal::from(1040),
        "NAV uses the observed 150 mark, not the stale 100 trade or 50 oracle"
    );
    assert_eq!(assets[0].mark, Some((0, Decimal::from(100))));
    let stale = 3 * HOUR + 1;
    assets[0].mark = Some((stale, Decimal::from(100)));
    let mut reason = String::new();
    assert!(
        value(
            margin.as_ref().unwrap(),
            &assets,
            &BTreeSet::new(),
            stale,
            HOUR,
            true,
            &mut contexts,
            Some(&mut reason)
        )
        .unwrap()
        .is_none(),
        "explicit context cannot fall back to a fresh trade when context expires"
    );
    assert!(reason.starts_with("PERP_MARK_UNAVAILABLE:quantity="));
    assert!(reason.contains(&format!(
        "source=CONTEXT_MARK:instrument_id={LATE}:last_ns={}",
        2 * HOUR
    )));
    assert!(reason.contains("state=STALE"));
    contexts.as_mut().unwrap().finish().unwrap();
    margin.as_mut().unwrap().finish().unwrap();
}

#[test]
fn daily_reason_identifies_stale_nonzero_spot_balance_but_not_zero_balance() {
    let directory = tempfile::tempdir().unwrap();
    let (prepared, mut request, _) = source(directory.path(), 0, None, true);
    request.scenario.maximum_mark_age_ns = counter(HOUR).unwrap();
    let cache =
        cohort_cache::load(&prepared.join("independent/catalog"), &request.research).unwrap();
    let at = HOUR + 2 * SECOND + 1;
    let mut assets: Vec<_> = definitions()
        .into_iter()
        .map(|instrument| {
            let id = instrument.id().to_string();
            Asset {
                instrument,
                reader: cache
                    .cohorts(
                        cache
                            .assets
                            .iter()
                            .find(|asset| asset.instrument_id == id)
                            .unwrap(),
                    )
                    .unwrap(),
                head: None,
                previous: None,
                fast: ExponentialMovingAverage::new(1, None),
                slow: ExponentialMovingAverage::new(2, None),
                model: None,
                score: None,
                mark: Some(if id == SPOT {
                    (2 * SECOND, Decimal::from(100))
                } else {
                    (at, Decimal::new(1003, 3))
                }),
                cohorts: 0,
                decisions: 0,
                attempts: 0,
                pending: None,
                target_weight: None,
            }
        })
        .collect();
    let instrument = assets
        .iter()
        .find(|asset| asset.instrument.id().to_string() == SPOT)
        .unwrap()
        .instrument
        .clone();
    let fx_ids = BTreeSet::from([BRIDGE.into()]);
    let mut cash = Some(build(&assets, false, Decimal::from(1000), &request.scenario).unwrap());
    cash.as_mut()
        .unwrap()
        .step(SECOND, &instrument, 100.0, 10.0, None)
        .unwrap();
    cash.as_mut()
        .unwrap()
        .step(2 * SECOND, &instrument, 100.0, 10.0, Some(Decimal::ONE))
        .unwrap();
    assert!(
        cash.as_ref()
            .unwrap()
            .balance(Currency::BTC())
            .unwrap()
            .as_decimal()
            > Decimal::ZERO
    );
    let margin = Some(build(&assets, true, Decimal::from(1000), &request.scenario).unwrap());
    let mut days = Vec::new();
    let mut previous = Some(Decimal::from(2000));
    append_day(
        &mut days,
        0,
        at,
        &mut previous,
        &cash,
        &margin,
        &assets,
        &fx_ids,
        &request.scenario,
        Decimal::ZERO,
        Decimal::from(2000),
        Decimal::ZERO,
        false,
        &mut None,
    )
    .unwrap();
    assert!(days[0].equity_usdt.is_none());
    assert!(days[0].net_return.is_none());
    let reason = days[0].unknown_reason.as_ref().unwrap();
    assert!(reason.starts_with("BALANCE_FX_UNAVAILABLE:ledger=CASH:amount="));
    assert!(reason.contains(&format!(
        "currency=BTC:source=COHORT_MARK:instrument_id={SPOT}:last_ns={}",
        2 * SECOND
    )));
    assert!(reason.contains(&format!("at_ns={at}:maximum_age_ns={HOUR}:state=STALE")));
    assert!(previous.is_none());
    let zero_cash = Some(build(&assets, false, Decimal::from(1000), &request.scenario).unwrap());
    append_day(
        &mut days,
        DAY,
        at,
        &mut previous,
        &zero_cash,
        &margin,
        &assets,
        &fx_ids,
        &request.scenario,
        Decimal::ZERO,
        Decimal::from(2000),
        Decimal::ZERO,
        false,
        &mut None,
    )
    .unwrap();
    assert!(days[1].equity_usdt.is_some());
    assert!(days[1].unknown_reason.is_none());
    assets
        .iter_mut()
        .find(|asset| asset.instrument.id().to_string() == SPOT)
        .unwrap()
        .mark = Some((at, Decimal::from(100)));
    append_day(
        &mut days,
        2 * DAY,
        at,
        &mut previous,
        &cash,
        &margin,
        &assets,
        &fx_ids,
        &request.scenario,
        Decimal::ZERO,
        Decimal::from(2000),
        Decimal::ZERO,
        false,
        &mut None,
    )
    .unwrap();
    assert!(days[2].equity_usdt.is_some());
    assert!(
        days[2].unknown_reason.is_none(),
        "valuation failures are not sticky"
    );
}

#[test]
fn funding_is_expected_only_for_exposure_at_the_hour_boundary() {
    for (close_hour, include_rate, missing_hour) in
        [(4, true, None), (4, false, Some(4)), (3, false, None)]
    {
        let directory = tempfile::tempdir().unwrap();
        let close_at = close_hour * HOUR + 45 * 60 * SECOND;
        let (prepared, request, wasm) = source(directory.path(), 0, Some(close_at), include_rate);
        let result = replay(&prepared.join("independent/catalog"), &request, &wasm).unwrap();
        assert_eq!(
            result.native_fills.get(),
            4,
            "actual native spot and perp open/close fills"
        );
        let cause = result
            .limitations
            .iter()
            .find(|reason| reason.starts_with("FUNDING_FIRST_CAUSE:"));
        if let Some(hour) = missing_hour {
            assert_eq!(cause.unwrap(), &format!("FUNDING_FIRST_CAUSE:MISSING_RATE:event_ns={}:instrument_id={LATE}:rate_hour={hour}", hour * HOUR));
            assert!(result.days.iter().all(|day| day.equity_usdt.is_none()));
        } else {
            assert!(cause.is_none());
            assert!(result.days.iter().all(|day| day.equity_usdt.is_some()));
            if close_hour == 4 {
                assert!(decimal(&result.funding_usdc).unwrap() < Decimal::ZERO);
            } else {
                assert_eq!(decimal(&result.funding_usdc).unwrap(), Decimal::ZERO);
            }
        }
    }
}
