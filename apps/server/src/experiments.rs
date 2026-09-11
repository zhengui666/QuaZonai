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
    experiments::{ExperimentProposalV1, ExperimentView},
    research::ResearchListQuery,
    Id,
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
