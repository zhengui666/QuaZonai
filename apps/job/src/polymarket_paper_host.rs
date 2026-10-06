//! Current public-data Paper acceptance host. HF originals remain research history.
//! The data producer is a separate data-only native process; this process owns
//! one official simulation engine. No exchange execution client is registered.
use anyhow::{anyhow, ensure, Result};
use contracts::{
    account_observation::{NativeAccountBindingV1, NATIVE_ACCOUNT_VERSION},
    catalogs::RuntimeCatalogMetadataV1,
    control::CommandResult,
    data::DatasetView,
    delivery::HandoffStateV1,
    execution_assumptions::ExecutionAssumptionsViewV1,
    forward::ForwardEnvironmentV1,
    portfolio::AllocationTargetV1,
    research::DataOrigin,
    science::NativeTargetPointV1,
    strategy_portfolio::{HandoffClaimViewV2, TargetPackageEnvelopeV2},
    DbCounter, Id, SchemaV1,
};
use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore},
    enums::Environment,
    logging::logger::LoggerConfig,
    messages::system::{QueueStateChanged, SocketState, SocketStateChanged},
    msgbus::{
        self,
        switchboard::{self, MessagingSwitchboard},
        ShareableMessageHandler, TypedHandler,
    },
    nautilus_actor,
};
use nautilus_live::{
    node::{LiveNode, NodeRunMode, NodeState},
    SocketControlFactory,
};
use nautilus_model::{
    data::{BarType, Data, InstrumentClose, InstrumentStatus, QuoteTick, TradeTick},
    enums::MarketStatusAction,
    identifiers::{ClientId, InstrumentId, TraderId, Venue},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_polymarket::{
    config::PolymarketDataClientConfig,
    factories::PolymarketDataClientFactory,
    websocket::{
        client::PolymarketWebSocketClient,
        messages::{MarketWsMessage, PolymarketWsMessage},
    },
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    fmt,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    rc::Rc,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    time::{Duration, Instant},
};

use crate::polymarket_data_probe::{
    public_config_from_proxy_env, public_transport_error, validate_proxy_env_name,
};
use crate::polymarket_streaming_paper::PolymarketStreamingPaper;

const CLIENT: &str = "QZ-POLYMARKET-PAPER-DATA";
const MAX_RECORD: u64 = 2 * 1024 * 1024;
const MAX_INPUT: u64 = 8 * 1024 * 1024;

#[derive(clap::Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}

#[derive(clap::Subcommand)]
enum Operation {
    /// Serve one bounded Polymarket Cash simulation using original frozen inputs.
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
    /// Apply the original claimed V2 target to the existing local service.
    Apply {
        #[arg(long)]
        origin: String,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long)]
        claim: PathBuf,
    },
    /// Read observed lifecycle state; never implies venue-account connectivity.
    Status {
        #[arg(long)]
        origin: String,
        #[arg(long)]
        credential_file: PathBuf,
    },
    /// Request cancellation; termination is confirmed separately in status.
    Stop {
        #[arg(long)]
        origin: String,
        #[arg(long)]
        credential_file: PathBuf,
    },
    /// Use an original accepted Paper claim and frozen catalog with current public data.
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        claim: PathBuf,
        #[arg(long)]
        frozen_metadata: PathBuf,
        #[arg(long)]
        dataset_revision: PathBuf,
        #[arg(long)]
        source_output: PathBuf,
        #[arg(long)]
        report_output: PathBuf,
        #[arg(long)]
        snapshots_output: PathBuf,
        #[arg(long)]
        binding_output: PathBuf,
        #[arg(long, default_value_t = 30)]
        max_seconds: u64,
        /// Explicit existing HTTP(S) proxy environment variable; no automatic selection.
        #[arg(long)]
        proxy_env: Option<String>,
    },
    /// Data-only child entrypoint. Does not create a catalog, account or strategy.
    #[command(hide = true)]
    Source {
        #[arg(long, required = true)]
        instrument_id: Vec<String>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 30)]
        max_seconds: u64,
        #[arg(long)]
        proxy_env: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostConfig {
    schema_version: SchemaV1,
    project_id: Id,
    downstream_id: Id,
    pub(crate) market_capability_version: String,
    execution_assumptions: ExecutionAssumptionsViewV1,
    bar_interval_seconds: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceRecord {
    schema_version: SchemaV1,
    sequence: DbCounter,
    observed_at_ns: DbCounter,
    kind: String,
    payload: Value,
}

fn now_ns() -> Result<DbCounter> {
    let value = chrono::Utc::now()
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow!("PAPER_HOST_CLOCK"))?;
    DbCounter::new(u64::try_from(value)?).map_err(anyhow::Error::msg)
}

pub(crate) fn read_original<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_INPUT,
        "PAPER_HOST_INPUT_LIMIT"
    );
    Ok(serde_json::from_reader(file.take(MAX_INPUT + 1))?)
}

fn new_output(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

pub(crate) fn claim(path: &Path) -> Result<HandoffClaimViewV2> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OriginalClaim {
        Receipt(CommandResult<HandoffClaimViewV2>),
        Body(HandoffClaimViewV2),
    }
    Ok(match read_original(path)? {
        OriginalClaim::Receipt(value) => value.resource,
        OriginalClaim::Body(value) => value,
    })
}

fn prepare(
    config: &HostConfig,
    claim: &HandoffClaimViewV2,
    metadata: &RuntimeCatalogMetadataV1,
    dataset: &DatasetView,
) -> Result<(PolymarketStreamingPaper, Vec<InstrumentId>)> {
    let _ = config.schema_version;
    let TargetPackageEnvelopeV2::TargetDecision(package) = &claim.package else {
        return Err(anyhow!("PAPER_HOST_REQUIRES_ORIGINAL_V2_TARGET_DECISION"));
    };
    let handoff = &claim.handoff;
    let now = chrono::Utc::now();
    ensure!(
        handoff.state == HandoffStateV1::Claimed
            && handoff.environment == ForwardEnvironmentV1::Paper
            && package.execution_environment == ForwardEnvironmentV1::Paper
            && handoff.project_id == config.project_id
            && handoff.project_id == package.project_id
            && handoff.downstream_id == config.downstream_id
            && handoff.release_id == package.release_id
            && handoff.candidate_id == package.candidate_id
            && handoff.mandate_id == package.mandate_id
            && handoff
                .external_claim_id
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            && handoff
                .claimed_at
                .is_some_and(|at| at <= now && at < handoff.expires_at),
        "PAPER_HOST_ORIGINAL_CLAIM_BINDING"
    );
    ensure!(
        package.asof <= package.valid_from
            && package.valid_from < package.valid_until
            && package.valid_until > now
            && package
                .compatible_market_capabilities
                .contains(&config.market_capability_version)
            && package.account_start.downstream_id == config.downstream_id
            && package.account_start.execution_assumptions_id == config.execution_assumptions.id
            && config.execution_assumptions.project_id == config.project_id
            && package.cost_assumption_ref == config.execution_assumptions.id
            && package.base_currency == config.execution_assumptions.settings.base_currency
            && package.capital_assumption == config.execution_assumptions.settings.starting_capital
            && package.exposure_tolerance
                == config.execution_assumptions.settings.exposure_tolerance
            && serde_json::to_value(&package.execution_settings)?
                == serde_json::to_value(&config.execution_assumptions.settings)?,
        "PAPER_HOST_ORIGINAL_EXECUTION_SETTINGS"
    );
    ensure!(
        metadata.origin == DataOrigin::Real
            && package.source.input_provenance.market_data_origin == DataOrigin::Real
            && package
                .source
                .input_provenance
                .feature_artifact_origins
                .values()
                .all(|origin| *origin == DataOrigin::Real),
        "PAPER_HOST_REQUIRES_REAL_FROZEN_SOURCE"
    );
    ensure!(
        dataset.id == package.source.input_provenance.dataset_revision_id
            && package.input_revision_refs.contains(&dataset.id)
            && dataset.native_snapshot_ref == metadata.native_snapshot_ref
            && dataset.storage_version == metadata.storage_version
            && dataset.origin == metadata.origin
            && dataset.pit_status == metadata.pit_status
            && dataset.pit_status == package.source.input_provenance.pit_status
            && dataset.revision_policy == metadata.revision_policy
            && dataset.revision_policy == package.source.input_provenance.revision_policy
            && dataset.available_through == metadata.available_through
            && dataset.event_start == metadata.event_start
            && dataset.event_end == metadata.event_end
            && dataset.row_count == metadata.row_count,
        "PAPER_HOST_ORIGINAL_DATASET_METADATA_BINDING"
    );
    ensure!(
        (1..=60).contains(&config.bar_interval_seconds),
        "PAPER_HOST_BAR_INTERVAL"
    );
    let asof = u64::try_from(
        package
            .valid_from
            .timestamp_nanos_opt()
            .ok_or_else(|| anyhow!("PAPER_HOST_TARGET_CLOCK"))?,
    )?;
    let until = u64::try_from(
        package
            .valid_until
            .timestamp_nanos_opt()
            .ok_or_else(|| anyhow!("PAPER_HOST_TARGET_CLOCK"))?,
    )?;
    let versions =
        domain::catalogs::instrument_versions(&metadata.universe.instrument_definitions)?;
    let mut instruments = Vec::new();
    let mut ids = Vec::new();
    let mut bars = Vec::new();
    let mut unique = BTreeSet::new();
    ensure!(
        (1..=8).contains(&package.targets.len()),
        "PAPER_HOST_SOURCE_SCOPE"
    );
    for target in &package.targets {
        let id: InstrumentId = target.instrument_id.parse()?;
        ensure!(
            id.venue == Venue::from("POLYMARKET") && unique.insert(id),
            "PAPER_HOST_SOURCE_IDENTITY"
        );
        let history = versions
            .get(target.instrument_id.as_str())
            .ok_or_else(|| anyhow!("PAPER_HOST_FROZEN_DEFINITION_MISSING"))?;
        let (class, original) = domain::catalogs::instrument_version_at(history, asof)?;
        ensure!(
            class == "BinaryOption",
            "PAPER_HOST_BINARY_DEFINITION_REQUIRED"
        );
        let mut value = serde_json::Map::new();
        value.insert(class.to_owned(), original.clone());
        instruments.push(serde_json::from_value::<InstrumentAny>(Value::Object(
            value,
        ))?);
        bars.push(BarType::from_str(&format!(
            "{id}-{}-SECOND-MID-INTERNAL",
            config.bar_interval_seconds
        ))?);
        ids.push(id);
    }
    let target = NativeTargetPointV1 {
        schema_version: SchemaV1,
        asof_ns: DbCounter::new(asof).map_err(anyhow::Error::msg)?,
        valid_until_ns: DbCounter::new(until).map_err(anyhow::Error::msg)?,
        targets: package
            .targets
            .iter()
            .map(|target| AllocationTargetV1 {
                instrument_id: target.instrument_id.clone(),
                weight: target.target_weight.clone(),
                currency: target.currency.clone(),
            })
            .collect(),
        cash_weight: package.cash_weight.clone(),
    };
    Ok((
        PolymarketStreamingPaper::new(
            &package.account_start,
            &package.execution_settings,
            &package.constraints_summary,
            target,
            instruments,
            bars,
        )?,
        ids,
    ))
}

