//! Trusted HTTP entrypoint. Native session transport carries an opaque login
//! reference; PostgreSQL independently owns initialization, expiry and revocation.
#![forbid(unsafe_code)]
mod access;
pub mod auth;
pub mod control;
pub mod error;
pub mod research;
pub mod runs;
pub mod secrets;

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
        }
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
        .route("/api/v2/runs", get(runs::list))
        .route("/api/v2/runs/{id}", get(runs::get))
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
        .route("/api/v2/input-sets/{id}", get(research::input_set))
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
#[openapi(paths(auth::bootstrap_status,auth::bootstrap_start,auth::bootstrap_confirm,auth::login,auth::logout,auth::session_status,auth::verify,auth::devices,auth::revoke_device,
control::projects,control::project,control::create_project,control::update_project,
control::principals,control::create_principal,control::update_principal,
control::credentials,control::issue_credential,control::revoke_credential,
control::machine_session,control::issue_grant,runs::list,runs::get,runs::cancel,runs::events,
research::input_sets,research::input_set,research::create_input_set,
research::evaluation_policies,research::evaluation_policy,research::create_evaluation_policy),components(schemas(error::Problem)),tags((name="Authentication",description="Native TOTP and revocable browser sessions")))]
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
                let cookie = SecurityRequirement::new("BrowserSession", std::iter::empty::<&str>());
                let bearer = SecurityRequirement::new("MachineBearer", std::iter::empty::<&str>());
                if !anonymous && !browser_auth {
                    operation.responses.responses.insert(
                        "429".into(),
                        utoipa::openapi::ResponseBuilder::new()
                            .description("Shared native machine attempt limit or bounded crypto capacity; respect Retry-After.")
                            .content("application/problem+json", utoipa::openapi::Content::new(
                                Some(utoipa::openapi::Ref::from_schema_name("Problem"))))
                            .header("Retry-After", utoipa::openapi::Header::new(
                                utoipa::openapi::ObjectBuilder::new().schema_type(utoipa::openapi::Type::String)))
                            .build().into(),
                    );
                }
                operation.security = Some(if anonymous {
                    vec![]
                } else if only_machine {
                    vec![bearer]
                } else if browser_auth || (!write && browser_read) {
                    vec![cookie]
                } else if write && path.ends_with("/cancel") && path.starts_with("/api/v2/runs/") {
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
