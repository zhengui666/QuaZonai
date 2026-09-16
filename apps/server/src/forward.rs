//! Target-only downstream observations. The machine identity, not the JSON, owns the source.
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
    http::{HeaderMap, StatusCode},
    Json,
};
use contracts::{
    control::{CommandResult, ListQuery, Page},
    forward::*,
    Id,
};
use store::StoreError;

#[utoipa::path(get,path="/api/v2/projects/{id}/forward-weight-snapshots",operation_id="list_downstream_weight_snapshots",tag="Forward",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<DownstreamWeightsViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn weight_snapshots(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<DownstreamWeightsViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .downstream_weight_snapshots(&actor, id, &query)
            .await?,
    ))
}

#[utoipa::path(post,path="/api/v2/forward/weights",operation_id="submit_downstream_weights",tag="Forward",request_body=DownstreamWeightsSubmitV1,responses((status=201,body=CommandResult<DownstreamWeightsViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn weights(
    State(state): State<AppState>,
    Authority(actor): Authority,
    body: Result<Json<DownstreamWeightsSubmitV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<DownstreamWeightsViewV1>>), ApiError> {
    let request = json(body)?;
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        let publishing = objects.clone();
        let mut allocated = None;
        let result = store
            .submit_downstream_weights(&actor, &request, |object| {
                allocated = Some(object.id);
                async move {
                    tokio::task::spawn_blocking(move || publishing.put(object.id, &object.bytes))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(crate::error::artifact_storage)
                }
            })
            .await;
        if let Some(artifact) = allocated.filter(|_| result.is_err()) {
            if store
                .discard_unpublished_forward_artifact(
                    request.project_id,
                    artifact,
                    |id| async move {
                        tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    },
                )
                .await
                .is_err()
            {
                tracing::warn!(artifact_id=%artifact,"Forward weight cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/forward",operation_id="list_forward_messages",tag="Forward",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ForwardMessageViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ForwardMessageViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.forward_messages(&actor, id, &query).await?,
    ))
}
#[utoipa::path(post,path="/api/v2/forward/messages",operation_id="submit_forward_message",tag="Forward",request_body=ForwardMessageSubmitV1,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ForwardMessageViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn message(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<ForwardMessageSubmitV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ForwardMessageViewV1>>), ApiError> {
    let request = json(body)?;
    if idempotency_key(&headers)? != request.external_message_id {
        return Err(ApiError::validation());
    }
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        let reading = objects.clone();
        let publishing = objects.clone();
        let mut allocated = None;
        let result = store
            .submit_forward_message(
                &actor,
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
                    allocated = Some(object.id);
                    async move {
                        tokio::task::spawn_blocking(move || {
                            publishing.put(object.id, &object.bytes)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(crate::error::artifact_storage)
                    }
                },
            )
            .await;
        if let Some(artifact) = allocated.filter(|_| result.is_err()) {
            if store
                .discard_unpublished_forward_artifact(
                    request.report.project_id,
                    artifact,
                    |id| async move {
                        tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    },
                )
                .await
                .is_err()
            {
                tracing::warn!(artifact_id=%artifact,"Forward report cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/handoffs/{id}/forward-window",operation_id="get_forward_window",tag="Forward",params(("id"=Id,Path),("stream_id"=String,Query)),responses((status=200,body=ForwardWindowViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn window(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ForwardWindowQueryV1>, QueryRejection>,
) -> Result<Json<ForwardWindowViewV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let result = state
        .store
        .forward_window(&actor, id, &query, move |id, size| {
            let objects = objects.clone();
            async move {
                tokio::task::spawn_blocking(move || objects.read(id, size))
                    .await
                    .map_err(|_| StoreError::Integrity)?
                    .map_err(|_| StoreError::Integrity)
            }
        })
        .await?;
    Ok(Json(result))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/forward-observations",operation_id="list_forward_observations",tag="Forward",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ForwardObservationViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn observations(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ForwardObservationViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.forward_observations(&actor, id, &query).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/wakes",operation_id="list_wake_events",tag="Forward",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<WakeViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn wakes(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<WakeViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.wake_events(&actor, id, &query).await?))
}
