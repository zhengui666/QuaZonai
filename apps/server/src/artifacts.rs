//! Authenticated research bytes and metadata, not a runtime or evidence gate.
use crate::{
    access::{idempotency_key, Authority},
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        FromRequestParts, Path, Query, State,
    },
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use contracts::{
    artifacts::*,
    control::{CommandResult, Page},
    research::ResearchListQuery,
    Id,
};
use integrations::artifacts::ArtifactStore;
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;

/// Acquired by the native extractor before any potentially large JSON body.
pub struct ArtifactCapacity(Arc<OwnedSemaphorePermit>);
impl FromRequestParts<AppState> for ArtifactCapacity {
    type Rejection = Response;
    async fn from_request_parts(_: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        state
            .artifact_slots
            .clone()
            .try_acquire_owned()
            .map(|p| Self(Arc::new(p)))
            .map_err(|_| {
                let mut response = ApiError::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "ARTIFACT_BUSY",
                    "产物服务繁忙，请稍后重试。",
                )
                .into_response();
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
                response
            })
    }
}
fn native(state: &AppState) -> Result<Arc<ArtifactStore>, ApiError> {
    state.artifact_store.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ARTIFACT_STORAGE_UNAVAILABLE",
            "本地原生产物存储尚未就绪。",
        )
    })
}
fn path(id: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    id.map(|Path(id)| id).map_err(|_| ApiError::validation())
}

#[utoipa::path(post,path="/api/v2/artifacts",tag="Artifacts",request_body=ArtifactCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ArtifactView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn create(
    State(state): State<AppState>,
    Authority(actor): Authority,
    capacity: ArtifactCapacity,
    headers: HeaderMap,
    body: Result<Json<ArtifactCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ArtifactView>>), ApiError> {
    let request = json(body)?;
    let objects = native(&state)?;
    let upload = state
        .store
        .prepare_artifact_upload(&actor, idempotency_key(&headers)?, &request)
        .await?;
    let id = upload.id();
    let replay = upload.replay()?;
    let bytes = request.content.into_bytes();
    // Once native publication starts, losing the HTTP waiter must not roll back
    // its quota/receipt transaction while non-abortable file I/O continues.
    // The already-bounded owned task holds the transaction and capacity through
    // commit; it is not a queue, a retry loop or a replacement for PGMQ.
    let result = tokio::spawn(async move {
        let _capacity = capacity;
        let permit = _capacity.0.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if let Some(previous) = replay {
                let original = objects
                    .read(id, previous.resource.byte_count)
                    .map_err(|_| ApiError::internal())?;
                if original != bytes {
                    return Err(store::StoreError::IdempotencyConflict.into());
                }
                Ok(())
            } else {
                objects.put(id, &bytes).map_err(|_| ApiError::internal())
            }
        })
        .await
        .map_err(|_| ApiError::internal())??;
        // COMMIT errors can have unknown durability. Never delete an object that
        // may already be referenced; process death still requires reconciliation.
        upload.publish().await.map_err(ApiError::from)
    })
    .await
    .map_err(|_| ApiError::internal())??;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/artifacts",tag="Artifacts",params(("project_id"=Id,Query),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ArtifactView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    query: Result<Query<ResearchListQuery>, QueryRejection>,
) -> Result<Json<Page<ArtifactView>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.artifacts(&actor, &query).await?))
}

#[utoipa::path(get,path="/api/v2/artifacts/{id}",tag="Artifacts",params(("id"=Id,Path)),responses((status=200,body=ArtifactView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<ArtifactView>, ApiError> {
    Ok(Json(state.store.artifact(&actor, path(id)?).await?))
}

/// OpenAPI-only binary shape; the actual handler streams the original bytes.
#[derive(utoipa::ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct ArtifactBytes(pub Vec<u8>);

#[utoipa::path(get,path="/api/v2/artifacts/{id}/content",tag="Artifacts",params(("id"=Id,Path)),responses((status=200,body=inline(ArtifactBytes),content_type="application/octet-stream",description="Attachment with original native bytes; never rendered inline."),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn content(
    State(state): State<AppState>,
    Authority(actor): Authority,
    capacity: ArtifactCapacity,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Response, ApiError> {
    let id = path(id)?;
    let locator = state
        .store
        .artifact_content(&actor, id)
        .await
        .map_err(|error| match error {
            store::StoreError::Domain(domain::DomainError::CapabilityUnavailable(
                "artifact_content_backend",
            )) => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "ARTIFACT_BACKEND_UNAVAILABLE",
                "此产物的原生内容后端暂不可读取。",
            ),
            error => error.into(),
        })?;
    let objects = native(&state)?;
    let permit = capacity.0.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        objects
            .read(locator.local_object_id, locator.metadata.byte_count)
            .map_err(|_| ApiError::internal())
    })
    .await
    .map_err(|_| ApiError::internal())??;
    // Keep the large-buffer permit in the native stream until EOF or disconnect.
    // Chunking prevents a slow consumer from moving every bounded buffer into an
    // unbounded set of already-returned response bodies.
    let stream = futures_util::stream::unfold(
        (axum::body::Bytes::from(bytes), capacity.0),
        |(mut bytes, permit)| async move {
            if bytes.is_empty() {
                None
            } else {
                let chunk = bytes.split_to(bytes.len().min(64 * 1024));
                Some((Ok::<_, std::convert::Infallible>(chunk), (bytes, permit)))
            }
        },
    );
    let mut response = axum::body::Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{id}.bin\""))
            .map_err(|_| ApiError::internal())?,
    );
    Ok(response)
}
