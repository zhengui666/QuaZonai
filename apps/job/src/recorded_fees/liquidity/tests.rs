use super::*;
use crate::recorded_fees::tests::{charge, market, model, native_scenario, set_time};
use nautilus_model::{identifiers::InstrumentId, orders::OrderTestBuilder};

fn ioc(side: OrderSide, quantity: &str) -> OrderAny {
    OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from("123.POLYMARKET"))
        .side(side)
        .quantity(Quantity::from(quantity))
        .time_in_force(TimeInForce::Ioc)
        .build()
}

fn preview(model: &mut RecordedLiquidityModel, order: &OrderAny) -> Result<OrderBook> {
    model
        .get_orderbook_for_fill_simulation(
            &market().series[0].instrument,
            order,
            Price::from("0.4999"),
            Price::from("0.5001"),
        )?
        .context("test requires positive recorded liquidity")
}

#[test]
fn preview_is_read_only_and_actual_fee_callbacks_share_one_pool_across_sides() {
    let (fees, clock) = model();
    let mut model = RecordedLiquidityModel::new(
        fees.clone(),
        DefaultFillModel::new(1.0, 1.0, Some(17)).unwrap(),
    );
    let buy = ioc(OrderSide::Buy, "100.000000");
    for _ in 0..2 {
        let book = preview(&mut model, &buy).unwrap();
        assert_eq!(book.best_ask_size(), Some(Quantity::from("10.000000")));
        assert_eq!(book.best_ask_price(), Some(Price::from("0.5001")));
    }
    assert!(fees.report().fills.is_empty());
    charge(&fees, "7.000000").unwrap();
    let sell = ioc(OrderSide::Sell, "100.000000");
    let book = preview(&mut model, &sell).unwrap();
    assert_eq!(book.best_bid_size(), Some(Quantity::from("3.000000")));
    assert_eq!(book.best_bid_price(), Some(Price::from("0.4999")));
    let small = preview(&mut model, &ioc(OrderSide::Buy, "1.000000")).unwrap();
    assert_eq!(small.best_ask_size(), Some(Quantity::from("1.000000")));
    charge(&fees, "3.000000").unwrap();
    let error = preview(&mut model, &buy).unwrap_err().to_string();
    assert!(error.contains("RECORDED_LIQUIDITY_EXHAUSTED"));
    assert_eq!(model.observer().check().unwrap_err().to_string(), error);
    assert_eq!(fees.report().fills.len(), 2);
    set_time(&clock, 2 * MINUTE_NS - 1);
    assert!(preview(&mut model, &sell).is_err());
    set_time(&clock, 2 * MINUTE_NS);
    assert_eq!(
        preview(&mut model, &buy).unwrap().best_ask_size(),
        Some(Quantity::from("10.000000"))
    );
    set_time(&clock, 3 * MINUTE_NS);
    assert!(preview(&mut model, &buy)
        .unwrap_err()
        .to_string()
        .contains("RECORDED_LIQUIDITY_CLOCK_UNMATCHED"));
    // Later failures cannot replace the original callback failure.
    assert_eq!(model.observer().check().unwrap_err().to_string(), error);
}

#[test]
fn unsupported_orders_are_errors_not_empty_books_or_default_liquidity() {
    let (fees, _) = model();
    let mut fill = RecordedLiquidityModel::new(fees, DefaultFillModel::default());
    let order = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from("123.POLYMARKET"))
        .side(OrderSide::Buy)
        .quantity(Quantity::from("1.000000"))
        .time_in_force(TimeInForce::Gtc)
        .build();
    assert!(preview(&mut fill, &order)
        .unwrap_err()
        .to_string()
        .contains("RECORDED_LIQUIDITY_MARKET_IOC_REQUIRED"));
    assert!(fill.observer().check().is_err());
}

