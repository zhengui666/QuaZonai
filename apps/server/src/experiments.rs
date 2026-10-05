//! Authenticated experiment proposals and disclosure-safe research metadata.
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
    control::{CommandResult, Page},
    experiments::{
        ExperimentEvaluateIntent, ExperimentEvaluateV1, ExperimentProposalV1, ExperimentView,
    },
    research::ResearchListQuery,
    runs::RunSnapshotV1,
    science::NativeExperimentEvaluationResultV1,
    Id, SchemaV1,
};

#[utoipa::path(post,path="/api/v2/experiments",operation_id="propose_experiment",tag="Experiments",request_body=ExperimentProposalV1,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ExperimentView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn propose(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<ExperimentProposalV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ExperimentView>>), ApiError> {
    let request = json(body)?;
    let result = state
        .store
        .propose_experiment(&actor, idempotency_key(&headers)?, &request)
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/experiments",operation_id="list_experiments",tag="Experiments",params(("project_id"=Id,Query),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ExperimentView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    query: Result<Query<ResearchListQuery>, QueryRejection>,
) -> Result<Json<Page<ExperimentView>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.experiments(&actor, &query).await?))
}

#[utoipa::path(get,path="/api/v2/experiments/{id}",operation_id="get_experiment",tag="Experiments",params(("id"=Id,Path)),responses((status=200,body=ExperimentView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<ExperimentView>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.experiment(&actor, id).await?))
}

#[utoipa::path(post,path="/api/v2/experiments/{id}/evaluate",operation_id="evaluate_experiment",tag="Experiments",request_body=ExperimentEvaluateV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<RunSnapshotV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn evaluate(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<ExperimentEvaluateV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<RunSnapshotV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let intent = ExperimentEvaluateIntent {
        schema_version: SchemaV1,
        experiment_id: id,
        request: json(body)?,
    };
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(store::StoreError::IntegrationUnavailable)?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        let reading = objects.clone();
        let publishing = objects.clone();
        let mut allocated = None;
        let result = store
            .evaluate_experiment(
                &actor,
                &key,
                &intent,
                move |id, size| {
                    let objects = reading.clone();
                    async move {
                        tokio::task::spawn_blocking(move || objects.read(id, size))
                            .await
                            .map_err(|_| store::StoreError::Integrity)?
                            .map_err(|_| store::StoreError::Integrity)
                    }
                },
                |object| {
                    allocated = Some(object.id);
                    async move {
                        tokio::task::spawn_blocking(move || {
                            publishing.put(object.id, &object.bytes)
                        })
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(crate::error::artifact_storage)
                    }
                },
            )
            .await;
        if let Some(id) = allocated.filter(|_| result.is_err()) {
            if store
                .discard_unpublished_operator_artifact(id, move |id| async move {
                    tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(|_| store::StoreError::Integrity)
                })
                .await
                .is_err()
            {
                tracing::warn!(artifact_id=%id,"external experiment parameter cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/experiments/{id}/evaluation",operation_id="get_experiment_evaluation",tag="Experiments",params(("id"=Id,Path)),responses((status=200,body=NativeExperimentEvaluationResultV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn evaluation(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<NativeExperimentEvaluationResultV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let objects = state
        .artifact_store
        .clone()
        .ok_or(store::StoreError::IntegrationUnavailable)?;
    Ok(Json(
        state
            .store
            .experiment_evaluation(&actor, id, move |id, size| {
                let objects = objects.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.read(id, size))
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(|_| store::StoreError::Integrity)
                }
            })
            .await?,
    ))
}

/// A bounded view of the same adopted report, with identical ResearchRead authority.
#[utoipa::path(get,path="/api/v2/experiments/{id}/summary",operation_id="get_experiment_summary",tag="Experiments",params(("id"=Id,Path)),responses((status=200,body=contracts::experiment_summary::ExperimentSummaryV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn summary(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<contracts::experiment_summary::ExperimentSummaryV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let objects = state
        .artifact_store
        .clone()
        .ok_or(store::StoreError::IntegrationUnavailable)?;
    Ok(Json(
        state
            .store
            .experiment_summary(&actor, id, move |id, size| {
                let objects = objects.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.read(id, size))
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(|_| store::StoreError::Integrity)
                }
            })
            .await?,
    ))
}

/// Adopt the accepted result as a reusable policy, preserving its research status.
#[utoipa::path(post,path="/api/v2/experiments/{id}/adopt-alpha",operation_id="adopt_experiment_alpha",tag="Experiments",request_body=contracts::strategy_portfolio::StrategyAlphaAdoptV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<contracts::strategy_portfolio::StrategyAlphaVersionV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn adopt_alpha(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<contracts::strategy_portfolio::StrategyAlphaAdoptV1>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        Json<CommandResult<contracts::strategy_portfolio::StrategyAlphaVersionV1>>,
    ),
    ApiError,
> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let intent = contracts::strategy_portfolio::StrategyAlphaAdoptIntentV1 {
        schema_version: SchemaV1,
        experiment_id: id,
        request: json(body)?,
    };
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(store::StoreError::IntegrationUnavailable)?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store
            .adopt_experiment_alpha(&actor, &key, &intent, move |id, size| {
                let objects = objects.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.read(id, size))
                        .await
                        .map_err(|_| store::StoreError::Integrity)?
                        .map_err(|_| store::StoreError::Integrity)
                }
            })
            .await
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}
