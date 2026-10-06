//! Bounded public Polymarket feed observation through the official data factory.
//! This entrypoint registers no execution client, strategy or account. It never
//! requests venue credentials, creates research records or changes qualification.
use anyhow::{anyhow, ensure, Result};
use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore},
    enums::Environment,
    logging::logger::LoggerConfig,
    nautilus_actor,
};
use nautilus_live::node::{LiveNode, NodeRunMode, NodeState};
use nautilus_model::{
    data::{QuoteTick, TradeTick},
    identifiers::{ClientId, InstrumentId, TraderId, Venue},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_polymarket::{
    config::{PolymarketDataClientConfig, PolymarketInstrumentProviderConfig},
    factories::PolymarketDataClientFactory,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

const CLIENT: &str = "QZ-POLYMARKET-PUBLIC-PROBE";

#[derive(clap::Args)]
pub struct Arguments {
    /// Exact official condition-token.POLYMARKET identities; never load all markets.
    #[arg(long, required = true)]
    instrument_id: Vec<String>,
    /// New local evidence file. An existing file is never overwritten.
    #[arg(long)]
    output: PathBuf,
    /// Bounds run/connect after construction; shutdown has a separate 15-second bound.
    #[arg(long, default_value_t = 30)]
    max_seconds: u64,
    /// In-memory event bound. Dropped events make the observation incomplete.
    #[arg(long, default_value_t = 4096)]
    max_events: usize,
    /// Use this existing HTTP(S) proxy environment variable for all public transports.
    /// Omitted: preserve official adapter defaults; no automatic proxy selection.
    #[arg(long)]
    proxy_env: Option<String>,
}

fn selection(arguments: &Arguments) -> Result<Vec<InstrumentId>> {
    ensure!(
        (1..=8).contains(&arguments.instrument_id.len())
            && (1..=300).contains(&arguments.max_seconds)
            && (1..=50_000).contains(&arguments.max_events),
        "POLYMARKET_PROBE_BOUNDS"
    );
    let mut unique = BTreeSet::new();
    arguments
        .instrument_id
        .iter()
        .map(|value| {
            let id: InstrumentId = value.parse()?;
            ensure!(
                id.venue == Venue::from("POLYMARKET") && unique.insert(id),
                "POLYMARKET_PROBE_INSTRUMENT_SCOPE"
            );
            Ok(id)
        })
        .collect()
}

pub(crate) fn public_config(ids: Vec<InstrumentId>) -> PolymarketDataClientConfig {
    PolymarketDataClientConfig {
        instrument_config: Some(PolymarketInstrumentProviderConfig {
            load_all: false,
            load_ids: Some(ids),
            ..Default::default()
        }),
        auto_load_missing_instruments: false,
        subscribe_new_markets: false,
        update_instruments_interval_mins: None,
        ..Default::default()
    }
}

pub(crate) fn validate_proxy_env_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 128
            && name.bytes().enumerate().all(|(index, value)| value == b'_'
                || value.is_ascii_alphabetic()
                || (index > 0 && value.is_ascii_digit())),
        "POLYMARKET_PUBLIC_PROXY_ENV_NAME"
    );
    Ok(())
}

pub(crate) fn public_config_from_proxy_env(
    ids: Vec<InstrumentId>,
    name: Option<&str>,
) -> Result<PolymarketDataClientConfig> {
    public_config_from_proxy_lookup(ids, name, |name| std::env::var(name))
}

fn public_config_from_proxy_lookup(
    ids: Vec<InstrumentId>,
    name: Option<&str>,
    lookup: impl FnOnce(&str) -> std::result::Result<String, std::env::VarError>,
) -> Result<PolymarketDataClientConfig> {
    let mut config = public_config(ids);
    if let Some(name) = name {
        validate_proxy_env_name(name)?;
        config.proxy_url =
            Some(lookup(name).map_err(|_| anyhow!("POLYMARKET_PUBLIC_PROXY_ENV_UNAVAILABLE"))?);
        // Strict official HTTP/HTTPS validation prevents the lower transport's
        // unsupported-SOCKS direct fallback. Never downgrade failure to None.
        config
            .validated_proxy_url()
            .map_err(|_| anyhow!("POLYMARKET_PUBLIC_PROXY_CONFIG_INVALID"))?;
    }
    Ok(config)
}

