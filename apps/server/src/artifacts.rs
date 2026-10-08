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
        Path, Query, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
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
    // The owned task holds the original transaction through
    // commit; it is not a queue, a retry loop or a replacement for PGMQ.
    let result = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            if let Some(previous) = replay {
                let original = objects
                    .read(id, previous.resource.byte_count)
                    .map_err(|_| ApiError::internal())?;
                if original != bytes {
                    return Err(store::StoreError::IdempotencyConflict.into());
                }
                Ok(())
            } else {
                objects
                    .put(id, &bytes)
                    .map_err(crate::error::artifact_storage)
                    .map_err(ApiError::from)
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
    id: Result<Path<Id>, PathRejection>,
) -> Result<Response, ApiError> {
    let id = path(id)?;
    let locator = state
        .store
        .artifact_content(&actor, id)
        .await
        .map_err(content_backend_error)?;
    let objects = native(&state)?;
    native_content(
        objects,
        locator.local_object_id,
        locator.metadata.byte_count,
    )
    .await
}
fn content_backend_error(error: store::StoreError) -> ApiError {
    match error {
        store::StoreError::Domain(domain::DomainError::CapabilityUnavailable(
            "artifact_content_backend",
        )) => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ARTIFACT_BACKEND_UNAVAILABLE",
            "此产物的原生内容后端暂不可读取。",
        ),
        error => error.into(),
    }
}

/// A typed view of the same immutable, scoped upload. Never an evaluation gate.
#[utoipa::path(get,path="/api/v2/artifacts/{id}/agent-evaluation",tag="Artifacts",params(("id"=Id,Path)),responses((status=200,body=contracts::agent_evaluation::AgentEvaluationReportV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn agent_evaluation(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Response, ApiError> {
    let id = path(id)?;
    // This is exactly the content read authority, including scope and revocation.
    let locator = state
        .store
        .artifact_content(&actor, id)
        .await
        .map_err(content_backend_error)?;
    if locator.metadata.kind != "REPORT" {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "AGENT_EVALUATION_REPORT_REQUIRED",
            "此产物不是受支持的 Agent 评估报告。",
        ));
    }
    let objects = native(&state)?;
    let bytes = tokio::task::spawn_blocking(move || {
        let bytes = objects
            .read(locator.local_object_id, locator.metadata.byte_count)
            .map_err(|_| ApiError::internal())?;
        let report = domain::agent_evaluation::parse(&bytes).map_err(|_| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "AGENT_EVALUATION_REPORT_REQUIRED",
                "此产物不是有效的 Agent 评估报告。",
            )
        })?;
        // The typed endpoint follows native wire serialization; the immutable
        // original bytes remain available through /content for external hash checks.
        serde_json::to_vec(&report).map_err(|_| ApiError::internal())
    })
    .await
    .map_err(|_| ApiError::internal())??;
    let mut response = buffered_content(bytes, id)?;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().remove(header::CONTENT_DISPOSITION);
    Ok(response)
}

pub(crate) async fn native_content(
    objects: Arc<ArtifactStore>,
    id: Id,
    byte_count: contracts::DbCounter,
) -> Result<Response, ApiError> {
    let bytes = tokio::task::spawn_blocking(move || {
        objects
            .read(id, byte_count)
            .map_err(|_| ApiError::internal())
    })
    .await
    .map_err(|_| ApiError::internal())??;
    buffered_content(bytes, id)
}
fn buffered_content(
    bytes: Vec<u8>,
    id: Id,
) -> Result<Response, ApiError> {
    // Bytes slices share the original complete allocation, which can stay live
    // until its final slice is dropped at EOF or disconnect. Concurrent responses
    // are not given an application memory ceiling; the immutable object is unchanged.
    let stream = futures_util::stream::unfold(
        axum::body::Bytes::from(bytes),
        |mut bytes| async move {
            if bytes.is_empty() { None } else {
                let chunk = bytes.split_to(bytes.len().min(64 * 1024));
                Some((Ok::<_, std::convert::Infallible>(chunk), bytes))
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn artifact_buffers_remain_independent_through_partial_reads_and_disconnect() {
        let mut streams = Vec::new();
        for _ in 0..5 {
            let response = buffered_content(vec![b'x'; 128 * 1024], Id::new()).unwrap();
            let mut stream = response.into_body().into_data_stream();
            assert_eq!(stream.next().await.unwrap().unwrap().len(), 64 * 1024);
            streams.push(stream);
        }
        streams.pop();
        for mut stream in streams {
            assert_eq!(stream.next().await.unwrap().unwrap().len(), 64 * 1024);
            assert!(stream.next().await.is_none());
        }
    }

    #[tokio::test]
    async fn artifact_content_backend_error_has_the_shared_machine_readable_code() {
        let response = content_backend_error(store::StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("artifact_content_backend"),
        ))
        .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(problem["code"], "ARTIFACT_BACKEND_UNAVAILABLE");
    }
}