struct ProducerState {
    sender: Option<SyncSender<SourceRecord>>,
    sequence: u64,
    dropped: u64,
    encoding_failed: bool,
    gap: bool,
    ready: bool,
    lifecycle_ready: bool,
    lifecycle_complete: bool,
    selected: BTreeSet<InstrumentId>,
    definitions: BTreeSet<InstrumentId>,
    ticks: BTreeSet<InstrumentId>,
}

impl fmt::Debug for ProducerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProducerState")
            .field("sequence", &self.sequence)
            .field("dropped", &self.dropped)
            .finish()
    }
}

impl ProducerState {
    fn emit(&mut self, kind: &str, payload: Value) {
        self.sequence += 1;
        let record = (|| -> Result<_> {
            Ok(SourceRecord {
                schema_version: SchemaV1,
                sequence: DbCounter::new(self.sequence).map_err(anyhow::Error::msg)?,
                observed_at_ns: now_ns()?,
                kind: kind.into(),
                payload,
            })
        })();
        match (record, self.sender.as_ref()) {
            (Ok(record), Some(sender)) => {
                if sender.try_send(record).is_err() {
                    self.dropped += 1;
                    self.gap = true;
                }
            }
            _ => {
                self.dropped += 1;
                self.gap = true;
            }
        }
    }

    fn native(&mut self, kind: &str, id: InstrumentId, value: &impl Serialize) {
        if !self.selected.contains(&id) {
            return;
        }
        if kind == "instrument" {
            self.definitions.insert(id);
        }
        if kind == "quote" || kind == "trade" {
            self.ticks.insert(id);
        }
        match serde_json::to_value(value) {
            Ok(value) => self.emit(kind, value),
            Err(_) => {
                self.encoding_failed = true;
                self.gap = true;
            }
        }
    }
}

struct SourceActor {
    core: DataActorCore,
    ids: Vec<InstrumentId>,
    state: Rc<RefCell<ProducerState>>,
    connected: Box<dyn Fn() -> bool>,
}

impl fmt::Debug for SourceActor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceActor")
            .field("ids", &self.ids)
            .finish()
    }
}
nautilus_actor!(SourceActor);

impl DataActor for SourceActor {
    fn on_start(&mut self) -> Result<()> {
        // Retention is installed independently of this actor's running state.
        for id in self.ids.clone() {
            if let Some(instrument) = self.cache().instrument(&id) {
                self.state
                    .borrow_mut()
                    .native("instrument", id, &instrument);
            }
            self.subscribe_instrument(id, Some(ClientId::from(CLIENT)), None);
        }
        let connected = (self.connected)();
        {
            let mut state = self.state.borrow_mut();
            let ready = connected && state.lifecycle_ready && state.definitions == state.selected;
            state.ready = ready;
            state.gap |= !ready;
            let lifecycle_ready = state.lifecycle_ready;
            state.emit("ready", json!({"data_clients_connected": connected, "definitions_complete": ready, "lifecycle_ready": lifecycle_ready}));
        }
        for id in self.ids.clone() {
            self.subscribe_quotes(id, Some(ClientId::from(CLIENT)), None);
            self.subscribe_trades(id, Some(ClientId::from(CLIENT)), None);
        }
        Ok(())
    }
}

// Message-bus retention outlives SourceActor stopping. The official runner's
// final drain still reaches these handlers before they are explicitly removed.
struct NativeRetention {
    instruments: Vec<(String, TypedHandler<InstrumentAny>)>,
    quotes: Vec<(String, TypedHandler<QuoteTick>)>,
    trades: Vec<(String, TypedHandler<TradeTick>)>,
    other: Vec<(String, ShareableMessageHandler)>,
}

impl NativeRetention {
    fn attach(state: Rc<RefCell<ProducerState>>, ids: &[InstrumentId]) -> Self {
        let mut retained = Self {
            instruments: Vec::new(),
            quotes: Vec::new(),
            trades: Vec::new(),
            other: Vec::new(),
        };
        for id in ids {
            let target = state.clone();
            let handler = TypedHandler::from(move |value: &InstrumentAny| {
                target.borrow_mut().native("instrument", value.id(), value)
            });
            let topic = switchboard::get_instrument_topic(*id).to_string();
            msgbus::subscribe_instruments(topic.as_str().into(), handler.clone(), None);
            retained.instruments.push((topic, handler));
            let target = state.clone();
            let handler = TypedHandler::from(move |value: &QuoteTick| {
                target
                    .borrow_mut()
                    .native("quote", value.instrument_id, value)
            });
            let topic = switchboard::get_quotes_topic(*id).to_string();
            msgbus::subscribe_quotes(topic.as_str().into(), handler.clone(), None);
            retained.quotes.push((topic, handler));
            let target = state.clone();
            let handler = TypedHandler::from(move |value: &TradeTick| {
                target
                    .borrow_mut()
                    .native("trade", value.instrument_id, value)
            });
            let topic = switchboard::get_trades_topic(*id).to_string();
            msgbus::subscribe_trades(topic.as_str().into(), handler.clone(), None);
            retained.trades.push((topic, handler));
            let target = state.clone();
            let handler = ShareableMessageHandler::from_typed(move |value: &InstrumentStatus| {
                target
                    .borrow_mut()
                    .native("status", value.instrument_id, value)
            });
            let topic = switchboard::get_instrument_status_topic(*id).to_string();
            msgbus::subscribe_any(topic.as_str().into(), handler.clone(), None);
            retained.other.push((topic, handler));
            let target = state.clone();
            let handler = ShareableMessageHandler::from_typed(move |value: &InstrumentClose| {
                target
                    .borrow_mut()
                    .native("close", value.instrument_id, value)
            });
            let topic = switchboard::get_instrument_close_topic(*id).to_string();
            msgbus::subscribe_any(topic.as_str().into(), handler.clone(), None);
            retained.other.push((topic, handler));
        }
        let target = state.clone();
        let handler = ShareableMessageHandler::from_typed(move |value: &SocketStateChanged| {
            if value.client_id != ClientId::from(CLIENT) {
                return;
            }
            let mut state = target.borrow_mut();
            // Official deliberate shutdown does not publish Disconnected.
            // A queued real loss remains a gap even when delivered during stop.
            if value.state == SocketState::Disconnected {
                state.gap = true;
            }
            state.emit("socket", json!({
                "trader_id": value.trader_id.to_string(), "client_id": value.client_id.to_string(),
                "venue": value.venue.map(|v| v.to_string()), "endpoint": value.endpoint.to_string(),
                "state": format!("{:?}", value.state), "event_id": value.event_id.to_string(),
                "ts_event": value.ts_event.as_u64().to_string(), "ts_init": value.ts_init.as_u64().to_string(),
            }));
        });
        let topic = MessagingSwitchboard::socket_state_changed_topic().to_string();
        msgbus::subscribe_any(topic.as_str().into(), handler.clone(), None);
        retained.other.push((topic, handler));
        let handler = ShareableMessageHandler::from_typed(move |value: &QueueStateChanged| {
            state.borrow_mut().emit("queue", json!({
                "trader_id": value.trader_id.to_string(), "channel": format!("{:?}", value.channel),
                "condition": format!("{:?}", value.condition), "state": format!("{:?}", value.state),
                "queue_depth": value.queue_depth, "mean_dispatch_ns": value.mean_dispatch_ns.to_string(),
                "event_id": value.event_id.to_string(), "ts_event": value.ts_event.as_u64().to_string(),
                "ts_init": value.ts_init.as_u64().to_string(),
            }));
        });
        let topic = MessagingSwitchboard::queue_state_changed_topic().to_string();
        msgbus::subscribe_any(topic.as_str().into(), handler.clone(), None);
        retained.other.push((topic, handler));
        retained
    }
}

