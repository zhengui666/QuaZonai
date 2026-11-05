//! Synthetic engineering inputs, never research qualification or real performance.
//! No HTTP service, database, exchange account or external feed is involved.
//! The test replaces the data factory, never the native target strategy, Sandbox
//! execution client, matching engine, portfolio or retained account observer.

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use contracts::{
    account_observation::{AccountConnectionV1, AccountObservationSubmitV1, NativeAccountTypeV1},
    forward::ForwardEnvironmentV1,
    portfolio::{
        NAUTILUS_EXECUTION_VERSION, NAUTILUS_FEE_CLASS, NAUTILUS_FILL_CLASS, NAUTILUS_LATENCY_CLASS,
    },
    science::{NativeAccountKind, NativeSimulationSettingsV1},
    strategy_portfolio::{HandoffClaimViewV2, TargetPackageEnvelopeV2},
    DbCounter, Id, SchemaV1,
};
use job::{
    account_observer::project_snapshot,
    paper_node::{
        data_config, data_engine_config, preflight_envelope as preflight, PaperConfig,
        PaperDataSource, PaperLatencyModel, PaperNode, PaperState, PaperStatus, DATA_CLIENT,
    },
};
use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::BinanceSpotMarketDataMode,
};
use nautilus_common::{
    cache::CacheView,
    clients::DataClient,
    clock::Clock,
    factories::{ClientConfig, DataClientFactory},
    live::get_data_event_sender,
    messages::{
        data::{SubscribeBars, SubscribeQuotes, UnsubscribeQuotes},
        DataEvent,
    },
    msgbus::{self, TypedHandler},
};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{Bar, Data, QuoteTick},
    enums::OrderSide,
    events::{OrderEventAny, OrderFilled, PortfolioSnapshot},
    identifiers::{AccountId, ClientId, InstrumentId, StrategyId, Venue},
    instruments::{stubs::currency_pair_btcusdt, InstrumentAny},
    types::{Money, Price, Quantity},
};
use serde_json::{json, Value};
use std::{any::Any, cell::RefCell, collections::HashMap, path::Path, rc::Rc, time::Duration};

const INSTRUMENT: &str = "BTCUSDT.BINANCE";
const ACCOUNT: &str = "BINANCE-PAPER01";
const CAPABILITY: &str = "synthetic-paper-acceptance/1";

#[derive(Debug, Default)]
struct FeedState {
    connected: bool,
    connects: usize,
    disconnects: usize,
    quote_subscriptions: Vec<InstrumentId>,
    external_bar_subscriptions: usize,
}

#[derive(Debug)]
struct SyntheticConfig;

impl ClientConfig for SyntheticConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
struct SyntheticFactory(Rc<RefCell<FeedState>>);

impl DataClientFactory for SyntheticFactory {
    fn create(
        &self,
        name: &str,
        config: &dyn ClientConfig,
        _cache: CacheView,
        _clock: Rc<RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn DataClient>> {
        assert_eq!(name, DATA_CLIENT);
        assert!(config.as_any().is::<SyntheticConfig>());
        Ok(Box::new(SyntheticDataClient(self.0.clone())))
    }

    fn name(&self) -> &'static str {
        "QZ synthetic acceptance data"
    }
    fn config_type(&self) -> &'static str {
        "SyntheticConfig"
    }
}

struct SyntheticDataClient(Rc<RefCell<FeedState>>);

