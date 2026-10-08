//! One original claimed target, one bounded native Cash session, no venue orders.
//! HTTP control reuses the existing authenticated loopback service. The original
//! synchronous engine owns its thread; cancellation never pretends it has joined.
use crate::{
    paper_service::{
        PaperProfile, PaperState, PaperStatus,
        initial_execution::{
            InitialCapitalPermit, InitialExecutionAuthority, InitialExecutionAuthorityConfig,
        },
    },
    polymarket_paper_host::{self as host, HostConfig},
};
use anyhow::{Result, anyhow, ensure};
use contracts::{
    SchemaV1, catalogs::RuntimeCatalogMetadataV1, data::DatasetView,
    strategy_portfolio::HandoffClaimViewV2,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceConfig {
    schema_version: SchemaV1,
    host_config: PathBuf,
    #[serde(default)]
    frozen_metadata: Option<PathBuf>,
    #[serde(default)]
    dataset_revision: Option<PathBuf>,
    /// Exclusive new directory. Reuse/restart is rejected, never replayed.
    output_directory: PathBuf,
    /// Stable private runtime volume, never the per-session output directory.
    claim_state_directory: PathBuf,
    credential_file: PathBuf,
    /// Reference the already existing QZ DownstreamClaim machine connection.
    /// Never fall back to the local loopback service bearer.
    #[serde(default)]
    initial_execution_authority: Option<InitialExecutionAuthorityConfig>,
    /// Opt in to the existing native account's capital-exit adapter. Reuses the
    /// configured QZ machine connection and its existing ForwardSubmit scope.
    #[serde(default)]
    capital_exit: bool,
    bind: SocketAddr,
    max_seconds: u64,
    #[serde(default)]
    proxy_env: Option<String>,
}

#[derive(Clone)]
struct ExecutionSummary {
    cleanup_confirmed: bool,
    available: bool,
    target_points_consumed: usize,
}
impl ExecutionSummary {
    fn from_report(report: &Value) -> Self {
        Self {
            cleanup_confirmed: report["source_process_cleanup_confirmed"] == true,
            available: report["performance_status"] == "NATIVE_SIMULATION_AVAILABLE"
                && report["account_relay_inputs_eligible"] == true,
            target_points_consumed: report["consumed_original_target_points"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0),
        }
    }
}

pub(crate) struct ExecutionControl {
    stop: Arc<AtomicBool>,
    status: watch::Sender<PaperStatus>,
    completion: Arc<Mutex<Option<ExecutionSummary>>>,
    capital_exit: Option<crate::paper_capital_exit::NativeInbox>,
}

impl ExecutionControl {
    pub(crate) fn new(stop: Arc<AtomicBool>, status: watch::Sender<PaperStatus>) -> Self {
        Self {
            stop,
            status,
            completion: Arc::new(Mutex::new(None)),
            capital_exit: None,
        }
    }
    fn with_capital_exit(mut self, inbox: Option<crate::paper_capital_exit::NativeInbox>) -> Self {
        self.capital_exit = inbox;
        self
    }
    pub(crate) fn capital_exit_root(&self) -> Option<&Path> {
        self.capital_exit.as_ref().map(|inbox| inbox.root())
    }
    pub(crate) fn service_capital_exit(
        &self,
        session: &mut crate::polymarket_streaming_paper::PolymarketStreamingPaper,
    ) -> Result<()> {
        if let Some(inbox) = &self.capital_exit {
            inbox.service(session)?;
        }
        Ok(())
    }
    pub(crate) fn finished(&self, report: &Value) {
        if let Ok(mut summary) = self.completion.lock() {
            *summary = Some(ExecutionSummary::from_report(report));
        }
    }
    pub(crate) fn check_stop(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "PAPER_HOST_STOP_REQUESTED"
        );
        Ok(())
    }
    pub(crate) fn started(&self, session: String) {
        self.status.send_modify(|s| {
            s.native_session_id = Some(session);
            s.updated_at = chrono::Utc::now();
        });
    }
    pub(crate) fn running(&self, consumed: usize) {
        self.status.send_modify(|s| {
            if s.state == PaperState::Starting {
                s.state = PaperState::Running;
            }
            s.target_points_consumed = consumed;
            s.updated_at = chrono::Utc::now();
        });
    }
}

