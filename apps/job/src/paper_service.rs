//! Loopback control for one foreground native Paper session.
//!
//! The native node stays on its owning thread. HTTP handlers only exchange typed
//! requests and observed status; this module never submits orders or records ACKs.
use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration};

use crate::paper_claim_store::{Admission, ClaimLease, ClaimStore, StoreError};

#[path = "paper_initial_execution.rs"]
pub(crate) mod initial_execution;

use axum::{
    extract::{rejection::JsonRejection, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use contracts::{
    delivery::{DownstreamCapabilitiesV1, DownstreamDeliveryModeV1, HandoffStateV1},
    forward::ForwardEnvironmentV1,
    science::NativeAccountKind,
    settings::PackageSchemaVersion,
    strategy_portfolio::{HandoffClaimViewV2, TargetPackageEnvelopeV2},
    Id, SchemaV1,
};
use nautilus_model::identifiers::{InstrumentId, Venue};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, oneshot, watch, Mutex},
    task::JoinHandle,
};

/// The control profile describes its native owner, never an exchange account.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaperProfile {
    #[default]
    Binance,
    Polymarket,
}

impl PaperProfile {
    fn journal_adapter(self) -> &'static str {
        match self {
            Self::Binance => "binance",
            Self::Polymarket => "polymarket",
        }
    }

    fn package_versions(self) -> Vec<PackageSchemaVersion> {
        match self {
            Self::Binance => vec![PackageSchemaVersion::V2],
            Self::Polymarket => vec![PackageSchemaVersion::V2],
        }
    }

    /// Profile admission is deliberately narrower than host source validation.
    /// The owner still checks frozen originals, fees, instruments and provenance.
    fn accepts(self, package: &TargetPackageEnvelopeV2) -> bool {
        match (self, package) {
            (Self::Binance, _) => true,
            (Self::Polymarket, TargetPackageEnvelopeV2::Forecast(package)) => {
                package.execution_settings.account_kind == NativeAccountKind::Cash
                    && package.execution_settings.leverage.as_decimal()
                        == &bigdecimal::BigDecimal::from(1)
                    && package.targets.iter().all(|target| {
                        target
                            .instrument_id
                            .parse::<InstrumentId>()
                            .is_ok_and(|id| id.venue == Venue::from("POLYMARKET"))
                    })
            }
            (Self::Polymarket, TargetPackageEnvelopeV2::TargetDecision(package)) => {
                package.account_start.account_id == "POLYMARKET-001"
                    && package.execution_settings.account_kind == NativeAccountKind::Cash
                    && package.execution_settings.leverage.as_decimal()
                        == &bigdecimal::BigDecimal::from(1)
                    && package.targets.iter().all(|target| {
                        target
                            .instrument_id
                            .parse::<InstrumentId>()
                            .is_ok_and(|id| id.venue == Venue::from("POLYMARKET"))
                    })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaperState {
    Idle,
    Starting,
    Running,
    Stopped,
    Failed,
}

/// Observed native lifecycle, not proof of QZ approval, fills or account recovery.
/// A completed claim can replay the original observation after process restart.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperStatus {
    pub schema_version: SchemaV1,
    pub native_account_model: String,
    pub restart_policy: String,
    pub claim_replay_scope: String,
    /// Native strategy consumption is separate from lifecycle and fill receipts.
    pub target_points_consumed: usize,
    pub state: PaperState,
    pub execution_environment: String,
    pub market_data_source: String,
    pub market_time_basis: String,
    pub fee_basis: String,
    pub latency_basis: String,
    #[serde(deserialize_with = "required_option")]
    pub handoff_id: Option<Id>,
    #[serde(deserialize_with = "required_option")]
    pub release_id: Option<Id>,
    #[serde(deserialize_with = "required_option")]
    pub external_claim_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub native_session_id: Option<String>,
    pub stop_requested: bool,
    #[serde(deserialize_with = "required_option")]
    pub reason_code: Option<String>,
    pub updated_at: DateTime<Utc>,
}

// Serde normally treats a missing Option field as None. Durable V1 records
// require the key to exist; an explicit null is valid where the model allows it.
fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl PaperStatus {
    pub fn idle() -> Self {
        Self::idle_with_profile(PaperProfile::Binance)
    }

    pub fn idle_with_profile(profile: PaperProfile) -> Self {
        let mut status = Self {
            schema_version: SchemaV1,
            native_account_model: "MARGIN_LEVERAGE_ONE".into(),
            restart_policy: "FRESH_ACCOUNT_AND_SESSION_NO_RESTORE".into(),
            claim_replay_scope: "DURABLE_RUNTIME_PROJECT_ADAPTER_CLAIM".into(),
            target_points_consumed: 0,
            state: PaperState::Idle,
            execution_environment: "PAPER_SANDBOX".into(),
            market_data_source: "BINANCE_SPOT_PUBLIC_JSON".into(),
            market_time_basis:
                "NATIVE_ADAPTER_TIMESTAMPS_WITH_RECEIVE_TIME_FALLBACK_WHEN_EXCHANGE_TIME_ABSENT"
                    .into(),
            fee_basis: "native default maker/taker 0.001; simulated, not account-specific".into(),
            latency_basis: "native wall clock; historical StaticLatencyModel not applied".into(),
            handoff_id: None,
            release_id: None,
            external_claim_id: None,
            native_session_id: None,
            stop_requested: false,
            reason_code: None,
            updated_at: Utc::now(),
        };
        if profile == PaperProfile::Polymarket {
            status.native_account_model = "CASH".into();
            status.execution_environment = "PAPER_SIMULATION".into();
            status.market_data_source = "POLYMARKET_REAL_PUBLIC_DATA".into();
            status.market_time_basis = "ORIGINAL_NATIVE_TS_EVENT_AND_TS_INIT".into();
            status.fee_basis =
                "UNCHANGED_OFFICIAL_POLYMARKET_FEE_MODEL_FROM_ORIGINAL_SCHEDULE".into();
            status.latency_basis = "ORIGINAL_FROZEN_STATIC_LATENCY_IN_NATIVE_EVENT_TIME".into();
        }
        status
    }
}

/// Only fixed public failure codes cross the native/HTTP boundary.
#[derive(Clone, Copy, Debug)]
pub enum PaperApplyError {
    InvalidClaim,
    Stopped,
    UnsupportedForecastInitialization,
    UnsupportedForecastContinuation,
    UnsupportedForecastConstraints,
    UnsupportedForecastAssets,
    InitialExecutionConfigurationRequired,
    InitialExecutionBlocked,
    InitialExecutionUnknown,
    Unavailable,
}

impl PaperApplyError {
    fn code(self) -> &'static str {
        match self {
            Self::InvalidClaim => "invalid_claim",
            Self::Stopped => "PAPER_HOST_STOP_REQUESTED",
            Self::UnsupportedForecastInitialization => {
                "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED"
            }
            Self::UnsupportedForecastContinuation => "PAPER_FORECAST_CONTINUATION_UNSUPPORTED",
            Self::UnsupportedForecastConstraints => {
                "PAPER_FORECAST_CONSTRAINT_MEASUREMENT_UNSUPPORTED"
            }
            Self::UnsupportedForecastAssets => "PAPER_FORECAST_ASSET_SCOPE_UNSUPPORTED",
            Self::InitialExecutionConfigurationRequired => {
                "PAPER_INITIAL_EXECUTION_CONNECTION_REQUIRED"
            }
            Self::InitialExecutionBlocked => "PAPER_INITIAL_EXECUTION_BLOCKED",
            Self::InitialExecutionUnknown => "PAPER_INITIAL_EXECUTION_RESULT_UNKNOWN",
            Self::Unavailable => "native_unavailable",
        }
    }
}

pub struct ApplyRequest {
    pub claim: HandoffClaimViewV2,
    /// Successful construction is still Starting. The owner separately publishes
    /// Running only when it observes the native node running.
    pub reply: oneshot::Sender<Result<(), PaperApplyError>>,
}

pub struct ControlServer {
    pub local_addr: SocketAddr,
    pub requests: mpsc::Receiver<ApplyRequest>,
    pub status: watch::Sender<PaperStatus>,
    /// Stop intent only; the owner must confirm actual native termination.
    pub stop: watch::Receiver<bool>,
    /// Close HTTP after publishing the final native state. Separate from stop so
    /// callers can observe termination before the foreground process exits.
    pub shutdown: watch::Sender<bool>,
    pub task: JoinHandle<std::io::Result<()>>,
    claim: Arc<Mutex<Option<SessionClaim>>>,
}

struct SessionClaim {
    original: serde_json::Value,
    lease: Option<ClaimLease>,
    receipt: Option<Vec<u8>>,
}

impl ControlServer {
    /// The native owner must call this after it has actually stopped/joined.
    /// A failed write leaves the durable reservation unresolved and returns an
    /// error; it never permits a retry of the original execution.
    pub async fn retain_terminal_observation(&self) -> anyhow::Result<()> {
        let mut claim = self.claim.lock().await;
        let Some(claim) = claim.as_mut() else {
            return Ok(());
        };
        let Some(lease) = claim.lease.as_ref() else {
            return Ok(());
        };
        let receipt = serde_json::to_vec(&self.status.borrow().clone())?;
        lease
            .complete(&receipt)
            .map_err(|error| anyhow::anyhow!(error.code()))?;
        claim.receipt = Some(receipt);
        claim.lease = None;
        Ok(())
    }
}

#[derive(Clone)]
struct ServiceState {
    authorization: HeaderValue,
    market_capability: String,
    profile: PaperProfile,
    claim: Arc<Mutex<Option<SessionClaim>>>,
    store: ClaimStore,
    #[cfg(test)]
    _temporary_state: Option<Arc<tempfile::TempDir>>,
    requests: mpsc::Sender<ApplyRequest>,
    status: watch::Sender<PaperStatus>,
    stop: watch::Sender<bool>,
}

/// Bind only a literal loopback socket; exposure to QZ uses its existing trusted
/// reverse proxy. Credentials are read by the caller, never created here.
pub async fn start_control(
    bind: SocketAddr,
    credential: Vec<u8>,
    market_capability: String,
    claim_state_directory: &Path,
) -> anyhow::Result<ControlServer> {
    start_control_with_profile(
        bind,
        credential,
        market_capability,
        PaperProfile::Binance,
        claim_state_directory,
    )
    .await
}

/// Bind the same authenticated controls with explicit native-owner semantics.
pub async fn start_control_with_profile(
    bind: SocketAddr,
    credential: Vec<u8>,
    market_capability: String,
    profile: PaperProfile,
    claim_state_directory: &Path,
) -> anyhow::Result<ControlServer> {
    anyhow::ensure!(bind.ip().is_loopback(), "paper_control_requires_loopback");
    anyhow::ensure!(
        !credential.is_empty() && credential.iter().all(|byte| byte.is_ascii_graphic()),
        "paper_control_invalid_credential"
    );
    anyhow::ensure!(
        !market_capability.is_empty()
            && market_capability.trim() == market_capability
            && !market_capability.chars().any(char::is_control),
        "paper_control_invalid_market_capability"
    );
    let mut bearer = b"Bearer ".to_vec();
    bearer.extend_from_slice(&credential);
    let mut authorization = HeaderValue::from_bytes(&bearer)
        .map_err(|_| anyhow::anyhow!("paper_control_invalid_credential"))?;
    authorization.set_sensitive(true);
    let store = ClaimStore::open(claim_state_directory, profile.journal_adapter())
        .map_err(|error| anyhow::anyhow!(error.code()))?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|_| anyhow::anyhow!("paper_control_bind_failed"))?;
    let local_addr = listener.local_addr()?;
    let (requests_tx, requests) = mpsc::channel(1);
    let (status, _) = watch::channel(PaperStatus::idle_with_profile(profile));
    let (stop_tx, stop) = watch::channel(false);
    let (shutdown, mut shutdown_rx) = watch::channel(false);
    let claim = Arc::new(Mutex::new(None));
    let state = ServiceState {
        authorization,
        market_capability,
        profile,
        claim: claim.clone(),
        store,
        #[cfg(test)]
        _temporary_state: None,
        requests: requests_tx,
        status: status.clone(),
        stop: stop_tx,
    };
    let app = Router::new()
        .route("/downstream/v1/capabilities", get(capabilities))
        .route("/downstream/v1/targets", post(apply))
        .route("/downstream/v1/status", get(status_snapshot))
        .route("/downstream/v1/stop", post(stop_session))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state);
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                while !*shutdown_rx.borrow() {
                    if shutdown_rx.changed().await.is_err() {
                        break;
                    }
                }
            })
            .await
    });
    Ok(ControlServer {
        local_addr,
        requests,
        status,
        stop,
        shutdown,
        task,
        claim,
    })
}