#[async_trait(?Send)]
impl DataClient for SyntheticDataClient {
    fn client_id(&self) -> ClientId {
        ClientId::from(DATA_CLIENT)
    }
    fn venue(&self) -> Option<Venue> {
        Some(Venue::from("BINANCE"))
    }
    fn start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn reset(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn dispose(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
    fn is_connected(&self) -> bool {
        self.0.borrow().connected
    }
    fn is_disconnected(&self) -> bool {
        !self.is_connected()
    }

    async fn connect(&mut self) -> anyhow::Result<()> {
        // This uses the same public native event channel as an official adapter.
        // No test reaches into the node or writes to its private cache.
        let instrument = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
        let definition = serde_json::to_value(&instrument)?;
        assert!(definition["CurrencyPair"].get("maker_fee").is_none());
        assert!(definition["CurrencyPair"].get("taker_fee").is_none());
        get_data_event_sender()
            .send(DataEvent::Data(Data::Instrument(Box::new(instrument))))
            .map_err(|_| anyhow::anyhow!("synthetic instrument event channel closed"))?;
        let mut state = self.0.borrow_mut();
        state.connects += 1;
        state.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        let mut state = self.0.borrow_mut();
        state.disconnects += 1;
        state.connected = false;
        Ok(())
    }

    fn subscribe_quotes(&mut self, command: SubscribeQuotes) -> anyhow::Result<()> {
        self.0
            .borrow_mut()
            .quote_subscriptions
            .extend(command.instrument_ids);
        Ok(())
    }

    fn unsubscribe_quotes(&mut self, _command: &UnsubscribeQuotes) -> anyhow::Result<()> {
        Ok(())
    }

    fn subscribe_bars(&mut self, _command: SubscribeBars) -> anyhow::Result<()> {
        self.0.borrow_mut().external_bar_subscriptions += 1;
        anyhow::bail!("synthetic fixture supplies quotes, never external bars")
    }
}

#[derive(Default)]
struct NativeEvents {
    quotes: Vec<QuoteTick>,
    bars: Vec<Bar>,
    fills: Vec<OrderFilled>,
    snapshots: Vec<PortfolioSnapshot>,
}

fn fixture(directory: &Path) -> (PaperConfig, HandoffClaimViewV2) {
    let settings: NativeSimulationSettingsV1 = serde_json::from_value(json!({
        "schema_version": 1, "base_currency": "USDT", "starting_capital": "10000",
        "account_kind": "MARGIN", "leverage": "1", "snapshot_interval_ms": 50,
        "exposure_tolerance": "0.000001",
        "fee_rates": [{"instrument_id": INSTRUMENT, "maker": "0.001", "taker": "0.001"}],
        "fill_model": {
            "schema_version": 1, "adapter_kind": "NAUTILUS_DEFAULT_FILL",
            "upstream_class": NAUTILUS_FILL_CLASS, "upstream_version": NAUTILUS_EXECUTION_VERSION,
            "parameters": {"prob_fill_on_limit": "1", "prob_slippage": "0", "random_seed": "1"}
        },
        "fee_model": {
            "schema_version": 1, "adapter_kind": "NAUTILUS_MAKER_TAKER",
            "upstream_class": NAUTILUS_FEE_CLASS, "upstream_version": NAUTILUS_EXECUTION_VERSION,
            "parameters": {}
        },
        "latency_model": {
            "schema_version": 1, "adapter_kind": "NAUTILUS_STATIC_LATENCY",
            "upstream_class": NAUTILUS_LATENCY_CLASS, "upstream_version": NAUTILUS_EXECUTION_VERSION,
            "parameters": {"base_latency_ns": "0", "insert_latency_ns": "1000000",
                "update_latency_ns": "0", "cancel_latency_ns": "0"}
        }
    })).unwrap();
    let project_id = Id::new();
    let execution_assumptions = serde_json::from_value(json!({
        "id": Id::new(), "project_id": project_id, "input_set_id": Id::new(),
        "dataset_revision_id": Id::new(), "runtime_id": Id::new(),
        "capability_snapshot_artifact_id": Id::new(), "fee_schedule_artifact_id": Id::new(),
        "engine_image_ref": "synthetic-acceptance-only", "venue_capability_ref": CAPABILITY,
        "calendar_version": "synthetic-continuous/1", "settlement_rule_ref": "synthetic-spot/1",
        "cost_assumption_status": "CONSERVATIVE_ASSUMPTION", "settings": settings,
        "bar_liquidity": null, "bar_liquidity_valid_until": null,
        "rolling_liquidity": null, "rolling_liquidity_artifact_id": null,
        "created_at": Utc::now()
    }))
    .unwrap();
    let config = PaperConfig {
        schema_version: SchemaV1,
        project_id,
        downstream_id: Id::new(),
        trader_id: "QZ-PAPER01".into(),
        account_id: ACCOUNT.into(),
        market_capability_version: CAPABILITY.into(),
        execution_assumptions: Some(execution_assumptions),
        paper_latency: PaperLatencyModel::NativeWallClock,
        source: PaperDataSource::BinanceSpotPublic {
            bar_interval_seconds: 1,
        },
        observations_file: directory.join("paper.ndjson"),
    };
    let input: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let now = Utc::now();
    let release = Id::new();
    let candidate = Id::new();
    let mandate = Id::new();
    // REAL is a required input-contract discriminator, not evidence that this
    // fixture passed any Q research, approval, claim transaction or qualification.
    let dataset = Id::new();
    let metadata = Id::new();
    let weights = Id::new();
    let parameters = Id::new();
    let report = Id::new();
    let asof = now.timestamp_nanos_opt().unwrap() as u64;
    let until = (now + ChronoDuration::seconds(60))
        .timestamp_nanos_opt()
        .unwrap() as u64;
    let definition =
        serde_json::to_value(InstrumentAny::CurrencyPair(currency_pair_btcusdt())).unwrap();
    let claim = serde_json::from_value(json!({
        "handoff": {
            "id": Id::new(), "project_id": config.project_id, "candidate_id": candidate,
            "mandate_id": mandate, "release_id": release, "approval_id": Id::new(),
            "downstream_id": config.downstream_id, "environment": "PAPER",
            "delivery_sequence": "1", "revision": "1", "state": "CLAIMED",
            "supersedes_handoff_id": null, "external_claim_id": "synthetic-q-claim-1",
            "offered_at": now - ChronoDuration::seconds(30),
            "claimed_at": now - ChronoDuration::seconds(20),
            "expires_at": now - ChronoDuration::seconds(10), "acknowledged_at": null
        },
        "package": {
            "release_id": release, "package_schema_version": "2", "environment_origin": "REAL",
            "source_kind": "FORECAST_EVALUATION",
            "source": { "build_run_id": Id::new(), "build_accepted_attempt_id": Id::new(),
                "build_parameters_artifact_id": parameters, "build_report_artifact_id": report,
                "build_input_set_id": Id::new(), "build_environment": "LIVE",
                "forward_dataset_revision_id": dataset, "forward_metadata_artifact_id": metadata,
                "current_weights_artifact_id": weights },
            "forward_dataset": { "dataset_revision_id": dataset, "native_metadata_artifact_id": metadata,
                "storage_version": "synthetic-test-only/1", "data_kind": "BAR", "partition": "FORWARD",
                "origin": "REAL", "pit_status": "VERIFIED", "revision_policy": "AS_KNOWN_THEN",
                "event_start": now - ChronoDuration::seconds(10), "event_end": now,
                "available_through": now, "row_count": "10",
                "selection": { "schema_version": 1, "bar_types": [format!("{INSTRUMENT}-1-SECOND-LAST-EXTERNAL")],
                    "event_start_ns": (asof - 10_000_000_000).to_string(), "event_end_ns": asof.to_string(),
                    "decision_cutoff_ns": asof.to_string(), "maximum_rows": 10 },
                "instrument_definitions": [definition] },
            "execution_settings": config.execution_assumptions.as_ref().unwrap().settings,
            "current_weights": { "schema_version": 1,
                "source": {"kind": "LAST_TARGET", "candidate_id": Id::new()},
                "asof_ns": asof.to_string(), "available_ns": asof.to_string(), "valid_until_ns": until.to_string(),
                "base_currency": "USDT", "cash_weight": "0.5",
                "weights": [{"instrument_id": INSTRUMENT, "weight": "0.5", "currency": "USDT"}] },
            "project_id": config.project_id, "candidate_id": candidate, "mandate_id": mandate,
            "qualification_refs": [Id::new(), Id::new()], "evaluation_refs": [Id::new()],
            "input_revision_refs": [dataset], "engine_versions": {"synthetic-test": "1"},
            "asof": now, "valid_from": now + ChronoDuration::seconds(6),
            "valid_until": now + ChronoDuration::seconds(60),
            "base_currency": "USDT", "capital_assumption": "10000",
            "current_weights_source": "LAST_TARGET",
            "targets": [{"instrument_id": INSTRUMENT, "target_weight": "0.5", "currency": "USDT"}],
            "cash_weight": "0.5", "constraints_summary": input["constraints"],
            "exposure_tolerance": "0.000001", "cost_assumption_ref": config.execution_assumptions.as_ref().unwrap().id,
            "compatible_market_capabilities": [CAPABILITY],
            "limitations": ["Synthetic engineering fixture; not research or real performance"],
            "provenance_artifact_refs": [parameters, report, weights, metadata]
        }
    })).unwrap();
    (config, claim)
}

fn strategy_claim(config: &PaperConfig, claim: &HandoffClaimViewV2) -> HandoffClaimViewV2 {
    let mut value = serde_json::to_value(claim).unwrap();
    let package = value["package"].as_object_mut().unwrap();
    for field in [
        "environment_origin",
        "qualification_refs",
        "evaluation_refs",
        "current_weights_source",
        "current_weights",
        "forward_dataset",
    ] {
        package.remove(field);
    }
    package.insert("package_schema_version".into(), json!("2"));
    package.insert("source_kind".into(), json!("NATIVE_TARGET_DECISION"));
    package.insert("execution_environment".into(), json!("PAPER"));
    package.insert("source".into(), json!({
        "run_id":Id::new(),"accepted_attempt_id":Id::new(),"report_artifact_id":Id::new(),"alpha_version_ids":[Id::new()],
        "input_provenance":{"dataset_revision_id":Id::new(),"market_data_origin":"SYNTHETIC","pit_status":"VERIFIED","revision_policy":"AS_KNOWN_THEN","feature_artifact_origins":{}}
    }));
    package.insert("account_start".into(), json!({
        "downstream_id":config.downstream_id,"trader_id":config.trader_id,"account_id":config.account_id,
        "base_currency":config.execution_assumptions.as_ref().unwrap().settings.base_currency,"starting_capital":config.execution_assumptions.as_ref().unwrap().settings.starting_capital,
        "execution_assumptions_id":config.execution_assumptions.as_ref().unwrap().id
    }));
    package.insert(
        "execution_settings".into(),
        json!(&config.execution_assumptions.as_ref().unwrap().settings),
    );
    // This fixture tests the target consumer, not a stored research decision.
    // Keep constraints supported by the declared all-cash Paper start.
    let constraints = package.get_mut("constraints_summary").unwrap();
    constraints["max_ex_ante_risk"] = Value::Null;
    constraints["max_participation"] = Value::Null;
    constraints["min_net_exposure"] = json!("0");
    constraints["max_cash_weight"] = json!("1");
    constraints["transaction_costs_ref"] = json!(config.execution_assumptions.as_ref().unwrap().id);
    serde_json::from_value(value).unwrap()
}

#[test]
fn strategy_preflight_binds_full_initial_account_and_execution_settings() {
    let directory = tempfile::tempdir().unwrap();
    let (config, legacy) = fixture(directory.path());
    let claim = strategy_claim(&config, &legacy);
    job::paper_node::preflight_envelope(&config, &claim).unwrap();
    for value in ["", "TRADER", "TRADER-", "-001", "交易者-1"] {
        let mut invalid = config.clone();
        invalid.trader_id = value.into();
        assert!(invalid.validate().is_err(), "invalid trader ID: {value}");
    }
    for value in ["", "BINANCE", "BINANCE-", "-ACCOUNT", "交易所-1"] {
        let mut invalid = config.clone();
        invalid.account_id = value.into();
        assert!(invalid.validate().is_err(), "invalid account ID: {value}");
    }
    assert_eq!(
        serde_json::to_value(&claim).unwrap()["package"]["source"]["input_provenance"]
            ["market_data_origin"],
        "SYNTHETIC"
    );
    for changed in [
        "trader",
        "account",
        "downstream",
        "capital",
        "settings",
        "environment",
    ] {
        let mut wrong = claim.clone();
        let TargetPackageEnvelopeV2::TargetDecision(package) = &mut wrong.package else {
            unreachable!()
        };
        match changed {
            "trader" => package.account_start.trader_id = "OTHER-001".into(),
            "account" => package.account_start.account_id = "BINANCE-OTHER".into(),
            "downstream" => package.account_start.downstream_id = Id::new(),
            "capital" => package.account_start.starting_capital = "10001".parse().unwrap(),
            "settings" => package.execution_settings.snapshot_interval_ms += 1,
            "environment" => package.execution_environment = ForwardEnvironmentV1::Live,
            _ => unreachable!(),
        }
        assert!(
            job::paper_node::preflight_envelope(&config, &wrong).is_err(),
            "{changed}"
        );
    }
    assert!(!config.observations_file.exists());
}

fn source_contract(config: &PaperConfig, claim: &HandoffClaimViewV2) {
    let checked = preflight(config, claim).unwrap();
    assert_eq!(checked.execution_environment, "PAPER_SANDBOX");
    assert_eq!(
        checked.restart_policy,
        "FRESH_ACCOUNT_AND_SESSION_NO_RESTORE"
    );
    assert_eq!(checked.claim_replay_scope, "EXTERNAL_TYPESCRIPT_SERVICE");
    assert_eq!(checked.market_data_source, "BINANCE_SPOT_PUBLIC_JSON");
    assert_eq!(
        checked.fee_basis,
        "explicit simulated venue policy 0.001; not account commission rates"
    );
    assert_eq!(
        checked.market_time_basis,
        "NATIVE_ADAPTER_TIMESTAMPS_WITH_RECEIVE_TIME_FALLBACK_WHEN_EXCHANGE_TIME_ABSENT"
    );
    assert!(!checked.connects_market_data);
    assert_eq!(
        checked.bar_types,
        [format!("{INSTRUMENT}-1-SECOND-MID-INTERNAL")]
    );
    let source = data_config(&checked);
    assert!(matches!(source.environment, BinanceEnvironment::Live));
    assert!(matches!(source.product_type, BinanceProductType::Spot));
    assert!(matches!(
        source.spot_market_data_mode,
        BinanceSpotMarketDataMode::Json
    ));
    assert!(source.api_key.is_none() && source.api_secret.is_none());
    assert!(!source.instrument_provider.load_all);
    assert!(!source.instrument_provider.query_commission_rates);
    assert_eq!(
        source.instrument_provider.load_ids.as_ref().unwrap(),
        &[INSTRUMENT.to_owned()]
    );
    let engine = data_engine_config();
    assert!(!engine.time_bars_build_with_no_updates);
    assert!(engine.time_bars_skip_first_non_full_bar);
    assert!(engine.time_bars_timestamp_on_close && engine.validate_data_sequence);
    let document = serde_json::to_value(config).unwrap();
    assert_eq!(document["paper_latency"], "NATIVE_WALL_CLOCK");
    let mut unknown_source = document.clone();
    unknown_source["source"]["kind"] = json!("ARBITRARY_LIVE_EXECUTION");
    assert!(serde_json::from_value::<PaperConfig>(unknown_source).is_err());
    let mut unexpected_credential = document;
    unexpected_credential["source"]["api_key"] = json!("synthetic-unexpected-field");
    assert!(serde_json::from_value::<PaperConfig>(unexpected_credential).is_err());
    for bad_settings in [
        "cash",
        "leverage",
        "fee",
        "account",
        "assumption_project",
        "assumption_identity",
    ] {
        let mut invalid = config.clone();
        match bad_settings {
            "cash" => {
                invalid
                    .execution_assumptions
                    .as_mut()
                    .unwrap()
                    .settings
                    .account_kind = NativeAccountKind::Cash
            }
            "leverage" => {
                invalid
                    .execution_assumptions
                    .as_mut()
                    .unwrap()
                    .settings
                    .leverage = "2".parse().unwrap()
            }
            "fee" => {
                invalid
                    .execution_assumptions
                    .as_mut()
                    .unwrap()
                    .settings
                    .fee_rates[0]
                    .taker = "0".parse().unwrap()
            }
            "account" => invalid.account_id = "OTHER-PAPER01".into(),
            "assumption_project" => {
                invalid.execution_assumptions.as_mut().unwrap().project_id = Id::new()
            }
            "assumption_identity" => invalid.execution_assumptions.as_mut().unwrap().id = Id::new(),
            _ => unreachable!(),
        }
        // These original owner bindings apply to Native TargetDecision only.
        assert!(
            preflight(&invalid, &strategy_claim(config, claim)).is_err(),
            "{bad_settings}"
        );
    }
    assert!(!config.observations_file.exists());
}

fn quote(marked: bool) -> QuoteTick {
    let init = Utc::now().timestamp_nanos_opt().unwrap() as u64;
    QuoteTick::new(
        InstrumentId::from(INSTRUMENT),
        Price::from(if marked { "109.99" } else { "99.99" }),
        Price::from(if marked { "110.01" } else { "100.01" }),
        Quantity::from("100.000000"),
        Quantity::from("100.000000"),
        UnixNanos::from(init - 1_000_000),
        UnixNanos::from(init),
    )
}

// One test owns the native globals/event thread for the complete lifecycle.
#[tokio::test(flavor = "current_thread")]
async fn claimed_target_reaches_one_native_paper_session_and_retained_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let (config, claim) = fixture(directory.path());
    // This live-clock Paper source has an installed venue policy, not a
    // historical catalog. The actual fill below must still charge 0.001.
    assert!(!directory
        .path()
        .join(contracts::catalogs::MAKER_TAKER_FEES_FILE)
        .exists());
    source_contract(&config, &claim);
    let wire_claim = strategy_claim(&config, &claim);
    let package = domain::delivery::package_delivery(&wire_claim.package);
    job::paper_node::preflight_envelope(&config, &wire_claim).unwrap();
    let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
    let (status, current) = tokio::sync::watch::channel(PaperStatus {
        state: PaperState::Starting,
        target_points_consumed: 0,
        reason_code: None,
        updated_at: Utc::now(),
    });
    let mut stale = wire_claim.clone();
    match &mut stale.package {
        TargetPackageEnvelopeV2::Forecast(package) => {
            package.asof = Utc::now() - ChronoDuration::seconds(30);
            package.valid_from = Utc::now() - ChronoDuration::seconds(20);
            package.valid_until = Utc::now() - ChronoDuration::seconds(10);
        }
        TargetPackageEnvelopeV2::TargetDecision(package) => {
            package.asof = Utc::now() - ChronoDuration::seconds(30);
            package.valid_from = Utc::now() - ChronoDuration::seconds(20);
            package.valid_until = Utc::now() - ChronoDuration::seconds(10);
        }
    }
    let mut future_claim = wire_claim.clone();
    future_claim.handoff.claimed_at = Some(Utc::now() + ChronoDuration::seconds(30));
    future_claim.handoff.expires_at = Utc::now() + ChronoDuration::seconds(40);
    for rejected in [stale, future_claim] {
        assert!(job::paper_node::preflight_envelope(&config, &rejected).is_err());
    }

    let feed = Rc::new(RefCell::new(FeedState::default()));
    let events = Rc::new(RefCell::new(NativeEvents::default()));
    let session = Rc::new(RefCell::new(None::<String>));
    let status = status.clone();
    let stop = stop_receiver.clone();
    let native = async {
        let node = PaperNode::build_with_data_factory_envelope(
            &config,
            &wire_claim,
            Box::new(SyntheticFactory(feed.clone())),
            Box::new(SyntheticConfig),
        )
        .unwrap();
        assert_eq!(
            feed.borrow().connects,
            0,
            "construction must not connect data"
        );
        *session.borrow_mut() = Some(node.session_id());

        let observed = events.clone();
        let quotes =
            TypedHandler::from(move |quote: &QuoteTick| observed.borrow_mut().quotes.push(*quote));
        let observed = events.clone();
        let bars = TypedHandler::from(move |bar: &Bar| observed.borrow_mut().bars.push(*bar));
        let observed = events.clone();
        let orders = TypedHandler::from(move |event: &OrderEventAny| {
            if let OrderEventAny::Filled(fill) = event {
                observed.borrow_mut().fills.push(fill.clone());
            }
        });
        let observed = events.clone();
        let snapshots = TypedHandler::from(move |snapshot: &PortfolioSnapshot| {
            observed.borrow_mut().snapshots.push(snapshot.clone())
        });
        msgbus::subscribe_quotes("data.quotes.*".into(), quotes.clone(), None);
        msgbus::subscribe_bars("data.bars.*".into(), bars.clone(), None);
        msgbus::subscribe_order_events("events.order.*".into(), orders.clone(), None);
        msgbus::subscribe_portfolio_snapshot(
            "events.portfolio.BINANCE-PAPER01".into(),
            snapshots.clone(),
            None,
        );
        let result = node.run(stop, status).await;
        msgbus::unsubscribe_quotes("data.quotes.*".into(), &quotes);
        msgbus::unsubscribe_bars("data.bars.*".into(), &bars);
        msgbus::unsubscribe_order_events("events.order.*".into(), &orders);
        msgbus::unsubscribe_portfolio_snapshot(
            "events.portfolio.BINANCE-PAPER01".into(),
            &snapshots,
        );
        result
    };
    let operator = async {
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
        let mut sent = Vec::new();
        let mut premature_bar = false;
        let mut bars_at_fill = None;
        loop {
            interval.tick().await;
            assert!(
                tokio::time::Instant::now() < deadline,
                "native target did not complete: {:?}",
                current.borrow()
            );
            assert_ne!(current.borrow().state, PaperState::Failed);
            if current.borrow().state != PaperState::Running {
                continue;
            }
            assert!(feed
                .borrow()
                .quote_subscriptions
                .contains(&InstrumentId::from(INSTRUMENT)));
            if Utc::now() < package.valid_from {
                assert!(
                    events.borrow().fills.is_empty(),
                    "future target filled before valid_from"
                );
                premature_bar |= !events.borrow().bars.is_empty();
            }
            let filled = !events.borrow().fills.is_empty();
            if filled {
                let count = *bars_at_fill.get_or_insert(events.borrow().bars.len());
                // Further native bars and marked portfolio snapshots must not
                // replay the target or create another order.
                if events.borrow().bars.len() >= count + 2 {
                    break;
                }
            }
            let input = quote(filled);
            sent.push(input);
            get_data_event_sender()
                .send(DataEvent::Data(Data::Quote(input)))
                .unwrap();
        }
        assert!(
            premature_bar,
            "native bars must be observed while future target waits"
        );
        stop_sender.send_replace(true);
        assert_eq!(
            current.borrow().state,
            PaperState::Running,
            "stop intent is not completed native shutdown"
        );
        sent
    };
    let (native_result, sent) = tokio::time::timeout(Duration::from_secs(35), async {
        tokio::join!(native, operator)
    })
    .await
    .unwrap();
    native_result.unwrap();
    assert_eq!(current.borrow().state, PaperState::Stopped);
    assert_eq!(current.borrow().target_points_consumed, 1);
    assert_eq!(feed.borrow().connects, 1);
    assert!(feed.borrow().disconnects >= 1 && !feed.borrow().connected);
    assert_eq!(feed.borrow().external_bar_subscriptions, 0);

    let captured = events.borrow();
    assert_eq!(
        captured.quotes, sent,
        "source event/availability clocks must survive native dispatch unchanged"
    );
    assert!(captured
        .quotes
        .iter()
        .all(|quote| quote.ts_event < quote.ts_init));
    assert!(captured
        .bars
        .iter()
        .all(|bar| bar.bar_type.is_internally_aggregated() && bar.ts_event <= bar.ts_init));
    assert!(
        captured.bars.iter().all(|bar| {
            bar.ts_event > sent[0].ts_init && bar.ts_event.as_u64() % 1_000_000_000 == 0
        }),
        "native bars must close on their original one-second boundary after new quotes"
    );
    assert_eq!(
        captured.fills.len(),
        1,
        "original native Sandbox must fill exactly one target order"
    );
    let fill = &captured.fills[0];
    assert_eq!(fill.account_id, AccountId::from(ACCOUNT));
    assert_eq!(fill.strategy_id, StrategyId::from("QZ-TARGET-001"));
    assert_eq!(fill.instrument_id, InstrumentId::from(INSTRUMENT));
    assert_eq!(fill.order_side, OrderSide::Buy);
    // Shared TargetReplay sizes 50% of 10,000 USDT at native MID=100.
    // Original Sandbox then matches the buy at the ask and charges native fees.
    assert_eq!(fill.last_qty, Quantity::from("50.000000"));
    assert_eq!(fill.last_px, Price::from("100.01"));
    assert_eq!(fill.commission, Some(Money::from("5.00050000 USDT")));
    assert!(fill.ts_event.as_u64() >= package.valid_from.timestamp_nanos_opt().unwrap() as u64);
    assert!(fill.ts_event.as_u64() < package.valid_until.timestamp_nanos_opt().unwrap() as u64);
    assert!(captured
        .bars
        .iter()
        .any(|bar| bar.ts_init < fill.ts_event && bar.close == Price::from("100.00")));

    let originals: HashMap<_, _> = captured
        .snapshots
        .iter()
        .map(|snapshot| {
            (
                snapshot.event_id.to_string(),
                project_snapshot(snapshot).unwrap(),
            )
        })
        .collect();
    let records: Vec<AccountObservationSubmitV1> =
        std::fs::read_to_string(&config.observations_file)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    let retained: Vec<_> = records
        .iter()
        .filter_map(|record| record.snapshot.as_ref())
        .collect();
    assert!(!originals.is_empty());
    assert_eq!(retained.len(), originals.len());
    assert!(retained
        .iter()
        .any(|snapshot| !snapshot.unrealized_pnls.is_empty()));
    assert!(retained
        .windows(2)
        .any(|pair| pair[0].total_equity != pair[1].total_equity));
    for (index, record) in records.iter().enumerate() {
        domain::account_observation::observation(record).unwrap();
        assert_eq!(record.sequence.get(), index as u64 + 1);
        assert_eq!(record.dropped_events, DbCounter::ZERO);
        assert_eq!(record.binding.environment, ForwardEnvironmentV1::Paper);
        assert_eq!(record.binding.project_id, config.project_id);
        assert_eq!(record.binding.native_trader_id, config.trader_id);
        assert_eq!(record.binding.native_account_id, ACCOUNT);
        assert!(!record.binding.native_session_id.is_empty());
        assert_eq!(
            Some(&record.binding.native_session_id),
            session.borrow().as_ref()
        );
        if let Some(snapshot) = &record.snapshot {
            assert_eq!(snapshot.account_type, NativeAccountTypeV1::Margin);
            assert_eq!(
                snapshot, &originals[&snapshot.event_id],
                "retain original native values and both timestamps"
            );
        }
    }
    assert_eq!(
        records.last().unwrap().connection,
        AccountConnectionV1::Disconnected
    );
    assert!(records.last().unwrap().snapshot.is_none());
}

#[test]
fn forecast_v2_uses_original_bundle_without_owner_economics_or_environment_rewrite() {
    let directory = tempfile::tempdir().unwrap();
    let (mut config, claim) = fixture(directory.path());
    config.execution_assumptions = None;
    let checked = preflight(&config, &claim).unwrap();
    assert!(!checked.execution_supported && !checked.connects_market_data);
    assert_eq!(
        checked.execution_blocker,
        Some("PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED")
    );
    let TargetPackageEnvelopeV2::Forecast(package) = &claim.package else {
        panic!("Forecast V2");
    };
    assert_eq!(package.source.build_environment, ForwardEnvironmentV1::Live);
    assert_eq!(claim.handoff.environment, ForwardEnvironmentV1::Paper);
    assert_eq!(
        package.current_weights.weights[0].weight,
        "0.5".parse().unwrap()
    );
    let before = serde_json::to_value(&claim).unwrap();
    // Returns before a native builder/account/data client exists; original
    // nonzero research weights are never silently replaced by new cash.
    let failure = PaperNode::build_envelope(&config, &claim).err().unwrap();
    assert_eq!(
        failure.to_string(),
        "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED"
    );
    assert_eq!(serde_json::to_value(&claim).unwrap(), before);
    assert!(!config.observations_file.exists());
}

#[test]
fn forecast_v2_preflight_rejects_tampered_frozen_sources_before_native_construction() {
    let directory = tempfile::tempdir().unwrap();
    let (mut config, original) = fixture(directory.path());
    config.execution_assumptions = None;
    for changed in [
        "dataset",
        "metadata",
        "provenance",
        "definitions",
        "current_weights",
        "settings",
        "origin",
        "pit",
        "revision",
        "partition",
        "source",
        "ttl",
    ] {
        let mut claim = original.clone();
        let TargetPackageEnvelopeV2::Forecast(package) = &mut claim.package else {
            panic!("Forecast V2");
        };
        match changed {
            "dataset" => package.source.forward_dataset_revision_id = Id::new(),
            "metadata" => package.source.forward_metadata_artifact_id = Id::new(),
            "provenance" => package.provenance_artifact_refs.clear(),
            "definitions" => package.forward_dataset.instrument_definitions.clear(),
            "current_weights" => package.current_weights.weights[0].currency = "USD".into(),
            "settings" => package.execution_settings.starting_capital = "9999".parse().unwrap(),
            "origin" => package.forward_dataset.origin = contracts::research::DataOrigin::Synthetic,
            "pit" => {
                package.forward_dataset.pit_status = contracts::research::PitStatus::Unverified
            }
            "revision" => {
                package.forward_dataset.revision_policy =
                    contracts::catalogs::DataRevisionPolicy::Unknown
            }
            "partition" => {
                package.forward_dataset.partition = contracts::research::DataPartition::Sealed
            }
            "source" => {
                package.current_weights_source =
                    contracts::portfolio::CandidateWeightsSourceV1::ForwardSnapshot
            }
            "ttl" => package.current_weights.valid_until_ns = DbCounter::new(1).unwrap(),
            _ => unreachable!(),
        }
        assert!(preflight(&config, &claim).is_err(), "{changed}");
    }
    assert!(!config.observations_file.exists());
}
