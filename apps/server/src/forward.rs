//! Target-only downstream observations. The machine identity, not the JSON, owns the source.
use crate::{
    access::Authority,
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    Json,
};
use contracts::{control::CommandResult, forward::*};
use store::StoreError;

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
                        .map_err(|_| StoreError::Integrity)
                }
            })
            .await;
        if let Some(artifact) = allocated.filter(|_| result.is_err()) {
            if store
                .discard_unpublished_forward_weights(
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