fn new_private(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn original_bytes(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= 8 * 1024 * 1024,
        "PAPER_SERVICE_INPUT_LIMIT"
    );
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 8 * 1024 * 1024, "PAPER_SERVICE_INPUT_LIMIT");
    Ok(bytes)
}

fn retain(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = new_private(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        File::open(
            path.parent()
                .ok_or_else(|| anyhow!("PAPER_SERVICE_OUTPUT"))?,
        )?
        .sync_all()?;
    }
    Ok(())
}

fn reserve_output(path: &Path) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|_| anyhow!("PAPER_SERVICE_NEW_OUTPUT_REQUIRED"))?;
    retain(
        &path.join("service-reservation.json"),
        b"{\"schema_version\":1,\"restart\":\"REFUSED_USE_ORIGINAL_EVIDENCE\"}\n",
    )
}

fn publish_terminal(
    status: &watch::Sender<PaperStatus>,
    success: bool,
    cancelled: bool,
    report: Option<&ExecutionSummary>,
) {
    let clean = report.is_some_and(|r| r.cleanup_confirmed);
    let available = success && clean && report.is_some_and(|r| r.available);
    status.send_modify(|s| {
        // This function is called only after the native thread has actually joined.
        s.state = if available || (cancelled && clean) {
            PaperState::Stopped
        } else {
            PaperState::Failed
        };
        if let Some(report) = report {
            s.target_points_consumed = report.target_points_consumed;
        }
        s.reason_code = if available {
            None
        } else if cancelled && clean {
            Some("PAPER_CANCELLED_INCOMPLETE_NO_RELAY".into())
        } else {
            Some("PAPER_NATIVE_INCOMPLETE_RETAIN_EVIDENCE".into())
        };
        s.updated_at = chrono::Utc::now();
    });
}

async fn initial_permit_for<F: std::future::Future<Output = ()>>(
    host_config: &HostConfig,
    claim: &HandoffClaimViewV2,
    authority: &mut Option<InitialExecutionAuthority>,
    stop: &watch::Receiver<bool>,
    stop_latch: &AtomicBool,
    mut signal: std::pin::Pin<&mut F>,
    status: &watch::Sender<PaperStatus>,
) -> Result<Option<InitialCapitalPermit>> {
    ensure!(
        !*stop.borrow() && !stop_latch.load(Ordering::Acquire),
        "PAPER_HOST_STOP_REQUESTED"
    );
    let contracts::strategy_portfolio::TargetPackageEnvelopeV2::Forecast(package) = &claim.package
    else {
        return Ok(None);
    };
    host::forecast_preflight(host_config, claim)?;
    crate::paper_node::forecast_initial_account(package)?
        .ok_or_else(|| anyhow!(crate::paper_node::forecast_initialization_blocker(package)))?;
    let authority = authority
        .as_mut()
        .ok_or_else(|| anyhow!("PAPER_INITIAL_EXECUTION_CONNECTION_REQUIRED"))?;
    let mut api_stop = stop.clone();
    let latch = || {
        stop_latch.store(true, Ordering::Release);
        status.send_modify(|value| value.stop_requested = true);
    };
    // Keep polling the original OS handler while HTTP is pending. A signal or
    // API stop wins if both cancellation and a response are ready. Dropping an
    // in-flight consume leaves its outcome unknown, never rolls back its root.
    let permit = tokio::select! {
        biased;
        _ = signal.as_mut() => { latch(); return Err(anyhow!("PAPER_HOST_STOP_REQUESTED")); },
        _ = async {
            while !*api_stop.borrow() {
                if api_stop.changed().await.is_err() { break; }
            }
        } => { latch(); return Err(anyhow!("PAPER_HOST_STOP_REQUESTED")); },
        result = authority.consume(claim) => result?,
    };
    // Poll the same handler once more at the permit handoff, including a signal
    // queued while the completed response was being validated synchronously.
    let pending_signal = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(signal.as_mut(), cx).is_ready())
    })
    .await;
    if pending_signal {
        latch();
    }
    // The native owner receives this same latch, rather than a new false flag.
    ensure!(
        !*stop.borrow() && !stop_latch.load(Ordering::Acquire),
        "PAPER_HOST_STOP_REQUESTED"
    );
    Ok(Some(permit))
}

