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
use std::{io::Write, path::PathBuf};

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
    let at = DAY - 1;
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
        None,
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
    let path = directory.path().join("spot-valuation.jsonl");
    let mut input = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    for clock in 1..=20_000 {
        writeln!(input, "{}", json!({"instrument_id": SPOT, "received_ns": clock.to_string(), "bid": "100", "ask": "102"})).unwrap();
    }
    for row in [
        json!({"instrument_id": SPOT, "received_ns": (at - 1).to_string(), "bid": "110", "ask": "112"}),
        json!({"instrument_id": SPOT, "received_ns": (DAY + 1).to_string(), "bid": "120", "ask": "122"}),
        json!({"instrument_id": SPOT, "received_ns": (DAY + 1).to_string(), "bid": "120", "ask": "122"}),
        json!({"instrument_id": SPOT, "received_ns": (2 * DAY + 1).to_string(), "bid": null, "ask": "122"}),
        json!({"instrument_id": SPOT, "received_ns": (3 * DAY + 1).to_string(), "bid": "123", "ask": "122"}),
        json!({"instrument_id": SPOT, "received_ns": (4 * DAY + 1).to_string(), "bid": "130", "ask": "132"}),
        json!({"instrument_id": SPOT, "received_ns": (7 * DAY).to_string(), "bid": "150", "ask": "152"}),
    ] {
        writeln!(input, "{row}").unwrap();
    }
    input.flush().unwrap();
    drop(input);
    let quotes = SpotQuotes::open(&path, &assets, 0, 7 * DAY).unwrap();
    let a = assets
        .iter()
        .find(|a| a.instrument.id().to_string() == SPOT)
        .unwrap();
    assert_eq!(
        quotes.0[SPOT].len(),
        5,
        "dense same-day history retains one row per day"
    );
    assert_eq!(quotes.price(a, at, HOUR), Some(Some(Decimal::from(111))));
    assert_eq!(
        quotes.price(a, 2 * DAY - 1, DAY),
        Some(None),
        "duplicate latest timestamps stay unknown"
    );
    assert_eq!(
        quotes.price(a, 3 * DAY - 1, DAY),
        Some(None),
        "missing bid stays unknown"
    );
    assert_eq!(
        quotes.price(a, 4 * DAY - 1, DAY),
        Some(None),
        "crossed quote stays unknown"
    );
    assert_eq!(
        quotes.price(a, 6 * DAY - 1, HOUR),
        Some(None),
        "stale quote is not reused"
    );
    let future = SpotQuotes(BTreeMap::from([(
        SPOT.into(),
        vec![(DAY + 1, Some(Decimal::ONE))],
    )]));
    assert!(
        future.price(a, DAY - 1, HOUR).is_none(),
        "future quote cannot be used"
    );
    let late_start = SpotQuotes::open(&path, &assets, 5 * DAY, 7 * DAY).unwrap();
    assert_eq!(
        late_start.0[SPOT].len(),
        1,
        "all pre-start history is reduced to its latest quote"
    );
    assert_eq!(
        late_start.price(a, 6 * DAY - 1, 3 * DAY),
        Some(Some(Decimal::from(131)))
    );
    let before = cash
        .as_ref()
        .unwrap()
        .balance(Currency::BTC())
        .unwrap()
        .as_decimal();
    let mut quote_days = Vec::new();
    let mut quote_previous = Some(Decimal::from(2000));
    append_day(
        &mut quote_days,
        0,
        at,
        &mut quote_previous,
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
        Some(&quotes),
    )
    .unwrap();
    assert!(quote_days[0].equity_usdt.is_some());
    assert_eq!(
        before,
        cash.as_ref()
            .unwrap()
            .balance(Currency::BTC())
            .unwrap()
            .as_decimal()
    );
    assert!(
        fx(Currency::BTC(), &assets, &fx_ids, at, HOUR).is_none(),
        "trading valuation does not use day-end quotes"
    );
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
        None,
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
        None,
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

fn small_assets(directory: &Path) -> (Vec<Asset>, NativeTickEconomicRequestV1) {
    let (prepared, mut request, _) = source(directory, 0, None, true);
    request.small_capital_policy = Some(NativeTickSmallCapitalPolicyV1 {
        maximum_entry_notional_usdc: "30".parse().unwrap(),
        maximum_holding_ns: counter(60 * SECOND).unwrap(),
    });
    let cache =
        cohort_cache::load(&prepared.join("independent/catalog"), &request.research).unwrap();
    let assets = definitions()
        .into_iter()
        .map(|instrument| {
            let id = instrument.id().to_string();
            let mut fast = ExponentialMovingAverage::new(1, None);
            fast.update_raw(150.0);
            Asset {
                instrument,
                reader: cache
                    .cohorts(cache.assets.iter().find(|a| a.instrument_id == id).unwrap())
                    .unwrap(),
                head: None,
                previous: None,
                fast,
                slow: ExponentialMovingAverage::new(2, None),
                model: None,
                score: Some(1.0),
                mark: Some((
                    2 * SECOND,
                    if id == BRIDGE {
                        Decimal::ONE
                    } else {
                        Decimal::from(100)
                    },
                )),
                cohorts: 0,
                decisions: 0,
                attempts: 0,
                pending: None,
                target_weight: None,
            }
        })
        .collect();
    (assets, request)
}

