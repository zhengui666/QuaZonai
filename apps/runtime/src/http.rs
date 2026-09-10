//! Trusted-service Runtime HTTP protocol. No browser session, arbitrary command or host path API.
use crate::{materialize, supervisor::RuntimeService, Failure, Result};
use axum::{
    body::{Body, Bytes},
    extract::{
        rejection::{BytesRejection, JsonRejection, PathRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use contracts::{catalogs::*, runtime::RuntimeCapabilitiesV1, runtime_jobs::*, Id, SchemaV1};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProblem {
    pub schema_version: SchemaV1,
    pub code: String,
    pub status: u16,
    pub retryable: bool,
    pub field: Option<String>,
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let (status, code, retryable, field) = match self {
            Self::Invalid(field) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "RUNTIME_INPUT_INVALID",
                false,
                Some(field),
            ),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "RUNTIME_IDEMPOTENCY_CONFLICT",
                false,
                None,
            ),
            Self::Missing => (StatusCode::NOT_FOUND, "RUNTIME_NOT_FOUND", false, None),
            Self::NotReady => (StatusCode::CONFLICT, "RUNTIME_RESULT_NOT_READY", true, None),
            Self::StaleOwner => (StatusCode::CONFLICT, "RUNTIME_OWNER_STALE", false, None),
            Self::Capacity => (
                StatusCode::INSUFFICIENT_STORAGE,
                "RUNTIME_CAPACITY_EXHAUSTED",
                false,
                None,
            ),
            Self::Busy => (StatusCode::SERVICE_UNAVAILABLE, "RUNTIME_BUSY", true, None),
            Self::Database(_) | Self::Io(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "RUNTIME_STORAGE_UNAVAILABLE",
                true,
                None,
            ),
            Self::Integrity => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "RUNTIME_INTEGRITY_ERROR",
                false,
                None,
            ),
            Self::Engine => (
                StatusCode::SERVICE_UNAVAILABLE,
                "RUNTIME_ENGINE_UNAVAILABLE",
                true,
                None,
            ),
            Self::Authentication => (
                StatusCode::UNAUTHORIZED,
                "RUNTIME_AUTHENTICATION_REQUIRED",
                false,
                None,
            ),
            Self::BodyLimit => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "RUNTIME_BODY_LIMIT",
                false,
                None,
            ),
        };
        let mut response = (
            status,
            Json(RuntimeProblem {
                schema_version: SchemaV1,
                code: code.into(),
                status: status.as_u16(),
                retryable,
                field: field.map(str::to_owned),
            }),
        )
            .into_response();
        if retryable {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        }
        if status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

/// OpenAPI-only binary shape. The handlers retain the original object bytes.
#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct RuntimeBytes(pub Vec<u8>);

struct HttpState {
    service: Arc<RuntimeService>,
    credential: String,
    requests: Arc<Semaphore>,
}

fn native_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str> {
    if headers.get_all(name).iter().count() != 1 {
        return Err(Failure::Invalid("header"));
    }
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or(Failure::Invalid("header"))
}