fn initial_error(error: &anyhow::Error) -> (crate::paper_service::PaperApplyError, &'static str) {
    use crate::paper_service::PaperApplyError;
    match error.to_string().as_str() {
        "PAPER_HOST_STOP_REQUESTED" => (PaperApplyError::Stopped, "PAPER_HOST_STOP_REQUESTED"),
        "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED" => (
            PaperApplyError::UnsupportedForecastInitialization,
            "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED",
        ),
        "PAPER_FORECAST_CONTINUATION_UNSUPPORTED" => (
            PaperApplyError::UnsupportedForecastContinuation,
            "PAPER_FORECAST_CONTINUATION_UNSUPPORTED",
        ),
        "PAPER_FORECAST_CONSTRAINT_MEASUREMENT_UNSUPPORTED" => (
            PaperApplyError::UnsupportedForecastConstraints,
            "PAPER_FORECAST_CONSTRAINT_MEASUREMENT_UNSUPPORTED",
        ),
        "PAPER_FORECAST_ASSET_SCOPE_UNSUPPORTED" => (
            PaperApplyError::UnsupportedForecastAssets,
            "PAPER_FORECAST_ASSET_SCOPE_UNSUPPORTED",
        ),
        "PAPER_INITIAL_EXECUTION_CONNECTION_REQUIRED" => (
            PaperApplyError::InitialExecutionConfigurationRequired,
            "PAPER_INITIAL_EXECUTION_CONNECTION_REQUIRED",
        ),
        "PAPER_INITIAL_EXECUTION_RESULT_UNKNOWN" => (
            PaperApplyError::InitialExecutionUnknown,
            "PAPER_INITIAL_EXECUTION_RESULT_UNKNOWN",
        ),
        "PAPER_INITIAL_EXECUTION_BLOCKED"
        | "PAPER_INITIAL_EXECUTION_RESPONSE_INVALID"
        | "PAPER_INITIAL_EXECUTION_RESPONSE_BLOCKED"
        | "PAPER_INITIAL_EXECUTION_ALREADY_ATTEMPTED" => (
            PaperApplyError::InitialExecutionBlocked,
            "PAPER_INITIAL_EXECUTION_BLOCKED",
        ),
        _ => (
            PaperApplyError::InvalidClaim,
            "PAPER_INVALID_CLAIM_OR_CONFIGURATION",
        ),
    }
}

