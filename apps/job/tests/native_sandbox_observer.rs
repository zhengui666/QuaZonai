//! Engineering fixture only: new synthetic quotes into the original LiveNode
//! dispatch and Sandbox matching engine. No network, credentials or historical data.
use contracts::{account_observation::*, forward::ForwardEnvironmentV1, DbCounter};
use job::{account_observer::project_snapshot, native_node_observer::NativeNodeObserver};
use nautilus_common::{
    actor::DataActor,
    enums::Environment,
    live::get_data_event_sender,
    logging::logger::LoggerConfig,
    messages::DataEvent,
    msgbus::{self, TypedHandler},
};
use nautilus_core::UnixNanos;
use nautilus_live::node::{LiveNode, LiveNodeHandle, NodeRunMode, NodeState};
use nautilus_model::{
    data::{Data, QuoteTick},
    enums::{AccountType, OrderSide},
    events::{AccountState, OrderFilled, PortfolioSnapshot},
    identifiers::{AccountId, ClientId, InstrumentId, StrategyId, TraderId, Venue},
    instruments::{stubs::crypto_perpetual_ethusdt, Instrument, InstrumentAny},
    types::{Currency, Money, Price, Quantity},
};
use nautilus_portfolio::config::PortfolioConfig;
use nautilus_sandbox::{SandboxExecutionClientConfig, SandboxExecutionClientFactory};
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore, StrategyNative},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

#[derive(Debug)]
struct SandboxRoundTrip {
    core: StrategyCore,
    instrument_id: InstrumentId,
    quotes: usize,
    fills: Rc<RefCell<Vec<OrderFilled>>>,
}

nautilus_strategy!(SandboxRoundTrip, {
    fn on_order_filled(&mut self, event: &OrderFilled) {
        self.fills.borrow_mut().push(event.clone());
    }
});

impl DataActor for SandboxRoundTrip {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_quotes(self.instrument_id, None, None);
        Ok(())
    }

    fn on_quote(&mut self, _quote: &QuoteTick) -> anyhow::Result<()> {
        self.quotes += 1;
        let side = match self.quotes {
            1 => OrderSide::Buy,
            3 => OrderSide::Sell,
            _ => return Ok(()),
        };
        let instrument_id = self.instrument_id;
        let order = self.order_factory().market(
            instrument_id,
            side,
            Quantity::from("1.000"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        self.submit_order(order, None, Some(ClientId::new("QZ-SANDBOX")), None)
    }
}

fn binding(node: &LiveNode) -> NativeAccountBindingV1 {
    let fixture: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/native-account-paper-snapshot.json"
    ))
    .unwrap();
    NativeAccountBindingV1 {
        environment: ForwardEnvironmentV1::Paper,
        native_trader_id: node.trader_id().to_string(),
        native_session_id: node.instance_id().to_string(),
        native_account_id: "QZTEST-001".into(),
        ..fixture.binding
    }
}

