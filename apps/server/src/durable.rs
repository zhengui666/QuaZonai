//! Owner-authenticated transport to the official durable host. No local task/session journal.
use crate::{access::Authority, error::ApiError, service_http, AppState};
use axum::{
    body::Bytes,
    extract::{rejection::PathRejection, Path, RawQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use contracts::{
    durable_research::{
        DurableModelViewV1, DurableResearchErrorV1, DurableResearchStartV1,
        DurableSubmissionReceiptV1,
    },
    http::Problem,
};
use reqwest::{Client, Method, Url};
use store::authority::Actor;
use url::Host;

pub struct ResearchService {
    origin: Url,
    client: Client,
}
impl Default for ResearchService {
    fn default() -> Self {
        Self::new("http://127.0.0.1:8092").expect("static research origin")
    }
}
impl ResearchService {
    /// Deployment configuration only; never accepts model/user-selected URLs or credentials.
    pub fn new(origin: &str) -> Result<Self, &'static str> {
        let origin = Url::parse(origin).map_err(|_| "QZ_RESEARCH_ORIGIN must be an HTTP origin")?;
        if !matches!(origin.scheme(), "http" | "https")
            || !matches!(origin.host(), Some(Host::Ipv4(ip)) if ip.is_loopback())
                && !matches!(origin.host(), Some(Host::Ipv6(ip)) if ip.is_loopback())
                && !matches!(origin.host(), Some(Host::Domain("localhost")))
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err("QZ_RESEARCH_ORIGIN must contain only a loopback HTTP origin");
        }
        let client = service_http::builder(HeaderMap::new())
            .no_proxy()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| "research HTTP transport unavailable")?;
        Ok(Self { origin, client })
    }

    async fn forward(
        &self,
        actor: &Actor,
        method: Method,
        path: &str,
        query: Option<&str>,
        bytes: Bytes,
    ) -> Result<Response, ApiError> {
        if !matches!(actor, Actor::Browser { .. } | Actor::OwnerDevice { .. }) {
            return Err(store::StoreError::Forbidden.into());
        }
        let mut url = self.origin.clone();
        url.set_path(path);
        url.set_query(query);
        let mut request = self.client.request(method, url);
        if !bytes.is_empty() {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(bytes);
        }
        // Owner cookies/tokens and operator grants never cross this boundary.
        let response = service_http::send(request)
            .await
            .map_err(|_| unavailable())?;
        let status = response.status();
        let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
        let bytes = service_http::body(response, None)
            .await
            .map_err(|_| unavailable())?;
        let mut response = (status, bytes).into_response();
        if let Some(content_type) = content_type {
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, content_type);
        }
        Ok(response)
    }
}
fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "RESEARCH_UNAVAILABLE",
        "Research service unavailable or result unknown; inspect the original request/task before retrying.",
    )
}
fn numeric_id(id: Result<Path<u64>, PathRejection>) -> Result<u64, ApiError> {
    id.map(|Path(id)| id).map_err(|_| ApiError::validation())
}

#[utoipa::path(get, path="/api/v2/research", operation_id="listDurableResearch", tag="Durable research", params(("cursor"=Option<String>,Query,description="JSON-encoded native pi-durable Cursor")), responses((status=200,body=Value,description="Native pi-durable JSON snapshot"),(status=400,body=DurableResearchErrorV1),(status=401,body=Problem),(status=403,body=Problem),(status=500,body=DurableResearchErrorV1),(status=503,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    state
        .research_service
        .forward(
            &actor,
            Method::GET,
            "/research",
            query.as_deref(),
            Bytes::new(),
        )
        .await
}

#[utoipa::path(post, path="/api/v2/research", operation_id="startDurableResearch", tag="Durable research", request_body=DurableResearchStartV1, responses((status=202,body=DurableSubmissionReceiptV1,description="Native submission admitted or replayed; not completion"),(status=400,body=DurableResearchErrorV1),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=DurableResearchErrorV1),(status=422,body=Problem),(status=500,body=DurableResearchErrorV1),(status=503,body=Problem)))]
pub async fn start(
    State(state): State<AppState>,
    Authority(actor): Authority,
    RawQuery(query): RawQuery,
    bytes: Bytes,
) -> Result<Response, ApiError> {
    // Parse without reserializing: retries keep the original requestId and exact bytes.
    let _: DurableResearchStartV1 =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::validation())?;
    state
        .research_service
        .forward(&actor, Method::POST, "/research", query.as_deref(), bytes)
        .await
}