#[test]
fn seeded_official_probability_sequence_is_preserved_without_pre_slipping_books() {
    let (fees, _) = model();
    let mut fill =
        RecordedLiquidityModel::new(fees, DefaultFillModel::new(0.35, 0.7, Some(17)).unwrap());
    let mut official = DefaultFillModel::new(0.35, 0.7, Some(17)).unwrap();
    for _ in 0..20 {
        assert_eq!(
            fill.is_limit_filled().unwrap(),
            official.is_limit_filled().unwrap()
        );
        assert_eq!(fill.is_slipped().unwrap(), official.is_slipped().unwrap());
    }
    assert_eq!(
        preview(&mut fill, &ioc(OrderSide::Buy, "1.000000"))
            .unwrap()
            .best_ask_price(),
        Some(Price::from("0.5001"))
    );
}

fn recorded_settings(request: &mut contracts::science::NativeSimulationRequestV1) {
    request.settings.fill_model = serde_json::from_value(serde_json::json!({
        "schema_version":1,"adapter_kind":"POLYMARKET_V2_RECORDED_LIQUIDITY",
        "native_version":"0.63.0",
        "parameters":{"prob_fill_on_limit":"1","prob_slippage":"1","random_seed":"17"}
    }))
    .unwrap();
    request.target_points[0].targets[0].weight = "0.05".parse().unwrap();
    request.target_points[0].cash_weight = "0.95".parse().unwrap();
}