fn node() -> LiveNode {
    LiveNode::builder(TraderId::from("QZTEST-001"), Environment::Sandbox)
        .unwrap()
        .with_logging(LoggerConfig {
            bypass_logging: true,
            ..Default::default()
        })
        .with_load_state(false)
        .with_save_state(false)
        .with_reconciliation(false)
        .with_timeout_connection(2)
        .with_timeout_portfolio(2)
        .with_delay_post_stop_secs(0)
        .with_delay_shutdown_secs(0)
        .with_portfolio_config(PortfolioConfig {
            snapshot_interval_ms: Some(10),
            ..Default::default()
        })
        .add_simulated_exec_client(
            Some("QZ-SANDBOX".into()),
            Box::new(SandboxExecutionClientFactory::new()),
            Box::new(SandboxExecutionClientConfig {
                account_id: AccountId::from("QZTEST-001"),
                venue: Venue::from("QZTEST"),
                starting_balances: vec![Money::from("10000 USDT")],
                base_currency: Some(Currency::USDT()),
                account_type: AccountType::Margin,
                ..Default::default()
            }),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn quote(instrument_id: InstrumentId, bid: &str, ask: &str) -> QuoteTick {
    let ts =
        UnixNanos::from(u64::try_from(chrono::Utc::now().timestamp_nanos_opt().unwrap()).unwrap());
    QuoteTick::new(
        instrument_id,
        Price::from(bid),
        Price::from(ask),
        Quantity::from("100.000"),
        Quantity::from("100.000"),
        ts,
        ts,
    )
}

async fn drive_quotes(
    handle: LiveNodeHandle,
    instrument_id: InstrumentId,
    observer: &NativeNodeObserver,
    fills: Rc<RefCell<Vec<OrderFilled>>>,
    snapshots: Rc<RefCell<Vec<PortfolioSnapshot>>>,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut ticker = tokio::time::interval(Duration::from_millis(10));
    let mut stage = 0;
    let mut equity_before_mark = None;
    loop {
        ticker.tick().await;
        observer.heartbeat().unwrap();
        if tokio::time::Instant::now() >= deadline {
            handle.stop();
            break;
        }
        if !handle.is_running() {
            continue;
        }
        match stage {
            0 => {
                get_data_event_sender()
                    .send(DataEvent::Data(Data::Quote(quote(
                        instrument_id,
                        "1000.00",
                        "1001.00",
                    ))))
                    .unwrap();
                stage = 1;
            }
            1 if fills.borrow().len() == 1 => {
                // Wait for an original timer snapshot while open before marking up.
                if let Some(snapshot) = snapshots
                    .borrow()
                    .iter()
                    .rev()
                    .find(|s| !s.unrealized_pnls.is_empty())
                {
                    equity_before_mark = Some(snapshot.total_equity.clone());
                    get_data_event_sender()
                        .send(DataEvent::Data(Data::Quote(quote(
                            instrument_id,
                            "1010.00",
                            "1011.00",
                        ))))
                        .unwrap();
                    stage = 2;
                }
            }
            2 if snapshots
                .borrow()
                .last()
                .is_some_and(|s| Some(&s.total_equity) != equity_before_mark.as_ref()) =>
            {
                get_data_event_sender()
                    .send(DataEvent::Data(Data::Quote(quote(
                        instrument_id,
                        "1010.00",
                        "1011.00",
                    ))))
                    .unwrap();
                stage = 3;
            }
            3 if fills.borrow().len() == 2 => {
                handle.stop();
                break;
            }
            _ => {}
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn native_sandbox_lifecycle_fills_and_portfolio_events_reach_retained_stream() {
    let mut node = node();
    let mut instrument = crypto_perpetual_ethusdt();
    instrument.id = InstrumentId::from("ETHUSDT-PERP.QZTEST");
    let instrument_id = instrument.id();
    node.kernel()
        .cache()
        .borrow_mut()
        .add_instrument(InstrumentAny::CryptoPerpetual(instrument))
        .unwrap();
    let fills = Rc::new(RefCell::new(Vec::new()));
    node.add_strategy(SandboxRoundTrip {
        core: StrategyCore::new(StrategyConfig {
            strategy_id: Some(StrategyId::from("QZ-FIXTURE-001")),
            ..Default::default()
        }),
        instrument_id,
        quotes: 0,
        fills: fills.clone(),
    })
    .unwrap();
    let snapshots = Rc::new(RefCell::new(Vec::new()));
    let received = snapshots.clone();
    let native_handler = TypedHandler::from(move |snapshot: &PortfolioSnapshot| {
        received.borrow_mut().push(snapshot.clone())
    });
    msgbus::subscribe_portfolio_snapshot(
        "events.portfolio.QZTEST-001".into(),
        native_handler.clone(),
        None,
    );
    let accounts = Rc::new(RefCell::new(Vec::new()));
    let received = accounts.clone();
    let account_handler =
        TypedHandler::from(move |event: &AccountState| received.borrow_mut().push(event.clone()));
    msgbus::subscribe_account_state(
        "events.account.QZTEST-001".into(),
        account_handler.clone(),
        None,
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sandbox.ndjson");
    let bound = binding(&node);
    let invalid_path = directory.path().join("invalid.ndjson");
    for field in ["trader", "session", "account", "environment"] {
        let mut invalid = bound.clone();
        match field {
            "trader" => invalid.native_trader_id = "OTHER-001".into(),
            "session" => invalid.native_session_id = "wrong-session".into(),
            "account" => invalid.native_account_id = "OTHER-001".into(),
            "environment" => invalid.environment = ForwardEnvironmentV1::Live,
            _ => unreachable!(),
        }
        assert!(NativeNodeObserver::attach(
            &node,
            invalid,
            ClientId::new("QZ-SANDBOX"),
            &invalid_path,
            64
        )
        .is_err());
        assert!(!invalid_path.exists());
    }
    let observer =
        NativeNodeObserver::attach(&node, bound.clone(), ClientId::new("QZ-SANDBOX"), &path, 64)
            .unwrap();
    observer.heartbeat().unwrap();
    let handle = node.handle();
    let (result, ()) = tokio::join!(
        node.run_with_mode(NodeRunMode::Hosted),
        drive_quotes(
            handle,
            instrument_id,
            &observer,
            fills.clone(),
            snapshots.clone()
        )
    );
    result.unwrap();
    assert_eq!(node.state(), NodeState::Stopped);
    let cursor = observer.finish().unwrap();
    msgbus::unsubscribe_portfolio_snapshot("events.portfolio.QZTEST-001".into(), &native_handler);
    msgbus::unsubscribe_account_state("events.account.QZTEST-001".into(), &account_handler);
    assert_eq!(
        fills.borrow().len(),
        2,
        "original Sandbox must fill both fixture orders"
    );
    assert!(
        !accounts.borrow().is_empty(),
        "original Sandbox must generate its account state"
    );
    assert!(node
        .kernel()
        .cache()
        .borrow()
        .positions_open(None, None, None, Some(&AccountId::from("QZTEST-001")), None)
        .is_empty());
    let native: HashMap<_, _> = snapshots
        .borrow()
        .iter()
        .map(|s| (s.event_id.to_string(), project_snapshot(s).unwrap()))
        .collect();
    assert!(
        native.len() >= 3,
        "initial, open/marked and shutdown snapshots are native events"
    );
    let records: Vec<AccountObservationSubmitV1> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cursor.0.get(), records.len() as u64);
    assert_eq!(cursor.1, DbCounter::ZERO);
    assert!(records
        .iter()
        .any(|r| r.connection == AccountConnectionV1::Connected && r.snapshot.is_none()));
    assert_eq!(
        records.last().unwrap().connection,
        AccountConnectionV1::Disconnected
    );
    let projected: Vec<_> = records.iter().filter_map(|r| r.snapshot.as_ref()).collect();
    assert_eq!(projected.len(), native.len());
    for (i, record) in records.iter().enumerate() {
        assert_eq!(record.binding, bound);
        assert_eq!(record.sequence.get(), i as u64 + 1);
        domain::account_observation::observation(record).unwrap();
        if let Some(snapshot) = &record.snapshot {
            assert_eq!(snapshot, &native[&snapshot.event_id]);
        }
    }
    assert!(projected
        .windows(2)
        .any(|s| s[0].total_equity != s[1].total_equity));
    assert!(NativeNodeObserver::attach(
        &node,
        binding(&node),
        ClientId::new("QZ-SANDBOX"),
        &path,
        64
    )
    .is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn client_bound_observer_uses_actual_native_identity_without_submitting_orders() {
    let mut node = node();
    let project_id = contracts::Id::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client-bound.ndjson");
    let invalid_path = directory.path().join("invalid-client.ndjson");
    assert!(NativeNodeObserver::attach_client_bound(
        &node,
        project_id,
        ClientId::new("MISSING-CLIENT"),
        &invalid_path,
        32,
    )
    .is_err());
    assert!(!invalid_path.exists());
    assert!(NativeNodeObserver::attach_client_bound(
        &node,
        project_id,
        ClientId::new("QZ-SANDBOX"),
        &invalid_path,
        0,
    )
    .is_err());
    assert!(!invalid_path.exists());

    let snapshots = Rc::new(RefCell::new(Vec::<PortfolioSnapshot>::new()));
    let captured = snapshots.clone();
    let handler = TypedHandler::from(move |snapshot: &PortfolioSnapshot| {
        captured.borrow_mut().push(snapshot.clone());
    });
    msgbus::subscribe_portfolio_snapshot(
        "events.portfolio.QZTEST-001".into(),
        handler.clone(),
        None,
    );
    let observer = NativeNodeObserver::attach_client_bound(
        &node,
        project_id,
        ClientId::new("QZ-SANDBOX"),
        &path,
        64,
    )
    .unwrap();
    observer.heartbeat().unwrap();
    let handle = node.handle();
    let (result, ()) = tokio::join!(node.run_with_mode(NodeRunMode::Hosted), async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut ticker = tokio::time::interval(Duration::from_millis(10));
        loop {
            ticker.tick().await;
            observer.heartbeat().unwrap();
            if (handle.is_running() && !snapshots.borrow().is_empty())
                || tokio::time::Instant::now() >= deadline
            {
                handle.stop();
                break;
            }
        }
    });
    result.unwrap();
    let cursor = observer.finish().unwrap();
    msgbus::unsubscribe_portfolio_snapshot("events.portfolio.QZTEST-001".into(), &handler);
    let native: HashMap<_, _> = snapshots
        .borrow()
        .iter()
        .map(|s| (s.event_id.to_string(), project_snapshot(s).unwrap()))
        .collect();
    assert!(
        !native.is_empty(),
        "official Sandbox account snapshots must be observed"
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let records: Vec<AccountObservationSubmitV2> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cursor.0.get(), records.len() as u64);
    assert_eq!(cursor.1, DbCounter::ZERO);
    assert!(records.iter().any(|r| r.observation.snapshot.is_some()));
    for (index, record) in records.iter().enumerate() {
        let original = &record.observation;
        domain::account_observation::client_observation(record).unwrap();
        assert_eq!(record.native_client_id, "QZ-SANDBOX");
        assert_eq!(original.binding.project_id, project_id);
        assert_eq!(
            original.binding.native_trader_id,
            node.trader_id().to_string()
        );
        assert_eq!(
            original.binding.native_session_id,
            node.instance_id().to_string()
        );
        assert_eq!(original.binding.native_account_id, "QZTEST-001");
        assert_eq!(original.binding.environment, ForwardEnvironmentV1::Paper);
        assert_eq!(
            original.binding.native_version,
            nautilus_core::consts::NAUTILUS_VERSION_CORE
        );
        assert_eq!(original.sequence.get(), index as u64 + 1);
        if let Some(snapshot) = &original.snapshot {
            assert_eq!(snapshot, &native[&snapshot.event_id]);
        }
    }
    assert!(text
        .lines()
        .all(|line| serde_json::from_str::<AccountObservationSubmitV1>(line).is_err()));
    assert!(
        node.kernel()
            .cache()
            .borrow()
            .orders(None, None, None, None, None)
            .is_empty(),
        "this fixture registers no strategy and submits no order"
    );
}
