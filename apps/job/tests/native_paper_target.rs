//! Engineering acceptance only. Local tokens, instruments and quotes below
//! are synthetic fixtures, never research qualification or real performance.
//! QZ_NATIVE_STRATEGY_CLAIM_FILE optionally reads the original SQL claim and
//! assumptions exported by native_research_loop's actual OCI producer test.
//! QZ_NATIVE_PAPER_OBSERVATIONS_FILE optionally exports the unchanged retained
//! stream after every assertion succeeds, for that still-live producer's readback.
//! Only the HTTP control uses loopback; no exchange, account or feed is contacted.
//! The test replaces the data factory, never the native target strategy, Sandbox
//! execution client, matching engine, portfolio or retained account observer.

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use contracts::{
    account_observation::{AccountConnectionV1, AccountObservationSubmitV1, NativeAccountTypeV1},
    delivery::HandoffClaimViewV1,
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
        data_config, data_engine_config, preflight, PaperConfig, PaperDataSource,
        PaperLatencyModel, PaperNode, DATA_CLIENT,
    },
    paper_service::{start_control, PaperState},
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
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::{any::Any, cell::RefCell, collections::HashMap, path::Path, rc::Rc, time::Duration};

const TOKEN: &str = "synthetic-paper-acceptance-only";
const INSTRUMENT: &str = "BTCUSDT.BINANCE";
const ACCOUNT: &str = "BINANCE-PAPER01";
const CAPABILITY: &str = "synthetic-paper-acceptance/1";

/// Exercise the shipped command rather than the acceptance harness's HTTP
/// client. Proxy variables are isolated to the child, never changed process-wide.
#[test]
fn production_loopback_control_ignores_ambient_proxy() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        process::Command,
        time::Instant,
    };
    let directory = tempfile::tempdir().unwrap();
    let credential = directory.path().join("synthetic-control-token");
    std::fs::write(&credential, TOKEN).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let proxy_origin = format!("http://{}", proxy.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut connection = loop {
            match listener.accept() {
                Ok((connection, _)) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "local control request never arrived"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("local acceptance listener: {error}"),
            }
        };
        connection
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut bytes = [0_u8; 1024];
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let read = connection.read(&mut bytes).unwrap();
            assert!(read > 0 && request.len() + read <= 8192);
            request.extend_from_slice(&bytes[..read]);
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("GET /downstream/v1/status HTTP/1.1\r\n"));
        assert!(request
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {TOKEN}")));
        let body = br#"{"state":"idle","source":"local-control-fixture"}"#;
        write!(connection, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        connection.write_all(body).unwrap();
    });
    let output = Command::new(env!("CARGO_BIN_EXE_job"))
        .args(["paper", "status", "--origin", &origin, "--credential-file"])
        .arg(&credential)
        .env("HTTP_PROXY", &proxy_origin)
        .env("http_proxy", &proxy_origin)
        .env("ALL_PROXY", &proxy_origin)
        .env("all_proxy", &proxy_origin)
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "production local control must bypass ambient proxy"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["source"],
        "local-control-fixture"
    );
    server.join().unwrap();
    match proxy.accept() {
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
        Ok(_) => panic!("control credentials were sent to an ambient proxy"),
    }
}

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
        get_data_event_sender()
            .send(DataEvent::Instrument(InstrumentAny::CurrencyPair(
                currency_pair_btcusdt(),
            )))
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
            .push(command.instrument_id);
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