#[test]
fn native_oversized_ioc_is_partial_with_one_adverse_tick_and_exact_native_accounting() {
    let (directory, data, mut request) = native_scenario();
    recorded_settings(&mut request);
    #[cfg(feature = "native-paper")]
    {
        let error = crate::simulation::paper_target_strategy(
            request.settings.clone(),
            vec![],
            vec![data.series[0].bar_type],
            request.target_points[0].clone(),
            nautilus_model::identifiers::StrategyId::from("RECORDED-001"),
            nautilus_model::identifiers::ClientId::from("RECORDED-TEST"),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("RECORDED_FEES_BACKTEST_ONLY"));
    }
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.unwrap();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(report.fills[0].fill_quantity, "10".parse().unwrap());
    assert_eq!(report.fills[1].fill_quantity, "10".parse().unwrap());
    assert_eq!(report.fills[0].fill_price, "0.5001".parse().unwrap());
    assert_eq!(report.fills[1].fill_price, "0.4999".parse().unwrap());
    assert!(report
        .fills
        .iter()
        .all(|fill| { fill.bar_end_ns.get() == fill.fill_ns.get() / MINUTE_NS * MINUTE_NS }));
    let allocated: BigDecimal = report
        .fills
        .iter()
        .map(|fill| fill.commission.as_decimal())
        .sum();
    assert!(allocated > BigDecimal::from(0));
    let charged = Money::from_str(
        result.canonical_result["accounts"][0]["Margin"]["base"]["commissions"]["pUSD"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        allocated,
        BigDecimal::from_str(&charged.as_decimal().to_string()).unwrap()
    );
    let balance = result
        .summary
        .iter()
        .find(|(key, _)| key.ends_with(".balance.pUSD.total"))
        .unwrap()
        .1;
    let balance = Money::from_str(balance).unwrap();
    assert_eq!(
        BigDecimal::from_str(&balance.as_decimal().to_string()).unwrap(),
        BigDecimal::from(3000) - allocated - BigDecimal::from_str("0.002").unwrap()
    );
    assert!(result
        .summary
        .iter()
        .any(|(k, v)| k == "orders.open" && v == "0"));
    assert!(result
        .summary
        .iter()
        .any(|(k, v)| k == "orders.inflight" && v == "0"));
    // The legacy policy still refuses its synthetic oversized fill. No old
    // request silently acquires the new IOC/source-liquidity semantics.
    request.settings.fill_model = serde_json::from_value(serde_json::json!({
        "schema_version":1,"adapter_kind":"NAUTILUS_DEFAULT_FILL",
        "upstream_class":"nautilus_execution::models::fill::DefaultFillModel",
        "upstream_version":"0.63.0",
        "parameters":{"prob_fill_on_limit":"1","prob_slippage":"1","random_seed":"17"}
    }))
    .unwrap();
    assert!(format!(
        "{:#}",
        crate::simulation::simulate(directory.path(), &request).unwrap_err()
    )
    .contains("RECORDED_FEE_QUANTITY_EXHAUSTED"));
}

#[test]
fn native_swallowed_liquidity_callback_is_observed_without_default_fallback() {
    use nautilus_backtest::{
        config::{BacktestEngineConfig, SimulatedVenueConfig},
        engine::BacktestEngine,
    };
    use nautilus_common::{actor::DataActor, logging::logger::LoggerConfig};
    use nautilus_model::{
        data::{Bar, BarType, Data},
        enums::{AccountType, OmsType},
        identifiers::StrategyId,
    };
    use nautilus_trading::{
        nautilus_strategy,
        strategy::{Strategy, StrategyConfig, StrategyCore},
    };
    #[derive(Debug)]
    struct DoubleIoc {
        core: StrategyCore,
        kind: BarType,
        submitted: bool,
    }
    nautilus_strategy!(DoubleIoc, {});
    impl DataActor for DoubleIoc {
        fn on_start(&mut self) -> Result<()> {
            self.subscribe_bars(self.kind, None, None);
            Ok(())
        }
        fn on_bar(&mut self, _bar: &Bar) -> Result<()> {
            if !self.submitted {
                self.submitted = true;
                for size in ["10.000000", "1.000000"] {
                    let id = self.kind.instrument_id();
                    let order = self.order().market(
                        id,
                        OrderSide::Buy,
                        Quantity::from(size),
                        Some(TimeInForce::Ioc),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    );
                    self.submit_order(order, None, None, None)?;
                }
            }
            Ok(())
        }
    }
    let (directory, data, _) = native_scenario();
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .build()
            .unwrap(),
        bypass_logging: true,
        shutdown_on_error: true,
        ..Default::default()
    })
    .unwrap();
    let fees =
        RecordedFeeModel::load(directory.path(), &data, engine.kernel().clock.clone()).unwrap();
    let fill = RecordedLiquidityModel::new(fees.clone(), DefaultFillModel::default());
    let observed = fill.observer();
    let instrument = &data.series[0].instrument;
    engine
        .add_venue(
            SimulatedVenueConfig::builder()
                .venue(instrument.venue())
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Margin)
                .book_type(BookType::L1_MBP)
                .base_currency(instrument.quote_currency())
                .default_leverage(Decimal::ONE)
                .starting_balances(vec![Money::from("3000 pUSD")])
                .fee_model(fees.handle())
                .fill_model(nautilus_execution::models::fill::FillModelHandle::new(fill))
                .bar_execution(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    engine.add_instrument(instrument).unwrap();
    engine
        .add_strategy(DoubleIoc {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from("DOUBLE-001")),
                ..Default::default()
            }),
            kind: data.series[0].bar_type,
            submitted: false,
        })
        .unwrap();
    engine
        .add_data(
            data.series[0].bars.iter().copied().map(Data::Bar).collect(),
            None,
            true,
            true,
        )
        .unwrap();
    // Upstream logs and swallows the second callback failure. This is precisely
    // why the production wrapper must check its shared observer after run.
    engine.run(None, None, None, false).unwrap();
    assert!(observed
        .check()
        .unwrap_err()
        .to_string()
        .contains("RECORDED_LIQUIDITY_EXHAUSTED"));
    let report = fees.report();
    assert_eq!(report.fills.len(), 1);
    assert_eq!(report.fills[0].fill_quantity, "10".parse().unwrap());
    engine.dispose();
}