#[test]
fn small_policy_uses_real_partial_fills_and_retries_stale_nav_exit_without_erasing_dust() {
    let directory = tempfile::tempdir().unwrap();
    let (mut assets, mut request) = small_assets(directory.path());
    request.scenario.maximum_mark_age_ns = counter(SECOND).unwrap();
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    let mut margin = build(&assets, true, Decimal::from(1000), &request.scenario).unwrap();
    let i = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == SPOT)
        .unwrap();
    let instrument = assets[i].instrument.clone();
    cash.step(SECOND, &instrument, 100.0, 10.0, None).unwrap();
    let mut small = SmallCapital::new(assets.len());
    let mut seed = 0;
    let mut contexts = None;
    let fx_ids = BTreeSet::from([BRIDGE.into()]);
    small_new_entries(
        &mut small,
        &[i],
        &mut assets,
        &cash,
        &margin,
        &fx_ids,
        2 * SECOND,
        request.small_capital_policy.as_ref().unwrap(),
        &request.scenario,
        &mut seed,
        &mut contexts,
    )
    .unwrap();
    let target = assets[i].pending.take().unwrap().2;
    assert_eq!(target, Decimal::new(3, 1));
    cash.step(4 * SECOND, &instrument, 100.0, 0.2, Some(target))
        .unwrap();
    let quantity = small_quantity(&assets[i], &cash, &margin);
    assert_eq!(quantity, Decimal::new(2, 1));
    small
        .reconcile(
            i,
            4 * SECOND,
            quantity,
            true,
            &mut assets,
            &request.scenario,
            &mut seed,
        )
        .unwrap();
    assert_eq!(small.positions_opened, 1);
    assert_eq!(
        small.cycles[i].as_ref().unwrap().first_fill,
        Some(4 * SECOND)
    );
    // A partial entry is never topped up on a later signal.
    small_new_entries(
        &mut small,
        &[i],
        &mut assets,
        &cash,
        &margin,
        &fx_ids,
        20 * SECOND,
        request.small_capital_policy.as_ref().unwrap(),
        &request.scenario,
        &mut seed,
        &mut contexts,
    )
    .unwrap();
    assert!(assets[i].pending.is_none());
    assert!(value(
        &cash,
        &assets,
        &fx_ids,
        100 * SECOND,
        SECOND,
        false,
        &mut contexts,
        None
    )
    .unwrap()
    .is_none());
    small
        .expire(100 * SECOND, &mut assets, &request.scenario, &mut seed)
        .unwrap();
    let pending = assets[i].pending.take().unwrap();
    assert_eq!(pending.0, 62 * SECOND); // Exact deadline from entry decision, not next event100s.
    assert_eq!(pending.1, 63 * SECOND + 1);
    assert_eq!(pending.2, Decimal::ZERO);
    cash.step(100 * SECOND, &instrument, 100.0, 0.1, Some(Decimal::ZERO))
        .unwrap();
    small
        .reconcile(
            i,
            100 * SECOND,
            small_quantity(&assets[i], &cash, &margin),
            true,
            &mut assets,
            &request.scenario,
            &mut seed,
        )
        .unwrap();
    assert_eq!(
        small.cycles[i].as_ref().unwrap().quantity,
        Decimal::new(1, 1)
    );
    assert_eq!(small.positions_closed, 0);
    // Remaining0.1 BTC at99 is below the actual10USDC minimum: retained, never marked closed.
    assets[i].pending.take();
    cash.step(102 * SECOND, &instrument, 99.0, 10.0, Some(Decimal::ZERO))
        .unwrap();
    small
        .reconcile(
            i,
            102 * SECOND,
            small_quantity(&assets[i], &cash, &margin),
            true,
            &mut assets,
            &request.scenario,
            &mut seed,
        )
        .unwrap();
    assert_eq!(small.positions_closed, 0);
    assert_eq!(
        decimal(&small.report(&assets).unwrap().remaining_positions[0].quantity).unwrap(),
        Decimal::new(1, 1)
    );
    assert_eq!(small.exit_attempts, 2);
    assets[i].pending.take();
    cash.step(104 * SECOND, &instrument, 101.0, 10.0, Some(Decimal::ZERO))
        .unwrap();
    small
        .reconcile(
            i,
            104 * SECOND,
            small_quantity(&assets[i], &cash, &margin),
            true,
            &mut assets,
            &request.scenario,
            &mut seed,
        )
        .unwrap();
    assert!(small.cycles[i].is_none());
    assert_eq!(small.positions_closed, 1);
    assert_eq!(small.maximum_exit_delay, Some(42 * SECOND));
    assert!(small
        .report(&assets)
        .unwrap()
        .remaining_positions
        .is_empty());
    cash.finish().unwrap();
    margin.finish().unwrap();
}

#[test]
fn small_unfilled_entry_expires_before_late_execution_and_reference_exit_requires_later_event() {
    let directory = tempfile::tempdir().unwrap();
    let (mut assets, request) = small_assets(directory.path());
    let i = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == SPOT)
        .unwrap();
    let mut small = SmallCapital::new(assets.len());
    let mut seed = 0;
    let cycle = SmallCycle {
        decision: 2 * SECOND,
        deadline: 62 * SECOND,
        reference: Decimal::from(150),
        direction: Decimal::ONE,
        reservation_usdc: Decimal::from(30),
        reservation_quote: Decimal::from(30),
        first_fill: None,
        exit_decision: None,
        quantity: Decimal::ZERO,
    };
    small.cycles[i] = Some(cycle.clone());
    small.deadlines.push(Reverse((cycle.deadline, i)));
    small_queue(
        &mut assets[i],
        cycle.decision,
        cycle.decision,
        Decimal::new(3, 1),
        &request.scenario,
        &mut seed,
    )
    .unwrap();
    small
        .expire(62 * SECOND, &mut assets, &request.scenario, &mut seed)
        .unwrap();
    assert!(small.cycles[i].is_none() && assets[i].pending.is_none());
    assert_eq!(small.unfilled_entries, 1);
    assert_eq!(small.positions_opened, 0);
    small.cycles[i] = Some(SmallCycle {
        first_fill: Some(4 * SECOND),
        quantity: Decimal::new(3, 1),
        ..cycle
    });
    small
        .observe_reference(
            i,
            10 * SECOND,
            Decimal::from(150),
            &mut assets,
            &request.scenario,
            &mut seed,
        )
        .unwrap();
    let (decision, due, target) = assets[i].pending.unwrap();
    assert_eq!(decision, 10 * SECOND);
    assert!(due > decision);
    assert_eq!(target, Decimal::ZERO);
}

