//! Trusted HTTP entrypoint. Native session transport carries an opaque login
//! reference; PostgreSQL independently owns initialization, expiry and revocation.
#![forbid(unsafe_code)]
pub mod auth;
pub mod error;

use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
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
}
impl AppState {
    pub fn new(store: Store, vault: SecretVault, policy: WebPolicy) -> Self {
        Self {
            store,
            vault: Arc::new(vault),
            policy,
            crypto_slots: Arc::new(Semaphore::new(2)),
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
    let rejection = if host.is_none_or(|host| !state.policy.valid_host(host)) {
        Some(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_HOST",
            "请求的主机与配置的公共入口不一致。",
        ))
    } else if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && request
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
        != Some(state.policy.origin())
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
#[openapi(paths(auth::bootstrap_status,auth::bootstrap_start,auth::bootstrap_confirm,auth::login,auth::logout,auth::session_status,auth::verify,auth::devices,auth::revoke_device),components(schemas(error::Problem)),tags((name="Authentication",description="Native TOTP and revocable browser sessions")))]
struct HttpContracts;
pub fn openapi_json() -> Result<String, serde_json::Error> {
    let mut document = HttpContracts::openapi();
    document.info.title = "QuaZonai HTTP API".into();
    let mut value = serde_json::to_value(document)?;
    value.sort_all_objects();
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}