async fn serve(config: ServiceConfig) -> Result<()> {
    let _ = config.schema_version;
    ensure!(
        config.bind.ip().is_loopback(),
        "PAPER_CONTROL_LOOPBACK_ORIGIN_REQUIRED"
    );
    ensure!(
        (1..=300).contains(&config.max_seconds),
        "PAPER_HOST_OBSERVATION_BOUND"
    );
    if let Some(name) = &config.proxy_env {
        crate::polymarket_data_probe::validate_proxy_env_name(name)?;
    }
    let mut initial_authority = config
        .initial_execution_authority
        .as_ref()
        .map(InitialExecutionAuthority::open)
        .transpose()?;
    ensure!(
        !config.capital_exit || initial_authority.is_some(),
        "CAPITAL_EXIT_EXISTING_MACHINE_CONNECTION_REQUIRED"
    );
    // Read and retain the immutable original inputs once before accepting work.
    let original_config = original_bytes(&config.host_config)?;
    let original_metadata = config
        .frozen_metadata
        .as_deref()
        .map(original_bytes)
        .transpose()?;
    let original_dataset = config
        .dataset_revision
        .as_deref()
        .map(original_bytes)
        .transpose()?;
    let host_config: HostConfig = serde_json::from_slice(&original_config)?;
    if let Some(bytes) = &original_metadata {
        let _: RuntimeCatalogMetadataV1 = serde_json::from_slice(bytes)?;
    }
    if let Some(bytes) = &original_dataset {
        let _: DatasetView = serde_json::from_slice(bytes)?;
    }
    let credential = crate::paper_node::read_credential(&config.credential_file)?;
    ensure!(
        config.claim_state_directory.is_absolute(),
        "PAPER_STABLE_CLAIM_STATE_REQUIRED"
    );
    reserve_output(&config.output_directory)?;
    ensure!(
        !config
            .claim_state_directory
            .starts_with(std::fs::canonicalize(&config.output_directory)?),
        "PAPER_CLAIM_STATE_MUST_BE_INDEPENDENT_OF_OUTPUT"
    );
    let out = config.output_directory;
    let frozen_config = out.join("host-config.json");
    let frozen_metadata = original_metadata
        .as_ref()
        .map(|_| out.join("frozen-metadata.json"));
    let frozen_dataset = original_dataset
        .as_ref()
        .map(|_| out.join("dataset-revision.json"));
    retain(&frozen_config, &original_config)?;
    if let (Some(path), Some(bytes)) = (&frozen_metadata, &original_metadata) {
        retain(path, bytes)?;
    }
    if let (Some(path), Some(bytes)) = (&frozen_dataset, &original_dataset) {
        retain(path, bytes)?;
    }
    let mut service = crate::paper_service::start_control_with_profile(
        config.bind,
        credential,
        host_config.market_capability_version.clone(),
        PaperProfile::Polymarket,
        &config.claim_state_directory,
    )
    .await?;
    println!(
        "{}",
        json!({"state":"idle", "bind":service.local_addr,
        "environment":"PAPER", "market_data_connected":false,
        "restart":"FRESH_ACCOUNT_AND_SESSION_NO_RESTORE",
        "claim_replay_scope":"DURABLE_RUNTIME_PROJECT_ADAPTER_CLAIM"})
    );
    let stop_latch = Arc::new(AtomicBool::new(*service.stop.borrow()));
    let signal = crate::paper_node::stop_signal();
    tokio::pin!(signal);
    let request = tokio::select! {
        request = service.requests.recv() => request,
        _ = service.stop.changed() => None,
        _ = &mut signal => None,
    };
    let result = if let Some(request) = request {
        let claim_path = out.join("accepted-claim.json");
        if let Err(error) = retain(&claim_path, &serde_json::to_vec(&request.claim)?) {
            let _ = request
                .reply
                .send(Err(crate::paper_service::PaperApplyError::Unavailable));
            publish_terminal(&service.status, false, false, None);
            Err(error)
        } else {
            let permit = if *service.stop.borrow() {
                Err(anyhow!("PAPER_HOST_STOP_REQUESTED"))
            } else {
                initial_permit_for(
                    &host_config,
                    &request.claim,
                    &mut initial_authority,
                    &service.stop,
                    &stop_latch,
                    signal.as_mut(),
                    &service.status,
                )
                .await
            };
            match permit {
                Err(error) => {
                    let (reply_error, code) = initial_error(&error);
                    crate::paper_node::publish_state(
                        &service.status,
                        if code == "PAPER_HOST_STOP_REQUESTED" {
                            PaperState::Stopped
                        } else {
                            PaperState::Failed
                        },
                        Some(code),
                    );
                    let _ = request.reply.send(Err(reply_error));
                    Err(anyhow!(code))
                }
                Ok(initial_permit) => {
                    let stop = stop_latch.clone();
                    if *service.stop.borrow() {
                        stop.store(true, Ordering::Release);
                    }
                    let (capital_inbox, capital_task) = if config.capital_exit {
                        let (inbox, sender) = crate::paper_capital_exit::channel(
                            config.claim_state_directory.clone(),
                        );
                        let transport = initial_authority
                            .as_ref()
                            .ok_or_else(|| {
                                anyhow!("CAPITAL_EXIT_EXISTING_MACHINE_CONNECTION_REQUIRED")
                            })?
                            .capital_exit_transport();
                        let poller = crate::paper_capital_exit::Poller::new(
                            transport,
                            sender,
                            config.claim_state_directory.clone(),
                        );
                        (
                            Some(inbox),
                            Some(tokio::spawn(poller.run(service.status.clone()))),
                        )
                    } else {
                        (None, None)
                    };
                    let execution = ExecutionControl::new(stop.clone(), service.status.clone())
                        .with_capital_exit(capital_inbox);
                    let completion = execution.completion.clone();
                    let source = out.join("source.ndjson");
                    let report_path = out.join("report.json");
                    let report = report_path.clone();
                    let snapshots = out.join("snapshots.ndjson");
                    let binding = out.join("binding.json");
                    let worker = std::thread::Builder::new()
                        .name("polymarket-paper-owner".into())
                        .spawn(move || {
                            host::execute(
                                &frozen_config,
                                &claim_path,
                                frozen_metadata.as_deref(),
                                frozen_dataset.as_deref(),
                                &source,
                                &report,
                                &snapshots,
                                &binding,
                                config.max_seconds,
                                config.proxy_env.as_deref(),
                                Some(&execution),
                                initial_permit,
                            )
                        });
                    match worker {
                        Ok(worker) => {
                            // An admitted thread remains STARTING until actual native execution.
                            let _ = request.reply.send(Ok(()));
                            while !worker.is_finished() {
                                tokio::select! {
                                    _ = tokio::time::sleep(Duration::from_millis(50)) => {},
                                    _ = service.stop.changed() => { stop.store(true, Ordering::Release); },
                                    _ = &mut signal, if !stop.load(Ordering::Acquire) => {
                                        stop.store(true, Ordering::Release);
                                        service.status.send_modify(|s| s.stop_requested = true);
                                    },
                                }
                            }
                            let result = worker
                                .join()
                                .unwrap_or_else(|_| Err(anyhow!("PAPER_NATIVE_OWNER_PANICKED")));
                            if let Some(task) = capital_task {
                                task.abort();
                                let _ = task.await;
                            }
                            let mut terminal_relay_failed = false;
                            if config.capital_exit {
                                // Native execution has actually joined. Relay its
                                // retained terminal frame; never reconstruct a
                                // balance or mark a local stop as a withdrawal.
                                let terminal: Result<()> = async {
                                    let value: Value = host::read_original(&report_path)?;
                                    let frame: contracts::account_observation::AccountObservationSubmitV2 =
                                        serde_json::from_value(value["capital_exit_terminal_observation"].clone())?;
                                    initial_authority.as_ref()
                                        .ok_or_else(|| anyhow!("CAPITAL_EXIT_EXISTING_MACHINE_CONNECTION_REQUIRED"))?
                                        .capital_exit_transport().submit_observation(&frame).await?;
                                    Ok(())
                                }.await;
                                terminal_relay_failed = terminal.is_err();
                            }
                            let report = completion.lock().ok().and_then(|summary| summary.clone());
                            publish_terminal(
                                &service.status,
                                result.is_ok(),
                                stop.load(Ordering::Acquire),
                                report.as_ref(),
                            );
                            if terminal_relay_failed {
                                service.status.send_modify(|value| value.reason_code =
                                    Some("CAPITAL_EXIT_TERMINAL_OBSERVATION_RETAINED_RELAY_UNCONFIRMED".into()));
                            }
                            result
                        }
                        Err(_) => {
                            if let Some(task) = capital_task {
                                task.abort();
                                let _ = task.await;
                            }
                            let _ = request
                                .reply
                                .send(Err(crate::paper_service::PaperApplyError::Unavailable));
                            publish_terminal(&service.status, false, false, None);
                            Err(anyhow!("PAPER_NATIVE_OWNER_UNAVAILABLE"))
                        }
                    }
                }
            }
        }
    } else {
        service.status.send_modify(|s| {
            s.state = PaperState::Stopped;
            s.stop_requested = true;
        });
        Ok(())
    };
    let journal_retained = service.retain_terminal_observation().await;
    let final_status = serde_json::to_vec(&service.status.borrow().clone())?;
    let retained = retain(&out.join("terminal-status.json"), &final_status);
    println!("{}", String::from_utf8_lossy(&final_status));
    tokio::time::sleep(Duration::from_secs(2)).await;
    service.shutdown.send_replace(true);
    service.task.await??;
    journal_retained?;
    retained?;
    result
}