#[utoipa::path(get, path="/api/v2/research/{id}", operation_id="getDurableResearch", tag="Durable research", params(("id"=u64,Path)), responses((status=200,body=Value,description="Native pi-durable JSON snapshot"),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=DurableResearchErrorV1),(status=422,body=Problem),(status=500,body=DurableResearchErrorV1),(status=503,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<u64>, PathRejection>,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    state
        .research_service
        .forward(
            &actor,
            Method::GET,
            &format!("/research/{}", numeric_id(id)?),
            query.as_deref(),
            Bytes::new(),
        )
        .await
}

#[utoipa::path(post, path="/api/v2/research/{id}/stop", operation_id="stopDurableResearch", tag="Durable research", params(("id"=u64,Path)), responses((status=202,body=Value,description="Native stop request accepted, not cancellation proof"),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=DurableResearchErrorV1),(status=422,body=Problem),(status=500,body=DurableResearchErrorV1),(status=503,body=Problem)))]
pub async fn stop(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<u64>, PathRejection>,
    RawQuery(query): RawQuery,
    bytes: Bytes,
) -> Result<Response, ApiError> {
    state
        .research_service
        .forward(
            &actor,
            Method::POST,
            &format!("/research/{}/stop", numeric_id(id)?),
            query.as_deref(),
            bytes,
        )
        .await
}

#[utoipa::path(get, path="/api/v2/research/models", operation_id="listDurableModels", tag="Durable research", responses((status=200,body=Vec<DurableModelViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=500,body=DurableResearchErrorV1),(status=503,body=Problem)))]
pub async fn models(
    State(state): State<AppState>,
    Authority(actor): Authority,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    state
        .research_service
        .forward(
            &actor,
            Method::GET,
            "/models",
            query.as_deref(),
            Bytes::new(),
        )
        .await
}

#[utoipa::path(get, path="/api/v2/research/tasks", operation_id="listDurableTasks", tag="Durable research", responses((status=200,body=Value),(status=401,body=Problem),(status=403,body=Problem),(status=500,body=DurableResearchErrorV1),(status=503,body=Problem)))]
pub async fn tasks(
    State(state): State<AppState>,
    Authority(actor): Authority,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    state
        .research_service
        .forward(
            &actor,
            Method::GET,
            "/tasks",
            query.as_deref(),
            Bytes::new(),
        )
        .await
}

#[utoipa::path(get, path="/api/v2/research/usage", operation_id="getDurableUsage", tag="Durable research", responses((status=200,body=Value),(status=401,body=Problem),(status=403,body=Problem),(status=500,body=DurableResearchErrorV1),(status=503,body=Problem)))]
pub async fn usage(
    State(state): State<AppState>,
    Authority(actor): Authority,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    state
        .research_service
        .forward(
            &actor,
            Method::GET,
            "/usage",
            query.as_deref(),
            Bytes::new(),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::ResearchService;

    #[test]
    fn private_research_origin_is_loopback_only() {
        for origin in [
            "http://127.0.0.1:8092",
            "https://localhost:8092",
            "http://[::1]:8092",
        ] {
            assert!(ResearchService::new(origin).is_ok(), "{origin}");
        }
        for origin in [
            "https://example.com",
            "http://192.168.1.2:8092",
            "http://0.0.0.0:8092",
            "http://[::]:8092",
            "http://localhost.example.com",
            "http://user@localhost",
            "http://127.0.0.1/path",
            "http://127.0.0.1?query=1",
            "http://127.0.0.1#fragment",
        ] {
            assert!(ResearchService::new(origin).is_err(), "{origin}");
        }
    }
}