#[test]
fn native_partial_reduction_cancels_the_remainder_without_assuming_a_full_exit() {
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    let (directory, mut data, mut request) = native_scenario();
    recorded_settings(&mut request);
    let path = directory.path().join(RECORDED_FEES_FILE);
    let mut source: RecordedTradeFeesV1 =
        serde_json::from_reader(std::fs::File::open(&path).unwrap()).unwrap();
    for record in &mut source.records {
        if record.ts_event_ns.get() >= 3 * MINUTE_NS {
            record.quantity = "1".parse().unwrap();
            record.source_quantity = "1".parse().unwrap();
            record.usdc_amount = "0.5".parse().unwrap();
        }
    }
    std::fs::write(path, serde_json::to_vec(&source).unwrap()).unwrap();
    for bar in &mut data.series[0].bars {
        if bar.ts_init.as_u64() >= 4 * MINUTE_NS {
            bar.volume = Quantity::from("1.000000");
        }
    }
    std::fs::remove_dir_all(directory.path().join("data/bars")).unwrap();
    ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap()
    .write_to_parquet(&data.series[0].bars, None, None, None)
    .unwrap();
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.unwrap();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(report.fills[0].fill_quantity, "10".parse().unwrap());
    assert_eq!(report.fills[1].fill_quantity, "1".parse().unwrap());
    assert_eq!(report.fills[1].order_side, "SELL");
    assert!(result
        .summary
        .iter()
        .any(|(k, v)| k == "orders.open" && v == "0"));
    assert!(result
        .summary
        .iter()
        .any(|(k, v)| k == "orders.inflight" && v == "0"));
}

#[test]
fn native_partial_ioc_cancellation_resumes_the_due_consecutive_bar_target() {
    let (directory, _, mut request) = native_scenario();
    recorded_settings(&mut request);
    request.target_points[1].asof_ns = DbCounter::new(2 * MINUTE_NS).unwrap();
    // Keep the next target due while the first IOC is settling; the new exact
    // latency wakeup would otherwise settle a 1ns order before the next BAR.
    request.settings.latency_model = serde_json::from_value(serde_json::json!({
        "schema_version":1,"adapter_kind":"NAUTILUS_STATIC_LATENCY",
        "upstream_class":"nautilus_execution::models::latency::StaticLatencyModel",
        "upstream_version":"0.63.0",
        "parameters":{"base_latency_ns":"0","insert_latency_ns":"60000000000","update_latency_ns":"0","cancel_latency_ns":"0"}
    })).unwrap();
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.unwrap();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(report.fills[0].bar_end_ns.get(), 2 * MINUTE_NS);
    assert_eq!(report.fills[1].bar_end_ns.get(), 3 * MINUTE_NS);
    assert_eq!(report.fills[0].fill_quantity, "10".parse().unwrap());
    assert_eq!(report.fills[1].fill_quantity, "10".parse().unwrap());
}

#[test]
fn native_ioc_fills_at_declared_positive_latency_not_the_next_bar() {
    let (directory, _, mut request) = native_scenario();
    recorded_settings(&mut request);
    request.settings.snapshot_interval_ms = 86_400_000;
    request.settings.latency_model = serde_json::from_value(serde_json::json!({
        "schema_version":1,"adapter_kind":"NAUTILUS_STATIC_LATENCY",
        "upstream_class":"nautilus_execution::models::latency::StaticLatencyModel",
        "upstream_version":"0.63.0",
        "parameters":{"base_latency_ns":"0","insert_latency_ns":"1000000","update_latency_ns":"0","cancel_latency_ns":"0"}
    })).unwrap();
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.unwrap();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(report.fills[0].fill_ns.get(), MINUTE_NS + 1_000_000);
    assert_eq!(report.fills[1].fill_ns.get(), 4 * MINUTE_NS + 1_000_000);
    assert_eq!(report.fills[0].bar_end_ns.get(), MINUTE_NS);
    assert_eq!(report.fills[1].bar_end_ns.get(), 4 * MINUTE_NS);
}