#[test]
fn small_pending_entry_counts_toward_gross_and_cash_and_bridge_is_not_alpha() {
    let directory = tempfile::tempdir().unwrap();
    let (mut assets, request) = small_assets(directory.path());
    let mut cash = build(&assets, false, Decimal::from(100), &request.scenario).unwrap();
    let mut margin = build(&assets, true, Decimal::from(100), &request.scenario).unwrap();
    let i = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == SPOT)
        .unwrap();
    let bridge = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == BRIDGE)
        .unwrap();
    let mut small = SmallCapital::new(assets.len());
    let mut seed = 0;
    let mut contexts = None;
    let fx_ids = BTreeSet::from([BRIDGE.into()]);
    small_new_entries(
        &mut small,
        &[bridge, i],
        &mut assets,
        &cash,
        &margin,
        &fx_ids,
        2 * SECOND,
        request.small_capital_policy.as_ref().unwrap(),
        &request.scenario,
        &mut seed,
        &mut contexts,
    )
    .unwrap();
    assert!(small.cycles[bridge].is_none());
    assert_eq!(
        small_exposure(
            &small,
            &assets,
            &cash,
            &margin,
            &fx_ids,
            2 * SECOND,
            request.scenario.maximum_mark_age_ns.get(),
            false,
            &mut contexts
        )
        .unwrap()
        .unwrap()[i],
        Decimal::from(30)
    );
    assert_eq!(
        small_quote_available(
            &small,
            &assets,
            &cash,
            Currency::USDC(),
            2 * SECOND,
            request.scenario.maximum_mark_age_ns.get(),
            Decimal::ZERO
        ),
        Some(Decimal::from(70))
    );
    // Execution price200 doubles: the same intent shrinks to0.15 instead of spending60.
    assets[i].mark = Some((3 * SECOND, Decimal::from(200)));
    let mut rows = [NativeMark {
        instrument_id: assets[i].instrument.id(),
        mark: 200.0,
        capacity: Decimal::ONE,
        target_quantity: Some(Decimal::new(3, 1)),
    }];
    small_cap_entries(
        &small,
        &mut rows,
        &assets,
        &cash,
        &margin,
        &fx_ids,
        3 * SECOND,
        &request.scenario,
        &mut contexts,
    )
    .unwrap();
    assert_eq!(rows[0].target_quantity, Some(Decimal::new(15, 2)));
    cash.finish().unwrap();
    margin.finish().unwrap();
}

#[test]
fn small_execution_cap_keeps_perp_short_and_respects_entry_quote_reservation() {
    let directory = tempfile::tempdir().unwrap();
    let (mut assets, request) = small_assets(directory.path());
    let mut cash = build(&assets, false, Decimal::from(100), &request.scenario).unwrap();
    let mut margin = build(&assets, true, Decimal::from(100), &request.scenario).unwrap();
    let i = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == LATE)
        .unwrap();
    let mut small = SmallCapital::new(assets.len());
    small.cycles[i] = Some(SmallCycle {
        decision: SECOND,
        deadline: 61 * SECOND,
        reference: Decimal::from(90),
        direction: -Decimal::ONE,
        reservation_usdc: Decimal::from(30),
        reservation_quote: Decimal::from(30),
        first_fill: None,
        exit_decision: None,
        quantity: Decimal::ZERO,
    });
    let mut rows = [NativeMark {
        instrument_id: assets[i].instrument.id(),
        mark: 100.0,
        capacity: Decimal::ONE,
        target_quantity: Some(Decimal::new(-3, 1)),
    }];
    let fx_ids = BTreeSet::from([BRIDGE.into()]);
    let mut contexts = None;
    small_cap_entries(
        &small,
        &mut rows,
        &assets,
        &cash,
        &margin,
        &fx_ids,
        2 * SECOND,
        &request.scenario,
        &mut contexts,
    )
    .unwrap();
    assert_eq!(rows[0].target_quantity, Some(Decimal::new(-3, 1)));
    let spot = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == SPOT)
        .unwrap();
    small.cycles[spot] = Some(SmallCycle {
        direction: Decimal::ONE,
        reservation_quote: Decimal::from(20),
        ..small.cycles[i].as_ref().unwrap().clone()
    });
    assets[spot].pending = Some((SECOND, 2 * SECOND, Decimal::new(3, 1)));
    let mut rows = [NativeMark {
        instrument_id: assets[spot].instrument.id(),
        mark: 100.0,
        capacity: Decimal::ONE,
        target_quantity: Some(Decimal::new(3, 1)),
    }];
    small_cap_entries(
        &small,
        &mut rows,
        &assets,
        &cash,
        &margin,
        &fx_ids,
        2 * SECOND,
        &request.scenario,
        &mut contexts,
    )
    .unwrap();
    assert_eq!(
        rows[0].target_quantity,
        Some(Decimal::new(2, 1)),
        "cannot spend quote cash reserved beyond this entry's own20"
    );
    cash.finish().unwrap();
    margin.finish().unwrap();
}

#[test]
fn small_same_base_different_quotes_simultaneous_fresh_signals_get_one_pending_cycle() {
    for reverse in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (mut assets, request) = small_assets(directory.path());
        let duplicate_directory = tempfile::tempdir().unwrap();
        let (mut duplicate_source, _) = small_assets(duplicate_directory.path());
        let mut other = duplicate_source.remove(1);
        assert_eq!(other.instrument.base_currency(), Some(Currency::BTC()));
        let usdt0 = Currency::from_str("USDT0").unwrap();
        other.instrument = InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from("BTC-USDT0-SPOT.HYPERLIQUID"))
                .raw_symbol(Symbol::new("SAME-BASE-UNIT-ONLY"))
                .base_currency(Currency::BTC())
                .quote_currency(usdt0)
                .price_precision(2)
                .size_precision(4)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.0001"))
                .min_notional(Money::new(10.0, usdt0))
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        );
        let other_index = assets.len();
        assets.push(other);
        let i = assets
            .iter()
            .position(|a| a.instrument.id().to_string() == SPOT)
            .unwrap();
        let bridge = assets
            .iter()
            .position(|a| a.instrument.id().to_string() == BRIDGE)
            .unwrap();
        let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
        let mut margin = build(&assets, true, Decimal::from(1000), &request.scenario).unwrap();
        cash.inventory_target_instruments(&[assets[bridge].instrument.id()])
            .unwrap();
        cash.step(
            SECOND,
            &assets[bridge].instrument,
            1.0,
            1000.0,
            Some(Decimal::from(25)),
        )
        .unwrap();
        assert_eq!(cash.balance(usdt0).unwrap().as_decimal(), Decimal::from(25));
        let mut small = SmallCapital::new(assets.len());
        let mut seed = 0;
        let mut contexts = None;
        let fresh = if reverse {
            vec![other_index, i]
        } else {
            vec![i, other_index]
        };
        small_new_entries(
            &mut small,
            &fresh,
            &mut assets,
            &cash,
            &margin,
            &BTreeSet::from([BRIDGE.into()]),
            2 * SECOND,
            request.small_capital_policy.as_ref().unwrap(),
            &request.scenario,
            &mut seed,
            &mut contexts,
        )
        .unwrap();
        assert!(small.cycles[fresh[0]].is_some() && assets[fresh[0]].pending.is_some());
        assert!(small.cycles[fresh[1]].is_none() && assets[fresh[1]].pending.is_none());
        assert_eq!(small.cycles.iter().filter(|c| c.is_some()).count(), 1);
        cash.finish().unwrap();
        margin.finish().unwrap();
    }
}