pub(crate) fn public_transport_error(error: &anyhow::Error, configured_proxy: bool) -> String {
    if !configured_proxy {
        return error.to_string().chars().take(1024).collect();
    }
    // Keep safe official rejection codes, never raw proxy URL/user-info,
    // response headers or arbitrary upstream error text.
    for cause in error.chain() {
        if let Some(error) = cause.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind;
            let code = match error.kind() {
                ErrorKind::TimedOut => Some("POLYMARKET_PUBLIC_TRANSPORT_TIMEOUT"),
                ErrorKind::ConnectionRefused => Some("POLYMARKET_PUBLIC_CONNECTION_REFUSED"),
                ErrorKind::ConnectionReset => Some("POLYMARKET_PUBLIC_CONNECTION_RESET"),
                ErrorKind::ConnectionAborted => Some("POLYMARKET_PUBLIC_CONNECTION_ABORTED"),
                ErrorKind::PermissionDenied => {
                    Some("POLYMARKET_PUBLIC_TRANSPORT_PERMISSION_DENIED")
                }
                ErrorKind::InvalidData => error.get_ref().and_then(|inner| {
                    let message = inner.to_string();
                    if message.starts_with("invalid peer certificate: ") {
                        Some("POLYMARKET_PUBLIC_TLS_CERTIFICATE_FAILED")
                    } else if message.starts_with("received fatal alert: ") {
                        Some("POLYMARKET_PUBLIC_TLS_ALERT_RECEIVED")
                    } else {
                        None
                    }
                }),
                ErrorKind::UnexpectedEof
                    if error
                        .get_ref()
                        .is_some_and(|inner| inner.to_string() == "tls handshake eof") =>
                {
                    Some("POLYMARKET_PUBLIC_TLS_HANDSHAKE_EOF")
                }
                _ => None,
            };
            if let Some(code) = code {
                return code.into();
            }
        }
        let message = cause.to_string();
        for prefix in [
            "proxy CONNECT rejected with status ",
            "WebSocket upgrade rejected with status ",
        ] {
            if let Some(status) = message
                .strip_prefix(prefix)
                .and_then(|value| value.parse::<u16>().ok())
            {
                if (100..=599).contains(&status) {
                    return format!("{prefix}{status}");
                }
            }
        }
        if matches!(
            message.as_str(),
            "PAPER_LIFECYCLE_CONNECT_TIMEOUT"
                | "POLYMARKET_PUBLIC_PROXY_ENV_NAME"
                | "POLYMARKET_PUBLIC_PROXY_ENV_UNAVAILABLE"
                | "POLYMARKET_PUBLIC_PROXY_CONFIG_INVALID"
        ) {
            return message;
        }
        if message.starts_with("TLS error: ") {
            return "POLYMARKET_PUBLIC_TLS_FAILED".into();
        }
    }
    "POLYMARKET_PUBLIC_PROXY_UPSTREAM_FAILED".into()
}

#[derive(Debug)]
struct Capture {
    selected: BTreeSet<String>,
    instruments: BTreeSet<String>,
    ticks: BTreeSet<String>,
    events: Vec<Value>,
    maximum: usize,
    dropped_events: u64,
    encoding_failed: bool,
}

impl Capture {
    fn retain(&mut self, kind: &str, id: InstrumentId, event: &impl Serialize) {
        let id = id.to_string();
        if !self.selected.contains(&id) {
            return;
        }
        if kind == "instrument" {
            self.instruments.insert(id.clone());
        } else {
            self.ticks.insert(id.clone());
        }
        if self.events.len() == self.maximum {
            self.dropped_events = self.dropped_events.saturating_add(1);
            return;
        }
        match serde_json::to_value(event) {
            Ok(native) => self.events.push(json!({
                "kind": kind,
                "instrument_id": id,
                "observed_at": chrono::Utc::now().to_rfc3339(),
                "native": native,
            })),
            Err(_) => self.encoding_failed = true,
        }
    }

    fn complete(&self) -> bool {
        self.instruments == self.selected
            && self.ticks == self.selected
            && self.dropped_events == 0
            && !self.encoding_failed
    }
}

