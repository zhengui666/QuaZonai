//! Synthetic native-engine regressions; these are not historical-market evidence.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
#[path = "support/polymarket.rs"]
mod prediction;

use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::{
    actor::{DataActor, DataActorNative},
    logging::logger::LoggerConfig,
};
use nautilus_execution::models::{
    fee::FeeModelHandle,
    fill::{DefaultFillModel, FillModelHandle},
};
use nautilus_model::{
    accounts::Account,
    data::{Bar, BarType, Data, InstrumentClose, QuoteTick},
    enums::{AccountType, BookType, InstrumentCloseType, OmsType, OrderSide, OrderStatus},
    events::OrderFilled,
    identifiers::{ClientOrderId, InstrumentId, StrategyId},
    instruments::{Instrument, InstrumentAny},
    orders::Order,
    types::{Currency, Money, Price, Quantity},
};
use nautilus_polymarket::models::PolymarketFeeModel;
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore},
};
use std::{cell::RefCell, rc::Rc, str::FromStr};

const SECOND: u64 = 1_000_000_000;

#[derive(Debug, Clone, PartialEq)]
struct State {
    order_id: ClientOrderId,
    status: OrderStatus,
    filled: Quantity,
    leaves: Quantity,
    position_id: String,
    position: Quantity,
    total: Money,
    free: Money,
    locked: Money,
    fills: usize,
}

#[derive(Default)]
struct Observations {
    partial: Option<State>,
    completed: Option<State>,
    fills: Vec<OrderFilled>,
}

struct RestingOrder {
    core: StrategyCore,
    instrument_id: InstrumentId,
    bar_type: BarType,
    submitted: bool,
    observations: Rc<RefCell<Observations>>,
}

impl std::fmt::Debug for RestingOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestingOrder").finish_non_exhaustive()
    }
}

impl RestingOrder {
    fn state(&self) -> State {
        let cache = self.cache_ref();
        let order = cache.order(&ClientOrderId::from("PERSIST-001")).unwrap();
        let positions = cache.positions_open(None, Some(&self.instrument_id), None, None, None);
        assert_eq!(positions.len(), 1);
        let account = cache.account_for_venue(&self.instrument_id.venue).unwrap();
        State {
            order_id: order.client_order_id(),
            status: order.status(),
            filled: order.filled_qty(),
            leaves: order.leaves_qty(),
            position_id: positions[0].id.to_string(),
            position: positions[0].quantity,
            total: account.balance_total(None).unwrap(),
            free: account.balance_free(None).unwrap(),
            locked: account.balance_locked(None).unwrap(),
            fills: self.observations.borrow().fills.len(),
        }
    }
}

nautilus_strategy!(RestingOrder, {
    fn on_order_filled(&mut self, event: &OrderFilled) {
        self.observations.borrow_mut().fills.push(event.clone());
    }
});

impl DataActor for RestingOrder {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_quotes(self.instrument_id, None, None);
        self.subscribe_bars(self.bar_type, None, None);
        Ok(())
    }

    fn on_quote(&mut self, quote: &QuoteTick) -> anyhow::Result<()> {
        if !self.submitted {
            self.submitted = true;
            let order = self.order().limit(
                self.instrument_id,
                OrderSide::Buy,
                Quantity::from("10.000000"),
                Price::from("0.5000"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(ClientOrderId::from("PERSIST-001")),
            );
            self.submit_order(order, None, None, None)?;
        } else if quote.ts_init.as_u64() == 2 * SECOND {
            let state = self.state();
            self.observations.borrow_mut().partial = Some(state);
        }
        Ok(())
    }

    fn on_bar(&mut self, _bar: &Bar) -> anyhow::Result<()> {
        let state = self.state();
        self.observations.borrow_mut().completed = Some(state);
        Ok(())
    }
}