#[test]
fn small_sleeve_budgets_are_independent_and_perp_exposure_uses_context_mark() {
    let directory = tempfile::tempdir().unwrap();
    let (mut assets, request) = small_assets(directory.path());
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    let mut margin = build(&assets, true, Decimal::from(1000), &request.scenario).unwrap();
    let spot = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == SPOT)
        .unwrap();
    let perp = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == LATE)
        .unwrap();
    cash.step(SECOND, &assets[spot].instrument, 100.0, 10.0, None)
        .unwrap();
    cash.step(
        2 * SECOND,
        &assets[spot].instrument,
        100.0,
        10.0,
        Some(Decimal::new(2, 1)),
    )
    .unwrap();
    margin
        .step(SECOND, &assets[perp].instrument, 100.0, 10.0, None)
        .unwrap();
    margin
        .step(
            2 * SECOND,
            &assets[perp].instrument,
            100.0,
            10.0,
            Some(Decimal::ONE),
        )
        .unwrap();
    let mut small = SmallCapital::new(assets.len());
    small.cycles[spot] = Some(SmallCycle {
        decision: SECOND,
        deadline: 61 * SECOND,
        reference: Decimal::from(150),
        direction: Decimal::ONE,
        reservation_usdc: Decimal::from(20),
        reservation_quote: Decimal::from(20),
        first_fill: Some(2 * SECOND),
        exit_decision: None,
        quantity: Decimal::new(2, 1),
    });
    let csv = format!("event_ns,oracle_px,mark_px\n{},50,250\n", 10 * SECOND);
    std::fs::write(directory.path().join("small-context.csv"), &csv).unwrap();
    let reader = ContextReader::open(
        directory.path(),
        ContextFile {
            instrument_id: LATE.into(),
            path: "small-context.csv".into(),
            bytes: counter(csv.len() as u64).unwrap(),
            rows: counter(1).unwrap(),
            first_event_ns: Some(counter(10 * SECOND).unwrap()),
            last_event_ns: Some(counter(10 * SECOND).unwrap()),
        },
    )
    .unwrap();
    let mut contexts = Some(ContextPrices {
        source_revision: "unit-context".into(),
        readers: BTreeMap::from([(LATE.into(), reader)]),
    });
    let fx_ids = BTreeSet::from([BRIDGE.into()]);
    let exposure = small_exposure(
        &small,
        &assets,
        &cash,
        &margin,
        &fx_ids,
        10 * SECOND,
        SECOND,
        true,
        &mut contexts,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        exposure[perp],
        Decimal::from(250),
        "context250, not stale cohort100 or oracle50"
    );
    assert!(
        small_exposure(
            &small,
            &assets,
            &cash,
            &margin,
            &fx_ids,
            10 * SECOND,
            SECOND,
            false,
            &mut contexts
        )
        .unwrap()
        .is_none(),
        "cash sleeve independently missing its own BTC valuation"
    );
    assets[spot].mark = Some((12 * SECOND, Decimal::from(100)));
    assets[perp].mark = Some((12 * SECOND, Decimal::from(100)));
    let exposure = small_exposure(
        &small,
        &assets,
        &cash,
        &margin,
        &fx_ids,
        12 * SECOND,
        SECOND,
        false,
        &mut contexts,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        exposure[spot],
        Decimal::from(20),
        "missing perp context cannot block fresh cash sleeve"
    );
    assert!(
        small_exposure(
            &small,
            &assets,
            &cash,
            &margin,
            &fx_ids,
            12 * SECOND,
            SECOND,
            true,
            &mut contexts
        )
        .unwrap()
        .is_none(),
        "fresh trade cannot replace expired explicit perp context"
    );
    contexts.as_mut().unwrap().finish().unwrap();
    cash.finish().unwrap();
    margin.finish().unwrap();
}

fn small_spot_template(mut template: Asset, id: &str, base: Currency, quote: Currency) -> Asset {
    template.instrument = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from(id))
            .raw_symbol(Symbol::new("SMALL-CAPITAL-UNIT-ONLY"))
            .base_currency(base)
            .quote_currency(quote)
            .price_precision(2)
            .size_precision(4)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.0001"))
            .min_notional(Money::new(10.0, quote))
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    template
}

#[test]
fn small_same_base_perp_and_spot_are_independent_and_cash_inventory_belongs_to_spot() {
    for reverse in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let extra_directory = tempfile::tempdir().unwrap();
        let (mut assets, request) = small_assets(directory.path());
        let (mut templates, _) = small_assets(extra_directory.path());
        let spot = assets.len();
        assets.push(small_spot_template(
            templates.remove(1),
            "ETH-USDC-SPOT.HYPERLIQUID",
            Currency::ETH(),
            Currency::USDC(),
        ));
        let perp = assets
            .iter()
            .position(|a| a.instrument.id().to_string() == LATE)
            .unwrap();
        assert!(
            perp < spot,
            "perp-first ordering catches inventory attribution bug"
        );
        let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
        let mut margin = build(&assets, true, Decimal::from(1000), &request.scenario).unwrap();
        let mut small = SmallCapital::new(assets.len());
        let mut seed = 0;
        let mut contexts = None;
        let fx_ids = BTreeSet::from([BRIDGE.into()]);
        let fresh = if reverse {
            vec![spot, perp]
        } else {
            vec![perp, spot]
        };
        small_new_entries(
            &mut small,
            &fresh,
            &mut assets,
            &cash,
            &margin,
            &fx_ids,
            2 * SECOND,
            request.small_capital_policy.as_ref().unwrap(),
            &request.scenario,
            &mut seed,
            &mut contexts,
        )
        .unwrap();
        assert!(small.cycles[spot].is_some() && small.cycles[perp].is_some());
        cash.step(3 * SECOND, &assets[spot].instrument, 100.0, 10.0, None)
            .unwrap();
        cash.step(
            4 * SECOND,
            &assets[spot].instrument,
            100.0,
            10.0,
            Some(Decimal::new(3, 1)),
        )
        .unwrap();
        assets[spot].pending = None;
        small
            .reconcile(
                spot,
                4 * SECOND,
                small_quantity(&assets[spot], &cash, &margin),
                true,
                &mut assets,
                &request.scenario,
                &mut seed,
            )
            .unwrap();
        let exposure = small_exposure(
            &small,
            &assets,
            &cash,
            &margin,
            &fx_ids,
            4 * SECOND,
            request.scenario.maximum_mark_age_ns.get(),
            false,
            &mut contexts,
        )
        .unwrap()
        .unwrap();
        assert_eq!(exposure[spot], Decimal::from(30));
        assert_eq!(
            exposure[perp],
            Decimal::ZERO,
            "cash ETH cannot be attributed to earlier ETH perp cycle"
        );
        cash.finish().unwrap();
        margin.finish().unwrap();
    }
}

