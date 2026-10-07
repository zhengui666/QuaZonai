//! One original claimed target, one bounded native Cash session, no venue orders.
//! HTTP control reuses the existing authenticated loopback service. The original
//! synchronous engine owns its thread; cancellation never pretends it has joined.
use crate::{
    paper_service::{PaperProfile, PaperState, PaperStatus},
    polymarket_paper_host::{self as host, HostConfig},
};
use anyhow::{anyhow, ensure, Result};
use contracts::{
    catalogs::RuntimeCatalogMetadataV1, data::DatasetView, strategy_portfolio::HandoffClaimViewV2,
    SchemaV1,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::watch;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceConfig {
    schema_version: SchemaV1,
    host_config: PathBuf,
    frozen_metadata: PathBuf,
    dataset_revision: PathBuf,
    /// Exclusive new directory. Reuse/restart is rejected, never replayed.
    output_directory: PathBuf,
    /// Stable private runtime volume, never the per-session output directory.
    claim_state_directory: PathBuf,
    credential_file: PathBuf,
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
}

impl ExecutionControl {
    pub(crate) fn new(stop: Arc<AtomicBool>, status: watch::Sender<PaperStatus>) -> Self {
        Self {
            stop,
            status,
            completion: Arc::new(Mutex::new(None)),
        }
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
    // Read and retain the immutable original inputs once before accepting work.
    let original_config = original_bytes(&config.host_config)?;
    let original_metadata = original_bytes(&config.frozen_metadata)?;
    let original_dataset = original_bytes(&config.dataset_revision)?;
    let host_config: HostConfig = serde_json::from_slice(&original_config)?;
    let _: RuntimeCatalogMetadataV1 = serde_json::from_slice(&original_metadata)?;
    let _: DatasetView = serde_json::from_slice(&original_dataset)?;
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
    let frozen_metadata = out.join("frozen-metadata.json");
    let frozen_dataset = out.join("dataset-revision.json");
    retain(&frozen_config, &original_config)?;
    retain(&frozen_metadata, &original_metadata)?;
    retain(&frozen_dataset, &original_dataset)?;
    let mut service = crate::paper_service::start_control_with_profile(
        config.bind,
        credential,
        host_config.market_capability_version,
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
            let stop = Arc::new(AtomicBool::new(*service.stop.borrow()));
            let execution = ExecutionControl::new(stop.clone(), service.status.clone());
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
                        &frozen_metadata,
                        &frozen_dataset,
                        &source,
                        &report,
                        &snapshots,
                        &binding,
                        config.max_seconds,
                        config.proxy_env.as_deref(),
                        Some(&execution),
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
                    let report = completion.lock().ok().and_then(|summary| summary.clone());
                    publish_terminal(
                        &service.status,
                        result.is_ok(),
                        stop.load(Ordering::Acquire),
                        report.as_ref(),
                    );
                    result
                }
                Err(_) => {
                    let _ = request
                        .reply
                        .send(Err(crate::paper_service::PaperApplyError::Unavailable));
                    publish_terminal(&service.status, false, false, None);
                    Err(anyhow!("PAPER_NATIVE_OWNER_UNAVAILABLE"))
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