fn fixture(directory: &Path) -> (PaperConfig, HandoffClaimViewV1) {
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
        execution_assumptions,
        paper_latency: PaperLatencyModel::NativeWallClock,
        source: PaperDataSource::BinanceSpotPublic {
            bar_interval_seconds: 1,
        },
        observations_file: directory.join("paper.ndjson"),
        credential_file: directory.join("not-created.test-token"),
        bind: "127.0.0.1:0".parse().unwrap(),
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
            "release_id": release, "package_schema_version": "1", "environment_origin": "REAL",
            "project_id": config.project_id, "candidate_id": candidate, "mandate_id": mandate,
            "qualification_refs": [Id::new(), Id::new()], "evaluation_refs": [Id::new()],
            "input_revision_refs": [Id::new()], "engine_versions": {"synthetic-test": "1"},
            "asof": now, "valid_from": now + ChronoDuration::seconds(6),
            "valid_until": now + ChronoDuration::seconds(60),
            "base_currency": "USDT", "capital_assumption": "10000",
            "current_weights_source": "LAST_TARGET",
            "targets": [{"instrument_id": INSTRUMENT, "target_weight": "0.5", "currency": "USDT"}],
            "cash_weight": "0.5", "constraints_summary": input["constraints"],
            "exposure_tolerance": "0.000001", "cost_assumption_ref": config.execution_assumptions.id,
            "compatible_market_capabilities": [CAPABILITY],
            "limitations": ["Synthetic engineering fixture; not research or real performance"],
            "provenance_artifact_refs": [Id::new()]
        }
    })).unwrap();
    (config, claim)
}

fn strategy_claim(config: &PaperConfig, claim: &HandoffClaimViewV1) -> HandoffClaimViewV2 {
    let mut value = serde_json::to_value(claim).unwrap();
    let package = value["package"].as_object_mut().unwrap();
    for field in [
        "environment_origin",
        "qualification_refs",
        "evaluation_refs",
        "current_weights_source",
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
        "base_currency":config.execution_assumptions.settings.base_currency,"starting_capital":config.execution_assumptions.settings.starting_capital,
        "execution_assumptions_id":config.execution_assumptions.id
    }));
    package.insert(
        "execution_settings".into(),
        json!(&config.execution_assumptions.settings),
    );
    // This fixture tests the target consumer, not a stored research decision.
    // Keep constraints supported by the declared all-cash Paper start.
    let constraints = package.get_mut("constraints_summary").unwrap();
    constraints["max_ex_ante_risk"] = Value::Null;
    constraints["max_participation"] = Value::Null;
    constraints["min_net_exposure"] = json!("0");
    constraints["max_cash_weight"] = json!("1");
    constraints["transaction_costs_ref"] = json!(config.execution_assumptions.id);
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

fn source_contract(config: &PaperConfig, claim: &HandoffClaimViewV1) {
    let checked = preflight(config, claim).unwrap();
    assert_eq!(checked.execution_environment, "PAPER_SANDBOX");
    assert_eq!(
        checked.restart_policy,
        "FRESH_ACCOUNT_AND_SESSION_NO_RESTORE"
    );
    assert_eq!(checked.claim_replay_scope, "CURRENT_PROCESS_ONLY");
    assert_eq!(checked.market_data_source, "BINANCE_SPOT_PUBLIC_JSON");
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
            "cash" => invalid.execution_assumptions.settings.account_kind = NativeAccountKind::Cash,
            "leverage" => invalid.execution_assumptions.settings.leverage = "2".parse().unwrap(),
            "fee" => {
                invalid.execution_assumptions.settings.fee_rates[0].taker = "0".parse().unwrap()
            }
            "account" => invalid.account_id = "OTHER-PAPER01".into(),
            "assumption_project" => invalid.execution_assumptions.project_id = Id::new(),
            "assumption_identity" => invalid.execution_assumptions.id = Id::new(),
            _ => unreachable!(),
        }
        assert!(preflight(&invalid, claim).is_err(), "{bad_settings}");
    }
    assert!(!config.observations_file.exists());
    assert!(!config.credential_file.exists());
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