impl Drop for NativeRetention {
    fn drop(&mut self) {
        for (topic, handler) in &self.instruments {
            msgbus::unsubscribe_instruments(topic.as_str().into(), handler);
        }
        for (topic, handler) in &self.quotes {
            msgbus::unsubscribe_quotes(topic.as_str().into(), handler);
        }
        for (topic, handler) in &self.trades {
            msgbus::unsubscribe_trades(topic.as_str().into(), handler);
        }
        for (topic, handler) in &self.other {
            msgbus::unsubscribe_any(topic.as_str().into(), handler);
        }
    }
}

struct LifecycleWatch {
    client: PolymarketWebSocketClient,
    messages: tokio::sync::mpsc::UnboundedReceiver<PolymarketWsMessage>,
    edges: mpsc::Receiver<(String, Option<DbCounter>)>,
    lost: Arc<AtomicBool>,
    tokens: BTreeSet<String>,
    conditions: BTreeSet<String>,
    books: BTreeSet<String>,
    _control: nautilus_live::SocketControl,
}

fn lifecycle_message(
    message: PolymarketWsMessage,
    tokens: &BTreeSet<String>,
    conditions: &BTreeSet<String>,
    books: &mut BTreeSet<String>,
    state: &Rc<RefCell<ProducerState>>,
) -> Result<()> {
    match message {
        PolymarketWsMessage::Market(value) => {
            let mut selected_lifecycle = false;
            match &value {
                MarketWsMessage::Book(book) if tokens.contains(book.asset_id.as_str()) => {
                    if books.insert(book.asset_id.to_string()) {
                        state
                            .borrow_mut()
                            .emit("lifecycle_initial_book", serde_json::to_value(&value)?);
                    }
                }
                MarketWsMessage::MarketResolved(value) => {
                    selected_lifecycle = conditions.contains(value.market.as_str())
                        || value.assets_ids.iter().any(|id| tokens.contains(id));
                }
                MarketWsMessage::TickSizeChange(value) => {
                    selected_lifecycle = tokens.contains(value.asset_id.as_str())
                        && value.old_tick_size.parse::<rust_decimal::Decimal>()?
                            != value.new_tick_size.parse::<rust_decimal::Decimal>()?;
                }
                MarketWsMessage::NewMarket(value) => {
                    selected_lifecycle = conditions.contains(&value.condition_id)
                        || value.assets_ids.iter().any(|id| tokens.contains(id));
                }
                _ => {}
            }
            if selected_lifecycle {
                let mut source = state.borrow_mut();
                source.gap = true;
                source.emit("lifecycle_event", serde_json::to_value(value)?);
            }
        }
        PolymarketWsMessage::Reconnected => {
            let mut source = state.borrow_mut();
            source.gap = true;
            source.emit(
                "lifecycle_reconnected",
                json!({"reason_code": "PUBLIC_LIFECYCLE_RECONNECTED"}),
            );
        }
        PolymarketWsMessage::User(_) => {
            return Err(anyhow!("PAPER_PUBLIC_CHANNEL_RETURNED_USER_EVENT"))
        }
    }
    Ok(())
}

fn lifecycle_edges(
    edges: &mpsc::Receiver<(String, Option<DbCounter>)>,
    lost: &AtomicBool,
    state: &Rc<RefCell<ProducerState>>,
) {
    while let Ok((value, observed)) = edges.try_recv() {
        let mut source = state.borrow_mut();
        if value == "Disconnected" || observed.is_none() {
            source.gap = true;
        }
        source.emit(
            "lifecycle_socket",
            json!({"state": value, "observed_at_ns": observed}),
        );
    }
    if lost.load(Ordering::Acquire) {
        state.borrow_mut().gap = true;
    }
}