#[test]
fn native_ioc_sparse_bars_do_not_settle_on_an_unmatched_daily_snapshot() {
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    let (directory, mut data, mut request) = native_scenario();
    recorded_settings(&mut request);
    let gap = 2 * 86_400_000_000_000;
    for bar in data.series[0].bars.iter_mut().skip(1) {
        bar.ts_init = (bar.ts_init.as_u64() + gap).into();
        bar.ts_event = bar.ts_init;
    }
    let path = directory.path().join(RECORDED_FEES_FILE);
    let mut source: RecordedTradeFeesV1 =
        serde_json::from_reader(std::fs::File::open(&path).unwrap()).unwrap();
    for record in &mut source.records {
        if record.ts_event_ns.get() >= MINUTE_NS {
            record.ts_event_ns = DbCounter::new(record.ts_event_ns.get() + gap).unwrap();
        }
    }
    std::fs::write(path, serde_json::to_vec(&source).unwrap()).unwrap();
    std::fs::remove_dir_all(directory.path().join("data/bars")).unwrap();
    ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap()
    .write_to_parquet(&data.series[0].bars, None, None, None)
    .unwrap();
    let end = DbCounter::new(7 * MINUTE_NS + gap).unwrap();
    request.target_points[1].asof_ns = DbCounter::new(4 * MINUTE_NS + gap).unwrap();
    for point in &mut request.target_points {
        point.valid_until_ns = end;
    }
    request.selection.event_end_ns = end;
    request.selection.decision_cutoff_ns = end;
    request.settings.snapshot_interval_ms = 86_400_000;
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.unwrap();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(report.fills[0].fill_ns.get(), MINUTE_NS + 1);
    assert_eq!(report.fills[1].fill_ns.get(), 4 * MINUTE_NS + gap + 1);
    assert_eq!(report.fills[0].allocations[0].original.id, "a");
    assert_eq!(report.fills[1].bar_end_ns.get(), 4 * MINUTE_NS + gap);
    assert!(report
        .fills
        .iter()
        .all(|fill| fill.fill_quantity == "10".parse().unwrap()));
}

#[test]
fn native_final_bar_reduction_settles_before_the_running_strategy_stops() {
    let (directory, _, mut request) = native_scenario();
    recorded_settings(&mut request);
    request.settings.snapshot_interval_ms = 86_400_000;
    request.target_points[1].asof_ns = DbCounter::new(6 * MINUTE_NS).unwrap();
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.unwrap();
    assert_eq!(report.fills.len(), 2);
    assert_eq!(report.fills[1].order_side, "SELL");
    assert_eq!(report.fills[1].fill_ns.get(), 6 * MINUTE_NS + 1);
    assert_eq!(report.fills[1].bar_end_ns.get(), 6 * MINUTE_NS);
    assert!(report
        .fills
        .iter()
        .all(|fill| fill.fill_ns.get() < request.selection.event_end_ns.get()));
    assert!(result
        .summary
        .iter()
        .any(|(k, v)| k == "orders.open" && v == "0"));
    assert!(result
        .summary
        .iter()
        .any(|(k, v)| k == "orders.inflight" && v == "0"));
}

#[test]
fn native_open_position_does_not_add_daily_zero_returns_in_an_empty_tail() {
    let (directory, _, mut request) = native_scenario();
    recorded_settings(&mut request);
    let end = DbCounter::new(3 * 86_400_000_000_000).unwrap();
    request.selection.event_end_ns = end;
    request.selection.decision_cutoff_ns = end;
    for point in &mut request.target_points {
        point.valid_until_ns = end;
    }
    request.target_points[1].asof_ns = DbCounter::new(6 * MINUTE_NS).unwrap();
    request.target_points[1].targets[0].weight = "0.05".parse().unwrap();
    request.target_points[1].cash_weight = "0.95".parse().unwrap();
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    assert_eq!(result.recorded_fee_replay.as_ref().unwrap().fills.len(), 2);
    assert!(
        result
            .returns
            .iter()
            .all(|point| point.timestamp_ns.get() <= 6 * MINUTE_NS + 1),
        "empty tail produced returns: {:?}",
        result.returns
    );
}