async fn get(client: &Client, origin: &str, path: &str) -> Value {
    client
        .get(format!("{origin}/downstream/v1/{path}"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

// One test owns the native globals/event thread for the complete lifecycle.
#[tokio::test(flavor = "current_thread")]
async fn claimed_target_reaches_one_native_paper_session_and_retained_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let (mut config, claim) = fixture(directory.path());
    source_contract(&config, &claim);
    let from_sql = std::env::var_os("QZ_NATIVE_STRATEGY_CLAIM_FILE");
    let wire_claim = if let Some(path) = &from_sql {
        // This is the original output of runtime/direct_native_strategy_research_to_claim.
        // Never manufacture source IDs or refresh accepted target/claim clocks.
        let path = std::path::Path::new(path);
        assert!(path.is_absolute());
        let document: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let original: HandoffClaimViewV2 =
            serde_json::from_value(document["claim"].clone()).unwrap();
        let TargetPackageEnvelopeV2::TargetDecision(package) = &original.package else {
            panic!("native SQL producer must export its original V2 claim");
        };
        assert_eq!(
            package.source.input_provenance.market_data_origin,
            contracts::research::DataOrigin::Fixture
        );
        assert_eq!(
            package.source.input_provenance.pit_status,
            contracts::research::PitStatus::Unverified
        );
        assert_eq!(package.source.alpha_version_ids.len(), 1);
        assert_eq!(package.targets.len(), 1);
        assert_eq!(package.targets[0].instrument_id, INSTRUMENT);
        assert_eq!(package.targets[0].target_weight, "0.5".parse().unwrap());
        assert_eq!(package.capital_assumption, "10000".parse().unwrap());
        assert!(package.valid_from <= Utc::now());
        config.project_id = package.project_id;
        config.downstream_id = package.account_start.downstream_id;
        config.trader_id = package.account_start.trader_id.clone();
        config.account_id = package.account_start.account_id.clone();
        config.execution_assumptions =
            serde_json::from_value(document["execution_assumptions"].clone()).unwrap();
        config.market_capability_version =
            config.execution_assumptions.venue_capability_ref.clone();
        assert_eq!(config.account_id, ACCOUNT);
        original
    } else {
        match std::env::var("QZ_TEST_PAPER_PACKAGE_VERSION").as_deref() {
            Ok("2") => strategy_claim(&config, &claim),
            Err(std::env::VarError::NotPresent) | Ok("1") => HandoffClaimViewV2 {
                handoff: claim.handoff.clone(),
                package: TargetPackageEnvelopeV2::Forecast(Box::new(claim.package.clone())),
            },
            _ => panic!("QZ_TEST_PAPER_PACKAGE_VERSION must be 1 or 2"),
        }
    };
    let package = domain::delivery::package_delivery(&wire_claim.package);
    job::paper_node::preflight_envelope(&config, &wire_claim).unwrap();
    let mut control = start_control(
        config.bind,
        TOKEN.as_bytes().to_vec(),
        config.market_capability_version.clone(),
    )
    .await
    .unwrap();
    let origin = format!("http://{}", control.local_addr);
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    let endpoint = format!("{origin}/downstream/v1/targets");

    let unauthorized = client
        .get(format!("{origin}/downstream/v1/capabilities"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let capabilities = get(&client, &origin, "capabilities").await;
    assert_eq!(capabilities["delivery_mode"], "TARGET_ONLY");
    assert_eq!(capabilities["environments"], json!(["PAPER"]));
    assert_eq!(
        capabilities["market_capability_versions"],
        json!([config.market_capability_version])
    );
    assert_eq!(capabilities["accepting_targets"], true);
    assert_eq!(get(&client, &origin, "status").await["state"], "idle");

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
        let response = client
            .post(&endpoint)
            .bearer_auth(TOKEN)
            .json(&rejected)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(control.requests.try_recv().is_err());
        assert_eq!(control.status.borrow().state, PaperState::Idle);
    }

    let feed = Rc::new(RefCell::new(FeedState::default()));
    let events = Rc::new(RefCell::new(NativeEvents::default()));
    let session = Rc::new(RefCell::new(None::<String>));
    let status = control.status.clone();
    let stop = control.stop.clone();
    let native = async {
        let request = control.requests.recv().await.unwrap();
        assert_eq!(
            serde_json::to_value(&request.claim).unwrap(),
            serde_json::to_value(&wire_claim).unwrap()
        );
        let node = PaperNode::build_with_data_factory_envelope(
            &config,
            &request.claim,
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
        status.send_modify(|current| current.native_session_id = Some(node.session_id()));

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
        request.reply.send(Ok(())).unwrap();
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
        let applied = client
            .post(&endpoint)
            .bearer_auth(TOKEN)
            .json(&wire_claim)
            .send()
            .await
            .unwrap();
        assert!(matches!(
            applied.status(),
            StatusCode::OK | StatusCode::ACCEPTED
        ));
        let applied: Value = applied.json().await.unwrap();
        assert_eq!(applied["handoff_id"], json!(wire_claim.handoff.id));
        assert_eq!(applied["release_id"], json!(package.release_id));
        assert_eq!(
            applied["external_claim_id"],
            json!(wire_claim.handoff.external_claim_id)
        );
        assert_eq!(
            get(&client, &origin, "capabilities").await["accepting_targets"],
            false
        );

        let mut interval = tokio::time::interval(Duration::from_millis(20));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
        let mut sent = Vec::new();
        let mut replayed = false;
        let mut premature_bar = false;
        let mut bars_at_fill = None;
        loop {
            interval.tick().await;
            assert!(
                tokio::time::Instant::now() < deadline,
                "native target did not complete: {:?}",
                control.status.borrow()
            );
            assert_ne!(control.status.borrow().state, PaperState::Failed);
            if control.status.borrow().state != PaperState::Running {
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
            if !replayed {
                let replay = client
                    .post(&endpoint)
                    .bearer_auth(TOKEN)
                    .json(&wire_claim)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(replay.status(), StatusCode::OK);
                assert_eq!(
                    replay.json::<Value>().await.unwrap()["native_session_id"],
                    json!(*session.borrow())
                );
                let mut changed = wire_claim.clone();
                match &mut changed.package {
                    TargetPackageEnvelopeV2::Forecast(p) => {
                        p.targets[0].target_weight = "0.25".parse().unwrap()
                    }
                    TargetPackageEnvelopeV2::TargetDecision(p) => {
                        p.targets[0].target_weight = "0.25".parse().unwrap()
                    }
                }
                let conflict = client
                    .post(&endpoint)
                    .bearer_auth(TOKEN)
                    .json(&changed)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(conflict.status(), StatusCode::CONFLICT);
                replayed = true;
            }
            let filled = !events.borrow().fills.is_empty();
            if filled {
                let count = *bars_at_fill.get_or_insert(events.borrow().bars.len());
                // Further native bars and marked portfolio snapshots must not
                // replay the target or create another order after HTTP retry.
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
        if from_sql.is_none() {
            assert!(
                premature_bar,
                "native bars must be observed while future target waits"
            );
        }
        let stopping = client
            .post(format!("{origin}/downstream/v1/stop"))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(stopping.status(), StatusCode::ACCEPTED);
        let stopping: Value = stopping.json().await.unwrap();
        assert_eq!(stopping["stop_requested"], true);
        assert_eq!(
            stopping["state"], "running",
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
    assert_eq!(control.status.borrow().state, PaperState::Stopped);
    assert!(
        control.requests.try_recv().is_err(),
        "retries must not create another native request"
    );
    assert_eq!(feed.borrow().connects, 1);
    assert!(feed.borrow().disconnects >= 1 && !feed.borrow().connected);
    assert_eq!(feed.borrow().external_bar_subscriptions, 0);
    let terminal = get(&client, &origin, "status").await;
    assert_eq!(terminal["state"], "stopped");
    assert_eq!(terminal["target_points_consumed"], 1);
    let replay = client
        .post(&endpoint)
        .bearer_auth(TOKEN)
        .json(&wire_claim)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(replay.json::<Value>().await.unwrap()["state"], "stopped");
    assert!(control.requests.try_recv().is_err());
    control.shutdown.send_replace(true);
    control.task.await.unwrap().unwrap();

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
    assert!(
        !config.credential_file.exists(),
        "test must not create authentication credentials"
    );
    if let Some(path) = std::env::var_os("QZ_NATIVE_PAPER_OBSERVATIONS_FILE") {
        use std::io::Write;
        assert!(
            from_sql.is_some(),
            "joined export requires the original SQL claim"
        );
        let path = Path::new(&path);
        assert!(
            path.is_absolute(),
            "retained output must be an explicit absolute path"
        );
        let bytes = std::fs::read(&config.observations_file).unwrap();
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        output.write_all(&bytes).unwrap();
        output.sync_all().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
