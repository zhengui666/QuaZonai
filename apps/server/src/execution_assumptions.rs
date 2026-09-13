//! Original Operator transaction and object cleanup, not a simulation launch.
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
    execution_assumptions::*,
    Id,
};
use store::StoreError;

#[utoipa::path(post,path="/api/v2/execution-assumptions",operation_id="create_execution_assumptions",tag="Execution assumptions",params(("Idempotency-Key"=String,Header)),request_body=ExecutionAssumptionsCreateV1,responses((status=201,body=CommandResult<ExecutionAssumptionsViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn create(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<ExecutionAssumptionsCreateV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ExecutionAssumptionsViewV1>>), ApiError> {
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
        let mut allocated = None;
        let result = store
            .create_execution_assumptions(
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
                    allocated = Some(object.id);
                    async move {
                        tokio::task::spawn_blocking(move || {
                            publishing.put(object.id, &object.bytes)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                    }
                },
            )
            .await;
        if let Some(id) = allocated.filter(|_| result.is_err()) {
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
                tracing::warn!(artifact_id=%id,"Execution assumption cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/execution-assumptions/{id}",operation_id="get_execution_assumptions",tag="Execution assumptions",params(("id"=Id,Path)),responses((status=200,body=ExecutionAssumptionsViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<ExecutionAssumptionsViewV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.execution_assumption(&actor, id).await?))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/execution-assumptions",operation_id="list_execution_assumptions",tag="Execution assumptions",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ExecutionAssumptionsViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ExecutionAssumptionsViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .execution_assumptions(&actor, id, &query)
            .await?,
    ))
}