fn engine(original: &InstrumentAny) -> BacktestEngine {
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .build()
            .unwrap(),
        shutdown_on_error: true,
        bypass_logging: true,
        ..Default::default()
    })
    .unwrap();
    engine
        .add_venue(
            SimulatedVenueConfig::builder()
                .venue(original.venue())
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Cash)
                .book_type(BookType::L1_MBP)
                .base_currency(Currency::from_str("pUSD").unwrap())
                .starting_balances(vec![Money::from("1000 pUSD")])
                .fee_model(FeeModelHandle::new(PolymarketFeeModel))
                .fill_model(FillModelHandle::new(
                    DefaultFillModel::new(1.0, 0.0, Some(7)).unwrap(),
                ))
                .bar_execution(true)
                .liquidity_consumption(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    engine.add_instrument(original).unwrap();
    engine
}

#[test]
fn upstream_engine_preserves_partial_order_cash_position_and_original_settlement() {
    let original = prediction::instruments("0", 5 * SECOND).remove(0);
    let id = original.id();
    let bar_type = BarType::from_str(&format!("{id}-1-SECOND-LAST-EXTERNAL")).unwrap();
    let observations = Rc::new(RefCell::new(Observations::default()));
    let mut engine = engine(&original);
    engine
        .add_strategy(RestingOrder {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from("PRESERVE-001")),
                order_id_tag: Some("001".into()),
                oms_type: Some(OmsType::Netting),
                ..Default::default()
            }),
            instrument_id: id,
            bar_type,
            submitted: false,
            observations: observations.clone(),
        })
        .unwrap();
    let quote = |bid: &str, ask: &str, size: &str, seconds: u64| {
        Data::Quote(QuoteTick::new(
            id,
            Price::from(bid),
            Price::from(ask),
            Quantity::from(size),
            Quantity::from(size),
            (seconds * SECOND).into(),
            (seconds * SECOND).into(),
        ))
    };
    let price = Price::from("0.4900");
    engine
        .add_data(
            vec![
                quote("0.5900", "0.6000", "100.000000", 1),
                quote("0.4900", "0.5000", "4.000000", 2),
                Data::Bar(Bar::new(
                    bar_type,
                    price,
                    price,
                    price,
                    price,
                    Quantity::from("24.000000"),
                    (4 * SECOND).into(),
                    (4 * SECOND).into(),
                )),
                Data::InstrumentClose(InstrumentClose::new(
                    id,
                    Price::from("1.0000"),
                    InstrumentCloseType::ContractExpired,
                    (5 * SECOND).into(),
                    (6 * SECOND).into(),
                )),
            ],
            None,
            false,
            true,
        )
        .unwrap();
    engine.run(None, None, None, false).unwrap();

    let observed = observations.borrow();
    let partial = observed.partial.as_ref().expect("original quote callback");
    assert_eq!(partial.status, OrderStatus::PartiallyFilled);
    assert_eq!(partial.filled, Quantity::from("4.000000"));
    assert_eq!(partial.leaves, Quantity::from("6.000000"));
    assert_eq!(partial.position, Quantity::from("4.000000"));
    assert_eq!(partial.total, Money::from("998 pUSD"));
    assert_eq!(partial.fills, 1);
    let completed = observed.completed.as_ref().expect("original bar callback");
    assert_eq!(completed.order_id, partial.order_id);
    assert_eq!(completed.position_id, partial.position_id);
    assert_eq!(completed.status, OrderStatus::Filled);
    assert_eq!(completed.filled, Quantity::from("10.000000"));
    assert_eq!(completed.leaves, Quantity::from("0.000000"));
    assert_eq!(completed.position, Quantity::from("10.000000"));
    assert_eq!(completed.total, Money::from("995 pUSD"));
    assert_eq!(completed.fills, 2);
    // Native maker fills use the resting limit, even when a later bar crosses it.
    assert_eq!(observed.fills[0].last_px, Price::from("0.5000"));
    assert_eq!(observed.fills[1].last_px, Price::from("0.5000"));
    assert_eq!(observed.fills[1].last_qty, Quantity::from("6.000000"));
    assert_eq!(observed.fills[1].ts_event.as_u64(), 4 * SECOND);
    assert_eq!(observed.fills.len(), 3);
    let settlement = &observed.fills[2];
    assert!(settlement
        .client_order_id
        .as_str()
        .starts_with("EXPIRATION-POLYMARKET-"));
    assert_eq!(settlement.ts_event.as_u64(), 6 * SECOND);
    assert_eq!(settlement.last_qty, Quantity::from("10.000000"));
    assert_eq!(settlement.last_px, Price::from("1.0000"));
    let cache = engine.kernel().cache.borrow();
    assert!(cache
        .positions_open(None, Some(&id), None, None, None)
        .is_empty());
    assert_eq!(
        cache
            .positions_closed(None, Some(&id), None, None, None)
            .len(),
        1
    );
    assert_eq!(
        cache
            .account_for_venue(&id.venue)
            .unwrap()
            .balance_total(None),
        Some(Money::from("1005 pUSD"))
    );
    drop(cache);
    drop(observed);
    engine.dispose();
}