#[derive(Debug)]
struct PublicObserver {
    core: DataActorCore,
    ids: Vec<InstrumentId>,
    capture: Rc<RefCell<Capture>>,
}

nautilus_actor!(PublicObserver);

impl DataActor for PublicObserver {
    fn on_start(&mut self) -> Result<()> {
        for id in self.ids.clone() {
            // Bootstrap definitions can arrive before the actor starts. Retain
            // the original cached object as well as later native updates.
            let instrument = self.cache().instrument(&id);
            if let Some(instrument) = instrument {
                self.capture
                    .borrow_mut()
                    .retain("instrument", id, &instrument);
            }
            self.subscribe_instrument(id, Some(ClientId::from(CLIENT)), None);
            self.subscribe_quotes(id, Some(ClientId::from(CLIENT)), None);
            self.subscribe_trades(id, Some(ClientId::from(CLIENT)), None);
        }
        Ok(())
    }

    fn on_instrument(&mut self, instrument: &InstrumentAny) -> Result<()> {
        self.capture
            .borrow_mut()
            .retain("instrument", instrument.id(), instrument);
        Ok(())
    }

    fn on_quote(&mut self, quote: &QuoteTick) -> Result<()> {
        self.capture
            .borrow_mut()
            .retain("quote", quote.instrument_id, quote);
        Ok(())
    }

    fn on_trade(&mut self, trade: &TradeTick) -> Result<()> {
        self.capture
            .borrow_mut()
            .retain("trade", trade.instrument_id, trade);
        Ok(())
    }
}

fn new_output(path: &PathBuf) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

async fn observe(arguments: &Arguments, ids: Vec<InstrumentId>) -> Result<Value> {
    let started_at = chrono::Utc::now();
    let config = public_config_from_proxy_env(ids.clone(), arguments.proxy_env.as_deref())?;
    let configured_proxy = config.has_proxy_url();
    let capture = Rc::new(RefCell::new(Capture {
        selected: ids.iter().map(ToString::to_string).collect(),
        instruments: BTreeSet::new(),
        ticks: BTreeSet::new(),
        events: Vec::new(),
        maximum: arguments.max_events,
        dropped_events: 0,
        encoding_failed: false,
    }));
    let mut node = LiveNode::builder(TraderId::from("QZ-PUBLIC-PROBE-001"), Environment::Live)?
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
            Box::new(config),
        )?
        .build()?;
    node.add_actor(PublicObserver {
        core: DataActorCore::new(DataActorConfig {
            log_events: false,
            log_commands: false,
            ..Default::default()
        }),
        ids,
        capture: capture.clone(),
    })?;
    let session = node.instance_id().to_string();
    let handle = node.handle();
    let (native_failure, shutdown_timed_out) = {
        let native = node.run_with_mode(NodeRunMode::Hosted);
        tokio::pin!(native);
        tokio::select! {
            result = &mut native => (result.err().map(|error| public_transport_error(&error, configured_proxy)), false),
            _ = tokio::time::sleep(Duration::from_secs(arguments.max_seconds)) => {
                handle.stop();
                match tokio::time::timeout(Duration::from_secs(15), &mut native).await {
                    Ok(result) => (result.err().map(|error| public_transport_error(&error, configured_proxy)), false),
                    Err(_) => (Some("POLYMARKET_PROBE_SHUTDOWN_TIMEOUT".into()), true),
                }
            }
        }
    };
    let native_ok = native_failure.is_none();
    let stopped = handle.state() == NodeState::Stopped;
    let observed = capture.borrow();
    let complete = native_ok && stopped && observed.complete();
    Ok(json!({
        "schema_version": 1,
        "status": if complete { "PUBLIC_NATIVE_DATA_OBSERVED" } else { "INCOMPLETE" },
        "started_at": started_at,
        "ended_at": chrono::Utc::now(),
        "native_version": contracts::account_observation::NATIVE_ACCOUNT_VERSION,
        "native_session_id": session,
        "selected_instruments": observed.selected,
        "instruments_observed": observed.instruments,
        "tick_instruments_observed": observed.ticks,
        "max_seconds": arguments.max_seconds,
        "proxy_configured": configured_proxy,
        "native_returned_success": native_ok,
        "native_failure": native_failure,
        "native_shutdown_confirmed": stopped,
        "shutdown_timed_out": shutdown_timed_out,
        "dropped_events": observed.dropped_events,
        "encoding_failed": observed.encoding_failed,
        "execution_surface": "NO_EXECUTION_CLIENT_OR_STRATEGY_REGISTERED",
        "scientific_qualification": "NOT_ASSESSED",
        "historical_pit": "NOT_ESTABLISHED",
        "fee_model": "NOT_RUN",
        "events": observed.events,
    }))
}