impl LifecycleWatch {
    async fn start(
        ids: &[InstrumentId],
        state: &Rc<RefCell<ProducerState>>,
        config: &PolymarketDataClientConfig,
    ) -> Result<Self> {
        state
            .borrow_mut()
            .emit("source_phase", json!({"phase": "lifecycle_start"}));
        let mut tokens = BTreeSet::new();
        let mut conditions = BTreeSet::new();
        for id in ids {
            let (condition, token) = id
                .symbol
                .as_str()
                .rsplit_once('-')
                .ok_or_else(|| anyhow!("PAPER_SOURCE_NATIVE_ID_FORMAT"))?;
            ensure!(
                !condition.is_empty()
                    && !token.is_empty()
                    && token.bytes().all(|value| value.is_ascii_digit()),
                "PAPER_SOURCE_NATIVE_ID_FORMAT"
            );
            conditions.insert(condition.to_owned());
            tokens.insert(token.to_owned());
        }
        let (sender, edges) = mpsc::sync_channel(64);
        let lost = Arc::new(AtomicBool::new(false));
        let callback_lost = lost.clone();
        let control = SocketControlFactory::new(
            ClientId::from("QZ-PAPER-LIFECYCLE"),
            Some(Venue::from("POLYMARKET")),
        )
        .control("polymarket-paper-lifecycle");
        let sink = control.sink_with(move |value| {
            // This inferred enum is nautilus_network::SocketState, not the
            // similarly named system-message enum. Preserve its official value.
            let value = format!("{value:?}");
            if value == "Disconnected" {
                callback_lost.store(true, Ordering::Release);
            }
            if sender.try_send((value, now_ns().ok())).is_err() {
                callback_lost.store(true, Ordering::Release);
            }
        });
        state
            .borrow_mut()
            .emit("source_phase", json!({"phase": "lifecycle_control_ready"}));
        let mut client = PolymarketWebSocketClient::new_market_with_proxy(
            config.base_url_ws.clone(),
            true,
            config.transport_backend,
            config.validated_proxy_url()?,
        )
        .with_state_sink(sink);
        state
            .borrow_mut()
            .emit("source_phase", json!({"phase": "lifecycle_connect_begin"}));
        // Official 0.63 allows 15 seconds for dialing. Give it room to return
        // its own result instead of cancelling that configured budget at 10s.
        let connection = tokio::time::timeout(Duration::from_secs(20), client.connect())
            .await
            .map_err(|_| anyhow!("PAPER_LIFECYCLE_CONNECT_TIMEOUT"))
            .and_then(|result| result);
        if let Err(error) = connection {
            let detail = public_transport_error(&error, config.has_proxy_url());
            let mut source = state.borrow_mut();
            source.gap = true;
            source.emit(
                "gap",
                json!({"phase": "lifecycle_connect", "reason_code": detail}),
            );
            return Err(anyhow!(detail));
        }
        state
            .borrow_mut()
            .emit("source_phase", json!({"phase": "lifecycle_connected"}));
        let messages = client
            .take_message_receiver()
            .ok_or_else(|| anyhow!("PAPER_LIFECYCLE_RECEIVER_MISSING"))?;
        client
            .subscribe_market(tokens.iter().cloned().collect())
            .await?;
        state.borrow_mut().emit(
            "source_phase",
            json!({"phase": "lifecycle_subscription_requested"}),
        );
        let mut watcher = Self {
            client,
            messages,
            edges,
            lost,
            tokens,
            conditions,
            books: BTreeSet::new(),
            _control: control,
        };
        tokio::time::timeout(Duration::from_secs(10), async {
            while watcher.books != watcher.tokens {
                let value = watcher
                    .messages
                    .recv()
                    .await
                    .ok_or_else(|| anyhow!("PAPER_LIFECYCLE_EARLY_EOF"))?;
                lifecycle_message(
                    value,
                    &watcher.tokens,
                    &watcher.conditions,
                    &mut watcher.books,
                    state,
                )?;
                lifecycle_edges(&watcher.edges, &watcher.lost, state);
                ensure!(!state.borrow().gap, "PAPER_LIFECYCLE_START_INCOMPLETE");
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        ensure!(watcher.client.is_active(), "PAPER_LIFECYCLE_NOT_ACTIVE");
        state.borrow_mut().lifecycle_ready = true;
        state.borrow_mut().emit(
            "lifecycle_ready",
            json!({"tokens": watcher.tokens, "initial_books_complete": true}),
        );
        Ok(watcher)
    }

    async fn finish(self, state: &Rc<RefCell<ProducerState>>) -> Result<()> {
        let Self {
            mut client,
            mut messages,
            edges,
            lost,
            tokens,
            conditions,
            mut books,
            _control,
        } = self;
        ensure!(
            client.is_active(),
            "PAPER_LIFECYCLE_LOST_BEFORE_PRIMARY_DRAIN"
        );
        while let Ok(value) = messages.try_recv() {
            lifecycle_message(value, &tokens, &conditions, &mut books, state)?;
        }
        lifecycle_edges(&edges, &lost, state);
        // Disconnect Ok alone is not evidence of drain: the official client can
        // abort its task. Read its actual output channel through EOF as well.
        let shutdown = client.disconnect();
        tokio::pin!(shutdown);
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut ended = false;
            let mut stopped = false;
            while !ended || !stopped {
                tokio::select! {
                    result = &mut shutdown, if !stopped => { result?; stopped = true; }
                    value = messages.recv(), if !ended => match value {
                        Some(value) => lifecycle_message(value, &tokens, &conditions, &mut books, state)?,
                        None => ended = true,
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        }).await??;
        lifecycle_edges(&edges, &lost, state);
        ensure!(
            !lost.load(Ordering::Acquire),
            "PAPER_LIFECYCLE_SOCKET_LOSS_OR_OVERFLOW"
        );
        state.borrow_mut().lifecycle_complete = true;
        state.borrow_mut().emit(
            "lifecycle_coverage",
            json!({
                "primary_native_drain_finished": true, "parsed_message_channel_drained": true,
                "scope": "OBSERVED_NATIVE_AND_PARSED_PUBLIC_EVENTS_NOT_ALL_EXCHANGE_PACKETS",
            }),
        );
        Ok(())
    }
}

fn write_record(file: &mut File, stdout: &mut impl Write, record: &SourceRecord) -> Result<()> {
    let mut bytes = serde_json::to_vec(record)?;
    ensure!(
        bytes.len() as u64 + 1 <= MAX_RECORD,
        "PAPER_SOURCE_RECORD_LIMIT"
    );
    bytes.push(b'\n');
    // Retain first. The parent never simulates a frame absent from its source log.
    file.write_all(&bytes)?;
    file.sync_data()?;
    stdout.write_all(&bytes)?;
    stdout.flush()?;
    Ok(())
}

async fn source(
    ids: Vec<InstrumentId>,
    output: &Path,
    max_seconds: u64,
    proxy_env: Option<&str>,
) -> Result<()> {
    let config = public_config_from_proxy_env(ids.clone(), proxy_env)?;
    let configured_proxy = config.has_proxy_url();
    let file = new_output(output)?;
    let (sender, receiver) = mpsc::sync_channel::<SourceRecord>(4096);
    let writer = std::thread::spawn(move || -> Result<File> {
        let mut file = file;
        let mut stdout = std::io::stdout().lock();
        for record in receiver {
            write_record(&mut file, &mut stdout, &record)?;
        }
        Ok(file)
    });
    let state = Rc::new(RefCell::new(ProducerState {
        sender: Some(sender),
        sequence: 0,
        dropped: 0,
        encoding_failed: false,
        gap: false,
        ready: false,
        lifecycle_ready: false,
        lifecycle_complete: false,
        selected: ids.iter().copied().collect(),
        definitions: BTreeSet::new(),
        ticks: BTreeSet::new(),
    }));
    let mut node = LiveNode::builder(TraderId::from("QZ-PAPER-SOURCE-001"), Environment::Live)?
        .with_logging(LoggerConfig {
            bypass_logging: true,
            ..Default::default()
        })
        .with_load_state(false)
        .with_save_state(false)
        .with_reconciliation(false)
        .with_delay_post_stop_secs(0)
        .with_delay_shutdown_secs(0)
        .add_data_client(
            Some(CLIENT.into()),
            Box::new(PolymarketDataClientFactory),
            Box::new(config.clone()),
        )?
        .build()?;
    let data_engine = node.kernel().data_engine.clone();
    let monitored_data_engine = data_engine.clone();
    let connected = Box::new(move || {
        data_engine
            .try_borrow()
            .is_ok_and(|value| value.check_connected())
    });
    state.borrow_mut().emit(
        "start",
        json!({
            "purpose": "CURRENT_PUBLIC_PAPER_ACCEPTANCE_ONLY",
            "native_session_id": node.instance_id().to_string(),
            "native_version": NATIVE_ACCOUNT_VERSION,
            "proxy_configured": configured_proxy,
            "selected_instruments": ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "execution_clients_registered": 0,
        }),
    );
    let retention = NativeRetention::attach(state.clone(), &ids);
    state.borrow_mut().emit(
        "source_phase",
        json!({"phase": "native_retention_attached"}),
    );
    let mut lifecycle = match LifecycleWatch::start(&ids, &state, &config).await {
        Ok(lifecycle) => lifecycle,
        Err(error) => {
            // Startup failure must drain the actually retained diagnostics too.
            drop(retention);
            node.dispose();
            state.borrow_mut().sender.take();
            writer
                .join()
                .map_err(|_| anyhow!("PAPER_SOURCE_WRITER_PANICKED"))??
                .sync_all()?;
            return Err(error);
        }
    };
    node.add_actor(SourceActor {
        core: DataActorCore::new(DataActorConfig {
            log_events: false,
            log_commands: false,
            ..Default::default()
        }),
        ids,
        state: state.clone(),
        connected,
    })?;
    let handle = node.handle();
    let native_failure = {
        let native = node.run_with_mode(NodeRunMode::Hosted);
        tokio::pin!(native);
        let deadline = tokio::time::sleep(Duration::from_secs(max_seconds));
        tokio::pin!(deadline);
        let mut check = tokio::time::interval(Duration::from_millis(100));
        let mut lifecycle_eof = false;
        loop {
            tokio::select! {
                result = &mut native => break result.err().map(|error| public_transport_error(&error, configured_proxy)),
                _ = &mut deadline => {
                    handle.stop();
                    break match tokio::time::timeout(Duration::from_secs(15), &mut native).await {
                        Ok(result) => result.err().map(|error| public_transport_error(&error, configured_proxy)),
                        Err(_) => Some("PAPER_SOURCE_SHUTDOWN_TIMEOUT".into()),
                    };
                }
                message = lifecycle.messages.recv(), if !lifecycle_eof => {
                    match message {
                        Some(message) => {
                            if let Err(error) = lifecycle_message(message, &lifecycle.tokens, &lifecycle.conditions, &mut lifecycle.books, &state) {
                                let mut state = state.borrow_mut(); state.gap = true;
                                state.emit("gap", json!({"reason_code": error.to_string()})); handle.stop();
                            }
                        }
                        None => {
                            lifecycle_eof = true;
                            let mut state = state.borrow_mut(); state.gap = true;
                            state.emit("gap", json!({"reason_code": "PAPER_LIFECYCLE_EARLY_EOF"})); handle.stop();
                        }
                    }
                }
                _ = check.tick() => {
                    lifecycle_edges(&lifecycle.edges, &lifecycle.lost, &state);
                    if !lifecycle.client.is_active() { state.borrow_mut().gap = true; handle.stop(); }
                    if handle.is_running() && !monitored_data_engine.try_borrow()
                        .is_ok_and(|value| value.check_connected()) {
                        let mut state = state.borrow_mut();
                        state.gap = true;
                        state.emit("gap", json!({"reason_code": "NATIVE_DATA_CLIENT_DISCONNECTED"}));
                        handle.stop();
                    }
                }
            }
        }
    };
    let native_ok = native_failure.is_none();
    let stopped = handle.state() == NodeState::Stopped;
    if let Err(error) = lifecycle.finish(&state).await {
        let mut state = state.borrow_mut();
        state.gap = true;
        state.emit("gap", json!({"reason_code": error.to_string()}));
    }
    // Keep native retention until the actual runner and public watcher drains.
    drop(retention);
    node.dispose();
    let (sequence, complete, final_payload) = {
        let mut state = state.borrow_mut();
        state.sender.take();
        state.sequence += 1;
        let complete = native_ok
            && stopped
            && state.ready
            && state.lifecycle_complete
            && !state.gap
            && !state.encoding_failed
            && state.dropped == 0
            && state.ticks == state.selected;
        (
            state.sequence,
            complete,
            json!({
                "native_returned_success": native_ok, "native_shutdown_confirmed": stopped,
                "native_failure": native_failure.map(|value| value.chars().take(1024).collect::<String>()),
                "ready": state.ready, "lifecycle_complete": state.lifecycle_complete, "gap": state.gap, "dropped_events": state.dropped,
                "encoding_failed": state.encoding_failed, "all_selected_ticks_observed": state.ticks == state.selected,
                "complete": complete,
            }),
        )
    };
    let mut file = writer
        .join()
        .map_err(|_| anyhow!("PAPER_SOURCE_WRITER_PANICKED"))??;
    let final_record = SourceRecord {
        schema_version: SchemaV1,
        sequence: DbCounter::new(sequence).map_err(anyhow::Error::msg)?,
        observed_at_ns: now_ns()?,
        kind: "end".into(),
        payload: final_payload,
    };
    write_record(&mut file, &mut std::io::stdout().lock(), &final_record)?;
    file.sync_all()?;
    ensure!(complete, "PAPER_SOURCE_INCOMPLETE_RETAIN_RECORDS");
    Ok(())
}

fn read_record(reader: &mut impl BufRead) -> Result<Option<SourceRecord>> {
    let mut bytes = Vec::new();
    reader.take(MAX_RECORD + 1).read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    ensure!(
        bytes.len() as u64 <= MAX_RECORD && bytes.last() == Some(&b'\n'),
        "PAPER_SOURCE_TRUNCATED_OR_OVERSIZED_RECORD"
    );
    Ok(Some(serde_json::from_slice(&bytes)?))
}

struct SourceChild {
    process: Child,
    reaped: Option<ExitStatus>,
}

impl SourceChild {
    fn wait_until(&mut self, deadline: Instant) -> Result<ExitStatus> {
        self.wait_until_controlled(deadline, None)
    }

    fn wait_until_controlled(
        &mut self,
        deadline: Instant,
        control: Option<&crate::polymarket_paper_service::ExecutionControl>,
    ) -> Result<ExitStatus> {
        loop {
            if let Some(control) = control {
                control.check_stop()?;
            }
            if let Some(status) = self.reaped {
                return Ok(status);
            }
            if let Some(status) = self.process.try_wait()? {
                self.reaped = Some(status);
                return Ok(status);
            }
            ensure!(
                Instant::now() < deadline,
                "PAPER_SOURCE_PARENT_WALL_DEADLINE"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn terminate(&mut self) -> Result<ExitStatus> {
        if let Some(status) = self.reaped {
            return Ok(status);
        }
        if let Some(status) = self.process.try_wait()? {
            self.reaped = Some(status);
            return Ok(status);
        }
        if let Err(error) = self.process.kill() {
            if let Some(status) = self.process.try_wait()? {
                self.reaped = Some(status);
                return Ok(status);
            }
            return Err(error.into());
        }
        self.wait_until(Instant::now() + Duration::from_secs(5))
    }
}

impl Drop for SourceChild {
    fn drop(&mut self) {
        if self.reaped.is_none() {
            let _ = self.process.kill();
            let _ = self.wait_until(Instant::now() + Duration::from_secs(2));
        }
    }
}

type SourceRead = std::result::Result<Option<SourceRecord>, String>;

fn source_reader(stdout: impl Read + Send + 'static) -> mpsc::Receiver<SourceRead> {
    let (sender, receiver) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let value = read_record(&mut reader).map_err(|error| error.to_string());
            let finished = !matches!(&value, Ok(Some(_)));
            if sender.send(value).is_err() || finished {
                break;
            }
        }
    });
    receiver
}

#[cfg(test)]
fn next_source_record(
    receiver: &mpsc::Receiver<SourceRead>,
    deadline: Instant,
) -> Result<Option<SourceRecord>> {
    next_source_record_controlled(receiver, deadline, None)
}

fn next_source_record_controlled(
    receiver: &mpsc::Receiver<SourceRead>,
    deadline: Instant,
    control: Option<&crate::polymarket_paper_service::ExecutionControl>,
) -> Result<Option<SourceRecord>> {
    loop {
        if let Some(control) = control {
            control.check_stop()?;
        }
        ensure!(
            Instant::now() < deadline,
            "PAPER_SOURCE_PARENT_WALL_DEADLINE"
        );
        let wait = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(50));
        match receiver.recv_timeout(wait) {
            Ok(value) => return value.map_err(anyhow::Error::msg),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("PAPER_SOURCE_PARENT_WALL_DEADLINE_OR_READER_LOSS"))
            }
        }
    }
}

pub(crate) fn execute(
    config_path: &Path,
    claim_path: &Path,
    metadata_path: &Path,
    dataset_path: &Path,
    source_path: &Path,
    report_path: &Path,
    snapshots_path: &Path,
    binding_path: &Path,
    max_seconds: u64,
    proxy_env: Option<&str>,
    control: Option<&crate::polymarket_paper_service::ExecutionControl>,
) -> Result<()> {
    ensure!(
        (1..=300).contains(&max_seconds),
        "PAPER_HOST_OBSERVATION_BOUND"
    );
    if let Some(name) = proxy_env {
        validate_proxy_env_name(name)?;
    }
    let config: HostConfig = read_original(config_path)?;
    let original_claim = claim(claim_path)?;
    let metadata: RuntimeCatalogMetadataV1 = read_original(metadata_path)?;
    let dataset: DatasetView = read_original(dataset_path)?;
    let (mut session, ids) = prepare(&config, &original_claim, &metadata, &dataset)?;
    if let Some(control) = control {
        control.check_stop()?;
        control.started(session.session_id());
    }
    let mut paths = BTreeSet::new();
    for path in [source_path, report_path, snapshots_path, binding_path] {
        ensure!(
            paths.insert(path.to_path_buf()) && !path.exists(),
            "PAPER_HOST_NEW_DISTINCT_OUTPUTS_REQUIRED"
        );
    }
    let mut report_file = new_output(report_path)?;
    let mut snapshot_file = new_output(snapshots_path)?;
    let mut binding_file = new_output(binding_path)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["polymarket-paper", "source", "--output"])
        .arg(source_path)
        .arg("--max-seconds")
        .arg(max_seconds.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    for id in &ids {
        command.arg("--instrument-id").arg(id.to_string());
    }
    if let Some(name) = proxy_env {
        command.arg("--proxy-env").arg(name);
    }
    // Independent parent deadline covers child startup, capture, lifecycle
    // observation and shutdown/drain. A silent or half-line child cannot hang us.
    let deadline = Instant::now() + Duration::from_secs(max_seconds + 55);
    let mut child = SourceChild {
        process: command.spawn()?,
        reaped: None,
    };
    let receiver = source_reader(
        child
            .process
            .stdout
            .take()
            .ok_or_else(|| anyhow!("PAPER_SOURCE_PIPE_MISSING"))?,
    );
    let mut expected = 1_u64;
    let mut ready = false;
    let mut started = false;
    let mut lifecycle_ready = false;
    let mut lifecycle_ended = false;
    let mut ended = false;
    let mut source_failure: Option<String> = None;
    let mut definitions = BTreeSet::new();
    let mut last_observed = 0;
    let consumed = (|| -> Result<()> {
        while let Some(record) = next_source_record_controlled(&receiver, deadline, control)? {
            ensure!(
                !ended
                    && record.sequence.get() == expected
                    && record.observed_at_ns.get() >= last_observed,
                "PAPER_SOURCE_SEQUENCE_OR_CLOCK_GAP"
            );
            expected += 1;
            last_observed = record.observed_at_ns.get();
            ensure!(
                started || record.kind == "start",
                "PAPER_SOURCE_MISSING_START"
            );
            match record.kind.as_str() {
                "start" => {
                    ensure!(
                        !started
                            && record.sequence.get() == 1
                            && record.payload["purpose"] == "CURRENT_PUBLIC_PAPER_ACCEPTANCE_ONLY"
                            && record.payload["native_version"] == NATIVE_ACCOUNT_VERSION
                            && record.payload["selected_instruments"]
                                == json!(ids.iter().map(ToString::to_string).collect::<Vec<_>>()),
                        "PAPER_SOURCE_START_BINDING"
                    );
                    started = true;
                }
                "source_phase" | "lifecycle_initial_book" => {}
                "lifecycle_ready" => {
                    ensure!(
                        !lifecycle_ready && record.payload["initial_books_complete"] == true,
                        "PAPER_LIFECYCLE_NOT_READY"
                    );
                    lifecycle_ready = true;
                }
                "lifecycle_socket" => {
                    ensure!(
                        record.payload["state"] != "Disconnected",
                        "PAPER_LIFECYCLE_TRANSPORT_LOSS"
                    );
                }
                "lifecycle_event" | "lifecycle_reconnected" => {
                    return Err(anyhow!("PAPER_LIFECYCLE_CHANGED_OR_LOST"))
                }
                "lifecycle_coverage" => {
                    ensure!(
                        ready
                            && !lifecycle_ended
                            && record.payload["primary_native_drain_finished"] == true
                            && record.payload["parsed_message_channel_drained"] == true,
                        "PAPER_LIFECYCLE_DRAIN_UNCONFIRMED"
                    );
                    lifecycle_ended = true;
                }
                "instrument" => {
                    let instrument: InstrumentAny = serde_json::from_value(record.payload)?;
                    session.observe_definition(&instrument)?;
                    definitions.insert(instrument.id());
                }
                "ready" => {
                    ensure!(
                        !ready
                            && lifecycle_ready
                            && record.payload["lifecycle_ready"] == true
                            && record.payload["data_clients_connected"] == true
                            && record.payload["definitions_complete"] == true
                            && definitions == ids.iter().copied().collect(),
                        "PAPER_SOURCE_NOT_READY"
                    );
                    ready = true;
                }
                "quote" => {
                    ensure!(ready, "PAPER_SOURCE_TICK_BEFORE_READY");
                    session.push(Data::Quote(serde_json::from_value(record.payload)?))?;
                    if session.has_started() {
                        if let Some(control) = control {
                            control.running(session.consumed_targets());
                        }
                    }
                }
                "trade" => {
                    ensure!(ready, "PAPER_SOURCE_TICK_BEFORE_READY");
                    session.push(Data::Trade(serde_json::from_value(record.payload)?))?;
                    if session.has_started() {
                        if let Some(control) = control {
                            control.running(session.consumed_targets());
                        }
                    }
                }
                "socket" => {
                    ensure!(
                        record.payload["state"] != "Disconnected",
                        "PAPER_SOURCE_NATIVE_DISCONNECTION"
                    );
                }
                "queue" => {} // Retain native pressure; pressure alone is not proven loss.
                "gap" => return Err(anyhow!("PAPER_SOURCE_NATIVE_CONTINUITY_GAP")),
                "status" => {
                    let status: InstrumentStatus = serde_json::from_value(record.payload)?;
                    ensure!(
                        ids.contains(&status.instrument_id)
                            && status.action == MarketStatusAction::Trading
                            && status.is_trading != Some(false)
                            && status.is_quoting != Some(false),
                        "PAPER_SOURCE_MARKET_LIFECYCLE_UNSUPPORTED"
                    );
                }
                "close" => return Err(anyhow!("PAPER_SOURCE_SETTLEMENT_EVENT_UNSUPPORTED")),
                "end" => {
                    ensure!(
                        ready
                            && lifecycle_ended
                            && record.payload["lifecycle_complete"] == true
                            && record.payload["complete"] == true
                            && record.payload["native_returned_success"] == true
                            && record.payload["native_shutdown_confirmed"] == true
                            && record.payload["gap"] == false
                            && record.payload["dropped_events"] == 0,
                        "PAPER_SOURCE_TERMINAL_INCOMPLETE"
                    );
                    ended = true;
                }
                _ => return Err(anyhow!("PAPER_SOURCE_UNKNOWN_RECORD")),
            }
        }
        ensure!(ended, "PAPER_SOURCE_MISSING_DRAINED_END");
        Ok(())
    })();
    drop(receiver); // Release a reader blocked on its bounded send queue.
    let mut cleanup_confirmed = true;
    if let Err(error) = consumed {
        source_failure = Some(error.to_string());
        session.source_gap();
        cleanup_confirmed = child.terminate().is_ok();
    }
    let child_ok = match child.wait_until_controlled(deadline, control) {
        Ok(status) => status.success(),
        Err(error) => {
            session.source_gap();
            source_failure.get_or_insert(error.to_string());
            cleanup_confirmed = child.terminate().is_ok();
            false
        }
    };
    if !child_ok {
        session.source_gap();
        source_failure.get_or_insert("PAPER_SOURCE_PROCESS_FAILED".into());
    }
    if let Some(control) = control {
        if control.check_stop().is_err() {
            session.source_gap();
            source_failure.get_or_insert("PAPER_HOST_STOP_REQUESTED".into());
        }
    }
    let mut report = session.finish()?;
    report["purpose"] = json!("CURRENT_PUBLIC_PAPER_ACCEPTANCE_ONLY_NOT_HISTORICAL_DATASET");
    report["handoff_id"] = json!(original_claim.handoff.id);
    report["release_id"] = json!(original_claim.handoff.release_id);
    report["source_records_seen"] = json!((expected - 1).to_string());
    report["source_failure"] = json!(source_failure);
    report["source_process_cleanup_confirmed"] = json!(cleanup_confirmed);
    report["source_output"] = json!(source_path);
    report["dataset_revision_id"] = json!(dataset.id);
    report["frozen_catalog_ref"] = json!(metadata.registered_ref);
    report["frozen_origin"] = json!(metadata.origin);
    report["frozen_pit_status"] = json!(metadata.pit_status);
    let complete = child_ok
        && cleanup_confirmed
        && report["performance_status"] == "NATIVE_SIMULATION_AVAILABLE";
    report["account_relay"] = json!("NOT_PERFORMED");
    report["account_relay_inputs_eligible"] = json!(complete);
    // Preserve diagnostics before attempting any further output. Incomplete
    // simulations keep their original snapshots in this report, not in files
    // that could be mistaken for a healthy account-relay segment.
    serde_json::to_writer(&mut report_file, &report)?;
    report_file.write_all(b"\n")?;
    report_file.sync_all()?;
    if complete {
        let snapshots = report["original_portfolio_snapshots"]
            .as_array()
            .ok_or_else(|| anyhow!("PAPER_NATIVE_SNAPSHOTS_MISSING"))?;
        for snapshot in snapshots {
            serde_json::to_writer(&mut snapshot_file, snapshot)?;
            snapshot_file.write_all(b"\n")?;
        }
        let binding = NativeAccountBindingV1 {
            schema_version: SchemaV1,
            project_id: config.project_id,
            environment: ForwardEnvironmentV1::Paper,
            native_trader_id: report["native_trader_id"]
                .as_str()
                .ok_or_else(|| anyhow!("PAPER_NATIVE_TRADER_ID_MISSING"))?
                .into(),
            native_session_id: report["native_session_id"]
                .as_str()
                .ok_or_else(|| anyhow!("PAPER_NATIVE_SESSION_ID_MISSING"))?
                .into(),
            native_account_id: report["native_account_id"]
                .as_str()
                .ok_or_else(|| anyhow!("PAPER_NATIVE_ACCOUNT_ID_MISSING"))?
                .into(),
            native_version: NATIVE_ACCOUNT_VERSION.into(),
        };
        domain::account_observation::binding(&binding)?;
        serde_json::to_writer(&mut binding_file, &binding)?;
    } else {
        serde_json::to_writer(
            &mut binding_file,
            &json!({
                "schema_version": 1, "status": "UNAVAILABLE", "reason_code": "PAPER_HOST_INCOMPLETE",
            }),
        )?;
    }
    binding_file.write_all(b"\n")?;
    snapshot_file.sync_all()?;
    binding_file.sync_all()?;
    if let Some(control) = control {
        control.finished(&report);
    }
    ensure!(complete, "PAPER_HOST_INCOMPLETE_RETAIN_EVIDENCE");
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(
        &mut stdout,
        &json!({
            "schema_version": 1, "status": "NATIVE_PAPER_SIMULATION_RECORDED",
            "report": report_path, "source": source_path, "snapshots": snapshots_path, "binding": binding_path,
            "account_relay": "NOT_PERFORMED_USE_EXISTING_NATIVE_CONVERTER_AND_CLI",
            "historical_dataset": "UNCHANGED_HF_ORIGINALS_REMAIN_RESEARCH_SOURCE",
        }),
    )?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

pub fn run(arguments: Arguments) -> Result<()> {
    match arguments.operation {
        Operation::Serve { config } => crate::polymarket_paper_service::run(&config),
        Operation::Apply {
            origin,
            credential_file,
            claim: path,
        } => {
            let original = claim(&path)?;
            crate::polymarket_paper_service::control(
                &origin,
                &credential_file,
                "targets",
                Some(original),
            )
        }
        Operation::Status {
            origin,
            credential_file,
        } => crate::polymarket_paper_service::control(&origin, &credential_file, "status", None),
        Operation::Stop {
            origin,
            credential_file,
        } => crate::polymarket_paper_service::control(&origin, &credential_file, "stop", None),
        Operation::Run {
            config,
            claim,
            frozen_metadata,
            dataset_revision,
            source_output,
            report_output,
            snapshots_output,
            binding_output,
            max_seconds,
            proxy_env,
        } => execute(
            &config,
            &claim,
            &frozen_metadata,
            &dataset_revision,
            &source_output,
            &report_output,
            &snapshots_output,
            &binding_output,
            max_seconds,
            proxy_env.as_deref(),
            None,
        ),
        Operation::Source {
            instrument_id,
            output,
            max_seconds,
            proxy_env,
        } => {
            ensure!(
                (1..=300).contains(&max_seconds) && (1..=8).contains(&instrument_id.len()),
                "PAPER_SOURCE_BOUNDS"
            );
            let mut unique = BTreeSet::new();
            let ids = instrument_id
                .iter()
                .map(|value| -> Result<_> {
                    let id: InstrumentId = value.parse()?;
                    ensure!(
                        id.venue == Venue::from("POLYMARKET") && unique.insert(id),
                        "PAPER_SOURCE_IDENTITY"
                    );
                    Ok(id)
                })
                .collect::<Result<Vec<_>>>()?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let result = runtime.block_on(source(ids, &output, max_seconds, proxy_env.as_deref()));
            // A cancelled connect can leave an uncancellable system DNS task.
            // Bound official runtime shutdown so its drop cannot conceal the
            // already-recorded source failure from the parent process.
            runtime.shutdown_timeout(Duration::from_secs(2));
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejecting_proxy() -> (String, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, received) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "configured proxy received no request"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("loopback proxy accept: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0_u8; 1];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() <= 16 * 1024);
            }
            socket.write_all(b"HTTP/1.1 407 synthetic-proxy-secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            sender.send(String::from_utf8(request).unwrap()).unwrap();
        });
        (
            format!("http://fixture-user:synthetic-proxy-secret@{address}"),
            received,
            worker,
        )
    }

    fn proxy_fixture_id() -> InstrumentId {
        InstrumentId::from(
            "0x0000000000000000000000000000000000000000000000000000000000000000-1.POLYMARKET",
        )
    }

    async fn assert_factory_proxy_route(websocket: bool) {
        use nautilus_common::{
            cache::Cache, clock::TestClock, factories::DataClientFactory,
            live::runner::replace_data_event_sender, messages::DataEvent,
        };
        let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        origin.set_nonblocking(true).unwrap();
        let origin_address = origin.local_addr().unwrap();
        let (proxy, requests, worker) = rejecting_proxy();
        let mut config = crate::polymarket_data_probe::public_config(if websocket {
            vec![]
        } else {
            vec![proxy_fixture_id()]
        });
        config.proxy_url = Some(proxy.clone());
        config.base_url_gamma = Some(format!("http://{origin_address}"));
        config.base_url_ws = Some(format!("wss://{origin_address}/ws/market"));
        config.http_timeout_secs = 2;
        config.validated_proxy_url().unwrap();
        let (sender, _events) = tokio::sync::mpsc::unbounded_channel::<DataEvent>();
        replace_data_event_sender(sender);
        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(TestClock::new()));
        let mut client = PolymarketDataClientFactory
            .create("QZ-PROXY-TEST", &config, cache.into(), clock)
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(3), client.connect()).await;
        assert!(
            result.is_ok(),
            "official connect did not return after proxy rejection"
        );
        let error = result.unwrap().unwrap_err();
        let detail = public_transport_error(&error, true);
        assert!(!detail.contains("synthetic-proxy-secret") && !detail.contains(&proxy));
        let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        let first = request.lines().next().unwrap();
        if websocket {
            assert_eq!(first, format!("CONNECT {origin_address} HTTP/1.1"));
        } else {
            assert!(
                first.starts_with(&format!("GET http://{origin_address}/markets/keyset?")),
                "{first}"
            );
        }
        assert!(
            matches!(origin.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "proxy error must not fall back to origin"
        );
        worker.join().unwrap();
        client.dispose().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn explicit_proxy_reaches_public_factory_http_without_direct_fallback() {
        assert_factory_proxy_route(false).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn explicit_proxy_reaches_public_factory_websocket_without_direct_fallback() {
        assert_factory_proxy_route(true).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn explicit_proxy_reaches_lifecycle_and_redacts_rejection() {
        let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        origin.set_nonblocking(true).unwrap();
        let address = origin.local_addr().unwrap();
        let (proxy, requests, worker) = rejecting_proxy();
        let id = proxy_fixture_id();
        let mut config = crate::polymarket_data_probe::public_config(vec![id]);
        config.proxy_url = Some(proxy.clone());
        config.base_url_ws = Some(format!("wss://{address}/ws/market"));
        let (state, records) = producer(&[id]);
        let error = LifecycleWatch::start(&[id], &state, &config)
            .await
            .err()
            .expect("proxy must reject");
        assert_eq!(error.to_string(), "proxy CONNECT rejected with status 407");
        assert!(state.borrow().gap && !state.borrow().lifecycle_ready);
        let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            request.lines().next().unwrap(),
            format!("CONNECT {address} HTTP/1.1")
        );
        let evidence = records
            .try_iter()
            .map(|record| serde_json::to_string(&record).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            evidence.contains("407")
                && !evidence.contains("synthetic-proxy-secret")
                && !evidence.contains(&proxy)
        );
        assert!(
            matches!(origin.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        worker.join().unwrap();
    }

    fn producer(
        ids: &[InstrumentId],
    ) -> (Rc<RefCell<ProducerState>>, mpsc::Receiver<SourceRecord>) {
        let (sender, receiver) = mpsc::sync_channel(32);
        (
            Rc::new(RefCell::new(ProducerState {
                sender: Some(sender),
                sequence: 0,
                dropped: 0,
                encoding_failed: false,
                gap: false,
                ready: false,
                lifecycle_ready: false,
                lifecycle_complete: false,
                selected: ids.iter().copied().collect(),
                definitions: BTreeSet::new(),
                ticks: BTreeSet::new(),
            })),
            receiver,
        )
    }

    #[test]
    fn expired_parent_deadline_rejects_even_buffered_records() {
        let (sender, receiver) = mpsc::sync_channel(1);
        assert!(sender
            .send(Ok(Some(SourceRecord {
                schema_version: SchemaV1,
                sequence: DbCounter::new(1).unwrap(),
                observed_at_ns: DbCounter::new(1).unwrap(),
                kind: "start".into(),
                payload: json!({}),
            })))
            .is_ok());
        let expired = Instant::now() - Duration::from_millis(1);
        assert!(next_source_record(&receiver, expired).is_err());
        // Expiry is checked before consuming a queued frame.
        assert!(receiver.try_recv().is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn half_line_child_is_deadlined_killed_and_reaped() {
        let mut child = SourceChild {
            process: Command::new("/bin/sh")
                .args(["-c", "printf '{'; exec sleep 60"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
            reaped: None,
        };
        let receiver = source_reader(child.process.stdout.take().unwrap());
        let began = Instant::now();
        assert!(next_source_record(&receiver, began + Duration::from_millis(100)).is_err());
        drop(receiver);
        let status = child.terminate().unwrap();
        assert!(!status.success());
        assert!(child.reaped.is_some());
        assert_eq!(child.process.try_wait().unwrap(), Some(status));
        assert!(began.elapsed() < Duration::from_secs(6));
    }

    #[cfg(unix)]
    #[test]
    fn downstream_stop_interrupts_silent_source_and_reaps_the_actual_child() {
        use crate::paper_service::{PaperProfile, PaperStatus};
        use crate::polymarket_paper_service::ExecutionControl;
        let mut child = SourceChild {
            process: Command::new("/bin/sh")
                .args(["-c", "printf '{'; exec sleep 60"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
            reaped: None,
        };
        let receiver = source_reader(child.process.stdout.take().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let (status, _) =
            tokio::sync::watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let control = ExecutionControl::new(stop.clone(), status);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            stop.store(true, Ordering::Release);
        });
        let began = Instant::now();
        let error = next_source_record_controlled(
            &receiver,
            began + Duration::from_secs(60),
            Some(&control),
        )
        .err()
        .expect("stop must interrupt read");
        assert_eq!(error.to_string(), "PAPER_HOST_STOP_REQUESTED");
        drop(receiver);
        let outcome = child.terminate().unwrap();
        stopper.join().unwrap();
        assert!(!outcome.success());
        assert_eq!(child.process.try_wait().unwrap(), Some(outcome));
        assert!(began.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn downstream_stop_after_stdout_eof_interrupts_exit_wait_and_reaps() {
        use crate::paper_service::{PaperProfile, PaperStatus};
        let mut child = SourceChild {
            process: Command::new("/bin/sh")
                .args(["-c", "exec 1>&-; exec sleep 60"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
            reaped: None,
        };
        let receiver = source_reader(child.process.stdout.take().unwrap());
        assert!(
            next_source_record(&receiver, Instant::now() + Duration::from_secs(3))
                .unwrap()
                .is_none()
        );
        let stop = Arc::new(AtomicBool::new(false));
        let (status, _) =
            tokio::sync::watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let control = crate::polymarket_paper_service::ExecutionControl::new(stop.clone(), status);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            stop.store(true, Ordering::Release);
        });
        let began = Instant::now();
        let error = child
            .wait_until_controlled(began + Duration::from_secs(60), Some(&control))
            .unwrap_err();
        assert_eq!(error.to_string(), "PAPER_HOST_STOP_REQUESTED");
        let outcome = child.terminate().unwrap();
        stopper.join().unwrap();
        assert_eq!(child.process.try_wait().unwrap(), Some(outcome));
        assert!(began.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn downstream_stop_precedes_already_buffered_source_data() {
        use crate::paper_service::{PaperProfile, PaperStatus};
        let (sender, receiver) = mpsc::sync_channel(1);
        sender
            .send(Ok(Some(SourceRecord {
                schema_version: SchemaV1,
                sequence: DbCounter::new(1).unwrap(),
                observed_at_ns: DbCounter::new(1).unwrap(),
                kind: "start".into(),
                payload: json!({}),
            })))
            .ok()
            .expect("fixture receiver is open");
        let (status, _) =
            tokio::sync::watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let control = crate::polymarket_paper_service::ExecutionControl::new(
            Arc::new(AtomicBool::new(true)),
            status,
        );
        assert!(next_source_record_controlled(
            &receiver,
            Instant::now() + Duration::from_secs(60),
            Some(&control)
        )
        .is_err());
        assert!(receiver.try_recv().is_ok());
    }

    #[test]
    fn retention_keeps_same_timestamp_events_without_a_running_actor() {
        use nautilus_model::types::{Price, Quantity};
        let id = InstrumentId::from("retention-101.POLYMARKET");
        let (state, receiver) = producer(&[id]);
        let retention = NativeRetention::attach(state.clone(), &[id]);
        let first = QuoteTick::new(
            id,
            Price::from_str("0.4000").unwrap(),
            Price::from_str("0.4100").unwrap(),
            Quantity::from_str("10.000000").unwrap(),
            Quantity::from_str("10.000000").unwrap(),
            1_000_000_001_u64.into(),
            1_000_000_002_u64.into(),
        );
        let second = QuoteTick::new(
            id,
            Price::from_str("0.4200").unwrap(),
            Price::from_str("0.4300").unwrap(),
            first.bid_size,
            first.ask_size,
            first.ts_event,
            first.ts_init,
        );
        // Deliberately no running SourceActor: these are the handlers which
        // remain attached during the native runner's final residual drain.
        msgbus::publish_quote(switchboard::get_quotes_topic(id), &first);
        msgbus::publish_quote(switchboard::get_quotes_topic(id), &second);
        let a = receiver.try_recv().unwrap();
        let b = receiver.try_recv().unwrap();
        assert_eq!(a.kind, "quote");
        assert_eq!(b.kind, "quote");
        assert_eq!(a.payload, serde_json::to_value(first).unwrap());
        assert_eq!(b.payload, serde_json::to_value(second).unwrap());
        assert_eq!(b.sequence.get(), a.sequence.get() + 1);
        assert!(!state.borrow().gap);

        let loss = SocketStateChanged::new(
            TraderId::from("QZ-RETENTION-001"),
            ClientId::from(CLIENT),
            Some(Venue::from("POLYMARKET")),
            "test-market".into(),
            SocketState::Disconnected,
            nautilus_core::UUID4::from("00000000-0000-4000-8000-000000000001"),
            1_000_000_001_u64.into(),
            1_000_000_003_u64.into(),
        );
        msgbus::publish_any(MessagingSwitchboard::socket_state_changed_topic(), &loss);
        assert!(state.borrow().gap); // A late-dispatched loss is never expected shutdown.
        assert_eq!(receiver.try_recv().unwrap().kind, "socket");
        drop(retention);
        msgbus::publish_quote(switchboard::get_quotes_topic(id), &first);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn lifecycle_reconnect_and_delayed_loss_remain_incomplete() {
        let (state, receiver) = producer(&[]);
        lifecycle_message(
            PolymarketWsMessage::Reconnected,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &mut BTreeSet::new(),
            &state,
        )
        .unwrap();
        assert!(state.borrow().gap);
        assert_eq!(receiver.try_recv().unwrap().kind, "lifecycle_reconnected");
        let (sender, edges) = mpsc::sync_channel(1);
        sender
            .send(("Disconnected".to_owned(), Some(DbCounter::new(5).unwrap())))
            .unwrap();
        state.borrow_mut().gap = false; // Exercise the edge independently of reconnect.
        lifecycle_edges(&edges, &AtomicBool::new(false), &state);
        assert!(state.borrow().gap);
        assert_eq!(receiver.try_recv().unwrap().kind, "lifecycle_socket");
    }
    #[test]
    fn truncated_record_is_not_a_complete_source() {
        let mut input = std::io::Cursor::new(b"{\"schema_version\":1}".as_slice());
        assert!(read_record(&mut input).is_err());
    }
    #[test]
    fn existing_source_file_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.ndjson");
        std::fs::write(&path, b"original").unwrap();
        assert!(new_output(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }
    #[test]
    fn retained_record_roundtrip_preserves_original_native_clocks() {
        let record = SourceRecord {
            schema_version: SchemaV1,
            sequence: DbCounter::new(1).unwrap(),
            observed_at_ns: DbCounter::new(1_000_000_003).unwrap(),
            kind: "quote".into(),
            payload: json!({"ts_event": "1000000001", "ts_init": "1000000002"}),
        };
        let mut bytes = serde_json::to_vec(&record).unwrap();
        bytes.push(b'\n');
        let parsed = read_record(&mut std::io::Cursor::new(bytes))
            .unwrap()
            .unwrap();
        assert_eq!(parsed.payload, record.payload);
        assert_eq!(parsed.observed_at_ns, record.observed_at_ns);
        assert_eq!(parsed.sequence, record.sequence);
    }

    #[test]
    fn source_queue_loss_is_latched_and_not_silently_skipped() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut state = ProducerState {
            sender: Some(sender),
            sequence: 0,
            dropped: 0,
            encoding_failed: false,
            gap: false,
            ready: false,
            lifecycle_ready: false,
            lifecycle_complete: false,
            selected: BTreeSet::new(),
            definitions: BTreeSet::new(),
            ticks: BTreeSet::new(),
        };
        state.emit("start", json!({}));
        state.emit("ready", json!({}));
        assert!(state.gap);
        assert_eq!(state.dropped, 1);
        assert_eq!(state.sequence, 2);
        assert_eq!(receiver.recv().unwrap().sequence.get(), 1);
        state.emit("queue", json!({}));
        assert!(state.gap);
        assert_eq!(receiver.recv().unwrap().sequence.get(), 3);
    }
}
