//! Trusted HTTP entrypoint. Native session transport carries an opaque login
//! reference; PostgreSQL independently owns initialization, expiry and revocation.
#![forbid(unsafe_code)]
mod access;
pub mod artifacts;
pub mod auth;
mod automation;
pub mod brief;
pub mod client;
pub mod codex_native;
pub mod codex_profiles;
pub mod control;
pub mod cycles;
pub mod data;
pub mod downstream;
pub mod equity_curve;
pub mod error;
pub mod evidence;
pub mod execution_assumptions;
pub mod experiments;
pub mod forward;
#[cfg(test)]
mod header_tests;
pub mod mcp;
pub mod migrations;
pub mod portfolio;
pub mod release;
pub mod research;
pub mod runs;
pub mod runtime;
pub mod runtime_transport;
pub mod secrets;
pub mod settings;
pub mod worker;

use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Router,
};
use contracts::Id;
use error::ApiError;
use integrations::secrets::SecretVault;
use std::{net::SocketAddr, sync::Arc};
use store::Store;
use tokio::sync::Semaphore;
use tower_sessions::{
    cookie::{Key, SameSite},
    Expiry, SessionManagerLayer,
};
use tower_sessions_sqlx_store::PostgresStore;
use url::{Host, Url};
use utoipa::OpenApi;

#[derive(Clone)]
pub struct WebPolicy {
    origin: String,
    secure: bool,
}
impl WebPolicy {
    pub fn new(
        public_url: &str,
        bind: SocketAddr,
        development_http: bool,
    ) -> Result<Self, &'static str> {
        let url = Url::parse(public_url).map_err(|_| "PUBLIC_URL must be an absolute URL")?;
        if url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err("PUBLIC_URL must contain only the public origin");
        }
        let loopback_host = match url.host() {
            Some(Host::Ipv4(ip)) => ip.is_loopback(),
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            Some(Host::Domain(name)) => name == "localhost",
            None => false,
        };
        let secure = match url.scheme() {
            "https" => true,
            "http" if development_http && bind.ip().is_loopback() && loopback_host => false,
            _ => return Err("HTTPS is required except explicitly enabled loopback development"),
        };
        Ok(Self {
            origin: url.origin().ascii_serialization(),
            secure,
        })
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    fn valid_host(&self, host: &str) -> bool {
        let scheme = if self.secure { "https" } else { "http" };
        Url::parse(&format!("{scheme}://{host}")).is_ok_and(|url| {
            url.origin().ascii_serialization() == self.origin
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
        })
    }
}
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub vault: Arc<SecretVault>,
    policy: WebPolicy,
    pub crypto_slots: Arc<Semaphore>,
    pub machine_crypto_slots: Arc<Semaphore>,
    pub run_stream_slots: Arc<Semaphore>,
    pub artifact_store: Option<Arc<integrations::artifacts::ArtifactStore>>,
    pub artifact_slots: Arc<Semaphore>,
    pub historical_artifact_store: Option<Arc<integrations::artifacts::ArtifactStore>>,
    pub historical_exports: Arc<migrations::HistoricalExports>,
    pub historical_import_slots: Arc<Semaphore>,
    pub integration_slots: Arc<Semaphore>,
    pub downstream_targets: Arc<runtime_transport::RuntimeTargets>,
    pub runtime_targets: Arc<runtime_transport::RuntimeTargets>,
    pub codex_deployment: Arc<codex_profiles::CodexDeployment>,
}
impl AppState {
    pub fn new(store: Store, vault: SecretVault, policy: WebPolicy) -> Self {
        Self {
            store,
            vault: Arc::new(vault),
            policy,
            crypto_slots: Arc::new(Semaphore::new(2)),
            machine_crypto_slots: Arc::new(Semaphore::new(2)),
            run_stream_slots: Arc::new(Semaphore::new(32)),
            artifact_store: None,
            artifact_slots: Arc::new(Semaphore::new(4)),
            historical_artifact_store: None,
            historical_exports: Arc::new(migrations::HistoricalExports::default()),
            historical_import_slots: Arc::new(Semaphore::new(1)),
            integration_slots: Arc::new(Semaphore::new(4)),
            downstream_targets: Arc::new(runtime_transport::RuntimeTargets::default()),
            runtime_targets: Arc::new(runtime_transport::RuntimeTargets::default()),
            codex_deployment: Arc::new(codex_profiles::CodexDeployment::default()),
        }
    }
    pub fn with_historical_artifact_store(
        mut self,
        store: integrations::artifacts::ArtifactStore,
    ) -> Self {
        self.historical_artifact_store = Some(Arc::new(store));
        self
    }
    pub fn with_historical_exports(mut self, exports: migrations::HistoricalExports) -> Self {
        self.historical_exports = Arc::new(exports);
        self
    }
    pub fn with_codex_deployment(mut self, deployment: codex_profiles::CodexDeployment) -> Self {
        self.codex_deployment = Arc::new(deployment);
        self
    }
    pub fn with_artifact_store(mut self, store: integrations::artifacts::ArtifactStore) -> Self {
        self.artifact_store = Some(Arc::new(store));
        self
    }
    pub fn with_downstream_targets(mut self, targets: runtime_transport::RuntimeTargets) -> Self {
        self.downstream_targets = Arc::new(targets);
        self
    }
    pub fn with_runtime_targets(mut self, targets: runtime_transport::RuntimeTargets) -> Self {
        self.runtime_targets = Arc::new(targets);
        self
    }
}