pub fn run(arguments: Arguments) -> Result<()> {
    let ids = selection(&arguments)?;
    // Reserve a fresh evidence destination before opening any network connection.
    let mut file = new_output(&arguments.output)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(observe(&arguments, ids));
    runtime.shutdown_timeout(Duration::from_secs(2));
    let report = match result {
        Ok(report) => report,
        Err(error) => json!({
            "schema_version": 1,
            "status": "INCOMPLETE",
            "reason_code": "POLYMARKET_PUBLIC_PROBE_SETUP_FAILED",
            "failure_detail": public_transport_error(&error, arguments.proxy_env.is_some()),
            "execution_surface": "NO_EXECUTION_CLIENT_OR_STRATEGY_REGISTERED",
            "scientific_qualification": "NOT_ASSESSED",
        }),
    };
    serde_json::to_writer(&mut file, &report)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    ensure!(
        report["status"] == "PUBLIC_NATIVE_DATA_OBSERVED",
        "POLYMARKET_PUBLIC_PROBE_INCOMPLETE_RETAIN_EVIDENCE"
    );
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(
        &mut stdout,
        &json!({
            "schema_version": 1,
            "status": report["status"],
            "output": arguments.output,
            "scientific_qualification": "NOT_ASSESSED",
        }),
    )?;
    stdout.write_all(b"\n")?;
    stdout
        .flush()
        .map_err(|_| anyhow!("POLYMARKET_PROBE_STDOUT_FAILED"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_selection_is_explicit_and_strictly_validated_without_secret_output() {
        let defaults = public_config_from_proxy_lookup(vec![], None, |_| {
            panic!("must not read environment implicitly")
        })
        .unwrap();
        assert!(!defaults.has_proxy_url());
        let configured = public_config_from_proxy_lookup(vec![], Some("QZ_TEST_PROXY"), |name| {
            assert_eq!(name, "QZ_TEST_PROXY");
            Ok("http://fixture-user:synthetic-proxy-secret@127.0.0.1:18888".into())
        })
        .unwrap();
        assert!(configured.validated_proxy_url().unwrap().is_some());
        assert!(!format!("{configured:?}").contains("synthetic-proxy-secret"));
        for value in [
            "",
            "socks5://127.0.0.1:18888",
            "http://fixture-user:synthetic-proxy-secret@[::1",
        ] {
            let error =
                public_config_from_proxy_lookup(
                    vec![],
                    Some("QZ_TEST_PROXY"),
                    |_| Ok(value.into()),
                )
                .unwrap_err();
            assert_eq!(error.to_string(), "POLYMARKET_PUBLIC_PROXY_CONFIG_INVALID");
            assert!(!format!("{error:?}").contains("synthetic-proxy-secret"));
        }
        assert!(
            public_config_from_proxy_lookup(vec![], Some("QZ_TEST_PROXY"), |_| Err(
                std::env::VarError::NotPresent
            ))
            .is_err()
        );
        assert!(
            public_config_from_proxy_lookup(vec![], Some("not=a-variable"), |_| panic!(
                "invalid name must not be read"
            ))
            .is_err()
        );
    }

    #[test]
    fn proxy_transport_diagnostics_keep_status_without_arbitrary_text() {
        let secret =
            anyhow!("upstream http://fixture-user:synthetic-proxy-secret@127.0.0.1:18888 rejected");
        assert_eq!(
            public_transport_error(&secret, true),
            "POLYMARKET_PUBLIC_PROXY_UPSTREAM_FAILED"
        );
        for status in [403, 407] {
            let error = anyhow!("proxy CONNECT rejected with status {status}")
                .context("synthetic-proxy-secret context");
            assert_eq!(
                public_transport_error(&error, true),
                format!("proxy CONNECT rejected with status {status}")
            );
        }
        let timed_out = anyhow::Error::from(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "synthetic-proxy-secret",
        ));
        assert_eq!(
            public_transport_error(&timed_out, true),
            "POLYMARKET_PUBLIC_TRANSPORT_TIMEOUT"
        );
        for (kind, detail, expected) in [
            (
                std::io::ErrorKind::InvalidData,
                "invalid peer certificate: synthetic-proxy-secret",
                "POLYMARKET_PUBLIC_TLS_CERTIFICATE_FAILED",
            ),
            (
                std::io::ErrorKind::InvalidData,
                "received fatal alert: synthetic-proxy-secret",
                "POLYMARKET_PUBLIC_TLS_ALERT_RECEIVED",
            ),
            (
                std::io::ErrorKind::UnexpectedEof,
                "tls handshake eof",
                "POLYMARKET_PUBLIC_TLS_HANDSHAKE_EOF",
            ),
            (
                std::io::ErrorKind::ConnectionReset,
                "synthetic-proxy-secret",
                "POLYMARKET_PUBLIC_CONNECTION_RESET",
            ),
            (
                std::io::ErrorKind::InvalidData,
                "unknown synthetic-proxy-secret",
                "POLYMARKET_PUBLIC_PROXY_UPSTREAM_FAILED",
            ),
        ] {
            let error = anyhow::Error::from(std::io::Error::new(kind, detail))
                .context("synthetic-proxy-secret context");
            assert_eq!(public_transport_error(&error, true), expected);
        }
        assert_eq!(
            public_transport_error(&anyhow!("TLS error: synthetic-proxy-secret"), true),
            "POLYMARKET_PUBLIC_TLS_FAILED"
        );
        for code in [
            "POLYMARKET_PUBLIC_PROXY_ENV_NAME",
            "POLYMARKET_PUBLIC_PROXY_ENV_UNAVAILABLE",
            "POLYMARKET_PUBLIC_PROXY_CONFIG_INVALID",
        ] {
            assert_eq!(public_transport_error(&anyhow!(code), true), code);
        }
    }

    fn args(ids: &[&str]) -> Arguments {
        Arguments {
            instrument_id: ids.iter().map(|v| (*v).to_owned()).collect(),
            output: PathBuf::from("never-opened-in-unit-tests.json"),
            max_seconds: 1,
            max_events: 8,
            proxy_env: None,
        }
    }

    #[test]
    fn public_scope_is_explicit_unique_and_bounded() {
        assert!(selection(&args(&[])).is_err());
        assert!(selection(&args(&["BTCUSDT.BINANCE"])).is_err());
        assert!(selection(&args(&["condition-1.POLYMARKET", "condition-1.POLYMARKET"])).is_err());
        let mut value = args(&["condition-1.POLYMARKET"]);
        value.max_seconds = 301;
        assert!(selection(&value).is_err());
    }

    #[test]
    fn official_public_config_cannot_expand_selection() {
        let ids = selection(&args(&["condition-1.POLYMARKET"])).unwrap();
        let config = public_config(ids.clone());
        let scope = config.instrument_config.unwrap();
        assert_eq!(scope.load_ids, Some(ids));
        assert!(!scope.load_all);
        assert!(!config.auto_load_missing_instruments);
        assert!(!config.subscribe_new_markets);
        assert!(config.base_url_http.is_none() && config.base_url_ws.is_none());
        assert!(config.update_instruments_interval_mins.is_none());
    }

    #[test]
    fn evidence_requires_all_selected_definitions_and_ticks_without_loss() {
        let id = "condition-1.POLYMARKET";
        let mut capture = Capture {
            selected: [id.to_owned()].into(),
            instruments: BTreeSet::new(),
            ticks: BTreeSet::new(),
            events: Vec::new(),
            maximum: 1,
            dropped_events: 0,
            encoding_failed: false,
        };
        assert!(!capture.complete());
        capture.instruments.insert(id.into());
        assert!(!capture.complete());
        capture.ticks.insert(id.into());
        assert!(capture.complete());
        capture.dropped_events = 1;
        assert!(!capture.complete());
    }
    #[test]
    fn existing_evidence_is_never_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing.json");
        std::fs::write(&path, b"original evidence").unwrap();
        assert!(new_output(&path).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"original evidence");
    }
}