async fn authenticate(
    State(state): State<Arc<HttpState>>,
    request: Request,
    next: Next,
) -> Response {
    let valid = !request.headers().contains_key(header::COOKIE)
        && request
            .headers()
            .get_all(header::AUTHORIZATION)
            .iter()
            .count()
            == 1
        && request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|value| value == state.credential);
    let mut response = if !valid {
        Failure::Authentication.into_response()
    } else if request.headers().contains_key(header::CONTENT_ENCODING) {
        Failure::Invalid("content_encoding").into_response()
    } else if let Ok(_slot) = state.requests.clone().try_acquire_owned() {
        match tokio::time::timeout(Duration::from_secs(15), next.run(request)).await {
            Ok(response) => response,
            Err(_) => Failure::Busy.into_response(),
        }
    } else {
        Failure::Busy.into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

pub fn router(service: Arc<RuntimeService>, credential: String) -> Result<Router> {
    domain::settings::secret_value(
        contracts::settings::IntegrationSecretPurpose::Runtime,
        &credential,
    )
    .map_err(|_| Failure::Invalid("runtime_credential"))?;
    let state = Arc::new(HttpState {
        service,
        credential,
        requests: Arc::new(Semaphore::new(4)),
    });
    Ok(Router::new()
        .route("/runtime/v1/capabilities", get(capabilities))
        .route(
            "/runtime/v1/catalogs/{registered_ref}/metadata",
            get(catalog),
        )
        .route("/runtime/v1/jobs", post(submit))
        .route("/runtime/v1/jobs/{external_job_id}", get(status))
        .route("/runtime/v1/jobs/{external_job_id}/cancel", post(cancel))
        .route("/runtime/v1/jobs/{external_job_id}/result", get(result))
        .route(
            "/runtime/v1/jobs/{external_job_id}/artifacts/{storage_ref}",
            get(artifact),
        )
        .route(
            "/runtime/v1/objects/{artifact_id}",
            put(object).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .fallback(|| async { Failure::Missing.into_response() })
        .method_not_allowed_fallback(|| async {
            (
                StatusCode::METHOD_NOT_ALLOWED,
                Json(RuntimeProblem {
                    schema_version: SchemaV1,
                    code: "RUNTIME_METHOD_NOT_ALLOWED".into(),
                    status: 405,
                    retryable: false,
                    field: None,
                }),
            )
        })
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state))
}

#[utoipa::path(get, path = "/runtime/v1/capabilities", responses((status=200, body=RuntimeCapabilitiesV1), (status=503, body=RuntimeProblem)))]
async fn capabilities(State(state): State<Arc<HttpState>>) -> Result<Json<RuntimeCapabilitiesV1>> {
    Ok(Json(
        state
            .service
            .engine
            .capabilities(&state.service.catalogs)
            .await?,
    ))
}

#[utoipa::path(get, path = "/runtime/v1/catalogs/{registered_ref}/metadata",
    params(("registered_ref" = String, Path), ("storage_version" = String, Query)),
    responses((status=200, body=RuntimeCatalogMetadataV1), (status=404, body=RuntimeProblem), (status=422, body=RuntimeProblem)))]
async fn catalog(
    State(state): State<Arc<HttpState>>,
    id: std::result::Result<Path<String>, PathRejection>,
    query: std::result::Result<Query<CatalogVersionQuery>, QueryRejection>,
) -> Result<Response> {
    let Path(id) = id.map_err(|_| Failure::Invalid("registered_ref"))?;
    let Query(query) = query.map_err(|_| Failure::Invalid("storage_version"))?;
    let catalog = materialize::registered(&state.service.catalogs, &id, &query.storage_version)
        .map_err(|_| Failure::Missing)?;
    response("application/json", catalog.raw_metadata.clone())
}

#[utoipa::path(post, path = "/runtime/v1/jobs", request_body=JobSpecV1,
    responses((status=200, body=RuntimeJobStatusV1), (status=202, body=RuntimeJobStatusV1), (status=409, body=RuntimeProblem), (status=422, body=RuntimeProblem), (status=503, body=RuntimeProblem)))]
async fn submit(
    State(state): State<Arc<HttpState>>,
    body: std::result::Result<Json<JobSpecV1>, JsonRejection>,
) -> Result<Response> {
    let Json(spec) = body.map_err(|_| Failure::Invalid("job_spec"))?;
    if let Some(replay) = state.service.journal.replay(&spec).await? {
        return Ok((StatusCode::OK, Json(replay)).into_response());
    }
    materialize::parameters(&state.service.journal, &spec, &state.service.catalogs).await?;
    let capabilities = state
        .service
        .engine
        .capabilities(&state.service.catalogs)
        .await?;
    let (result, replayed) = state.service.journal.submit(&spec, &capabilities).await?;
    state.service.notify();
    Ok((
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::ACCEPTED
        },
        Json(result),
    )
        .into_response())
}

#[utoipa::path(get, path = "/runtime/v1/jobs/{external_job_id}", params(("external_job_id" = String, Path)),
    responses((status=200, body=RuntimeJobStatusV1), (status=404, body=RuntimeProblem)))]