#[test]
fn small_quote_shrink_keeps_signed_deficit_and_protects_other_pending_reservation() {
    let directory = tempfile::tempdir().unwrap();
    let extra1 = tempfile::tempdir().unwrap();
    let extra2 = tempfile::tempdir().unwrap();
    let (mut assets, request) = small_assets(directory.path());
    let (mut templates1, _) = small_assets(extra1.path());
    let (mut templates2, _) = small_assets(extra2.path());
    let quote = Currency::from_str("USDT0").unwrap();
    let btc = assets.len();
    assets.push(small_spot_template(
        templates1.remove(1),
        "BTC-USDT0-SPOT.HYPERLIQUID",
        Currency::BTC(),
        quote,
    ));
    let eth = assets.len();
    assets.push(small_spot_template(
        templates2.remove(1),
        "ETH-USDT0-SPOT.HYPERLIQUID",
        Currency::ETH(),
        quote,
    ));
    let bridge = assets
        .iter()
        .position(|a| a.instrument.id().to_string() == BRIDGE)
        .unwrap();
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    let mut margin = build(&assets, true, Decimal::from(1000), &request.scenario).unwrap();
    cash.inventory_target_instruments(&[assets[bridge].instrument.id()])
        .unwrap();
    cash.step(
        SECOND,
        &assets[bridge].instrument,
        1.0,
        1000.0,
        Some(Decimal::from(60)),
    )
    .unwrap();
    let mut small = SmallCapital::new(assets.len());
    for i in [btc, eth] {
        small.cycles[i] = Some(SmallCycle {
            decision: SECOND,
            deadline: 61 * SECOND,
            reference: Decimal::from(150),
            direction: Decimal::ONE,
            reservation_usdc: Decimal::from(30),
            reservation_quote: Decimal::from(30),
            first_fill: None,
            exit_decision: None,
            quantity: Decimal::ZERO,
        });
        assets[i].pending = Some((SECOND, 2 * SECOND, Decimal::new(3, 1)));
    }
    // A real intervening native fill reduces the quote wallet after the two reservations.
    cash.step(
        2 * SECOND,
        &assets[bridge].instrument,
        1.0,
        1000.0,
        Some(Decimal::from(40)),
    )
    .unwrap();
    assert_eq!(cash.balance(quote).unwrap().as_decimal(), Decimal::from(40));
    let fx_ids = BTreeSet::from([BRIDGE.into()]);
    let mut contexts = None;
    assert_eq!(
        small_quote_available(
            &small,
            &assets,
            &cash,
            quote,
            3 * SECOND,
            request.scenario.maximum_mark_age_ns.get(),
            Decimal::ZERO
        ),
        Some(Decimal::from(-20))
    );
    let mut rows = [NativeMark {
        instrument_id: assets[btc].instrument.id(),
        mark: 100.0,
        capacity: Decimal::ONE,
        target_quantity: Some(Decimal::new(3, 1)),
    }];
    small_cap_entries(
        &small,
        &mut rows,
        &assets,
        &cash,
        &margin,
        &fx_ids,
        3 * SECOND,
        &request.scenario,
        &mut contexts,
    )
    .unwrap();
    assert_eq!(rows[0].target_quantity, Some(Decimal::new(1,1)), "only10 quote is unreserved after protecting the other30; old clamped deficit incorrectly allowed30");
    cash.finish().unwrap();
    margin.finish().unwrap();
}

#[test]
fn small_nonzero_roundtrip_cost_hurdle_excludes_equality_and_below() {
    for (price, allowed) in [("99.81", true), ("99.82", false), ("99.83", false)] {
        let directory = tempfile::tempdir().unwrap();
        let (mut assets, mut request) = small_assets(directory.path());
        request.scenario.spot_taker_fee_bps = "7".parse().unwrap();
        request.scenario.extra_cost_bps = "2".parse().unwrap();
        let i = assets
            .iter()
            .position(|a| a.instrument.id().to_string() == SPOT)
            .unwrap();
        assets[i].mark = Some((2 * SECOND, Decimal::from_str(price).unwrap()));
        assets[i].fast.update_raw(100.0);
        let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
        let mut margin = build(&assets, true, Decimal::from(1000), &request.scenario).unwrap();
        let mut small = SmallCapital::new(assets.len());
        let mut seed = 0;
        let mut contexts = None;
        small_new_entries(
            &mut small,
            &[i],
            &mut assets,
            &cash,
            &margin,
            &BTreeSet::from([BRIDGE.into()]),
            2 * SECOND,
            request.small_capital_policy.as_ref().unwrap(),
            &request.scenario,
            &mut seed,
            &mut contexts,
        )
        .unwrap();
        assert_eq!(
            small.cycles[i].is_some(),
            allowed,
            "price{price}: gap must exceed18bp, not equal it"
        );
        cash.finish().unwrap();
        margin.finish().unwrap();
    }
}