#[test]
fn recorded_fees_reject_receive_event_clock_divergence_before_execution() {
    let (directory, mut data, _) = native_scenario();
    data.series[0].bars[0].ts_init = (8 * MINUTE_NS).into();
    let source: RecordedTradeFeesV1 = serde_json::from_reader(
        std::fs::File::open(directory.path().join(RECORDED_FEES_FILE)).unwrap(),
    )
    .unwrap();
    let (_, clock) = model();
    let error = RecordedFeeModel::new(source, &data, clock).unwrap_err();
    assert!(error
        .to_string()
        .contains("RECORDED_FEE_BAR_CLOCK_MISMATCH"));
}

#[test]
fn native_every_instrument_receives_its_final_bar_before_recorded_shutdown() {
    use nautilus_model::{data::BarType, identifiers::Symbol, instruments::InstrumentAny};
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    let (directory, mut data, mut request) = native_scenario();
    recorded_settings(&mut request);
    let id = InstrumentId::from("456.POLYMARKET");
    let kind = BarType::from_str("456.POLYMARKET-1-MINUTE-LAST-EXTERNAL").unwrap();
    let mut second = data.series.remove(0);
    let InstrumentAny::CurrencyPair(pair) = &mut second.instrument else {
        unreachable!();
    };
    pair.id = id;
    pair.raw_symbol = Symbol::new("456");
    second.bar_type = kind;
    for bar in &mut second.bars {
        bar.bar_type = kind;
    }
    let last = second.bars.last_mut().unwrap();
    // Both series end at the same receive time. Only the last-delivered series
    // changes price, after the sole target and its native IOC have settled.
    last.open = Price::from("0.8000");
    last.high = last.open;
    last.low = last.open;
    last.close = last.open;
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    catalog.write_instruments(vec![second.instrument]).unwrap();
    catalog
        .write_to_parquet(&second.bars, None, None, None)
        .unwrap();
    let path = directory.path().join(RECORDED_FEES_FILE);
    let mut source: RecordedTradeFeesV1 =
        serde_json::from_reader(std::fs::File::open(&path).unwrap()).unwrap();
    let second_records = source
        .records
        .iter()
        .cloned()
        .map(|mut record| {
            record.id = format!("second-{}", record.id);
            record.instrument_id = id.to_string();
            if record.ts_event_ns.get() >= 5 * MINUTE_NS {
                record.price = "0.8".parse().unwrap();
                record.usdc_amount = "8".parse().unwrap();
            }
            record
        })
        .collect::<Vec<_>>();
    source.records.extend(second_records);
    std::fs::write(path, serde_json::to_vec(&source).unwrap()).unwrap();
    request.selection.bar_types.push(kind.to_string());
    request.selection.maximum_rows = 12;
    request.target_points.truncate(1);
    let point = &mut request.target_points[0];
    let mut target = point.targets[0].clone();
    point.targets[0].weight = "0".parse().unwrap();
    target.instrument_id = id.to_string();
    point.targets.push(target);
    request
        .settings
        .fee_rates
        .push(contracts::science::NativeFeeRateV1 {
            instrument_id: id.to_string(),
            maker: "0".parse().unwrap(),
            taker: "0".parse().unwrap(),
        });
    let result = crate::simulation::simulate(directory.path(), &request).unwrap();
    let report = result.recorded_fee_replay.as_ref().unwrap();
    assert_eq!(report.fills.len(), 1);
    assert_eq!(report.fills[0].instrument_id, id.to_string());
    assert_eq!(report.fills[0].fill_quantity, "10".parse().unwrap());
    assert_eq!(result.consumed_target_points.get(), 1);
    assert_eq!(result.iterations.get(), 12, "a terminal BAR was omitted");
    let snapshot = result.canonical_result["portfolio_snapshots"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let equity = snapshot["total_equity"][0].as_str().unwrap();
    assert_eq!(
        Money::from_str(equity).unwrap().as_decimal(),
        Decimal::from_str("3002.839").unwrap()
    );
}