async fn status(
    State(state): State<Arc<HttpState>>,
    id: std::result::Result<Path<String>, PathRejection>,
) -> Result<Json<RuntimeJobStatusV1>> {
    let Path(id) = id.map_err(|_| Failure::Invalid("external_job_id"))?;
    Ok(Json(state.service.journal.get(&id).await?.status()?))
}

#[utoipa::path(post, path = "/runtime/v1/jobs/{external_job_id}/cancel", params(("external_job_id" = String, Path)), request_body=RuntimeCancelV1,
    responses((status=200, body=RuntimeJobStatusV1), (status=202, body=RuntimeJobStatusV1), (status=409, body=RuntimeProblem)))]
async fn cancel(
    State(state): State<Arc<HttpState>>,
    id: std::result::Result<Path<String>, PathRejection>,
    body: std::result::Result<Json<RuntimeCancelV1>, JsonRejection>,
) -> Result<Response> {
    let Path(id) = id.map_err(|_| Failure::Invalid("external_job_id"))?;
    let Json(request) = body.map_err(|_| Failure::Invalid("cancellation"))?;
    let status = state.service.journal.cancel(&id, &request).await?;
    state.service.notify();
    Ok((
        if status.state.is_terminal() {
            StatusCode::OK
        } else {
            StatusCode::ACCEPTED
        },
        Json(status),
    )
        .into_response())
}

#[utoipa::path(get, path = "/runtime/v1/jobs/{external_job_id}/result", params(("external_job_id" = String, Path)),
    responses((status=200, body=ResultManifestV1), (status=404, body=RuntimeProblem), (status=409, body=RuntimeProblem)))]
async fn result(
    State(state): State<Arc<HttpState>>,
    id: std::result::Result<Path<String>, PathRejection>,
) -> Result<Response> {
    let Path(id) = id.map_err(|_| Failure::Invalid("external_job_id"))?;
    response("application/json", state.service.journal.result(&id).await?)
}

#[utoipa::path(put, path = "/runtime/v1/objects/{artifact_id}", params(("artifact_id" = Id, Path), ("X-QZ-Storage-Version" = String, Header)),
    request_body(content=inline(RuntimeBytes), content_type="application/octet-stream"),
    responses((status=200, body=RuntimeObjectReceiptV1), (status=201, body=RuntimeObjectReceiptV1), (status=409, body=RuntimeProblem), (status=413, body=RuntimeProblem)))]
async fn object(
    State(state): State<Arc<HttpState>>,
    id: std::result::Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Response> {
    let Path(id) = id.map_err(|_| Failure::Invalid("artifact_id"))?;
    if native_header(&headers, "content-type")? != "application/octet-stream" {
        return Err(Failure::Invalid("content_type"));
    }
    let version = native_header(&headers, "x-qz-storage-version")?;
    let bytes = body.map_err(|_| Failure::BodyLimit)?;
    let (receipt, replayed) = state
        .service
        .journal
        .put_object(id, version, &bytes)
        .await?;
    Ok((
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(receipt),
    )
        .into_response())
}

#[utoipa::path(get, path = "/runtime/v1/jobs/{external_job_id}/artifacts/{storage_ref}",
    params(("external_job_id" = String, Path), ("storage_ref" = Id, Path)),
    responses((status=200, content(
        (inline(RuntimeBytes) = "application/wasm"),
        (contracts::execution::NativeJsonOutputV1 = "application/json")
    )), (status=404, body=RuntimeProblem), (status=409, body=RuntimeProblem)))]
async fn artifact(
    State(state): State<Arc<HttpState>>,
    path: std::result::Result<Path<(String, Id)>, PathRejection>,
) -> Result<Response> {
    let Path((id, storage)) = path.map_err(|_| Failure::Invalid("artifact_identity"))?;
    let (metadata, bytes) = state.service.journal.output(&id, storage).await?;
    let mut result = response(&metadata.media_type, bytes)?;
    result.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment"),
    );
    Ok(result)
}

fn response(media: &str, bytes: Vec<u8>) -> Result<Response> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media)
        .body(Body::from(bytes))
        .map_err(|_| Failure::Integrity)
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        capabilities,
        catalog,
        submit,
        status,
        cancel,
        result,
        object,
        artifact
    ),
    components(schemas(RuntimeProblem))
)]
pub struct RuntimeApi;