#[test]
fn small_opt_in_replay_funds_quote_then_waits_for_fresh_signal_and_exits_at_real_later_cohort() {
    let directory = tempfile::tempdir().unwrap();
    let mut instruments = definitions();
    let usdt0 = Currency::from_str("USDT0").unwrap();
    let alpha = "BTC-USDT0-SPOT.HYPERLIQUID";
    instruments[1] = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from(alpha))
            .raw_symbol(Symbol::new("SMALL-REPLAY-UNIT-ONLY"))
            .base_currency(Currency::BTC())
            .quote_currency(usdt0)
            .price_precision(2)
            .size_precision(4)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.0001"))
            .min_notional(Money::new(10.0, usdt0))
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    let mut request = super::tests::request();
    request.research.input.instrument_ids = vec![BRIDGE.into(), alpha.into(), LATE.into()];
    request.research.input.event_end_ns = counter(4 * DAY).unwrap();
    request.research.input.decision_cutoff_ns = request.research.input.event_end_ns;
    request.research.train_end_ns = counter(DAY).unwrap();
    request.research.validation_end_ns = counter(3 * DAY).unwrap();
    request.research.parameters.fast_period_cohorts = 2;
    request.research.parameters.slow_period_cohorts = 3;
    request.research.target = contracts::ticks::NativeTickTargetV1::FixedEventTime {
        horizon_ns: counter(60 * SECOND).unwrap(),
    };
    request.scenario.participation_fraction = "1".parse().unwrap();
    request.scenario.maximum_asset_weight = "0.05".parse().unwrap();
    request.scenario.spot_taker_fee_bps = "7".parse().unwrap();
    request.scenario.extra_cost_bps = "2".parse().unwrap();
    request.scenario.decision_interval_ns = counter(10 * SECOND).unwrap();
    request.scenario.base_latency_ns = counter(SECOND).unwrap();
    request.scenario.jitter_min_ns = counter(0).unwrap();
    request.scenario.jitter_max_ns = counter(0).unwrap();
    request.scenario.maximum_mark_age_ns = counter(HOUR).unwrap();
    request.fx_instrument_ids = vec![BRIDGE.into()];
    request.small_capital_policy = Some(NativeTickSmallCapitalPolicyV1 {
        maximum_entry_notional_usdc: "30".parse().unwrap(),
        maximum_holding_ns: counter(60 * SECOND).unwrap(),
    });
    let mut rows = Vec::new();
    let mut add = |id: &str, seconds: u64, price: &str, side: AggressorSide| {
        rows.push(TradeTick::new(
            InstrumentId::from(id),
            Price::from(price),
            Quantity::from("10000.0000"),
            side,
            TradeId::from(format!("SMALL-UNIT-{}", rows.len()).as_str()),
            (seconds * SECOND).into(),
            (seconds * SECOND).into(),
        ));
    };
    for seconds in [1, 3, 11, 12, 90, 100] {
        add(BRIDGE, seconds, "1.000", AggressorSide::Buy);
        add(BRIDGE, seconds, "1.000", AggressorSide::Sell);
    }
    for seconds in [4, 6, 8, 10, 20, 21, 22, 100, 3 * DAY / SECOND + 10] {
        add(
            alpha,
            seconds,
            if seconds < 10 {
                "200.00"
            } else if seconds < 110 {
                "100.00"
            } else {
                "90.00"
            },
            AggressorSide::Buy,
        );
    }
    // A real source cohort creates the perp cache identity; clipping then legitimately leaves it empty in primary.
    add(LATE, 3 * DAY / SECOND + 20, "100.00", AggressorSide::Buy);
    rows.sort_by_key(|row| (row.ts_event, row.instrument_id));
    let source = directory.path().join("source");
    let mut builder = cohort_cache::Builder::new_flow(&source, 100, 100, 1_000_000).unwrap();
    for row in rows {
        builder.push(row).unwrap();
    }
    let manifest = builder.seal("fixed-source", 0, 4 * DAY).unwrap();
    builder.publish(manifest, json!({"status":"IMPORTED","availability":"EVENT_TIME_PROXY_UNVERIFIED","source_revision":"fixed-source",
        "event_start_ns":"0","event_end_ns":(4*DAY).to_string(),"missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0,
        "instrument_source":{"currencies":crate::catalog::currencies::definitions(&instruments),"instruments":instruments}})).unwrap();
    let catalog = source.join("catalog");
    std::fs::write(catalog.join("funding-rates.jsonl"), "").unwrap();
    std::fs::write(
        catalog.join("funding-rates.manifest.json"),
        serde_json::to_vec(&json!({"source_revision":"fixed-source","start_ns":"0",
        "end_ns":(4*DAY).to_string(),"instruments":[LATE],"rows":0,"missing_instruments":[]}))
        .unwrap(),
    )
    .unwrap();
    let primary_quote = format!(
        "{}\n",
        json!({
            "instrument_id": BRIDGE, "received_ns": (DAY - 1).to_string(), "bid": "0.99", "ask": "1.01"
        })
    );
    let heldout_quote = format!(
        "{}\n",
        json!({
            "instrument_id": BRIDGE, "received_ns": (3 * DAY).to_string(), "bid": "0.98", "ask": "1.02"
        })
    );
    let after_end = format!(
        "{}\n",
        json!({
            "instrument_id": BRIDGE, "received_ns": (4 * DAY).to_string(), "bid": "0.97", "ask": "1.03"
        })
    );
    let spot_quotes = format!("{primary_quote}{heldout_quote}{after_end}");
    std::fs::write(catalog.join("spot-valuation.jsonl"), &spot_quotes).unwrap();
    let prepared = directory.path().join("prepared");
    let metadata =
        cohort_cache::prepare_views(&catalog, &request.research, &prepared, 1_000_000).unwrap();
    assert!(metadata["economic_sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "spot-valuation.jsonl"
            && file["research_bytes"] == primary_quote.len().to_string()
            && file["bytes"] == (primary_quote.len() + heldout_quote.len()).to_string()));
    assert_eq!(
        std::fs::read_to_string(prepared.join("research/catalog/spot-valuation.jsonl")).unwrap(),
        primary_quote
    );
    assert_eq!(
        std::fs::read_to_string(prepared.join("independent/catalog/spot-valuation.jsonl")).unwrap(),
        format!("{primary_quote}{heldout_quote}")
    );
    // Consume the official clipped request paired with the primary cache, not a mismatched full-window request.
    request.research = serde_json::from_value(metadata["research_request"].clone()).unwrap();
    assert_eq!(request.research.input.event_end_ns.get(), 3 * DAY);
    let result = replay(
        &prepared.join("research/catalog"),
        &request,
        &crate::ticks::tests::model(7),
    )
    .unwrap();
    let diagnostics = result.small_capital_diagnostics.as_ref().unwrap();
    assert_eq!(
        diagnostics.entry_attempts.get(),
        1,
        "bridge fill must precede a new fresh alpha signal; out-of-window source cannot cause re-entry"
    );
    assert_eq!(diagnostics.positions_opened.get(), 1);
    assert_eq!(diagnostics.positions_closed.get(), 1);
    assert_eq!(diagnostics.exit_attempts.get(), 1);
    assert_eq!(
        diagnostics.maximum_exit_delay_ns.unwrap().get(),
        20 * SECOND,
        "entry decision20s -> deadline80s -> actual exit100s"
    );
    assert!(diagnostics.remaining_positions.is_empty());
    assert_eq!(result.native_fills.get(), 3, "native quote bridge buy, native alpha buy, native alpha sell; initial wealth conversion is separate");
    assert!(result
        .fees_by_currency
        .values()
        .any(|fee| decimal(fee).unwrap() > Decimal::ZERO));
    assert!(
        result.fees_usdc_equivalent.is_none(),
        "expired final quote FX leaves fee conversion unknown too"
    );
    assert!(
        result.days.iter().any(|d| d.equity_usdt.is_none()),
        "unquoted residual FX cannot be magically valued after the source ends"
    );
    assert_eq!(
        result.assets.len(),
        3,
        "zero-cohort perp remains in full recorded scope"
    );
    assert_eq!(
        result
            .assets
            .iter()
            .find(|a| a.instrument_id == alpha)
            .unwrap()
            .cohorts
            .get(),
        8,
        "profitable-looking source event after the prepared primary-view end is outside the replay"
    );
    assert_eq!(result.model, "SEGREGATED_COHORT_MARK_ESTIMATE_V1");
    request.scenario.execution_model =
        Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1);
    let quote_result = replay(
        &prepared.join("research/catalog"),
        &request,
        &crate::ticks::tests::model(7),
    )
    .unwrap();
    assert_eq!(quote_result.model, "SEGREGATED_SYNTHETIC_QUOTE_IOC_V1");
    assert_eq!(
        serde_json::to_value(&quote_result.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    assert!(quote_result.methods["execution"].contains("not observed quotes or book depth"));
    request.spot_valuation_source = Some("spot-valuation.jsonl".into());
    let valued = replay(
        &prepared.join("research/catalog"),
        &request,
        &crate::ticks::tests::model(7),
    )
    .unwrap();
    assert_eq!(valued.native_fills, quote_result.native_fills);
    assert_eq!(valued.native_rejections, quote_result.native_rejections);
    assert_eq!(
        serde_json::to_value(&valued.fees_by_currency).unwrap(),
        serde_json::to_value(&quote_result.fees_by_currency).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&valued.small_capital_diagnostics).unwrap(),
        serde_json::to_value(&quote_result.small_capital_diagnostics).unwrap()
    );
    assert!(valued.methods["spot_valuation"].contains("do not change trading"));
}