pub fn run(path: &Path) -> Result<()> {
    let config = host::read_original(path)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(serve(config))
}

pub(crate) fn control(
    origin: &str,
    credential: &Path,
    operation: &str,
    claim: Option<HandoffClaimViewV2>,
) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(crate::paper_node::control_request(
            origin, credential, operation, claim,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn stopped_before_consume_never_contacts_authority_or_constructs_account() {
        let (_files, connection, claim, count, server) =
            crate::paper_service::initial_execution::tests::fixture("first-only").await;
        let config = crate::polymarket_paper_host::tests::config_for(&claim);
        let mut authority = Some(InitialExecutionAuthority::open(&connection).unwrap());
        let (_stop, stopped) = watch::channel(true);
        let stop_latch = AtomicBool::new(false);
        let mut signal = Box::pin(std::future::pending());
        let (status, _) = watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let result = initial_permit_for(
            &config,
            &claim,
            &mut authority,
            &stopped,
            &stop_latch,
            signal.as_mut(),
            &status,
        )
        .await;
        assert_eq!(
            result.err().unwrap().to_string(),
            "PAPER_HOST_STOP_REQUESTED"
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stopped_after_consume_drops_permit_without_reissuing_capital() {
        let (stop, stopped) = watch::channel(false);
        let (_files, connection, claim, count, server) =
            crate::paper_service::initial_execution::tests::fixture_with_stop(
                "first-only",
                Some(stop),
            )
            .await;
        let config = crate::polymarket_paper_host::tests::config_for(&claim);
        let mut authority = Some(InitialExecutionAuthority::open(&connection).unwrap());
        let stop_latch = AtomicBool::new(false);
        let mut signal = Box::pin(std::future::pending());
        let (status, _) = watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let result = initial_permit_for(
            &config,
            &claim,
            &mut authority,
            &stopped,
            &stop_latch,
            signal.as_mut(),
            &status,
        )
        .await;
        assert_eq!(
            result.err().unwrap().to_string(),
            "PAPER_HOST_STOP_REQUESTED"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(authority.as_mut().unwrap().consume(&claim).await.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fixed_signal_handler_cancels_before_delayed_consume_response_without_watch_stop() {
        let received = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let (_files, connection, claim, count, server) =
            crate::paper_service::initial_execution::tests::fixture_with_response_gate(
                "first-only",
                None,
                Some((received.clone(), release.clone())),
            )
            .await;
        let config = crate::polymarket_paper_host::tests::config_for(&claim);
        let mut authority = Some(InitialExecutionAuthority::open(&connection).unwrap());
        let (_api_sender, api_stop) = watch::channel(false);
        let stop_latch = AtomicBool::new(false);
        let (status, mut observed) =
            watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let (send_signal, receive_signal) = tokio::sync::oneshot::channel();
        // Production supplies stop_signal() at exactly this seam. This fixed
        // handler avoids sending an OS signal to unrelated tests in the process.
        let mut signal = Box::pin(async move {
            receive_signal.await.unwrap();
        });
        let attempt = initial_permit_for(
            &config,
            &claim,
            &mut authority,
            &api_stop,
            &stop_latch,
            signal.as_mut(),
            &status,
        );
        let driver = async {
            received.notified().await;
            send_signal.send(()).unwrap();
            observed
                .wait_for(|value| value.stop_requested)
                .await
                .unwrap();
            assert!(stop_latch.load(Ordering::Acquire));
            assert!(!*api_stop.borrow(), "the API watch must remain false");
            release.notify_one();
        };
        let (result, ()) = tokio::join!(attempt, driver);
        assert_eq!(
            result.err().unwrap().to_string(),
            "PAPER_HOST_STOP_REQUESTED"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(authority.as_mut().unwrap().consume(&claim).await.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ready_fixed_signal_prevents_consume_without_watch_stop() {
        let (_files, connection, claim, count, server) =
            crate::paper_service::initial_execution::tests::fixture("first-only").await;
        let config = crate::polymarket_paper_host::tests::config_for(&claim);
        let mut authority = Some(InitialExecutionAuthority::open(&connection).unwrap());
        let (_api_sender, api_stop) = watch::channel(false);
        let stop_latch = AtomicBool::new(false);
        let (status, _) = watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let mut signal = Box::pin(std::future::ready(()));
        let result = initial_permit_for(
            &config,
            &claim,
            &mut authority,
            &api_stop,
            &stop_latch,
            signal.as_mut(),
            &status,
        )
        .await;
        assert_eq!(
            result.err().unwrap().to_string(),
            "PAPER_HOST_STOP_REQUESTED"
        );
        assert!(stop_latch.load(Ordering::Acquire));
        assert!(!*api_stop.borrow());
        assert_eq!(count.load(Ordering::SeqCst), 0);
        server.abort();
    }

    #[test]
    fn terminal_failure_keeps_already_observed_target_consumption() {
        let (status, _) = watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let execution = ExecutionControl::new(Arc::new(AtomicBool::new(false)), status.clone());
        status.send_modify(|s| s.state = PaperState::Starting);
        execution.running(1);
        publish_terminal(&status, false, false, None);
        assert_eq!(status.borrow().state, PaperState::Failed);
        assert_eq!(status.borrow().target_points_consumed, 1);
    }

    #[test]
    fn terminal_summary_is_independent_of_large_retained_report_size() {
        let (status, _) = watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let execution = ExecutionControl::new(Arc::new(AtomicBool::new(false)), status.clone());
        let mut report = json!({"source_process_cleanup_confirmed":true,
            "performance_status":"NATIVE_SIMULATION_AVAILABLE",
            "account_relay_inputs_eligible":true,"consumed_original_target_points":1});
        execution.finished(&report);
        let small = execution.completion.lock().unwrap().clone().unwrap();
        publish_terminal(&status, true, false, Some(&small));
        assert_eq!(status.borrow().state, PaperState::Stopped);
        report["original_portfolio_snapshots"] = json!("x".repeat(8 * 1024 * 1024 + 1));
        execution.finished(&report);
        let large = execution.completion.lock().unwrap().clone().unwrap();
        assert_eq!(large.available, small.available);
        assert_eq!(large.target_points_consumed, small.target_points_consumed);
        publish_terminal(&status, true, false, Some(&large));
        assert_eq!(status.borrow().state, PaperState::Stopped);
        assert_eq!(status.borrow().target_points_consumed, 1);
        assert!(status.borrow().reason_code.is_none());
    }

    #[test]
    fn evidence_directory_prevents_restarting_or_overwriting_claims() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("session");
        reserve_output(&path).unwrap();
        retain(&path.join("accepted-claim.json"), b"original").unwrap();
        assert!(reserve_output(&path).is_err());
        assert!(retain(&path.join("accepted-claim.json"), b"replacement").is_err());
        assert_eq!(
            std::fs::read(path.join("accepted-claim.json")).unwrap(),
            b"original"
        );
    }
    #[test]
    fn cancel_does_not_report_stopped_until_owner_cleanup_is_confirmed() {
        let (status, _) = watch::channel(PaperStatus::idle_with_profile(PaperProfile::Polymarket));
        let stop = Arc::new(AtomicBool::new(false));
        let control = ExecutionControl::new(stop.clone(), status.clone());
        status.send_modify(|s| s.state = PaperState::Starting);
        control.started("actual-native-session".into());
        assert_eq!(status.borrow().state, PaperState::Starting);
        control.running(0);
        stop.store(true, Ordering::Release);
        assert!(control.check_stop().is_err());
        assert_eq!(status.borrow().state, PaperState::Running);
        publish_terminal(&status, false, true, None);
        assert_eq!(status.borrow().state, PaperState::Failed);
        publish_terminal(
            &status,
            false,
            true,
            Some(&ExecutionSummary::from_report(
                &json!({"source_process_cleanup_confirmed":true}),
            )),
        );
        assert_eq!(status.borrow().state, PaperState::Stopped);
        assert_eq!(
            status.borrow().reason_code.as_deref(),
            Some("PAPER_CANCELLED_INCOMPLETE_NO_RELAY")
        );
    }
}