pub fn router(state: AppState, cookie_key: Key) -> Router {
    let session_store = PostgresStore::new(state.store.native_pool());
    let sessions = SessionManagerLayer::new(session_store)
        .with_name(if state.policy.secure {
            "__Host-quazonai"
        } else {
            "quazonai-dev"
        })
        .with_http_only(true)
        .with_same_site(SameSite::Strict)
        .with_secure(state.policy.secure)
        .with_path("/")
        .with_expiry(Expiry::OnInactivity(time::Duration::minutes(10)))
        .with_private(cookie_key);
    Router::new()
        .route("/health/live", get(|| async { StatusCode::NO_CONTENT }))
        .route(
            "/api/v2/migrations/reports/{id}/records/{record}/fields",
            get(migrations::fields),
        )
        .route(
            "/api/v2/migrations/reports/{id}/records/{record}/field",
            get(migrations::field),
        )
        .route(
            "/api/v2/migrations/reports/{id}/artifacts/summary",
            get(migrations::artifact_summary),
        )
        .route(
            "/api/v2/migrations/reports/{id}/artifacts",
            get(migrations::artifact_results),
        )
        .route(
            "/api/v2/migrations/reports/{id}/artifacts/{record}/content",
            get(migrations::artifact_content),
        )
        .route(
            "/api/v2/migrations/reports/{id}/artifacts/{record}",
            get(migrations::artifact),
        )
        .route("/api/v2/migrations/import", post(migrations::import))
        .route("/api/v2/migrations/reports", get(migrations::reports))
        .route(
            "/api/v2/migrations/reports/{id}/source",
            get(migrations::source),
        )
        .route(
            "/api/v2/migrations/reports/{id}/mappings",
            get(migrations::mappings),
        )
        .route("/api/v2/migrations/reports/{id}", get(migrations::report))
        .route("/api/v2/bootstrap/status", get(auth::bootstrap_status))
        .route("/api/v2/bootstrap/start", post(auth::bootstrap_start))
        .route("/api/v2/bootstrap/confirm", post(auth::bootstrap_confirm))
        .route("/api/v2/auth/login", post(auth::login))
        .route("/api/v2/auth/logout", post(auth::logout))
        .route("/api/v2/auth/session", get(auth::session_status))
        .route("/api/v2/auth/verify", post(auth::verify))
        .route("/api/v2/auth/devices", get(auth::devices))
        .route("/api/v2/auth/devices/{id}", delete(auth::revoke_device))
        .route(
            "/api/v2/projects",
            get(control::projects).post(control::create_project),
        )
        .route(
            "/api/v2/projects/{id}",
            get(control::project).patch(control::update_project),
        )
        .route(
            "/api/v2/machine-principals",
            get(control::principals).post(control::create_principal),
        )
        .route(
            "/api/v2/machine-principals/{id}",
            patch(control::update_principal),
        )
        .route(
            "/api/v2/machine-principals/{id}/credentials",
            get(control::credentials).post(control::issue_credential),
        )
        .route(
            "/api/v2/machine-credentials/{id}/revoke",
            post(control::revoke_credential),
        )
        .route(
            "/api/v2/artifacts",
            get(artifacts::list)
                .post(artifacts::create)
                .layer(DefaultBodyLimit::max(
                    contracts::artifacts::MAX_UPLOAD_BODY_BYTES,
                )),
        )
        .route("/api/v2/artifacts/{id}", get(artifacts::get))
        .route("/api/v2/artifacts/{id}/content", get(artifacts::content))
        .route(
            "/api/v2/experiments",
            get(experiments::list)
                .post(experiments::propose)
                .layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route("/api/v2/experiments/{id}", get(experiments::get))
        .route("/api/v2/alphas", get(evidence::alphas))
        .route("/api/v2/alphas/{id}/versions", get(evidence::versions))
        .route(
            "/api/v2/alphas/{id}/versions/{version}",
            get(evidence::version),
        )
        .route(
            "/api/v2/alpha-versions/{id}/evaluations",
            get(evidence::evaluations).post(evidence::evaluate),
        )
        .route(
            "/api/v2/alpha-versions/{id}/calibration",
            get(evidence::calibration),
        )
        .route("/api/v2/evaluations/{id}", get(evidence::evaluation))
        .route(
            "/api/v2/alpha-versions/{id}/qualifications",
            get(evidence::qualifications),
        )
        .route("/api/v2/evaluations/{id}/metrics", get(evidence::metrics))
        .route(
            "/api/v2/evaluations/{id}/equity-curve",
            get(equity_curve::get),
        )
        .route(
            "/api/v2/settings/codex",
            get(codex_profiles::profiles)
                .post(codex_profiles::create)
                .patch(codex_profiles::update_selected),
        )
        .route(
            "/api/v2/settings/codex/{id}",
            get(codex_profiles::profile).patch(codex_profiles::update),
        )
        .route("/api/v2/codex/homes", get(codex_profiles::homes))
        .route("/api/v2/codex/probe", post(codex_profiles::probe))
        .route("/api/v2/codex/models", get(codex_profiles::models))
        .route("/api/v2/codex/account", get(codex_profiles::account))
        .route(
            "/api/v2/codex/login/start",
            post(codex_profiles::account::login_start),
        )
        .route(
            "/api/v2/codex/logout",
            post(codex_profiles::account::logout),
        )
        .route(
            "/api/v2/codex/login/cancel",
            post(codex_profiles::account::login_cancel),
        )
        .route(
            "/api/v2/codex/login/{id}",
            get(codex_profiles::account::login_operation),
        )
        .route(
            "/api/v2/codex/login",
            get(codex_profiles::account::latest_operation),
        )
        .route(
            "/api/v2/settings/credentials",
            post(settings::register_secret).layer(DefaultBodyLimit::max(512 * 1024)),
        )
        .route(
            "/api/v2/integrations/runtimes",
            get(settings::runtimes).post(settings::create_runtime),
        )
        .route(
            "/api/v2/integrations/runtimes/{id}",
            get(settings::runtime).patch(settings::update_runtime),
        )
        .route(
            "/api/v2/integrations/runtimes/{id}/probe",
            post(runtime::probe),
        )
        .route(
            "/api/v2/integrations/runtimes/{id}/readiness",
            get(runtime::readiness),
        )
        .route(
            "/api/v2/integrations/downstreams",
            get(settings::downstreams).post(settings::create_downstream),
        )
        .route(
            "/api/v2/integrations/downstreams/{id}",
            get(settings::downstream).patch(settings::update_downstream),
        )
        .route(
            "/api/v2/integrations/downstreams/{id}/probe",
            post(downstream::probe).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/v2/integrations/downstreams/{id}/readiness",
            get(downstream::readiness),
        )
        .route("/api/v2/data/validate", post(data::validate))
        .route("/api/v2/runs", get(runs::list))
        .route("/api/v2/runs/{id}", get(runs::get))
        .route("/api/v2/runs/{id}/rebalance", get(runs::rebalance))
        .route("/api/v2/runs/{id}/cancel", post(runs::cancel))
        .route("/api/v2/runs/{id}/events", get(runs::events))
        .route("/api/v2/auth/machine", get(control::machine_session))
        .route(
            "/api/v2/auth/operator-command-grants",
            post(control::issue_grant).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/v2/input-sets",
            get(research::input_sets)
                .post(research::create_input_set)
                .layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/v2/data/sources",
            get(data::sources).post(data::create_source),
        )
        .route(
            "/api/v2/data/sources/{id}",
            get(data::source).patch(data::update_source),
        )
        .route(
            "/api/v2/data/sources/{id}/grants",
            get(data::grants).post(data::create_grant),
        )
        .route("/api/v2/data/grants/{id}/revoke", post(data::revoke_grant))
        .route(
            "/api/v2/data/grants/{id}/revocations",
            get(data::revocations),
        )
        .route(
            "/api/v2/data/revisions",
            get(data::revisions).post(data::register),
        )
        .route("/api/v2/data/revisions/{id}", get(data::revision))
        .route("/api/v2/data/universes", get(data::universes))
        .route("/api/v2/data/universes/{id}", get(data::universe))
        .route("/api/v2/input-sets/{id}", get(research::input_set))
        .route(
            "/api/v2/portfolio-mandates",
            post(portfolio::create).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route("/api/v2/portfolio-mandates/{id}", get(portfolio::get))
        .route(
            "/api/v2/portfolio-candidates/{id}",
            get(portfolio::candidate),
        )
        .route(
            "/api/v2/portfolio-candidates/{id}/evaluations",
            get(evidence::candidate_evaluations),
        )
        .route(
            "/api/v2/projects/{id}/portfolio-candidates",
            get(portfolio::candidates),
        )
        .route(
            "/api/v2/portfolio-builds",
            post(portfolio::build).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/api/v2/candidate-simulations",
            post(portfolio::simulate).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/api/v2/releases",
            post(release::create).layer(DefaultBodyLimit::max(4096)),
        )
        .route("/api/v2/releases/{id}", get(release::get))
        .route("/api/v2/projects/{id}/releases", get(release::list))
        .route(
            "/api/v2/releases/{id}/approvals",
            get(release::approvals)
                .post(release::approve)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/v2/projects/{id}/automation-policies",
            get(automation::automation_policies)
                .post(automation::authorize_automation)
                .layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/v2/automation-policies/{id}",
            get(automation::automation_policy),
        )
        .route(
            "/api/v2/automation-policies/{id}/revoke",
            post(automation::revoke_automation).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/v2/automation-policies/{id}/revocations",
            get(automation::automation_revocations),
        )
        .route("/api/v2/approvals/{id}", get(release::approval))
        .route(
            "/api/v2/approvals/{id}/revoke",
            post(release::revoke_approval).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/v2/approvals/{id}/revocations",
            get(release::revocations),
        )
        .route(
            "/api/v2/handoffs",
            post(release::offer).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/api/v2/projects/{id}/handoffs", get(release::handoffs))
        .route(
            "/api/v2/projects/{id}/forward-weight-snapshots",
            get(forward::weight_snapshots),
        )
        .route("/api/v2/handoffs/{id}", get(release::handoff))
        .route(
            "/api/v2/handoffs/{id}/ack",
            post(release::ack).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/v2/handoffs/{id}/claim",
            post(release::claim).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/api/v2/releases/{id}/rejections",
            post(release::reject).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/api/v2/releases/{id}/decisions", get(release::decisions))
        .route(
            "/api/v2/release-decisions/{id}/reopen",
            post(release::reopen).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/v2/portfolio-studies",
            post(portfolio::study).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/api/v2/execution-assumptions",
            post(execution_assumptions::create).layer(DefaultBodyLimit::max(1024 * 1024)),
        )
        .route(
            "/api/v2/forward/messages",
            post(forward::message).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/api/v2/projects/{id}/forward", get(forward::list))
        .route(
            "/api/v2/projects/{id}/forward-observations",
            get(forward::observations),
        )
        .route("/api/v2/projects/{id}/wakes", get(forward::wakes))
        .route("/api/v2/handoffs/{id}/forward-window", get(forward::window))
        .route(
            "/api/v2/forward/weights",
            post(forward::weights).layer(DefaultBodyLimit::max(1024 * 1024)),
        )
        .route(
            "/api/v2/execution-assumptions/{id}",
            get(execution_assumptions::get),
        )
        .route(
            "/api/v2/projects/{id}/execution-assumptions",
            get(execution_assumptions::list),
        )
        .route(
            "/api/v2/projects/{id}/portfolio-mandates",
            get(portfolio::list),
        )
        .route("/api/v2/briefs/{id}/freeze", post(cycles::freeze))
        .route("/api/v2/briefs/{id}/execution-context", get(cycles::frozen))
        .route(
            "/api/v2/projects/{id}/cycles",
            get(cycles::list).post(cycles::start),
        )
        .route("/api/v2/cycles/{id}", get(cycles::get))
        .route("/api/v2/cycles/{id}/selection", get(cycles::selection))
        .route("/api/v2/cycles/{id}/selection/trials", get(cycles::trials))
        .route(
            "/api/v2/projects/{id}/briefs",
            get(brief::list)
                .post(brief::create)
                .layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/v2/briefs/{id}",
            get(brief::get)
                .patch(brief::update)
                .layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/v2/evaluation-policies",
            get(research::evaluation_policies)
                .post(research::create_evaluation_policy)
                .layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/v2/evaluation-policies/{id}",
            get(research::evaluation_policy),
        )
        .fallback(|| async {
            ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "接口不存在。")
        })
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(sessions)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            browser_boundary,
        ))
        .with_state(state)
}

async fn browser_boundary(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .or_else(|| request.uri().authority().map(|a| a.as_str()));
    let headers = request.headers();
    let mutating = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let path = request.uri().path();
    let browser_auth = (path.starts_with("/api/v2/auth/")
        && !matches!(
            path,
            "/api/v2/auth/machine" | "/api/v2/auth/operator-command-grants"
        ))
        || path.starts_with("/api/v2/bootstrap/");
    let has_bearer = headers.contains_key(header::AUTHORIZATION);
    let origin = access::one_header(headers, "origin");
    let url_credential = request.uri().query().is_some_and(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .any(|(k, _)| matches!(k.as_ref(), "token" | "access_token" | "bearer"))
    });
    let rejection = if host.is_none_or(|host| !state.policy.valid_host(host)) {
        Some(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_HOST",
            "请求的主机与配置的公共入口不一致。",
        ))
    } else if url_credential || headers.get_all(header::HOST).iter().count() > 1 {
        Some(ApiError::validation())
    } else if has_bearer && (browser_auth || headers.contains_key(header::COOKIE)) {
        Some(ApiError::authentication())
    } else if origin.is_err()
        || origin
            .as_ref()
            .is_ok_and(|o| o.is_some_and(|o| o != state.policy.origin()))
        || (mutating
            && (!has_bearer || browser_auth)
            && origin.as_ref().ok().copied().flatten() != Some(state.policy.origin()))
    {
        Some(ApiError::new(
            StatusCode::FORBIDDEN,
            "INVALID_ORIGIN",
            "跨来源或缺少来源标识的浏览器写入被拒绝。",
        ))
    } else {
        None
    };
    let mut response = if let Some(error) = rejection {
        error.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    if state.policy.secure {
        headers.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    if !headers.contains_key("x-request-id") {
        headers.insert(
            "x-request-id",
            HeaderValue::from_str(&Id::new().to_string()).expect("UUID header"),
        );
    }
    response
}

#[derive(OpenApi)]
#[openapi(paths(migrations::artifact,migrations::artifact_summary,migrations::artifact_results,migrations::artifact_content,migrations::fields,migrations::field,migrations::reports,migrations::source,migrations::mappings,migrations::import,migrations::report,auth::bootstrap_status,auth::bootstrap_start,auth::bootstrap_confirm,auth::login,auth::logout,auth::session_status,auth::verify,auth::devices,auth::revoke_device,
control::projects,control::project,control::create_project,control::update_project,
control::principals,control::create_principal,control::update_principal,
control::credentials,control::issue_credential,control::revoke_credential,
control::machine_session,control::issue_grant,runs::list,runs::get,runs::rebalance,runs::cancel,runs::events,
research::input_sets,research::input_set,research::create_input_set,
research::evaluation_policies,research::evaluation_policy,research::create_evaluation_policy,
brief::list,brief::get,brief::create,brief::update,
automation::authorize_automation,automation::revoke_automation,automation::automation_policy,automation::automation_policies,automation::automation_revocations,release::ack,release::revoke_approval,release::revocations,release::claim,release::offer,release::handoff,release::handoffs,release::create,release::get,release::list,release::approvals,release::approve,release::approval,release::reject,release::reopen,release::decisions,portfolio::list,portfolio::get,portfolio::create,portfolio::build,portfolio::simulate,portfolio::study,portfolio::candidates,portfolio::candidate,
execution_assumptions::list,execution_assumptions::get,execution_assumptions::create,
forward::weight_snapshots,forward::weights,forward::message,forward::list,forward::window,forward::observations,forward::wakes,
cycles::freeze,cycles::frozen,cycles::start,cycles::list,cycles::get,cycles::selection,cycles::trials,
experiments::propose,experiments::list,experiments::get,
evidence::alphas,evidence::versions,evidence::version,evidence::calibration,evidence::qualifications,evidence::evaluations,evidence::candidate_evaluations,evidence::evaluate,evidence::evaluation,evidence::metrics,equity_curve::get,
settings::register_secret,settings::runtimes,settings::runtime,settings::create_runtime,settings::update_runtime,
settings::downstreams,settings::downstream,settings::create_downstream,settings::update_downstream,
runtime::probe,runtime::readiness,downstream::probe,downstream::readiness,
codex_profiles::profiles,codex_profiles::profile,codex_profiles::homes,codex_profiles::create,
codex_profiles::update,codex_profiles::update_selected,codex_profiles::probe,codex_profiles::models,codex_profiles::account,
codex_profiles::account::login_start,codex_profiles::account::logout,codex_profiles::account::login_cancel,
codex_profiles::account::login_operation,codex_profiles::account::latest_operation,
data::sources,data::source,data::create_source,data::update_source,
data::grants,data::create_grant,data::revoke_grant,data::revocations,
data::revisions,data::revision,data::register,data::universes,data::universe,data::validate,
artifacts::list,artifacts::get,artifacts::create,artifacts::content),components(schemas(error::Problem)),tags((name="Authentication",description="Native TOTP and revocable browser sessions")))]
struct HttpContracts;
pub fn openapi_json() -> Result<String, serde_json::Error> {
    let mut document = HttpContracts::openapi();
    document.info.title = "QuaZonai HTTP API".into();
    describe_authority(&mut document);
    let mut value = serde_json::to_value(document)?;
    value.sort_all_objects();
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

/// Security metadata uses utoipa's native OpenAPI types, derived alongside real
/// routes. A machine grant is one human-approved operation, not an OAuth scope.
fn describe_authority(document: &mut utoipa::openapi::OpenApi) {
    use utoipa::openapi::security::{
        ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme,
    };
    let components = document.components.get_or_insert_with(Default::default);
    components.add_security_scheme("BrowserSession",SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description("__Host-quazonai","Native private cookie; browser writes require exact same-origin Origin. Explicit loopback development uses quazonai-dev."))));
    components.add_security_scheme("MachineBearer",SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("qz2.UUIDv7.opaque-capability").description(Some("Opaque native capability; only project/run/downstream-scoped server records confer authority. Never combine with browser Cookie." )).build()));
    components.add_security_scheme("OperatorCommandGrant",SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description("X-Operator-Grant","One-time TOTP-verified CLI grant bound to this credential, exact operation, target and full nonsecret request. No Agent/automation grant issuance."))));
    for (path, item) in &mut document.paths.paths {
        let anonymous = path.starts_with("/api/v2/bootstrap/") || path == "/api/v2/auth/login";
        let only_machine = matches!(
            path.as_str(),
            "/api/v2/auth/machine" | "/api/v2/auth/operator-command-grants"
        );
        let browser_auth = path.starts_with("/api/v2/auth/") && !only_machine;
        let browser_read = path.starts_with("/api/v2/machine-principals");
        for (write, operation) in [
            (false, &mut item.get),
            (false, &mut item.head),
            (true, &mut item.post),
            (true, &mut item.patch),
            (true, &mut item.delete),
            (true, &mut item.put),
        ] {
            if let Some(operation) = operation {
                // Match the actual HeaderValue transport and access validator,
                // without copying constraints into every route annotation.
                for parameter in operation.parameters.iter_mut().flatten() {
                    if parameter.name.eq_ignore_ascii_case("Idempotency-Key")
                        && parameter.parameter_in == utoipa::openapi::path::ParameterIn::Header
                    {
                        parameter.required = utoipa::openapi::Required::True;
                        parameter.schema = Some(
                            utoipa::openapi::schema::ObjectBuilder::new()
                                .schema_type(utoipa::openapi::schema::Type::String)
                                .min_length(Some(1))
                                .max_length(Some(200))
                                .pattern(Some(r"^[!-~]([ -~]*[!-~])?(?![\s\S])"))
                                .into(),
                        );
                        parameter.description = Some(
                            "One printable ASCII header value, 1–200 bytes; no leading/trailing space or controls. Internal spaces are allowed. Repeated headers are rejected.".into(),
                        );
                    }
                }
                let cookie = SecurityRequirement::new("BrowserSession", std::iter::empty::<&str>());
                let bearer = SecurityRequirement::new("MachineBearer", std::iter::empty::<&str>());
                if !anonymous && !browser_auth {
                    operation.responses.responses.insert(
                        "429".into(),
                        utoipa::openapi::ResponseBuilder::new()
                            .description("Authentication/capacity limit, or BUDGET_EXHAUSTED for frozen resource quotas. Only retryable limits may include Retry-After; budget exhaustion is nonretryable and does not include it.")
                            .content("application/problem+json", utoipa::openapi::Content::new(
                                Some(utoipa::openapi::Ref::from_schema_name("Problem"))))
                            .header("Retry-After", utoipa::openapi::Header::new(
                                utoipa::openapi::ObjectBuilder::new().schema_type(utoipa::openapi::Type::String)))
                            .build().into(),
                    );
                }
                // ApiError always emits Problem media, not ordinary JSON.
                // Keep each route's declared statuses and schema; change only
                // the native annotation's default JSON media for that DTO.
                for (status, response) in &mut operation.responses.responses {
                    if !status.parse::<u16>().is_ok_and(|code| code >= 400) {
                        continue;
                    }
                    if let utoipa::openapi::RefOr::T(response) = response {
                        if let Some(content) = response.content.shift_remove("application/json") {
                            response
                                .content
                                .insert("application/problem+json".into(), content);
                        }
                    }
                }
                operation.security = Some(if anonymous {
                    vec![]
                } else if only_machine {
                    vec![bearer]
                } else if browser_auth || (!write && browser_read) {
                    vec![cookie]
                } else if write
                    && (path == "/api/v2/artifacts"
                        || path == "/api/v2/experiments"
                        || (path.ends_with("/cancel") && path.starts_with("/api/v2/runs/")))
                {
                    vec![cookie, bearer]
                } else if write {
                    vec![
                        cookie,
                        bearer.add("OperatorCommandGrant", std::iter::empty::<&str>()),
                    ]
                } else {
                    vec![cookie, bearer]
                });
            }
        }
    }
}