#[test]
fn ioc_bounded_flat_bar_investigation() {
    let directory = tempfile::tempdir().unwrap();
    let (assets, mut request) = small_assets(directory.path());
    request.scenario.execution_model =
        Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1);
    let instrument = assets
        .iter()
        .find(|a| a.instrument.id().to_string() == SPOT)
        .unwrap()
        .instrument
        .clone();
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    cash.step(SECOND, &instrument, 100.0, 4.0, None).unwrap();
    cash.step(2 * SECOND, &instrument, 101.0, 0.4, Some(Decimal::ONE))
        .unwrap();
    assert_eq!(
        cash.balance(Currency::BTC()).map(|m| m.as_decimal()),
        Some(Decimal::new(4, 1))
    );
    cash.step(3 * SECOND, &instrument, 101.0, 0.4, Some(Decimal::ONE))
        .unwrap();
    assert_eq!(
        cash.balance(Currency::BTC()).map(|m| m.as_decimal()),
        Some(Decimal::new(8, 1))
    );
    cash.step(4 * SECOND, &instrument, 102.0, 0.4, Some(Decimal::ONE))
        .unwrap();
    assert_eq!(
        cash.balance(Currency::BTC()).map(|m| m.as_decimal()),
        Some(Decimal::ONE)
    );
    cash.step(5 * SECOND, &instrument, 101.0, 0.95, Some(Decimal::ZERO))
        .unwrap();
    assert_eq!(
        cash.balance(Currency::BTC()).map(|m| m.as_decimal()),
        Some(Decimal::new(5, 2))
    );
    cash.step(6 * SECOND, &instrument, 99.0, 1.0, Some(Decimal::ZERO))
        .unwrap();
    assert_eq!(
        cash.balance(Currency::BTC()).map(|m| m.as_decimal()),
        Some(Decimal::new(5, 2))
    );
}

#[test]
fn ioc_zero_capacity_quote_probe() {
    let directory = tempfile::tempdir().unwrap();
    let (assets, mut request) = small_assets(directory.path());
    request.scenario.execution_model =
        Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1);
    let instrument = assets
        .iter()
        .find(|a| a.instrument.id().to_string() == SPOT)
        .unwrap()
        .instrument
        .clone();
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    cash.step(SECOND, &instrument, 100.0, 0.4, Some(Decimal::ONE))
        .unwrap();
    assert_eq!(
        cash.balance(Currency::BTC()).map(|m| m.as_decimal()),
        Some(Decimal::new(4, 1))
    );
    cash.step(2 * SECOND, &instrument, 101.0, 0.0, Some(Decimal::ONE))
        .unwrap();
    assert_eq!(
        cash.balance(Currency::BTC()).map(|m| m.as_decimal()),
        Some(Decimal::new(4, 1))
    );
}