async fn authenticate(State(state): State<ServiceState>, request: Request, next: Next) -> Response {
    let mut values = request.headers().get_all(header::AUTHORIZATION).iter();
    let authorized = values.next() == Some(&state.authorization) && values.next().is_none();
    let mut response = if authorized {
        next.run(request).await
    } else {
        let mut response = failure(StatusCode::UNAUTHORIZED, "unauthorized");
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        response
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn capabilities(State(state): State<ServiceState>) -> Json<DownstreamCapabilitiesV1> {
    Json(DownstreamCapabilitiesV1 {
        schema_version: SchemaV1,
        delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
        accepted_package_versions: state.profile.package_versions(),
        environments: vec![ForwardEnvironmentV1::Paper],
        market_capability_versions: vec![state.market_capability.clone()],
        accepting_targets: state.status.borrow().state == PaperState::Idle
            && !*state.stop.borrow()
            && !state.requests.is_closed(),
        checked_at: Utc::now(),
    })
}

fn snapshot(state: &ServiceState) -> PaperStatus {
    let mut status = state.status.borrow().clone();
    status.stop_requested |= *state.stop.borrow();
    status
}

async fn status_snapshot(State(state): State<ServiceState>) -> Json<PaperStatus> {
    Json(snapshot(&state))
}

fn status_response(state: &ServiceState) -> Response {
    let status = snapshot(state);
    let code = if status.state == PaperState::Starting {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    (code, Json(status)).into_response()
}

async fn apply(
    State(state): State<ServiceState>,
    body: Result<Json<HandoffClaimViewV2>, JsonRejection>,
) -> Response {
    let claim = match body {
        Ok(Json(claim)) => claim,
        Err(_) => return failure(StatusCode::UNPROCESSABLE_ENTITY, "invalid_claim"),
    };
    let value = match serde_json::to_value(&claim) {
        Ok(value) => value,
        Err(_) => return failure(StatusCode::UNPROCESSABLE_ENTITY, "invalid_claim"),
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    {
        // Reservation and stop share this lock; once a claim is admitted, this
        // process never admits another, even after a failure or stop.
        let mut original = state.claim.lock().await;
        if let Some(original) = original.as_ref() {
            return if original.original == value {
                if let Some(receipt) = &original.receipt {
                    receipt_response(receipt.clone())
                } else {
                    status_response(&state)
                }
            } else {
                failure(StatusCode::CONFLICT, "paper_session_claim_conflict")
            };
        }
        if *state.stop.borrow() || state.status.borrow().state != PaperState::Idle {
            return failure(StatusCode::CONFLICT, "paper_session_not_idle");
        }
        let valid = valid_claim(&claim, &state.market_capability, state.profile);
        let lease = match state.store.admit(&claim, valid) {
            Ok(Some(Admission::Reserved(lease))) => lease,
            Ok(Some(Admission::Replay(receipt))) => return receipt_response(receipt),
            Ok(None) => return failure(StatusCode::UNPROCESSABLE_ENTITY, "invalid_claim"),
            Err(error) => {
                let code = if error == StoreError::Unavailable {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::CONFLICT
                };
                return failure(code, error.code());
            }
        };
        let status = PaperStatus {
            target_points_consumed: 0,
            state: PaperState::Starting,
            handoff_id: Some(claim.handoff.id),
            release_id: Some(claim.handoff.release_id),
            external_claim_id: claim.handoff.external_claim_id.clone(),
            ..PaperStatus::idle_with_profile(state.profile)
        };
        *original = Some(SessionClaim {
            original: value,
            lease: Some(lease),
            receipt: None,
        });
        state.status.send_replace(status);
        if state
            .requests
            .try_send(ApplyRequest {
                claim,
                reply: reply_tx,
            })
            .is_err()
        {
            fail_starting(&state, "native_unavailable");
            return failure(StatusCode::SERVICE_UNAVAILABLE, "native_unavailable");
        }
    }
    match tokio::time::timeout(Duration::from_secs(10), reply_rx).await {
        Ok(Ok(Ok(()))) => status_response(&state),
        Ok(Ok(Err(error))) => {
            fail_starting(&state, error.code());
            let code = match error {
                PaperApplyError::InvalidClaim
                | PaperApplyError::UnsupportedForecastInitialization
                | PaperApplyError::UnsupportedForecastContinuation
                | PaperApplyError::UnsupportedForecastConstraints
                | PaperApplyError::UnsupportedForecastAssets => StatusCode::UNPROCESSABLE_ENTITY,
                PaperApplyError::Stopped | PaperApplyError::InitialExecutionBlocked => {
                    StatusCode::CONFLICT
                }
                PaperApplyError::InitialExecutionConfigurationRequired
                | PaperApplyError::InitialExecutionUnknown
                | PaperApplyError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            failure(code, error.code())
        }
        // A missing reply does not prove the native node failed or stopped.
        Ok(Err(_)) => failure(StatusCode::SERVICE_UNAVAILABLE, "apply_result_unavailable"),
        Err(_) => status_response(&state),
    }
}

fn receipt_response(receipt: Vec<u8>) -> Response {
    // Exact bytes recorded by the original owner, never the new process's Idle
    // snapshot or an invented successful execution/fill receipt.
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (
                axum::http::HeaderName::from_static("x-paper-claim-replayed"),
                "true",
            ),
        ],
        receipt,
    )
        .into_response()
}

fn fail_starting(state: &ServiceState, reason: &str) {
    state.status.send_modify(|status| {
        if status.state == PaperState::Starting {
            status.state = PaperState::Failed;
            status.reason_code = Some(reason.to_owned());
            status.updated_at = Utc::now();
        }
    });
}

async fn stop_session(State(state): State<ServiceState>) -> Response {
    let _original = state.claim.lock().await;
    state.stop.send_replace(true);
    state.status.send_modify(|status| {
        status.stop_requested = true;
        status.updated_at = Utc::now();
        if status.state == PaperState::Idle {
            // No request has been admitted, so no native session exists to stop.
            status.state = PaperState::Stopped;
        }
    });
    let status = snapshot(&state);
    let code = if matches!(status.state, PaperState::Starting | PaperState::Running) {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    (code, Json(status)).into_response()
}

fn valid_claim(claim: &HandoffClaimViewV2, capability: &str, profile: PaperProfile) -> bool {
    let handoff = &claim.handoff;
    let package = domain::delivery::package_delivery(&claim.package);
    let now = Utc::now();
    handoff.environment == ForwardEnvironmentV1::Paper
        && profile.accepts(&claim.package)
        && handoff.state == HandoffStateV1::Claimed
        && handoff.release_id == package.release_id
        && handoff.project_id == package.project_id
        && handoff.candidate_id == package.candidate_id
        && handoff.mandate_id == package.mandate_id
        && handoff.external_claim_id.as_ref().is_some_and(|id| {
            !id.is_empty() && id.trim() == id && !id.chars().any(char::is_control)
        })
        && handoff
            .claimed_at
            .is_some_and(|at| at >= handoff.offered_at && at < handoff.expires_at && at <= now)
        && handoff.acknowledged_at.is_none()
        && match &claim.package {
            TargetPackageEnvelopeV2::Forecast(p) => p
                .current_weights
                .paper_initialization
                .as_ref()
                .is_none_or(|root| {
                    root.downstream_id == handoff.downstream_id
                        && p.source.build_environment == ForwardEnvironmentV1::Paper
                        && p.environment_origin == contracts::delivery::PackageOriginV1::Synthetic
                }),
            TargetPackageEnvelopeV2::TargetDecision(p) => {
                p.execution_environment == ForwardEnvironmentV1::Paper
                    && p.account_start.downstream_id == handoff.downstream_id
            }
        }
        && package.asof <= package.valid_from
        && package.valid_from < package.valid_until
        && package.valid_until > now
        && !package.targets.is_empty()
        && package
            .compatible_market_capabilities
            .iter()
            .any(|value| value == capability)
}

fn failure(status: StatusCode, code: &'static str) -> Response {
    #[derive(Serialize)]
    struct Failure {
        code: &'static str,
    }
    (status, Json(Failure { code })).into_response()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn private_state_directory() -> tempfile::TempDir {
        let mut builder = tempfile::Builder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        builder.tempdir().unwrap()
    }

    pub(crate) fn claim() -> HandoffClaimViewV2 {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contracts/allocation-input.json"
        ))
        .unwrap();
        let now = Utc::now();
        let project = Id::new();
        let candidate = Id::new();
        let mandate = Id::new();
        let release = Id::new();
        let dataset = Id::new();
        let metadata = Id::new();
        let weights = Id::new();
        let parameters = Id::new();
        let report = Id::new();
        serde_json::from_value(json!({
            "handoff": {
                "id": Id::new(), "project_id": project, "candidate_id": candidate,
                "mandate_id": mandate, "release_id": release, "approval_id": Id::new(),
                "downstream_id": Id::new(), "environment": "PAPER",
                "delivery_sequence": "1", "revision": "1", "state": "CLAIMED",
                "supersedes_handoff_id": null, "external_claim_id": "service-test-claim",
                "offered_at": now - chrono::Duration::seconds(30),
                "claimed_at": now - chrono::Duration::seconds(20),
                "expires_at": now - chrono::Duration::seconds(10),
                "acknowledged_at": null
            },
            "package": {
                "release_id": release, "package_schema_version": "2",
                "source_kind": "FORECAST_EVALUATION",
                "source": {
                    "build_run_id": Id::new(), "build_accepted_attempt_id": Id::new(),
                    "build_parameters_artifact_id": parameters, "build_report_artifact_id": report,
                    "build_input_set_id": Id::new(), "build_environment": "LIVE",
                    "forward_dataset_revision_id": dataset, "forward_metadata_artifact_id": metadata,
                    "current_weights_artifact_id": weights
                },
                "forward_dataset": {
                    "dataset_revision_id": dataset, "native_metadata_artifact_id": metadata,
                    "storage_version": "synthetic-control-only/1", "data_kind": "BAR",
                    "partition": "FORWARD", "origin": "SYNTHETIC", "pit_status": "UNVERIFIED",
                    "revision_policy": "UNKNOWN", "event_start": now - chrono::Duration::seconds(20),
                    "event_end": now, "available_through": now, "row_count": "1",
                    "selection": { "schema_version": 1, "bar_types": ["ALPHA.EXAMPLE-1-SECOND-LAST-EXTERNAL"],
                        "event_start_ns": "1", "event_end_ns": "2", "decision_cutoff_ns": "2", "maximum_rows": 1 },
                    "instrument_definitions": []
                },
                "current_weights": { "schema_version": 1,
                    "source": {"kind": "LAST_TARGET", "candidate_id": Id::new()},
                    "asof_ns": "1", "available_ns": "1", "valid_until_ns": "999999999999999999",
                    "base_currency": "USD", "cash_weight": "0",
                    "weights": [{"instrument_id": "ALPHA.EXAMPLE", "weight": "1", "currency": "USD"}]
                },
                "execution_settings": {
                    "schema_version": 1, "base_currency": "USD", "starting_capital": "1000",
                    "account_kind": "MARGIN", "leverage": "1", "snapshot_interval_ms": 1000,
                    "exposure_tolerance": "0.000001", "fee_rates": [],
                    "fee_model": { "schema_version": 1, "adapter_kind": "NAUTILUS_MAKER_TAKER",
                        "upstream_class": contracts::portfolio::NAUTILUS_FEE_CLASS,
                        "upstream_version": contracts::portfolio::NAUTILUS_EXECUTION_VERSION, "parameters": {} },
                    "fill_model": { "schema_version": 1, "adapter_kind": "NAUTILUS_DEFAULT_FILL",
                        "upstream_class": contracts::portfolio::NAUTILUS_FILL_CLASS,
                        "upstream_version": contracts::portfolio::NAUTILUS_EXECUTION_VERSION,
                        "parameters": { "prob_fill_on_limit": "1", "prob_slippage": "0", "random_seed": "1" } },
                    "latency_model": { "schema_version": 1, "adapter_kind": "NAUTILUS_STATIC_LATENCY",
                        "upstream_class": contracts::portfolio::NAUTILUS_LATENCY_CLASS,
                        "upstream_version": contracts::portfolio::NAUTILUS_EXECUTION_VERSION,
                        "parameters": { "base_latency_ns": "0", "insert_latency_ns": "0", "update_latency_ns": "0", "cancel_latency_ns": "0" } }
                },
                "environment_origin": "DEMO", "project_id": project,
                "candidate_id": candidate, "mandate_id": mandate,
                "qualification_refs": [Id::new(), Id::new()],
                "evaluation_refs": [Id::new()], "input_revision_refs": [dataset],
                "engine_versions": {"test": "1"}, "asof": now,
                "valid_from": now + chrono::Duration::seconds(10),
                "valid_until": now + chrono::Duration::seconds(300),
                "base_currency": "USD", "capital_assumption": "1000",
                "current_weights_source": "LAST_TARGET",
                "targets": [{"instrument_id": "ALPHA.EXAMPLE",
                    "target_weight": "1", "currency": "USD"}],
                "cash_weight": "0", "constraints_summary": input["constraints"],
                "exposure_tolerance": "0.000001", "cost_assumption_ref": Id::new(),
                "compatible_market_capabilities": ["service-test/1"],
                "limitations": [], "provenance_artifact_refs": [parameters, report, weights, metadata]
            }
        }))
        .unwrap()
    }

    fn state() -> (ServiceState, mpsc::Receiver<ApplyRequest>) {
        state_with_profile(PaperProfile::Binance)
    }

    fn state_with_profile(profile: PaperProfile) -> (ServiceState, mpsc::Receiver<ApplyRequest>) {
        let root = Arc::new(private_state_directory());
        let (mut state, receiver) = state_with_store(profile, root.path());
        state._temporary_state = Some(root);
        (state, receiver)
    }

    fn state_with_store(
        profile: PaperProfile,
        root: &Path,
    ) -> (ServiceState, mpsc::Receiver<ApplyRequest>) {
        let (requests, receiver) = mpsc::channel(1);
        let (status, _) = watch::channel(PaperStatus::idle_with_profile(profile));
        let (stop, _) = watch::channel(false);
        (
            ServiceState {
                authorization: HeaderValue::from_static("Bearer test-only"),
                market_capability: "service-test/1".into(),
                profile,
                claim: Arc::new(Mutex::new(None)),
                store: ClaimStore::open(root, profile.journal_adapter()).unwrap(),
                _temporary_state: None,
                requests,
                status,
                stop,
            },
            receiver,
        )
    }

    #[test]
    fn active_claim_rejects_legacy_version_even_with_v2_fields() {
        let original = claim();
        let mut legacy = serde_json::to_value(&original).unwrap();
        legacy["package"]["package_schema_version"] = json!("1");
        assert!(serde_json::from_value::<HandoffClaimViewV2>(legacy).is_err());
        let mut missing = serde_json::to_value(&original).unwrap();
        missing["package"]
            .as_object_mut()
            .unwrap()
            .remove("source_kind");
        assert!(serde_json::from_value::<HandoffClaimViewV2>(missing).is_err());
    }

    #[test]
    fn claim_deadline_is_not_execution_deadline() {
        let mut claim = claim();
        assert!(valid_claim(&claim, "service-test/1", PaperProfile::Binance));
        assert!(domain::delivery::package_delivery(&claim.package).valid_from > Utc::now());
        claim.handoff.claimed_at = Some(claim.handoff.expires_at);
        assert!(!valid_claim(
            &claim,
            "service-test/1",
            PaperProfile::Binance
        ));
        claim.handoff.claimed_at = Some(claim.handoff.offered_at);
        let TargetPackageEnvelopeV2::Forecast(package) = &mut claim.package else {
            panic!("Forecast V2 fixture");
        };
        package.valid_until = Utc::now() - chrono::Duration::seconds(1);
        assert!(!valid_claim(
            &claim,
            "service-test/1",
            PaperProfile::Binance
        ));
    }

    /// Synthetic control-boundary fixture; it is not an original market claim
    /// and intentionally provides no host/source validation evidence.
    pub(crate) fn polymarket_claim() -> HandoffClaimViewV2 {
        use contracts::portfolio::{
            NAUTILUS_EXECUTION_VERSION, NAUTILUS_FILL_CLASS, NAUTILUS_LATENCY_CLASS,
            NAUTILUS_POLYMARKET_FEE_CLASS,
        };

        let mut value = serde_json::to_value(claim()).unwrap();
        let downstream = value["handoff"]["downstream_id"].clone();
        let package = value["package"].as_object_mut().unwrap();
        for key in [
            "environment_origin",
            "qualification_refs",
            "evaluation_refs",
            "current_weights_source",
            "current_weights",
            "forward_dataset",
        ] {
            package.remove(key);
        }
        let assumptions = Id::new();
        let dataset = Id::new();
        package.insert("package_schema_version".into(), json!("2"));
        package.insert("source_kind".into(), json!("NATIVE_TARGET_DECISION"));
        package.insert("execution_environment".into(), json!("PAPER"));
        package.insert("base_currency".into(), json!("pUSD"));
        package.insert("input_revision_refs".into(), json!([dataset]));
        package.insert("cost_assumption_ref".into(), json!(assumptions));
        package.insert(
            "source".into(),
            json!({
                "run_id": Id::new(), "accepted_attempt_id": Id::new(),
                "report_artifact_id": Id::new(), "alpha_version_ids": [Id::new()],
                "input_provenance": {
                    "dataset_revision_id": dataset, "market_data_origin": "SYNTHETIC",
                    "pit_status": "UNVERIFIED", "revision_policy": "UNKNOWN",
                    "feature_artifact_origins": {}
                }
            }),
        );
        package.insert(
            "account_start".into(),
            json!({
                "downstream_id": downstream, "trader_id": "TEST-001",
                "account_id": "POLYMARKET-001", "base_currency": "pUSD",
                "starting_capital": "1000", "execution_assumptions_id": assumptions
            }),
        );
        package.insert(
            "targets".into(),
            json!([{
                "instrument_id": "synthetic-condition-1.POLYMARKET",
                "target_weight": "1", "currency": "pUSD"
            }]),
        );
        package.insert(
            "execution_settings".into(),
            json!({
                "schema_version": 1, "base_currency": "pUSD", "starting_capital": "1000",
                "account_kind": "CASH", "leverage": "1", "snapshot_interval_ms": 1000,
                "exposure_tolerance": "0.000001",
                "fee_rates": [{
                    "instrument_id": "synthetic-condition-1.POLYMARKET",
                    "maker": "0", "taker": "0"
                }],
                "fee_model": {
                    "schema_version": 1, "adapter_kind": "NAUTILUS_POLYMARKET",
                    "upstream_class": NAUTILUS_POLYMARKET_FEE_CLASS,
                    "upstream_version": NAUTILUS_EXECUTION_VERSION, "parameters": {}
                },
                "fill_model": {
                    "schema_version": 1, "adapter_kind": "NAUTILUS_DEFAULT_FILL",
                    "upstream_class": NAUTILUS_FILL_CLASS,
                    "upstream_version": NAUTILUS_EXECUTION_VERSION,
                    "parameters": {
                        "prob_fill_on_limit": "1", "prob_slippage": "0", "random_seed": "1"
                    }
                },
                "latency_model": {
                    "schema_version": 1, "adapter_kind": "NAUTILUS_STATIC_LATENCY",
                    "upstream_class": NAUTILUS_LATENCY_CLASS,
                    "upstream_version": NAUTILUS_EXECUTION_VERSION,
                    "parameters": {
                        "base_latency_ns": "100", "insert_latency_ns": "200",
                        "update_latency_ns": "300", "cancel_latency_ns": "400"
                    }
                }
            }),
        );
        serde_json::from_value(value).unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn profiles_advertise_only_supported_versions_and_paper() {
        for (profile, versions) in [
            (PaperProfile::Binance, vec![PackageSchemaVersion::V2]),
            (PaperProfile::Polymarket, vec![PackageSchemaVersion::V2]),
        ] {
            let (state, _requests) = state_with_profile(profile);
            let result = capabilities(State(state)).await.0;
            assert_eq!(result.accepted_package_versions, versions);
            assert_eq!(result.environments, vec![ForwardEnvironmentV1::Paper]);
            assert_eq!(result.delivery_mode, DownstreamDeliveryModeV1::TargetOnly);
            assert_eq!(result.market_capability_versions, vec!["service-test/1"]);
            assert!(result.accepting_targets);
        }
    }

    fn assert_polymarket_status(status: &PaperStatus) {
        assert_eq!(status.native_account_model, "CASH");
        assert_eq!(status.execution_environment, "PAPER_SIMULATION");
        assert_eq!(
            status.restart_policy,
            "FRESH_ACCOUNT_AND_SESSION_NO_RESTORE"
        );
        assert_eq!(
            status.claim_replay_scope,
            "DURABLE_RUNTIME_PROJECT_ADAPTER_CLAIM"
        );
        assert_eq!(status.market_data_source, "POLYMARKET_REAL_PUBLIC_DATA");
        assert_eq!(
            status.market_time_basis,
            "ORIGINAL_NATIVE_TS_EVENT_AND_TS_INIT"
        );
        assert_eq!(
            status.fee_basis,
            "UNCHANGED_OFFICIAL_POLYMARKET_FEE_MODEL_FROM_ORIGINAL_SCHEDULE"
        );
        assert_eq!(
            status.latency_basis,
            "ORIGINAL_FROZEN_STATIC_LATENCY_IN_NATIVE_EVENT_TIME"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn polymarket_rejects_incompatible_claims_without_reserving_session() {
        let (state, mut requests) = state_with_profile(PaperProfile::Polymarket);
        let valid = polymarket_claim();
        assert!(valid_claim(
            &valid,
            "service-test/1",
            PaperProfile::Polymarket
        ));
        let mut invalid = vec![claim()];
        for changed_field in 0..8 {
            let mut rejected = valid.clone();
            let TargetPackageEnvelopeV2::TargetDecision(package) = &mut rejected.package else {
                panic!("V2 fixture");
            };
            match changed_field {
                0 => package.account_start.account_id = "BINANCE-001".into(),
                1 => package.execution_settings.account_kind = NativeAccountKind::Margin,
                2 => package.execution_settings.leverage = "2".parse().unwrap(),
                3 => package.targets[0].instrument_id = "BTCUSDT.BINANCE".into(),
                4 => package.targets[0].instrument_id = "missing-venue".into(),
                5 => {
                    let mut other = package.targets[0].clone();
                    other.instrument_id = "OTHER.BINANCE".into();
                    package.targets.push(other);
                }
                6 => package.account_start.downstream_id = Id::new(),
                7 => package.targets.clear(),
                _ => unreachable!(),
            }
            invalid.push(rejected);
        }
        for rejected in invalid {
            assert_eq!(
                apply(State(state.clone()), Ok(Json(rejected)))
                    .await
                    .status(),
                StatusCode::UNPROCESSABLE_ENTITY
            );
            assert!(state.claim.lock().await.is_none());
            assert_eq!(snapshot(&state).state, PaperState::Idle);
            assert_polymarket_status(&snapshot(&state));
            assert!(requests.try_recv().is_err());
            assert!(capabilities(State(state.clone())).await.0.accepting_targets);
        }
        let native = async { requests.recv().await.unwrap().reply.send(Ok(())).unwrap() };
        let (response, ()) = tokio::join!(apply(State(state.clone()), Ok(Json(valid))), native);
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(snapshot(&state).state, PaperState::Starting);
        assert_polymarket_status(&snapshot(&state));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn polymarket_exact_claim_runs_once_and_only_owner_can_publish_running() {
        let (state, mut requests) = state_with_profile(PaperProfile::Polymarket);
        let claim = polymarket_claim();
        assert_polymarket_status(&snapshot(&state));
        let native = async {
            let request = requests.recv().await.unwrap();
            assert_eq!(request.claim.handoff.id, claim.handoff.id);
            request.reply.send(Ok(())).unwrap();
        };
        let (response, ()) =
            tokio::join!(apply(State(state.clone()), Ok(Json(claim.clone()))), native);
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(snapshot(&state).state, PaperState::Starting);
        assert_polymarket_status(&snapshot(&state));
        assert_eq!(
            apply(State(state.clone()), Ok(Json(claim.clone())))
                .await
                .status(),
            StatusCode::ACCEPTED
        );
        assert!(requests.try_recv().is_err());
        assert_eq!(snapshot(&state).state, PaperState::Starting);
        assert!(!capabilities(State(state.clone())).await.0.accepting_targets);

        state
            .status
            .send_modify(|status| status.state = PaperState::Running);
        assert_eq!(
            apply(State(state.clone()), Ok(Json(claim.clone())))
                .await
                .status(),
            StatusCode::OK
        );
        let mut conflict = claim.clone();
        let TargetPackageEnvelopeV2::TargetDecision(package) = &mut conflict.package else {
            panic!("V2 fixture");
        };
        package.targets[0].target_weight = "0.5".parse().unwrap();
        assert_eq!(
            apply(State(state.clone()), Ok(Json(conflict)))
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            stop_session(State(state.clone())).await.status(),
            StatusCode::ACCEPTED
        );
        assert_eq!(snapshot(&state).state, PaperState::Running);
        assert!(snapshot(&state).stop_requested);
        state
            .status
            .send_modify(|status| status.state = PaperState::Stopped);
        assert_eq!(
            apply(State(state.clone()), Ok(Json(claim))).await.status(),
            StatusCode::OK
        );
        assert_eq!(snapshot(&state).state, PaperState::Stopped);
        assert!(requests.try_recv().is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_before_first_claim_prevents_polymarket_execution() {
        let (state, mut requests) = state_with_profile(PaperProfile::Polymarket);
        assert_eq!(
            stop_session(State(state.clone())).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            apply(State(state.clone()), Ok(Json(polymarket_claim())))
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(snapshot(&state).state, PaperState::Stopped);
        assert!(state.claim.lock().await.is_none());
        assert!(requests.try_recv().is_err());
        assert!(!capabilities(State(state)).await.0.accepting_targets);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn loopback_start_preserves_default_and_initializes_explicit_profile() {
        let bind = "127.0.0.1:0".parse().unwrap();
        let root = private_state_directory();
        let default = start_control(
            bind,
            b"test-only".to_vec(),
            "service-test/1".into(),
            root.path(),
        )
        .await
        .unwrap();
        assert!(default.local_addr.ip().is_loopback());
        assert_eq!(
            default.status.borrow().native_account_model,
            "MARGIN_LEVERAGE_ONE"
        );
        assert_eq!(
            default.status.borrow().market_data_source,
            "BINANCE_SPOT_PUBLIC_JSON"
        );
        default.shutdown.send_replace(true);
        default.task.await.unwrap().unwrap();

        let polymarket = start_control_with_profile(
            bind,
            b"test-only".to_vec(),
            "service-test/1".into(),
            PaperProfile::Polymarket,
            root.path(),
        )
        .await
        .unwrap();
        assert!(polymarket.local_addr.ip().is_loopback());
        assert_eq!(polymarket.status.borrow().state, PaperState::Idle);
        assert_polymarket_status(&polymarket.status.borrow());
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let origin = format!("http://{}", polymarket.local_addr);
        let rejected = client
            .get(format!("{origin}/downstream/v1/capabilities"))
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);
        let caps: serde_json::Value = client
            .get(format!("{origin}/downstream/v1/capabilities"))
            .bearer_auth("test-only")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(caps["accepted_package_versions"], json!(["2"]));
        assert_eq!(caps["environments"], json!(["PAPER"]));
        assert_eq!(caps["accepting_targets"], true);
        let stopped: serde_json::Value = client
            .post(format!("{origin}/downstream/v1/stop"))
            .bearer_auth("test-only")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(stopped["state"], "stopped");
        assert_eq!(stopped["stop_requested"], true);
        assert_eq!(stopped["execution_environment"], "PAPER_SIMULATION");
        polymarket.shutdown.send_replace(true);
        polymarket.task.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_claim_replays_without_restarting_and_stop_is_only_intent() {
        let (state, mut requests) = state();
        let claim = claim();
        let native = async {
            let request = requests.recv().await.unwrap();
            assert_eq!(request.claim.handoff.id, claim.handoff.id);
            request.reply.send(Ok(())).unwrap();
        };
        let (response, ()) =
            tokio::join!(apply(State(state.clone()), Ok(Json(claim.clone()))), native);
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(snapshot(&state).state, PaperState::Starting);
        assert_eq!(
            snapshot(&state).release_id,
            Some(domain::delivery::package_delivery(&claim.package).release_id)
        );
        assert!(!capabilities(State(state.clone())).await.0.accepting_targets);
        state
            .status
            .send_modify(|status| status.state = PaperState::Running);
        assert_eq!(
            apply(State(state.clone()), Ok(Json(claim.clone())))
                .await
                .status(),
            StatusCode::OK
        );
        let mut changed = claim.clone();
        let TargetPackageEnvelopeV2::Forecast(package) = &mut changed.package else {
            panic!("Forecast V2 fixture");
        };
        package.targets[0].target_weight = "0.5".parse().unwrap();
        assert_eq!(
            apply(State(state.clone()), Ok(Json(changed)))
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            stop_session(State(state.clone())).await.status(),
            StatusCode::ACCEPTED
        );
        assert_eq!(snapshot(&state).state, PaperState::Running);
        assert!(snapshot(&state).stop_requested);
        state
            .status
            .send_modify(|status| status.state = PaperState::Stopped);
        assert_eq!(
            apply(State(state.clone()), Ok(Json(claim))).await.status(),
            StatusCode::OK
        );
        assert!(requests.try_recv().is_err());
        assert_eq!(snapshot(&state).state, PaperState::Stopped);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_construction_keeps_original_identity_and_blocks_new_claims() {
        let (state, mut requests) = state();
        let claim = claim();
        let native = async {
            requests
                .recv()
                .await
                .unwrap()
                .reply
                .send(Err(PaperApplyError::InvalidClaim))
                .unwrap();
        };
        let (response, ()) =
            tokio::join!(apply(State(state.clone()), Ok(Json(claim.clone()))), native);
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(snapshot(&state).state, PaperState::Failed);
        assert_eq!(snapshot(&state).handoff_id, Some(claim.handoff.id));
        assert_eq!(
            apply(State(state.clone()), Ok(Json(claim.clone())))
                .await
                .status(),
            StatusCode::OK
        );
        let mut replacement = claim;
        replacement.handoff.id = Id::new();
        assert_eq!(
            apply(State(state.clone()), Ok(Json(replacement)))
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert!(requests.try_recv().is_err());
    }

    async fn response_bytes(response: Response) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn restarted_service_does_not_queue_incomplete_claim_with_new_output_directory() {
        let root = private_state_directory();
        let output_one = root.path().join("output-one");
        let output_two = root.path().join("output-two");
        std::fs::create_dir(&output_one).unwrap();
        std::fs::create_dir(&output_two).unwrap();
        let journal = root.path().join("stable-state");
        let original = polymarket_claim();
        let (first, mut requests) = state_with_store(PaperProfile::Polymarket, &journal);
        let accepted = async {
            let request = requests.recv().await.unwrap();
            std::fs::write(
                output_one.join("accepted-claim.json"),
                serde_json::to_vec(&request.claim).unwrap(),
            )
            .unwrap();
            request.reply.send(Ok(())).unwrap();
        };
        let (response, ()) = tokio::join!(
            apply(State(first.clone()), Ok(Json(original.clone()))),
            accepted
        );
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        drop(first);
        drop(requests);
        // Evidence output changes, while the stable runtime journal does not.
        let (second, mut requests) = state_with_store(PaperProfile::Polymarket, &journal);
        let response = apply(State(second.clone()), Ok(Json(original))).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let failure: serde_json::Value =
            serde_json::from_slice(&response_bytes(response).await).unwrap();
        assert_eq!(failure["code"], "paper_claim_recovery_required");
        assert!(requests.try_recv().is_err());
        assert!(second.claim.lock().await.is_none());
        assert_eq!(std::fs::read_dir(output_two).unwrap().count(), 0);
        assert!(capabilities(State(second)).await.0.accepting_targets);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn completed_replay_returns_original_status_and_does_not_consume_new_session() {
        let root = private_state_directory();
        let original = claim();
        let (first, mut requests) = state_with_store(PaperProfile::Binance, root.path());
        let accepted = async {
            requests.recv().await.unwrap().reply.send(Ok(())).unwrap();
        };
        let (response, ()) = tokio::join!(
            apply(State(first.clone()), Ok(Json(original.clone()))),
            accepted
        );
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        first.status.send_modify(|s| {
            s.state = PaperState::Stopped;
            s.native_session_id = Some("original-real-owner-session".into());
            s.target_points_consumed = 4;
        });
        let bytes = serde_json::to_vec(&first.status.borrow().clone()).unwrap();
        first
            .claim
            .lock()
            .await
            .as_ref()
            .unwrap()
            .lease
            .as_ref()
            .unwrap()
            .complete(&bytes)
            .unwrap();
        drop(first);
        drop(requests);
        let (second, mut requests) = state_with_store(PaperProfile::Binance, root.path());
        for _ in 0..2 {
            let response = apply(State(second.clone()), Ok(Json(original.clone()))).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["x-paper-claim-replayed"], "true");
            assert_eq!(response_bytes(response).await, bytes);
            assert!(requests.try_recv().is_err());
            assert!(second.claim.lock().await.is_none());
            assert_eq!(second.status.borrow().state, PaperState::Idle);
        }
        let other = claim();
        let accepted = async {
            requests.recv().await.unwrap().reply.send(Ok(())).unwrap();
        };
        let (response, ()) = tokio::join!(apply(State(second.clone()), Ok(Json(other))), accepted);
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_services_do_not_queue_same_claim_twice() {
        let root = private_state_directory();
        let original = claim();
        let (first, mut first_requests) = state_with_store(PaperProfile::Binance, root.path());
        let (second, mut second_requests) = state_with_store(PaperProfile::Binance, root.path());
        let owner = async {
            first_requests
                .recv()
                .await
                .unwrap()
                .reply
                .send(Ok(()))
                .unwrap();
        };
        let (response, ()) = tokio::join!(
            apply(State(first.clone()), Ok(Json(original.clone()))),
            owner
        );
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let rejected = apply(State(second), Ok(Json(original))).await;
        assert_eq!(rejected.status(), StatusCode::CONFLICT);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&response_bytes(rejected).await).unwrap()
                ["code"],
            "paper_claim_in_progress"
        );
        assert!(second_requests.try_recv().is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn actual_control_owner_records_only_terminal_observation_and_releases_lock() {
        let root = private_state_directory();
        let mut control = start_control(
            "127.0.0.1:0".parse().unwrap(),
            b"test-only".to_vec(),
            "service-test/1".into(),
            root.path(),
        )
        .await
        .unwrap();
        let original = claim();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let pending = client
            .post(format!(
                "http://{}/downstream/v1/targets",
                control.local_addr
            ))
            .bearer_auth("test-only")
            .json(&original)
            .send();
        let owner = async {
            control
                .requests
                .recv()
                .await
                .unwrap()
                .reply
                .send(Ok(()))
                .unwrap();
        };
        let (response, ()) = tokio::join!(pending, owner);
        assert_eq!(response.unwrap().status(), StatusCode::ACCEPTED);
        assert!(control.retain_terminal_observation().await.is_err());
        control.status.send_modify(|s| {
            s.state = PaperState::Failed;
            s.reason_code = Some("OBSERVED_OWNER_FAILURE".into());
        });
        control.retain_terminal_observation().await.unwrap();
        let bytes = serde_json::to_vec(&control.status.borrow().clone()).unwrap();
        let response = client
            .post(format!(
                "http://{}/downstream/v1/targets",
                control.local_addr
            ))
            .bearer_auth("test-only")
            .json(&original)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.bytes().await.unwrap().as_ref(), bytes.as_slice());
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        assert!(matches!(
            store.admit(&original, false),
            Ok(Some(Admission::Replay(_)))
        ));
        control.shutdown.send_replace(true);
        control.task.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn matching_identity_cannot_replay_incomplete_or_wrongly_typed_status() {
        for incomplete in [true, false] {
            let root = private_state_directory();
            let original = claim();
            let (first, mut requests) = state_with_store(PaperProfile::Binance, root.path());
            let accepted = async {
                requests.recv().await.unwrap().reply.send(Ok(())).unwrap();
            };
            let (response, ()) = tokio::join!(
                apply(State(first.clone()), Ok(Json(original.clone()))),
                accepted
            );
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            first.status.send_modify(|s| s.state = PaperState::Stopped);
            let valid = serde_json::to_vec(&first.status.borrow().clone()).unwrap();
            first
                .claim
                .lock()
                .await
                .as_ref()
                .unwrap()
                .lease
                .as_ref()
                .unwrap()
                .complete(&valid)
                .unwrap();
            drop(first);
            drop(requests);
            let mut corrupt: serde_json::Value = serde_json::from_slice(&valid).unwrap();
            if incomplete {
                corrupt.as_object_mut().unwrap().retain(|key, _| {
                    ["state", "handoff_id", "release_id", "external_claim_id"]
                        .contains(&key.as_str())
                });
            } else {
                corrupt["target_points_consumed"] = json!("four");
            }
            let path = root
                .path()
                .join("binance")
                .join(original.handoff.project_id.to_string())
                .join(original.handoff.downstream_id.to_string())
                .join(original.handoff.id.to_string())
                .join("terminal-status.json");
            std::fs::write(path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
            let (second, mut requests) = state_with_store(PaperProfile::Binance, root.path());
            let response = apply(State(second), Ok(Json(original))).await;
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&response_bytes(response).await)
                    .unwrap()["code"],
                "paper_claim_recovery_required"
            );
            assert!(requests.try_recv().is_err());
        }
    }
}