#[test]
fn quazonai_rejects_definition_transitions_but_preserves_catalog_history() {
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;

    // Include a transition at selection-start and one after event-end: late
    // market data and pending orders still depend on the receive-time cutoff.
    for (minute, start) in [(1, 1), (5, 0), (22, 0)] {
        let received = minute * market::INTERVAL_NS;
        let (root, mut request) = market::market("0", 20);
        request.selection.event_start_ns = market::count(start * market::INTERVAL_NS);
        request.selection.decision_cutoff_ns = market::instant(23);
        let catalog =
            ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
                .unwrap();
        let original = catalog.instruments(None, None, None).unwrap().remove(0);
        let mut definition = serde_json::to_value(&original).unwrap();
        definition["CurrencyPair"]["price_increment"] = "0.00010".into();
        definition["CurrencyPair"]["ts_event"] = (received - 1).into();
        definition["CurrencyPair"]["ts_init"] = received.into();
        let update: InstrumentAny = serde_json::from_value(definition).unwrap();
        catalog.write_instruments(vec![update.clone()]).unwrap();
        let data = job::catalog::load_catalog(root.path(), &request.selection).unwrap();
        let series = data
            .series
            .iter()
            .find(|series| series.instrument.id() == original.id())
            .unwrap();
        assert_eq!(series.instrument_updates.len(), 1);
        assert_eq!(
            serde_json::to_value(&series.instrument_updates[0]).unwrap(),
            serde_json::to_value(&update).unwrap()
        );
        assert_eq!(
            series
                .instrument_at(received - 1)
                .unwrap()
                .price_increment(),
            original.price_increment()
        );
        assert_eq!(
            series.instrument_at(received).unwrap().price_increment(),
            update.price_increment()
        );
        assert_eq!(
            job::simulation::simulate(root.path(), &request)
                .unwrap_err()
                .to_string(),
            "SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED"
        );
        let output = native::command(
            &[
                "simulate".as_ref(),
                "--catalog".as_ref(),
                root.path().as_os_str(),
            ],
            &request,
        );
        assert!(!output.status.success());
        assert!(
            output.stdout.is_empty(),
            "unsupported replay must publish no result"
        );
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "QZ_SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED\nSIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED"
        );
    }
}

#[test]
fn stable_window_after_original_transition_uses_the_original_new_definition() {
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;

    let (root, mut request) = market::market("0", 20);
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    let mut versions = Vec::new();
    for original in catalog.instruments(None, None, None).unwrap() {
        let mut value = serde_json::to_value(original).unwrap();
        value["CurrencyPair"]["price_increment"] = "0.00010".into();
        value["CurrencyPair"]["ts_event"] = (2 * market::INTERVAL_NS - 1).into();
        value["CurrencyPair"]["ts_init"] = (2 * market::INTERVAL_NS).into();
        versions.push(serde_json::from_value::<InstrumentAny>(value).unwrap());
    }
    catalog.write_instruments(versions).unwrap();
    request.selection.event_start_ns = market::count(3 * market::INTERVAL_NS);
    request.target_points[0].asof_ns = market::instant(4);
    let data = job::catalog::load_catalog(root.path(), &request.selection).unwrap();
    assert!(data
        .series
        .iter()
        .all(|series| series.instrument_updates.is_empty()
            && series.instrument.ts_init().as_u64() == 2 * market::INTERVAL_NS
            && series.instrument.price_increment() == Price::from("0.00010")));
    let output = native::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
        ],
        &request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: contracts::science::NativeSimulationResultV1 =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result.consumed_target_points.get(),
        request.target_points.len() as u64
    );
    assert_eq!(result.native_version, "0.63.0");
}