#[test]
fn quote_batch_ioc_instruments_do_not_reuse_capacity() {
    let directory = tempfile::tempdir().unwrap();
    let (assets, mut request) = small_assets(directory.path());
    request.scenario.execution_model =
        Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1);
    let instrument = |name: &str| {
        assets
            .iter()
            .find(|a| a.instrument.id().to_string() == name)
            .unwrap()
            .instrument
            .clone()
    };
    let spot = instrument(SPOT);
    let bridge = instrument(BRIDGE);
    let bridge_currency = bridge.base_currency().unwrap();
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    cash.inventory_target_instruments(&[bridge.id()]).unwrap();
    let rows = |cap_btc: Decimal, cap_bridge: Decimal, target: Decimal| {
        vec![
            NativeMark {
                instrument_id: spot.id(),
                mark: 100.0,
                capacity: cap_btc,
                target_quantity: Some(target),
            },
            NativeMark {
                instrument_id: bridge.id(),
                mark: 100.0,
                capacity: cap_bridge,
                target_quantity: Some(target),
            },
        ]
    };
    cash.step_batch(
        SECOND,
        &rows(Decimal::new(4, 1), Decimal::new(3, 1), Decimal::ONE),
    )
    .unwrap();
    assert_eq!(cash.summary().submitted, 2);
    assert_eq!(cash.pending_order_count(), 0);
    assert_eq!(
        cash.balance(Currency::BTC()).unwrap().as_decimal(),
        Decimal::new(4, 1)
    );
    assert_eq!(
        cash.balance(bridge_currency).unwrap().as_decimal(),
        Decimal::new(3, 1)
    );
    cash.step_batch(
        2 * SECOND,
        &rows(Decimal::new(2, 1), Decimal::new(1, 1), Decimal::ONE),
    )
    .unwrap();
    assert_eq!(cash.summary().submitted, 4);
    assert_eq!(cash.pending_order_count(), 0);
    assert_eq!(
        cash.balance(Currency::BTC()).unwrap().as_decimal(),
        Decimal::new(6, 1)
    );
    assert_eq!(
        cash.balance(bridge_currency).unwrap().as_decimal(),
        Decimal::new(4, 1)
    );
    cash.step_batch(
        3 * SECOND,
        &rows(Decimal::ZERO, Decimal::ZERO, Decimal::ONE),
    )
    .unwrap();
    assert_eq!(cash.summary().submitted, 6);
    assert_eq!(cash.pending_order_count(), 0);
    assert_eq!(
        cash.balance(Currency::BTC()).unwrap().as_decimal(),
        Decimal::new(6, 1)
    );
    assert_eq!(
        cash.balance(bridge_currency).unwrap().as_decimal(),
        Decimal::new(4, 1)
    );
    cash.step_batch(
        4 * SECOND,
        &rows(Decimal::new(55, 2), Decimal::new(35, 2), Decimal::ZERO),
    )
    .unwrap();
    assert_eq!(cash.pending_order_count(), 0);
    assert_eq!(
        cash.balance(Currency::BTC()).unwrap().as_decimal(),
        Decimal::new(5, 2)
    );
    assert_eq!(
        cash.balance(bridge_currency).unwrap().as_decimal(),
        Decimal::new(5, 2)
    );
    let before = cash.summary().submitted;
    assert_eq!(before, 8);
    cash.step_batch(5 * SECOND, &rows(Decimal::ONE, Decimal::ONE, Decimal::ZERO))
        .unwrap();
    assert_eq!(cash.summary().submitted, before);
    assert_eq!(cash.pending_order_count(), 0);
    assert_eq!(
        cash.balance(Currency::BTC()).unwrap().as_decimal(),
        Decimal::new(5, 2)
    );
    assert_eq!(
        cash.balance(bridge_currency).unwrap().as_decimal(),
        Decimal::new(5, 2)
    );
}

#[test]
fn quote_batch_duplicate_instrument_rejected_before_order() {
    let directory = tempfile::tempdir().unwrap();
    let (assets, mut request) = small_assets(directory.path());
    request.scenario.execution_model =
        Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1);
    let spot = assets
        .iter()
        .find(|a| a.instrument.id().to_string() == SPOT)
        .unwrap()
        .instrument
        .clone();
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    let row = || NativeMark {
        instrument_id: spot.id(),
        mark: 100.0,
        capacity: Decimal::ONE,
        target_quantity: Some(Decimal::ONE),
    };
    assert!(cash
        .step_batch(SECOND, &[row(), row()])
        .unwrap_err()
        .to_string()
        .contains("TICK_ECONOMIC_DUPLICATE_MARK"));
    assert_eq!(cash.summary().submitted, 0);
    assert_eq!(cash.pending_order_count(), 0);
}

#[test]
fn quote_zero_capacity_does_not_reuse_prior_unused_size() {
    let directory = tempfile::tempdir().unwrap();
    let (assets, mut request) = small_assets(directory.path());
    request.scenario.execution_model =
        Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1);
    let instrument = assets
        .iter()
        .find(|a| a.instrument.id().to_string() == SPOT)
        .unwrap()
        .instrument
        .clone();
    let mut cash = build(&assets, false, Decimal::from(1000), &request.scenario).unwrap();
    cash.step(SECOND, &instrument, 100.0, 0.4, None).unwrap();
    assert_eq!(cash.summary().submitted, 0);
    cash.step(2 * SECOND, &instrument, 100.0, 0.0, Some(Decimal::ONE))
        .unwrap();
    assert_eq!(cash.summary().submitted, 1);
    assert_eq!(cash.pending_order_count(), 0);
    assert_eq!(
        cash.balance(Currency::BTC())
            .map(|m| m.as_decimal())
            .unwrap_or(Decimal::ZERO),
        Decimal::ZERO
    );
}

#[test]
fn quote_ioc_same_timestamp_funding_uses_previous_position() {
    let directory = tempfile::tempdir().unwrap();
    let (assets, mut request) = small_assets(directory.path());
    request.scenario.execution_model =
        Some(contracts::economic_tick::NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1);
    let instrument = assets
        .iter()
        .find(|a| a.instrument.id().to_string() == LATE)
        .unwrap()
        .instrument
        .clone();
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
    let path = directory.path().join("same-time-rates.jsonl");
    std::fs::write(
        &path,
        format!(
            "{}\n",
            json!({"instrument_id":LATE,"event_ns":(3*SECOND).to_string(),"rate":"0.01"})
        ),
    )
    .unwrap();
    let mut funding = Funding::open(&path, BTreeSet::from([LATE.into()])).unwrap();
    let rates = rates_at(
        &mut funding,
        &mut margin,
        &assets,
        3 * SECOND,
        0,
        HOUR,
        &mut None,
        &mut BTreeSet::new(),
        &mut None,
    )
    .unwrap();
    margin
        .as_mut()
        .unwrap()
        .step_batch_with_rates(
            3 * SECOND,
            &[NativeMark {
                instrument_id: instrument.id(),
                mark: 100.0,
                capacity: Decimal::from(10),
                target_quantity: Some(Decimal::ZERO),
            }],
            &rates,
        )
        .unwrap();
    assert!(margin.as_ref().unwrap().positions().is_empty());
    assert_eq!(
        margin.as_ref().unwrap().summary().adjustment_totals["USDC"],
        Decimal::from(-1)
    );
}
