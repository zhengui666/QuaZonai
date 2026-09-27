//! Synthetic native-engine regressions; these are not historical-market evidence.
#[path = "support/polymarket.rs"]
mod prediction;

use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
    instrument_update::InstrumentUpdate,
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
    updated: Option<State>,
    completed: Option<State>,
    update_time: Option<u64>,
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
        self.subscribe_instrument(self.instrument_id, None, None);
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

    fn on_instrument(&mut self, instrument: &InstrumentAny) -> anyhow::Result<()> {
        assert_eq!(instrument.price_increment(), Price::from("0.0010"));
        assert_eq!(instrument.price_precision(), 4);
        assert_eq!(instrument.size_precision(), 6);
        assert_eq!(instrument.ts_init().as_u64(), 3 * SECOND);
        let cached = self
            .cache_ref()
            .instrument(&self.instrument_id)
            .unwrap()
            .clone();
        // Native InstrumentAny equality compares IDs only; compare the definition too.
        assert_eq!(
            serde_json::to_value(cached).unwrap(),
            serde_json::to_value(instrument).unwrap()
        );
        let state = self.state();
        let mut observations = self.observations.borrow_mut();
        observations.updated = Some(state);
        observations.update_time = Some(self.clock().timestamp_ns().as_u64());
        Ok(())
    }

    fn on_bar(&mut self, _bar: &Bar) -> anyhow::Result<()> {
        let state = self.state();
        self.observations.borrow_mut().completed = Some(state);
        Ok(())
    }
}

fn instrument(index: usize, tick: &str, event: u64, received: u64) -> InstrumentAny {
    let mut instrument = prediction::instruments("0", 5 * SECOND).remove(index);
    let InstrumentAny::BinaryOption(binary) = &mut instrument else {
        unreachable!()
    };
    binary.price_increment = Price::from(tick);
    binary.ts_event = event.into();
    binary.ts_init = received.into();
    instrument
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
fn native_tick_update_preserves_partial_order_cash_position_and_original_settlement() {
    let original = instrument(0, "0.0100", 0, 0);
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
    let price = Price::from("0.4990");
    engine
        .add_data(
            vec![
                quote("0.5900", "0.6000", "100.000000", 1),
                quote("0.4900", "0.5000", "4.000000", 2),
                InstrumentUpdate(instrument(0, "0.0010", 3 * SECOND - 1, 3 * SECOND)).into(),
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
    // The update itself must neither refill old displayed liquidity nor reset native state.
    assert_eq!(observed.updated.as_ref(), Some(partial));
    assert_eq!(observed.update_time, Some(3 * SECOND));
    let completed = observed.completed.as_ref().expect("original bar callback");
    assert_eq!(completed.order_id, partial.order_id);
    assert_eq!(completed.position_id, partial.position_id);
    assert_eq!(completed.status, OrderStatus::Filled);
    assert_eq!(completed.filled, Quantity::from("10.000000"));
    assert_eq!(completed.leaves, Quantity::from("0.000000"));
    assert_eq!(completed.position, Quantity::from("10.000000"));
    assert_eq!(completed.total, Money::from("995 pUSD"));
    assert_eq!(completed.fills, 2);
    // Native maker fills use the resting limit, even when the new bar crosses it.
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
fn native_update_rejects_unknown_instrument_and_future_original_event() {
    for (update, reason) in [
        (
            instrument(1, "0.0010", 2 * SECOND, 3 * SECOND),
            "Unknown instrument",
        ),
        (
            instrument(0, "0.0010", 4 * SECOND, 3 * SECOND),
            "Invalid definition time",
        ),
    ] {
        let mut engine = engine(&instrument(0, "0.0100", 0, 0));
        engine
            .add_data(vec![InstrumentUpdate(update).into()], None, false, true)
            .unwrap();
        let error = engine.run(None, None, None, false).unwrap_err();
        assert!(error.to_string().contains(reason), "{error:#}");
        engine.dispose();
    }
}
