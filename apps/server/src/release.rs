//! Freeze original target-only packages; no approval or downstream authority.
use crate::{
    access::{idempotency_key, Authority},
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    Json,
};
use contracts::{
    control::CommandResult,
    delivery::{ReleaseCreateV1, ReleaseViewV1},
    Id,
};
use store::StoreError;

#[utoipa::path(post,path="/api/v2/releases",operation_id="create_release",tag="Release",request_body=ReleaseCreateV1,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ReleaseViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn create(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<ReleaseCreateV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ReleaseViewV1>>), ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        let reading = objects.clone();
        let publishing = objects.clone();
        let mut allocated = Vec::new();
        let result = store
            .create_release(
                &actor,
                &key,
                &request,
                move |id, size| {
                    let objects = reading.clone();
                    async move {
                        tokio::task::spawn_blocking(move || objects.read(id, size))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    }
                },
                |object| {
                    allocated.push(object.id);
                    let objects = publishing.clone();
                    async move {
                        tokio::task::spawn_blocking(move || objects.put(object.id, &object.bytes))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    }
                },
            )
            .await;
        for id in allocated.into_iter().filter(|_| result.is_err()) {
            let objects = objects.clone();
            if store
                .discard_unpublished_operator_artifact(id, move |id| async move {
                    tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                })
                .await
                .is_err()
            {
                tracing::warn!(artifact_id=%id, "Release package cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/releases/{id}",operation_id="get_release",tag="Release",params(("id"=Id,Path)),responses((status=200,body=ReleaseViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<ReleaseViewV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.release(&actor, id).await?))
}
